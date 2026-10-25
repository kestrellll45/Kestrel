//! The launcher's QoL page: Mod Manager, Resource Packs and Fullbright.
//!
//! * Mods and resource packs are files the game reads at start-up, so the
//!   page edits them only while Minecraft is not running (Forge keeps mod
//!   jars open, and Minecraft rewrites options.txt from memory). Folder scans,
//!   installs and moves run on worker threads; icons are decoded off the UI
//!   thread.
//! * Fullbright travels in the launcher → mod request file (see
//!   `kestrel_core::cosmetics`), so it also applies live within a second.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Layout as UiLayout, RichText, Sense, Stroke, Vec2,
};
use kestrel_core::cosmetics::{self, Rgba};
use kestrel_core::mods::{self, Candidate, Compat, ModEntry, ModScan, ModState, TrashEntry};
use kestrel_core::paths::Layout;
use kestrel_core::resourcepacks::{self, PackInfo, PackScan, Selection};

use crate::app::{ACCENT, CARD, ERR, MUTED, OK, WARN, card, heading, open_path};

const ICON_BG: Color32 = Color32::from_rgb(0x30, 0x33, 0x39);
const ROW: Color32 = Color32::from_rgb(0x1E, 0x21, 0x25);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Mods,
    Packs,
    Fullbright,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModSort {
    Name,
    Status,
}

enum Event {
    Mods(ModScan, Vec<TrashEntry>),
    Packs(PackScan, Result<Selection, String>),
    Icon {
        key: String,
        image: Result<Rgba, String>,
    },
    Checked(Result<Candidate, String>),
    ModOp(Result<String, String>),
}

pub struct QolPage {
    layout: Layout,
    tab: Tab,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    message: Option<(bool, String)>,

    // Mod manager
    mods: Vec<ModEntry>,
    mod_errors: Vec<String>,
    trash: Vec<TrashEntry>,
    mods_loading: bool,
    mod_sort: ModSort,
    mod_op: bool,
    confirm_install: Option<Candidate>,
    confirm_remove: Option<ModEntry>,

    // Resource packs
    packs: Vec<PackInfo>,
    pack_errors: Vec<String>,
    selection: Selection,
    packs_loading: bool,
    selected_pack: Option<String>,
    confirm_pack: Option<String>,
    icons: HashMap<String, egui::TextureHandle>,
    icon_failed: HashSet<String>,
    icon_requested: HashSet<String>,

    // Fullbright
    fullbright: bool,
    fullbright_pending: bool,
    last_poll: f64,
}

impl QolPage {
    pub fn new(layout: Layout) -> Self {
        let (tx, rx) = channel();
        QolPage {
            layout,
            tab: Tab::Mods,
            tx,
            rx,
            message: None,
            mods: Vec::new(),
            mod_errors: Vec::new(),
            trash: Vec::new(),
            mods_loading: false,
            mod_sort: ModSort::Name,
            mod_op: false,
            confirm_install: None,
            confirm_remove: None,
            packs: Vec::new(),
            pack_errors: Vec::new(),
            selection: Selection::default(),
            packs_loading: false,
            selected_pack: None,
            confirm_pack: None,
            icons: HashMap::new(),
            icon_failed: HashSet::new(),
            icon_requested: HashSet::new(),
            fullbright: false,
            fullbright_pending: false,
            last_poll: 0.0,
        }
    }

    /// Re-read everything (called whenever the page is opened).
    pub fn refresh(&mut self) {
        self.scan_mods();
        self.scan_packs();
        self.load_fullbright();
    }

    fn scan_mods(&mut self) {
        if self.mods_loading {
            return;
        }
        self.mods_loading = true;
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let scan = mods::scan(&layout);
            let trash = mods::list_trash(&layout);
            let _ = tx.send(Event::Mods(scan, trash));
        });
    }

    fn scan_packs(&mut self) {
        if self.packs_loading {
            return;
        }
        self.packs_loading = true;
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let scan = resourcepacks::scan(&layout);
            let sel = resourcepacks::read_selection(&layout).map_err(|e| e.to_string());
            let _ = tx.send(Event::Packs(scan, sel));
        });
    }

    fn load_fullbright(&mut self) {
        let st = cosmetics::load_state(&self.layout);
        self.fullbright = st.settings.qol.fullbright;
        self.fullbright_pending = st.pending;
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Mods(scan, trash) => {
                    self.mods_loading = false;
                    self.mods = scan.mods;
                    self.mod_errors = scan.errors;
                    self.trash = trash;
                }
                Event::Packs(scan, sel) => {
                    self.packs_loading = false;
                    self.packs = scan.packs;
                    self.pack_errors = scan.errors;
                    match sel {
                        Ok(s) => {
                            if let Some(w) = &s.warning {
                                self.pack_errors.push(w.clone());
                            }
                            self.selection = s;
                        }
                        Err(e) => self
                            .pack_errors
                            .push(format!("cannot read options.txt: {e}")),
                    }
                    // Packs may have changed on disk: drop stale icons.
                    self.icons.clear();
                    self.icon_failed.clear();
                    self.icon_requested.clear();
                    let ids: HashSet<&str> = self.packs.iter().map(|p| p.id.as_str()).collect();
                    if self
                        .selected_pack
                        .as_deref()
                        .is_some_and(|s| !ids.contains(s))
                    {
                        self.selected_pack = None;
                    }
                }
                Event::Icon { key, image } => match image {
                    Ok(img) => {
                        let ci = egui::ColorImage::from_rgba_unmultiplied(
                            [img.width as usize, img.height as usize],
                            &img.pixels,
                        );
                        let tex = ctx.load_texture(key.clone(), ci, egui::TextureOptions::NEAREST);
                        self.icons.insert(key, tex);
                    }
                    Err(e) => {
                        log::debug!("pack icon {key}: {e}");
                        self.icon_failed.insert(key);
                    }
                },
                Event::Checked(result) => {
                    self.mod_op = false;
                    match result {
                        Ok(c) => match &c.compat {
                            Compat::Supported(_) => self.run_install(c),
                            Compat::Unknown(_) => self.confirm_install = Some(c),
                            Compat::Incompatible(r) | Compat::Invalid(r) => {
                                self.message = Some((
                                    false,
                                    format!("{} was not installed: {r}.", c.file_name),
                                ));
                            }
                        },
                        Err(e) => self.message = Some((false, format!("Not installed: {e}"))),
                    }
                }
                Event::ModOp(result) => {
                    self.mod_op = false;
                    self.message = Some(match result {
                        Ok(m) => (true, m),
                        Err(e) => (false, e),
                    });
                    self.scan_mods();
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, game_active: bool, game_running: bool) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);

        heading(ui, "QoL");
        ui.label(
            RichText::new("Manage mods and resource packs for Kestrel's Minecraft 1.8.9, and switch Fullbright on or off.")
                .color(MUTED),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            for (t, name) in [
                (Tab::Mods, "Mod Manager"),
                (Tab::Packs, "Resource Packs"),
                (Tab::Fullbright, "Fullbright"),
            ] {
                let sel = self.tab == t;
                let b = egui::Button::new(RichText::new(name).size(15.0).color(if sel {
                    Color32::BLACK
                } else {
                    Color32::WHITE
                }))
                .fill(if sel { ACCENT } else { CARD })
                .min_size(Vec2::new(140.0, 30.0))
                .corner_radius(CornerRadius::same(6));
                if ui.add(b).clicked() && self.tab != t {
                    self.tab = t;
                    self.message = None;
                }
            }
        });
        if let Some((ok, msg)) = self.message.clone() {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(msg).color(if ok { OK } else { ERR }));
                if ui.small_button("Dismiss").clicked() {
                    self.message = None;
                }
            });
        }
        ui.add_space(8.0);
        match self.tab {
            Tab::Mods => self.mods_ui(ui, game_active),
            Tab::Packs => self.packs_ui(ui, game_active),
            Tab::Fullbright => self.fullbright_ui(ui, game_running),
        }
        self.dialogs(&ctx);

        if self.mods_loading || self.packs_loading || self.mod_op {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.fullbright_pending && game_running {
            // Notice when the running game picks the request up.
            let now = ctx.input(|i| i.time);
            if now - self.last_poll > 1.0 {
                self.last_poll = now;
                self.load_fullbright();
            }
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    // ---- Mod Manager ------------------------------------------------------------

    fn mods_ui(&mut self, ui: &mut egui::Ui, game_active: bool) {
        let locked = game_active || self.mod_op;
        card(ui, |ui| {
            ui.label(RichText::new("Mod Manager").strong());
            ui.label(
                RichText::new(format!(
                    "Forge 1.8.9 mods in {}. Changes take effect the next time Minecraft starts.",
                    self.layout.mods().display()
                ))
                .small()
                .color(MUTED),
            );
            if game_active {
                ui.label(
                    RichText::new("Minecraft is starting or running: close it to install, enable, disable or remove mods.")
                        .color(WARN),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!locked, egui::Button::new("Install mod…"))
                    .on_hover_text("Pick a Forge 1.8.9 mod (.jar) to copy into the mods folder")
                    .clicked()
                {
                    self.pick_mod();
                }
                if ui
                    .add_enabled(!self.mods_loading, egui::Button::new("Refresh"))
                    .clicked()
                {
                    self.scan_mods();
                }
                if ui.button("Open mods folder").clicked() {
                    open_path(&self.layout.mods());
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Sort").color(MUTED));
                egui::ComboBox::from_id_salt("qol_mod_sort")
                    .selected_text(match self.mod_sort {
                        ModSort::Name => "Name",
                        ModSort::Status => "Status",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.mod_sort, ModSort::Name, "Name");
                        ui.selectable_value(&mut self.mod_sort, ModSort::Status, "Status");
                    });
                if self.mods_loading || self.mod_op {
                    ui.add(egui::Spinner::new());
                    ui.label(
                        RichText::new(if self.mod_op {
                            "Working…"
                        } else {
                            "Scanning…"
                        })
                        .color(MUTED),
                    );
                }
            });
        });
        for e in &self.mod_errors {
            ui.label(RichText::new(e).color(ERR));
        }
        ui.add_space(8.0);

        let mut list: Vec<ModEntry> = self.mods.clone();
        if self.mod_sort == ModSort::Status {
            list.sort_by_key(|m| {
                (
                    !m.builtin,
                    m.state != ModState::Enabled,
                    m.display_name().to_lowercase(),
                )
            });
        }
        let external = list.iter().filter(|m| !m.builtin).count();
        let mut action: Option<(ModAction, ModEntry)> = None;
        for m in &list {
            if let Some(a) = mod_row(ui, m, locked) {
                action = Some((a, m.clone()));
            }
            ui.add_space(4.0);
        }
        if external == 0 && !self.mods_loading {
            card(ui, |ui| {
                ui.label(RichText::new("No external mods installed").strong());
                ui.label(
                    RichText::new(
                        "Kestrel's own features are built in. To add a Forge mod made for Minecraft 1.8.9, click \
                         Install mod… and choose its .jar file.",
                    )
                    .color(MUTED),
                );
            });
        }
        if let Some((a, m)) = action {
            match a {
                ModAction::Toggle => self.run_toggle(m),
                ModAction::Remove => self.confirm_remove = Some(m),
            }
        }

        if !self.trash.is_empty() {
            ui.add_space(8.0);
            egui::CollapsingHeader::new(format!("Recently removed ({})", self.trash.len()))
                .id_salt("qol_trash")
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Removed mods are kept in {} until you delete them. Restoring puts a mod back as disabled.",
                            self.layout.mod_trash().display()
                        ))
                        .small()
                        .color(MUTED),
                    );
                    let mut restore: Option<PathBuf> = None;
                    for t in &self.trash {
                        ui.horizontal(|ui| {
                            ui.label(&t.original_name);
                            ui.label(RichText::new(&t.stored_name).small().color(MUTED));
                            if ui.add_enabled(!locked, egui::Button::new("Restore")).clicked() {
                                restore = Some(t.path.clone());
                            }
                        });
                    }
                    if ui.button("Open removed-mods folder").clicked() {
                        open_path(&self.layout.mod_trash());
                    }
                    if let Some(p) = restore {
                        self.run_mod_op(move |l| {
                            mods::restore(l, &p).map(|d| format!("Restored as disabled: {}", file_name(&d)))
                        });
                    }
                });
        }
    }

    fn pick_mod(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose a Forge 1.8.9 mod")
            .add_filter("Minecraft mod (.jar, .zip)", &["jar", "JAR", "zip", "ZIP"])
            .pick_file()
        else {
            return;
        };
        self.mod_op = true;
        self.message = Some((true, format!("Checking {}…", path.display())));
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let r = mods::check_candidate(&layout, &path).map_err(|e| e.to_string());
            let _ = tx.send(Event::Checked(r));
        });
    }

    fn run_install(&mut self, c: Candidate) {
        let unknown = matches!(c.compat, Compat::Unknown(_));
        self.run_mod_op(move |l| {
            mods::install(l, &c).map(|_| {
                format!(
                    "Installed {}{}. It loads the next time Minecraft starts.",
                    c.meta.name.as_deref().unwrap_or(&c.file_name),
                    if unknown {
                        " (compatibility not verified)"
                    } else {
                        ""
                    }
                )
            })
        });
    }

    fn run_toggle(&mut self, m: ModEntry) {
        let name = m.display_name().to_string();
        match m.state {
            ModState::Enabled => self.run_mod_op(move |l| {
                mods::disable(l, &m.path).map(|_| {
                    format!("Disabled {name}. Takes effect the next time Minecraft starts.")
                })
            }),
            ModState::Disabled => self.run_mod_op(move |l| {
                mods::enable(l, &m.path).map(|_| {
                    format!("Enabled {name}. Takes effect the next time Minecraft starts.")
                })
            }),
        }
    }

    fn run_mod_op(
        &mut self,
        op: impl FnOnce(&Layout) -> kestrel_core::error::Result<String> + Send + 'static,
    ) {
        self.mod_op = true;
        let (tx, layout) = (self.tx.clone(), self.layout.clone());
        std::thread::spawn(move || {
            let r = op(&layout).map_err(|e| {
                log::warn!("mod manager: {e}");
                e.to_string()
            });
            let _ = tx.send(Event::ModOp(r));
        });
    }

    // ---- Resource packs -----------------------------------------------------------

    fn packs_ui(&mut self, ui: &mut egui::Ui, game_active: bool) {
        let locked = game_active;
        card(ui, |ui| {
            ui.label(RichText::new("Resource Packs").strong());
            ui.label(
                RichText::new(format!(
                    "Packs in {}. Enabled packs are saved to options.txt exactly like Minecraft's own menu; the top \
                     of the list wins when two packs change the same texture.",
                    self.layout.resourcepacks().display()
                ))
                .small()
                .color(MUTED),
            );
            if game_active {
                ui.label(
                    RichText::new(
                        "Minecraft is starting or running and would overwrite options.txt: close it to change packs here, \
                         or use Options > Resource Packs in game.",
                    )
                    .color(WARN),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Open Resource Pack Folder").clicked() {
                    let dir = self.layout.resourcepacks();
                    match std::fs::create_dir_all(&dir) {
                        Ok(()) => open_path(&dir),
                        Err(e) => {
                            self.message =
                                Some((false, format!("Cannot create {}: {e}", dir.display())))
                        }
                    }
                }
                if ui
                    .add_enabled(!self.packs_loading, egui::Button::new("Refresh"))
                    .clicked()
                {
                    self.scan_packs();
                }
                if self.packs_loading {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new("Scanning…").color(MUTED));
                }
            });
        });
        for e in &self.pack_errors {
            ui.label(RichText::new(e).color(ERR));
        }
        ui.add_space(8.0);

        // Preview of the selected pack.
        let enabled_top_first = self.selection.by_priority();
        let preview_id = self
            .selected_pack
            .clone()
            .or_else(|| {
                enabled_top_first
                    .iter()
                    .find(|id| self.pack(id).is_some())
                    .cloned()
            })
            .or_else(|| self.packs.first().map(|p| p.id.clone()));
        if let Some(id) = &preview_id
            && let Some(p) = self.pack(id).cloned()
        {
            let tex = self.icon(&p);
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    icon_widget(ui, tex.as_ref(), p.display_name(), 72.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(p.display_name()).size(17.0).strong());
                        if !p.description.is_empty() {
                            ui.label(&p.description);
                        }
                        let kind = match p.kind {
                            resourcepacks::PackKind::Zip => "ZIP pack",
                            resourcepacks::PackKind::Folder => "Folder pack",
                        };
                        let status = if let Some(problem) = &p.problem {
                            RichText::new(format!("{kind} · can't be used: {problem}")).color(ERR)
                        } else if p.is_other_version() {
                            RichText::new(format!(
                                "{kind} · made for another Minecraft version (pack_format {}); may look wrong in 1.8.9",
                                p.pack_format.unwrap_or_default()
                            ))
                            .color(WARN)
                        } else {
                            RichText::new(format!("{kind} · made for Minecraft 1.8.9")).color(OK)
                        };
                        ui.label(status.small());
                        let on = self.selection.enabled.contains(&p.id);
                        ui.label(
                            RichText::new(if on { "Enabled" } else { "Not enabled" })
                                .small()
                                .color(if on { OK } else { MUTED }),
                        );
                    });
                });
            });
            ui.add_space(8.0);
        }

        // Enabled (highest priority first, like Minecraft's menu).
        ui.label(RichText::new("Enabled — top has the highest priority").strong());
        ui.add_space(4.0);
        let mut new_order: Option<Vec<String>> = None;
        if enabled_top_first.is_empty() {
            ui.label(
                RichText::new("No packs enabled: Minecraft uses its default textures.")
                    .color(MUTED),
            );
        }
        let n = enabled_top_first.len();
        for (i, id) in enabled_top_first.iter().enumerate() {
            let pack = self.pack(id).cloned();
            let tex = pack.as_ref().and_then(|p| self.icon(p));
            let name = pack
                .as_ref()
                .map(|p| p.display_name().to_string())
                .unwrap_or_else(|| id.clone());
            let sub = match &pack {
                None => RichText::new(
                    "file not found in the resource pack folder; Minecraft ignores it",
                )
                .color(WARN),
                Some(p) if p.is_other_version() => {
                    RichText::new("made for another version").color(WARN)
                }
                Some(p) => RichText::new(first_line(&p.description)).color(MUTED),
            };
            let resp = pack_row(
                ui,
                tex.as_ref(),
                &name,
                sub,
                self.selected_pack.as_deref() == Some(id),
                |ui| {
                    let mut act = None;
                    if ui
                        .add_enabled(!locked, egui::Button::new("Disable"))
                        .clicked()
                    {
                        act = Some(PackAction::Disable);
                    }
                    if ui
                        .add_enabled(!locked && i + 1 < n, egui::Button::new("Down"))
                        .on_hover_text("Lower priority")
                        .clicked()
                    {
                        act = Some(PackAction::Down);
                    }
                    if ui
                        .add_enabled(!locked && i > 0, egui::Button::new("Up"))
                        .on_hover_text("Higher priority")
                        .clicked()
                    {
                        act = Some(PackAction::Up);
                    }
                    act
                },
            );
            if resp.clicked {
                self.selected_pack = Some(id.clone());
            }
            if let Some(a) = resp.action {
                let mut top_first = enabled_top_first.clone();
                match a {
                    PackAction::Disable => {
                        top_first.remove(i);
                    }
                    PackAction::Up => top_first.swap(i, i - 1),
                    PackAction::Down => top_first.swap(i, i + 1),
                    PackAction::Enable => {}
                }
                new_order = Some(top_first);
            }
            ui.add_space(3.0);
        }

        ui.add_space(10.0);
        ui.label(RichText::new("Available").strong());
        ui.add_space(4.0);
        let available: Vec<PackInfo> = self
            .packs
            .iter()
            .filter(|p| !self.selection.enabled.contains(&p.id))
            .cloned()
            .collect();
        if self.packs.is_empty() && !self.packs_loading {
            card(ui, |ui| {
                ui.label(RichText::new("No resource packs yet").strong());
                ui.label(
                    RichText::new(
                        "Click Open Resource Pack Folder and copy .zip packs (or unzipped pack folders) into it, then \
                         Refresh.",
                    )
                    .color(MUTED),
                );
            });
        } else if available.is_empty() && !self.packs.is_empty() {
            ui.label(RichText::new("All packs are enabled.").color(MUTED));
        }
        for p in &available {
            let tex = self.icon(p);
            let sub = match &p.problem {
                Some(problem) => RichText::new(format!("can't be used: {problem}")).color(ERR),
                None if p.is_other_version() => {
                    RichText::new("made for another version").color(WARN)
                }
                None => RichText::new(first_line(&p.description)).color(MUTED),
            };
            let valid = p.is_valid();
            let resp = pack_row(
                ui,
                tex.as_ref(),
                p.display_name(),
                sub,
                self.selected_pack.as_deref() == Some(&p.id),
                |ui| {
                    let b = ui.add_enabled(!locked && valid, egui::Button::new("Enable"));
                    let b = if valid {
                        b
                    } else {
                        b.on_disabled_hover_text("Minecraft can't load this pack")
                    };
                    b.clicked().then_some(PackAction::Enable)
                },
            );
            if resp.clicked {
                self.selected_pack = Some(p.id.clone());
            }
            if resp.action == Some(PackAction::Enable) {
                if p.is_other_version() {
                    self.confirm_pack = Some(p.id.clone());
                } else {
                    let mut top_first = enabled_top_first.clone();
                    top_first.insert(0, p.id.clone()); // like Minecraft: new packs go on top
                    new_order = Some(top_first);
                }
            }
            ui.add_space(3.0);
        }

        if let Some(top_first) = new_order {
            self.save_packs(top_first);
        }
    }

    fn pack(&self, id: &str) -> Option<&PackInfo> {
        self.packs.iter().find(|p| p.id == id)
    }

    /// Save a new selection given highest-priority-first order.
    fn save_packs(&mut self, top_first: Vec<String>) {
        let low_to_high: Vec<String> = top_first.into_iter().rev().collect();
        match resourcepacks::write_selection(&self.layout, &low_to_high, &self.packs) {
            Ok(()) => match resourcepacks::read_selection(&self.layout) {
                Ok(s) => {
                    self.selection = s;
                    self.message = Some((
                        true,
                        "Saved. Minecraft uses this order the next time it starts.".into(),
                    ));
                }
                Err(e) => {
                    self.message = Some((
                        false,
                        format!("Saved, but re-reading options.txt failed: {e}"),
                    ))
                }
            },
            Err(e) => self.message = Some((false, format!("Could not save options.txt: {e}"))),
        }
    }

    /// Pack icon texture, decoded once on a worker thread.
    fn icon(&mut self, p: &PackInfo) -> Option<egui::TextureHandle> {
        if !p.has_icon {
            return None;
        }
        let key = format!("pack:{}", p.id);
        if let Some(t) = self.icons.get(&key) {
            return Some(t.clone());
        }
        if !self.icon_failed.contains(&key) && self.icon_requested.insert(key.clone()) {
            let (tx, pack) = (self.tx.clone(), p.clone());
            std::thread::spawn(move || {
                let image = resourcepacks::load_icon(&pack);
                let _ = tx.send(Event::Icon { key, image });
            });
        }
        None
    }

    // ---- Fullbright -------------------------------------------------------------

    fn fullbright_ui(&mut self, ui: &mut egui::Ui, game_running: bool) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let mut on = self.fullbright;
                if toggle(ui, &mut on).clicked() {
                    match cosmetics::save_fullbright(&self.layout, on) {
                        Ok(st) => {
                            self.fullbright = st.settings.qol.fullbright;
                            self.fullbright_pending = st.pending;
                            self.message = Some((
                                true,
                                if game_running {
                                    "Saved. Minecraft applies it within a second.".into()
                                } else {
                                    "Saved. Applied the next time you press Play.".into()
                                },
                            ));
                        }
                        Err(e) => self.message = Some((false, format!("Could not save: {e}"))),
                    }
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Fullbright").size(18.0).strong());
                    ui.label(
                        RichText::new(if self.fullbright { "On" } else { "Off" })
                            .strong()
                            .color(if self.fullbright { OK } else { MUTED }),
                    );
                });
            });
            ui.add_space(6.0);
            ui.label(
                "Lights up caves, nights and dark rooms as if everything were fully lit. Turning it off restores \
                 Minecraft's normal lighting immediately.",
            );
            ui.label(
                RichText::new(
                    "Only rendering changes: your Brightness setting in Video Settings, options.txt, worlds and \
                     resource packs are not modified.",
                )
                .small()
                .color(MUTED),
            );
            ui.add_space(4.0);
            if self.fullbright_pending {
                ui.label(
                    RichText::new(if game_running {
                        "Applying…"
                    } else {
                        "Saved: applied the next time you press Play"
                    })
                    .color(WARN),
                );
            } else {
                ui.label(RichText::new("In sync with the client").color(OK));
            }
        });
    }

    // ---- dialogs ----------------------------------------------------------------

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(c) = self.confirm_install.clone() {
            let mut choice = None;
            let r = egui::Modal::new(egui::Id::unique("qol_confirm_install")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new("Install a mod Kestrel can't verify?").size(17.0).strong());
                ui.add_space(6.0);
                ui.label(format!(
                    "{} ({})",
                    c.meta.name.as_deref().unwrap_or(&c.file_name),
                    c.file_name
                ));
                ui.label(RichText::new(c.compat.reason()).color(WARN));
                ui.label(
                    RichText::new(
                        "If it isn't a Forge mod for Minecraft 1.8.9, the game may crash on start-up. You can disable or \
                         remove it here afterwards.",
                    )
                    .color(MUTED),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Install anyway").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(false);
                    }
                });
            });
            if r.should_close() {
                choice = Some(false);
            }
            match choice {
                Some(true) => {
                    self.confirm_install = None;
                    self.run_install(c);
                }
                Some(false) => {
                    self.confirm_install = None;
                    self.message = Some((true, "Install cancelled.".into()));
                }
                None => {}
            }
        }

        if let Some(m) = self.confirm_remove.clone() {
            let mut choice = None;
            let r = egui::Modal::new(egui::Id::unique("qol_confirm_remove")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new(format!("Remove {}?", m.display_name())).size(17.0).strong());
                ui.add_space(6.0);
                ui.label(format!(
                    "{} is moved to Kestrel's removed-mods folder ({}). You can restore it from \"Recently removed\".",
                    m.file_name,
                    self.layout.mod_trash().display()
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Remove").color(Color32::WHITE)).fill(ERR)).clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(false);
                    }
                });
            });
            if r.should_close() {
                choice = Some(false);
            }
            match choice {
                Some(true) => {
                    self.confirm_remove = None;
                    let name = m.display_name().to_string();
                    self.run_mod_op(move |l| {
                        mods::remove(l, &m.path).map(|_| {
                            format!(
                                "Removed {name}. Restore it from \"Recently removed\" if needed."
                            )
                        })
                    });
                }
                Some(false) => self.confirm_remove = None,
                None => {}
            }
        }

        if let Some(id) = self.confirm_pack.clone() {
            let mut choice = None;
            let format = self
                .pack(&id)
                .and_then(|p| p.pack_format)
                .unwrap_or_default();
            let r = egui::Modal::new(egui::Id::unique("qol_confirm_pack")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new("Enable a pack made for another version?").size(17.0).strong());
                ui.add_space(6.0);
                ui.label(format!(
                    "{id} declares pack_format {format}; Minecraft 1.8.9 uses 1. It may show missing or wrong textures. \
                     Minecraft's own menu asks the same question."
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Enable anyway").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        choice = Some(false);
                    }
                });
            });
            if r.should_close() {
                choice = Some(false);
            }
            match choice {
                Some(true) => {
                    self.confirm_pack = None;
                    let mut top_first = self.selection.by_priority();
                    top_first.insert(0, id);
                    self.save_packs(top_first);
                }
                Some(false) => self.confirm_pack = None,
                None => {}
            }
        }
    }
}

// ---- widgets ----------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModAction {
    Toggle,
    Remove,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PackAction {
    Enable,
    Disable,
    Up,
    Down,
}

struct RowResponse {
    clicked: bool,
    action: Option<PackAction>,
}

fn mod_row(ui: &mut egui::Ui, m: &ModEntry, locked: bool) -> Option<ModAction> {
    let mut action = None;
    egui::Frame::NONE
        .fill(ROW)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(720.0));
            ui.horizontal(|ui| {
                let (dot, state) = match (m.builtin, m.state) {
                    (true, _) => (ACCENT, "Built in"),
                    (false, ModState::Enabled) => (OK, "Enabled"),
                    (false, ModState::Disabled) => (MUTED, "Disabled"),
                };
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.5, dot);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(m.display_name()).strong());
                        if let Some(v) = &m.meta.version
                            && !m.builtin
                        {
                            ui.label(RichText::new(v).color(MUTED));
                        }
                        ui.label(RichText::new(state).small().color(dot));
                    });
                    if m.builtin {
                        ui.label(
                            RichText::new(format!(
                                "{} · built into Kestrel.exe and updated with it; managed automatically",
                                m.file_name
                            ))
                            .small()
                            .color(MUTED),
                        );
                    } else {
                        let (txt, col) = match &m.compat {
                            Compat::Supported(r) => (format!("Supported · {r}"), OK),
                            Compat::Unknown(r) => (format!("Unverified · {r}"), WARN),
                            Compat::Incompatible(r) => (format!("Incompatible · {r}"), ERR),
                            Compat::Invalid(r) => (format!("Broken file · {r}"), ERR),
                        };
                        ui.label(RichText::new(format!("{} · {}", m.file_name, human_size(m.size))).small().color(MUTED));
                        ui.label(RichText::new(txt).small().color(col));
                    }
                });
                if !m.builtin {
                    ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(!locked, egui::Button::new(RichText::new("Remove").color(ERR)))
                            .clicked()
                        {
                            action = Some(ModAction::Remove);
                        }
                        let label = match m.state {
                            ModState::Enabled => "Disable",
                            ModState::Disabled => "Enable",
                        };
                        if ui.add_enabled(!locked, egui::Button::new(label)).clicked() {
                            action = Some(ModAction::Toggle);
                        }
                    });
                }
            });
        });
    action
}

fn pack_row(
    ui: &mut egui::Ui,
    tex: Option<&egui::TextureHandle>,
    name: &str,
    sub: RichText,
    selected: bool,
    buttons: impl FnOnce(&mut egui::Ui) -> Option<PackAction>,
) -> RowResponse {
    let mut action = None;
    let mut clicked = false;
    egui::Frame::NONE
        .fill(ROW)
        .stroke(if selected {
            Stroke::new(1.0, ACCENT)
        } else {
            Stroke::NONE
        })
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().min(720.0));
            ui.horizontal(|ui| {
                // Only the icon + text select the pack for the preview; the
                // buttons are separate widgets so they keep their clicks.
                let info = ui.horizontal(|ui| {
                    icon_widget(ui, tex, name, 36.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(name).strong());
                        ui.label(sub.small());
                    });
                });
                clicked = info
                    .response
                    .interact(Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked();
                ui.with_layout(UiLayout::right_to_left(Align::Center), |ui| {
                    action = buttons(ui);
                });
            });
        });
    RowResponse { clicked, action }
}

/// Pack icon, or a neutral tile with the pack's initial.
fn icon_widget(ui: &mut egui::Ui, tex: Option<&egui::TextureHandle>, name: &str, side: f32) {
    let size = Vec2::splat(side);
    match tex {
        Some(t) => {
            ui.add(
                egui::Image::new(t)
                    .fit_to_exact_size(size)
                    .corner_radius(CornerRadius::same(4)),
            );
        }
        None => {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(4), ICON_BG);
            let initial = name.chars().find(|c| c.is_alphanumeric()).unwrap_or('?');
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                initial.to_uppercase().to_string(),
                FontId::proportional(side * 0.45),
                MUTED,
            );
        }
    }
}

/// iOS-style switch (same look as egui's demo toggle).
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = Vec2::new(52.0, 28.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
    let radius = rect.height() / 2.0;
    let bg = if *on {
        ACCENT
    } else {
        Color32::from_rgb(0x3A, 0x3E, 0x45)
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius as u8), bg);
    let x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
    ui.painter()
        .circle_filled(egui::pos2(x, rect.center().y), radius - 3.0, Color32::WHITE);
    response.on_hover_text(if *on {
        "Turn Fullbright off"
    } else {
        "Turn Fullbright on"
    })
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn human_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}
