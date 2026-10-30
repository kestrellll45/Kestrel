//! The launcher's Skin page: a local, client-side skin changer.
//!
//! State is shared with the client mod through `skins/skin.json` (see
//! `kestrel_core::skins`), so a skin applied here shows up in a running game
//! within about a second, and one applied from the in-game Skin screen shows
//! up here. File and network work runs on worker threads.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime};

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Vec2};
use kestrel_core::cosmetics::Rgba;
use kestrel_core::paths::Layout;
use kestrel_core::skins::{self, MODEL_CLASSIC, MODEL_SLIM, SkinSource, SkinState};

use crate::app::{ACCENT, CARD, ERR, MUTED, OK, WARN, card, heading};

/// Preview texture key for the account skin.
const ACCOUNT: &str = "account";
const THUMB: Vec2 = Vec2::new(96.0, 128.0);

enum Event {
    /// A decoded skin (raw 64x64/64x32 RGBA) for a key.
    Decoded {
        key: String,
        image: Result<(Rgba, Option<&'static str>), String>,
    },
    /// An import finished; on success the new state and the imported file.
    Imported(Result<(SkinState, String), String>),
}

pub struct SkinPage {
    layout: Layout,
    state: SkinState,
    /// File of the skin shown in the big preview (`None` = account skin).
    selected: Option<String>,
    url: String,
    busy: bool,
    message: Option<(bool, String)>,
    /// Decoded skin textures (64x64 or legacy 64x32 RGBA) by key.
    images: HashMap<String, Rgba>,
    textures: HashMap<(String, bool), egui::TextureHandle>,
    errors: HashMap<String, String>,
    requested: HashSet<String>,
    account_uuid: Option<String>,
    account_model: Option<&'static str>,
    state_mtime: Option<SystemTime>,
    last_poll: f64,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    loaded: bool,
}

impl SkinPage {
    pub fn new(layout: Layout) -> Self {
        let (tx, rx) = channel();
        SkinPage {
            layout,
            state: SkinState::default(),
            selected: None,
            url: String::new(),
            busy: false,
            message: None,
            images: HashMap::new(),
            textures: HashMap::new(),
            errors: HashMap::new(),
            requested: HashSet::new(),
            account_uuid: None,
            account_model: None,
            state_mtime: None,
            last_poll: 0.0,
            tx,
            rx,
            loaded: false,
        }
    }

    /// The signed-in Microsoft account (its real skin is shown as the
    /// "account skin"); `None` offline or signed out.
    pub fn set_account(&mut self, uuid: Option<&str>) {
        let uuid = uuid.and_then(skins::normalize_uuid);
        if uuid != self.account_uuid {
            self.account_uuid = uuid;
            self.account_model = None;
            self.forget(ACCOUNT);
        }
    }

    /// Re-read `skin.json` (called whenever the page is opened).
    pub fn refresh(&mut self) {
        self.state = skins::load_state(&self.layout);
        self.state_mtime = self.mtime();
        let sel_ok = self
            .selected
            .as_ref()
            .is_none_or(|f| self.state.skins.iter().any(|s| &s.file == f));
        if !self.loaded || !sel_ok {
            self.selected = self.state.active.clone();
        }
        self.loaded = true;
    }

    fn mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(skins::state_path(&self.layout))
            .and_then(|m| m.modified())
            .ok()
    }

    fn forget(&mut self, key: &str) {
        self.images.remove(key);
        self.errors.remove(key);
        self.requested.remove(key);
        self.textures.retain(|(k, _), _| k != key);
    }

    fn selected_entry(&self) -> Option<&skins::SkinEntry> {
        let f = self.selected.as_deref()?;
        self.state.skins.iter().find(|s| s.file == f)
    }

    fn save(&mut self, game_running: bool, what: &str) {
        match skins::save_state(&self.layout, &mut self.state) {
            Ok(()) => {
                self.state_mtime = self.mtime();
                let when = if game_running {
                    "The running game picks it up within a second."
                } else {
                    "It is used the next time you press Play."
                };
                self.message = Some((true, format!("{what} {when}")));
            }
            Err(e) => self.message = Some((false, format!("Could not save: {e}"))),
        }
    }

    fn handle_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Decoded { key, image } => match image {
                    Ok((img, model)) => {
                        if key == ACCOUNT {
                            self.account_model = model;
                        }
                        self.images.insert(key, img);
                    }
                    Err(e) => {
                        self.errors.insert(key, e);
                    }
                },
                Event::Imported(result) => {
                    self.busy = false;
                    match result {
                        Ok((state, file)) => {
                            self.state = state;
                            self.forget(&file);
                            let name = self
                                .state
                                .skins
                                .iter()
                                .find(|s| s.file == file)
                                .map(|s| s.name.clone())
                                .unwrap_or_default();
                            self.selected = Some(file);
                            self.state_mtime = self.mtime();
                            self.message = Some((
                                true,
                                format!(
                                    "Loaded \"{name}\". Check the preview and the arm model, then press Apply Skin."
                                ),
                            ));
                        }
                        Err(e) => self.message = Some((false, e)),
                    }
                }
            }
        }
    }

    /// Decoded image for a key, requested from a worker on first use.
    fn image(&mut self, key: &str) -> Option<&Rgba> {
        if !self.images.contains_key(key)
            && !self.errors.contains_key(key)
            && self.requested.insert(key.to_string())
        {
            let tx = self.tx.clone();
            let key = key.to_string();
            let path = skins::skin_path(&self.layout, &key);
            let account = self.account_uuid.clone();
            std::thread::spawn(move || {
                let image = if key == ACCOUNT {
                    load_account_skin(account)
                } else {
                    load_file(path)
                };
                let _ = tx.send(Event::Decoded { key, image });
            });
        }
        self.images.get(key)
    }

    fn texture(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        slim: bool,
    ) -> Option<egui::TextureHandle> {
        if let Some(t) = self.textures.get(&(key.to_string(), slim)) {
            return Some(t.clone());
        }
        let img = self.image(key)?;
        let p = skins::preview(img, slim);
        let ci = egui::ColorImage::from_rgba_unmultiplied(
            [p.width as usize, p.height as usize],
            &p.pixels,
        );
        let tex = ctx.load_texture(
            format!("skin:{key}:{slim}"),
            ci,
            egui::TextureOptions::NEAREST,
        );
        self.textures.insert((key.to_string(), slim), tex.clone());
        Some(tex)
    }

    fn import_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a Minecraft skin")
            .add_filter("Minecraft skin (PNG, 64x64 or 64x32)", &["png", "PNG"])
            .pick_file()
        else {
            return;
        };
        self.busy = true;
        self.message = Some((true, format!("Checking {}…", path.display())));
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let mut state = skins::load_state(&layout);
            let r = skins::import_file(&layout, &mut state, &path)
                .and_then(|e| skins::save_state(&layout, &mut state).map(|_| e.file));
            let r = r.map(|file| (state, file)).map_err(|e| {
                log::warn!("skin import of {} rejected: {e}", path.display());
                format!("Skin rejected: {e}")
            });
            let _ = tx.send(Event::Imported(r));
        });
    }

    fn import_url(&mut self) {
        let input = self.url.trim().to_string();
        // Parse first so unsupported links fail instantly, without network.
        match skins::parse_source(&input) {
            Err(e) => {
                self.message = Some((false, e));
                return;
            }
            Ok(SkinSource::NameMcSkin(_)) => {
                self.message = Some((false, skins::NAMEMC_SKIN_PAGE_HELP.into()));
                return;
            }
            Ok(_) => {}
        }
        self.busy = true;
        self.message = Some((true, "Looking up the skin…".into()));
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let mut state = skins::load_state(&layout);
            let r = skins::import_url(&layout, &mut state, &input)
                .and_then(|e| skins::save_state(&layout, &mut state).map(|_| e.file));
            let r = r.map(|file| (state, file)).map_err(|e| {
                log::warn!("skin URL import failed: {e}");
                format!("Could not load that link: {e}")
            });
            let _ = tx.send(Event::Imported(r));
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, game_running: bool) {
        if !self.loaded {
            self.refresh();
        }
        let ctx = ui.ctx().clone();
        self.handle_events();
        // Pick up changes made in game (or by another launcher window).
        let now = ctx.input(|i| i.time);
        if !self.busy && now - self.last_poll > 1.0 {
            self.last_poll = now;
            if self.mtime() != self.state_mtime {
                self.refresh();
            }
        }
        ctx.request_repaint_after(Duration::from_millis(if self.busy { 100 } else { 1000 }));

        heading(ui, "Skin");
        ui.label(
            RichText::new(
                "Local skin changer: the skin is shown only in your own Kestrel client. Nothing is uploaded to \
                 your Microsoft/Mojang account and other players keep seeing your real account skin.",
            )
            .color(MUTED),
        );
        ui.add_space(8.0);
        let active_name = match self.state.active_entry() {
            Some(e) => format!("Active: {} ({})", e.name, e.model),
            None => "Active: your account skin".into(),
        };
        ui.label(RichText::new(active_name).strong().color(OK));
        if let Some((ok, msg)) = &self.message {
            ui.add_space(4.0);
            ui.label(RichText::new(msg).color(if *ok { MUTED } else { ERR }));
        }
        ui.add_space(8.0);

        // Preview beside the actions on wide windows, above them on narrow ones.
        if ui.available_width() >= 860.0 {
            ui.horizontal_top(|ui| {
                self.preview_ui(ui, &ctx, game_running);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width().min(460.0));
                    self.actions_ui(ui, game_running);
                });
            });
        } else {
            self.preview_ui(ui, &ctx, game_running);
            ui.add_space(10.0);
            self.actions_ui(ui, game_running);
        }

        // Library of imported skins.
        ui.add_space(12.0);
        ui.label(RichText::new("Your skins").strong().size(16.0));
        ui.add_space(4.0);
        let mut select = None;
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            let account_sel = self.selected.is_none();
            let account_slim = self.account_model == Some(MODEL_SLIM);
            let tex = self.texture(&ctx, ACCOUNT, account_slim);
            if thumb(
                ui,
                tex,
                "Account skin",
                account_sel,
                self.state.active.is_none(),
                false,
            )
            .0
            {
                select = Some(None);
            }
            let entries = self.state.skins.clone();
            for e in &entries {
                let tex = self.texture(&ctx, &e.file, e.model == MODEL_SLIM);
                let sel = self.selected.as_deref() == Some(e.file.as_str());
                let active = self.state.active.as_deref() == Some(e.file.as_str());
                let (clicked, removed) = thumb(ui, tex, &e.name, sel, active, true);
                if clicked {
                    select = Some(Some(e.file.clone()));
                }
                if removed {
                    remove = Some(e.file.clone());
                }
            }
        });
        if let Some(s) = select {
            self.selected = s;
        }
        if let Some(file) = remove {
            match skins::remove(&self.layout, &mut self.state, &file) {
                Ok(()) => {
                    if self.selected.as_deref() == Some(file.as_str()) {
                        self.selected = self.state.active.clone();
                    }
                    self.forget(&file);
                    self.save(game_running, "Skin removed.");
                }
                Err(e) => self.message = Some((false, format!("Could not remove: {e}"))),
            }
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!(
                "Stored in {}",
                skins::skins_dir(&self.layout).display()
            ))
            .small()
            .color(MUTED),
        );
    }

    /// Big front/back preview of the selected skin and its arm model.
    fn preview_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, game_running: bool) {
        egui::Frame::NONE
            .fill(CARD)
            .corner_radius(CornerRadius::same(8))
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.set_width(250.0);
                let (key, slim, title) = match self.selected_entry() {
                    Some(e) => (e.file.clone(), e.model == MODEL_SLIM, e.name.clone()),
                    None => (
                        ACCOUNT.to_string(),
                        self.account_model == Some(MODEL_SLIM),
                        "Account skin".to_string(),
                    ),
                };
                ui.label(RichText::new(&title).strong());
                ui.label(
                    RichText::new("Front                Back")
                        .small()
                        .color(MUTED),
                );
                let size = Vec2::new(36.0 * 6.0, 32.0 * 6.0);
                if let Some(t) = self.texture(ctx, &key, slim) {
                    ui.add(egui::Image::new(&t).fit_to_exact_size(size));
                } else {
                    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                    let msg = match self.errors.get(&key) {
                        Some(e) => e.clone(),
                        None if key == ACCOUNT && self.account_uuid.is_none() => {
                            "Sign in with Microsoft to preview your account skin.".into()
                        }
                        None => "Loading…".into(),
                    };
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        msg,
                        egui::FontId::proportional(13.0),
                        MUTED,
                    );
                }
                // Arm model of the selected imported skin.
                let legacy = self.images.get(&key).is_some_and(|i| i.height == 32);
                if let Some(file) = self.selected.clone() {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Arms:");
                        let current = self
                            .selected_entry()
                            .map(|e| e.model.clone())
                            .unwrap_or_default();
                        let mut model = current.clone();
                        ui.selectable_value(
                            &mut model,
                            MODEL_CLASSIC.to_string(),
                            "Classic (4 px)",
                        );
                        ui.add_enabled_ui(!legacy, |ui| {
                            ui.selectable_value(&mut model, MODEL_SLIM.to_string(), "Slim (3 px)")
                                .on_disabled_hover_text(
                                    "Old 64x32 skins only support classic arms",
                                );
                        });
                        if model != current
                            && let Some(e) = self.state.skins.iter_mut().find(|s| s.file == file)
                        {
                            e.model = model;
                            let what = if self.state.active.as_deref() == Some(file.as_str()) {
                                "Arm model updated."
                            } else {
                                "Arm model saved."
                            };
                            self.save(game_running, what);
                        }
                    });
                }
            });
    }

    /// Import, URL and apply/restore controls.
    fn actions_ui(&mut self, ui: &mut egui::Ui, game_running: bool) {
        card(ui, |ui| {
            ui.add_enabled_ui(!self.busy, |ui| {
                if ui.add(big_button("Import Skin…", false)).clicked() {
                    self.import_file();
                }
                ui.label(RichText::new("PNG, 64x64 or the old 64x32 format, up to 512 KB.").small().color(MUTED));
                ui.add_space(10.0);
                ui.label(RichText::new("Load from NameMC").strong());
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("https://namemc.com/profile/<name>")
                        .desired_width(400.0)
                        .char_limit(300),
                );
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add(big_button("Load from URL", false)).clicked() || (enter && !self.url.trim().is_empty()) {
                    self.import_url();
                }
                ui.label(
                    RichText::new(
                        "Profile links load that player's current skin through Mojang's public profile API. \
                         NameMC skin pages (namemc.com/skin/…) can't be fetched automatically: download the \
                         PNG from the page and use Import Skin.",
                    )
                    .small()
                    .color(MUTED),
                );
            });
            if self.busy {
                ui.add(egui::Spinner::new());
            }
        });
        ui.add_space(10.0);
        card(ui, |ui| {
            let selected_is_active = self.selected == self.state.active;
            ui.horizontal_wrapped(|ui| {
                let apply = ui.add_enabled(
                    !self.busy && self.selected.is_some() && !selected_is_active,
                    big_button("Apply Skin", true),
                );
                if apply.clicked() {
                    self.state.active = self.selected.clone();
                    self.save(game_running, "Skin applied.");
                }
                let restore = ui.add_enabled(
                    !self.busy && self.state.active.is_some(),
                    big_button("Restore Account Skin", false),
                );
                if restore.clicked() {
                    self.state.active = None;
                    self.selected = None;
                    self.save(game_running, "Account skin restored.");
                }
            });
            if self.selected.is_some() && selected_is_active {
                ui.label(RichText::new("This skin is active.").small().color(OK));
            } else if self.selected.is_some() {
                ui.label(
                    RichText::new("Previewing; press Apply Skin to use it.")
                        .small()
                        .color(WARN),
                );
            }
        });
    }
}

fn big_button(text: &str, primary: bool) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).size(15.0).color(if primary {
        Color32::BLACK
    } else {
        Color32::WHITE
    }))
    .fill(if primary {
        ACCENT
    } else {
        CARD.gamma_multiply(1.4)
    })
    .min_size(Vec2::new(180.0, 32.0))
    .corner_radius(CornerRadius::same(6))
}

/// One library card. Returns (clicked, remove clicked).
fn thumb(
    ui: &mut egui::Ui,
    tex: Option<egui::TextureHandle>,
    name: &str,
    selected: bool,
    active: bool,
    removable: bool,
) -> (bool, bool) {
    let mut removed = false;
    let resp = egui::Frame::NONE
        .fill(if selected {
            ACCENT.gamma_multiply(0.25)
        } else {
            CARD
        })
        .stroke(if selected {
            Stroke::new(1.0, ACCENT)
        } else {
            Stroke::NONE
        })
        .corner_radius(CornerRadius::same(6))
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.set_width(THUMB.x);
            ui.vertical_centered(|ui| {
                let size = Vec2::new(THUMB.x, THUMB.x * 32.0 / 36.0);
                match tex {
                    Some(t) => {
                        ui.add(egui::Image::new(&t).fit_to_exact_size(size));
                    }
                    None => {
                        ui.allocate_exact_size(size, Sense::hover());
                    }
                }
                ui.label(RichText::new(name).small());
                ui.horizontal(|ui| {
                    if active {
                        ui.label(RichText::new("Active").small().color(OK));
                    }
                    if removable && ui.small_button("Remove").clicked() {
                        removed = true;
                    }
                });
            });
        })
        .response
        .interact(Sense::click());
    (resp.clicked() && !removed, removed)
}

fn load_file(path: Option<PathBuf>) -> Result<(Rgba, Option<&'static str>), String> {
    let path = path.ok_or_else(|| "invalid skin name".to_string())?;
    let bytes = std::fs::read(&path).map_err(|e| format!("cannot read skin: {e}"))?;
    let skin = skins::validate_skin(&bytes)?;
    Ok((skin.rgba, Some(skin.suggested_model)))
}

/// The signed-in account's real skin, through Mojang's public profile API.
fn load_account_skin(uuid: Option<String>) -> Result<(Rgba, Option<&'static str>), String> {
    let uuid =
        uuid.ok_or_else(|| "Sign in with Microsoft to preview your account skin.".to_string())?;
    let d = skins::resolve(&SkinSource::Profile(uuid), &skins::http_fetch).map_err(|e| {
        if e.contains("default skin") {
            "Your account uses a default skin (Steve/Alex).".to_string()
        } else {
            format!("Could not load your account skin: {e}")
        }
    })?;
    let skin = skins::validate_skin(&d.bytes)?;
    Ok((skin.rgba, d.model))
}
