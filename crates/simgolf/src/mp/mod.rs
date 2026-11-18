//! Multiplayer (private until launch; the plan's "Rival courses" and "Shared course"). Built only with the `multiplayer`
//! feature, and even then off until the player turns the Multiplayer extra on.
//!
//! * [`challenge`]: the weekly rival-courses challenge, the same for everyone each ISO week.
//! * [`result`]: a finished challenge's result, as the leaderboard Worker (multiplayer/worker) reads it.
//! * [`rival`]: the weekly challenge in the game (a Start New Game row) and its result sent when 2001 ends.
//! * [`settings`]: the player's own settings.json key (on, id, nickname, group).
//! * [`version`]: what two games, or a game and the server, must agree on before they play together.
// parts the game does not call yet (the shared course comes next)
#![allow(dead_code)]

pub mod challenge;
pub mod result;
pub mod rival;
pub mod settings;
pub mod version;
