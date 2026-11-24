#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actuation_page;
mod app;
mod capture;
mod keyboard_view;
mod keymap_page;
mod macros_page;
mod socd_page;
mod graphite;
mod worker;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Fizz control")
            .with_inner_size([960.0, 640.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Fizz control",
        options,
        Box::new(|cc| {
            graphite::install(
                &cc.egui_ctx,
                &graphite::Fonts {
                    regular: include_bytes!("../assets/fonts/Inter-Regular.ttf"),
                    medium: include_bytes!("../assets/fonts/Inter-Medium.ttf"),
                    semibold: include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
                    mono: include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
                    icons: Some(lucide_icons::LUCIDE_FONT_BYTES),
                },
            );
            Ok(Box::new(app::FizzApp::new(cc)))
        }),
    )
}
