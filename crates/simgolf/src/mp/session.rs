//! A shared game's timing and ordering: every player's game runs the same ticks and applies the same actions at the same
//! ticks, so the games stay the same (lockstep.rs keeps everything else out).
//!
//! * The server ([`Room`]; the Durable Object does the same in JavaScript) keeps the club's clock as a formula: the tick
//!   at a moment is `base_tick + (ms - base_ms) * rate / TICK_MS`, with rate 0 (paused: nobody on), 1, 2 or 4. It stores
//!   the formula only when it changes, so it needs no timer.
//! * A player's action goes to the server, which stamps it with the tick it is played at: the clock's tick [`LEAD_MS`]
//!   from now, and a sequence number, and sends it to everyone (the player too: nobody applies their own action before
//!   the server has ordered it).
//! * A game may run up to the clock's tick now ([`Session::may_run_to`]). An action is stamped [`LEAD_MS`] ahead of the
//!   clock, so it arrives in time unless the network took longer than that; one that arrives for a tick
//!   already played is caught ([`Session::late`]) and the game is loaded again from the server's save.
//! * While the clock is paused (nobody on, or a player paused it) actions wait for it to go on again. (Building while
//!   paused, as one can alone, needs actions played between ticks: not yet.)

use super::version::Compat;
use serde::{Deserialize, Serialize};
use sg_core::action::Action;
use std::collections::BTreeMap;

/// The game tick, in ms (sg_core::flight::TICK_MS).
pub const TICK_MS: f64 = sg_core::flight::TICK_MS as f64;
/// The time between the server stamping an action and the moment its tick comes, in ms: how late a message may reach a
/// game and still be in time (at any speed: at 4x it is 4 times as many ticks).
pub const LEAD_MS: f64 = 700.0;

/// The club's clock.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clock {
    pub base_tick: u32,
    /// The server's time of `base_tick`, in ms.
    pub base_ms: f64,
    /// 0 paused, else the game speed (1, 2, 4).
    pub rate: u8,
}

impl Clock {
    pub fn paused_at(tick: u32, ms: f64) -> Clock {
        Clock { base_tick: tick, base_ms: ms, rate: 0 }
    }

    /// The tick at server time `ms`.
    pub fn tick_at(&self, ms: f64) -> u32 {
        if self.rate == 0 || ms <= self.base_ms {
            return self.base_tick;
        }
        self.base_tick.saturating_add(((ms - self.base_ms) * self.rate as f64 / TICK_MS).floor() as u32)
    }

    /// The clock from `ms` on at `rate`, going on from the tick it has reached.
    pub fn with_rate(&self, ms: f64, rate: u8) -> Clock {
        Clock { base_tick: self.tick_at(ms), base_ms: ms, rate }
    }
}

/// An action as the server ordered it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stamped {
    pub tick: u32,
    pub seq: u64,
    pub player: u32,
    pub action: Action,
}

/// What a game sends the server (as JSON over the club's WebSocket; multiplayer/worker/src/club.js).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToServer {
    /// The first message: who this is and whether their game can play this club ([`Compat`]); `tick` is the tick the
    /// game has played up to, which starts a new club's clock.
    Hello {
        protocol: u32,
        player: String,
        nick: String,
        compat: Compat,
        tick: u32,
    },
    Act(Action),
    /// The game speed asked for (0 pause, 1, 2, 4).
    Speed(u8),
    /// The game saved when it had played up to `tick` (before that tick's actions), base64: what a joiner starts from.
    Save {
        tick: u32,
        data: String,
        /// The shared game's state the save file leaves out (club.rs Extra), as JSON.
        #[serde(default)]
        extra: String,
    },
    /// A hash of the game when it had played up to `tick`: a game whose hash differs from the first one sent is
    /// told to load again ([`ToGame::Reload`]).
    Check {
        tick: u32,
        hash: String,
    },
    /// This computer's time, in ms: the server answers with its own ([`ToGame::Pong`]).
    Ping(f64),
}

/// What the server sends the games.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToGame {
    /// The answer to Hello: this player's number in the club, the server's time and the clock. A [`ToGame::Save`] and
    /// every action since it follow.
    Welcome {
        me: u32,
        ms: f64,
        clock: Clock,
    },
    /// The club can't be joined; `parts` names what differs (version.rs Mismatch, as "version", "rules", ...).
    Refused {
        reason: String,
        parts: Vec<String>,
    },
    Save {
        tick: u32,
        data: String,
        /// The shared game's state the save file leaves out (club.rs Extra), as JSON.
        #[serde(default)]
        extra: String,
    },
    Act(Stamped),
    Clock(Clock),
    /// Send a save when you have played up to this tick (or now, if past it).
    SaveAsk(u32),
    /// Who is on.
    Players(Vec<Seat>),
    /// Your game is out of step from this tick: load the club again.
    Reload(u32),
    /// A Ping's time and the server's.
    Pong(f64, f64),
}

/// A player on in a club.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seat {
    pub me: u32,
    pub nick: String,
}

/// Whether an action is the player's own business and goes to the server, is kept on their own computer, or can't be
/// used in a shared game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Shared,
    Local,
    Refused,
}

/// The route of an action in a shared game.
pub fn route(a: &Action) -> Route {
    use Action::*;
    match a {
        // what the game around the simulation hands it: fixed in a shared game (lockstep.rs), never sent
        Sync | Pointer(_) | Flags(_) | View(_) | Screen | Check(_) | Rules(_) | TeeArrows(_) | Grown(_) => Route::Local,
        // the brush in hand is the player's own (a paint action carries what it paints with), and so is the undo list's
        // bookkeeping around a click
        PickBrush { .. } | UndoPress | UndoRelease => Route::Local,
        // undo (and redo) puts back one player's history over everyone's, and cheats would spoil everyone's game
        Undo | Redo | Cheat(_) => Route::Refused,
        _ => Route::Shared,
    }
}

/// One player's side of a shared game.
#[derive(Debug, Default)]
pub struct Session {
    pub me: u32,
    pub clock: Option<Clock>,
    /// The server's time less this computer's, in ms.
    pub offset_ms: f64,
    scheduled: BTreeMap<(u32, u64), Stamped>,
    outbox: Vec<ToServer>,
    /// The first tick an action came too late for (the game must be loaded again).
    pub late: Option<u32>,
}

impl Session {
    pub fn new(me: u32) -> Session {
        Session { me, ..Default::default() }
    }

    pub fn send(&mut self, m: ToServer) {
        self.outbox.push(m);
    }

    pub fn take_outbox(&mut self) -> Vec<ToServer> {
        std::mem::take(&mut self.outbox)
    }

    /// A message from the server, with the tick the game has played up to (`played`: the next tick to play).
    pub fn receive(&mut self, m: ToGame, played: u32) {
        match m {
            ToGame::Act(s) => {
                if s.tick < played && self.late.is_none() {
                    self.late = Some(s.tick);
                }
                self.scheduled.insert((s.tick, s.seq), s);
            }
            ToGame::Clock(c) | ToGame::Welcome { clock: c, .. } => self.clock = Some(c),
            _ => {}
        }
    }

    /// The actions to apply at the start of `tick`, in the server's order.
    pub fn due(&mut self, tick: u32) -> Vec<Stamped> {
        let mut out = Vec::new();
        while let Some(e) = self.scheduled.first_entry() {
            if e.key().0 > tick {
                break;
            }
            out.push(e.remove());
        }
        out
    }

    /// The tick the game may play up to (not including) at this computer's time `local_ms`.
    pub fn may_run_to(&self, local_ms: f64) -> u32 {
        self.clock.map(|c| c.tick_at(local_ms + self.offset_ms)).unwrap_or(0)
    }
}

/// The server's side (the Durable Object's logic, also used by the tests and a local server).
#[derive(Debug)]
pub struct Room {
    pub clock: Clock,
    next_seq: u64,
    /// Everything sent, in order (a late joiner is sent the save and what came after it).
    pub log: Vec<ToGame>,
}

impl Room {
    pub fn new(tick: u32, ms: f64) -> Room {
        Room { clock: Clock::paused_at(tick, ms), next_seq: 0, log: Vec::new() }
    }

    fn stamp(&mut self, ms: f64, player: u32, action: Action) -> ToGame {
        self.next_seq += 1;
        ToGame::Act(Stamped { tick: self.clock.tick_at(ms + LEAD_MS).max(self.clock.tick_at(ms) + 1), seq: self.next_seq, player, action })
    }

    /// A message from `player` at server time `ms`: what everyone is sent, in order.
    pub fn handle(&mut self, ms: f64, player: u32, m: ToServer) -> Vec<ToGame> {
        let out = match m {
            ToServer::Act(Action::Speed(s)) => return self.handle(ms, player, ToServer::Speed(s)),
            ToServer::Act(action) => vec![self.stamp(ms, player, action)],
            ToServer::Speed(rate) => {
                let rate = if rate >= 4 {
                    4
                } else if rate >= 2 {
                    2
                } else {
                    rate
                };
                // the new rate starts LEAD_MS from now, as an action would: a game whose clock runs a little ahead of
                // the server's (the offset is only known to about half a round trip) must not have played past the tick
                // a pause stops at. Until then the games wait at that tick.
                self.clock = self.clock.with_rate(ms + LEAD_MS, rate);
                // the clock moves at the new rate from now; the simulation's fast mode (which the speed sets) changes at
                // a stamped tick like any action
                let mut v = vec![ToGame::Clock(self.clock)];
                if rate > 0 {
                    v.push(self.stamp(ms, player, Action::Speed(rate)));
                }
                v
            }
            _ => Vec::new(),
        };
        self.log.extend(out.iter().cloned());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_is_a_formula() {
        let c = Clock::paused_at(100, 1000.0);
        assert_eq!(c.tick_at(99_999.0), 100, "paused");
        let c = c.with_rate(1000.0, 1);
        assert_eq!(c.tick_at(1000.0 + 87.0 * 10.0), 110);
        let c2 = c.with_rate(1000.0 + 87.0 * 10.0 + 40.0, 4);
        assert_eq!(c2.base_tick, 110);
        assert_eq!(c2.tick_at(1000.0 + 87.0 * 10.0 + 40.0 + 87.0), 114);
        assert_eq!(c2.with_rate(5000.0, 0).tick_at(1e9), c2.tick_at(5000.0));
    }

    #[test]
    fn actions_are_stamped_ahead_and_played_in_order() {
        let mut room = Room::new(0, 0.0);
        // (asked LEAD_MS before 0, so the clock runs from 0)
        let speed = room.handle(-LEAD_MS, 1, ToServer::Speed(1));
        assert_eq!(speed.len(), 2, "the clock, and the speed as an action");
        let a = room.handle(87.0 * 3.0, 1, ToServer::Act(Action::Sync)).remove(0);
        let b = room.handle(87.0 * 3.0, 2, ToServer::Act(Action::Screen)).remove(0);
        let mut s = Session::new(2);
        s.receive(room.log[0].clone(), 0);
        s.receive(b.clone(), 2);
        s.receive(a.clone(), 2);
        let at = 3 + (LEAD_MS / TICK_MS) as u32;
        assert!(s.due(at - 1).is_empty());
        let due = s.due(at);
        assert_eq!(due.iter().map(|d| d.player).collect::<Vec<_>>(), vec![1, 2], "the server's order");
        assert_eq!(s.late, None);
        s.receive(room.handle(0.0, 1, ToServer::Act(Action::Sync)).remove(0), 100);
        assert_eq!(s.late, Some((LEAD_MS / TICK_MS) as u32), "an action for a tick already played");
    }

    /// The inputs of the vectors the server's JavaScript room is tested against (multiplayer/worker/test/room_vectors.json;
    /// SG_WRITE_VECTORS=1 writes the file again).
    fn vector_inputs() -> Vec<(f64, u32, ToServer)> {
        use sg_core::action::Action as A;
        vec![
            (1000.0, 1, ToServer::Act(A::Sync)),
            (1500.0, 1, ToServer::Speed(1)),
            (1500.0, 2, ToServer::Act(A::Screen)),
            (1587.0, 1, ToServer::Act(A::Undo)),
            (2222.5, 2, ToServer::Act(A::Speed(4))),
            (2223.0, 1, ToServer::Act(A::Sync)),
            (2300.0, 2, ToServer::Speed(3)),
            (9000.0, 1, ToServer::Speed(0)),
            (9100.0, 3, ToServer::Act(A::Sync)),
            (20_000.0, 3, ToServer::Speed(2)),
            (20_000.0, 3, ToServer::Ping(5.0)),
            (123_456.789, 1, ToServer::Act(A::Sync)),
        ]
    }

    #[test]
    fn the_server_room_matches_the_vectors() {
        let mut room = Room::new(500, 0.0);
        let steps: Vec<serde_json::Value> = vector_inputs()
            .into_iter()
            .map(|(ms, player, m)| {
                let input = serde_json::to_value(&m).unwrap();
                let out = room.handle(ms, player, m);
                serde_json::json!({ "ms": ms, "player": player, "in": input, "out": out })
            })
            .collect();
        let v = serde_json::json!({ "start": { "tick": 500, "ms": 0.0 }, "steps": steps });
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../multiplayer/worker/test/room_vectors.json");
        let text = serde_json::to_string_pretty(&v).unwrap() + "\n";
        if std::env::var_os("SG_WRITE_VECTORS").is_some() {
            std::fs::write(path, &text).unwrap();
        }
        let want: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(v, want, "the vectors changed: SG_WRITE_VECTORS=1 writes them again, then test the Worker");
    }

    #[test]
    fn messages_read_back() {
        let c = Compat::new(true, false, 74, 0, vec![]);
        let m = ToServer::Hello { protocol: 1, player: "ab".into(), nick: "x".into(), compat: c, tick: 3 };
        let t = serde_json::to_string(&m).unwrap();
        assert!(t.starts_with("{\"Hello\":{"), "{t}");
        assert_eq!(serde_json::from_str::<ToServer>(&t).unwrap(), m);
        let g: ToGame = serde_json::from_str(r#"{"Pong":[1.5,2.0]}"#).unwrap();
        assert_eq!(g, ToGame::Pong(1.5, 2.0));
        let g: ToGame = serde_json::from_str(r#"{"Act":{"tick":4,"seq":1,"player":2,"action":"sync"}}"#).unwrap();
        assert!(matches!(g, ToGame::Act(Stamped { tick: 4, action: Action::Sync, .. })));
    }

    #[test]
    fn routes() {
        assert_eq!(route(&Action::Sync), Route::Local);
        assert_eq!(route(&Action::Undo), Route::Refused);
        assert_eq!(route(&Action::Redo), Route::Refused);
        // the 0.8.7 extras that change the course or the pro's play are everyone's
        assert_eq!(route(&Action::Practice { hole: 1 }), Route::Shared);
        assert_eq!(route(&Action::Mulligan), Route::Shared);
        assert_eq!(route(&Action::BoundClear { hole: 1 }), Route::Shared);
        assert_eq!(route(&Action::Speed(2)), Route::Shared);
    }
}
