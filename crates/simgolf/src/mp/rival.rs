//! Rival courses in the game: this week's challenge as a row of Start New Game (once Multiplayer is on), the same land
//! and start for everyone, and its result sent to the weekly board when 2001 ends.
//!
//! The game is an ordinary scenario game whose scenario id is `*rival/<week>` (never a pack id: packs can't start with
//! `*`), so the save already remembers which week it is and nothing new goes into the save format.
//!
//! The hooks outside this folder, each one line behind the `multiplayer` feature: scenario_ui.rs (the row, and starting
//! it), app.rs (the year end) and main.rs (polling the send each frame).

use super::challenge::{self, Challenge, Measure};
use super::{result, settings};
use crate::app::App;
use crate::gfx::Gfx;
use crate::scenario_ui::ScenarioEntry;
use sg_core::land::{ExeRng, Noise};
use sg_core::scenario::{GoalDef, Scenario, ScenarioDef};
use std::cell::RefCell;

/// The scenario id of a rival-courses game is this and the week id.
pub const ID_PREFIX: &str = "*rival/";

thread_local! {
    /// A result being sent.
    static SENDING: RefCell<Option<crate::net::Request>> = const { RefCell::new(None) };
}

/// The board's address: OPENSGOLF_BOARD_URL when set (a local Worker, for tests), else the real one.
fn board_url() -> String {
    std::env::var("OPENSGOLF_BOARD_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| result::BOARD_URL.to_string())
}

fn now_s() -> i64 {
    miniquad::date::now() as i64
}

/// The challenge a game is playing, from its scenario id.
pub fn challenge_of(scenario: Option<&str>) -> Option<Challenge> {
    let week = scenario?.strip_prefix(ID_PREFIX)?;
    challenge::is_week_id(week).then(|| challenge::challenge(week))
}

fn measure_text(m: Measure) -> &'static str {
    match m {
        Measure::Cash => "the most cash",
        Measure::Fun => "the highest fun rating",
        Measure::Members => "the most members",
        Measure::Skill => "the highest skill rating",
    }
}

/// The challenge as a scenario. Its one goal only says what the week is about: it has no conditions or deadline, so it
/// never ends the game (the board decides who did best).
pub fn scenario(c: &Challenge) -> Scenario {
    let goal = measure_text(c.measure);
    Scenario {
        scenario: ScenarioDef {
            name: Some(format!("Weekly challenge {}", c.id)),
            description: format!(
                "Everyone gets the same land and start this week. Have {goal} at the end of {}. Your nickname and \
                 result go to the weekly board then.",
                c.end_year
            ),
            theme: Some(c.theme),
            difficulty: Some(c.difficulty),
            intro: format!("Weekly challenge {}: have {goal} when {} ends.", c.id, c.end_year),
            ..Default::default()
        },
        goals: vec![GoalDef {
            id: Some("weekly".into()),
            text: format!("Have {goal} when {} ends (the weekly board)", c.end_year),
            ..Default::default()
        }],
    }
}

/// The rows Start New Game adds: this week's challenge while Multiplayer is on.
pub fn entries() -> Vec<ScenarioEntry> {
    if !settings::get().on {
        return Vec::new();
    }
    let c = challenge::this_week(now_s());
    let sc = scenario(&c);
    vec![ScenarioEntry {
        id: format!("{ID_PREFIX}{}", c.id),
        name: sc.scenario.name.clone().unwrap_or_default(),
        description: sc.scenario.description.clone(),
        def: Ok(sc),
    }]
}

impl App {
    /// Starts a week's challenge (`id` is the row's scenario id); false when it isn't one.
    pub fn start_rival(&mut self, g: &mut Gfx, id: &str) -> bool {
        let Some(c) = challenge_of(Some(id)) else { return false };
        self.scen.chosen = None;
        self.course_size = sg_core::mapsize::ORIGINAL;
        // the week's seed in place of the clock: the noise, the offer, the land and the club's first draws
        self.exe_rng = ExeRng { state: c.seed };
        self.noise = Noise::new(&mut self.exe_rng);
        self.difficulty = c.difficulty.level();
        self.course_id = Some(self.exe_rng.below(0xffff));
        self.econ.sandbox = false;
        self.deal_offer(false);
        let sc = scenario(&c);
        let prop = sc.property_index();
        self.cheap_slot(prop);
        self.scen.start_money = None;
        println!("rival: weekly challenge {} ({:?}, {:?}, {}, seed {})", c.id, c.theme, c.difficulty, c.measure.id(), c.seed);
        self.scen.starting = Some((id.to_string(), sc));
        self.start_game(g, prop, false);
        true
    }

    /// At a year end: when the challenge's year is over, its result is sent.
    pub fn rival_year_end(&mut self) {
        let Some(c) = challenge_of(self.scen.goals.scenario.as_deref()) else { return };
        let f = crate::scenario_ui::facts(self);
        if f.year != c.end_year + 1 {
            return;
        }
        let s = settings::with_player();
        let (nick, group) = (settings::nick(&s), settings::group(&s));
        let platform = crate::playcount::platform_here();
        let who = result::Sender { player: &s.player, nick: &nick, group: &group, platform: &platform, version: env!("CARGO_PKG_VERSION") };
        let score = c.measure.score(&f);
        match result::result_url(&board_url(), &c, &who, self.club.cheated, &f) {
            Ok(url) => {
                println!("rival: sending {} = {score}", c.measure.id());
                SENDING.with(|r| *r.borrow_mut() = Some(crate::net::get(&url, 4096)));
                self.show_toast(&format!("Weekly challenge over: {score}. Sending it to the board..."));
            }
            Err(result::NotSent::Cheated) => {
                self.show_toast(&format!("Weekly challenge over: {score}. Cheats were used, so it isn't sent."));
            }
            Err(result::NotSent::YearNotOver) => {}
        }
    }

    /// Each frame: the board's answer to a result sent.
    pub fn rival_poll(&mut self) {
        let Some(reply) = SENDING.with(|r| {
            let mut r = r.borrow_mut();
            let reply = r.as_mut()?.poll()?;
            *r = None;
            Some(reply)
        }) else {
            return;
        };
        let msg = match reply {
            Ok(body) => answer_text(&body),
            Err(e) => format!("The board couldn't be reached: {e}"),
        };
        println!("rival: {msg}");
        self.show_toast(&msg);
    }
}

/// What the player is told of the Worker's answer.
fn answer_text(body: &[u8]) -> String {
    let v: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    match (v["ok"].as_bool(), v["better"].as_bool(), v["error"].as_str()) {
        (Some(true), Some(true), _) => "Your result is on the weekly board.".into(),
        (Some(true), _, _) => "Result sent: your best this week is still the one on the board.".into(),
        (_, _, Some("update the game")) => "Update the game to send results to the board.".into(),
        (_, _, Some(e)) => format!("The board didn't take the result: {e}."),
        _ => "The board's answer couldn't be read.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_rival_ids_are_challenges() {
        assert_eq!(challenge_of(Some("*rival/2026-W47")).map(|c| c.id), Some("2026-W47".to_string()));
        for id in [None, Some("example-desert"), Some("*rival/nope"), Some("*challenges")] {
            assert!(challenge_of(id).is_none(), "{id:?}");
        }
    }

    #[test]
    fn the_scenario_names_the_week_and_sets_the_start() {
        let c = challenge::challenge("2026-W47");
        let sc = scenario(&c);
        assert_eq!(sc.scenario.theme, Some(c.theme));
        assert_eq!(sc.scenario.difficulty, Some(c.difficulty));
        assert!(sc.scenario.name.as_deref().unwrap().contains("2026-W47"));
        // the goal never decides the game: no conditions, no deadline
        let mut goals = sc.start_goals("*rival/2026-W47", "x");
        let f = sg_core::scenario::Facts { year: 2003, cash: 1 << 40, ..Default::default() };
        assert!(goals.check(&f, Some(2002)).is_empty() && goals.outcome.is_none());
    }

    #[test]
    fn answers() {
        assert_eq!(answer_text(br#"{"ok":true,"better":true}"#), "Your result is on the weekly board.");
        assert!(answer_text(br#"{"ok":true,"better":false}"#).contains("still"));
        assert!(answer_text(br#"{"ok":false,"error":"update the game"}"#).starts_with("Update"));
        assert!(answer_text(b"<html>").contains("couldn't"));
    }
}
