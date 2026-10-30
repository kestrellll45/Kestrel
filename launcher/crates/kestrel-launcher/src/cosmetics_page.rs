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
    self, CAPE_CATEGORIES, CAPE_NONE, CUSTOM_PREFIX, CosmeticSettings, KILL_EFFECT_CATEGORIES,
    KILL_EFFECTS, Kind, MOJANG_CATEGORY, MOJANG_PREFIX, Rgba, SKY_CATEGORIES,
};
use kestrel_core::paths::Layout;

use crate::app::{ACCENT, CARD, ERR, MUTED, OK, WARN, card, heading};

use crate::bundled_previews::{CAPE_THUMBS, SKY_THUMBS};

/// Frame rate for animated catalog capes whose catalog entry sets none.
const DEFAULT_CAPE_FPS: u32 = 8;

const CAPE_CARD: Vec2 = Vec2::new(112.0, 150.0);
const SKY_CARD: Vec2 = Vec2::new(228.0, 120.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    KillEffects,
    Skies,
    Capes,
}

/// Category chip + search box state of one grid.
#[derive(Default)]
struct Filter {
    /// "" = all categories.
    category: String,
    search: String,
}

impl Filter {
    fn matches(&self, name: &str, category: &str) -> bool {
        (self.category.is_empty() || self.category == category)
            && (self.search.trim().is_empty()
                || name
                    .to_lowercase()
                    .contains(&self.search.trim().to_lowercase()))
    }
}

/// One selectable cape or sky.
struct Item {
    id: String,
    name: String,
    category: String,
    badge: Option<&'static str>,
    custom: bool,
    /// Animated catalog cape: frames per second of its thumbnail strip.
    fps: Option<u32>,
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
    cape_filter: Filter,
    sky_filter: Filter,
    effect_filter: Filter,
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
            cape_filter: Filter::default(),
            sky_filter: Filter::default(),
            effect_filter: Filter::default(),
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

    /// Forget failed previews so they are tried again (e.g. after being offline).
    fn retry_failed_previews(&mut self) {
        for key in std::mem::take(&mut self.preview_errors).into_keys() {
            self.requested.remove(&key);
        }
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
                        RichText::new("(effects marked COLOUR)")
                            .small()
                            .color(MUTED),
                    );
                });
            });
        });
        ui.add_space(10.0);

        let shown_n = KILL_EFFECTS
            .iter()
            .filter(|e| self.effect_filter.matches(e.name, e.category))
            .count();
        filter_bar(
            ui,
            "effects",
            &mut self.effect_filter,
            KILL_EFFECT_CATEGORIES,
            shown_n,
            KILL_EFFECTS.len(),
        );
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            // Effect list.
            ui.vertical(|ui| {
                ui.set_width(330.0);
                if shown_n == 0 {
                    ui.label(RichText::new("Nothing matches.").color(MUTED));
                }
                for e in KILL_EFFECTS
                    .iter()
                    .filter(|e| self.effect_filter.matches(e.name, e.category))
                {
                    let equipped = self.settings.kill_effects.effect == e.id;
                    let resp = egui::Frame::NONE
                        .fill(if equipped { ACCENT.gamma_multiply(0.25) } else { CARD })
                        .stroke(if equipped { Stroke::new(1.0, ACCENT) } else { Stroke::NONE })
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_width(314.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(e.name).strong());
                                ui.label(
                                    RichText::new(category_label(KILL_EFFECT_CATEGORIES, e.category))
                                        .small()
                                        .color(MUTED),
                                );
                                if e.uses_color {
                                    ui.label(RichText::new("COLOUR").small().color(WARN));
                                }
                                if equipped {
                                    ui.label(RichText::new("EQUIPPED").small().color(ACCENT));
                                }
                            });
                            ui.label(RichText::new(e.desc).small().color(MUTED));
                        })
                        .response
                        .interact(Sense::click());
                    if resp.hovered() && self.preview_effect.as_deref() != Some(e.id) {
                        self.preview_effect = Some(e.id.to_string());
                        self.preview_started = time;
                    }
                    if resp.clicked() {
                        self.settings.kill_effects.effect = e.id.to_string();
                        self.preview_effect = Some(e.id.to_string());
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
                let name = KILL_EFFECTS
                    .iter()
                    .find(|e| e.id == shown)
                    .map(|e| e.name)
                    .unwrap_or("");
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

        let mut items = vec![Item {
            id: "__vanilla".into(),
            name: "Vanilla sky (default)".into(),
            category: String::new(),
            badge: None,
            custom: false,
            fps: None,
        }];
        items.extend(cosmetics::sky_catalog().iter().map(|e| Item {
            id: e.id.clone(),
            name: e.name.clone(),
            category: e.category.clone(),
            badge: None,
            custom: false,
            fps: None,
        }));
        items.extend(self.custom_skies.iter().map(|f| Item {
            id: format!("{CUSTOM_PREFIX}{f}"),
            name: f.clone(),
            category: "custom".into(),
            badge: Some("CUSTOM"),
            custom: true,
            fps: None,
        }));
        let selected_missing = self.settings.sky.enabled
            && self.settings.sky.preset.starts_with(CUSTOM_PREFIX)
            && !items.iter().any(|i| i.id == self.settings.sky.preset);
        if selected_missing {
            ui.label(RichText::new(format!(
                "The equipped sky ({}) is missing from the skies folder; the game will fall back to the vanilla sky. Choose another.",
                self.settings.sky.preset.trim_start_matches(CUSTOM_PREFIX)
            )).color(WARN));
        }
        let total = items.len();
        // The vanilla sky stays visible under every category.
        items.retain(|i| i.id == "__vanilla" || self.sky_filter.matches(&i.name, &i.category));
        filter_bar(
            ui,
            "skies",
            &mut self.sky_filter,
            &with_custom(SKY_CATEGORIES),
            items.len(),
            total,
        );
        ui.add_space(6.0);

        let mut remove: Option<String> = None;
        let cols = columns(ui, SKY_CARD.x);
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                for Item {
                    id,
                    name,
                    custom,
                    badge,
                    ..
                } in row
                {
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
                    let resp =
                        item_card(ui, SKY_CARD, name, *badge, equipped, *custom, |ui, size| {
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
                                thumb(ui, tex.as_ref(), err.as_deref(), size, None);
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
                    self.retry_failed_previews();
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

        let mut items = vec![Item {
            id: CAPE_NONE.into(),
            name: "No cape".into(),
            category: String::new(),
            badge: None,
            custom: false,
            fps: None,
        }];
        let me = self.account_uuid.as_deref();
        items.extend(
            cosmetics::cape_catalog()
                .iter()
                .filter(|c| c.allows(me))
                .map(|c| Item {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    category: c.category.clone(),
                    badge: c.animated().then_some("ANIMATED"),
                    custom: false,
                    fps: c.animated().then(|| c.fps.unwrap_or(DEFAULT_CAPE_FPS)),
                }),
        );
        items.extend(cosmetics::mojang_capes().iter().map(|m| Item {
            id: m.selection_id(),
            name: m.name.clone(),
            category: MOJANG_CATEGORY.0.into(),
            badge: None,
            custom: false,
            fps: None,
        }));
        items.extend(self.custom_capes.iter().map(|f| Item {
            id: format!("{CUSTOM_PREFIX}{f}"),
            name: f.clone(),
            category: "custom".into(),
            badge: Some("CUSTOM"),
            custom: true,
            fps: None,
        }));
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
        } else if !items.iter().any(|i| i.id == self.settings.cosmetics.cape) {
            ui.label(RichText::new(format!(
                "The equipped cape ({}) is missing from the capes folder and will not render. Choose another.",
                self.settings.cosmetics.cape.trim_start_matches(CUSTOM_PREFIX)
            )).color(WARN));
        }

        let total = items.len();
        items.retain(|i| i.id == CAPE_NONE || self.cape_filter.matches(&i.name, &i.category));
        filter_bar(
            ui,
            "capes",
            &mut self.cape_filter,
            &with_custom(&[CAPE_CATEGORIES, &[MOJANG_CATEGORY]].concat()),
            items.len(),
            total,
        );
        if self.cape_filter.category == MOJANG_CATEGORY.0 {
            ui.label(
                RichText::new(
                    "Historical Mojang capes, as local Kestrel cosmetics: only your Kestrel client shows them. \
                     Equipping one does not give your account that cape, and other players don't see it. \
                     The original textures are downloaded from Mojang's texture server the first time and kept on your PC.",
                )
                .small()
                .color(MUTED),
            );
        }
        ui.add_space(6.0);
        let animate = self.settings.cosmetics.animated_capes;
        let time = ui.input(|i| i.time);
        if animate && items.iter().any(|i| i.fps.is_some()) {
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }

        let mut remove: Option<String> = None;
        let cols = columns(ui, CAPE_CARD.x);
        for row in items.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                for Item {
                    id,
                    name,
                    custom,
                    badge,
                    fps,
                    ..
                } in row
                {
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
                    let fps = fps.filter(|_| animate);
                    let resp = item_card(
                        ui,
                        CAPE_CARD,
                        name,
                        *badge,
                        equipped,
                        *custom,
                        |ui, size| {
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
                                    let uv =
                                        tex.as_ref().map(|t| cape_frame_uv(t.size(), fps, time));
                                    thumb(ui, tex.as_ref(), err.as_deref(), Vec2::new(w, h), uv);
                                });
                            }
                        },
                    );
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
/// Each card is drawn 4 px wider than `card_w` (frame stroke on both sides).
fn columns(ui: &egui::Ui, card_w: f32) -> usize {
    ((ui.available_width() + 10.0) / (card_w + 14.0))
        .floor()
        .max(1.0) as usize
}

fn preview_key(kind: Kind, id: &str) -> String {
    match kind {
        Kind::Cape => format!("cape:{id}"),
        Kind::Sky => format!("sky:{id}"),
    }
}

/// Decode a preview (runs on a background thread). Catalog cosmetics use their
/// embedded thumbnails (animated capes: frames side by side); custom files are
/// read from the data folder and cropped like the game does.
pub(crate) fn load_preview(layout: &Layout, kind: Kind, id: &str) -> Result<Rgba, String> {
    if kind == Kind::Cape && id.starts_with(MOJANG_PREFIX) {
        let cape = cosmetics::mojang_cape(id).ok_or("unknown cape")?;
        let path = cosmetics::fetch_mojang_cape(layout, cape, &kestrel_core::skins::http_fetch)?;
        let bytes = std::fs::read(&path).map_err(|e| format!("cannot read: {e}"))?;
        return cosmetics::cape_preview_bytes(&bytes);
    }
    if id.starts_with(CUSTOM_PREFIX) {
        let p: PathBuf = cosmetics::custom_path(layout, kind, id).ok_or("invalid file name")?;
        let bytes = std::fs::read(&p).map_err(|e| format!("cannot read: {e}"))?;
        return match kind {
            Kind::Cape => cosmetics::cape_preview_bytes(&bytes),
            Kind::Sky => Ok(cosmetics::sky_preview(
                &cosmetics::validate_image(kind, &bytes)?,
                384,
            )),
        };
    }
    let table = match kind {
        Kind::Cape => CAPE_THUMBS,
        Kind::Sky => SKY_THUMBS,
    };
    let bytes = table
        .iter()
        .find(|(n, _)| *n == id)
        .map(|(_, b)| *b)
        .ok_or("unknown cosmetic")?;
    cosmetics::decode_png(bytes)
}

/// Number of 10:16 cape faces side by side in a thumbnail of this size.
fn cape_frames(size: [usize; 2]) -> usize {
    let face_w = (size[1] * 10 / 16).max(1);
    (size[0] / face_w).max(1)
}

/// UV rectangle of the frame to show now (frame 0 when not animating).
fn cape_frame_uv(size: [usize; 2], fps: Option<u32>, time: f64) -> egui::Rect {
    let frames = cape_frames(size);
    let frame = match fps {
        Some(fps) if frames > 1 => (time * fps as f64) as usize % frames,
        _ => 0,
    };
    let w = 1.0 / frames as f32;
    egui::Rect::from_min_max(
        Pos2::new(frame as f32 * w, 0.0),
        Pos2::new((frame + 1) as f32 * w, 1.0),
    )
}

/// Categories plus the "custom" uploads category.
fn with_custom(cats: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut v = cats.to_vec();
    v.push(("custom", "Custom"));
    v
}

fn category_label<'a>(cats: &[(&str, &'a str)], id: &str) -> &'a str {
    cats.iter().find(|c| c.0 == id).map(|c| c.1).unwrap_or("")
}

/// Category chips and a search box above a grid.
fn filter_bar(
    ui: &mut egui::Ui,
    id: &str,
    filter: &mut Filter,
    cats: &[(&str, &str)],
    shown: usize,
    total: usize,
) {
    ui.horizontal_wrapped(|ui| {
        let chip = |ui: &mut egui::Ui, on: bool, label: &str| {
            ui.add(
                egui::Button::new(RichText::new(label).color(if on {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                }))
                .fill(if on { ACCENT } else { CARD })
                .corner_radius(CornerRadius::same(10)),
            )
            .clicked()
        };
        if chip(ui, filter.category.is_empty(), "All") {
            filter.category.clear();
        }
        for (cid, label) in cats {
            if chip(ui, filter.category == *cid, label) {
                filter.category = cid.to_string();
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut filter.search)
                .id_salt(("cosmetics-search", id))
                .hint_text(format!("Search {total} {id}…"))
                .desired_width(260.0),
        );
        if !filter.search.is_empty() && ui.small_button("Clear").clicked() {
            filter.search.clear();
        }
        ui.label(
            RichText::new(format!("{shown} of {total}"))
                .small()
                .color(MUTED),
        );
    });
}

struct CardResponse {
    clicked: bool,
    remove: bool,
}

fn item_card(
    ui: &mut egui::Ui,
    size: Vec2,
    name: &str,
    badge: Option<&str>,
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
                let pic = Vec2::new(size.x - 12.0, size.y - 46.0);
                let top = ui.cursor().min;
                content(ui, pic);
                if let Some(b) = badge {
                    let font = egui::FontId::proportional(10.0);
                    let galley = ui.painter().layout_no_wrap(b.to_string(), font, ACCENT);
                    let r = egui::Rect::from_min_size(
                        Pos2::new(top.x + pic.x - galley.size().x - 8.0, top.y + 2.0),
                        galley.size() + Vec2::new(6.0, 2.0),
                    );
                    ui.painter().rect_filled(
                        r,
                        CornerRadius::same(3),
                        Color32::from_black_alpha(190),
                    );
                    ui.painter()
                        .galley(r.min + Vec2::new(3.0, 1.0), galley, ACCENT);
                }
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

fn thumb(
    ui: &mut egui::Ui,
    tex: Option<&egui::TextureHandle>,
    err: Option<&str>,
    size: Vec2,
    uv: Option<egui::Rect>,
) {
    match (tex, err) {
        (Some(t), _) => {
            // The size is exact: a cape strip's frame is shown through `uv`, so
            // the texture's own aspect ratio must not shrink it.
            let mut img = egui::Image::new(t)
                .fit_to_exact_size(size)
                .maintain_aspect_ratio(false)
                .corner_radius(CornerRadius::same(4));
            if let Some(uv) = uv {
                img = img.uv(uv);
            }
            ui.add(img);
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
                "unavailable",
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
        "smoke" => {
            for i in 0..(n / 2).max(4) {
                let a = (rand(i) - 0.5) * 2.0;
                let r = 8.0 + t * 40.0 * (0.6 + rand(i + 9));
                p.circle_filled(
                    Pos2::new(cx + a * t * 90.0, ground - 10.0 - rand(i + 4) * 30.0 * t),
                    r,
                    Color32::from_gray(110).gamma_multiply(life(t, 2.0) * 0.5),
                );
            }
        }
        "pixel" => {
            for i in 0..n {
                let vx = (rand(i) - 0.5) * 160.0;
                let vy = 60.0 + rand(i + 3) * 90.0;
                let mut y = ground - 40.0 - vy * t + 150.0 * t * t;
                if y > ground - 3.0 {
                    y = ground - 3.0; // lands and stays on the ground
                }
                let c = [
                    Color32::from_rgb(255, 80, 80),
                    Color32::from_rgb(80, 255, 120),
                    Color32::from_rgb(90, 140, 255),
                    Color32::from_rgb(255, 220, 60),
                ][(i % 4) as usize];
                let q = egui::Rect::from_center_size(Pos2::new(cx + vx * t, y), Vec2::splat(5.0));
                if rect.contains_rect(q) {
                    p.rect_filled(q, CornerRadius::ZERO, c.gamma_multiply(life(t, 1.8)));
                }
            }
        }
        "shock" => {
            if (t * 14.0) as i32 % 2 == 0 && t < 1.5 {
                for arc in 0..(4.0 * intensity).max(2.0) as u32 {
                    let seed = arc * 17 + (t * 14.0) as u32;
                    let mut prev = Pos2::new(cx + (rand(seed) - 0.5) * 20.0, ground - 64.0);
                    for s in 1..=6 {
                        let next = Pos2::new(
                            cx + (rand(seed + s) - 0.5) * 34.0,
                            ground - 64.0 + s as f32 * 10.0,
                        );
                        p.line_segment(
                            [prev, next],
                            Stroke::new(
                                1.5,
                                Color32::from_rgb(140, 220, 255).gamma_multiply(life(t, 1.5)),
                            ),
                        );
                        prev = next;
                    }
                }
            }
        }
        "ice" => {
            for i in 0..(n / 2).max(6) {
                let a = rand(i) * std::f32::consts::TAU;
                let d = t * (60.0 + rand(i + 5) * 50.0);
                let c = Pos2::new(
                    cx + a.cos() * d,
                    ground - 36.0 + a.sin() * d * 0.6 + 40.0 * t * t,
                );
                let s = 6.0;
                p.add(egui::Shape::convex_polygon(
                    vec![
                        c + Vec2::new(a.cos(), a.sin()) * s,
                        c + Vec2::new(-a.sin(), a.cos()) * s * 0.35,
                        c + Vec2::new(a.sin(), -a.cos()) * s * 0.35,
                    ],
                    Color32::from_rgb(180, 240, 255).gamma_multiply(life(t, 1.4)),
                    Stroke::NONE,
                ));
            }
            for i in 0..n {
                let x = cx + (rand(i + 50) - 0.5) * 120.0;
                let y = ground - 120.0 + ((t * 40.0 + rand(i + 60) * 120.0) % 120.0);
                dot(x, y, 1.5, Color32::WHITE, life(t, 2.0));
            }
        }
        "nether" => {
            for i in 0..n {
                let st = rand(i) * 0.6;
                if t < st {
                    continue;
                }
                let age = t - st;
                let x = cx + (rand(i + 1) - 0.5) * 50.0 + (age * 9.0 + i as f32).sin() * 4.0;
                let y = ground - age * 90.0;
                let c = if i % 3 == 0 {
                    Color32::from_gray(50)
                } else {
                    Color32::from_rgb(255, (60.0 + 80.0 * rand(i + 2)) as u8, 20)
                };
                dot(x, y, 3.0, c, life(age, 1.2));
            }
        }
        "tornado" => {
            for i in 0..((50.0 * intensity) as u32) {
                let h = rand(i);
                let a = t * 9.0 + h * 20.0;
                let r = 6.0 + h * 40.0;
                dot(
                    cx + a.cos() * r,
                    ground - h * 130.0 + a.sin() * 3.0,
                    2.0,
                    Color32::from_gray(200),
                    life(t, 2.0),
                );
            }
        }
        "soul" => {
            for i in 0..(n / 2).max(5) {
                let st = rand(i) * 0.7;
                if t < st {
                    continue;
                }
                let age = t - st;
                let x = cx + (rand(i + 3) - 0.5) * 30.0 + (age * 5.0 + i as f32).sin() * 10.0;
                let y = ground - 50.0 - age * 55.0;
                dot(
                    x,
                    y,
                    4.0,
                    Color32::from_rgb(120, 240, 230),
                    life(age, 1.5) * 0.4,
                );
                dot(x, y, 2.0, Color32::from_rgb(220, 255, 250), life(age, 1.5));
            }
        }
        "ender" => {
            for i in 0..n {
                let a = rand(i) * std::f32::consts::TAU + t * 3.0;
                let r = 50.0 * (1.0 - (t * 1.2).min(1.0)) + 4.0;
                dot(
                    cx + a.cos() * r,
                    ground - 36.0 + a.sin() * r,
                    2.0,
                    Color32::from_rgb(190, 90, 255),
                    life(t, 1.4),
                );
            }
        }
        "portal" => {
            let a = life(t, 2.0);
            for k in 0..3 {
                let r = 30.0 + k as f32 * 6.0;
                p.add(egui::Shape::ellipse_stroke(
                    Pos2::new(cx, ground - 40.0),
                    Vec2::new(r * 0.6, r),
                    Stroke::new(
                        2.0,
                        Color32::from_rgb(140, 60, 255).gamma_multiply(a * (1.0 - k as f32 * 0.3)),
                    ),
                ));
            }
            for i in 0..n {
                let ang = rand(i) * std::f32::consts::TAU;
                let r = 70.0 * (1.0 - ((t + rand(i + 2)) % 1.0));
                dot(
                    cx + ang.cos() * r,
                    ground - 40.0 + ang.sin() * r,
                    1.8,
                    Color32::from_rgb(200, 140, 255),
                    a,
                );
            }
        }
        "dragon" => {
            for i in 0..n {
                let a = (rand(i) - 0.5) * 1.6;
                let d = t * (50.0 + rand(i + 1) * 60.0);
                p.circle_filled(
                    Pos2::new(cx + a.sin() * d, ground - 30.0 - a.cos().abs() * d * 0.5),
                    4.0 + t * 8.0,
                    Color32::from_rgb(220, 100, 255).gamma_multiply(life(t, 1.8) * 0.35),
                );
            }
        }
        "energy" => {
            for k in 0..3 {
                let tt = t - k as f32 * 0.2;
                if tt < 0.0 {
                    continue;
                }
                p.circle_stroke(
                    Pos2::new(cx, ground - 40.0),
                    10.0 + tt * 90.0,
                    Stroke::new(3.0, user.gamma_multiply(life(tt, 1.0))),
                );
            }
            p.circle_filled(
                Pos2::new(cx, ground - 40.0),
                10.0,
                Color32::WHITE.gamma_multiply(life(t, 0.6)),
            );
        }
        "fireworks" => {
            let burst_y = rect.min.y + 60.0;
            if t < 0.5 {
                dot(
                    cx,
                    ground - (ground - burst_y) * t / 0.5,
                    2.5,
                    Color32::from_rgb(255, 230, 180),
                    1.0,
                );
            } else {
                let tt = t - 0.5;
                for i in 0..n * 2 {
                    let a = rand(i) * std::f32::consts::TAU;
                    let v = 50.0 + rand(i + 4) * 30.0;
                    let c = if i % 2 == 0 { user } else { Color32::WHITE };
                    dot(
                        cx + a.cos() * v * tt,
                        burst_y + a.sin() * v * tt + 30.0 * tt * tt,
                        2.0,
                        c,
                        life(tt, 1.4),
                    );
                }
            }
        }
        "confetti" => {
            for i in 0..n * 2 {
                let x = cx + (rand(i) - 0.5) * 200.0 + (t * 4.0 + i as f32).sin() * 8.0;
                let y = rect.min.y + t * 110.0 * (0.6 + rand(i + 2)) - rand(i + 3) * 30.0;
                let c = Color32::from_rgb(
                    (rand(i + 7) * 255.0) as u8,
                    (rand(i + 8) * 255.0) as u8,
                    (rand(i + 9) * 255.0) as u8,
                );
                let q = egui::Rect::from_center_size(Pos2::new(x, y), Vec2::new(4.0, 2.0));
                if rect.contains_rect(q) {
                    p.rect_filled(q, CornerRadius::ZERO, c.gamma_multiply(life(t, 2.2)));
                }
            }
        }
        "rainbow" => {
            for k in 0..6 {
                let tt = t - k as f32 * 0.08;
                if tt < 0.0 {
                    continue;
                }
                let c = egui::ecolor::Hsva::new(k as f32 / 6.0, 0.9, 1.0, 1.0);
                p.circle_stroke(
                    Pos2::new(cx, ground - 30.0),
                    12.0 + tt * 70.0,
                    Stroke::new(2.5, Color32::from(c).gamma_multiply(life(tt, 1.3))),
                );
            }
        }
        "starfall" => {
            for i in 0..(n / 2).max(5) {
                let st = rand(i) * 0.8;
                if t < st {
                    continue;
                }
                let age = t - st;
                let tx = cx + (rand(i + 1) - 0.5) * 80.0;
                let x = tx - 60.0 + (age / 0.5).min(1.0) * 60.0;
                let y = rect.min.y + 10.0 + (age / 0.5).min(1.0) * (ground - rect.min.y - 14.0);
                let c = Color32::from_rgb(255, 245, 180);
                p.line_segment(
                    [Pos2::new(x - 12.0, y - 12.0), Pos2::new(x, y)],
                    Stroke::new(1.5, c.gamma_multiply(life(age, 1.0) * 0.6)),
                );
                dot(x, y, 2.5, c, life(age, 1.0));
            }
        }
        "kestrel" => {
            // A falcon silhouette swooping over the victim, shedding feathers.
            let a = life(t, 1.8);
            let x = rect.min.x + (rect.width()) * (t / 1.2).min(1.0);
            let y = ground - 90.0 + (t * 3.0).sin() * 10.0;
            let orange = Color32::from_rgb(255, 140, 40);
            let flap = (t * 12.0).sin() * 8.0;
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x - 22.0, y - flap),
                    Pos2::new(x, y + 4.0),
                    Pos2::new(x - 4.0, y - 2.0),
                ],
                orange.gamma_multiply(a),
                Stroke::NONE,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x + 22.0, y - flap),
                    Pos2::new(x + 4.0, y - 2.0),
                    Pos2::new(x, y + 4.0),
                ],
                orange.gamma_multiply(a),
                Stroke::NONE,
            ));
            for i in 0..(n / 2).max(4) {
                let age = t - rand(i) * 0.8;
                if age < 0.0 {
                    continue;
                }
                dot(
                    cx + (rand(i + 1) - 0.5) * 70.0 + (age * 6.0).sin() * 6.0,
                    ground - 80.0 + age * 45.0,
                    2.2,
                    orange,
                    life(age, 1.4),
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

    /// Every catalog cape and sky has exactly one embedded thumbnail, in
    /// catalog order (regenerate bundled_previews.rs after editing a catalog).
    #[test]
    fn every_catalog_entry_has_a_thumbnail() {
        let capes: Vec<&str> = cosmetics::cape_catalog()
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        let embedded: Vec<&str> = CAPE_THUMBS.iter().map(|(id, _)| *id).collect();
        assert_eq!(capes, embedded);
        let skies: Vec<&str> = cosmetics::sky_catalog()
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        let embedded: Vec<&str> = SKY_THUMBS.iter().map(|(id, _)| *id).collect();
        assert_eq!(skies, embedded);
    }

    /// Thumbnails decode without touching the data folder and have the shape
    /// the grids draw: cape faces 10:16 (animated: one face per frame), skies 3:1.
    #[test]
    fn thumbnails_decode_with_the_expected_shape() {
        let layout = Layout::new(PathBuf::from("unused-root"));
        for c in cosmetics::cape_catalog() {
            let p = load_preview(&layout, Kind::Cape, &c.id).unwrap();
            let face_w = p.height * 10 / 16;
            assert_eq!(p.width % face_w, 0, "{}: strip of whole 10:16 faces", c.id);
            let frames = cape_frames([p.width as usize, p.height as usize]);
            if c.fps.is_some() {
                assert!(frames > 1, "{}: animated cape needs several frames", c.id);
            }
        }
        for s in cosmetics::sky_catalog() {
            let p = load_preview(&layout, Kind::Sky, &s.id).unwrap();
            assert_eq!(p.width, p.height * 3, "{}: north/east/south strip", s.id);
        }
    }

    #[test]
    fn frame_uv_steps_through_the_strip() {
        let size = [80, 16]; // 8 faces of 10x16
        assert_eq!(cape_frames(size), 8);
        let still = cape_frame_uv(size, None, 3.7);
        assert_eq!(still.min.x, 0.0);
        let f = cape_frame_uv(size, Some(8), 0.5); // frame 4
        assert!((f.min.x - 0.5).abs() < 1e-6 && (f.max.x - 0.625).abs() < 1e-6);
    }

    #[test]
    fn filter_matches_category_and_search() {
        let mut f = Filter::default();
        assert!(f.matches("Galaxy Core", "space"));
        f.category = "space".into();
        assert!(!f.matches("Golden Hour", "sunsets"));
        f.search = "  CORE ".into();
        assert!(f.matches("Galaxy Core", "space"));
        assert!(!f.matches("Deep Space", "space"));
    }
}
