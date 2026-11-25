//! What content packs add to a game beyond art, text and data (docs/MODDING.md, section 12; sg_core::modkit): their object
//! types on the land, their tools, the panels and questions their scripts show, and their map views. The port's own.
//!
//! The Mods window (Information > Mods, shown while a pack adds something) lists the packs' object types to build, their
//! tools, panels and map views, and a Reload line for pack makers. A type or tool picked is held as the Mods tool
//! ([`TOOL`]): a click on the course places the object (Shift takes the pack object on the tile away, R turns it, Esc or
//! Space puts the tool down), or calls the pack's `on_tool`. Everything done to the game goes through the action log
//! (`Action::ModBuild`, `Action::ModEvent`), so a replay does the same.
//!
//! With no pack adding anything none of this shows and nothing here runs: the game plays as before.
use crate::app::*;
use crate::gfx::Gfx;
use crate::render::Rect;
use crate::ui::{money, rgb, Screen as Ui, F_ARIAL10, F_MANUAL15, F_MANUAL20};
use sg_core::action::Action;
use sg_core::economy;
use sg_core::land;
use sg_core::lang::{tr, trf};
use sg_core::modkit::{Catalog, ObjType, Panel, Placed, Refusal, Row, ToolDef, World};
use sg_core::script::{BuildingView, Hook, Snapshot};
use sg_core::terrain::TILE_SIZE;

/// The Mods tool's number (`App::tool`): after the original's tools and the Hole boundaries brush (7).
pub const TOOL: i32 = 8;

const BOX: Rect = Rect { x: 40.0, y: 36.0, w: 720.0, h: 528.0 };
const TOP: f32 = 76.0;
const ROW_H: f32 = 24.0;
const ROWS: usize = 17;

/// What the Mods tool holds.
#[derive(Clone, Debug, PartialEq)]
pub enum Sel {
    Build(String),
    Tool(String, ToolDef),
}

/// A line of the Mods window.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Heading(String),
    Build(String),
    Tool(usize),
    Panel(usize),
    Overlay(usize),
    OverlayOff,
    Missing(String, usize),
    Warning(String, String),
    Reload,
}

#[derive(Default)]
pub struct ModsState {
    /// What the enabled packs add, and what the game keeps of it (saved).
    pub catalog: Catalog,
    pub world: World,
    /// Problems with the packs' data/objects.toml and art: (pack, message).
    pub warnings: Vec<(String, String)>,
    pub sel: Option<Sel>,
    /// A panel or question a script shows.
    pub panel: Option<Panel>,
    /// The map view switched on: (pack, overlay id).
    pub overlay: Option<(String, String)>,
    pub scroll: usize,
    /// Objects changed since the course was last rebuilt (`flush_mods`).
    pub dirty: bool,
}

fn refusal_text(r: Refusal) -> &'static str {
    match r {
        Refusal::OffLand => tr("STR_MODS_NOT_ON_LAND"),
        Refusal::Taken => tr("STR_TOAST_NO_ROOM"),
        Refusal::Water => tr("STR_MODS_WATER"),
        Refusal::Limit => tr("STR_MODS_LIMIT"),
        Refusal::Full => tr("STR_MODS_FULL"),
    }
}

impl App {
    /// Whether the Mods window has anything to show (Information > Mods).
    pub fn has_mods(&self) -> bool {
        !self.mods.catalog.is_empty() || !self.mods.world.objects.is_empty()
    }

    /// The enabled packs that add objects or scripts or change the game's data (prices, wages, starting money: any
    /// data/*.toml), which keep the multiplayer modes closed (a shared game, a league or the weekly challenge must play the
    /// same for everyone, so their games run no pack's scripts, objects or data). None when there are none.
    #[cfg_attr(not(feature = "multiplayer"), allow(dead_code))]
    pub fn mods_block_multiplayer(&self) -> Option<String> {
        let ids: Vec<String> = self
            .content
            .loaded
            .iter()
            .filter(|id| {
                self.content.found.packs.iter().any(|p| {
                    p.manifest.id == **id
                        && matches!(p.manifest.kind, sg_core::packs::Kind::Mod | sg_core::packs::Kind::Base)
                        && p.source.files().iter().any(|f| {
                            let f = f.to_lowercase();
                            (f.starts_with("data/") && f.ends_with(".toml")) || (f.starts_with("scripts/") && f.ends_with(".rhai"))
                        })
                })
            })
            .cloned()
            .collect();
        (!ids.is_empty()).then(|| ids.join(", "))
    }

    /// Says why a multiplayer mode cannot start, and returns true, while such mods are on.
    #[cfg_attr(not(feature = "multiplayer"), allow(dead_code))]
    pub fn refuse_multiplayer_for_mods(&mut self) -> bool {
        let Some(ids) = self.mods_block_multiplayer() else { return false };
        self.ui_sound(24);
        self.show_toast(&trf("STR_MODS_NO_MULTIPLAYER", &[("PACKS", &ids)]));
        true
    }

    /// Reads data/objects.toml of the packs whose scripts run in this game (scenario_ui's `load_scripts`), in load order.
    pub(crate) fn load_mods(&mut self, ids: &[String]) {
        let packs: Vec<(String, std::sync::Arc<sg_core::vfs::Source>)> = ids
            .iter()
            .filter_map(|id| self.content.found.packs.iter().find(|p| p.manifest.id == *id).map(|p| (id.clone(), p.source.clone())))
            .collect();
        let refs: Vec<(String, &sg_core::vfs::Source)> = packs.iter().map(|(id, s)| (id.clone(), s.as_ref())).collect();
        let (cat, warn) = Catalog::load(&refs);
        for (id, w) in &warn {
            crate::bugreport::warn(&format!("content pack {id}: {w}"));
        }
        if !cat.is_empty() {
            println!(
                "pack objects: {} types, {} tools, {} panels, {} map views",
                cat.objects.len(),
                cat.tools.len(),
                cat.panels.len(),
                cat.overlays.len()
            );
        }
        self.mods.catalog = cat;
        self.mods.warnings = warn;
        self.mods.sel = None;
        self.mods.panel = None;
        self.mods.overlay = None;
        self.sprite_index.retain(|k, _| !k.starts_with("mod:"));
        self.mod_values_changed();
    }

    /// A saved game's pack objects are back (before its scripts are loaded); the packs missing are told.
    pub(crate) fn restore_mods(&mut self, w: Option<World>) {
        self.mods.world = w.unwrap_or_default();
    }

    /// After a game is loaded: a note on the pack objects whose packs are not on.
    pub(crate) fn tell_missing_mods(&mut self) {
        let missing = self.mods.world.missing(&self.mods.catalog);
        if missing.is_empty() {
            return;
        }
        let packs: Vec<String> = missing.iter().map(|(p, _)| p.clone()).collect();
        let n: usize = missing.iter().map(|(_, n)| n).sum();
        println!("pack objects of packs not on: {missing:?}");
        let m = trf("STR_MODS_MISSING", &[("N", &n), ("PACKS", &packs.join(", "))]);
        self.scen.messages.push_back(m);
    }

    /// The home-site bonus of the packs' objects and scripts, given to sg_core::homes.
    pub(crate) fn mod_values_changed(&mut self) {
        let n = land::mn();
        sg_core::modkit::set_value_grid(n, self.mods.world.value_grid(&self.mods.catalog, n));
    }

    /// The land as the scripts see it.
    pub(crate) fn mod_snapshot(&self) -> Snapshot {
        let edge = land::mn();
        let (ty, height, buildings) = match &self.land {
            Some(l) => (
                l.ty.clone(),
                l.height.clone(),
                l.objects
                    .iter()
                    .filter(|o| o.kind >= 0)
                    .map(|o| {
                        let home = o.kind == land::K_HOME_SITE;
                        BuildingView {
                            kind: sg_core::data::object_id(o.kind as usize).to_string(),
                            x: o.a,
                            y: o.b,
                            size: l.footprint_size(o),
                            level: o.sub,
                            value: if home { o.val } else { 0 },
                            owner: if home { o.sub } else { 0 },
                        }
                    })
                    .collect(),
            ),
            None => Default::default(),
        };
        Snapshot {
            edge,
            ty,
            height,
            course: Some(self.course.clone()),
            holes: self.club.holes.clone(),
            difficulty: self.difficulty,
            buildings,
            objects: self.mods.world.objects.clone(),
            next_uid: self.mods.world.next.max(1),
            types: self.mods.catalog.objects.iter().map(|o| (o.def.id.clone(), o.def.name.clone(), o.pack.clone())).collect(),
        }
    }

    // ---- placing and removing ------------------------------------------------------------------------------------------

    /// `Action::ModBuild`: the player places one of the packs' types with the footprint round tile x, y (or, `remove`, takes
    /// the pack object on the tile away).
    pub fn mod_build(&mut self, x: i32, y: i32, kind: &str, turn: i32, remove: bool) -> bool {
        if self.rec.depth_zero() {
            return self.act(Action::ModBuild { x, y, kind: kind.to_string(), turn, remove });
        }
        if !self.terrain.inside(x, y) {
            return false;
        }
        self.ensure_land();
        if remove {
            return self.mod_take_back(x, y);
        }
        let Some(t) = self.mods.catalog.get(kind).cloned() else { return false };
        let holes = self.hole_numbers.len();
        if t.def.holes > holes as i32 {
            self.ui_sound(24);
            self.show_toast(&trf("STR_MODS_NEEDS_HOLES", &[("N", &t.def.holes)]));
            return false;
        }
        let dir = (turn & 3) as u8;
        let (w, h) = World::size(&t.def, dir);
        let (a, b) = (x - (w - 1) / 2, y - (h - 1) / 2);
        let Some(l) = self.land.as_mut() else { return false };
        l.sync_from_terrain(&self.terrain);
        if let Err(r) = self.mods.world.fits(l, &t.def, a, b, dir) {
            self.ui_sound(24);
            self.show_toast(refusal_text(r));
            return false;
        }
        let cost = t.def.price as f64;
        if !self.econ.affordable(cost, holes) {
            self.show_toast(&trf("STR_TOAST_CHANGE_COSTS", &[("COST", &money(cost as i64)), ("CASH", &money(self.econ.cash as i64))]));
            return false;
        }
        let cash = self.econ.cash;
        let uid = self.place_mod(&t, a, b, dir, None);
        self.econ.spend_to(economy::LEDGER_FACILITIES, cost);
        let mid = self.mods.world.get(uid).map(|o| o.middle()).unwrap_or((x, y));
        self.float_change(cash, mid);
        self.snd("Interface/Building.wav", 0.7, false);
        self.flush_mods();
        true
    }

    /// Puts a type down (checked already) and tells the scripts. Returns the uid.
    fn place_mod(&mut self, t: &ObjType, a: i32, b: i32, dir: u8, uid: Option<u32>) -> u32 {
        let Some(l) = self.land.as_mut() else { return 0 };
        let uid = match uid {
            Some(u) => self.mods.world.place_as(l, t, a, b, dir, u),
            None => self.mods.world.place(l, t, a, b, dir),
        };
        let (w, h) = World::size(&t.def, dir);
        l.write_area(&mut self.terrain, a - 1, b - 1, a + w, b + h);
        self.mod_objects_changed();
        if let Some(o) = self.mods.world.get(uid).cloned() {
            self.script_hook(Hook::ObjectPlaced(o));
        }
        uid
    }

    /// Takes a pack object away (refunding it when the player does) and tells the scripts.
    fn remove_mod(&mut self, uid: u32, refund: bool) -> Option<Placed> {
        let l = self.land.as_mut()?;
        let o = self.mods.world.remove(l, uid)?;
        l.write_area(&mut self.terrain, o.a - 1, o.b - 1, o.a + o.w, o.b + o.h);
        if refund {
            if let Some(t) = self.mods.catalog.get(&o.kind) {
                let r = t.def.refund as f64;
                if r > 0.0 {
                    let cash = self.econ.cash;
                    self.econ.earn_to(economy::LEDGER_OTHER, r);
                    self.float_change(cash, o.middle());
                }
            }
        }
        self.mod_objects_changed();
        self.script_hook(Hook::ObjectRemoved(o.clone()));
        Some(o)
    }

    /// The Undo tool or Shift with the Mods tool on a tile of a pack object: it is taken away. False when there is none.
    pub(crate) fn mod_take_back(&mut self, x: i32, y: i32) -> bool {
        let Some(uid) = self.mods.world.at(x, y).map(|o| o.uid) else { return false };
        self.remove_mod(uid, true);
        self.snd("Interface/Building.wav", 0.5, false);
        self.flush_mods();
        true
    }

    fn mod_objects_changed(&mut self) {
        self.mod_values_changed();
        self.mods.dirty = true;
    }

    /// The course rebuilt once after the pack objects changed (a script may place many in one hook).
    pub(crate) fn flush_mods(&mut self) {
        if std::mem::take(&mut self.mods.dirty) {
            self.after_object_change();
        }
    }

    /// The packs' upkeep and income of the month (scenario_ui, at each month start).
    pub(crate) fn mod_month(&mut self) {
        if self.mods.world.objects.is_empty() {
            return;
        }
        let (mut paid, mut earned) = (0i64, 0i64);
        for o in &self.mods.world.objects {
            if let Some(t) = self.mods.catalog.get(&o.kind) {
                paid += t.def.upkeep;
                earned += t.def.income;
            }
        }
        if earned > 0 {
            self.econ.earn_to(economy::LEDGER_OTHER, earned as f64);
        }
        if paid > 0 {
            self.econ.spend_to(economy::LEDGER_MAINTENANCE, paid as f64);
        }
    }

    // ---- the Mods tool and the scripts' events ---------------------------------------------------------------------------

    /// A click (or, for a dragging tool, a drag) of the Mods tool on tile (a, b).
    pub fn mod_tool_click(&mut self, a: i32, b: i32, remove: bool, dragging: bool) {
        match self.mods.sel.clone() {
            Some(Sel::Build(id)) if !dragging => {
                self.mod_build(a, b, &id, self.turn, remove);
            }
            Some(Sel::Tool(pack, t)) if !dragging || t.drag => {
                self.act(Action::ModEvent { pack, event: "tool".into(), id: t.id, arg: String::new(), x: a, y: b, remove });
            }
            _ => {}
        }
    }

    /// A left click on the course with no tool: a pack object there is clicked, when a pack's scripts listen for that.
    pub(crate) fn mod_object_click(&mut self, x: i32, y: i32) -> bool {
        if self.mods.world.at(x, y).is_none() || !self.scen.host.defines("on_object_clicked") {
            return false;
        }
        self.act(Action::ModEvent {
            pack: String::new(),
            event: "click".into(),
            id: String::new(),
            arg: String::new(),
            x,
            y,
            remove: false,
        })
    }

    /// `Action::ModEvent`: what was done in a pack's part of the interface goes to its scripts, at once.
    #[allow(clippy::too_many_arguments)]
    pub fn mod_event(&mut self, pack: &str, event: &str, id: &str, arg: &str, x: i32, y: i32, remove: bool) -> bool {
        let (p, i, a) = (pack.to_string(), id.to_string(), arg.to_string());
        let hook = match event {
            "tool" => Hook::Tool { pack: p, tool: i, x, y, remove },
            "panel" => Hook::PanelOpen { pack: p, panel: i },
            "button" => Hook::PanelButton { pack: p, panel: i, button: a },
            "answer" => Hook::Answer { pack: p, question: i, button: a },
            "overlay" => Hook::OverlayOpen { pack: p, overlay: i },
            "click" => match self.mods.world.at(x, y) {
                Some(o) => Hook::ObjectClicked(o.clone()),
                None => return false,
            },
            _ => return false,
        };
        self.scen.pending.push(hook);
        self.run_script_hooks();
        true
    }

    /// Carries out what a pack's script asked for that concerns its objects, panels and map views (scenario_ui's
    /// `run_script_hooks` hands these over). True when the land changed.
    pub(crate) fn mod_script_action(&mut self, pack: &str, a: sg_core::script::Action) -> bool {
        use sg_core::script::Action as S;
        match a {
            S::Toast(t) => self.show_toast(&t),
            S::Place { kind, x, y, dir, uid } => {
                let Some(t) = self.mods.catalog.get(&kind).cloned() else { return false };
                self.ensure_land();
                let ok = self.land.as_ref().is_some_and(|l| self.mods.world.fits(l, &t.def, x, y, dir).is_ok());
                if !ok {
                    println!("script {pack}: place_object {kind} at {x},{y}: no room");
                    return false;
                }
                self.place_mod(&t, x, y, dir, Some(uid));
            }
            S::Remove(uid) => {
                self.remove_mod(uid, false);
            }
            S::SetLevel(uid, level) => {
                let max = self.mods.world.get(uid).and_then(|o| self.mods.catalog.get(&o.kind)).map(|t| t.def.levels - 1).unwrap_or(0);
                if let Some(o) = self.mods.world.get_mut(uid) {
                    o.level = level.clamp(0, max.max(0));
                    return true;
                }
            }
            S::SetData(uid, v) => {
                if let Some(o) = self.mods.world.get_mut(uid) {
                    o.data = v;
                }
            }
            S::SetTile { x, y, ty } => {
                self.ensure_land();
                let blocked = self.mods.world.blocks(x, y);
                let Some(l) = self.land.as_mut() else { return false };
                if !l.in_play(x, y) || blocked {
                    return false;
                }
                let i = (x * land::mn() + y) as usize;
                let old = l.ty[i];
                let busy = |t: u8| matches!(t, land::T_TEE | land::T_GREEN | land::T_BUILDING | land::T_HOME_SITE | land::T_OUT);
                if busy(old) || busy(ty) || l.flags[i] & (land::flag::FOOTPRINT | land::flag::OBSTACLE) != 0 {
                    return false;
                }
                l.ty[i] = ty;
                l.layer[i] = sg_core::data::land_type(ty).layer;
                l.write_area(&mut self.terrain, x - 1, y - 1, x + 1, y + 1);
                return true;
            }
            S::Bonus { x, y, amount } => {
                let n = land::mn();
                if (0..n).contains(&x) && (0..n).contains(&y) {
                    let k = (x * n + y) as u32;
                    if amount == 0 {
                        self.mods.world.bonus.remove(&k);
                    } else {
                        self.mods.world.bonus.insert(k, amount);
                    }
                    self.mod_values_changed();
                }
            }
            S::ClearBonus => {
                self.mods.world.bonus.clear();
                self.mod_values_changed();
            }
            S::Overlay { id, values, low, high, legend } => {
                let v = sg_core::modkit::OverlayValues { values, edge: land::mn(), low, high, legend };
                self.mods.world.overlays.insert(format!("{pack}/{id}"), v);
            }
            S::ShowPanel { id, spec } => {
                self.mods.panel = Some(Panel::from_spec(pack, &id, &spec));
                self.open_mod_panel();
            }
            S::ClosePanel => {
                if self.mods.panel.as_ref().is_some_and(|p| p.pack == pack) {
                    self.close_mod_panel();
                }
            }
            S::Ask { id, text, buttons } => {
                let buttons = if buttons.is_empty() { vec![tr("STR_MODS_OK").to_string()] } else { buttons };
                let title = self.pack_name(pack);
                self.mods.panel = Some(Panel {
                    pack: pack.to_string(),
                    id,
                    title,
                    rows: vec![Row::Text(text, None)],
                    question: true,
                    buttons: buttons.into_iter().map(|b| (b.clone(), b)).collect(),
                });
                self.open_mod_panel();
            }
            _ => {}
        }
        false
    }

    fn pack_name(&self, id: &str) -> String {
        self.content.found.packs.iter().find(|p| p.manifest.id == id).map(|p| p.manifest.name.clone()).unwrap_or_else(|| id.to_string())
    }

    // ---- drawing on the course -------------------------------------------------------------------------------------------

    /// The sprite of a pack's type (its sheet made into a sprite the first time it is drawn).
    fn mod_sprite(&mut self, t: &ObjType) -> Option<usize> {
        let key = format!("mod:{}:{}", t.pack, t.def.id);
        if let Some(v) = self.sprite_index.get(&key) {
            return *v;
        }
        let d = &t.def;
        let data = if d.art.is_empty() {
            None
        } else {
            self.content.found.packs.iter().find(|p| p.manifest.id == t.pack).and_then(|p| p.source.read(&d.art))
        };
        let r = match data {
            None => {
                if !d.art.is_empty() {
                    let w = format!("object {}: no art file {}", d.id, d.art);
                    crate::bugreport::warn(&format!("content pack {}: {w}", t.pack));
                    self.mods.warnings.push((t.pack.clone(), w));
                }
                None
            }
            Some(png) => match sg_core::hires::object_sprite(&png, d.views as u32, d.frames as u32, d.scale, d.anchor, d.frame_ms) {
                Ok((s, big)) => {
                    let n = s.frames.len();
                    if let Some(b) = big {
                        self.pack_art.insert(self.sprites.len(), b);
                    }
                    self.sprites.push(GlSprite { s, tex: vec![None; n] });
                    Some(self.sprites.len() - 1)
                }
                Err(e) => {
                    let w = format!("object {}: {}: {e}", d.id, d.art);
                    crate::bugreport::warn(&format!("content pack {}: {w}", t.pack));
                    self.mods.warnings.push((t.pack.clone(), w));
                    None
                }
            },
        };
        self.sprite_index.insert(key, r);
        r
    }

    /// The packs' objects as sprites on the course (add_land_objects).
    pub(crate) fn add_mod_props(&mut self) {
        if self.mods.world.objects.is_empty() {
            return;
        }
        let objects = self.mods.world.objects.clone();
        for o in &objects {
            let Some(t) = self.mods.catalog.get(&o.kind).cloned() else { continue };
            let Some(si) = self.mod_sprite(&t) else { continue };
            let d = &t.def;
            let per = (d.frames / d.levels.max(1)).max(1);
            let frame = if d.join { self.mods.world.join_bits(o) as i32 } else { o.level.clamp(0, d.levels - 1) * per };
            let (x0, z0) = self.terrain.tile_centre(o.a, o.b);
            self.props.push(Prop {
                x: x0 + (o.w - 1) as f32 * 0.5 * TILE_SIZE,
                z: z0 + (o.h - 1) as f32 * 0.5 * TILE_SIZE,
                body: Some(si),
                frame,
                facing: o.dir as i32,
                object: true,
                flat: d.ground,
                animated: d.animated,
                ..Default::default()
            });
        }
    }

    /// The map view switched on, and the Mods tool's footprint under the pointer (draw_hud).
    pub fn draw_mod_views(&mut self, g: &mut Gfx, s: &Ui) {
        let corner = |app: &App, x: i32, y: i32| {
            let (wx, wz) = app.units_to_world(x, y);
            app.screen_of_world(wx, wz)
        };
        let quad = |app: &App, g: &mut Gfx, a: i32, b: i32, col: [f32; 4]| {
            let (x0, y0, x1, y1) = (a * 1024, b * 1024, a * 1024 + 1024, b * 1024 + 1024);
            if let (Some(p0), Some(p1), Some(p2), Some(p3)) =
                (corner(app, x0, y0), corner(app, x1, y0), corner(app, x1, y1), corner(app, x0, y1))
            {
                s.quad(g, [p0, p1, p2, p3], col);
            }
        };
        if let Some((pack, id)) = self.mods.overlay.clone() {
            let def = self.mods.catalog.overlays.iter().find(|(p, o)| *p == pack && o.id == id).map(|(_, o)| o.clone());
            let vals = self.mods.world.overlays.get(&format!("{pack}/{id}")).cloned();
            if let (Some(def), Some(v)) = (def, vals) {
                let lo = sg_core::modkit::colour(&def.low).unwrap_or([0.8, 0.1, 0.1, 0.6]);
                let hi = sg_core::modkit::colour(&def.high).unwrap_or([0.1, 0.8, 0.2, 0.6]);
                let span = (v.high - v.low) as f32;
                for a in 0..v.edge.min(land::mn()) {
                    for b in 0..v.edge.min(land::mn()) {
                        let Some(&x) = v.values.get((a * v.edge + b) as usize) else { continue };
                        if x == i32::MIN {
                            continue;
                        }
                        let k = if span == 0.0 { 1.0 } else { ((x - v.low) as f32 / span).clamp(0.0, 1.0) };
                        let c = [0, 1, 2, 3].map(|i| lo[i] + (hi[i] - lo[i]) * k);
                        quad(self, g, a, b, c);
                    }
                }
                let legend = if v.legend.is_empty() { def.name.clone() } else { format!("{}: {}", def.name, v.legend) };
                s.text_centered(g, 400.0, 34.0, &legend, 14.0, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        if self.edit && self.tool == TOOL && self.has_hit {
            let (x, y) = self.terrain.tile_of(self.hit_x, self.hit_z);
            match self.mods.sel.clone() {
                Some(Sel::Build(id)) => {
                    let Some(t) = self.mods.catalog.get(&id).cloned() else { return };
                    let dir = (self.turn & 3) as u8;
                    let (w, h) = World::size(&t.def, dir);
                    let (a, b) = (x - (w - 1) / 2, y - (h - 1) / 2);
                    let ok = self.land.as_ref().is_some_and(|l| self.mods.world.fits(l, &t.def, a, b, dir).is_ok());
                    let col = if ok { [0.2, 0.9, 0.3, 0.35] } else { [0.9, 0.15, 0.1, 0.4] };
                    for i in a..a + w {
                        for j in b..b + h {
                            quad(self, g, i, j, col);
                        }
                    }
                    let line = trf("STR_MODS_PLACING", &[("NAME", &t.def.name), ("COST", &money(t.def.price))]);
                    s.text_centered(g, 400.0, 52.0, &line, 13.0, [1.0, 1.0, 0.85, 1.0]);
                }
                Some(Sel::Tool(_, t)) => {
                    let [w, h] = t.footprint;
                    let (a, b) = (x - (w - 1) / 2, y - (h - 1) / 2);
                    for i in a..a + w {
                        for j in b..b + h {
                            quad(self, g, i, j, [1.0, 0.9, 0.2, 0.35]);
                        }
                    }
                    s.text_centered(g, 400.0, 52.0, &t.name, 13.0, [1.0, 1.0, 0.85, 1.0]);
                }
                None => {}
            }
        }
    }

    // ---- the Mods window -------------------------------------------------------------------------------------------------

    fn mod_items(&self) -> Vec<Item> {
        let c = &self.mods.catalog;
        let mut v = Vec::new();
        let groups = c.build_groups(self.hole_numbers.len() as i32);
        for (name, types) in groups {
            v.push(Item::Heading(if name.is_empty() { tr("STR_MODS_BUILD").to_string() } else { name }));
            v.extend(types.into_iter().map(|t| Item::Build(t.def.id.clone())));
        }
        if !c.tools.is_empty() {
            v.push(Item::Heading(tr("STR_MODS_TOOLS").to_string()));
            v.extend((0..c.tools.len()).map(Item::Tool));
        }
        if !c.panels.is_empty() {
            v.push(Item::Heading(tr("STR_MODS_PANELS").to_string()));
            v.extend((0..c.panels.len()).map(Item::Panel));
        }
        if !c.overlays.is_empty() {
            v.push(Item::Heading(tr("STR_MODS_VIEWS").to_string()));
            v.extend((0..c.overlays.len()).map(Item::Overlay));
            if self.mods.overlay.is_some() {
                v.push(Item::OverlayOff);
            }
        }
        let missing = self.mods.world.missing(c);
        let warnings: Vec<(String, String)> =
            self.mods.warnings.iter().cloned().chain(self.scen.errors.iter().rev().take(5).cloned()).collect();
        if !missing.is_empty() || !warnings.is_empty() {
            v.push(Item::Heading(tr("STR_MODS_PROBLEMS").to_string()));
            v.extend(missing.into_iter().map(|(p, n)| Item::Missing(p, n)));
            v.extend(warnings.into_iter().map(|(p, w)| Item::Warning(p, w)));
        }
        v.push(Item::Heading(tr("STR_MODS_MAKERS").to_string()));
        v.push(Item::Reload);
        v
    }

    pub fn open_mods(&mut self) {
        self.mods.scroll = 0;
        self.screen = Screen::Mods;
        self.hover = -1;
    }

    pub fn draw_mods(&mut self, g: &mut Gfx) {
        use crate::info_ui::c15;
        let s = Ui::new(self.draw_w, self.draw_h);
        self.view = s.view;
        crate::info_ui::dim(g, &s);
        self.draw_info_frame(g, &s, BOX.x, BOX.y, BOX.w, BOX.h);
        let (white, yellow, navy, red, dark) = (c15(0x7fff), c15(0x7ff0), c15(0x0018), rgb(0.75, 0.1, 0.1), c15(0x2108));
        s.put_shadowed(g, F_MANUAL20, 400.0, BOX.y + 14.0, tr("STR_MODS_TITLE"), yellow, true);
        let (px, py) = self.info.pointer;
        let items = self.mod_items();
        let c = self.mods.catalog.clone();
        for (k, it) in items.iter().enumerate().skip(self.mods.scroll).take(ROWS) {
            let y = TOP + (k - self.mods.scroll) as f32 * ROW_H;
            let clickable = !matches!(it, Item::Heading(_) | Item::Missing(..) | Item::Warning(..));
            if clickable && Rect::new(BOX.x + 10.0, y, BOX.w - 20.0, ROW_H - 2.0).has(px, py) {
                s.fill(g, BOX.x + 12.0, y, BOX.w - 24.0, ROW_H - 2.0, rgb(0.66, 0.66, 0.84));
            }
            let w = BOX.w - 80.0;
            match it {
                Item::Heading(h) => s.put(g, F_MANUAL15, BOX.x + 24.0, y + 3.0, h, white),
                Item::Build(id) => {
                    let Some(t) = c.get(id) else { continue };
                    let [fw, fh] = t.def.footprint;
                    let mut line =
                        trf("STR_MODS_BUILD_LINE", &[("NAME", &t.def.name), ("COST", &money(t.def.price)), ("W", &fw), ("H", &fh)]);
                    if !t.def.description.is_empty() {
                        line = format!("{line}  -  {}", t.def.description);
                    }
                    let held = self.mods.sel == Some(Sel::Build(id.clone()));
                    s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, &crate::content::fit(&line, w), if held { red } else { navy });
                }
                Item::Tool(i) => {
                    let (_, t) = &c.tools[*i];
                    let line = if t.description.is_empty() { t.name.clone() } else { format!("{}  -  {}", t.name, t.description) };
                    s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, &crate::content::fit(&line, w), navy);
                }
                Item::Panel(i) => {
                    let (_, p) = &c.panels[*i];
                    let line = if p.description.is_empty() { p.name.clone() } else { format!("{}  -  {}", p.name, p.description) };
                    s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, &crate::content::fit(&line, w), navy);
                }
                Item::Overlay(i) => {
                    let (pack, o) = &c.overlays[*i];
                    let on = self.mods.overlay.as_ref() == Some(&(pack.clone(), o.id.clone()));
                    let mut line = if o.description.is_empty() { o.name.clone() } else { format!("{}  -  {}", o.name, o.description) };
                    if on {
                        line = trf("STR_MODS_VIEW_ON", &[("NAME", &line)]);
                    }
                    s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, &crate::content::fit(&line, w), if on { red } else { navy });
                }
                Item::OverlayOff => s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, tr("STR_MODS_VIEW_OFF"), navy),
                Item::Missing(p, n) => {
                    let line = trf("STR_MODS_MISSING_LINE", &[("PACK", p), ("N", n)]);
                    s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, &crate::content::fit(&line, w), red);
                }
                Item::Warning(p, m) => {
                    s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, &crate::content::fit(&format!("{p}: {m}"), w), red);
                }
                Item::Reload => s.put(g, F_ARIAL10, BOX.x + 44.0, y + 6.0, tr("STR_MODS_RELOAD"), navy),
            }
        }
        let below = TOP + ROWS as f32 * ROW_H;
        if items.len() > ROWS {
            for (dx, label, ok) in [
                (24.0, tr("STR_CONTENT_PREVIOUS"), self.mods.scroll > 0),
                (110.0, tr("STR_CONTENT_NEXT"), self.mods.scroll + ROWS < items.len()),
            ] {
                s.fill(g, BOX.x + dx, below + 4.0, 80.0, 20.0, rgb(0.47, 0.47, 0.70));
                s.put_centered(g, F_ARIAL10, BOX.x + dx + 40.0, below + 8.0, label, if ok { navy } else { dark });
            }
        }
        s.put(g, F_ARIAL10, BOX.x + 24.0, BOX.y + BOX.h - 26.0, tr("STR_MODS_HELP"), navy);
        g.flush();
    }

    pub fn mods_click(&mut self, x: f32, y: f32) {
        let items = self.mod_items();
        let below = TOP + ROWS as f32 * ROW_H;
        if items.len() > ROWS && (below + 4.0..below + 24.0).contains(&y) {
            if (BOX.x + 24.0..BOX.x + 104.0).contains(&x) {
                self.mods.scroll = self.mods.scroll.saturating_sub(ROWS);
            } else if (BOX.x + 110.0..BOX.x + 190.0).contains(&x) && self.mods.scroll + ROWS < items.len() {
                self.mods.scroll += ROWS;
            }
            return;
        }
        if !BOX.has(x, y) {
            self.screen = Screen::Play;
            return;
        }
        if y < TOP || y >= below {
            return;
        }
        let k = self.mods.scroll + ((y - TOP) / ROW_H) as usize;
        let Some(it) = items.get(k).cloned() else { return };
        self.mod_item(it);
    }

    fn hold_mod_tool(&mut self, sel: Sel, name: &str) {
        self.mods.sel = Some(sel);
        self.tool = TOOL;
        self.edit = true;
        self.screen = Screen::Play;
        self.show_toast(&trf("STR_MODS_TOOL_HELD", &[("NAME", &name)]));
    }

    fn mod_item(&mut self, it: Item) {
        let c = self.mods.catalog.clone();
        match it {
            Item::Build(id) => {
                let name = c.get(&id).map(|t| t.def.name.clone()).unwrap_or_default();
                self.hold_mod_tool(Sel::Build(id), &name);
            }
            Item::Tool(i) => {
                let (pack, t) = c.tools[i].clone();
                let name = t.name.clone();
                self.hold_mod_tool(Sel::Tool(pack, t), &name);
            }
            Item::Panel(i) => {
                let (pack, p) = c.panels[i].clone();
                self.screen = Screen::Play;
                self.act(Action::ModEvent { pack, event: "panel".into(), id: p.id, arg: String::new(), x: 0, y: 0, remove: false });
            }
            Item::Overlay(i) => {
                let (pack, o) = c.overlays[i].clone();
                self.mods.overlay = Some((pack.clone(), o.id.clone()));
                self.screen = Screen::Play;
                self.act(Action::ModEvent { pack, event: "overlay".into(), id: o.id, arg: String::new(), x: 0, y: 0, remove: false });
            }
            Item::OverlayOff => {
                self.mods.overlay = None;
                self.screen = Screen::Play;
            }
            Item::Reload => self.reload_mods(),
            _ => {}
        }
    }

    pub fn mods_key(&mut self, k: miniquad::KeyCode) {
        use miniquad::KeyCode;
        let n = self.mod_items().len();
        match k {
            KeyCode::PageDown | KeyCode::Down if self.mods.scroll + ROWS < n => self.mods.scroll += ROWS,
            KeyCode::PageUp | KeyCode::Up => self.mods.scroll = self.mods.scroll.saturating_sub(ROWS),
            KeyCode::Escape | KeyCode::Enter | KeyCode::Space => self.screen = Screen::Play,
            _ => {}
        }
    }

    /// The Mods window's Reload (for pack makers): the packs are looked for again, and the scripts, objects and art of
    /// the game's packs read again, keeping the game and the scripts' state.
    pub fn reload_mods(&mut self) {
        self.content.rediscover();
        let saved = self.scripts_for_save();
        let errors_before = self.scen.errors.len();
        self.scen.errors.clear();
        self.reload_game_scripts(&saved);
        self.after_object_change();
        self.screen = Screen::Play;
        let problems = self.mods.warnings.len() + self.scen.errors.len();
        println!("mods reloaded: {} types, {problems} problems (before: {errors_before} script errors)", self.mods.catalog.objects.len());
        if problems == 0 {
            self.show_toast(tr("STR_MODS_RELOADED"));
        } else {
            self.show_toast(&trf("STR_MODS_RELOADED_PROBLEMS", &[("N", &problems)]));
        }
    }

    // ---- a script's panel or question ------------------------------------------------------------------------------------

    fn open_mod_panel(&mut self) {
        // over the game only: a panel asked for while another screen is up waits for the game
        if matches!(self.screen, Screen::Play | Screen::Mods | Screen::ModPanel) {
            self.screen = Screen::ModPanel;
            self.hover = -1;
        }
    }

    pub(crate) fn close_mod_panel(&mut self) {
        self.mods.panel = None;
        if self.screen == Screen::ModPanel {
            self.screen = Screen::Play;
        }
    }

    /// Back to the game with a panel still to show: it is shown (main.rs, as the game comes back).
    pub fn mod_panel_waiting(&mut self) {
        if self.screen == Screen::Play && self.mods.panel.is_some() {
            self.screen = Screen::ModPanel;
        }
    }

    /// The panel's box and its rows' and bottom buttons' places.
    fn panel_layout(p: &Panel) -> (Rect, Vec<f32>, Vec<Rect>) {
        let mut h = 70.0;
        let mut ys = Vec::new();
        for r in &p.rows {
            ys.push(h);
            h += match r {
                Row::Heading(_) => 26.0,
                Row::Bar { .. } => 30.0,
                Row::Button { .. } => 28.0,
                Row::Gap => 12.0,
                Row::Text(t, _) => 16.0 * crate::ui::wrap_text(t, 10.0, 440.0).len().max(1) as f32 + 4.0,
            };
        }
        h += 50.0;
        let h = h.min(560.0);
        let b = Rect::new(400.0 - 260.0, 300.0 - h / 2.0, 520.0, h);
        let mut buttons: Vec<(String, String)> = p.buttons.clone();
        if !p.question {
            buttons.push(("".into(), tr("STR_MODS_CLOSE").to_string()));
        }
        let n = buttons.len() as f32;
        let bw = 110.0;
        let x0 = b.x + b.w / 2.0 - (n * bw + (n - 1.0) * 10.0) / 2.0;
        let rects = (0..buttons.len()).map(|i| Rect::new(x0 + i as f32 * (bw + 10.0), b.y + b.h - 38.0, bw, 24.0)).collect();
        let ys = ys.into_iter().map(|y| b.y + y).collect();
        (b, ys, rects)
    }

    pub fn draw_mod_panel(&mut self, g: &mut Gfx) {
        use crate::info_ui::c15;
        let Some(p) = self.mods.panel.clone() else {
            self.screen = Screen::Play;
            return;
        };
        let s = Ui::new(self.draw_w, self.draw_h);
        self.view = s.view;
        crate::info_ui::dim(g, &s);
        let (b, ys, rects) = App::panel_layout(&p);
        self.draw_info_frame(g, &s, b.x, b.y, b.w, b.h);
        let (white, yellow, navy, grey) = (c15(0x7fff), c15(0x7ff0), c15(0x0018), c15(0x4210));
        s.put_shadowed(g, F_MANUAL20, 400.0, b.y + 16.0, &p.title, yellow, true);
        let (px, py) = self.info.pointer;
        for (r, &y) in p.rows.iter().zip(&ys) {
            if y > b.y + b.h - 60.0 {
                break;
            }
            match r {
                Row::Heading(t) => s.put(g, F_MANUAL15, b.x + 30.0, y + 2.0, &crate::content::fit(t, b.w - 60.0), white),
                Row::Text(t, c) => {
                    for (k, l) in crate::ui::wrap_text(t, 10.0, 440.0).iter().enumerate() {
                        s.put(g, F_ARIAL10, b.x + 30.0, y + 2.0 + k as f32 * 16.0, l, c.unwrap_or(navy));
                    }
                }
                Row::Bar { label, value, max } => {
                    let k = if *max > 0.0 { (value / max).clamp(0.0, 1.0) as f32 } else { 0.0 };
                    s.put(g, F_ARIAL10, b.x + 30.0, y + 2.0, &crate::content::fit(label, 160.0), navy);
                    s.fill(g, b.x + 200.0, y + 2.0, 280.0, 14.0, rgb(0.3, 0.3, 0.45));
                    s.fill(g, b.x + 200.0, y + 2.0, 280.0 * k, 14.0, rgb(0.2, 0.75, 0.3));
                }
                Row::Button { label, enabled, .. } => {
                    let r = Rect::new(b.x + 30.0, y, 260.0, 22.0);
                    crate::glass_ui::button_fill(g, &s, r.x, r.y, r.w, r.h, *enabled && r.has(px, py));
                    s.put_centered(
                        g,
                        F_ARIAL10,
                        r.x + r.w / 2.0,
                        r.y + 5.0,
                        &crate::content::fit(label, 250.0),
                        if *enabled { navy } else { grey },
                    );
                }
                Row::Gap => {}
            }
        }
        let mut labels: Vec<String> = p.buttons.iter().map(|(_, l)| l.clone()).collect();
        if !p.question {
            labels.push(tr("STR_MODS_CLOSE").to_string());
        }
        for (r, l) in rects.iter().zip(labels) {
            crate::glass_ui::button_fill(g, &s, r.x, r.y, r.w, r.h, r.has(px, py));
            s.put_centered(g, F_ARIAL10, r.x + r.w / 2.0, r.y + 6.0, &crate::content::fit(&l, r.w - 8.0), navy);
        }
        g.flush();
    }

    pub fn mod_panel_click(&mut self, x: f32, y: f32) {
        let Some(p) = self.mods.panel.clone() else { return };
        let (_, ys, rects) = App::panel_layout(&p);
        let (b, _, _) = App::panel_layout(&p);
        for (r, &ry) in p.rows.iter().zip(&ys) {
            if let Row::Button { id, enabled: true, .. } = r {
                if Rect::new(b.x + 30.0, ry, 260.0, 22.0).has(x, y) {
                    self.act(Action::ModEvent {
                        pack: p.pack.clone(),
                        event: "button".into(),
                        id: p.id.clone(),
                        arg: id.clone(),
                        x: 0,
                        y: 0,
                        remove: false,
                    });
                    return;
                }
            }
        }
        for (i, r) in rects.iter().enumerate() {
            if !r.has(x, y) {
                continue;
            }
            match p.buttons.get(i) {
                Some((id, _)) => {
                    let event = if p.question { "answer" } else { "button" };
                    if p.question {
                        self.close_mod_panel();
                    }
                    self.act(Action::ModEvent {
                        pack: p.pack.clone(),
                        event: event.into(),
                        id: p.id.clone(),
                        arg: id.clone(),
                        x: 0,
                        y: 0,
                        remove: false,
                    });
                }
                None => self.close_mod_panel(),
            }
            return;
        }
    }

    pub fn mod_panel_key(&mut self, k: miniquad::KeyCode) {
        let question = self.mods.panel.as_ref().is_some_and(|p| p.question);
        if !question && matches!(k, miniquad::KeyCode::Escape | miniquad::KeyCode::Enter) {
            self.close_mod_panel();
        }
    }
}
