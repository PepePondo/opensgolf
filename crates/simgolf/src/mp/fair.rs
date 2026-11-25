//! Fair play in the weekly challenge and the neighbours leagues: every player's game runs by the same rules. Their games
//! start with only the extras that change how the game looks (sg_core::extras `looks_only`), the Extras box can't turn on
//! one that changes how it plays while such a game is played, and a game that has one on anyway (an older save) sends no
//! result. Cheats (`Club::cheated`) and content packs that change the game's data or add objects or scripts
//! (mods_ui.rs `mods_block_multiplayer`) keep results off the board in the same way.

use crate::app::App;
use crate::gfx::Gfx;
use sg_core::extras::Extras;
use sg_core::lang::tr;

/// Whether the game `scenario` names is one whose result is compared with other players' (the weekly challenge or a
/// neighbours league; a shared club's extras come with its save, so they are everyone's already).
pub fn compared(scenario: Option<&str>) -> bool {
    super::rival::challenge_of(scenario).is_some() || super::league::league_of(scenario).is_some()
}

impl App {
    /// Starts a compared game (`start_game`) with only the looks extras, whatever the player picked for new games.
    pub fn start_fair_game(&mut self, g: &mut Gfx, prop: usize) {
        let picked = self.extras_next;
        self.extras_next = picked.looks_only();
        self.start_game(g, prop, false);
        self.extras_next = picked;
    }

    /// The Extras box's choice for the game being played: in a compared game only its looks change (saying so when the
    /// player ticked one that changes the rules).
    pub fn fair_extras(&mut self, e: Extras) -> Extras {
        if !compared(self.scen.goals.scenario.as_deref()) {
            return e;
        }
        let kept = Extras { rotate: e.rotate, path_style: e.path_style, ..self.club.extras };
        if kept != e {
            super::texts();
            self.show_toast(tr("STR_MP_FAIR_EXTRAS"));
        }
        kept
    }

    /// Whether this game's result can't go on a board or table: cheats, or an extra that changes the rules.
    pub fn unfair_game(&self) -> bool {
        self.club.cheated || self.club.extras.changes_rules()
    }
}
