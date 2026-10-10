//! Screenshot mode for the docs (`make screenshot`): with `UNAMP_SCREENSHOT`
//! set to a PNG path, UnAmp starts playing the restored playlist, waits for
//! the spectrum to fill, saves a screenshot of its window there and quits.
//!
//! The mode also keeps the user's machine out of the picture and the
//! desktop out of the run: no mounted drives or network shares in the
//! sidebar, and no MPRIS player in GNOME's media controls.

use std::path::PathBuf;

pub const ENV: &str = "UNAMP_SCREENSHOT";
/// Seconds of playback before the capture, so the spectrum has settled.
const AFTER_SECS: f64 = 4.0;

/// Where to save the screenshot, when running in screenshot mode.
pub fn requested() -> Option<PathBuf> {
    std::env::var_os(ENV).filter(|p| !p.is_empty()).map(PathBuf::from)
}

pub struct Screenshot {
    path: PathBuf,
    started_at: Option<f64>,
    requested: bool,
}

/// What the app should do this frame.
pub enum Step {
    /// Start playback.
    Play,
    Wait,
    /// The screenshot was saved (or failed); close the window.
    Done(Result<PathBuf, String>),
}

impl Screenshot {
    pub fn from_env() -> Option<Self> {
        requested().map(|path| Self { path, started_at: None, requested: false })
    }

    pub fn step(&mut self, ctx: &egui::Context) -> Step {
        let now = ctx.input(|i| i.time);
        let Some(started) = self.started_at else {
            self.started_at = Some(now);
            return Step::Play;
        };
        ctx.request_repaint();
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            return Step::Done(self.save(&image));
        }
        if !self.requested && now - started >= AFTER_SECS {
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        Step::Wait
    }

    fn save(&self, image: &egui::ColorImage) -> Result<PathBuf, String> {
        let [w, h] = image.size;
        let buf = image::RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec())
            .ok_or("screenshot has the wrong size")?;
        buf.save(&self.path).map_err(|e| e.to_string())?;
        Ok(self.path.clone())
    }
}
