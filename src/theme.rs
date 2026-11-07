use egui::{Color32, Context, Rounding, Stroke, TextureHandle};

pub struct Palette;

impl Palette {
    pub const FOND: Color32 = Color32::from_rgb(16, 18, 24);
    pub const CARTE: Color32 = Color32::from_rgb(24, 27, 36);
    pub const BORDURE: Color32 = Color32::from_rgb(45, 50, 66);
    pub const TEXTE: Color32 = Color32::from_rgb(232, 235, 245);
    pub const TEXTE_DOUX: Color32 = Color32::from_rgb(150, 158, 178);
    pub const ACCENT: Color32 = Color32::from_rgb(90, 156, 255);
    pub const ACCENT_FONCE: Color32 = Color32::from_rgb(58, 110, 200);
    pub const SUCCES: Color32 = Color32::from_rgb(72, 199, 142);
    pub const AVERT: Color32 = Color32::from_rgb(240, 180, 80);
    pub const ERREUR: Color32 = Color32::from_rgb(232, 94, 94);
}

pub struct PaletteClaire;

impl PaletteClaire {
    pub const FOND: Color32 = Color32::from_rgb(244, 246, 250);
    pub const CARTE: Color32 = Color32::from_rgb(255, 255, 255);
    pub const BORDURE: Color32 = Color32::from_rgb(210, 216, 228);
    pub const TEXTE: Color32 = Color32::from_rgb(28, 32, 44);
    pub const TEXTE_DOUX: Color32 = Color32::from_rgb(96, 106, 128);
    pub const ACCENT: Color32 = Color32::from_rgb(40, 100, 210);
    pub const ACCENT_FONCE: Color32 = Color32::from_rgb(34, 82, 176);
    pub const SUCCES: Color32 = Color32::from_rgb(34, 158, 108);
    pub const AVERT: Color32 = Color32::from_rgb(200, 138, 32);
    pub const ERREUR: Color32 = Color32::from_rgb(200, 64, 64);
}

fn style_commun(ctx: &Context) -> egui::Style {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(14.0, 8.0);
    style.spacing.interact_size = egui::vec2(40.0, 28.0);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = false;
    style.spacing.scroll.bar_inner_margin = 2.0;
    style.spacing.scroll.bar_outer_margin = 0.0;
    style
}

pub fn appliquer(ctx: &Context) {
    let mut style = style_commun(ctx);
    style.visuals = egui::Visuals::dark();
    let v = &mut style.visuals;
    v.panel_fill = Palette::FOND;
    v.window_fill = Palette::CARTE;
    v.extreme_bg_color = Color32::from_rgb(12, 13, 18);
    v.faint_bg_color = Palette::CARTE;
    v.override_text_color = Some(Palette::TEXTE);
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.window_stroke = Stroke::new(1.0_f32, Palette::BORDURE);
    v.selection.bg_fill = Palette::ACCENT_FONCE;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = Palette::ACCENT;

    v.widgets.noninteractive.bg_fill = Palette::CARTE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Palette::BORDURE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, Palette::TEXTE_DOUX);
    v.widgets.noninteractive.rounding = Rounding::same(8.0);

    v.widgets.inactive.bg_fill = Palette::CARTE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Palette::BORDURE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, Palette::TEXTE);
    v.widgets.inactive.rounding = Rounding::same(8.0);

    v.widgets.hovered.bg_fill = Color32::from_rgb(42, 48, 64);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Palette::ACCENT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Palette::TEXTE);
    v.widgets.hovered.rounding = Rounding::same(8.0);

    v.widgets.active.bg_fill = Palette::ACCENT_FONCE;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, Palette::ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.active.rounding = Rounding::same(8.0);

    v.widgets.open.bg_fill = Palette::CARTE;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, Palette::ACCENT);
    v.widgets.open.fg_stroke = Stroke::new(1.0_f32, Palette::TEXTE);

    ctx.set_style(style);
}

pub fn appliquer_clair(ctx: &Context) {
    let mut style = style_commun(ctx);
    style.visuals = egui::Visuals::light();
    let v = &mut style.visuals;
    v.panel_fill = PaletteClaire::FOND;
    v.window_fill = PaletteClaire::CARTE;
    v.extreme_bg_color = Color32::from_rgb(232, 236, 244);
    v.faint_bg_color = PaletteClaire::CARTE;
    v.override_text_color = Some(PaletteClaire::TEXTE);
    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);
    v.window_stroke = Stroke::new(1.0_f32, PaletteClaire::BORDURE);
    v.selection.bg_fill = PaletteClaire::ACCENT_FONCE;
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = PaletteClaire::ACCENT;

    v.widgets.noninteractive.bg_fill = PaletteClaire::CARTE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, PaletteClaire::BORDURE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, PaletteClaire::TEXTE_DOUX);
    v.widgets.noninteractive.rounding = Rounding::same(8.0);

    v.widgets.inactive.bg_fill = PaletteClaire::CARTE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, PaletteClaire::BORDURE);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, PaletteClaire::TEXTE);
    v.widgets.inactive.rounding = Rounding::same(8.0);

    v.widgets.hovered.bg_fill = Color32::from_rgb(232, 238, 250);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, PaletteClaire::ACCENT);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, PaletteClaire::TEXTE);
    v.widgets.hovered.rounding = Rounding::same(8.0);

    v.widgets.active.bg_fill = PaletteClaire::ACCENT_FONCE;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, PaletteClaire::ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.active.rounding = Rounding::same(8.0);

    v.widgets.open.bg_fill = PaletteClaire::CARTE;
    v.widgets.open.bg_stroke = Stroke::new(1.0_f32, PaletteClaire::ACCENT);
    v.widgets.open.fg_stroke = Stroke::new(1.0_f32, PaletteClaire::TEXTE);

    ctx.set_style(style);
}

pub fn charger_icone_fenetre() -> Option<egui::IconData> {
    let data = std::fs::read("images/logo.png").ok()?;
    let img = image::load_from_memory(&data).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}

pub fn charger_texture(ctx: &Context, chemin: &str, nom: &str) -> Option<TextureHandle> {
    let data = std::fs::read(chemin).ok()?;
    let img = image::load_from_memory(&data).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
    Some(ctx.load_texture(nom, color_image, egui::TextureOptions::LINEAR))
}

pub fn separateur(ui: &mut egui::Ui) {
    ui.add_space(10.0);
    let largeur = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(largeur, 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, Rounding::ZERO, Palette::BORDURE);
    ui.add_space(10.0);
}

pub fn badge(ui: &mut egui::Ui, texte: &str, couleur: Color32) {
    let padding = egui::vec2(10.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(texte.len() as f32 * 7.0 + padding.x * 2.0, 22.0),
        egui::Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, Rounding::same(11.0), couleur.gamma_multiply(0.14));
    ui.painter().rect_stroke(
        rect,
        Rounding::same(11.0),
        Stroke::new(1.0_f32, couleur.gamma_multiply(0.3)),
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        texte,
        egui::FontId::proportional(12.0),
        couleur,
    );
}

pub fn bouton_theme(
    ui: &mut egui::Ui,
    mode_sombre: bool,
    icone_soleil: Option<&TextureHandle>,
    icone_lune: Option<&TextureHandle>,
) -> bool {
    let taille = 32.0;
    let (rect, reponse) = ui.allocate_exact_size(egui::vec2(taille, taille), egui::Sense::click());
    let centre = rect.center();
    let rayon = taille / 2.0;
    let fond = if reponse.hovered() {
        if mode_sombre { Color32::from_rgb(42, 48, 64) } else { Color32::from_rgb(232, 238, 250) }
    } else {
        if mode_sombre { Palette::CARTE } else { PaletteClaire::CARTE }
    };
    let bordure = if mode_sombre { Palette::BORDURE } else { PaletteClaire::BORDURE };
    ui.painter().circle_filled(centre, rayon, fond);
    ui.painter().circle_stroke(centre, rayon, Stroke::new(1.0_f32, bordure));

    let icone = if mode_sombre { icone_soleil } else { icone_lune };
    if let Some(t) = icone {
        let marge = 5.0;
        let rect_img = egui::Rect::from_center_size(
            centre,
            egui::vec2(taille - marge * 2.0, taille - marge * 2.0),
        );
        ui.painter().image(
            t.id(),
            rect_img,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let symbole = if mode_sombre { "☀" } else { "☾" };
        let couleur = if mode_sombre { Palette::TEXTE } else { PaletteClaire::TEXTE };
        ui.painter().text(
            centre,
            egui::Align2::CENTER_CENTER,
            symbole,
            egui::FontId::proportional(16.0),
            couleur,
        );
    }
    reponse.clicked()
}

pub fn bouton_popup(
    ui: &mut egui::Ui,
    mode_sombre: bool,
    icone_popup: Option<&TextureHandle>,
) -> bool {
    let taille = 32.0;
    let (rect, reponse) = ui.allocate_exact_size(egui::vec2(taille, taille), egui::Sense::click());
    let centre = rect.center();
    let rayon = taille / 2.0;
    let fond = if reponse.hovered() {
        if mode_sombre { Color32::from_rgb(42, 48, 64) } else { Color32::from_rgb(232, 238, 250) }
    } else {
        if mode_sombre { Palette::CARTE } else { PaletteClaire::CARTE }
    };
    let bordure = if mode_sombre { Palette::BORDURE } else { PaletteClaire::BORDURE };
    ui.painter().circle_filled(centre, rayon, fond);
    ui.painter().circle_stroke(centre, rayon, Stroke::new(1.0_f32, bordure));

    if let Some(t) = icone_popup {
        let marge = 5.0;
        let rect_img = egui::Rect::from_center_size(
            centre,
            egui::vec2(taille - marge * 2.0, taille - marge * 2.0),
        );
        ui.painter().image(
            t.id(),
            rect_img,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        let couleur = if mode_sombre { Palette::TEXTE } else { PaletteClaire::TEXTE };
        ui.painter().text(
            centre,
            egui::Align2::CENTER_CENTER,
            "QR",
            egui::FontId::proportional(12.0),
            couleur,
        );
    }
    reponse.clicked()
}

pub fn generer_qr(contenu: &str, taille_pixels: u32) -> Option<egui::ColorImage> {
    let code = qrcode::QrCode::new(contenu.as_bytes()).ok()?;
    let largeur = code.width();
    let cellules = code.to_colors();
    let marge = 4usize;
    let total = largeur + 2 * marge;
    let mut image = egui::ColorImage::new([total, total], Color32::WHITE);
    for y in 0..largeur {
        for x in 0..largeur {
            let idx = y * largeur + x;
            if cellules[idx] == qrcode::Color::Dark {
                image[(x + marge, y + marge)] = Color32::BLACK;
            }
        }
    }
    let _ = taille_pixels;
    Some(image)
}

