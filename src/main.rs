mod app;
mod classic;
mod config;
mod eq;
mod library;
mod locations;
mod metadata;
mod mpris;
mod player;
mod playlist;
mod search;
mod session;
mod skin;
mod tree;
mod visualizer;
mod waveform;
mod wsz;

use app::UnAmpApp;
use config::AppConfig;

const WINDOW_ICON_PNG: &[u8] = include_bytes!("../assets/unamp-icon-128.png");

fn build_window_icon() -> egui::IconData {
    let icon = image::load_from_memory_with_format(WINDOW_ICON_PNG, image::ImageFormat::Png)
        .expect("embedded window icon should decode as PNG")
        .into_rgba8();
    let (width, height) = icon.dimensions();

    egui::IconData {
        rgba: icon.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result {
    let config = AppConfig::load();

    let width = config.window_width.unwrap_or(1285.0);
    let height = config.window_height.unwrap_or(860.0);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("UnAmp")
            .with_app_id("unamp")
            .with_icon(build_window_icon())
            .with_inner_size([width, height])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "unamp",
        native_options,
        Box::new(|cc| Ok(Box::new(UnAmpApp::new(cc, config)))),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_icon_buffer_matches_declared_dimensions() {
        let icon = super::build_window_icon();
        assert_eq!(icon.width, 128);
        assert_eq!(icon.height, 128);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }
}
