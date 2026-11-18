//! Lockstep: in a shared game every player's computer runs the same simulation from the same actions, so nothing that
//! only one computer sees may reach it: not the screen (the view, the zoom, the pointer, the ticker, where golfers stand
//! on the screen), not the art (its animation frame counts) and not the clock of the drawing. The single-player game
//! hands the simulation all of these (they are the "what the game around the simulation hands it" actions of
//! sg_core::action, logged for replays); in lockstep each one is replaced by a fixed value, the same everywhere:
//!
//! * the tick flags: nothing busy, no advisor, not close, no message, no snapshots ([`FLAGS`]);
//! * the golfers' screen places: every golfer off the screen;
//! * the pointer the pro's employee follows: none;
//! * the tees' arrows: set in the tick, as for the unturned view ([`CAMERA`]), not by the drawing;
//! * the weeds' frame count: the original's ([`WEED_FRAMES`]);
//! * a new plant's growing: [`SPROUT_TICKS`] game ticks, counted in the tick (not by the frames drawn);
//! * the course brought up to date with the land: at the start of every tick, not when a frame or report asks.
//!
//! The hooks outside this folder are one line each behind the `multiplayer` feature (replay.rs: tick_flags, screen_hook,
//! sync_hook, tick_begin; app.rs: the staff's world, update_tee_arrows, update_tree_growth).

use crate::app::App;
use sg_core::action::TickFlags;
use std::cell::Cell;

/// The tick flags of a shared game.
// (the arrows on turned views are drawn wrong for now: the renderer should keep its own picture of them)
pub const FLAGS: TickFlags = TickFlags { busy: false, advisor: false, close: false, message: false, snapshots: false };
/// The tees' arrows of a shared game: as for the view unturned.
pub const CAMERA: i32 = 0;
/// The weeds' frames (the original's crabgrass and theme weeds have 8).
pub const WEED_FRAMES: i32 = 8;
/// Game ticks a newly planted tree, scenic tree or flower bed grows before its tile's mark ends (about the original's 15
/// frames at one frame a tick).
pub const SPROUT_TICKS: u32 = 15;

thread_local! {
    static ON: Cell<bool> = Cell::new(std::env::var_os("SG_LOCKSTEP").is_some_and(|v| v != "0"));
}

/// Whether the game is a shared one (SG_LOCKSTEP=1 for tests until joining a club sets it).
pub fn on() -> bool {
    ON.with(|c| c.get())
}

pub fn set(on: bool) {
    ON.with(|c| c.set(on));
}

/// The staff's world inputs that come from outside the simulation: the pointer's tile and the weeds' frame count.
pub fn staff_inputs(weed_frames: i32, clicked: Option<(i32, i32)>) -> (i32, Option<(i32, i32)>) {
    if on() {
        (WEED_FRAMES, None)
    } else {
        (weed_frames, clicked)
    }
}

/// Test hook SG_MP_STEP=S (seconds): the play a frame of a scripted run may advance, so the drawing runs between ticks as
/// it does in a real game. Unset: all of it at once.
fn test_step() -> Option<f64> {
    static STEP: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    *STEP.get_or_init(|| std::env::var("SG_MP_STEP").ok().and_then(|v| v.parse().ok()).filter(|&s: &f64| s > 0.0))
}

/// How far the play may go this frame.
pub fn frame_target(sim_time: f64, time: f64) -> f64 {
    match test_step() {
        Some(s) => time.min(sim_time + s),
        None => time,
    }
}

/// A scripted still waits while SG_MP_STEP has play left to run.
pub fn test_waiting(app: &App) -> bool {
    test_step().is_some() && app.sim_time + 1e-9 < app.time && app.great_shot.is_none()
}

impl App {
    /// The golfers' screen places in a shared game: all off the screen.
    pub(crate) fn lockstep_screen(&mut self) {
        for g in self.club.g.iter_mut() {
            g.sx = -1;
            g.sy = -1;
        }
    }

    /// The start of a game tick in a shared game (inside the tick, so nothing here is logged): plants grow a tick, those
    /// done end their tiles' mark, and the course is brought up to date with the land.
    pub(crate) fn lockstep_tick(&mut self) {
        if !on() {
            return;
        }
        // (a golfer who arrived since the last drawn frame has the screen place he was made with otherwise)
        self.lockstep_screen();
        let mut done: Vec<(i32, i32)> = Vec::new();
        for (&at, c) in self.sprouts.iter_mut() {
            *c = c.saturating_add(1);
            if *c >= SPROUT_TICKS {
                done.push(at);
            }
        }
        if !done.is_empty() {
            // (the order is the map's, which differs run to run: sprouts_grown doesn't depend on it, but keep it sorted)
            done.sort_unstable();
            for at in &done {
                self.sprouts.remove(at);
            }
            self.sprouts_grown(&done);
        }
        self.sync_course();
        if self.tee_arrows_change(CAMERA) {
            self.tee_arrows(CAMERA);
        }
    }

    /// The drawing of growing plants in a shared game: their frames follow the tick's counters, and nothing the drawing
    /// sees goes back to the simulation (update_tree_growth's own does both).
    pub(crate) fn lockstep_tree_growth(&mut self) {
        let (mut trees, mut objects) = (false, false);
        for pi in 0..self.props.len() {
            let p = &self.props[pi];
            let (Some((a, b, _)), Some(bi), tree) = (p.sprout, p.body, p.tree) else { continue };
            let n = self.sprites[bi].s.frames_per_view.max(1);
            let frame = match self.sprouts.get(&(a, b)) {
                Some(&c) => ((c as i64 * (n - 1) as i64) / SPROUT_TICKS as i64).min((n - 1) as i64) as i32,
                None => {
                    // grown: drawn again as a grown plant
                    trees |= tree;
                    objects |= !tree;
                    n - 1
                }
            };
            self.props[pi].frame = frame;
        }
        if trees {
            self.refresh_trees();
        }
        if objects {
            self.props.retain(|p| !p.object);
            self.add_land_objects();
        }
        for p in self.props.iter_mut().filter(|p| p.object) {
            let (Some((a, b)), Some(bi)) = (p.grow_tile, p.body) else { continue };
            let t = &self.staff_tiles;
            let i = (b * t.w + a) as usize;
            let n = self.sprites[bi].s.frames_per_view;
            let c = t.counter.get(i).copied().unwrap_or(0) as i32;
            let washing = a < t.w && t.flags.get(i).is_some_and(|f| f & sg_core::staff::WORKED != 0);
            p.frame = if washing && c < n { c } else { 0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_unless_asked_and_then_fixed() {
        set(false);
        assert_eq!(staff_inputs(5, Some((1, 2))), (5, Some((1, 2))));
        set(true);
        assert_eq!(staff_inputs(5, Some((1, 2))), (WEED_FRAMES, None));
        set(false);
    }
}
