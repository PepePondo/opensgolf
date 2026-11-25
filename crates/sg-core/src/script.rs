//! Game scripts of content packs (docs/MODDING.md, section 6), in Rhai.
//!
//! A pack may hold `scripts/*.rhai`. Each file may define the hooks of [`Hook`] (`on_start()`, `on_month()`,
//! `on_object_placed(obj)`, `on_panel_button(panel, button)` ...); the game calls them for the packs the player enabled (and
//! for the scenario pack a game was started from), in load order. Inside a hook `this` is the pack's own state, a map that
//! is saved with the game.
//!
//! Scripts read the game from a [`GameView`] and a [`Snapshot`] of the land, made before the hooks run, and change it only
//! through the [`Action`]s they ask for, which the game carries out after each hook, between two game ticks. So a script
//! never sees the game half changed, and a game replays the same way from its log.
//!
//! Scripts are sandboxed: Rhai has no file or network access, `eval` is switched off, and every hook call is limited in
//! operations, call depth, string, array and map sizes. A script that fails (a syntax error, a run-time error, a limit hit)
//! is reported, never crashes the game; a pack whose scripts fail ten times is stopped for the session. Scripts get random
//! numbers from their own generator (saved with the game), so they never disturb the game's own random numbers and a
//! loaded game goes on the same way.
use rhai::{Array, CallFnOptions, Dynamic, Engine, ImmutableString, Map, Scope, AST};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Operations one hook call may run (a loop of a few thousand steps is fine; an endless one stops here).
pub const MAX_OPERATIONS: u64 = 2_000_000;
/// The largest array or map a script may make (a value for every tile of the largest map fits).
pub const MAX_ITEMS: usize = 100_000;
/// Errors after which a pack's scripts are stopped for the session.
pub const MAX_ERRORS: u32 = 10;

/// A golfer as the scripts see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GolferView {
    pub name: String,
    /// The hole the golfer is on (as numbered on the course), 0 when not on a hole.
    pub hole: i32,
    /// Strokes on the current hole so far.
    pub strokes: i32,
    /// A club member (not a visitor).
    pub member: bool,
    /// The player's own pro.
    pub pro: bool,
}

/// A hole as the scripts see it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoleView {
    pub number: i32,
    pub par: i32,
    pub yards: i32,
}

/// What the scripts can read of the game (filled by the game before each hook).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameView {
    /// The calendar year (2001 is the first) and month (3 March .. 10 October: the game's year has eight months).
    pub year: i32,
    pub month: i32,
    /// Cash, in money.
    pub cash: i64,
    pub fun: i32,
    /// Skill rating in hundredths.
    pub skill: i32,
    pub members: i32,
    /// 0 Municipal, 1 Daily Fee, 2 Country Club, 3 Championship.
    pub rank: i32,
    /// 0 Easy .. 3 Impossible.
    pub difficulty: i32,
    pub sandbox: bool,
    pub course: String,
    pub holes: Vec<HoleView>,
    pub golfers: Vec<GolferView>,
    /// The game tick (32 a day, 1024 a month).
    pub tick: u32,
}

/// One of the original's objects (a building, a home site, a landmark) as the scripts see it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildingView {
    /// Its id (`snack-bar`, `home-site`, ... as in buildings.toml and amenities.toml).
    pub kind: String,
    pub x: i32,
    pub y: i32,
    pub size: i32,
    /// A building's level; a landmark's type.
    pub level: i32,
    /// A home site's value and its celebrity owner (0 none).
    pub value: i32,
    pub owner: i32,
}

/// The land as the scripts see it, made by the game before the hooks run.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// The map's edge in tiles.
    pub edge: i32,
    /// Tile types (a * edge + b) and corner heights ((edge + 1) squared, a * (edge + 1) + b).
    pub ty: Vec<u8>,
    pub height: Vec<u8>,
    /// For land values.
    pub course: Option<crate::course::Course>,
    pub holes: Vec<crate::golfer::Hole>,
    pub difficulty: i32,
    pub buildings: Vec<BuildingView>,
    /// The packs' objects, and the uid the next one placed gets.
    pub objects: Vec<crate::modkit::Placed>,
    pub next_uid: u32,
    /// The packs' object types: id, name, pack.
    pub types: Vec<(String, String, String)>,
}

impl Snapshot {
    fn inside(&self, x: i64, y: i64) -> bool {
        (0..self.edge as i64).contains(&x) && (0..self.edge as i64).contains(&y)
    }
    fn tile(&self, x: i64, y: i64) -> Option<u8> {
        self.inside(x, y).then(|| self.ty.get((x * self.edge as i64 + y) as usize).copied()).flatten()
    }
}

fn object_map(o: &crate::modkit::Placed) -> Map {
    let mut m = Map::new();
    m.insert("uid".into(), (o.uid as i64).into());
    m.insert("type".into(), o.kind.clone().into());
    m.insert("pack".into(), o.pack.clone().into());
    m.insert("x".into(), (o.a as i64).into());
    m.insert("y".into(), (o.b as i64).into());
    m.insert("w".into(), (o.w as i64).into());
    m.insert("h".into(), (o.h as i64).into());
    m.insert("dir".into(), (o.dir as i64).into());
    m.insert("level".into(), (o.level as i64).into());
    let data = if o.data.is_null() { Dynamic::from(Map::new()) } else { rhai::serde::to_dynamic(&o.data).unwrap_or(Dynamic::UNIT) };
    m.insert("data".into(), data);
    m
}

fn building_map(b: &BuildingView) -> Map {
    let mut m = Map::new();
    m.insert("kind".into(), b.kind.clone().into());
    m.insert("x".into(), (b.x as i64).into());
    m.insert("y".into(), (b.y as i64).into());
    m.insert("size".into(), (b.size as i64).into());
    m.insert("level".into(), (b.level as i64).into());
    m.insert("value".into(), (b.value as i64).into());
    m.insert("owner".into(), (b.owner as i64).into());
    m
}

/// A hook the game calls.
#[derive(Clone, Debug, PartialEq)]
pub enum Hook {
    Start,
    Month,
    YearEnd,
    HoleFinished {
        golfer: GolferView,
        hole: i32,
        strokes: i32,
    },
    BuildingPlaced {
        kind: String,
        x: i32,
        y: i32,
    },
    /// A saved game was loaded (the scripts' state is back).
    Load,
    /// Each game day (32 ticks).
    Day,
    /// One of the packs' objects was placed (by the player or a script), removed, or clicked on the course.
    ObjectPlaced(crate::modkit::Placed),
    ObjectRemoved(crate::modkit::Placed),
    ObjectClicked(crate::modkit::Placed),
    /// A pack's tool was used on a tile (`remove`: with Shift or the right button). For that pack only.
    Tool {
        pack: String,
        tool: String,
        x: i32,
        y: i32,
        remove: bool,
    },
    /// A pack's panel was opened from the Mods window, a button of a panel was pressed, a question was answered, a map view
    /// was switched on. For that pack only.
    PanelOpen {
        pack: String,
        panel: String,
    },
    PanelButton {
        pack: String,
        panel: String,
        button: String,
    },
    Answer {
        pack: String,
        question: String,
        button: String,
    },
    OverlayOpen {
        pack: String,
        overlay: String,
    },
}

impl Hook {
    pub fn name(&self) -> &'static str {
        match self {
            Hook::Start => "on_start",
            Hook::Month => "on_month",
            Hook::YearEnd => "on_year_end",
            Hook::HoleFinished { .. } => "on_hole_finished",
            Hook::BuildingPlaced { .. } => "on_building_placed",
            Hook::Load => "on_load",
            Hook::Day => "on_day",
            Hook::ObjectPlaced(_) => "on_object_placed",
            Hook::ObjectRemoved(_) => "on_object_removed",
            Hook::ObjectClicked(_) => "on_object_clicked",
            Hook::Tool { .. } => "on_tool",
            Hook::PanelOpen { .. } => "on_panel_open",
            Hook::PanelButton { .. } => "on_panel_button",
            Hook::Answer { .. } => "on_answer",
            Hook::OverlayOpen { .. } => "on_overlay_open",
        }
    }

    /// The one pack a hook is for (None: every pack).
    pub fn target(&self) -> Option<&str> {
        match self {
            Hook::Tool { pack, .. }
            | Hook::PanelOpen { pack, .. }
            | Hook::PanelButton { pack, .. }
            | Hook::Answer { pack, .. }
            | Hook::OverlayOpen { pack, .. } => Some(pack),
            _ => None,
        }
    }
    fn args(&self) -> Vec<Dynamic> {
        match self {
            Hook::Start | Hook::Month | Hook::YearEnd | Hook::Load | Hook::Day => Vec::new(),
            Hook::ObjectPlaced(o) | Hook::ObjectRemoved(o) | Hook::ObjectClicked(o) => vec![object_map(o).into()],
            Hook::Tool { tool, x, y, remove, .. } => vec![tool.clone().into(), (*x as i64).into(), (*y as i64).into(), (*remove).into()],
            Hook::PanelOpen { panel, .. } => vec![panel.clone().into()],
            Hook::PanelButton { panel, button, .. } => vec![panel.clone().into(), button.clone().into()],
            Hook::Answer { question, button, .. } => vec![question.clone().into(), button.clone().into()],
            Hook::OverlayOpen { overlay, .. } => vec![overlay.clone().into()],
            Hook::HoleFinished { golfer, hole, strokes } => {
                vec![golfer_map(golfer).into(), (*hole as i64).into(), (*strokes as i64).into()]
            }
            Hook::BuildingPlaced { kind, x, y } => vec![kind.clone().into(), (*x as i64).into(), (*y as i64).into()],
        }
    }
}

/// What a script asked the game to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Message(String),
    /// Money (may be negative) and the reason, for the log.
    Money(i64, String),
    SetGoal {
        id: String,
        text: String,
    },
    /// A goal completed (true) or failed (false).
    FinishGoal {
        id: String,
        done: bool,
    },
    /// A short note at the top of the screen.
    Toast(String),
    /// One of the pack's object types placed (free of charge) with its uid as the script was told, facing `dir`.
    Place {
        kind: String,
        x: i32,
        y: i32,
        dir: u8,
        uid: u32,
    },
    Remove(u32),
    SetLevel(u32, i32),
    /// The pack's own fields of an object (a map, as JSON).
    SetData(u32, serde_json::Value),
    /// A tile painted a terrain type (not under a building or a hole's tee or green).
    SetTile {
        x: i32,
        y: i32,
        ty: u8,
    },
    /// What is added to the value of a home site on a tile; `ClearBonus` takes every bonus of the game away.
    Bonus {
        x: i32,
        y: i32,
        amount: i32,
    },
    ClearBonus,
    /// A map view's values (one per tile, a * edge + b), the values drawn in its two colours, and its legend.
    Overlay {
        id: String,
        values: Vec<i32>,
        low: i32,
        high: i32,
        legend: String,
    },
    /// A panel shown (its map, as JSON), or closed; a question asked with its buttons.
    ShowPanel {
        id: String,
        spec: serde_json::Value,
    },
    ClosePanel,
    Ask {
        id: String,
        text: String,
        buttons: Vec<String>,
    },
}

/// The result of calling a hook: the actions in order, each with its pack's id, and the problems.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub actions: Vec<(String, Action)>,
    /// (pack id, message)
    pub errors: Vec<(String, String)>,
    /// Lines the scripts printed (print, debug), with the pack id.
    pub log: Vec<(String, String)>,
}

/// One pack's saved script state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PackState {
    /// The pack's `this` map, as JSON.
    #[serde(default)]
    pub state: serde_json::Value,
    /// The pack's random generator.
    #[serde(default)]
    pub rng: u64,
}

struct Ctx {
    view: GameView,
    world: Rc<Snapshot>,
    actions: Vec<Action>,
    log: Vec<String>,
    rng: u64,
    /// The uids given to the objects the scripts placed during this round of hooks.
    next_uid: u32,
}

struct PackScripts {
    id: String,
    files: Vec<(String, AST)>,
    state: Dynamic,
    rng: u64,
    errors: u32,
}

/// The scripts of a game.
pub struct Host {
    engine: Engine,
    ctx: Rc<RefCell<Ctx>>,
    packs: Vec<PackScripts>,
}

fn golfer_map(g: &GolferView) -> Map {
    let mut m = Map::new();
    m.insert("name".into(), g.name.clone().into());
    m.insert("hole".into(), (g.hole as i64).into());
    m.insert("strokes".into(), (g.strokes as i64).into());
    m.insert("member".into(), g.member.into());
    m.insert("pro".into(), g.pro.into());
    m
}

/// A seed for a pack's generator: the game's seed mixed with the pack's id.
fn seed_for(seed: u64, id: &str) -> u64 {
    let h = id.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3));
    let s = seed ^ h;
    if s == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        s
    }
}

/// xorshift64*: the scripts' own generator.
fn next(rng: &mut u64) -> u64 {
    let mut x = *rng;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *rng = x;
    x.wrapping_mul(0x2545_f491_4f6c_dd1d)
}

impl Default for Host {
    fn default() -> Self {
        Host::new()
    }
}

impl Host {
    /// An engine with the limits and the game's functions, and no scripts yet.
    pub fn new() -> Host {
        let ctx = Rc::new(RefCell::new(Ctx {
            view: GameView::default(),
            world: Rc::new(Snapshot::default()),
            actions: Vec::new(),
            log: Vec::new(),
            rng: 1,
            next_uid: 1,
        }));
        let mut e = Engine::new();
        e.set_max_operations(MAX_OPERATIONS)
            .set_max_call_levels(32)
            .set_max_expr_depths(64, 32)
            .set_max_string_size(64 * 1024)
            .set_max_array_size(MAX_ITEMS)
            .set_max_map_size(MAX_ITEMS)
            .set_max_variables(1_000)
            .set_max_functions(1_000);
        e.disable_symbol("eval");
        {
            let c = ctx.clone();
            e.on_print(move |s| c.borrow_mut().log.push(s.to_string()));
            let c = ctx.clone();
            e.on_debug(move |s, _, _| c.borrow_mut().log.push(s.to_string()));
        }
        // reading the game
        macro_rules! getter {
            ($name:literal, |$v:ident| $body:expr) => {{
                let c = ctx.clone();
                e.register_fn($name, move || {
                    let $v = &c.borrow().view;
                    $body
                });
            }};
        }
        getter!("year", |v| v.year as i64);
        getter!("month", |v| v.month as i64);
        getter!("cash", |v| v.cash);
        getter!("fun_rating", |v| v.fun as i64);
        getter!("skill_rating", |v| v.skill as f64 / 100.0);
        getter!("members", |v| v.members as i64);
        getter!("rank", |v| v.rank as i64);
        getter!("rank_name", |v| crate::economy::rank_name(v.rank.clamp(0, 3) as usize).to_string());
        getter!("difficulty", |v| v.difficulty as i64);
        getter!("sandbox", |v| v.sandbox);
        getter!("course_name", |v| v.course.clone());
        getter!("hole_count", |v| v.holes.len() as i64);
        getter!("holes", |v| v
            .holes
            .iter()
            .map(|h| {
                let mut m = Map::new();
                m.insert("number".into(), (h.number as i64).into());
                m.insert("par".into(), (h.par as i64).into());
                m.insert("yards".into(), (h.yards as i64).into());
                Dynamic::from(m)
            })
            .collect::<Array>());
        getter!("golfers", |v| v.golfers.iter().map(|g| Dynamic::from(golfer_map(g))).collect::<Array>());
        getter!("tick", |v| v.tick as i64);
        getter!("day", |v| ((v.tick >> 5) & 31) as i64 + 1);
        // the land
        macro_rules! world {
            ($name:literal, |$w:ident $(, $arg:ident : $t:ty)*| $body:expr) => {{
                let c = ctx.clone();
                e.register_fn($name, move |$($arg: $t),*| {
                    let $w = c.borrow().world.clone();
                    $body
                });
            }};
        }
        world!("map_size", |w| w.edge as i64);
        world!("tile", |w, x: i64, y: i64| w.tile(x, y).map(|t| crate::data::terrain_id(t).to_string()).unwrap_or_default());
        world!("owned", |w, x: i64, y: i64| w.tile(x, y).is_some_and(|t| t != crate::land::T_OUT));
        world!("height", |w, x: i64, y: i64| {
            let e = w.edge as i64 + 1;
            if (0..e).contains(&x) && (0..e).contains(&y) {
                w.height.get((x * e + y) as usize).map(|&h| h as i64).unwrap_or(0)
            } else {
                0
            }
        });
        world!("land_value", |w, x: i64, y: i64| match &w.course {
            Some(c) if w.inside(x, y) => crate::homes::lot_value(c, &w.holes, w.difficulty, x as i32, y as i32) as i64,
            _ => 0,
        });
        world!("buildings", |w| w.buildings.iter().map(|b| Dynamic::from(building_map(b))).collect::<Array>());
        world!("home_sites", |w| w
            .buildings
            .iter()
            .filter(|b| b.kind == "home-site")
            .map(|b| Dynamic::from(building_map(b)))
            .collect::<Array>());
        world!("objects", |w| w.objects.iter().map(|o| Dynamic::from(object_map(o))).collect::<Array>());
        world!("objects_of", |w, kind: ImmutableString| w
            .objects
            .iter()
            .filter(|o| o.kind == kind.as_str())
            .map(|o| Dynamic::from(object_map(o)))
            .collect::<Array>());
        world!("object", |w, uid: i64| w
            .objects
            .iter()
            .find(|o| o.uid as i64 == uid)
            .map(|o| Dynamic::from(object_map(o)))
            .unwrap_or(Dynamic::UNIT));
        world!("object_at", |w, x: i64, y: i64| w
            .objects
            .iter()
            .filter(|o| o.covers(x as i32, y as i32))
            .min_by_key(|o| o.ground)
            .map(|o| Dynamic::from(object_map(o)))
            .unwrap_or(Dynamic::UNIT));
        world!("building_at", |w, x: i64, y: i64| w
            .buildings
            .iter()
            .find(|b| (b.x..b.x + b.size).contains(&(x as i32)) && (b.y..b.y + b.size).contains(&(y as i32)))
            .map(|b| Dynamic::from(building_map(b)))
            .unwrap_or(Dynamic::UNIT));
        world!("object_types", |w| w
            .types
            .iter()
            .map(|(id, name, pack)| {
                let mut m = Map::new();
                m.insert("id".into(), id.clone().into());
                m.insert("name".into(), name.clone().into());
                m.insert("pack".into(), pack.clone().into());
                Dynamic::from(m)
            })
            .collect::<Array>());
        {
            let c = ctx.clone();
            e.register_fn("hole_par", move |n: i64| {
                c.borrow().view.holes.iter().find(|h| h.number as i64 == n).map(|h| h.par as i64).unwrap_or(0)
            });
        }
        // acting on it
        {
            let c = ctx.clone();
            e.register_fn("show_message", move |t: ImmutableString| c.borrow_mut().actions.push(Action::Message(t.to_string())));
            let c = ctx.clone();
            e.register_fn("add_money", move |amount: i64, reason: ImmutableString| {
                c.borrow_mut().actions.push(Action::Money(amount.clamp(-10_000_000, 10_000_000), reason.to_string()))
            });
            let c = ctx.clone();
            e.register_fn("set_goal", move |id: ImmutableString, text: ImmutableString| {
                c.borrow_mut().actions.push(Action::SetGoal { id: id.to_string(), text: text.to_string() })
            });
            let c = ctx.clone();
            e.register_fn("complete_goal", move |id: ImmutableString| {
                c.borrow_mut().actions.push(Action::FinishGoal { id: id.to_string(), done: true })
            });
            let c = ctx.clone();
            e.register_fn("fail_goal", move |id: ImmutableString| {
                c.borrow_mut().actions.push(Action::FinishGoal { id: id.to_string(), done: false })
            });
            let c = ctx.clone();
            e.register_fn("toast", move |t: ImmutableString| c.borrow_mut().actions.push(Action::Toast(t.to_string())));
            let c = ctx.clone();
            let place = move |kind: ImmutableString, x: i64, y: i64, dir: i64| -> i64 {
                let mut c = c.borrow_mut();
                if !c.world.types.iter().any(|(id, _, _)| id == kind.as_str()) {
                    return -1;
                }
                let uid = c.next_uid;
                c.next_uid += 1;
                c.actions.push(Action::Place { kind: kind.to_string(), x: x as i32, y: y as i32, dir: (dir & 3) as u8, uid });
                uid as i64
            };
            let p2 = place.clone();
            e.register_fn("place_object", move |k: ImmutableString, x: i64, y: i64| p2(k, x, y, 0));
            e.register_fn("place_object", place);
            let c = ctx.clone();
            e.register_fn("remove_object", move |uid: i64| c.borrow_mut().actions.push(Action::Remove(uid.max(0) as u32)));
            let c = ctx.clone();
            e.register_fn("set_object_level", move |uid: i64, level: i64| {
                c.borrow_mut().actions.push(Action::SetLevel(uid.max(0) as u32, level.clamp(0, 15) as i32))
            });
            let c = ctx.clone();
            e.register_fn("set_object_data", move |uid: i64, data: Map| -> Result<(), Box<rhai::EvalAltResult>> {
                let v = rhai::serde::from_dynamic::<serde_json::Value>(&Dynamic::from(data))?;
                c.borrow_mut().actions.push(Action::SetData(uid.max(0) as u32, v));
                Ok(())
            });
            let c = ctx.clone();
            e.register_fn("set_tile", move |x: i64, y: i64, id: ImmutableString| -> Result<(), Box<rhai::EvalAltResult>> {
                let ty = crate::data::terrain_index(id.as_str()).ok_or_else(|| format!("set_tile: no terrain '{id}'"))?;
                c.borrow_mut().actions.push(Action::SetTile { x: x as i32, y: y as i32, ty });
                Ok(())
            });
            let c = ctx.clone();
            e.register_fn("set_land_value_bonus", move |x: i64, y: i64, amount: i64| {
                c.borrow_mut().actions.push(Action::Bonus { x: x as i32, y: y as i32, amount: amount.clamp(-100_000, 100_000) as i32 })
            });
            let c = ctx.clone();
            e.register_fn("clear_land_value_bonus", move || c.borrow_mut().actions.push(Action::ClearBonus));
            let c = ctx.clone();
            let overlay = move |id: ImmutableString, values: Array, low: i64, high: i64, legend: ImmutableString| {
                let values =
                    values.iter().map(|v| v.as_int().map(|n| n.clamp(-1_000_000_000, 1_000_000_000) as i32).unwrap_or(i32::MIN)).collect();
                c.borrow_mut().actions.push(Action::Overlay {
                    id: id.to_string(),
                    values,
                    low: low as i32,
                    high: high as i32,
                    legend: legend.to_string(),
                })
            };
            let o2 = overlay.clone();
            e.register_fn("set_overlay", move |id: ImmutableString, values: Array, low: i64, high: i64| {
                o2(id, values, low, high, "".into())
            });
            e.register_fn("set_overlay", overlay);
            let c = ctx.clone();
            e.register_fn("show_panel", move |id: ImmutableString, spec: Map| -> Result<(), Box<rhai::EvalAltResult>> {
                let spec = rhai::serde::from_dynamic::<serde_json::Value>(&Dynamic::from(spec))?;
                c.borrow_mut().actions.push(Action::ShowPanel { id: id.to_string(), spec });
                Ok(())
            });
            let c = ctx.clone();
            e.register_fn("close_panel", move || c.borrow_mut().actions.push(Action::ClosePanel));
            let c = ctx.clone();
            e.register_fn("ask", move |id: ImmutableString, text: ImmutableString, buttons: Array| {
                let buttons = buttons.iter().take(4).map(|b| b.to_string()).collect();
                c.borrow_mut().actions.push(Action::Ask { id: id.to_string(), text: text.to_string(), buttons })
            });
            let c = ctx.clone();
            e.register_fn("random", move |n: i64| {
                if n <= 0 {
                    return 0;
                }
                (next(&mut c.borrow_mut().rng) % n as u64) as i64
            });
        }
        Host { engine: e, ctx, packs: Vec::new() }
    }

    /// Compiles a pack's scripts (file name, text), in the order given. Returns the problems; a file that does not compile
    /// is left out, the others run. `seed` seeds the pack's generator.
    pub fn add_pack(&mut self, id: &str, files: &[(String, String)], seed: u64) -> Vec<String> {
        let mut errors = Vec::new();
        let mut asts = Vec::new();
        for (name, text) in files {
            match self.engine.compile(text) {
                Ok(ast) => asts.push((name.clone(), ast)),
                Err(e) => errors.push(format!("{name}: {e}")),
            }
        }
        if !asts.is_empty() {
            self.packs.retain(|p| p.id != id);
            self.packs.push(PackScripts { id: id.to_string(), files: asts, state: Map::new().into(), rng: seed_for(seed, id), errors: 0 });
        }
        errors
    }

    /// The packs with scripts, in calling order.
    pub fn pack_ids(&self) -> Vec<String> {
        self.packs.iter().map(|p| p.id.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }

    /// Whether any pack defines a hook (by its name, e.g. "on_day").
    pub fn defines(&self, name: &str) -> bool {
        self.packs.iter().any(|p| p.errors < MAX_ERRORS && p.files.iter().any(|(_, a)| a.iter_functions().any(|f| f.name == name)))
    }

    /// Gives the scripts the land as it now is (before a round of hooks).
    pub fn set_world(&mut self, w: Rc<Snapshot>) {
        let mut c = self.ctx.borrow_mut();
        c.next_uid = w.next_uid.max(1);
        c.world = w;
    }

    /// Calls a hook in every pack that defines it (or only in its pack, `Hook::target`).
    pub fn call(&mut self, hook: &Hook, view: &GameView) -> Output {
        let mut out = Output::default();
        let name = hook.name();
        let args = hook.args();
        self.ctx.borrow_mut().view = view.clone();
        let target = hook.target();
        for p in self.packs.iter_mut().filter(|p| p.errors < MAX_ERRORS && target.is_none_or(|t| t == p.id)) {
            for (file, ast) in &p.files {
                if !ast.iter_functions().any(|f| f.name == name && f.params.len() == args.len()) {
                    continue;
                }
                {
                    let mut c = self.ctx.borrow_mut();
                    c.actions.clear();
                    c.log.clear();
                    c.rng = p.rng;
                }
                let mut scope = Scope::new();
                let opts = CallFnOptions::new().eval_ast(false).rewind_scope(true).bind_this_ptr(&mut p.state);
                let r = self.engine.call_fn_with_options::<Dynamic>(opts, &mut scope, ast, name, args.clone());
                let mut c = self.ctx.borrow_mut();
                p.rng = c.rng;
                out.log.extend(c.log.drain(..).map(|l| (p.id.clone(), l)));
                match r {
                    Ok(_) => out.actions.extend(c.actions.drain(..).map(|a| (p.id.clone(), a))),
                    Err(e) => {
                        // what a failed call asked for before failing is dropped with it
                        c.actions.clear();
                        p.errors += 1;
                        let mut msg = format!("{file}: {name}: {e}");
                        if p.errors >= MAX_ERRORS {
                            msg.push_str(&format!(" (stopped after {MAX_ERRORS} errors)"));
                        }
                        out.errors.push((p.id.clone(), msg));
                    }
                }
                if !p.state.is_map() {
                    // `this` must stay a map (a script that assigns something else to it gets an empty one)
                    p.state = Map::new().into();
                }
                if p.errors >= MAX_ERRORS {
                    break;
                }
            }
        }
        out
    }

    /// The scripts' state, for the saved game.
    pub fn save(&self) -> BTreeMap<String, PackState> {
        self.packs
            .iter()
            .map(|p| {
                let state = rhai::serde::from_dynamic::<serde_json::Value>(&p.state).unwrap_or(serde_json::Value::Null);
                (p.id.clone(), PackState { state, rng: p.rng })
            })
            .collect()
    }

    /// Puts back the state of a saved game (packs not in it keep a fresh state). Returns the problems.
    pub fn restore(&mut self, saved: &BTreeMap<String, PackState>) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        for p in self.packs.iter_mut() {
            let Some(s) = saved.get(&p.id) else { continue };
            if s.rng != 0 {
                p.rng = s.rng;
            }
            match rhai::serde::to_dynamic(&s.state) {
                Ok(d) if d.is_map() => p.state = d,
                Ok(d) if d.is_unit() => {}
                Ok(_) => errors.push((p.id.clone(), "the saved script state is not a map".to_string())),
                Err(e) => errors.push((p.id.clone(), format!("the saved script state: {e}"))),
            }
        }
        errors
    }
}

/// The script files of a pack: every `scripts/*.rhai` (by name), as (name, text).
pub fn pack_files(source: &crate::vfs::Source) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = source
        .files()
        .into_iter()
        .filter(|f| {
            let low = f.to_lowercase();
            low.starts_with("scripts/") && low.ends_with(".rhai") && low.matches('/').count() == 1
        })
        .filter_map(|f| source.read(&f).map(|d| (f, String::from_utf8_lossy(&d).into_owned())))
        .collect();
    v.sort_by_key(|(f, _)| f.to_lowercase());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> GameView {
        GameView {
            year: 2002,
            month: 4,
            cash: 12345,
            fun: 30,
            skill: 1250,
            members: 7,
            rank: 1,
            holes: vec![HoleView { number: 1, par: 3, yards: 150 }, HoleView { number: 2, par: 4, yards: 380 }],
            golfers: vec![GolferView { name: "Ann".into(), hole: 2, strokes: 1, member: true, pro: false }],
            ..Default::default()
        }
    }

    fn host(src: &str) -> Host {
        let mut h = Host::new();
        let e = h.add_pack("p", &[("scripts/a.rhai".into(), src.into())], 42);
        assert!(e.is_empty(), "{e:?}");
        h
    }

    #[test]
    fn hooks_read_the_game_and_act() {
        let mut h = host(
            r#"
            fn on_month() {
                if cash() > 10000 && fun_rating() == 30 && skill_rating() == 12.5 && hole_par(2) == 4 && golfers()[0].name == "Ann" {
                    show_message(`It is ${month()}/${year()}, ${members()} members, ${rank_name()}`);
                }
            }
            fn on_hole_finished(golfer, hole, strokes) {
                if strokes == 1 { add_money(500, "ace bonus for " + golfer.name); }
            }
            fn on_building_placed(kind, x, y) { set_goal("b", kind + " at " + x + "," + y); complete_goal("b"); }
        "#,
        );
        let out = h.call(&Hook::Month, &view());
        assert_eq!(out.actions, vec![("p".into(), Action::Message("It is 4/2002, 7 members, Daily Fee".into()))]);
        let g = GolferView { name: "Bo".into(), ..Default::default() };
        let out = h.call(&Hook::HoleFinished { golfer: g.clone(), hole: 3, strokes: 1 }, &view());
        assert_eq!(out.actions, vec![("p".into(), Action::Money(500, "ace bonus for Bo".into()))]);
        assert!(h.call(&Hook::HoleFinished { golfer: g, hole: 3, strokes: 4 }, &view()).actions.is_empty());
        let out = h.call(&Hook::BuildingPlaced { kind: "snack-bar".into(), x: 3, y: 4 }, &view());
        assert_eq!(out.actions.len(), 2);
        // a hook the scripts do not define does nothing
        assert_eq!(h.call(&Hook::YearEnd, &view()), Output::default());
    }

    #[test]
    fn state_is_kept_and_saved_with_its_generator() {
        let src = "fn on_month() { this.n = (this.n ?? 0) + 1; this.r = random(1000); this.list = [1, \"two\", #{three: 3}]; }";
        let mut h = host(src);
        h.call(&Hook::Month, &view());
        h.call(&Hook::Month, &view());
        let saved = h.save();
        assert_eq!(saved["p"].state["n"], 2);
        // a fresh host given the saved state goes on exactly the same way
        let mut a = host(src);
        a.restore(&saved);
        a.call(&Hook::Month, &view());
        h.call(&Hook::Month, &view());
        assert_eq!(a.save(), h.save());
        assert_eq!(a.save()["p"].state["n"], 3);
        // the same seed gives the same numbers; another seed others
        let mut b = Host::new();
        b.add_pack("p", &[("a".into(), src.into())], 43);
        b.call(&Hook::Month, &view());
        let mut c = host(src);
        c.call(&Hook::Month, &view());
        assert_ne!(b.save()["p"].rng, c.save()["p"].rng);
    }

    #[test]
    fn errors_and_limits_never_escape() {
        let mut h = Host::new();
        let e = h.add_pack("bad", &[("scripts/x.rhai".into(), "fn on_start( {".into())], 1);
        assert_eq!(e.len(), 1);
        assert!(h.is_empty());
        let e = h.add_pack("bad", &[("scripts/x.rhai".into(), "fn on_start() { eval(\"1\") }".into())], 1);
        assert!(e[0].contains("eval"), "{e:?}");
        let mut h = host(
            r#"
            fn on_start() { show_message("lost"); let x = 1 / 0; }
            fn on_month() { loop { } }
            fn on_year_end() { let s = "x"; loop { s += s; } }
            fn on_building_placed(k, x, y) { let a = []; a.pad(1000000, 0); }
        "#,
        );
        let out = h.call(&Hook::Start, &view());
        assert!(out.actions.is_empty(), "a failed hook's actions are dropped");
        assert_eq!(out.errors.len(), 1);
        assert!(h.call(&Hook::Month, &view()).errors[0].1.to_lowercase().contains("operations"));
        assert_eq!(h.call(&Hook::YearEnd, &view()).errors.len(), 1);
        assert_eq!(h.call(&Hook::BuildingPlaced { kind: "x".into(), x: 0, y: 0 }, &view()).errors.len(), 1);
        // after ten errors the pack is stopped
        for _ in 0..10 {
            h.call(&Hook::Start, &view());
        }
        assert!(h.call(&Hook::Start, &view()).errors.is_empty());
    }

    #[test]
    fn the_example_neighbourhood_runs_its_association() {
        let id = "example-neighbourhood";
        let src = include_str!("../../../packs/example-neighbourhood/scripts/neighbourhood.rhai");
        let mut h = Host::new();
        assert!(h.add_pack(id, &[("scripts/neighbourhood.rhai".into(), src.into())], 3).is_empty());
        let mut w = Snapshot {
            edge: 10,
            ty: vec![4; 100],
            height: vec![3; 121],
            types: vec![("nb-house".into(), "House".into(), id.into()), ("nb-road".into(), "Street".into(), id.into())],
            next_uid: 1,
            ..Default::default()
        };
        h.set_world(Rc::new(w.clone()));
        let ok = |out: &Output| assert!(out.errors.is_empty(), "{:?}", out.errors);
        let out = h.call(&Hook::Start, &view());
        ok(&out);
        let house =
            crate::modkit::Placed { uid: 1, kind: "nb-house".into(), pack: id.into(), a: 2, b: 2, w: 2, h: 2, ..Default::default() };
        let out = h.call(&Hook::ObjectPlaced(house.clone()), &view());
        ok(&out);
        assert!(out.actions.iter().any(|(_, a)| matches!(a, Action::SetData(1, d) if d["family"].is_string())), "{:?}", out.actions);
        w.objects.push(house.clone());
        h.set_world(Rc::new(w.clone()));
        let mut levelled = false;
        for _ in 0..30 {
            let out = h.call(&Hook::Month, &view());
            ok(&out);
            levelled |= out.actions.iter().any(|(_, a)| matches!(a, Action::SetLevel(1, l) if *l >= 1));
        }
        assert!(levelled);
        let out = h.call(&Hook::PanelOpen { pack: id.into(), panel: "association".into() }, &view());
        ok(&out);
        assert!(matches!(&out.actions[0].1, Action::ShowPanel { id, .. } if id == "association"));
        let out = h.call(&Hook::PanelButton { pack: id.into(), panel: "association".into(), button: "party".into() }, &view());
        ok(&out);
        let out = h.call(&Hook::ObjectClicked(house), &view());
        ok(&out);
        assert!(matches!(&out.actions[0].1, Action::ShowPanel { id, .. } if id == "house"));
        let out = h.call(&Hook::Tool { pack: id.into(), tool: "survey".into(), x: 3, y: 3, remove: false }, &view());
        ok(&out);
        assert!(matches!(&out.actions[0].1, Action::Toast(_)));
        let out = h.call(&Hook::OverlayOpen { pack: id.into(), overlay: "value".into() }, &view());
        ok(&out);
        assert!(matches!(&out.actions[0].1, Action::Overlay { values, .. } if values.len() == 100));
        // another pack's tool is not this pack's business
        assert!(h.call(&Hook::Tool { pack: "other".into(), tool: "survey".into(), x: 3, y: 3, remove: false }, &view()).actions.is_empty());
        // a script places an object and is told the uid it will have
        let mut p = Host::new();
        p.add_pack(
            id,
            &[("a".into(), "fn on_start() { this.u = place_object(\"nb-road\", 1, 1); this.bad = place_object(\"nope\", 1, 1); }".into())],
            1,
        );
        p.set_world(Rc::new(w.clone()));
        let out = p.call(&Hook::Start, &view());
        assert_eq!(out.actions, vec![(id.to_string(), Action::Place { kind: "nb-road".into(), x: 1, y: 1, dir: 0, uid: 1 })]);
        assert_eq!((p.save()[id].state["u"].clone(), p.save()[id].state["bad"].clone()), (1.into(), (-1).into()));
    }

    #[test]
    fn the_example_script_pays_for_a_hole_in_one() {
        let m = crate::packs::Manifest::parse(include_str!("../../../packs/example-script-bonus/pack.toml")).unwrap();
        assert_eq!(m.kind, crate::packs::Kind::Mod);
        let mut h = Host::new();
        let src = include_str!("../../../packs/example-script-bonus/scripts/bonus.rhai");
        assert!(h.add_pack(&m.id, &[("scripts/bonus.rhai".into(), src.into())], 7).is_empty());
        let out = h.call(&Hook::Start, &view());
        assert!(out.errors.is_empty() && out.actions.len() == 1, "{out:?}");
        let g = GolferView { name: "Ann".into(), member: true, ..Default::default() };
        let out = h.call(&Hook::HoleFinished { golfer: g.clone(), hole: 2, strokes: 1 }, &view());
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert!(out.actions.iter().any(|(_, a)| matches!(a, Action::Money(m, _) if *m > 0)));
        assert!(out.actions.iter().any(|(_, a)| matches!(a, Action::Message(t) if t.contains("Ann"))));
        let out = h.call(&Hook::HoleFinished { golfer: g, hole: 2, strokes: 3 }, &view());
        assert!(out.actions.is_empty());
        for hook in [Hook::Month, Hook::YearEnd] {
            assert!(h.call(&hook, &view()).errors.is_empty());
        }
        assert!(h.save()[&m.id].state["aces"] == 1);
    }
}
