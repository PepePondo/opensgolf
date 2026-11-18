//! Multiplayer (private until launch; the plan's "Rival courses" and "Shared course"). Built only with the `multiplayer`
//! feature, and even then off until the player turns the Multiplayer extra on.
//!
//! * [`challenge`]: the weekly rival-courses challenge, the same for everyone each ISO week.
//! * [`result`]: a finished challenge's result, as the leaderboard Worker (multiplayer/worker) reads it.
//! * [`version`]: what two games, or a game and the server, must agree on before they play together.
// the game does not call these yet (the Multiplayer screen comes next)
#![allow(dead_code)]

pub mod challenge;
pub mod result;
pub mod version;
