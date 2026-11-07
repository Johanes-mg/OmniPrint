#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod journal;
mod mdns;
mod pare_feu;
mod printer;
mod reseau_utils;
mod session;
mod theme;
mod web;

fn main() -> eframe::Result<()> {
    let icone = theme::charger_icone_fenetre();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1040.0, 760.0])
        .with_min_inner_size([860.0, 600.0])
        .with_title("OmniPrint");
    if let Some(i) = icone {
        viewport = viewport.with_icon(i);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "OmniPrint",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
