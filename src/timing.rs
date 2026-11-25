use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub(crate) struct Timings {
    file: File,
    started: Instant,
    activation: Instant,
    graph: bool,
    status: bool,
}

impl Timings {
    pub fn open(started: Instant) -> Option<Self> {
        let path = std::env::var_os("REFDESK_TIMINGS")?;
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .ok()?;
        Some(Self {
            file,
            started,
            activation: started,
            graph: false,
            status: false,
        })
    }

    pub fn activate(&mut self, path: &Path) {
        self.activation = Instant::now();
        self.graph = false;
        self.status = false;
        self.write("activate", path);
    }

    pub fn presented(&mut self, path: &Path, graph: bool, status: bool) {
        if graph && !self.graph {
            self.graph = true;
            self.write("graph-ready", path);
        }
        if status && !self.status {
            self.status = true;
            self.write("status-ready", path);
        }
    }

    fn write(&mut self, event: &str, path: &Path) {
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = writeln!(
            self.file,
            "{event}\t{unix_ms}\t{}\t{}\t{}",
            self.started.elapsed().as_millis(),
            self.activation.elapsed().as_millis(),
            path.display(),
        );
    }
}

pub(crate) fn capture_viewport() -> Option<[f32; 2]> {
    std::env::var_os("REFDESK_CAPTURE")?;
    let setting = std::env::var("REFDESK_CAPTURE_VIEWPORT").ok()?;
    let (width, height) = setting.split_once('x')?;
    let dimensions = [width.parse::<f32>().ok()?, height.parse::<f32>().ok()?];
    ((930.0..=3000.0).contains(&dimensions[0]) && (610.0..=2000.0).contains(&dimensions[1]))
        .then_some(dimensions)
}

pub(crate) fn capture_sidebar() -> Option<f32> {
    std::env::var_os("REFDESK_CAPTURE")?;
    std::env::var("REFDESK_CAPTURE_SIDEBAR")
        .ok()?
        .parse::<f32>()
        .ok()
        .filter(|width| (184.0..=221.0).contains(width))
}

pub(crate) struct FrameCapture {
    path: std::path::PathBuf,
    pub commit: Option<String>,
    pub branch_query: Option<String>,
    scale: Option<f32>,
    requested: bool,
}

impl FrameCapture {
    pub fn from_environment() -> Option<Self> {
        Some(Self {
            path: std::env::var_os("REFDESK_CAPTURE")?.into(),
            commit: std::env::var("REFDESK_CAPTURE_COMMIT").ok(),
            branch_query: std::env::var("REFDESK_CAPTURE_BRANCH").ok(),
            scale: std::env::var("REFDESK_CAPTURE_SCALE")
                .ok()
                .and_then(|value| value.parse::<f32>().ok())
                .filter(|scale| (1.0..=2.5).contains(scale)),
            requested: false,
        })
    }

    pub fn adjust_scale(&mut self, ctx: &eframe::egui::Context) {
        if let Some(native) = ctx.native_pixels_per_point()
            && let Some(scale) = self.scale.take()
        {
            ctx.set_zoom_factor(scale / native);
        }
    }

    pub fn queue(&mut self, ctx: &eframe::egui::Context) {
        if !self.requested {
            self.requested = true;
            ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Screenshot(Default::default()));
        }
    }

    pub fn save(&self, ctx: &eframe::egui::Context) -> bool {
        use eframe::egui::Event;
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        let Some(image) = image else { return false };
        let _ = save_png(&self.path, &image);
        true
    }
}

fn save_png(path: &Path, image: &eframe::egui::ColorImage) -> anyhow::Result<()> {
    let mut encoder = png::Encoder::new(
        File::create(path)?,
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgb);
    let pixels = image
        .pixels
        .iter()
        .flat_map(|pixel| [pixel.r(), pixel.g(), pixel.b()])
        .collect::<Vec<_>>();
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
