//! The weekly challenge's page: what Start New Game's "Weekly challenge" row opens. It says what the week is, takes the
//! nickname and friends' group code, shows the week's board, and turns Multiplayer on (nothing is ever sent before that)
//! and starts the game.
//!
//! It is drawn in place of Start New Game while open, so it needs no screen of its own: scenario_ui.rs's draw, click and
//! key functions ask it first (one cfg-gated line each), and main.rs passes it typed characters.

use super::challenge::{Challenge, Measure};
use super::{rival, settings};
use crate::app::App;
use crate::extras::draw_button;
use crate::gfx::Gfx;
use crate::info_ui::c15;
use crate::render::Rect;
use crate::ui::{Screen as Ui, F_ARIAL10, F_INFO14, F_MANUAL20};
use sg_core::lang::{tr, trf};
use std::cell::RefCell;

const BOX: Rect = Rect { x: 40.0, y: 36.0, w: 720.0, h: 528.0 };
const NICK: Rect = Rect { x: BOX.x + 200.0, y: BOX.y + 150.0, w: 220.0, h: 22.0 };
const GROUP: Rect = Rect { x: BOX.x + 200.0, y: BOX.y + 180.0, w: 220.0, h: 22.0 };
const BACK: Rect = Rect { x: BOX.x + BOX.w - 330.0, y: BOX.y + BOX.h - 46.0, w: 100.0, h: 22.0 };
const PLAY: Rect = Rect { x: BOX.x + BOX.w - 220.0, y: BOX.y + BOX.h - 46.0, w: 190.0, h: 22.0 };
const OFF: Rect = Rect { x: BOX.x + 24.0, y: BOX.y + BOX.h - 46.0, w: 170.0, h: 22.0 };
const BOARD_TOP: f32 = BOX.y + 262.0;
/// Board rows shown.
const ROWS: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Nick,
    Group,
}

/// A row of the board, as the Worker sends it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub place: i64,
    pub nick: String,
    pub score: i64,
    pub mine: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Board {
    Loading,
    Rows { rows: Vec<Row>, me: Option<(i64, i64)> },
    Failed(String),
}

#[derive(Clone)]
struct Lobby {
    /// The row's scenario id (`*rival/<week>`).
    id: String,
    challenge: Challenge,
    nick: String,
    group: String,
    editing: Option<Field>,
    board: Board,
    /// The group code of the board shown (empty: the public one).
    board_group: String,
}

thread_local! {
    static LOBBY: RefCell<Option<Lobby>> = const { RefCell::new(None) };
    static FETCH: RefCell<Option<crate::net::Request>> = const { RefCell::new(None) };
}

fn lobby() -> Option<Lobby> {
    LOBBY.with(|l| l.borrow().clone())
}

fn change(f: impl FnOnce(&mut Lobby)) {
    LOBBY.with(|l| {
        if let Some(l) = l.borrow_mut().as_mut() {
            f(l)
        }
    })
}

/// Whether a field is being typed in (a phone shows its keyboard: touch.rs).
pub fn editing() -> bool {
    LOBBY.with(|l| l.borrow().as_ref().is_some_and(|l| l.editing.is_some()))
}

/// Whether the page is open.
pub fn is_open() -> bool {
    LOBBY.with(|l| l.borrow().is_some())
}

/// The board's answer read: its rows and the player's own place.
pub fn read_board(body: &[u8]) -> Board {
    let v: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(_) => return Board::Failed("the board's answer couldn't be read".into()),
    };
    if v["ok"].as_bool() != Some(true) {
        return Board::Failed(v["error"].as_str().unwrap_or("the board said no").to_string());
    }
    let rows = v["rows"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| Row {
                    place: r["place"].as_i64().unwrap_or(0),
                    nick: r["nick"].as_str().unwrap_or("?").chars().take(16).collect(),
                    score: r["score"].as_i64().unwrap_or(0),
                    mine: r["mine"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    let me = v["me"].as_object().map(|m| (m["place"].as_i64().unwrap_or(0), m["score"].as_i64().unwrap_or(0)));
    Board::Rows { rows, me }
}

/// A score as the measure shows it.
pub fn score_text(m: Measure, v: i64) -> String {
    match m {
        Measure::Cash => format!("${}", sg_core::economy::money_digits(v)),
        // the club keeps skill in hundredths
        Measure::Skill => format!("{:.2}", v as f64 / 100.0),
        Measure::Fun | Measure::Members => v.to_string(),
    }
}

pub fn measure_name(m: Measure) -> &'static str {
    super::texts();
    match m {
        Measure::Cash => tr("STR_MP_MEASURE_CASH"),
        Measure::Fun => tr("STR_MP_MEASURE_FUN"),
        Measure::Members => tr("STR_MP_MEASURE_MEMBERS"),
        Measure::Skill => tr("STR_MP_MEASURE_SKILL"),
    }
}

/// Whether a typed character may go into a field (the Worker's rules: names.js, index.js isGroup).
pub fn allowed(f: Field, text: &str, c: char) -> bool {
    match f {
        Field::Nick => text.chars().count() < 16 && (c.is_ascii_alphanumeric() || " _.-".contains(c)),
        Field::Group => text.len() < 6 && c.is_ascii_alphanumeric(),
    }
}

fn start_fetch(l: &Lobby) {
    let s = settings::get();
    let mut url = format!("{}/board?c={}", rival::board_url(), l.challenge.id);
    let g = settings::group(&settings::Settings { group: l.group.clone(), ..Default::default() });
    if !g.is_empty() {
        url.push_str(&format!("&g={g}"));
    }
    if settings::is_player(&s.player) {
        url.push_str(&format!("&p={}", s.player));
    }
    FETCH.with(|f| *f.borrow_mut() = Some(crate::net::get(&url, 64 * 1024)));
    change(|l| {
        l.board = Board::Loading;
        l.board_group = g;
    });
}

/// Each frame: the board's answer, once it is in.
pub fn poll() {
    let Some(reply) = FETCH.with(|f| {
        let mut f = f.borrow_mut();
        let r = f.as_mut()?.poll()?;
        *f = None;
        Some(r)
    }) else {
        return;
    };
    let b = match reply {
        Ok(body) => read_board(&body),
        Err(e) => {
            println!("board: couldn't be fetched: {e}");
            Board::Failed(tr("STR_MP_CHECK_CONNECTION").into())
        }
    };
    change(|l| l.board = b);
}

impl App {
    /// Opens the page for a Start New Game row (`id`: `*rival/<week>`).
    pub fn open_rival_lobby(&mut self, id: &str) -> bool {
        let Some(c) = rival::challenge_of(Some(id)) else { return false };
        let s = settings::get();
        let l = Lobby {
            id: id.to_string(),
            challenge: c,
            nick: s.nick.clone(),
            group: s.group.clone(),
            editing: None,
            board: Board::Loading,
            board_group: String::new(),
        };
        LOBBY.with(|x| *x.borrow_mut() = Some(l.clone()));
        start_fetch(&l);
        self.hover = -1;
        true
    }

    fn close_rival_lobby(&mut self) {
        LOBBY.with(|x| *x.borrow_mut() = None);
        FETCH.with(|f| *f.borrow_mut() = None);
        self.hover = -1;
    }

    /// Keeps the typed nickname and group code (Multiplayer stays as it was).
    fn keep_lobby_fields(&self, l: &Lobby) {
        let mut s = settings::get();
        if (s.nick.as_str(), s.group.as_str()) != (l.nick.trim(), l.group.as_str()) {
            s.nick = l.nick.trim().to_string();
            s.group = l.group.clone();
            settings::save(s);
        }
    }

    fn finish_editing(&mut self) {
        let Some(l) = lobby() else { return };
        if l.editing == Some(Field::Group) {
            start_fetch(&l);
        }
        change(|l| l.editing = None);
        self.keep_lobby_fields(&l);
    }

    fn lobby_play(&mut self, g: &mut Gfx) {
        let Some(l) = lobby() else { return };
        self.keep_lobby_fields(&l);
        let mut s = settings::with_player();
        if !s.on {
            s.on = true;
            settings::save(s);
        }
        self.close_rival_lobby();
        self.start_rival(g, &l.id);
    }

    fn lobby_turn_off(&mut self) {
        let mut s = settings::get();
        s.on = false;
        settings::save(s);
        self.close_rival_lobby();
        // the row goes on being listed: it is how Multiplayer is turned on again
    }

    /// A click while the page is open (false: it isn't).
    pub fn rival_lobby_click(&mut self, g: &mut Gfx, x: f32, y: f32) -> bool {
        if self.clubs_click(x, y) || self.leagues_click(g, x, y) {
            return true;
        }
        let Some(l) = lobby() else { return false };
        if l.editing.is_some() && !NICK.has(x, y) && !GROUP.has(x, y) {
            self.finish_editing();
        }
        if NICK.has(x, y) {
            change(|l| l.editing = Some(Field::Nick));
        } else if GROUP.has(x, y) {
            change(|l| l.editing = Some(Field::Group));
        } else if PLAY.has(x, y) {
            self.lobby_play(g);
        } else if BACK.has(x, y) || !BOX.has(x, y) {
            self.close_rival_lobby();
        } else if settings::get().on && OFF.has(x, y) {
            self.lobby_turn_off();
        }
        true
    }

    /// A key while the page is open (false: it isn't).
    pub fn rival_lobby_key(&mut self, g: &mut Gfx, k: miniquad::KeyCode) -> bool {
        use miniquad::KeyCode;
        if self.clubs_key(k) || self.leagues_key(g, k) {
            return true;
        }
        let Some(l) = lobby() else { return false };
        match (l.editing, k) {
            (Some(_), KeyCode::Backspace) => change(|l| {
                let t = if l.editing == Some(Field::Nick) { &mut l.nick } else { &mut l.group };
                t.pop();
            }),
            (Some(Field::Nick), KeyCode::Tab) => {
                self.finish_editing();
                change(|l| l.editing = Some(Field::Group));
            }
            (Some(_), KeyCode::Escape | KeyCode::Enter | KeyCode::KpEnter | KeyCode::Tab) => self.finish_editing(),
            (Some(_), _) => {}
            (None, KeyCode::Escape) => self.close_rival_lobby(),
            (None, KeyCode::Enter | KeyCode::KpEnter) => self.lobby_play(g),
            (None, KeyCode::Tab) => change(|l| l.editing = Some(Field::Nick)),
            _ => {}
        }
        true
    }

    /// A typed character (nothing unless a field is being edited).
    pub fn rival_lobby_char(&mut self, c: char) {
        if self.clubs_char(c) || self.leagues_char(c) {
            return;
        }
        change(|l| match l.editing {
            Some(Field::Nick) if allowed(Field::Nick, &l.nick, c) => l.nick.push(c),
            Some(Field::Group) if allowed(Field::Group, &l.group, c) => l.group.push(c.to_ascii_uppercase()),
            _ => {}
        });
    }

    /// Draws the page while it is open (false: it isn't).
    pub fn rival_lobby_draw(&mut self, g: &mut Gfx) -> bool {
        // the shared clubs' page (clubs_ui.rs) is drawn the same way
        if self.clubs_draw(g) || self.leagues_draw(g) {
            return true;
        }
        let Some(l) = lobby() else { return false };
        let s = Ui::new(self.draw_w, self.draw_h);
        self.view = s.view;
        crate::info_ui::dim(g, &s);
        self.draw_info_frame(g, &s, BOX.x, BOX.y, BOX.w, BOX.h);
        let (white, yellow, navy, grey) = (c15(0x7fff), c15(0x7ff0), c15(0x0018), c15(0x6318));
        let (px, py) = self.info.pointer;
        let c = &l.challenge;
        let f = crate::textsize::big(F_INFO14);
        let x = BOX.x + 24.0;
        s.put_shadowed(g, F_MANUAL20, 400.0, BOX.y + 14.0, &trf("STR_MP_WEEKLY_NAME", &[("WEEK", &c.id)]), yellow, true);
        let sc = rival::scenario(c);
        let place = sg_core::data::properties().get(sc.property_index()).map(|p| p.name().to_string()).unwrap_or_default();
        let diff = sg_core::lang::tr(crate::render::DIFFICULTY_KEYS[c.difficulty.level() as usize]);
        s.put(
            g,
            f,
            x,
            BOX.y + 50.0,
            format!(
                "{} {}",
                trf(
                    "STR_MP_WEEKLY_PLACE",
                    &[
                        ("PLACE", &place),
                        ("DIFFICULTY", &diff),
                        ("MEASURE", &measure_name(c.measure).to_lowercase()),
                        ("YEAR", &c.end_year)
                    ],
                ),
                rival::kind_text(c.kind)
            )
            .trim_end(),
            white,
        );
        s.put(g, f, x, BOX.y + 72.0, tr("STR_MP_WEEKLY_SAME"), white);
        let on = settings::get().on;
        let note = if on { tr("STR_MP_ON_NOTE") } else { tr("STR_MP_OFF_NOTE") };
        s.put_fit(g, F_ARIAL10, x, BOX.y + 104.0, BOX.w - 48.0, note, navy);
        s.put_fit(g, F_ARIAL10, x, BOX.y + 118.0, BOX.w - 48.0, tr("STR_MP_NOTHING_ELSE"), navy);
        for (label, r, text, field, hint) in [
            (tr("STR_MP_NICKNAME"), NICK, &l.nick, Field::Nick, tr("STR_MP_NICKNAME_HINT")),
            (tr("STR_MP_GROUP"), GROUP, &l.group, Field::Group, tr("STR_MP_GROUP_HINT")),
        ] {
            let editing = l.editing == Some(field);
            s.put(g, f, x, r.y + 2.0, label, white);
            s.fill(g, r.x - 1.0, r.y - 1.0, r.w + 2.0, r.h + 2.0, c15(if editing { 0x7ff0 } else { 0x2d6b }));
            s.fill(g, r.x, r.y, r.w, r.h, c15(0x7fff));
            let shown = if text.is_empty() && !editing {
                if field == Field::Nick {
                    let s = settings::get();
                    if settings::is_player(&s.player) {
                        settings::nick(&s)
                    } else {
                        tr("STR_MP_NICKNAME_MADE").into()
                    }
                } else {
                    tr("STR_MP_GROUP_NONE").into()
                }
            } else {
                text.clone()
            };
            let caret = if editing && (miniquad::date::now() * 2.0) as i64 % 2 == 0 { "|" } else { "" };
            s.put(g, f, r.x + 6.0, r.y + 2.0, &format!("{shown}{caret}"), if text.is_empty() && !editing { grey } else { navy });
            s.put_fit(g, F_ARIAL10, r.x + r.w + 12.0, r.y + 6.0, BOX.x + BOX.w - 24.0 - (r.x + r.w + 12.0), hint, navy);
        }
        let grp = &l.board_group;
        let title = if grp.is_empty() { tr("STR_MP_BOARD_TITLE").to_string() } else { trf("STR_MP_BOARD_TITLE_GROUP", &[("GROUP", grp)]) };
        s.put_shadowed(g, crate::ui::F_MANUAL15, x, BOARD_TOP - 26.0, &title, yellow, false);
        match &l.board {
            Board::Loading => s.put(g, f, x, BOARD_TOP, tr("STR_MP_BOARD_LOADING"), white),
            Board::Failed(e) => s.put_fit(g, f, x, BOARD_TOP, BOX.w - 48.0, &trf("STR_MP_BOARD_UNREACHABLE", &[("ERROR", e)]), white),
            Board::Rows { rows, me } => {
                if rows.is_empty() {
                    s.put(g, f, x, BOARD_TOP, tr("STR_MP_BOARD_EMPTY"), white);
                }
                for (i, r) in rows.iter().take(ROWS).enumerate() {
                    let y = BOARD_TOP + i as f32 * 18.0;
                    let col = if r.mine { yellow } else { white };
                    s.put(g, f, x, y, &format!("{}.", r.place), col);
                    s.put(g, f, x + 40.0, y, &r.nick, col);
                    s.put(g, f, x + 300.0, y, &score_text(c.measure, r.score), col);
                }
                if let Some((place, score)) = me {
                    let y = BOARD_TOP + ROWS as f32 * 18.0 + 4.0;
                    s.put(g, f, x, y, &trf("STR_MP_BOARD_YOU", &[("PLACE", place), ("SCORE", &score_text(c.measure, *score))]), yellow);
                }
            }
        }
        draw_button(g, &s, BACK, tr("STR_MP_BACK"), true, BACK.has(px, py));
        draw_button(g, &s, PLAY, tr(if on { "STR_MP_PLAY" } else { "STR_MP_TURN_ON_AND_PLAY" }), true, PLAY.has(px, py));
        if on {
            draw_button(g, &s, OFF, tr("STR_MP_TURN_OFF"), true, OFF.has(px, py));
        }
        g.flush();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boards_read() {
        let b = read_board(br#"{"ok":true,"rows":[{"place":1,"nick":"Test Pro","score":40000,"mine":true}],"me":{"place":60,"score":5}}"#);
        assert_eq!(b, Board::Rows { rows: vec![Row { place: 1, nick: "Test Pro".into(), score: 40000, mine: true }], me: Some((60, 5)) });
        assert_eq!(read_board(br#"{"ok":true,"rows":[],"me":null}"#), Board::Rows { rows: vec![], me: None });
        assert!(matches!(read_board(br#"{"ok":false,"error":"bad board"}"#), Board::Failed(e) if e == "bad board"));
        assert!(matches!(read_board(b"nope"), Board::Failed(_)));
    }

    #[test]
    fn typing_follows_the_boards_rules() {
        assert!(allowed(Field::Nick, "Big", ' ') && allowed(Field::Nick, "Big", '-') && !allowed(Field::Nick, "Big", '<'));
        assert!(!allowed(Field::Nick, &"x".repeat(16), 'a'));
        assert!(allowed(Field::Group, "FRIE", 'n') && !allowed(Field::Group, "FRIEND", 'S') && !allowed(Field::Group, "", '-'));
    }

    #[test]
    fn scores_read_as_the_measure() {
        assert_eq!(score_text(Measure::Skill, 1250), "12.50");
        assert_eq!(score_text(Measure::Members, 40), "40");
        assert!(score_text(Measure::Cash, 40000).starts_with('$'));
    }
}
