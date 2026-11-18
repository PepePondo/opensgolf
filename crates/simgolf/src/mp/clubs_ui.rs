//! The shared clubs' page: what Start New Game's "Shared clubs" row opens. It lists the clubs (those with someone on
//! first), greys out the ones this game can't play (another version, other rules settings or rule-changing packs), and
//! joins one or makes a new one. A new club is a new game on the chosen land, played by everyone who joins.
//!
//! Like the weekly challenge's page (lobby.rs) it is drawn in place of Start New Game, and lobby.rs's draw, click, key
//! and character functions ask it first.

use super::club::server;
use super::settings;
use super::version::{Compat, Mismatch};
use crate::app::App;
use crate::extras::draw_button;
use crate::gfx::Gfx;
use crate::info_ui::c15;
use crate::render::Rect;
use crate::scenario_ui::ScenarioEntry;
use crate::ui::{Screen as Ui, F_ARIAL10, F_INFO14, F_MANUAL20};
use sg_core::scenario::{Scenario, ScenarioDef};
use std::cell::RefCell;

/// The Start New Game row's id (never a pack's: packs can't start with `*`).
pub const ROW_ID: &str = "*clubs";

const BOX: Rect = Rect { x: 40.0, y: 36.0, w: 720.0, h: 528.0 };
const LIST_TOP: f32 = BOX.y + 112.0;
const ROW_H: f32 = 20.0;
/// Rows shown.
const ROWS: usize = 14;
const NAME: Rect = Rect { x: BOX.x + 150.0, y: BOX.y + BOX.h - 82.0, w: 230.0, h: 22.0 };
const MAKE: Rect = Rect { x: BOX.x + 392.0, y: BOX.y + BOX.h - 82.0, w: 150.0, h: 22.0 };
const BACK: Rect = Rect { x: BOX.x + 24.0, y: BOX.y + BOX.h - 46.0, w: 100.0, h: 22.0 };
const REFRESH: Rect = Rect { x: BOX.x + 136.0, y: BOX.y + BOX.h - 46.0, w: 110.0, h: 22.0 };
const JOIN: Rect = Rect { x: BOX.x + BOX.w - 220.0, y: BOX.y + BOX.h - 46.0, w: 190.0, h: 22.0 };
/// A club's name: the Worker's rule (index.js cleanClubName).
const NAME_MAX: usize = 32;

/// A club in the list.
#[derive(Clone, Debug, PartialEq)]
pub struct ClubRow {
    pub id: String,
    pub name: String,
    pub players: i64,
    /// Why this game can't play it (empty: it can).
    pub why_not: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum List {
    Loading,
    Rows(Vec<ClubRow>),
    Failed(String),
}

#[derive(Clone)]
struct Page {
    list: List,
    picked: Option<usize>,
    name: String,
    editing: bool,
}

thread_local! {
    static PAGE: RefCell<Option<Page>> = const { RefCell::new(None) };
    static FETCH: RefCell<Option<crate::net::Request>> = const { RefCell::new(None) };
}

fn page() -> Option<Page> {
    PAGE.with(|p| p.borrow().clone())
}

fn change(f: impl FnOnce(&mut Page)) {
    PAGE.with(|p| {
        if let Some(p) = p.borrow_mut().as_mut() {
            f(p)
        }
    })
}

pub fn is_open() -> bool {
    PAGE.with(|p| p.borrow().is_some())
}

/// The row Start New Game adds (once Multiplayer is on, or always while it is private: the page says what is sent).
pub fn entry() -> ScenarioEntry {
    let sc = Scenario {
        scenario: ScenarioDef {
            name: Some("Shared clubs".into()),
            description: "Play one course together with friends: everyone builds and runs the same club, and it pauses \
                          when nobody is on."
                .into(),
            // (set so Start New Game opens the page instead of Select Difficulty: a new club asks for it later)
            difficulty: Some(sg_core::scenario::Difficulty::Easy),
            ..Default::default()
        },
        goals: Vec::new(),
    };
    ScenarioEntry { id: ROW_ID.into(), name: "Shared clubs".into(), description: sc.scenario.description.clone(), def: Ok(sc) }
}

/// The Start New Game row's line for the shared clubs' row (other rows: None).
pub fn row_line(id: &str) -> Option<String> {
    (id == ROW_ID).then(|| "Play one course together: everyone builds and runs the same club.".to_string())
}

/// What the player reads for the first part that differs.
pub fn why_not(m: &[Mismatch]) -> String {
    match m.first() {
        None => String::new(),
        Some(Mismatch::Protocol { .. } | Mismatch::Log { .. }) => "needs another game version".into(),
        Some(Mismatch::Version { theirs, .. }) => format!("needs version {theirs}"),
        Some(Mismatch::Rules) => "other rules settings (Fix original bugs, Random trees)".into(),
        Some(Mismatch::Map { .. }) => "another course size".into(),
        Some(Mismatch::Extras) => "other gameplay extras".into(),
        Some(Mismatch::Packs { missing, extra }) => match (missing.first(), extra.first()) {
            (Some(p), _) => format!("needs the pack {p}"),
            (None, Some(p)) => format!("turn off the pack {p}"),
            _ => "other packs".into(),
        },
    }
}

/// The Worker's list, checked against this game's record.
pub fn read_list(body: &[u8], ours: &Compat) -> List {
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else { return List::Failed("not an answer".into()) };
    if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
        return List::Failed(v.get("error").and_then(|e| e.as_str()).unwrap_or("no list").to_string());
    }
    let rows = v
        .get("clubs")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let compat: Compat = serde_json::from_value(c.get("compat")?.clone()).ok()?;
                    // the course size and gameplay extras come with the club's save (club.rs club_compat)
                    let m = ours.check(&Compat { map: ours.map, extras: ours.extras, ..compat });
                    Some(ClubRow {
                        id: c.get("id")?.as_str()?.to_string(),
                        name: c.get("name")?.as_str()?.to_string(),
                        players: c.get("players").and_then(|p| p.as_i64()).unwrap_or(0),
                        why_not: why_not(&m),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    List::Rows(rows)
}

fn start_fetch() {
    FETCH.with(|f| *f.borrow_mut() = Some(crate::net::get(&format!("{}/clubs", server()), 256 * 1024)));
    change(|p| {
        p.list = List::Loading;
        p.picked = None;
    });
}

/// Each frame: the list's answer, once it is in.
pub fn poll(ours: &Compat) {
    let Some(reply) = FETCH.with(|f| {
        let mut f = f.borrow_mut();
        let r = f.as_mut()?.poll()?;
        *f = None;
        Some(r)
    }) else {
        return;
    };
    let l = match reply {
        Ok(body) => read_list(&body, ours),
        Err(e) => List::Failed(e),
    };
    change(|p| p.list = l);
}

/// Whether a typed character may go into a club's name.
pub fn name_allowed(text: &str, c: char) -> bool {
    text.chars().count() < NAME_MAX && (c.is_ascii_alphanumeric() || " _.,'&!-".contains(c))
}

impl App {
    pub fn open_clubs_page(&mut self) -> bool {
        PAGE.with(|p| *p.borrow_mut() = Some(Page { list: List::Loading, picked: None, name: String::new(), editing: false }));
        start_fetch();
        self.hover = -1;
        true
    }

    fn close_clubs_page(&mut self) {
        PAGE.with(|p| *p.borrow_mut() = None);
        FETCH.with(|f| *f.borrow_mut() = None);
        self.hover = -1;
    }

    /// Joining or making a club turns Multiplayer on (the page says what is sent).
    fn clubs_turn_on(&self) {
        let mut s = settings::with_player();
        if !s.on {
            s.on = true;
            settings::save(s);
        }
    }

    fn clubs_join(&mut self) {
        let Some(p) = page() else { return };
        let List::Rows(rows) = &p.list else { return };
        let Some(r) = p.picked.and_then(|k| rows.get(k)) else { return };
        if !r.why_not.is_empty() {
            self.show_toast(&format!("Can't join \"{}\": {}.", r.name, r.why_not));
            return;
        }
        if r.players >= 8 {
            self.show_toast("That club is full (8 on).");
            return;
        }
        let (id, name) = (r.id.clone(), r.name.clone());
        self.clubs_turn_on();
        self.close_clubs_page();
        self.club_join(&id, &name);
    }

    /// A new club: a new game on the land the player picks next (the usual Select Difficulty and property screens), which
    /// becomes the club's start once it begins.
    fn clubs_make(&mut self) {
        let Some(p) = page() else { return };
        let name = p.name.trim().replace("  ", " ");
        if name.chars().count() < 3 {
            self.show_toast("Type the new club's name first (3 to 32 letters).");
            change(|p| p.editing = true);
            return;
        }
        self.clubs_turn_on();
        self.close_clubs_page();
        self.mp.make_next = Some(name);
        // the usual new game (Start New Game's first row: Select Difficulty, then the land), never a scenario's (its
        // goals would be the owner's alone)
        self.scen.chosen = None;
        self.sandbox_choice = false;
        self.title.champ_pending = false;
        self.screen = crate::app::Screen::Difficulty;
        self.hover = -1;
    }

    /// A club to make once the new game has begun (Start New Game's usual screens came first).
    pub(crate) fn clubs_make_pending(&mut self) {
        if self.mp.make_next.is_some() && self.screen == crate::app::Screen::Play && self.land.is_some() && !self.club_on() {
            let name = self.mp.make_next.take().unwrap_or_default();
            self.club_make(&name, true);
        }
    }

    pub fn clubs_click(&mut self, x: f32, y: f32) -> bool {
        let Some(p) = page() else { return false };
        change(|p| p.editing = NAME.has(x, y));
        if MAKE.has(x, y) {
            self.clubs_make();
        } else if JOIN.has(x, y) {
            self.clubs_join();
        } else if REFRESH.has(x, y) {
            start_fetch();
        } else if BACK.has(x, y) || !BOX.has(x, y) {
            self.close_clubs_page();
        } else if let List::Rows(rows) = &p.list {
            if x >= BOX.x + 24.0 && x < BOX.x + BOX.w - 24.0 && y >= LIST_TOP {
                let k = ((y - LIST_TOP) / ROW_H) as usize;
                if k < rows.len().min(ROWS) {
                    if p.picked == Some(k) {
                        self.clubs_join();
                    } else {
                        change(|p| p.picked = Some(k));
                    }
                }
            }
        }
        true
    }

    pub fn clubs_key(&mut self, k: miniquad::KeyCode) -> bool {
        use miniquad::KeyCode;
        let Some(p) = page() else { return false };
        let n = if let List::Rows(r) = &p.list { r.len().min(ROWS) } else { 0 };
        match (p.editing, k) {
            (true, KeyCode::Backspace) => change(|p| {
                p.name.pop();
            }),
            (true, KeyCode::Enter | KeyCode::KpEnter) => self.clubs_make(),
            (true, KeyCode::Escape | KeyCode::Tab) => change(|p| p.editing = false),
            (true, _) => {}
            (false, KeyCode::Escape) => self.close_clubs_page(),
            (false, KeyCode::Enter | KeyCode::KpEnter) => self.clubs_join(),
            (false, KeyCode::Tab) => change(|p| p.editing = true),
            (false, KeyCode::Down) if n > 0 => change(|p| p.picked = Some(p.picked.map_or(0, |k| (k + 1).min(n - 1)))),
            (false, KeyCode::Up) if n > 0 => change(|p| p.picked = Some(p.picked.map_or(0, |k| k.saturating_sub(1)))),
            (false, KeyCode::F5) => start_fetch(),
            _ => {}
        }
        true
    }

    pub fn clubs_char(&mut self, c: char) -> bool {
        if !is_open() {
            return false;
        }
        change(|p| {
            if p.editing && name_allowed(&p.name, c) {
                p.name.push(c);
            }
        });
        true
    }

    pub fn clubs_draw(&mut self, g: &mut Gfx) -> bool {
        let Some(p) = page() else { return false };
        poll(&self.club_compat());
        let s = Ui::new(self.draw_w, self.draw_h);
        self.view = s.view;
        crate::info_ui::dim(g, &s);
        self.draw_info_frame(g, &s, BOX.x, BOX.y, BOX.w, BOX.h);
        let (white, yellow, navy, grey) = (c15(0x7fff), c15(0x7ff0), c15(0x0018), c15(0x6318));
        let (px, py) = self.info.pointer;
        let f = crate::textsize::big(F_INFO14);
        let x = BOX.x + 24.0;
        s.put_shadowed(g, F_MANUAL20, 400.0, BOX.y + 14.0, "Shared clubs", yellow, true);
        s.put(g, f, x, BOX.y + 50.0, "Everyone in a club builds and runs the same course, together. It pauses when nobody is on.", white);
        let st = settings::get();
        let who = if settings::is_player(&st.player) { settings::nick(&st) } else { "a name made for you".into() };
        s.put_fit(
            g,
            F_ARIAL10,
            x,
            BOX.y + 74.0,
            BOX.w - 48.0,
            &format!("You play as {who} (the Weekly challenge page changes it). A club sends your nickname, a random player id and what you do in it; nothing else."),
            navy,
        );
        s.put(g, F_ARIAL10, x, LIST_TOP - 16.0, "Club", navy);
        s.put(g, F_ARIAL10, x + 360.0, LIST_TOP - 16.0, "On now", navy);
        match &p.list {
            List::Loading => s.put(g, f, x, LIST_TOP, "Loading the clubs...", white),
            List::Failed(e) => s.put_fit(g, f, x, LIST_TOP, BOX.w - 48.0, &format!("The clubs couldn't be reached: {e}"), white),
            List::Rows(rows) => {
                if rows.is_empty() {
                    s.put(g, f, x, LIST_TOP, "No clubs yet: make the first one below.", white);
                }
                for (i, r) in rows.iter().take(ROWS).enumerate() {
                    let y = LIST_TOP + i as f32 * ROW_H;
                    // Start New Game's row colours: the picked row, and a lighter one under the pointer
                    if p.picked == Some(i) {
                        s.fill(g, x - 4.0, y - 2.0, BOX.w - 40.0, ROW_H, crate::ui::rgb(0.47, 0.47, 0.70));
                    } else if py >= y - 2.0 && py < y - 2.0 + ROW_H && px >= x && px < BOX.x + BOX.w - 24.0 {
                        s.fill(g, x - 4.0, y - 2.0, BOX.w - 40.0, ROW_H, crate::ui::rgb(0.66, 0.66, 0.84));
                    }
                    let col = if !r.why_not.is_empty() {
                        grey
                    } else if p.picked == Some(i) {
                        yellow
                    } else {
                        white
                    };
                    s.put(g, f, x, y, &r.name, col);
                    s.put(g, f, x + 360.0, y, &format!("{} of 8", r.players), col);
                    if !r.why_not.is_empty() {
                        s.put_fit(g, F_ARIAL10, x + 450.0, y + 3.0, BOX.w - 498.0, &r.why_not, col);
                    }
                }
            }
        }
        s.put(g, f, x, NAME.y + 2.0, "New club", white);
        s.fill(g, NAME.x - 1.0, NAME.y - 1.0, NAME.w + 2.0, NAME.h + 2.0, c15(if p.editing { 0x7ff0 } else { 0x2d6b }));
        s.fill(g, NAME.x, NAME.y, NAME.w, NAME.h, c15(0x7fff));
        let caret = if p.editing && (miniquad::date::now() * 2.0) as i64 % 2 == 0 { "|" } else { "" };
        let shown = if p.name.is_empty() && !p.editing { "its name".to_string() } else { p.name.clone() };
        s.put(g, f, NAME.x + 6.0, NAME.y + 2.0, &format!("{shown}{caret}"), if p.name.is_empty() && !p.editing { grey } else { navy });
        draw_button(g, &s, MAKE, "Make the club", true, MAKE.has(px, py));
        s.put_fit(
            g,
            F_ARIAL10,
            MAKE.x + MAKE.w + 10.0,
            MAKE.y + 6.0,
            BOX.x + BOX.w - 24.0 - (MAKE.x + MAKE.w + 10.0),
            "a new game, everyone's",
            navy,
        );
        draw_button(g, &s, BACK, "Back", true, BACK.has(px, py));
        draw_button(g, &s, REFRESH, "Refresh", true, REFRESH.has(px, py));
        let can = matches!(&p.list, List::Rows(r) if p.picked.and_then(|k| r.get(k)).is_some_and(|r| r.why_not.is_empty()));
        draw_button(g, &s, JOIN, "Join", can, can && JOIN.has(px, py));
        g.flush();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_reads_and_greys_out_what_cant_be_played() {
        let ours = Compat::new(true, false, 0, 0, vec![]);
        let theirs = |version: &str, packs: Vec<&str>| {
            let mut c = Compat::new(true, false, 74, 5, packs.into_iter().map(String::from).collect());
            c.version = version.into();
            serde_json::to_value(c).unwrap()
        };
        let body = serde_json::json!({ "ok": true, "clubs": [
            { "id": "a", "name": "Pine", "players": 2, "compat": theirs(&ours.version, vec![]) },
            { "id": "b", "name": "Old", "players": 0, "compat": theirs("0.0.1", vec![]) },
            { "id": "c", "name": "Modded", "players": 0, "compat": theirs(&ours.version, vec!["big@1.0.0"]) },
            { "id": "d", "name": "Broken" },
        ]});
        let List::Rows(r) = read_list(body.to_string().as_bytes(), &ours) else { panic!() };
        assert_eq!(r.len(), 3, "a row without its record is left out");
        assert_eq!((r[0].players, r[0].why_not.as_str()), (2, ""), "the course size and extras come with the save");
        assert_eq!(r[1].why_not, "needs version 0.0.1");
        assert_eq!(r[2].why_not, "needs the pack big@1.0.0");
        assert!(
            matches!(read_list(br#"{"ok":false,"error":"no shared clubs here"}"#, &ours), List::Failed(e) if e == "no shared clubs here")
        );
    }

    #[test]
    fn a_row_is_picked_then_joined() {
        let mut a = App::with_clock(std::path::PathBuf::new(), 1);
        a.open_clubs_page();
        FETCH.with(|f| *f.borrow_mut() = None);
        let row = |id: &str, why: &str| ClubRow { id: id.into(), name: id.into(), players: 1, why_not: why.into() };
        change(|p| p.list = List::Rows(vec![row("a", "needs version 0.0.1"), row("b", "")]));
        let y = LIST_TOP + ROW_H * 1.5;
        assert!(a.clubs_click(400.0, y));
        assert_eq!(page().unwrap().picked, Some(1));
        // a row that can't be played isn't joined
        a.clubs_click(400.0, LIST_TOP + 2.0);
        a.clubs_click(400.0, LIST_TOP + 2.0);
        assert!(is_open() && !a.club_on());
        a.clubs_click(400.0, y);
        a.clubs_click(JOIN.x + 5.0, JOIN.y + 5.0);
        assert!(!is_open(), "the page closes");
        assert!(a.club_on(), "joining");
        a.club_leave("");
        crate::mp::lockstep::set(false);
    }

    #[test]
    fn club_names_follow_the_workers_rules() {
        assert!(name_allowed("Pine", ' ') && name_allowed("Pine", '&') && !name_allowed("Pine", '<'));
        assert!(!name_allowed(&"x".repeat(32), 'a'));
    }
}
