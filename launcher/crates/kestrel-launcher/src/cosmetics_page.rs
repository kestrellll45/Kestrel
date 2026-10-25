//! The launcher's Cosmetics page: kill effects, skies and capes.
//!
//! Selections are written as a launcher request (see `kestrel_core::cosmetics`)
//! which the client mod applies at start-up, or within about a second while
//! the game is running. Uploads are validated and copied into the client's
//! cosmetics folders on a worker thread; previews are decoded off the UI
//! thread and cached as textures.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use egui::{Color32, CornerRadius, Pos2, RichText, Sense, Stroke, Vec2};
use kestrel_core::cosmetics::{
    self, BUILTIN_SKIES, CAPE_NONE, CUSTOM_PREFIX, CosmeticSettings, KILL_EFFECTS, Kind, Rgba,
};
use kestrel_core::paths::Layout;

use crate::app::{ACCENT, CARD, ERR, MUTED, OK, WARN, card, heading};

// Bundled textures, shared with the client mod's resources, for previews.
// One entry per catalog cape (capes.json); animated GIF capes use their
// static fallback PNG here.
pub(crate) const BUNDLED_CAPES: &[(&str, &[u8])] = &[
    (
        "ember",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/capes/ember.png"
        ),
    ),
    (
        "glacier",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/capes/glacier.png"
        ),
    ),
    (
        "midnight",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/capes/midnight.png"
        ),
    ),
    (
        "verdant",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/capes/verdant.png"
        ),
    ),
    (
        "aurora",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/capes/aurora.png"
        ),
    ),
    (
        "teto",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/capes/teto.png"
        ),
    ),
];
const BUNDLED_SKIES: &[(&str, &[u8])] = &[
    (
        "dusk",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/sky/dusk.png"
        ),
    ),
    (
        "starry",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/sky/starry.png"
        ),
    ),
    (
        "nebula",
        include_bytes!(
            "../../../../client/src/main/resources/assets/kestrel/textures/sky/nebula.png"
        ),
    ),
];

const CAPE_CARD: Vec2 = Vec2::new(112.0, 150.0);
const SKY_CARD: Vec2 = Vec2::new(228.0, 120.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    KillEffects,
    Skies,
    Capes,
}

enum Event {
    Preview {
        key: String,
        image: Result<Rgba, String>,
    },
    Imported {
        kind: Kind,
        result: Result<String, String>,
    },
}

pub struct CosmeticsPage {
    layout: Layout,
    tab: Tab,
    settings: CosmeticSettings,
    pending: bool,
    custom_capes: Vec<String>,
    custom_skies: Vec<String>,
    /// UUID of the signed-in Microsoft account (None offline / signed out),
    /// used only to decide which restricted catalog capes to offer.
    account_uuid: Option<String>,
    textures: HashMap<String, egui::TextureHandle>,
    preview_errors: HashMap<String, String>,
    requested: std::collections::HashSet<String>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    importing: bool,
    message: Option<(bool, String)>,
    preview_effect: Option<String>,
    preview_started: f64,
    loaded: bool,
}

impl CosmeticsPage {
    pub fn new(layout: Layout) -> Self {
        let (tx, rx) = channel();
        CosmeticsPage {
            layout,
            tab: Tab::KillEffects,
            settings: CosmeticSettings::default(),
            pending: false,
            custom_capes: Vec::new(),
            custom_skies: Vec::new(),
            account_uuid: None,
            textures: HashMap::new(),
            preview_errors: HashMap::new(),
            requested: Default::default(),
            tx,
            rx,
            importing: false,
            message: None,
            preview_effect: None,
            preview_started: 0.0,
            loaded: false,
        }
    }

    /// Set the account whose restricted capes may be offered. The mod checks
    /// ownership again in game; this only keeps the list honest.
    pub fn set_account(&mut self, uuid: Option<&str>) {
        self.account_uuid = uuid.map(str::to_string);
    }

    /// Re-read state from disk (called whenever the page is opened).
    pub fn refresh(&mut self) {
        let state = cosmetics::load_state(&self.layout);
        self.settings = state.settings;
        self.pending = state.pending;
        self.custom_capes = cosmetics::list_custom(&self.layout, Kind::Cape);
        self.custom_skies = cosmetics::list_custom(&self.layout, Kind::Sky);
        self.loaded = true;
    }

    fn save(&mut self, game_running: bool) {
        match cosmetics::save_request(&self.layout, &self.settings) {
            Ok(()) => {
                self.pending = true;
                self.message = Some((
                    true,
                    if game_running {
                        "Saved. Minecraft applies it within a second.".into()
                    } else {
                        "Saved. Applied automatically the next time you press Play.".into()
                    },
                ));
            }
            Err(e) => self.message = Some((false, format!("Could not save: {e}"))),
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context, game_running: bool) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Preview { key, image } => match image {
                    Ok(img) => {
                        let opts = if key.starts_with("cape:") {
                            egui::TextureOptions::NEAREST
                        } else {
                            egui::TextureOptions::LINEAR
                        };
                        let ci = egui::ColorImage::from_rgba_unmultiplied(
                            [img.width as usize, img.height as usize],
                            &img.pixels,
                        );
                        self.textures
                            .insert(key.clone(), ctx.load_texture(key, ci, opts));
                    }
                    Err(e) => {
                        self.preview_errors.insert(key, e);
                    }
                },
                Event::Imported { kind, result } => {
                    self.importing = false;
                    match result {
                        Ok(id) => {
                            match kind {
                                Kind::Cape => {
                                    self.custom_capes =
                                        cosmetics::list_custom(&self.layout, Kind::Cape);
                                    self.settings.cosmetics.cape = id.clone();
                                    self.settings.cosmetics.capes_enabled = true;
                                }
                                Kind::Sky => {
                                    self.custom_skies =
                                        cosmetics::list_custom(&self.layout, Kind::Sky);
                                    self.settings.sky.preset = id.clone();
                                    self.settings.sky.enabled = true;
                                }
                            }
                            // Drop any stale preview for a re-used name.
                            let key = preview_key(kind, &id);
                            self.textures.remove(&key);
                            self.requested.remove(&key);
                            self.save(game_running);
                            let name = id.trim_start_matches(CUSTOM_PREFIX);
                            self.message = Some((
                                true,
                                format!(
                                    "Uploaded and equipped {name}. {}",
                                    self.message.take().map(|m| m.1).unwrap_or_default()
                                ),
                            ));
                        }
                        Err(e) => self.message = Some((false, format!("Upload rejected: {e}"))),
                    }
                }
            }
        }
    }

    /// Request a preview texture (decoded on a background thread, once).
    fn texture(&mut self, kind: Kind, id: &str) -> Option<egui::TextureHandle> {
        let key = preview_key(kind, id);
        if let Some(t) = self.textures.get(&key) {
            return Some(t.clone());
        }
        if self.requested.insert(key.clone()) {
            let tx = self.tx.clone();
            let layout = self.layout.clone();
            let id = id.to_string();
            std::thread::spawn(move || {
                let image = load_preview(&layout, kind, &id);
                let _ = tx.send(Event::Preview { key, image });
            });
        }
        None
    }

    fn upload(&mut self, kind: Kind) {
        let (title, label) = match kind {
            Kind::Cape => (
                "Choose a cape texture",
                "PNG or GIF cape (64x32, HD, animated)",
            ),
            Kind::Sky => ("Choose a sky texture", "PNG sky (3x2 grid, 3:2)"),
        };
        let exts: &[&str] = match kind {
            Kind::Cape => &["png", "PNG", "gif", "GIF"],
            Kind::Sky => &["png", "PNG"],
        };
        let Some(path) = rfd::FileDialog::new()
            .set_title(title)
            .add_filter(label, exts)
            .pick_file()
        else {
            return;
        };
        self.importing = true;
        self.message = Some((true, format!("Checking {}…", path.display())));
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let result = cosmetics::import(&layout, kind, &path).map_err(|e| e.to_string());
            if let Err(e) = &result {
                log::warn!("cosmetic upload of {} rejected: {e}", path.display());
            }
            let _ = tx.send(Event::Imported { kind, result });
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, game_running: bool) {
        if !self.loaded {
            self.refresh();
        }
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx, game_running);

        heading(ui, "Cosmetics");
        ui.label(
            RichText::new(
                "Local cosmetics: rendered by your Kestrel client only. Other players see them only if they run a \
                 compatible client that knows about them — Kestrel does not send cosmetics to servers.",
            )
            .color(MUTED),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (t, name) in [
                (Tab::KillEffects, "Kill effects"),
                (Tab::Skies, "Skies"),
                (Tab::Capes, "Capes"),
            ] {
                let sel = self.tab == t;
                let b = egui::Button::new(RichText::new(name).size(15.0).color(if sel {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                }))
                .fill(if sel { ACCENT } else { CARD })
                .min_size(Vec2::new(120.0, 30.0))
                .corner_radius(CornerRadius::same(6));
                if ui.add(b).clicked() {
                    self.tab = t;
                }
            }
            ui.add_space(12.0);
            if self.pending {
                ui.label(
                    RichText::new(if game_running {
                        "Applying…"
                    } else {
                        "Pending: applied on next launch"
                    })
                    .color(WARN),
                );
            } else {
                ui.label(RichText::new("In sync with the client").color(OK));
            }
        });
        if let Some((ok, msg)) = &self.message {
            ui.add_space(4.0);
            ui.label(RichText::new(msg).color(if *ok { OK } else { ERR }));
        }
        ui.add_space(8.0);

        let before = self.settings.clone();
        match self.tab {
            Tab::KillEffects => self.kill_effects_ui(ui),
            Tab::Skies => self.skies_ui(ui),
            Tab::Capes => self.capes_ui(ui),
        }
        if self.settings != before {
            self.save(game_running);
        }
        if game_running && self.pending {
            // Notice when the running game has picked the request up.
            ctx.request_repaint_after(Duration::from_millis(500));
            if ctx.input(|i| i.time) % 1.0 < 0.05 {
                self.pending = cosmetics::load_state(&self.layout).pending;
            }
        }
        if self.importing || self.requested.len() > self.textures.len() + self.preview_errors.len()
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    // ---- kill effects -------------------------------------------------------

    fn kill_effects_ui(&mut self, ui: &mut egui::Ui) {
        let time = ui.input(|i| i.time);
        card(ui, |ui| {
            ui.checkbox(
                &mut self.settings.kill_effects.enabled,
                RichText::new("Kill effects enabled").strong(),
            );
            ui.add_enabled_ui(self.settings.kill_effects.enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Intensity");
                    ui.add(
                        egui::Slider::new(&mut self.settings.kill_effects.intensity, 0.25..=2.0)
                            .step_by(0.05)
                            .fixed_decimals(2)
                            .suffix("x"),
                    );
                    ui.add_space(12.0);
                    ui.checkbox(&mut self.settings.kill_effects.sounds, "Sounds");
                    ui.add_space(12.0);
                    ui.label("Colour");
                    let c = self.settings.kill_effects.color;
                    let mut col = Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
                    if ui.color_edit_button_srgba(&mut col).changed() {
                        self.settings.kill_effects.color = 0xFF00_0000
                            | (col.r() as u32) << 16
                            | (col.g() as u32) << 8
                            | col.b() as u32;
                    }
                    ui.label(
                        RichText::new("(colour helix / colour burst)")
                            .small()
                            .color(MUTED),
                    );
                });
            });
        });
        ui.add_space(10.0);

        ui.horizontal_top(|ui| {
            // Effect list.
            ui.vertical(|ui| {
                ui.set_width(330.0);
                for (id, name, desc) in KILL_EFFECTS {
                    let equipped = self.settings.kill_effects.effect == *id;
                    let resp = egui::Frame::NONE
                        .fill(if equipped { ACCENT.gamma_multiply(0.25) } else { CARD })
                        .stroke(if equipped { Stroke::new(1.0, ACCENT) } else { Stroke::NONE })
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_width(314.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(*name).strong());
                                if equipped {
                                    ui.label(RichText::new("EQUIPPED").small().color(ACCENT));
                                }
                            });
                            ui.label(RichText::new(*desc).small().color(MUTED));
                        })
                        .response
                        .interact(Sense::click());
                    if resp.hovered() && self.preview_effect.as_deref() != Some(*id) {
                        self.preview_effect = Some(id.to_string());
                        self.preview_started = time;
                    }
                    if resp.clicked() {
                        self.settings.kill_effects.effect = id.to_string();
                        self.preview_effect = Some(id.to_string());
                        self.preview_started = time;
                    }
                    ui.add_space(4.0);
                }
            });
            ui.add_space(12.0);
            // Animated preview.
            ui.vertical(|ui| {
                let shown = self
                    .preview_effect
                    .clone()
                    .unwrap_or_else(|| self.settings.kill_effects.effect.clone());
                let name = KILL_EFFECTS.iter().find(|e| e.0 == shown).map(|e| e.1).unwrap_or("");
                ui.label(RichText::new(format!("Preview: {name}")).strong());
                let (resp, painter) = ui.allocate_painter(Vec2::new(300.0, 260.0), Sense::click());
                painter.rect_filled(resp.rect, CornerRadius::same(8), Color32::from_rgb(0x0E, 0x10, 0x14));
                let t = ((time - self.preview_started) % 2.2) as f32;
                draw_effect_preview(
                    &painter,
                    resp.rect,
                    &shown,
                    t,
                    self.settings.kill_effects.intensity,
                    self.settings.kill_effects.color,
                );
                ui.ctx().request_repaint_after(Duration::from_millis(16));
                ui.label(
                    RichText::new("Approximate 2D preview. For the real effect in-game: Right Shift > Kill effects > Preview effect.")
                        .small()
                        .color(MUTED),
                );
                if !self.settings.kill_effects.enabled {
                    ui.label(RichText::new("Kill effects are disabled.").color(WARN));
                }
            });
        });
    }

    // ---- skies --------------------------------------------------------------

    fn skies_ui(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.importing, egui::Button::new("Upload sky…"))
                    .clicked()
                {
                    self.upload(Kind::Sky);
                }
                if ui.button("Open skies folder").clicked() {
                    crate::app::open_path(&cosmetics::skies_dir(&self.layout));
                }
                if ui.button("Rescan").clicked() {
                    self.custom_skies = cosmetics::list_custom(&self.layout, Kind::Sky);
                }
            });
            ui.label(
                RichText::new("PNG, 3:2 grid of square faces (north, east, south / west, up, down), 384–3072 px wide, max 16 MB. See documentation/SKIES.md.")
                    .small()
                    .color(MUTED),
            );
            ui.add_enabled_ui(self.settings.sky.enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Brightness");
                    ui.add(
                        egui::Slider::new(&mut self.settings.sky.brightness, 0.1..=1.0)
                            .step_by(0.05)
                            .fixed_decimals(2),
                    );
                    ui.add_space(12.0);
                    ui.checkbox(&mut self.settings.sky.rotate, "Slow rotation");
                });
            });
        });
        ui.add_space(10.0);

        let mut items: Vec<(String, String, bool)> =
            vec![("__vanilla".into(), "Vanilla sky (default)".into(), false)];
        items.extend(
            BUILTIN_SKIES
                .iter()
                .map(|(id, n)| (id.to_string(), n.to_string(), false)),
        );
        items.extend(
            self.custom_skies
                .iter()
                .map(|f| (format!("{CUSTOM_PREFIX}{f}"), f.clone(), true)),
        );
        let selected_missing = self.settings.sky.enabled
            && self.settings.sky.preset.starts_with(CUSTOM_PREFIX)
            && !items.iter().any(|i| i.0 == self.settings.sky.preset);
        if selected_missing {
            ui.label(RichText::new(format!(
                "The equipped sky ({}) is missing from the skies folder; the game will fall back to the vanilla sky. Choose another.",
                self.settings.sky.preset.trim_start_matches(CUSTOM_PREFIX)
            )).color(WARN));
        }

        let mut remove: Option<String> = None;
        let cols = columns(ui, SKY_CARD.x);
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                for (id, name, custom) in row {
                    let equipped = if id == "__vanilla" {
                        !self.settings.sky.enabled
                    } else {
                        self.settings.sky.enabled && self.settings.sky.preset == *id
                    };
                    let tex = if id == "__vanilla" {
                        None
                    } else {
                        self.texture(Kind::Sky, id)
                    };
                    let err = self
                        .preview_errors
                        .get(&preview_key(Kind::Sky, id))
                        .cloned();
                    let resp = item_card(ui, SKY_CARD, name, equipped, *custom, |ui, size| {
                        if id == "__vanilla" {
                            let (r, p) = ui.allocate_painter(size, Sense::hover());
                            p.rect_filled(
                                r.rect,
                                CornerRadius::same(4),
                                Color32::from_rgb(0x79, 0xA6, 0xFF),
                            );
                            p.rect_filled(
                                egui::Rect::from_min_max(
                                    Pos2::new(r.rect.min.x, r.rect.center().y + 10.0),
                                    r.rect.max,
                                ),
                                CornerRadius::same(4),
                                Color32::from_rgb(0x5C, 0x8A, 0x3C),
                            );
                        } else {
                            thumb(ui, tex.as_ref(), err.as_deref(), size);
                        }
                    });
                    if resp.clicked {
                        if id == "__vanilla" {
                            self.settings.sky.enabled = false;
                        } else {
                            self.settings.sky.enabled = true;
                            self.settings.sky.preset = id.clone();
                        }
                    }
                    if resp.remove {
                        remove = Some(id.clone());
                    }
                }
            });
            ui.add_space(10.0);
        }
        if let Some(id) = remove {
            self.remove(Kind::Sky, &id);
        }
    }

    // ---- capes --------------------------------------------------------------

    fn capes_ui(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.importing, egui::Button::new("Upload cape…"))
                    .clicked()
                {
                    self.upload(Kind::Cape);
                }
                if ui.button("Open capes folder").clicked() {
                    crate::app::open_path(&cosmetics::capes_dir(&self.layout));
                }
                if ui.button("Rescan").clicked() {
                    self.custom_capes = cosmetics::list_custom(&self.layout, Kind::Cape);
                }
                ui.add_space(12.0);
                ui.checkbox(&mut self.settings.cosmetics.capes_enabled, "Show cape");
                ui.checkbox(&mut self.settings.cosmetics.animated_capes, "Animate");
                ui.checkbox(
                    &mut self.settings.cosmetics.show_assigned_capes,
                    "Show players' assigned capes",
                );
            });
            ui.label(
                RichText::new("PNG (max 2 MB): Minecraft cape layout, 64x32 or HD multiples (128x64 …); animated PNGs stack frames vertically. \
                               GIF (max 4 MB): plays with its own timing; a 2:1 GIF is used as a cape sheet, any other GIF is fitted onto both sides of the cape. \
                               If you own an official Minecraft cape, the game shows that one instead.")
                    .small()
                    .color(MUTED),
            );
        });
        ui.add_space(10.0);

        let mut items: Vec<(String, String, bool)> =
            vec![(CAPE_NONE.into(), "No cape".into(), false)];
        let me = self.account_uuid.as_deref();
        items.extend(
            cosmetics::cape_catalog()
                .iter()
                .filter(|c| c.allows(me))
                .map(|c| (c.id.clone(), c.name.clone(), false)),
        );
        items.extend(
            self.custom_capes
                .iter()
                .map(|f| (format!("{CUSTOM_PREFIX}{f}"), f.clone(), true)),
        );
        let reserved = cosmetics::cape_catalog()
            .iter()
            .find(|c| c.id == self.settings.cosmetics.cape && !c.allows(me));
        if let Some(c) = reserved {
            ui.label(
                RichText::new(format!(
                    "The equipped cape ({}) is reserved for another account and will not render. \
                     Sign in with the account it belongs to, or choose another.",
                    c.name
                ))
                .color(WARN),
            );
        } else if !items.iter().any(|i| i.0 == self.settings.cosmetics.cape) {
            ui.label(RichText::new(format!(
                "The equipped cape ({}) is missing from the capes folder and will not render. Choose another.",
                self.settings.cosmetics.cape.trim_start_matches(CUSTOM_PREFIX)
            )).color(WARN));
        }

        let mut remove: Option<String> = None;
        let cols = columns(ui, CAPE_CARD.x);
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                for (id, name, custom) in row {
                    let equipped = if id == CAPE_NONE {
                        self.settings.cosmetics.cape == CAPE_NONE
                            || !self.settings.cosmetics.capes_enabled
                    } else {
                        self.settings.cosmetics.capes_enabled && self.settings.cosmetics.cape == *id
                    };
                    let tex = if id == CAPE_NONE {
                        None
                    } else {
                        self.texture(Kind::Cape, id)
                    };
                    let err = self
                        .preview_errors
                        .get(&preview_key(Kind::Cape, id))
                        .cloned();
                    let resp = item_card(ui, CAPE_CARD, name, equipped, *custom, |ui, size| {
                        if id == CAPE_NONE {
                            let (r, p) = ui.allocate_painter(size, Sense::hover());
                            p.text(
                                r.rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "—",
                                egui::FontId::proportional(28.0),
                                MUTED,
                            );
                        } else {
                            // Cape face is 10:16; centre it.
                            let h = size.y;
                            let w = h * 10.0 / 16.0;
                            ui.horizontal(|ui| {
                                ui.add_space((size.x - w) / 2.0);
                                thumb(ui, tex.as_ref(), err.as_deref(), Vec2::new(w, h));
                            });
                        }
                    });
                    if resp.clicked {
                        if id == CAPE_NONE {
                            self.settings.cosmetics.cape = CAPE_NONE.into();
                        } else {
                            self.settings.cosmetics.cape = id.clone();
                            self.settings.cosmetics.capes_enabled = true;
                        }
                    }
                    if resp.remove {
                        remove = Some(id.clone());
                    }
                }
            });
            ui.add_space(10.0);
        }
        if let Some(id) = remove {
            self.remove(Kind::Cape, &id);
        }
    }

    fn remove(&mut self, kind: Kind, id: &str) {
        match cosmetics::remove_custom(&self.layout, kind, id) {
            Ok(()) => {
                match kind {
                    Kind::Cape => {
                        self.custom_capes = cosmetics::list_custom(&self.layout, Kind::Cape);
                        if self.settings.cosmetics.cape == id {
                            self.settings.cosmetics.cape = CAPE_NONE.into();
                        }
                    }
                    Kind::Sky => {
                        self.custom_skies = cosmetics::list_custom(&self.layout, Kind::Sky);
                        if self.settings.sky.preset == id {
                            self.settings.sky.enabled = false;
                            self.settings.sky.preset = "dusk".into();
                        }
                    }
                }
                let key = preview_key(kind, id);
                self.textures.remove(&key);
                self.requested.remove(&key);
                self.message = Some((
                    true,
                    format!("Removed {}.", id.trim_start_matches(CUSTOM_PREFIX)),
                ));
            }
            Err(e) => self.message = Some((false, format!("Could not remove: {e}"))),
        }
    }
}

/// How many cards of `card_w` fit in the available width (at least one).
fn columns(ui: &egui::Ui, card_w: f32) -> usize {
    ((ui.available_width() + 10.0) / (card_w + 10.0))
        .floor()
        .max(1.0) as usize
}

fn preview_key(kind: Kind, id: &str) -> String {
    match kind {
        Kind::Cape => format!("cape:{id}"),
        Kind::Sky => format!("sky:{id}"),
    }
}

/// Decode and crop a preview (runs on a background thread).
pub(crate) fn load_preview(layout: &Layout, kind: Kind, id: &str) -> Result<Rgba, String> {
    let bytes: Vec<u8> = if id.starts_with(CUSTOM_PREFIX) {
        let p: PathBuf = cosmetics::custom_path(layout, kind, id).ok_or("invalid file name")?;
        std::fs::read(&p).map_err(|e| format!("cannot read: {e}"))?
    } else {
        let table = match kind {
            Kind::Cape => BUNDLED_CAPES,
            Kind::Sky => BUNDLED_SKIES,
        };
        table
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, b)| b.to_vec())
            .ok_or("unknown cosmetic")?
    };
    Ok(match kind {
        Kind::Cape => cosmetics::cape_preview_bytes(&bytes)?,
        Kind::Sky => cosmetics::sky_preview(&cosmetics::validate_image(kind, &bytes)?, 384),
    })
}

struct CardResponse {
    clicked: bool,
    remove: bool,
}

fn item_card(
    ui: &mut egui::Ui,
    size: Vec2,
    name: &str,
    equipped: bool,
    removable: bool,
    content: impl FnOnce(&mut egui::Ui, Vec2),
) -> CardResponse {
    let mut remove = false;
    let mut remove_rect: Option<egui::Rect> = None;
    let inner = egui::Frame::NONE
        .fill(if equipped {
            ACCENT.gamma_multiply(0.22)
        } else {
            CARD
        })
        .stroke(Stroke::new(
            if equipped { 2.0 } else { 1.0 },
            if equipped {
                ACCENT
            } else {
                Color32::from_gray(45)
            },
        ))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.set_width(size.x - 12.0);
            ui.vertical(|ui| {
                content(ui, Vec2::new(size.x - 12.0, size.y - 46.0));
                ui.horizontal(|ui| {
                    let label = if name.len() > 22 {
                        format!("{}…", &name[..name.floor_char_boundary(21)])
                    } else {
                        name.to_string()
                    };
                    ui.label(RichText::new(label).small().strong())
                        .on_hover_text(name);
                });
                ui.horizontal(|ui| {
                    if equipped {
                        ui.label(RichText::new("EQUIPPED").small().color(ACCENT));
                    } else {
                        ui.label(RichText::new("Click to equip").small().color(MUTED));
                    }
                    if removable {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let b = ui.small_button("Remove");
                            remove_rect = Some(b.rect);
                            if b.clicked() {
                                remove = true;
                            }
                        });
                    }
                });
            });
        });
    let resp = inner.response.interact(Sense::click());
    // The card-wide click sense is registered after the button and wins the
    // hit test, so route a click on the button's area to Remove.
    if resp.clicked()
        && let (Some(r), Some(p)) = (remove_rect, resp.interact_pointer_pos())
        && r.contains(p)
    {
        remove = true;
    }
    CardResponse {
        clicked: resp.clicked() && !remove,
        remove,
    }
}

fn thumb(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, err: Option<&str>, size: Vec2) {
    match (tex, err) {
        (Some(t), _) => {
            ui.add(
                egui::Image::new(t)
                    .fit_to_exact_size(size)
                    .corner_radius(CornerRadius::same(4)),
            );
        }
        (None, Some(e)) => {
            let (r, p) = ui.allocate_painter(size, Sense::hover());
            p.rect_filled(
                r.rect,
                CornerRadius::same(4),
                Color32::from_rgb(0x30, 0x18, 0x18),
            );
            p.text(
                r.rect.center(),
                egui::Align2::CENTER_CENTER,
                "invalid image",
                egui::FontId::proportional(11.0),
                ERR,
            );
            r.on_hover_text(e);
        }
        (None, None) => {
            let (r, p) = ui.allocate_painter(size, Sense::hover());
            p.rect_filled(r.rect, CornerRadius::same(4), Color32::from_gray(30));
            p.text(
                r.rect.center(),
                egui::Align2::CENTER_CENTER,
                "loading…",
                egui::FontId::proportional(11.0),
                MUTED,
            );
        }
    }
}

// ---- 2D effect previews ---------------------------------------------------------

/// Small deterministic PRNG so previews look identical every loop.
fn rand(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}

fn draw_effect_preview(
    p: &egui::Painter,
    rect: egui::Rect,
    effect: &str,
    t: f32,
    intensity: f32,
    argb: u32,
) {
    let ground = rect.max.y - 40.0;
    let cx = rect.center().x;
    p.line_segment(
        [
            Pos2::new(rect.min.x + 10.0, ground),
            Pos2::new(rect.max.x - 10.0, ground),
        ],
        Stroke::new(1.0, Color32::from_gray(60)),
    );
    // Victim silhouette, fading out as it "dies".
    let fade = (1.0 - t * 2.0).clamp(0.15, 1.0);
    let body = Color32::from_rgba_unmultiplied(170, 170, 180, (fade * 255.0) as u8);
    p.rect_filled(
        egui::Rect::from_min_size(Pos2::new(cx - 8.0, ground - 64.0), Vec2::new(16.0, 16.0)),
        CornerRadius::same(2),
        body,
    );
    p.rect_filled(
        egui::Rect::from_min_size(Pos2::new(cx - 8.0, ground - 48.0), Vec2::new(16.0, 48.0)),
        CornerRadius::same(2),
        body,
    );

    let user = Color32::from_rgb((argb >> 16) as u8, (argb >> 8) as u8, argb as u8);
    let n = (30.0 * intensity) as u32;
    let life = |age: f32, max: f32| (1.0 - age / max).clamp(0.0, 1.0);
    let dot = |x: f32, y: f32, r: f32, c: Color32, a: f32| {
        if a > 0.01 && rect.contains(Pos2::new(x, y)) {
            p.circle_filled(Pos2::new(x, y), r, c.gamma_multiply(a));
        }
    };
    match effect {
        "burst" => {
            for i in 0..n {
                let a = rand(i) * std::f32::consts::TAU;
                let v = 40.0 + rand(i + 99) * 90.0;
                let x = cx + a.cos() * v * t;
                let y = ground - 40.0 - a.sin().abs() * v * t + 60.0 * t * t;
                let c = if i % 3 == 0 {
                    Color32::from_rgb(120, 220, 255)
                } else {
                    Color32::from_rgb(255, 240, 200)
                };
                dot(x, y, 2.0, c, life(t, 1.0));
            }
        }
        "ring" => {
            for step in 0..10 {
                let st = step as f32 * 0.07;
                if t < st {
                    continue;
                }
                let r = 12.0 + step as f32 * 6.0;
                let y = ground - 6.0 - step as f32 * 7.0;
                for k in 0..(10.0 * intensity) as u32 {
                    let a =
                        k as f32 / (10.0 * intensity) * std::f32::consts::TAU + step as f32 * 0.3;
                    dot(
                        cx + a.cos() * r,
                        y + a.sin() * r * 0.3,
                        2.0,
                        Color32::from_rgb(190, 90, 255),
                        life(t - st, 1.0),
                    );
                }
            }
        }
        "helix" => {
            for k in 0..((60.0 * intensity) as u32) {
                let tt = k as f32 / (60.0 * intensity) * 1.0;
                if tt > t {
                    break;
                }
                for side in 0..2 {
                    let a = tt * 14.0 + side as f32 * std::f32::consts::PI;
                    let c = if side == 0 { user } else { Color32::WHITE };
                    dot(
                        cx + a.cos() * 22.0,
                        ground - tt * 110.0 + a.sin() * 4.0,
                        2.2,
                        c,
                        life(t - tt, 1.0),
                    );
                }
            }
        }
        "dust" => {
            for i in 0..(n * 2) {
                let u = rand(i) * 2.0 - 1.0;
                let a = rand(i + 500) * std::f32::consts::TAU;
                let r = (12.0 + t * 110.0).min(80.0);
                let s = (1.0 - u * u).sqrt();
                dot(
                    cx + s * a.cos() * r,
                    ground - 40.0 + u * r,
                    2.4,
                    user,
                    life(t, 1.2),
                );
            }
        }
        "explosion" => {
            for i in 0..(3.0 * intensity).max(1.0) as u32 {
                let r = 18.0 + t * 50.0;
                let ox = (rand(i) - 0.5) * 30.0;
                p.circle_filled(
                    Pos2::new(cx + ox, ground - 40.0),
                    r,
                    Color32::from_rgb(255, 200, 140).gamma_multiply(life(t, 0.6) * 0.8),
                );
            }
            for i in 0..n {
                let a = rand(i) * std::f32::consts::TAU;
                let y = ground - 40.0 - t * 60.0 * (0.5 + rand(i + 3));
                dot(
                    cx + a.cos() * 40.0 * t,
                    y,
                    5.0,
                    Color32::from_gray(120),
                    life(t, 1.6),
                );
            }
        }
        "lightning" => {
            if t < 1.2 {
                // Flicker like a real bolt.
                let a = life(t, 1.2) * if (t * 20.0) as i32 % 3 == 0 { 0.5 } else { 1.0 };
                let mut prev = Pos2::new(cx, rect.min.y + 8.0);
                let mut ox = 0.0;
                for s in 1..=12 {
                    ox = ox * 0.5 + (rand(s) - 0.5) * 30.0;
                    let y = rect.min.y + 8.0 + (ground - rect.min.y - 8.0) * s as f32 / 12.0;
                    let next = Pos2::new(cx + if s == 12 { 0.0 } else { ox }, y);
                    p.line_segment(
                        [prev, next],
                        Stroke::new(
                            6.0,
                            Color32::from_rgb(140, 170, 255).gamma_multiply(a * 0.35),
                        ),
                    );
                    p.line_segment(
                        [prev, next],
                        Stroke::new(2.0, Color32::from_rgb(220, 235, 255).gamma_multiply(a)),
                    );
                    prev = next;
                }
            }
        }
        "hearts" => {
            for i in 0..(n / 2).max(3) {
                let st = rand(i) * 0.8;
                if t < st {
                    continue;
                }
                let age = t - st;
                let x = cx + (rand(i + 7) - 0.5) * 60.0;
                let y = ground - 60.0 - age * 50.0;
                heart(
                    p,
                    Pos2::new(x, y),
                    6.0,
                    Color32::from_rgb(255, 80, 100).gamma_multiply(life(age, 1.0)),
                );
            }
        }
        "flames" => {
            for i in 0..((40.0 * intensity) as u32) {
                let tt = i as f32 / (40.0 * intensity);
                if tt > t {
                    break;
                }
                let a = tt * 18.0;
                let r = 20.0 * (1.0 - tt * 0.6);
                let c = Color32::from_rgb(255, (140.0 + 100.0 * rand(i)) as u8, 40);
                dot(
                    cx + a.cos() * r,
                    ground - tt * 120.0 - (t - tt) * 15.0,
                    3.0,
                    c,
                    life(t - tt, 0.6),
                );
            }
        }
        "arcane" => {
            for i in 0..n {
                let a = rand(i) * std::f32::consts::TAU;
                let start = 90.0 + rand(i + 1) * 30.0;
                let r = start * (1.0 - (t * 1.4).min(1.0));
                let x = cx + a.cos() * r;
                let y = ground - 40.0 + a.sin() * r * 0.7;
                // Default fonts have no enchanting-table glyphs; use runic-looking ASCII.
                p.text(
                    Pos2::new(x, y),
                    egui::Align2::CENTER_CENTER,
                    ["Y", "K", "T", "X", "H", "V"][(i % 6) as usize],
                    egui::FontId::monospace(10.0),
                    Color32::from_rgb(200, 180, 255).gamma_multiply(life(t, 1.4)),
                );
            }
        }
        _ => {}
    }
}

fn heart(p: &egui::Painter, c: Pos2, r: f32, col: Color32) {
    if col.a() == 0 {
        return;
    }
    p.circle_filled(Pos2::new(c.x - r * 0.5, c.y - r * 0.2), r * 0.55, col);
    p.circle_filled(Pos2::new(c.x + r * 0.5, c.y - r * 0.2), r * 0.55, col);
    p.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(c.x - r, c.y),
            Pos2::new(c.x + r, c.y),
            Pos2::new(c.x, c.y + r),
        ],
        col,
        Stroke::NONE,
    ));
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    /// Every catalog cape needs an embedded preview (and nothing stale is embedded).
    #[test]
    fn every_catalog_cape_has_a_preview() {
        let ids: Vec<&str> = cosmetics::cape_catalog()
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        let embedded: Vec<&str> = BUNDLED_CAPES.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, embedded);
        // Bundled previews never touch the data folder.
        let layout = Layout::new(PathBuf::from("unused-root"));
        for id in ids {
            let p = load_preview(&layout, Kind::Cape, id).unwrap();
            assert_eq!(
                p.width * 16,
                p.height * 10,
                "{id}: preview is one 10:16 face"
            );
        }
    }
}
