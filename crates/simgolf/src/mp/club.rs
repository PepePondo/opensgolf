//! A shared club in the game: making one from the game being played, joining one, playing it through the club's room
//! (multiplayer/worker/src/club.js), and leaving.
//!
//! While a club is on, the game's own clock stands still (update_props plays nothing) and [`App::club_frame`] plays the
//! ticks the club's clock has reached (session.rs), applying everyone's actions at the ticks the room stamped. A joiner
//! loads the room's latest save and plays the actions since it, quickly, up to the clock. When the room asks for a save
//! the game sends one at that tick, and every [`CHECK_EVERY`] ticks it sends a hash of the game: one that differs from
//! the others' is told to load the club again, and so is one an action reached too late. A lost connection is tried
//! again, and then the club goes on as the player's own game.

use super::conn::{club_url, Conn, Event};
use super::session::{Seat, Session, ToGame, ToServer};
use super::version::{Compat, PROTOCOL};
use crate::app::App;
use crate::gfx::Gfx;
use sg_core::lang::{tr, trf};

/// Ticks between the hashes sent for the out-of-step check.
pub const CHECK_EVERY: u32 = 128;
/// Real time a frame may spend playing ticks, in ms (more when a joiner is catching up).
const FRAME_BUDGET_MS: f64 = 12.0;
const CATCH_UP_BUDGET_MS: f64 = 60.0;
/// Seconds between pings (the clock's offset between this computer and the server).
const PING_EVERY_MS: f64 = 10_000.0;
/// Tries to reconnect before the club goes on as the player's own game.
const RETRIES: u32 = 6;

/// Where a club is.
#[derive(Debug)]
pub enum Phase {
    /// Asking the server to make a club; the answer is its id.
    Making(crate::net::Request),
    /// Connecting (or connected and waiting for the Welcome).
    Connecting,
    /// Welcomed, waiting for the save to start from.
    Joining,
    Playing,
}

impl std::fmt::Debug for crate::net::Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Request")
    }
}

/// The club being played.
#[derive(Debug)]
pub struct Live {
    pub id: String,
    pub name: String,
    pub phase: Phase,
    conn: Option<Conn>,
    /// A new club's owner: the game being played is the club's start (no save comes).
    starting: bool,
    /// A save the room asked for, at this tick.
    ask: Option<u32>,
    pub players: Vec<Seat>,
    ping_sent: Option<f64>,
    next_ping: f64,
    best_rtt: f64,
    retries: u32,
    retry_at: f64,
    /// The last time a frame played ticks behind the clock (catching up), for the screen's note.
    pub behind: u32,
    last_check: Option<u32>,
    /// Times the game was loaded again (out of step).
    pub reloads: u32,
}

impl Live {
    fn new(id: &str, name: &str, phase: Phase, starting: bool) -> Live {
        Live {
            id: id.into(),
            name: name.into(),
            phase,
            conn: None,
            starting,
            ask: None,
            players: Vec::new(),
            ping_sent: None,
            next_ping: 0.0,
            best_rtt: f64::INFINITY,
            retries: 0,
            retry_at: 0.0,
            behind: 0,
            last_check: None,
            reloads: 0,
        }
    }

    fn send(&self, m: &ToServer) {
        if let Some(c) = &self.conn {
            c.send(m);
        }
    }
}

/// The server's address (the leaderboard's Worker serves the clubs too).
pub fn server() -> String {
    super::rival::board_url()
}

/// This computer's time in ms.
fn now_ms() -> f64 {
    crate::app::now() * 1000.0
}

/// The URL that makes a club.
pub fn make_url(base: &str, player: &str, name: &str, listed: bool, compat: &Compat) -> String {
    let body = serde_json::json!({
        "protocol": PROTOCOL, "player": player, "nick": super::settings::nick(&super::settings::get()), "name": name, "compat": compat,
        "listed": listed,
    });
    format!("{}/clubs/new?d={}", base.trim_end_matches('/'), super::result::base64url(body.to_string().as_bytes()))
}

/// The shared game's state that a save file leaves out, sent with the room's save: what a replay's log keeps at its
/// start (replay.rs), and the growing plants, which the drawing keeps in a game of one's own but the simulation reads in
/// a shared one (lockstep.rs).
#[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Extra {
    pub start: sg_core::action::Start,
    /// The growing plants' tick counts, by tile (sorted).
    pub sprouts: Vec<((i32, i32), u32)>,
}

impl App {
    /// The room's save of this game: the save file (base64) and the [`Extra`] (JSON).
    pub fn club_snapshot(&self) -> Result<(String, String), String> {
        Ok((crate::share::base64(&self.save_bytes(6)?), self.club_extra()?))
    }

    /// This game's [`Extra`], as JSON.
    fn club_extra(&self) -> Result<String, String> {
        let mut sprouts: Vec<_> = self.sprouts.iter().map(|(&k, &v)| (k, v)).collect();
        sprouts.sort_unstable();
        serde_json::to_string(&Extra { start: self.log_start(), sprouts }).map_err(|e| e.to_string())
    }

    /// Loads the room's save (`g` None: without art, the tests). The game must then be the saver's: its hash is checked.
    /// With `keep_art` (the saver loading its own save) the theme's textures stay.
    pub fn club_restore(&mut self, g: Option<&mut Gfx>, data: &str, extra: &str, keep_art: bool) -> Result<(), String> {
        let extra: Extra = serde_json::from_str(extra).map_err(|e| format!("the save's extra: {e}"))?;
        let b = crate::share::unbase64(data).ok_or_else(|| "not base64".to_string())?;
        let s = crate::app::read_save_bytes(&b, "club")?;
        self.keep_plan_on_load = extra.start.planned;
        let r = match g {
            Some(g) if keep_art => {
                self.undo.clear();
                self.load_save_with(g, s, true).map(|_| self.log_restart())
            }
            Some(g) => self.load_save(g, s),
            None => {
                self.undo.clear();
                self.editor = None;
                self.load_save_into(None, s, false)
            }
        };
        self.keep_plan_on_load = false;
        r?;
        self.apply_start(&extra.start);
        self.sprouts = extra.sprouts.into_iter().collect();
        let now = self.replay_hash();
        // (the saver's own load can't match: its record is from before it)
        if !keep_art && now != extra.start.hash {
            let mine = self.start_parts();
            let differ: Vec<&str> =
                extra.start.parts.iter().filter(|(k, h)| mine.iter().any(|(m, g)| m == k && g != h)).map(|(k, _)| k.as_str()).collect();
            return Err(format!("the loaded game differs from the club's in {}", differ.join(" ")));
        }
        Ok(())
    }

    /// What a club this game plays must match: the version and the rules settings (the course size, the gameplay
    /// extras and the rest come with the save, so they are the club's anyway).
    pub fn club_compat(&self) -> Compat {
        let rules = (crate::enhanced::fix_bugs(), crate::extras::random_trees());
        let packs = self
            .content
            .stamps
            .iter()
            .filter(|p| matches!(p.kind, sg_core::packs::Kind::Mod | sg_core::packs::Kind::Scenario))
            .map(|p| format!("{}@{}", p.id, p.version))
            .collect();
        Compat::new(rules.0, rules.1, 0, 0, packs)
    }

    /// Whether a shared club is on (the game's own clock stands still).
    pub fn club_on(&self) -> bool {
        self.mp.live.is_some()
    }

    /// Makes a club from the game being played: the club starts where this game is.
    pub fn club_make(&mut self, name: &str, listed: bool) {
        if self.terrain.ty.is_empty() || self.club_on() {
            return;
        }
        super::settings::with_player();
        let url = make_url(&server(), &self.club_player(), name, listed, &self.club_compat());
        self.mp.live = Some(Live::new("", name, Phase::Making(crate::net::get(&url, 4096)), true));
        self.show_toast(&trf("STR_MP_CLUB_MAKING", &[("CLUB", &name)]));
    }

    /// Joins the club `id` (its game replaces the one being played).
    pub fn club_join(&mut self, id: &str, name: &str) {
        if self.club_on() {
            self.club_leave("");
        }
        super::settings::with_player();
        self.mp.live = Some(Live::new(id, name, Phase::Connecting, false));
        self.club_connect();
        self.show_toast(&trf("STR_MP_CLUB_JOINING", &[("CLUB", &name)]));
    }

    /// Leaves the club; the game goes on as the player's own copy (`why`, when not empty, is told).
    pub fn club_leave(&mut self, why: &str) {
        let Some(l) = self.mp.live.take() else { return };
        drop(l);
        self.mp.session = None;
        super::lockstep::set(false);
        // the game's own clock goes on from here
        self.sim_time = self.time;
        self.tick_acc = 0.0;
        if !why.is_empty() {
            self.show_toast(why);
        }
    }

    /// The pause key in a club: pauses or goes on for everyone. False when not in a club.
    pub fn club_pause_toggle(&mut self) -> bool {
        let Some(l) = &self.mp.live else { return false };
        let rate = self.mp.session.as_ref().and_then(|s| s.clock).map(|c| c.rate).unwrap_or(0);
        let speed = self.speed.clamp(1, 4);
        l.send(&ToServer::Speed(if rate > 0 { 0 } else { speed }));
        true
    }

    /// The player's id (a test plays two players in one process: `mp.player`).
    fn club_player(&self) -> String {
        self.mp.player.clone().unwrap_or_else(|| super::settings::get().player)
    }

    fn hello(&self) -> ToServer {
        ToServer::Hello {
            protocol: PROTOCOL,
            player: self.club_player(),
            nick: super::settings::nick(&super::settings::get()),
            compat: self.club_compat(),
            tick: self.game_tick,
        }
    }

    /// Opens the club's connection (again) and says hello. A reconnect starts from the room's save.
    fn club_connect(&mut self) {
        let hello = self.hello();
        let Some(l) = self.mp.live.as_mut() else { return };
        let c = Conn::open(&club_url(&server(), &l.id));
        c.send(&hello);
        l.conn = Some(c);
        if !matches!(l.phase, Phase::Making(_)) {
            l.phase = Phase::Connecting;
        }
        l.ask = None;
        l.ping_sent = None;
        l.best_rtt = f64::INFINITY;
        self.mp.session = None;
    }

    /// Loads the club again from the room's save (the game fell out of step).
    fn club_reload(&mut self, why: &str) {
        println!("club: loading again ({why})");
        if let Some(l) = self.mp.live.as_mut() {
            l.starting = false;
            l.reloads += 1;
        }
        self.club_connect();
    }

    /// Each frame while a club is on: the network, then the ticks the club's clock has reached. `g` is None in the tests
    /// (a joiner's save is then loaded without art).
    pub fn club_frame(&mut self, g: Option<&mut Gfx>) {
        self.club_frame_at(g, now_ms());
    }

    pub(crate) fn club_frame_at(&mut self, mut g: Option<&mut Gfx>, ms: f64) {
        self.clubs_make_pending();
        // back on the title menu: the club is left
        if self.club_on() && self.screen == crate::app::Screen::Menu {
            self.club_leave(tr("STR_MP_CLUB_LEFT"));
        }
        let Some(l) = self.mp.live.as_mut() else { return };
        // a club being made: the server's answer
        if let Phase::Making(req) = &mut l.phase {
            let Some(reply) = req.poll() else { return };
            let id = reply
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(String::from));
            match id {
                Some(id) => {
                    l.id = id;
                    l.phase = Phase::Connecting;
                    self.club_connect();
                }
                None => self.club_leave(tr("STR_MP_CLUB_NOT_MADE")),
            }
            return;
        }
        // waiting to connect again
        if l.conn.is_none() {
            if ms >= l.retry_at {
                self.club_connect();
            }
            return;
        }
        let events = l.conn.as_mut().map(|c| c.poll()).unwrap_or_default();
        for e in events {
            if !self.club_event(g.as_deref_mut(), e, ms) {
                return;
            }
        }
        self.club_play(g, ms);
    }

    /// One event from the connection; false when the club ended or is connecting again.
    fn club_event(&mut self, g: Option<&mut Gfx>, e: Event, ms: f64) -> bool {
        match e {
            Event::Open => {}
            Event::Closed(why) => {
                let Some(l) = self.mp.live.as_mut() else { return false };
                l.conn = None;
                l.retries += 1;
                if l.retries > RETRIES {
                    self.club_leave(&trf("STR_MP_CLUB_LOST", &[("WHY", &why)]));
                    return false;
                }
                l.retry_at = ms + 1000.0 * (1u32 << l.retries.min(5)) as f64;
                println!("club: connection closed ({why}); trying again");
                return false;
            }
            Event::Msg(m) => return self.club_message(g, m, ms),
        }
        true
    }

    fn club_message(&mut self, g: Option<&mut Gfx>, m: ToGame, ms: f64) -> bool {
        let Some(l) = self.mp.live.as_mut() else { return false };
        match m {
            ToGame::Welcome { me, ms: server_ms, clock } => {
                let mut s = Session::new(me);
                s.clock = Some(clock);
                s.offset_ms = server_ms - ms;
                self.mp.session = Some(s);
                l.retries = 0;
                l.next_ping = ms;
                l.phase = if l.starting { Phase::Playing } else { Phase::Joining };
                super::lockstep::set(true);
            }
            ToGame::Refused { reason, parts } => {
                let why = if parts.is_empty() { reason } else { format!("{reason} ({})", parts.join(", ")) };
                self.club_leave(&trf("STR_MP_CLUB_REFUSED", &[("WHY", &why)]));
                return false;
            }
            ToGame::Save { tick, data, extra } => {
                if !matches!(l.phase, Phase::Joining) {
                    return true;
                }
                if let Err(e) = self.club_restore(g, &data, &extra, false) {
                    self.club_leave(&trf("STR_MP_CLUB_NOT_LOADED", &[("ERROR", &e)]));
                    return false;
                }
                if self.game_tick != tick {
                    self.club_leave(tr("STR_MP_CLUB_BAD_SAVE"));
                    return false;
                }
                self.screen = crate::app::Screen::Play;
                if let Some(l) = self.mp.live.as_mut() {
                    l.phase = Phase::Playing;
                }
                super::lockstep::set(true);
            }
            m @ (ToGame::Act(_) | ToGame::Clock(_)) => {
                let t = self.game_tick;
                if let Some(s) = self.mp.session.as_mut() {
                    s.receive(m, t);
                }
            }
            ToGame::SaveAsk(t) => l.ask = Some(t),
            ToGame::Players(p) => {
                // who came and went, once the club is being played
                let me = self.mp.session.as_ref().map(|s| s.me);
                let told: Vec<String> = if matches!(l.phase, Phase::Playing) {
                    let came = p.iter().filter(|n| Some(n.me) != me && !l.players.iter().any(|o| o.me == n.me));
                    let went = l.players.iter().filter(|o| Some(o.me) != me && !p.iter().any(|n| n.me == o.me));
                    came.map(|n| trf("STR_MP_CLUB_CAME", &[("NICK", &n.nick)]))
                        .chain(went.map(|o| trf("STR_MP_CLUB_WENT", &[("NICK", &o.nick)])))
                        .collect()
                } else {
                    Vec::new()
                };
                l.players = p;
                for t in told {
                    self.show_toast(&t);
                }
            }
            ToGame::Reload(t) => {
                self.club_reload(&format!("out of step at tick {t}"));
                return false;
            }
            ToGame::Pong(sent, server_ms) => {
                let rtt = ms - sent;
                if rtt >= 0.0 && rtt <= l.best_rtt * 1.5 {
                    l.best_rtt = l.best_rtt.min(rtt);
                    if let Some(s) = self.mp.session.as_mut() {
                        s.offset_ms = server_ms - (sent + rtt / 2.0);
                    }
                }
                l.ping_sent = None;
            }
        }
        true
    }

    /// Plays the ticks the club's clock has reached, sending saves and hashes on the way.
    fn club_play(&mut self, mut g: Option<&mut Gfx>, ms: f64) {
        let Some(l) = self.mp.live.as_mut() else { return };
        if !matches!(l.phase, Phase::Playing) {
            return;
        }
        if ms >= l.next_ping && l.ping_sent.is_none() {
            l.send(&ToServer::Ping(ms));
            l.ping_sent = Some(ms);
            l.next_ping = ms + PING_EVERY_MS;
        }
        let Some(s) = self.mp.session.as_ref() else { return };
        let to = s.may_run_to(ms);
        self.paused = s.clock.is_none_or(|c| c.rate == 0);
        let behind = to.saturating_sub(self.game_tick);
        let budget = if behind > 40 { CATCH_UP_BUDGET_MS } else { FRAME_BUDGET_MS };
        let t0 = now_ms();
        loop {
            self.club_between_ticks(g.as_deref_mut());
            if self.game_tick >= to || now_ms() - t0 > budget {
                break;
            }
            self.lockstep_step();
            if self.mp.session.as_ref().is_some_and(|s| s.late.is_some()) {
                let t = self.mp.session.as_ref().and_then(|s| s.late).unwrap_or(0);
                self.club_reload(&format!("an action for tick {t} came too late"));
                return;
            }
        }
        if let Some(l) = self.mp.live.as_mut() {
            l.behind = to.saturating_sub(self.game_tick);
        }
        // what the player did this frame
        let out = self.mp.session.as_mut().map(|s| s.take_outbox()).unwrap_or_default();
        if let Some(l) = self.mp.live.as_ref() {
            for m in &out {
                l.send(m);
            }
        }
    }

    /// The point between two ticks (the game has played up to `game_tick`): a save the room asked for, and the hash.
    fn club_between_ticks(&mut self, g: Option<&mut Gfx>) {
        let tick = self.game_tick;
        let Some(l) = self.mp.live.as_ref() else { return };
        if l.ask.is_some_and(|t| t <= tick) {
            // the saver goes on from its own save, as a joiner does from it: a game just loaded differs from one played
            // up to the same point in ways only the next ticks show (stories read again, hole records rebuilt)
            let sent = self.club_snapshot().and_then(|(data, extra)| {
                self.club_restore(g, &data, &extra, true)?;
                // what a joiner's load is checked against: this game as loaded
                Ok((data, self.club_extra()?))
            });
            let Some(l) = self.mp.live.as_mut() else { return };
            l.ask = None;
            match sent {
                Ok((data, extra)) => l.send(&ToServer::Save { tick, data, extra }),
                Err(e) => println!("club: no save for the room: {e}"),
            }
        }
        if tick.is_multiple_of(CHECK_EVERY) {
            let checked = self.mp.live.as_ref().is_some_and(|l| l.last_check == Some(tick));
            if !checked {
                let hash = format!("{:016x}", self.replay_hash());
                if let Some(l) = self.mp.live.as_mut() {
                    l.send(&ToServer::Check { tick, hash });
                    l.last_check = Some(tick);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp::lockstep::tests::{paint, shared_game};
    use sg_core::action::Action;

    #[test]
    fn a_club_is_made_with_the_game_record() {
        let a = App::with_clock(std::path::PathBuf::new(), 1);
        let u = make_url("https://x.dev/", "00000000000000aa", "Pine", true, &a.club_compat());
        assert!(u.starts_with("https://x.dev/clubs/new?d="), "{u}");
    }

    /// A game loaded from another's room save plays on exactly as the other does (nothing the save leaves out matters).
    #[test]
    fn a_joiner_from_the_save_stays_in_step() {
        crate::mp::lockstep::set(true);
        let mut a = shared_game(1);
        a.apply(&Action::Speed(1));
        a.apply(&Action::OpenHole);
        for _ in 0..40 {
            a.lockstep_step();
        }
        a.apply(&paint(13, &[(20, 20), (21, 20), (22, 20)]));
        for _ in 0..5 {
            a.lockstep_step();
        }
        // the save is taken with trees still growing and the game at speed 4
        a.apply(&Action::Speed(4));
        let (data, extra) = a.club_snapshot().unwrap();
        a.club_restore(None, &data, &extra, true).unwrap();
        let extra = a.club_extra().unwrap();
        let mut b = shared_game(2);
        b.terrain.paint(40, 40, 13, 0);
        b.club_restore(None, &data, &extra, false).unwrap();
        assert_eq!(b.game_tick, a.game_tick);
        for t in 0..600 {
            if t == 100 {
                a.apply(&paint(4, &[(25, 25)]));
                b.apply(&paint(4, &[(25, 25)]));
            }
            assert_eq!(a.replay_hash(), b.replay_hash(), "the same at tick {}", a.game_tick);
            a.lockstep_step();
            b.lockstep_step();
        }
        assert_eq!(a.fingerprint().total(), b.fingerprint().total());
        crate::mp::lockstep::set(false);
    }

    /// Two games playing one club through a running Worker: OPENSGOLF_CLUB_TEST=http://127.0.0.1:8787 (wrangler dev in
    /// multiplayer/worker) or the deployed one. Skipped without it.
    #[test]
    fn two_games_through_the_server() {
        let Ok(url) = std::env::var("OPENSGOLF_CLUB_TEST") else { return };
        std::env::set_var("OPENSGOLF_BOARD_URL", &url);
        let mut a = shared_game(0);
        let mut b = shared_game(0);
        a.mp.session = None;
        b.mp.session = None;
        a.mp.player = Some(format!("{:016x}", now_ms() as u64));
        b.mp.player = Some(format!("{:016x}", now_ms() as u64 + 1));
        // B's own game differs: the club's replaces it
        b.terrain.paint(40, 40, 13, 0);
        let step = |a: &mut App, b: &mut App| {
            a.club_frame(None);
            b.club_frame(None);
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        a.club_make("Test Club", false);
        let t0 = now_ms();
        while !a.mp.live.as_ref().is_some_and(|l| matches!(l.phase, Phase::Playing)) {
            assert!(a.club_on(), "the club was made");
            assert!(now_ms() - t0 < 10_000.0, "A plays the club");
            step(&mut a, &mut b);
        }
        let id = a.mp.live.as_ref().unwrap().id.clone();
        // A plays a while, opens the hole and paints, then B joins and paints
        let t1 = now_ms();
        let mut done = [false; 4];
        while now_ms() - t1 < 6000.0 {
            let dt = now_ms() - t1;
            if dt > 300.0 && !done[0] {
                done[0] = true;
                a.act(Action::OpenHole);
                a.act(paint(13, &[(20, 20), (21, 20)]));
            }
            if dt > 1500.0 && !done[1] {
                done[1] = true;
                b.club_join(&id, "Test Club");
            }
            if dt > 3500.0 && !done[2] && b.mp.live.as_ref().is_some_and(|l| matches!(l.phase, Phase::Playing)) {
                done[2] = true;
                b.act(paint(4, &[(25, 25), (26, 25)]));
                a.act(Action::Speed(4));
            }
            step(&mut a, &mut b);
        }
        assert!(done[2], "B joined and played");
        // A pauses: both play up to the stopped clock
        a.club_pause_toggle();
        let t2 = now_ms();
        loop {
            step(&mut a, &mut b);
            let stopped = |x: &App| x.mp.session.as_ref().and_then(|s| s.clock).is_some_and(|c| c.rate == 0 && x.game_tick == c.base_tick);
            if stopped(&a) && stopped(&b) {
                break;
            }
            assert!(now_ms() - t2 < 5000.0, "both stop");
        }
        assert_eq!(a.game_tick, b.game_tick);
        assert!(a.game_tick > 50, "the club ran ({} ticks)", a.game_tick);
        assert_eq!(a.terrain.ty, b.terrain.ty, "the same course");
        assert_eq!(b.terrain.ty[b.terrain.tile_index(40, 40)], 2, "B's own game was replaced by the club's");
        assert_eq!(a.terrain.ty[a.terrain.tile_index(25, 25)], 4, "B's paint reached A");
        assert_eq!(a.fingerprint().total(), b.fingerprint().total(), "the same game on both");
        assert_eq!(a.mp.live.as_ref().unwrap().players.len(), 2);
        assert_eq!(b.mp.live.as_ref().unwrap().reloads, 0, "B never fell out of step");
        a.club_leave("");
        b.club_leave("");
    }
}
