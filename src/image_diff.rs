use crate::diff;
use crate::git::{self, Compare, LfsPointer, Version};
use crate::logging::ReadLog;
use anyhow::{Context, Result, ensure};
use eframe::egui;
use image::{ImageDecoder, ImageError, ImageFormat, ImageReader, Limits};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};

const MAX_EDGE: u32 = 8192;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
const IMAGE_LIMIT: &str = "图片尺寸超过预览上限";

fn supported_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::Bmp
            | ImageFormat::Gif
            | ImageFormat::WebP
    )
}

fn supports(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(ImageFormat::from_extension)
        .is_some_and(supported_format)
}

pub(crate) fn supports_pair(pair: &Compare) -> bool {
    !pair.left_gitlink
        && !pair.right_gitlink
        && (supports(&pair.left_path) || supports(&pair.right_path))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageColumn {
    Left,
    Right,
}

pub(crate) struct ImageDiff {
    pub left: ImageSide,
    pub right: ImageSide,
}

impl ImageDiff {
    pub fn pending() -> Self {
        Self {
            left: ImageSide::Loading,
            right: ImageSide::Loading,
        }
    }

    pub fn apply(&mut self, column: ImageColumn, side: ImageSide) {
        match column {
            ImageColumn::Left => self.left = side,
            ImageColumn::Right => self.right = side,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.left, ImageSide::Loading) || matches!(self.right, ImageSide::Loading)
    }
}

pub(crate) fn load(
    root: &Path,
    pair: &Compare,
    log: &ReadLog,
    canceled: &AtomicBool,
    ready: impl Fn(ImageColumn, ImageSide) + Sync,
) {
    let object_dir = OnceLock::new();
    let load_side = |column| {
        let (version, path) = match column {
            ImageColumn::Left => (&pair.left, &pair.left_path),
            ImageColumn::Right => (&pair.right, &pair.right_path),
        };
        let snapshot = diff::read_snapshot(root, version, path, false, log);
        ready(
            column,
            ImageSide::read(root, version, path, snapshot, &object_dir, canceled),
        );
    };
    std::thread::scope(|scope| {
        scope.spawn(|| load_side(ImageColumn::Left));
        load_side(ImageColumn::Right);
    });
}

pub(crate) enum ImageSide {
    Loading,
    Pixels(Arc<egui::ColorImage>),
    Preview(egui::TextureHandle),
    Absent,
    Unavailable(String),
}

impl ImageSide {
    fn read(
        root: &Path,
        version: &Version,
        path: &str,
        snapshot: Result<Vec<u8>>,
        object_dir: &OnceLock<Result<PathBuf>>,
        canceled: &AtomicBool,
    ) -> Self {
        if matches!(version, Version::Empty) {
            return Self::Absent;
        }
        let decode = || {
            let mut bytes = snapshot?;
            if let Some(pointer) = LfsPointer::parse(&bytes)? {
                let dir = object_dir
                    .get_or_init(|| git::lfs_object_dir(root))
                    .as_ref()
                    .map_err(|_| anyhow::anyhow!("无法定位本地 Git LFS 缓存"))?;
                bytes = if let Some(cached) = pointer.read(dir)? {
                    cached
                } else {
                    git::download_lfs_object(root, path, &bytes, canceled)?;
                    pointer
                        .read(dir)?
                        .context("Git LFS 对象未下载，可能被仓库设置跳过")?
                };
            }
            decode_image(&bytes)
        };
        match decode() {
            Ok(pixels) => Self::Pixels(Arc::new(pixels)),
            Err(error) => Self::Unavailable(error.to_string()),
        }
    }

    pub fn upload(&mut self, ctx: &egui::Context) {
        let Self::Pixels(pixels) = self else { return };
        let max_edge = ctx.input(|input| input.max_texture_side);
        *self = if pixels.size.iter().any(|&size| size > max_edge) {
            Self::Unavailable("图片尺寸超过当前显卡的纹理上限".into())
        } else {
            Self::Preview(ctx.load_texture(
                "image-diff",
                egui::ImageData::Color(pixels.clone()),
                egui::TextureOptions::LINEAR,
            ))
        };
    }
}

fn decode_image(bytes: &[u8]) -> Result<egui::ColorImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("无法识别图片格式")?;
    ensure!(
        reader.format().is_some_and(supported_format),
        "无法识别图片格式"
    );
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image_error = |error| match error {
        ImageError::Limits(_) => anyhow::anyhow!(IMAGE_LIMIT),
        _ => anyhow::anyhow!("无法解码图片"),
    };
    let decoder = reader.into_decoder().map_err(image_error)?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        IMAGE_LIMIT
    );
    let pixels = image::DynamicImage::from_decoder(decoder)
        .map_err(image_error)?
        .into_rgba8();
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        pixels.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::Area;
    use crossbeam_channel::bounded;
    use std::fs;
    use std::sync::Mutex;
    use std::time::Duration;

    fn image_bytes(width: u32, format: ImageFormat) -> Result<Vec<u8>> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(width, 2).write_to(&mut bytes, format)?;
        Ok(bytes.into_inner())
    }

    fn fixture(root: &Path, args: &[&str]) -> Result<String> {
        let output = git::fixture_command(root).args(args).output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    fn compare(root: &Path, pair: &Compare, canceled: bool) -> ImageDiff {
        assert!(supports_pair(pair));
        let log = ReadLog::queue("diff", root, 1);
        let images = Mutex::new(ImageDiff::pending());
        load(
            root,
            pair,
            &log,
            &AtomicBool::new(canceled),
            |column, image| {
                images.lock().unwrap().apply(column, image);
            },
        );
        let images = images.into_inner().unwrap();
        assert!(!images.is_loading());
        images
    }

    fn dimensions(side: &ImageSide) -> Option<[usize; 2]> {
        match side {
            ImageSide::Pixels(pixels) => Some(pixels.size),
            _ => None,
        }
    }

    #[test]
    fn image_diff_can_deliver_either_column_while_its_peer_is_busy() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        fs::write(root.join("left.png"), image_bytes(2, ImageFormat::Png)?)?;
        fs::write(root.join("right.png"), image_bytes(3, ImageFormat::Png)?)?;
        let pair = Compare {
            left: Version::Worktree,
            right: Version::Worktree,
            base: Version::Empty,
            left_path: "left.png".into(),
            right_path: "right.png".into(),
            left_gitlink: false,
            right_gitlink: false,
        };
        let log = ReadLog::queue("diff", root, 1);
        for delayed in [ImageColumn::Left, ImageColumn::Right] {
            let (emitted, peer) = bounded(1);
            load(
                root,
                &pair,
                &log,
                &AtomicBool::new(false),
                |column, image| {
                    assert!(matches!(image, ImageSide::Pixels(_)));
                    if column == delayed {
                        peer.recv_timeout(Duration::from_secs(5)).unwrap();
                    } else {
                        emitted.send(()).unwrap();
                    }
                },
            );
        }
        Ok(())
    }

    #[test]
    fn image_diff_decodes_supported_formats_and_preserves_alpha() -> Result<()> {
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::Bmp,
            ImageFormat::Gif,
            ImageFormat::WebP,
        ] {
            assert_eq!(
                decode_image(&image_bytes(3, format)?)?.size,
                [3, 2],
                "{format:?}"
            );
        }
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2).write_to(&mut png, ImageFormat::Png)?;
        assert_eq!(
            decode_image(png.get_ref())?.pixels[0],
            egui::Color32::TRANSPARENT
        );
        assert!(decode_image(&image_bytes(2, ImageFormat::Png)?[..8]).is_err());
        let mut bmp = image_bytes(2, ImageFormat::Bmp)?;
        bmp[18..22].copy_from_slice(&5000_u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&5000_u32.to_le_bytes());
        assert_eq!(decode_image(&bmp).unwrap_err().to_string(), IMAGE_LIMIT);
        Ok(())
    }

    #[test]
    fn image_diff_uses_commit_index_and_worktree_snapshots() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        let path = "角色 图片.PNG";
        fixture(root, &["init", "-b", "main"])?;
        let write = |width| -> Result<()> {
            fs::write(root.join(path), image_bytes(width, ImageFormat::Png)?)?;
            Ok(())
        };
        write(2)?;
        fixture(root, &["add", "--", path])?;
        fixture(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "baseline",
            ],
        )?;
        write(3)?;
        fixture(root, &["add", "--", path])?;
        write(4)?;
        let state = git::status(root)?;
        for (area, left, right) in [(Area::Staged, 2, 3), (Area::Unstaged, 3, 4)] {
            let change = state
                .files
                .iter()
                .find(|change| change.area == area)
                .unwrap();
            let images = compare(
                root,
                &git::compare(change, None, state.head.as_deref()),
                false,
            );
            assert_eq!(dimensions(&images.left), Some([left, 2]));
            assert_eq!(dimensions(&images.right), Some([right, 2]));
        }
        fs::remove_file(root.join(path))?;
        let state = git::status(root)?;
        let removed = state
            .files
            .iter()
            .find(|change| change.area == Area::Unstaged)
            .unwrap();
        let images = compare(
            root,
            &git::compare(removed, None, state.head.as_deref()),
            false,
        );
        assert_eq!(dimensions(&images.left), Some([3, 2]));
        assert!(matches!(images.right, ImageSide::Absent));
        Ok(())
    }

    #[test]
    fn image_diff_reads_shared_lfs_cache_and_keeps_the_available_side() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        fixture(root, &["init", "-b", "main"])?;
        fixture(root, &["config", "lfs.storage", "LFS 缓存"])?;
        fixture(root, &["config", "lfs.fetchexclude", "*"])?;
        let path = "image.png";
        let cached = "1234".repeat(16);
        let missing = "abcd".repeat(16);
        let bytes = image_bytes(2, ImageFormat::Png)?;
        let object_dir = root.join(".git/LFS 缓存/objects/12/34");
        fs::create_dir_all(&object_dir)?;
        fs::write(object_dir.join(&cached), &bytes)?;
        let pointer = |oid: &str| {
            format!(
                "version https://git-lfs.github.com/spec/v1\r\noid sha256:{oid}\r\nsize {}\r\n",
                bytes.len()
            )
        };
        fs::write(root.join(path), pointer(&cached))?;
        fixture(root, &["add", "--", path])?;
        fixture(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "baseline",
            ],
        )?;
        let head = fixture(root, &["rev-parse", "HEAD"])?;
        fixture(
            root,
            &["worktree", "add", "--detach", "linked worktree", &head],
        )?;
        let linked = root.join("linked worktree");
        fs::write(linked.join(path), pointer(&missing))?;
        fixture(&linked, &["add", "--", path])?;
        let pair = Compare {
            left: Version::Commit(head.clone()),
            right: Version::Index,
            base: Version::Commit(head),
            left_path: path.into(),
            right_path: path.into(),
            left_gitlink: false,
            right_gitlink: false,
        };
        let images = compare(&linked, &pair, false);
        assert_eq!(dimensions(&images.left), Some([2, 2]));
        assert!(
            matches!(images.right, ImageSide::Unavailable(reason) if reason.contains("Git LFS 对象"))
        );
        assert_eq!(fs::read_to_string(linked.join(path))?, pointer(&missing));
        assert!(
            !root
                .join(".git/LFS 缓存/objects/ab/cd")
                .join(&missing)
                .exists()
        );
        Ok(())
    }

    #[test]
    fn image_diff_downloads_only_the_selected_lfs_snapshot() -> Result<()> {
        let sandbox = tempfile::tempdir()?;
        let root = sandbox.path();
        fixture(root, &["init", "-b", "main"])?;
        if fixture(root, &["lfs", "version"]).is_err() {
            eprintln!("Git LFS unavailable, skipping transfer fixture");
            return Ok(());
        }
        let remote = root.join("local remote.git");
        fixture(root, &["config", "core.hooksPath", "project hooks"])?;
        fs::create_dir(&remote)?;
        fixture(&remote, &["init", "--bare"])?;
        fixture(root, &["remote", "add", "origin", remote.to_str().unwrap()])?;

        let put_on_remote = |width| -> Result<(String, PathBuf)> {
            let bytes = image_bytes(width, ImageFormat::Png)?;
            let temp = root.join("encoded.png");
            fs::write(&temp, &bytes)?;
            let pointer = fixture(root, &["lfs", "pointer", "--file", temp.to_str().unwrap()])?;
            let oid = pointer
                .lines()
                .find_map(|line| line.strip_prefix("oid sha256:"))
                .context("fixture pointer has no object ID")?;
            let object = Path::new("lfs/objects")
                .join(&oid[..2])
                .join(&oid[2..4])
                .join(oid);
            let remote_object = remote.join(&object);
            fs::create_dir_all(remote_object.parent().unwrap())?;
            fs::write(remote_object, bytes)?;
            Ok((format!("{pointer}\n"), root.join(".git").join(object)))
        };

        let (old, old_object) = put_on_remote(2)?;
        let (new, new_object) = put_on_remote(3)?;
        let (other, other_object) = put_on_remote(4)?;
        let path = "角色 图片.png";
        fs::write(root.join(path), old)?;
        fixture(root, &["add", "--", path])?;
        fixture(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "-m",
                "baseline",
            ],
        )?;
        fs::write(root.join(path), &new)?;
        fs::write(root.join("other.png"), other)?;
        fixture(root, &["add", "--", path, "other.png"])?;
        let state = git::status(root)?;
        let change = state
            .files
            .iter()
            .find(|change| change.path == path)
            .unwrap();
        let pair = git::compare(change, None, state.head.as_deref());
        assert!(!old_object.exists() && !new_object.exists());
        let canceled = compare(root, &pair, true);
        assert!(
            matches!(canceled.right, ImageSide::Unavailable(reason) if reason.contains("已取消"))
        );
        assert!(!old_object.exists() && !new_object.exists());

        let views = compare(root, &pair, false);
        assert_eq!(dimensions(&views.left), Some([2, 2]));
        assert_eq!(dimensions(&views.right), Some([3, 2]));
        assert!(old_object.is_file() && new_object.is_file());
        assert!(!other_object.exists());
        assert!(!root.join("project hooks").exists());
        assert_eq!(fs::read_to_string(root.join(path))?, new);
        assert_eq!(fixture(root, &["show", &format!(":{path}")])?, new.trim());
        Ok(())
    }
}
