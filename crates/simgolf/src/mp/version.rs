//! Version checks: what must match before games play together. A shared club records its [`Compat`] when it is made;
//! a game may join only when its own is the same in every part (the plan's "Refuse games that can't stay in step"). A
//! rival-courses result carries [`PROTOCOL`] and the game's version, which the leaderboard Worker checks.

use serde::{Deserialize, Serialize};

/// The version of what goes over the network: the Worker's routes and, later, the shared club's messages. Raised on any
/// change a game of the old version could misread; the Worker refuses others.
pub const PROTOCOL: u32 = 1;

/// Everything that decides how a shared game plays on. Two games stay in step only when all of it is the same.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compat {
    /// [`PROTOCOL`].
    pub protocol: u32,
    /// The game's version, e.g. "0.9.0": the simulation may change between any two releases.
    pub version: String,
    /// The action log's format (sg_core::action::LOG_VERSION).
    pub log: u32,
    /// The rules settings (Fix original bugs, Random trees).
    pub fix_bugs: bool,
    pub random_trees: bool,
    /// The map's edge in tiles.
    pub map: i32,
    /// The gameplay extras that are on (sg_core::extras' bits).
    pub extras: u64,
    /// The enabled content packs that change the rules (data tables, scenarios, scripts), as "id@version", sorted. Art
    /// packs are left out: each player may use their own art.
    pub packs: Vec<String>,
}

/// One part that differs, for the message the player reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mismatch {
    Protocol { ours: u32, theirs: u32 },
    Version { ours: String, theirs: String },
    Log { ours: u32, theirs: u32 },
    Rules,
    Map { ours: i32, theirs: i32 },
    Extras,
    Packs { missing: Vec<String>, extra: Vec<String> },
}

impl Compat {
    /// This game's, from its settings.
    pub fn new(fix_bugs: bool, random_trees: bool, map: i32, extras: u64, mut packs: Vec<String>) -> Compat {
        packs.sort();
        packs.dedup();
        Compat {
            protocol: PROTOCOL,
            version: env!("CARGO_PKG_VERSION").to_string(),
            log: sg_core::action::LOG_VERSION,
            fix_bugs,
            random_trees,
            map,
            extras,
            packs,
        }
    }

    /// Every part of `club` (the shared club's record) this game does not match; empty when it may join.
    pub fn check(&self, club: &Compat) -> Vec<Mismatch> {
        let mut v = Vec::new();
        if self.protocol != club.protocol {
            v.push(Mismatch::Protocol { ours: self.protocol, theirs: club.protocol });
        }
        if self.version != club.version {
            v.push(Mismatch::Version { ours: self.version.clone(), theirs: club.version.clone() });
        }
        if self.log != club.log {
            v.push(Mismatch::Log { ours: self.log, theirs: club.log });
        }
        if (self.fix_bugs, self.random_trees) != (club.fix_bugs, club.random_trees) {
            v.push(Mismatch::Rules);
        }
        if self.map != club.map {
            v.push(Mismatch::Map { ours: self.map, theirs: club.map });
        }
        if self.extras != club.extras {
            v.push(Mismatch::Extras);
        }
        if self.packs != club.packs {
            let missing = club.packs.iter().filter(|p| !self.packs.contains(p)).cloned().collect();
            let extra = self.packs.iter().filter(|p| !club.packs.contains(p)).cloned().collect();
            v.push(Mismatch::Packs { missing, extra });
        }
        v
    }

    /// A short hash of the whole record, for the club list (games that can't join are shown greyed out without
    /// fetching every club's record).
    pub fn key(&self) -> u64 {
        let text = serde_json::to_string(self).unwrap_or_default();
        text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Compat {
        Compat::new(true, false, 74, 0b101, vec!["b@1.0.0".into(), "a@2.1.0".into(), "a@2.1.0".into()])
    }

    #[test]
    fn the_same_game_matches() {
        assert!(base().check(&base()).is_empty());
        assert_eq!(base().packs, vec!["a@2.1.0".to_string(), "b@1.0.0".to_string()]);
        assert_eq!(base().key(), base().key());
    }

    #[test]
    fn every_difference_is_named() {
        let mut club = base();
        club.version = "0.0.1".into();
        club.random_trees = true;
        club.map = 50;
        club.packs = vec!["a@2.1.0".into(), "c@1.0.0".into()];
        let m = base().check(&club);
        assert!(m.contains(&Mismatch::Version { ours: env!("CARGO_PKG_VERSION").into(), theirs: "0.0.1".into() }));
        assert!(m.contains(&Mismatch::Rules));
        assert!(m.contains(&Mismatch::Map { ours: 74, theirs: 50 }));
        assert!(m.contains(&Mismatch::Packs { missing: vec!["c@1.0.0".into()], extra: vec!["b@1.0.0".into()] }));
        assert_eq!(m.len(), 4);
        assert_ne!(base().key(), club.key());
    }
}
