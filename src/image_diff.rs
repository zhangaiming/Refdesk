use crate::builtin_diff::{Issue, Snapshot};
use crate::git::{self, Compare, LfsPointer};
use eframe::egui;
use image::{ImageDecoder, ImageError, ImageFormat, ImageReader, Limits};
use std::io::Cursor;
use std::path::Path;

const MAX_EDGE: u32 = 8192;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
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

pub(crate) fn load(snapshot: Snapshot<'_>) -> Result<egui::ColorImage, Issue> {
    let mut bytes = snapshot.read()?;
    if let Some(pointer) = LfsPointer::parse(&bytes).map_err(|_| Issue::LfsPointer)? {
        let dir = git::lfs_object_dir(snapshot.root).map_err(|_| Issue::LfsCache)?;
        let required = |pointer: &LfsPointer<'_>| {
            pointer.read(&dir).map_err(|error| {
                if error.is::<git::SnapshotLimit>() {
                    Issue::SnapshotLimit
                } else {
                    Issue::LfsCache
                }
            })
        };
        bytes = if let Some(cached) = required(&pointer)? {
            cached
        } else {
            git::download_lfs_object(snapshot.root, snapshot.path, &bytes, || {
                snapshot.cancel.requested()
            })
            .map_err(|_| Issue::LfsDownload)?;
            required(&pointer)?.ok_or(Issue::LfsMissing)?
        };
    }
    decode_image(&bytes)
}

fn decode_image(bytes: &[u8]) -> Result<egui::ColorImage, Issue> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Issue::Decode)?;
    if !reader.format().is_some_and(supported_format) {
        return Err(Issue::Decode);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_EDGE);
    limits.max_image_height = Some(MAX_EDGE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image_error = |error| match error {
        ImageError::Limits(_) => Issue::PreviewLimit,
        _ => Issue::Decode,
    };
    let decoder = reader.into_decoder().map_err(image_error)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(Issue::PreviewLimit);
    }
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
    use crate::builtin_diff::{Cancellation, Column, Side};
    use crate::git::{Area, Version};
    use crate::logging::ReadLog;
    use anyhow::{Context, Result, ensure};
    use std::fs;
    use std::path::PathBuf;

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

    struct Images {
        left: Side<egui::ColorImage>,
        right: Side<egui::ColorImage>,
    }

    fn compare(root: &Path, pair: &Compare, canceled: bool) -> Images {
        assert!(supports_pair(pair));
        let log = ReadLog::queue("diff", root, 1);
        let (sender, cancel) = Cancellation::new();
        let _sender = (!canceled).then_some(sender);
        let read = |column| {
            let snapshot = Snapshot::new(root, pair, column, &log, &cancel);
            if matches!(snapshot.version, Version::Empty) {
                Side::Absent
            } else {
                match load(snapshot) {
                    Ok(image) => Side::Ready(image),
                    Err(issue) => Side::Failed(issue),
                }
            }
        };
        Images {
            left: read(Column::Left),
            right: read(Column::Right),
        }
    }

    fn dimensions(side: &Side<egui::ColorImage>) -> Option<[usize; 2]> {
        side.ready().map(|pixels| pixels.size)
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
                decode_image(&image_bytes(3, format)?).unwrap().size,
                [3, 2],
                "{format:?}"
            );
        }
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2, 2).write_to(&mut png, ImageFormat::Png)?;
        assert_eq!(
            decode_image(png.get_ref()).unwrap().pixels[0],
            egui::Color32::TRANSPARENT
        );
        assert!(decode_image(&image_bytes(2, ImageFormat::Png)?[..8]).is_err());
        let mut bmp = image_bytes(2, ImageFormat::Bmp)?;
        bmp[18..22].copy_from_slice(&5000_u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&5000_u32.to_le_bytes());
        assert_eq!(decode_image(&bmp).unwrap_err(), Issue::PreviewLimit);
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
        assert!(matches!(images.right, Side::Absent));
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
        assert!(matches!(
            images.right,
            Side::Failed(Issue::LfsMissing | Issue::LfsDownload)
        ));
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
        assert!(matches!(canceled.right, Side::Failed(Issue::Read)));
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
