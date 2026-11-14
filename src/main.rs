#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod elevation;
mod journal;
mod mdns;
mod pare_feu;
mod partage;
mod plage_pages;
mod printer;
mod reseau_utils;
mod session;
mod theme;
mod web;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == elevation::WORKER_FLAG {
        let code = elevation::run_worker(&args[2..]);
        std::process::exit(code);
    }

    let session = session::Session::charger();
    let (largeur, hauteur) = session
        .taille_fenetre
        .map(|(w, h)| (w.max(900.0), h.max(640.0)))
        .unwrap_or((1100.0, 760.0));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([largeur, hauteur])
            .with_min_inner_size([900.0, 640.0])
            .with_title("OmniPrint")
            .with_icon(
                theme::charger_icone_fenetre()
                    .map(std::sync::Arc::new)
                    .unwrap_or_default(),
            ),
        ..Default::default()
    };

    eframe::run_native(
        "OmniPrint",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
