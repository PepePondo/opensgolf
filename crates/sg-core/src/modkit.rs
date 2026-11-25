//! What content packs add to the game beyond art, text and the data tables (docs/MODDING.md, section 12): new object types
//! with their own art, tools, panels and map views, and the record of the objects placed in a game. The port's own.
//!
//! A pack's `data/objects.toml` lists `[[object]]`, `[[tool]]`, `[[panel]]` and `[[overlay]]` tables. The objects are
//! placed by the player (the Mods window's Build list) or by the pack's scripts; their behaviour is the pack's scripts
//! (crate::script). A placed object stands on the land like a building: its tiles are building tiles (golfers walk round them,
//! nothing is painted over them) unless the type is `ground` (a road, a lot, a flat sign), whose tiles keep their type.
//!
//! The objects are kept apart from the land's own object table, which holds the original's 20 kinds in its own layout, so
//! nothing the original's rules do with that table ever meets a pack's object. A game keeps its pack objects when the pack
//! is switched off later: they stay on the land (and are listed as missing) until the pack is back or they are removed.
use crate::land::{flag, mn, Land, T_BUILDING, T_GREEN, T_HOME_SITE, T_OUT, T_TEE, T_WATER, UNDO_NONE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// The largest footprint edge.
pub const MAX_EDGE: i32 = 16;
/// The most objects a game holds.
pub const MAX_OBJECTS: usize = 20_000;
/// The most levels an object type has.
pub const MAX_LEVELS: i32 = 16;

fn one() -> i32 {
    1
}
fn one_u() -> u32 {
    1
}
fn edge_one() -> [i32; 2] {
    [1, 1]
}
fn frame_ms() -> u32 {
    120
}
fn radius() -> i32 {
    4
}

/// An `[[object]]` of data/objects.toml.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectDef {
    /// Its id, unique among the enabled packs (a later pack's entry with the same id replaces an earlier one's).
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The Build list's group it is shown in.
    #[serde(default)]
    pub category: String,
    /// Width and depth in tiles (turned objects swap them).
    #[serde(default = "edge_one")]
    pub footprint: [i32; 2],
    /// What placing it costs, and each level up (money as shown, as in the other data files).
    #[serde(default)]
    pub price: i64,
    #[serde(default)]
    pub upgrade_price: i64,
    /// Paid each month for each one standing, and earned each month for each one standing (booked as Other).
    #[serde(default)]
    pub upkeep: i64,
    #[serde(default)]
    pub income: i64,
    /// Given back when it is removed.
    #[serde(default)]
    pub refund: i64,
    /// The sprite sheet (a PNG in the pack): `views` rows (1, or 4 for the four facings) of `frames` columns.
    #[serde(default)]
    pub art: String,
    #[serde(default = "one")]
    pub views: i32,
    #[serde(default = "one")]
    pub frames: i32,
    /// The pixel of a frame that sits on the middle of the footprint (default: the middle of the frame's bottom edge).
    #[serde(default)]
    pub anchor: Option<[i32; 2]>,
    /// The sheet is drawn this many times the original's size (1, 2 or 4).
    #[serde(default = "one_u")]
    pub scale: u32,
    /// The frames play in turn, `frame_ms` each.
    #[serde(default)]
    pub animated: bool,
    #[serde(default = "frame_ms")]
    pub frame_ms: u32,
    /// Levels (1 .. 16): the frames are split evenly between them (level 0 the first share).
    #[serde(default = "one")]
    pub levels: i32,
    /// A flat object (a road, a lot, a sign on the ground): its tiles keep their type, golfers walk over it, and it is drawn
    /// on the ground under everything.
    #[serde(default)]
    pub ground: bool,
    /// A ground object's tiles take this terrain (e.g. "rough", which also clears trees); their own comes back when it is
    /// removed. Empty: the tiles keep their terrain.
    #[serde(default)]
    pub terrain: String,
    /// Pieces that join their neighbours of the same type (roads, fences): the sheet has 16 frames, frame = the bits of the
    /// neighbours of the same type (1 north (y - 1), 2 east (x + 1), 4 south, 8 west).
    #[serde(default)]
    pub join: bool,
    /// It may only stand on water (a jetty, a fountain), or also on water.
    #[serde(default)]
    pub on_water: bool,
    #[serde(default)]
    pub water_only: bool,
    /// What it adds to (or takes from) the value of home sites within `view_radius` tiles.
    #[serde(default)]
    pub view: i32,
    #[serde(default = "radius")]
    pub view_radius: i32,
    /// At most this many may stand (0: any number).
    #[serde(default)]
    pub limit: i32,
    /// Shown in the Build list only once this many holes are open.
    #[serde(default)]
    pub holes: i32,
    /// Not in the Build list: only the pack's scripts place it.
    #[serde(default)]
    pub hidden: bool,
}

/// A `[[tool]]`: a tool in the Mods window whose clicks on the course go to the pack's `on_tool(tool, x, y, remove)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The tiles the cursor frames.
    #[serde(default = "edge_one")]
    pub footprint: [i32; 2],
    /// A drag calls it on every tile passed.
    #[serde(default)]
    pub drag: bool,
}

/// A `[[panel]]`: a line in the Mods window that calls the pack's `on_panel_open(panel)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// An `[[overlay]]`: a map view in the Mods window, its values set by the pack's scripts (`set_overlay`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The colours of the lowest and the highest value ("#rrggbb" or "#rrggbbaa").
    #[serde(default = "low_colour")]
    pub low: String,
    #[serde(default = "high_colour")]
    pub high: String,
}

fn low_colour() -> String {
    "#d02020a0".into()
}
fn high_colour() -> String {
    "#20d040a0".into()
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    object: Vec<ObjectDef>,
    #[serde(default)]
    tool: Vec<ToolDef>,
    #[serde(default)]
    panel: Vec<PanelDef>,
    #[serde(default)]
    overlay: Vec<OverlayDef>,
}

/// An object type with the pack it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjType {
    pub pack: String,
    pub def: ObjectDef,
}

/// Everything the enabled packs add, in load order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub objects: Vec<ObjType>,
    pub tools: Vec<(String, ToolDef)>,
    pub panels: Vec<(String, PanelDef)>,
    pub overlays: Vec<(String, OverlayDef)>,
}

/// Whether an id is one this file accepts: letters, digits, `-` and `_`, 1 to 64 long.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

fn check(d: &ObjectDef) -> Result<(), String> {
    if !valid_id(&d.id) {
        return Err(format!("object id '{}': use letters, digits, - and _", d.id));
    }
    let e = |m: &str| Err(format!("object {}: {m}", d.id));
    if d.footprint.iter().any(|&v| !(1..=MAX_EDGE).contains(&v)) {
        return e(&format!("footprint must be 1 to {MAX_EDGE} tiles each way"));
    }
    if ![1, 4].contains(&d.views) {
        return e("views must be 1 or 4");
    }
    if !(1..=256).contains(&d.frames) {
        return e("frames must be 1 to 256");
    }
    if ![1, 2, 4].contains(&d.scale) {
        return e("scale must be 1, 2 or 4");
    }
    if !(1..=MAX_LEVELS).contains(&d.levels) || d.frames % d.levels != 0 {
        return e("levels must be 1 to 16 and divide the frames evenly");
    }
    if d.animated && d.levels != 1 {
        return e("an animated object has one level");
    }
    if d.join && (d.frames != 16 || d.levels != 1) {
        return e("a joining object has 16 frames and one level");
    }
    for (what, v) in
        [("price", d.price), ("upgrade_price", d.upgrade_price), ("upkeep", d.upkeep), ("income", d.income), ("refund", d.refund)]
    {
        if !(0..=100_000_000).contains(&v) {
            return e(&format!("{what} must be 0 to 100,000,000"));
        }
    }
    if !(-1000..=1000).contains(&d.view) || !(0..=16).contains(&d.view_radius) {
        return e("view must be -1000 to 1000 and view_radius 0 to 16");
    }
    if !(1..=60_000).contains(&d.frame_ms) {
        return e("frame_ms must be 1 to 60000");
    }
    if !d.terrain.is_empty() {
        match crate::data::terrain_index(&d.terrain) {
            None => return e(&format!("there is no terrain '{}'", d.terrain)),
            Some(t) if [T_TEE, T_GREEN, T_BUILDING, T_HOME_SITE, T_OUT].contains(&t) || !d.ground => {
                return e("terrain is for ground objects, and cannot be a tee, a green, a building, a home site or out of bounds")
            }
            _ => {}
        }
    }
    Ok(())
}

/// Parses a pack's data/objects.toml into the catalog (entries with an id already there replace it). Returns the problems:
/// a wrong entry is left out, the others are kept.
pub fn add_file(cat: &mut Catalog, pack: &str, text: &str) -> Vec<String> {
    let mut warn = Vec::new();
    let f: File = match toml::from_str(text) {
        Ok(f) => f,
        Err(e) => return vec![format!("data/objects.toml: {e}")],
    };
    for d in f.object {
        if let Err(m) = check(&d) {
            warn.push(format!("data/objects.toml: {m}"));
            continue;
        }
        cat.objects.retain(|o| o.def.id != d.id);
        cat.objects.push(ObjType { pack: pack.to_string(), def: d });
    }
    macro_rules! simple {
        ($list:expr, $into:expr, $what:literal) => {
            for d in $list {
                if !valid_id(&d.id) {
                    warn.push(format!("data/objects.toml: {} id '{}': use letters, digits, - and _", $what, d.id));
                    continue;
                }
                $into.retain(|(p, o)| !(p == pack && o.id == d.id));
                $into.push((pack.to_string(), d));
            }
        };
    }
    simple!(f.tool, cat.tools, "tool");
    simple!(f.panel, cat.panels, "panel");
    simple!(f.overlay, cat.overlays, "overlay");
    warn
}

impl Catalog {
    /// Reads data/objects.toml of each pack (id, source), in load order.
    pub fn load(packs: &[(String, &crate::vfs::Source)]) -> (Catalog, Vec<(String, String)>) {
        let mut cat = Catalog::default();
        let mut warn = Vec::new();
        for (id, src) in packs {
            let Some(name) = src.files().into_iter().find(|f| f.eq_ignore_ascii_case("data/objects.toml")) else { continue };
            let Some(data) = src.read(&name) else { continue };
            for w in add_file(&mut cat, id, &String::from_utf8_lossy(&data)) {
                warn.push((id.clone(), w));
            }
        }
        (cat, warn)
    }

    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.tools.is_empty() && self.panels.is_empty() && self.overlays.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&ObjType> {
        self.objects.iter().find(|o| o.def.id == id)
    }

    /// The Build list's groups, in order of first appearance, with their types (hidden ones and the ones locked by `holes`
    /// left out).
    pub fn build_groups(&self, open_holes: i32) -> Vec<(String, Vec<&ObjType>)> {
        let mut out: Vec<(String, Vec<&ObjType>)> = Vec::new();
        for o in self.objects.iter().filter(|o| !o.def.hidden && o.def.holes <= open_holes) {
            match out.iter_mut().find(|(c, _)| *c == o.def.category) {
                Some((_, v)) => v.push(o),
                None => out.push((o.def.category.clone(), vec![o])),
            }
        }
        out
    }
}

/// A pack's object on the land.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    /// Its number in the game, never used again.
    pub uid: u32,
    /// Its type's id, and the pack that had the type when it was placed.
    #[serde(rename = "type")]
    pub kind: String,
    pub pack: String,
    /// The footprint's first tile and its size as it stands (turned: width and depth swapped).
    pub a: i32,
    pub b: i32,
    pub w: i32,
    pub h: i32,
    /// Facing 0..3.
    #[serde(default)]
    pub dir: u8,
    #[serde(default)]
    pub level: i32,
    /// The pack's own fields for it (`set_object_data`), as JSON.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
    /// A building's tiles before it stood there (tile types), put back when it goes. Empty for a ground object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub under: Vec<u8>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ground: bool,
    /// The terrain a ground object gave its tiles (`terrain`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub laid: Option<u8>,
}

impl Placed {
    pub fn covers(&self, a: i32, b: i32) -> bool {
        (self.a..self.a + self.w).contains(&a) && (self.b..self.b + self.h).contains(&b)
    }

    /// The footprint's middle tile.
    pub fn middle(&self) -> (i32, i32) {
        (self.a + (self.w - 1) / 2, self.b + (self.h - 1) / 2)
    }
}

/// The values of one map view, set by its pack's scripts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OverlayValues {
    /// Per tile (a * edge + b); i32::MIN for none.
    pub values: Vec<i32>,
    pub edge: i32,
    /// The value drawn in the low colour and the value drawn in the high colour.
    pub low: i32,
    pub high: i32,
    /// A line shown over the map while the view is on.
    #[serde(default)]
    pub legend: String,
}

/// What a game keeps of the packs' objects (saved with it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct World {
    #[serde(default)]
    pub next: u32,
    #[serde(default)]
    pub objects: Vec<Placed>,
    /// What the scripts add to the value of a home site anchored on a tile (`set_land_value_bonus`), by tile (a * edge + b).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bonus: BTreeMap<u32, i32>,
    /// The map views' values, by "pack/overlay".
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overlays: BTreeMap<String, OverlayValues>,
}

/// Why an object cannot go somewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Off the land the club owns.
    OffLand,
    /// Something stands there (a building, a home site, another object) or a hole's tee or green is there.
    Taken,
    /// It needs water there, or no water.
    Water,
    /// As many stand as its type allows.
    Limit,
    /// The game holds as many objects as it can.
    Full,
}

impl World {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.bonus.is_empty() && self.overlays.is_empty()
    }

    pub fn get(&self, uid: u32) -> Option<&Placed> {
        self.objects.iter().find(|o| o.uid == uid)
    }

    pub fn get_mut(&mut self, uid: u32) -> Option<&mut Placed> {
        self.objects.iter_mut().find(|o| o.uid == uid)
    }

    /// The object on a tile (a building before a ground object under it).
    pub fn at(&self, a: i32, b: i32) -> Option<&Placed> {
        self.objects.iter().filter(|o| o.covers(a, b)).min_by_key(|o| o.ground)
    }

    /// Whether a pack's building stands on the tile (its tiles take no paint, path or other object).
    pub fn blocks(&self, a: i32, b: i32) -> bool {
        self.objects.iter().any(|o| !o.ground && o.covers(a, b))
    }

    /// The footprint of a type turned `dir`.
    pub fn size(def: &ObjectDef, dir: u8) -> (i32, i32) {
        let [w, h] = def.footprint;
        if dir & 1 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }

    /// Whether a type fits with its footprint's first tile at (a, b), turned `dir`.
    pub fn fits(&self, land: &Land, def: &ObjectDef, a: i32, b: i32, dir: u8) -> Result<(), Refusal> {
        if self.objects.len() >= MAX_OBJECTS {
            return Err(Refusal::Full);
        }
        if def.limit > 0 && self.objects.iter().filter(|o| o.kind == def.id).count() as i32 >= def.limit {
            return Err(Refusal::Limit);
        }
        let (w, h) = World::size(def, dir);
        for x in a..a + w {
            for y in b..b + h {
                if !land.in_play(x, y) {
                    return Err(Refusal::OffLand);
                }
                let i = (x * mn() + y) as usize;
                let t = land.ty[i];
                let f = land.flags[i];
                if t == T_BUILDING || t == T_HOME_SITE || f & (flag::FOOTPRINT | flag::OBSTACLE) != 0 {
                    return Err(Refusal::Taken);
                }
                if !def.ground && (t == T_TEE || t == T_GREEN) {
                    return Err(Refusal::Taken);
                }
                // a ground object may lie under nothing but another type's ground object
                if self.objects.iter().any(|o| o.covers(x, y) && (!o.ground || !def.ground || o.kind == def.id)) {
                    return Err(Refusal::Taken);
                }
                let wet = t == T_WATER;
                if (def.water_only && !wet) || (wet && !def.on_water && !def.water_only) {
                    return Err(Refusal::Water);
                }
            }
        }
        Ok(())
    }

    /// Puts an object down (after `fits`): a building's tiles become building tiles. Returns its uid.
    pub fn place(&mut self, land: &mut Land, ty: &ObjType, a: i32, b: i32, dir: u8) -> u32 {
        let uid = self.next.max(1);
        self.place_as(land, ty, a, b, dir, uid)
    }

    /// `place` with the uid a script was told (`place_object`); a uid already in use takes the next free one.
    pub fn place_as(&mut self, land: &mut Land, ty: &ObjType, a: i32, b: i32, dir: u8, uid: u32) -> u32 {
        let (w, h) = World::size(&ty.def, dir);
        let mut under = Vec::new();
        if !ty.def.ground {
            // a building stands level: the corners under it take the height of its first corner, as the original's do
            let hp = crate::mapsize::n() + 1;
            let hi = |x: i32, y: i32| (0..hp).contains(&x) && (0..hp).contains(&y);
            if hi(a, b - 1) {
                let level = land.height[(a * hp + b - 1) as usize];
                for x in a..=a + w {
                    for y in b - 1..b + h {
                        if hi(x, y) {
                            land.height[(x * hp + y) as usize] = level;
                        }
                    }
                }
            }
            for x in a..a + w {
                for y in b..b + h {
                    let i = (x * mn() + y) as usize;
                    under.push(land.ty[i]);
                    land.ty[i] = T_BUILDING;
                    land.layer[i] = crate::data::land_type(T_BUILDING).layer;
                    land.flags[i] = (land.flags[i] & !flag::PATH) | flag::FOOTPRINT;
                    land.var[i] = 0xff;
                    land.undo[i] = UNDO_NONE;
                }
            }
        } else if let Some(t) = crate::data::terrain_index(&ty.def.terrain) {
            for x in a..a + w {
                for y in b..b + h {
                    let i = (x * mn() + y) as usize;
                    under.push(land.ty[i]);
                    if land.ty[i] != T_WATER {
                        land.ty[i] = t;
                        land.layer[i] = crate::data::land_type(t).layer;
                        land.undo[i] = UNDO_NONE;
                    }
                }
            }
        }
        let uid = if uid == 0 || self.objects.iter().any(|o| o.uid == uid) { self.next.max(1) } else { uid };
        self.next = self.next.max(uid + 1);
        self.objects.push(Placed {
            uid,
            kind: ty.def.id.clone(),
            pack: ty.pack.clone(),
            a,
            b,
            w,
            h,
            dir,
            level: 0,
            data: serde_json::Value::Null,
            under,
            ground: ty.def.ground,
            laid: if ty.def.ground { crate::data::terrain_index(&ty.def.terrain) } else { None },
        });
        uid
    }

    /// Takes an object away: a building's tiles get their types back. Returns it.
    pub fn remove(&mut self, land: &mut Land, uid: u32) -> Option<Placed> {
        let i = self.objects.iter().position(|o| o.uid == uid)?;
        let o = self.objects.remove(i);
        if o.ground && !o.under.is_empty() {
            // a ground object's tiles get their terrain back, unless they were painted over since
            let mut k = 0;
            for x in o.a..o.a + o.w {
                for y in o.b..o.b + o.h {
                    let t = o.under.get(k).copied().unwrap_or(crate::land::T_ROUGH);
                    k += 1;
                    if !(0..mn()).contains(&x) || !(0..mn()).contains(&y) {
                        continue;
                    }
                    let i = (x * mn() + y) as usize;
                    if Some(land.ty[i]) == o.laid && land.flags[i] & flag::FOOTPRINT == 0 {
                        land.ty[i] = t;
                        land.layer[i] = crate::data::land_type(t).layer;
                    }
                }
            }
        } else if !o.ground {
            let mut k = 0;
            for x in o.a..o.a + o.w {
                for y in o.b..o.b + o.h {
                    let t = o.under.get(k).copied().unwrap_or(crate::land::T_ROUGH);
                    k += 1;
                    if !(0..mn()).contains(&x) || !(0..mn()).contains(&y) {
                        continue;
                    }
                    let i = (x * mn() + y) as usize;
                    land.ty[i] = if t == T_BUILDING || t == T_OUT { crate::land::T_ROUGH } else { t };
                    land.layer[i] = crate::data::land_type(land.ty[i]).layer;
                    land.flags[i] &= !flag::FOOTPRINT;
                    land.var[i] = if land.ty[i] == T_WATER { 0 } else { 0xff };
                }
            }
        }
        Some(o)
    }

    /// The frame bits of a joining piece: the neighbours of its own type (1 north, 2 east, 4 south, 8 west).
    pub fn join_bits(&self, o: &Placed) -> u32 {
        let same = |x: i32, y: i32| self.objects.iter().any(|p| p.uid != o.uid && p.kind == o.kind && p.covers(x, y));
        let mut bits = 0;
        if (o.a..o.a + o.w).any(|x| same(x, o.b - 1)) {
            bits |= 1;
        }
        if (o.b..o.b + o.h).any(|y| same(o.a + o.w, y)) {
            bits |= 2;
        }
        if (o.a..o.a + o.w).any(|x| same(x, o.b + o.h)) {
            bits |= 4;
        }
        if (o.b..o.b + o.h).any(|y| same(o.a - 1, y)) {
            bits |= 8;
        }
        bits
    }

    /// The packs whose objects stand in the game but are not enabled (pack id, object count).
    pub fn missing(&self, cat: &Catalog) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for o in self.objects.iter().filter(|o| cat.get(&o.kind).is_none()) {
            match out.iter_mut().find(|(p, _)| *p == o.pack) {
                Some((_, n)) => *n += 1,
                None => out.push((o.pack.clone(), 1)),
            }
        }
        out
    }

    /// What is added to the value of a home site anchored on each tile: the scripts' bonuses and the `view` of the objects
    /// within reach (by tile, a * edge + b, for the edge `n`).
    pub fn value_grid(&self, cat: &Catalog, n: i32) -> Vec<i32> {
        let mut g = vec![0; (n * n) as usize];
        for (&i, &v) in &self.bonus {
            if let Some(c) = g.get_mut(i as usize) {
                *c += v;
            }
        }
        for o in &self.objects {
            let Some(t) = cat.get(&o.kind) else { continue };
            if t.def.view == 0 {
                continue;
            }
            let r = t.def.view_radius;
            for x in (o.a - r).max(0)..(o.a + o.w + r).min(n) {
                for y in (o.b - r).max(0)..(o.b + o.h + r).min(n) {
                    g[(x * n + y) as usize] += t.def.view;
                }
            }
        }
        g
    }
}

/// What the packs add to home-site values, by tile (crate::homes::lot_value adds it). Set by the game when the packs'
/// objects or bonuses change; empty when no pack adds anything, so the original's values are untouched.
static VALUE: RwLock<Option<Arc<ValueGrid>>> = RwLock::new(None);

/// The grid's edge and its values, x * edge + y.
type ValueGrid = (i32, Vec<i32>);

pub fn set_value_grid(n: i32, g: Vec<i32>) {
    let v = if g.iter().all(|&x| x == 0) { None } else { Some(Arc::new((n, g))) };
    *VALUE.write().unwrap_or_else(|e| e.into_inner()) = v;
}

/// The packs' addition to the value of a home site anchored at (a, b).
pub fn value_bonus(a: i32, b: i32) -> i32 {
    let g = VALUE.read().unwrap_or_else(|e| e.into_inner());
    match g.as_deref() {
        Some((n, v)) if (0..*n).contains(&a) && (0..*n).contains(&b) => v[(a * n + b) as usize],
        _ => 0,
    }
}

/// "#rrggbb" or "#rrggbbaa" as RGBA 0..1 (None when it is neither).
pub fn colour(s: &str) -> Option<[f32; 4]> {
    let h = s.strip_prefix('#')?;
    if !(h.len() == 6 || h.len() == 8) || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |k: usize| u8::from_str_radix(&h[k..k + 2], 16).ok().map(|v| v as f32 / 255.0);
    Some([byte(0)?, byte(2)?, byte(4)?, if h.len() == 8 { byte(6)? } else { 1.0 }])
}

/// One line of a panel a script shows (`show_panel`), read from the map the script gave.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// A heading, a line of text (with an optional colour), a bar (value of max, with its label), a button (its id and
    /// label; greyed out when disabled), and a gap.
    Heading(String),
    Text(String, Option<[f32; 4]>),
    Bar {
        label: String,
        value: f64,
        max: f64,
    },
    Button {
        id: String,
        label: String,
        enabled: bool,
    },
    Gap,
}

/// A panel a script shows: its title, lines and the buttons along its bottom.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Panel {
    pub pack: String,
    pub id: String,
    pub title: String,
    pub rows: Vec<Row>,
    /// A question (`ask`): answered by one of its bottom buttons, which go to `on_answer` instead of `on_panel_button`.
    pub question: bool,
    pub buttons: Vec<(String, String)>,
}

fn text_of(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        v => v.to_string(),
    }
}

impl Panel {
    /// A panel from the map a script gave `show_panel`: `title`, `rows` (maps with `heading`, `text` (and `colour`), `bar` with
    /// `max` and `label`, `button` with `label` (and `enabled`), or `gap`) and `buttons` (maps with `id` and `label`).
    pub fn from_spec(pack: &str, id: &str, spec: &serde_json::Value) -> Panel {
        let mut p = Panel { pack: pack.into(), id: id.into(), title: text_of(&spec["title"]), ..Default::default() };
        for r in spec["rows"].as_array().map(|v| v.as_slice()).unwrap_or(&[]).iter().take(40) {
            let row = if let Some(h) = r.get("heading") {
                Row::Heading(text_of(h))
            } else if let Some(b) = r.get("button") {
                let label = r.get("label").map(text_of).unwrap_or_else(|| text_of(b));
                Row::Button { id: text_of(b), label, enabled: r.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true) }
            } else if let Some(v) = r.get("bar") {
                Row::Bar {
                    label: r.get("label").map(text_of).unwrap_or_default(),
                    value: v.as_f64().unwrap_or(0.0),
                    max: r.get("max").and_then(|v| v.as_f64()).unwrap_or(100.0),
                }
            } else if let Some(t) = r.get("text") {
                Row::Text(text_of(t), r.get("colour").or(r.get("color")).and_then(|c| c.as_str()).and_then(colour))
            } else if r.is_string() {
                Row::Text(text_of(r), None)
            } else {
                Row::Gap
            };
            p.rows.push(row);
        }
        for b in spec["buttons"].as_array().map(|v| v.as_slice()).unwrap_or(&[]).iter().take(4) {
            let (id, label) = match b {
                serde_json::Value::String(s) => (s.clone(), s.clone()),
                b => (text_of(&b["id"]), b.get("label").map(text_of).unwrap_or_else(|| text_of(&b["id"]))),
            };
            p.buttons.push((id, label));
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r##"
[[object]]
id = "hoa-house"
name = "HOA house"
category = "Neighbourhood"
footprint = [2, 3]
price = 25000
upkeep = 200
art = "art/hoa-house.png"
views = 4
frames = 3
levels = 3
view = 8

[[object]]
id = "road"
name = "Road"
category = "Neighbourhood"
ground = true
join = true
frames = 16

[[tool]]
id = "zone"
name = "Zone lots"

[[panel]]
id = "hoa"
name = "Homeowners' association"

[[overlay]]
id = "demand"
name = "Housing demand"
"##;

    fn land() -> Land {
        let mut l = crate::land::Land::from_terrain(&crate::terrain::Terrain::default(), 0);
        for t in l.ty.iter_mut() {
            *t = crate::land::T_ROUGH;
        }
        l
    }

    #[test]
    fn the_file_is_read_and_checked() {
        let mut c = Catalog::default();
        assert!(add_file(&mut c, "p", FILE).is_empty());
        assert_eq!(c.objects.len(), 2);
        assert_eq!(c.get("hoa-house").unwrap().def.footprint, [2, 3]);
        assert_eq!((c.tools.len(), c.panels.len(), c.overlays.len()), (1, 1, 1));
        assert_eq!(c.build_groups(0).len(), 1);
        // a wrong entry is left out with a warning, and a later pack replaces an entry by id
        let w = add_file(
            &mut c,
            "q",
            "[[object]]\nid = \"road\"\nname = \"Lane\"\nframes = 3\nlevels = 2\n[[object]]\nid = \"hoa-house\"\nname = \"Big\"\n",
        );
        assert_eq!(w.len(), 1);
        assert_eq!(c.get("hoa-house").unwrap().def.name, "Big");
        assert_eq!(c.get("road").unwrap().def.name, "Road");
        assert_eq!(add_file(&mut c, "q", "[[object]]\nid = \"x\"\nname = \"X\"\ncolour = 1\n").len(), 1);
    }

    #[test]
    fn objects_go_down_and_come_up_again() {
        let mut c = Catalog::default();
        add_file(&mut c, "p", FILE);
        let mut l = land();
        let mut w = World::default();
        let house = c.get("hoa-house").unwrap().clone();
        assert_eq!(w.fits(&l, &house.def, 10, 10, 1), Ok(()));
        let uid = w.place(&mut l, &house, 10, 10, 1);
        // turned, the footprint is 3 by 2
        let o = w.get(uid).unwrap().clone();
        assert_eq!((o.w, o.h), (3, 2));
        assert!(w.blocks(12, 11) && !w.blocks(13, 11));
        assert_eq!(l.ty[(12 * mn() + 11) as usize], T_BUILDING);
        assert_eq!(w.fits(&l, &house.def, 11, 9, 0), Err(Refusal::Taken));
        // a road may not go under a building, but next to it
        let mut road = c.get("road").unwrap().clone();
        road.def.terrain = "brush".into();
        assert_eq!(w.fits(&l, &road.def, 12, 11, 0), Err(Refusal::Taken));
        let r1 = w.place(&mut l, &road, 13, 10, 0);
        let r2 = w.place(&mut l, &road, 13, 11, 0);
        assert_eq!(w.join_bits(w.get(r1).unwrap()), 4);
        assert_eq!(w.join_bits(w.get(r2).unwrap()), 1);
        assert_eq!(l.ty[(13 * mn() + 10) as usize], crate::land::T_BRUSH);
        w.remove(&mut l, r1);
        assert_eq!(l.ty[(13 * mn() + 10) as usize], crate::land::T_ROUGH);
        let r1 = w.place(&mut l, &road, 13, 10, 0);
        assert_eq!(w.join_bits(w.get(r1).unwrap()), 4);
        // the value of lots nearby goes up
        let g = w.value_grid(&c, mn());
        assert_eq!(g[(9 * mn() + 9) as usize], 8);
        assert_eq!(g[(30 * mn() + 30) as usize], 0);
        w.remove(&mut l, uid).unwrap();
        assert_eq!(l.ty[(12 * mn() + 11) as usize], crate::land::T_ROUGH);
        assert_eq!(l.flags[(12 * mn() + 11) as usize] & flag::FOOTPRINT, 0);
        assert!(w.fits(&l, &house.def, 10, 10, 0).is_ok());
        // uids are never used again
        assert_eq!(w.place(&mut l, &house, 10, 10, 0), 5);
        // a game whose pack is gone lists it
        assert_eq!(w.missing(&Catalog::default()), vec![("p".to_string(), 3)]);
    }

    #[test]
    fn the_example_neighbourhood_is_read_and_its_art_cut() {
        let mut c = Catalog::default();
        let w = add_file(&mut c, "example-neighbourhood", include_str!("../../../packs/example-neighbourhood/data/objects.toml"));
        assert!(w.is_empty(), "{w:?}");
        assert_eq!((c.objects.len(), c.tools.len(), c.panels.len(), c.overlays.len()), (3, 1, 1, 1));
        for (id, png) in [
            ("nb-road", &include_bytes!("../../../packs/example-neighbourhood/art/road.png")[..]),
            ("nb-house", &include_bytes!("../../../packs/example-neighbourhood/art/house.png")[..]),
            ("nb-hall", &include_bytes!("../../../packs/example-neighbourhood/art/hall.png")[..]),
        ] {
            let d = &c.get(id).unwrap().def;
            let r = crate::hires::object_sprite(png, d.views as u32, d.frames as u32, d.scale, d.anchor, d.frame_ms);
            assert!(r.is_ok(), "{id}: {:?}", r.err());
        }
    }

    #[test]
    fn a_panel_is_read_from_the_scripts_map() {
        let spec = serde_json::json!({
            "title": "HOA",
            "rows": [{"heading": "Level 2"}, {"text": "Dues", "colour": "#ff0000"}, {"bar": 40, "max": 100, "label": "XP"},
                     {"button": "meet", "label": "Hold a meeting", "enabled": false}, {}, "plain"],
            "buttons": ["ok", {"id": "no", "label": "No thanks"}]
        });
        let p = Panel::from_spec("p", "hoa", &spec);
        assert_eq!(p.title, "HOA");
        assert_eq!(p.rows[1], Row::Text("Dues".into(), Some([1.0, 0.0, 0.0, 1.0])));
        assert_eq!(p.rows[3], Row::Button { id: "meet".into(), label: "Hold a meeting".into(), enabled: false });
        assert_eq!((p.rows[4].clone(), p.rows[5].clone()), (Row::Gap, Row::Text("plain".into(), None)));
        assert_eq!(p.buttons, vec![("ok".into(), "ok".into()), ("no".into(), "No thanks".into())]);
    }
}
