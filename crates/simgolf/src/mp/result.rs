//! A finished challenge's result, sent to the leaderboard Worker (multiplayer/worker: `GET /result?d=`, which the game's
//! HTTP client can send everywhere, net.rs) only when the player presses Send result.

use super::challenge::Challenge;
use serde::Serialize;
use sg_core::scenario::Facts;

/// The multiplayer Worker (the board and the shared clubs, multiplayer/worker): where `wrangler deploy` puts it on the
/// project's Cloudflare account. A build may name another with OPENSGOLF_BOARD_URL set when it is compiled.
pub const BOARD_URL: &str = match option_env!("OPENSGOLF_BOARD_URL") {
    Some(u) => u,
    None => "https://opensgolf-board.openmcm2-laps.workers.dev",
};

/// What the Worker reads (multiplayer/worker/src/index.js `readResult`).
#[derive(Serialize)]
struct Body<'a> {
    protocol: u32,
    challenge: &'a str,
    player: &'a str,
    nick: &'a str,
    group: &'a str,
    platform: &'a str,
    version: &'a str,
    cheats: bool,
    facts: FactsBody,
}

#[derive(Serialize)]
struct FactsBody {
    year: i32,
    cash: i64,
    holes: i32,
    fun: i32,
    skill: i32,
    members: i32,
    rank: i32,
}

/// Why a result can't be sent.
#[derive(Debug, PartialEq, Eq)]
pub enum NotSent {
    /// The game used a cheat (Club::cheated).
    Cheated,
    /// The challenge's game year is not over yet.
    YearNotOver,
}

/// Base64url without padding.
pub fn base64url(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        let k = c.len() + 1;
        for i in 0..k {
            s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    s
}

/// Who is sending: kept in settings.json once made (the id is random and never linked to anything else).
pub struct Sender<'a> {
    pub player: &'a str,
    pub nick: &'a str,
    pub group: &'a str,
    pub platform: &'a str,
    pub version: &'a str,
}

/// The address that sends a result, or why it can't be sent.
pub fn result_url(base: &str, c: &Challenge, who: &Sender, cheated: bool, f: &Facts) -> Result<String, NotSent> {
    if cheated {
        return Err(NotSent::Cheated);
    }
    if f.year <= c.end_year {
        return Err(NotSent::YearNotOver);
    }
    let body = Body {
        protocol: super::version::PROTOCOL,
        challenge: &c.id,
        player: who.player,
        nick: who.nick,
        group: who.group,
        platform: who.platform,
        version: who.version,
        cheats: false,
        facts: FactsBody { year: f.year, cash: f.cash, holes: f.holes, fun: f.fun, skill: f.skill, members: f.members, rank: f.rank },
    };
    let json = serde_json::to_vec(&body).unwrap_or_default();
    Ok(format!("{base}/result?d={}", base64url(&json)))
}

/// A new random player id: 16 hex digits.
pub fn new_player_id(entropy: u128) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hasher::write_u128(&mut h, entropy);
    format!("{:016x}", std::hash::Hasher::finish(&h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mp::challenge::challenge;

    fn facts(year: i32) -> Facts {
        Facts { year, cash: 50_000, holes: 9, fun: 120, skill: 1250, members: 40, rank: 1 }
    }

    #[test]
    fn base64url_matches_the_standard() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn a_result_only_after_the_year_and_never_with_cheats() {
        let c = challenge("2026-W47");
        let who = Sender { player: "0123456789abcdef", nick: "Michael", group: "", platform: "linux", version: "0.9.0" };
        assert_eq!(result_url("b", &c, &who, true, &facts(2002)), Err(NotSent::Cheated));
        assert_eq!(result_url("b", &c, &who, false, &facts(2001)), Err(NotSent::YearNotOver));
        let url = result_url("https://board.test", &c, &who, false, &facts(2002)).unwrap();
        assert!(url.starts_with("https://board.test/result?d=eyJwcm90b2NvbCI6MSwiY2hhbGxlbmdlIjoiMjAyNi1XNDci"), "{url}");
    }

    #[test]
    fn player_ids_are_16_hex_digits() {
        let a = new_player_id(1);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, new_player_id(2));
    }
}
