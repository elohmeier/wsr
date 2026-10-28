//! The turn loop: `SimScheduledEvents` (0x4a395a), `TickerSimSteps` (0x41ef3f), `EndTurn` (0x424fd6) with
//! their helpers, and the scenario ("adventure") story events `MaybeStartScenario` (0x490982),
//! `AdvanceScenarioStory` (0x48fe45), `RunScenarioEvent` (0x48ec04) and `RunScenarioEvent_689361`
//! (0x689361).
//!
//! # How the original drives the game clock
//!
//! There is no separate game clock: the game advances one *tick* at a time, and a tick is one step of
//! the per-entity simulation. A quarter is 1,600 ticks: `SimCoCount` runs 1..1600 (players 1..NumPlayers,
//! then the entities 11..1600) and wraps to 1, which closes the quarter (see
//! [`crate::logic::sim_entity::sim_process_next_entity`]). The calendar date shown is derived from
//! `SimCoCount` (`UpdateDateDisplay`: `CalcCurMonthOfYear(SimCoCount)`).
//!
//! One tick ([`ticker_sim_steps`], inner loop) is: [`sim_scheduled_events`] (the events and AI routines
//! scheduled at fixed or periodic values of `SimCoCount`, decided *before* the entity is processed),
//! `SimProcessNextEntity` (increments `SimCoCount` and simulates that entity), [`show_pending_popup`],
//! an earnings report pop-up for companies the player follows, `CommodityDemandEffects`, `RandSeed + 1`
//! and, when `EconCycleCounter > 22`, an economy update.
//!
//! `TickerSimSteps(stepCount)` runs a batch of ticks (`stepCount`, or 10..20 at random, x1.5 / x2.25 for
//! fast ticker speeds; just one while `ClockState` is "RUN" or "STOP"). Unless `ActionCode = "SUPP"`
//! (suppressed ticker), every tick also scrolls the ticker text by one character (the scrolling text is
//! the pacing: a tick is skipped entirely when `TickerText` is empty). After the batch it posts a pending
//! `NewsMsg`, refreshes the market display (`UpdateMarketDisplay`, every 40th batch while the clock runs)
//! and, once the player has used up the turn (`TurnActionCount >= 5`, counted up at tick 775 of every
//! quarter and by player actions), ends the turn with [`end_turn`].
//!
//! [`end_turn`] passes the turn to the next player in `PlayerTurnOrder` (running whole computer-player
//! turns, `AiComputerPlayerTurn`, in between), detects the end of the game (`GameEndYear = CurYear`, all
//! players out) and restores the next human player's selections.
//!
//! # Host entry points
//!
//! The original runs the ticker from `RunTickerLoop` (0x41d9c2, the "START TICKER" button): while
//! `ClockState = "RUN"` and the game is not over, it calls `TickerSimSteps(0)` whenever
//! `TIMER > TickerPauseUntil` (`PauseTicker` sets a pause, e.g. while a message is shown). Player actions
//! call `TickerSimSteps(0)` after each transaction, and the "END TURN" button (`CbEndTurn`, 0x411de0,
//! [`cb_end_turn`]) ends the turn and runs 20 suppressed ticks. A host therefore:
//!
//! * advances time by calling [`ticker_sim_steps`]`(g, ui, rng, 0, &now)` repeatedly (with
//!   `ClockState = "RUN"` for the running ticker, one tick per call; or `""` for a batch of 10..20 ticks
//!   as after a player action), passing the wall-clock time `now` (`TIMER` / `TIME$` / `DATE$`);
//! * calls [`cb_end_turn`] for the END TURN button.
//!
//! Window/menu effects (control texts, enabling, `SLEEP`, the ticker label) are skipped; only their
//! effects on the game state are ported.
//!
//! The original's `TIME$`/`TIMER`/`DATE$` are the `now: &LocalTime` parameter
//! ([`crate::logic::newgame::LocalTime`]): `VAL(RIGHT$(TIME$, 2))` is `now.second`, `TIMER` is
//! `timer_ms / 1000`, `DATE$` is `"mm-dd-yyyy"`.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init,
    clippy::neg_multiply,
    clippy::manual_clamp
)]


mod end_turn;
mod scenario;
mod scenario_cases_a;
mod scenario_cases_b;
mod scenario_cases_c;
mod scenario_cases_d;
mod scenario_event;
mod scheduled;
mod ticker;

pub use end_turn::{
    cb_end_turn, enable_main_controls, end_turn, end_turn_restore_active_entity, end_turn_skipping, end_turn_stub,
    update_settings_menu_text,
};
pub use scenario::{
    advance_scenario_story, maybe_start_scenario, run_scenario_event, set_scenwin_file, run_scenario_event_skipping,
};
pub use scenario_event::run_scenario_event_689361;
pub use scheduled::{sim_scheduled_events, sim_scheduled_events_skipping};
pub use ticker::{
    pause_ticker, refresh_news_listbox, show_pending_popup, ticker_sim_steps, ticker_sim_steps_skipping,
    ticker_watch_list_quotes, update_date_display,
};

/// The clock and the routines to skip (tests) of one call into the turn loop.
pub(crate) struct Env<'a> {
    /// The local time (`TIMER`, `TIME$`, `DATE$`).
    pub now: &'a LocalTime,
    /// Game routines (addresses) skipped like the test harness's `--skip` (differential tests).
    pub skip: &'a [u32],
}

impl Env<'_> {
    /// True if the routine at `addr` is to be skipped.
    pub fn skips(&self, addr: u32) -> bool {
        self.skip.contains(&addr)
    }
}

use crate::logic::newgame::LocalTime;

/// Common imports of the submodules.
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    #[allow(unused_imports)]
    pub(crate) use crate::logic::newgame::LocalTime;
    #[allow(unused_imports)]
    pub(crate) use crate::logic::news::post_news;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::format;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{str_num, val};
    #[allow(unused_imports)]
    pub(crate) use crate::pb::pbstr::PbStr;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::rng::Rng;
    #[allow(unused_imports)]
    pub(crate) use crate::pb::string::{
        instr, instr_any, lcase, len, ltrim, mcase, mid, parse_any, replace, right, rtrim, ucase, CaseMode,
    };
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{ceil, frac, int, to_integer, to_long, to_quad};
    #[allow(unused_imports)]
    pub(crate) use crate::state::Globals;
    #[allow(unused_imports)]
    pub(crate) use crate::ui::Ui;

    /// A string literal as [`PbStr`].
    #[allow(dead_code)]
    pub(crate) fn s(x: &str) -> PbStr {
        PbStr::from(x)
    }

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(x: &[u8]) -> PbStr {
        rtrim(ltrim(x).as_bytes())
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value.
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `UCASE$(x)`.
    #[allow(dead_code)]
    pub(crate) fn uc(x: &PbStr) -> PbStr {
        ucase(x.as_bytes(), CaseMode::Ansi)
    }

    /// `MCASE$(x)`.
    #[allow(dead_code)]
    pub(crate) fn mc(x: &PbStr) -> PbStr {
        mcase(x.as_bytes(), CaseMode::Ansi)
    }

    /// `REPLACE$(x, find, with)` in place (`REPLACE find WITH with IN x`).
    #[allow(dead_code)]
    pub(crate) fn repl(x: &mut PbStr, find: &str, with: &str) {
        *x = replace(x.as_bytes(), find.as_bytes(), with.as_bytes());
    }

    /// `REPLACE$` with a string variable as the replacement.
    #[allow(dead_code)]
    pub(crate) fn repl_s(x: &mut PbStr, find: &str, with: &PbStr) {
        *x = replace(x.as_bytes(), find.as_bytes(), with.as_bytes());
    }

    /// `x = "literal"`.
    #[allow(dead_code)]
    pub(crate) fn set(x: &mut PbStr, lit: &str) {
        *x = PbStr::from(lit);
    }

    /// The `TIME$` seconds `VAL(RIGHT$(TIME$, 2))`.
    #[allow(dead_code)]
    pub(crate) fn time_secs(now: &LocalTime) -> i32 {
        now.second
    }

    /// `TIMER`: seconds since midnight.
    #[allow(dead_code)]
    pub(crate) fn timer(now: &LocalTime) -> Ext {
        Ext::from(crate::pb::rng::timer_f64(now.timer_ms()))
    }

    /// `DATE$`: `"mm-dd-yyyy"`.
    #[allow(dead_code)]
    pub(crate) fn date_str(now: &LocalTime) -> PbStr {
        PbStr::from(format!("{:02}-{:02}-{:04}", now.month, now.day, now.year).as_str())
    }
}
