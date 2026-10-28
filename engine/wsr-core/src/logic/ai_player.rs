//! The computer players: `AiComputerPlayerTurn` (0x5c3172) and its strategy steps (`AiPlayer_*` GOSUB
//! blocks, 0x5c3730..0x5d18dd); management firing / CEO election (`FireManagement`, `ElectSelfCeo`,
//! `CheckCeoOptionForfeiture`) and `AiFindAffordableBuyer`.
//!
//! The GOSUB blocks run on the caller's stack frame: they are ported as fns over [`Frame`] (the routine's
//! locals, zeroed at entry) in the submodules [`buyer`] (shared helpers), [`position`], [`invest`],
//! [`actions`], [`mgmt`], [`merge`], [`portfolio`].
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.05)`; SINGLE locals
//! are rounded with `as f32` on every store (`fstp dword`).

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
    clippy::needless_return
)]
use crate::pb::real::{Ext, cast};


pub mod actions;
pub mod buyer;
pub mod invest;
pub mod merge;
pub mod mgmt;
pub mod portfolio;
pub mod position;

pub use buyer::{add_recent_offering, ai_find_affordable_buyer};
pub use mgmt::{check_ceo_option_forfeiture, elect_self_ceo, fire_management};

/// Common imports of the submodules.
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    #[allow(unused_imports)]
    pub(crate) use super::Frame;
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
        instr, instr_any, len, ltrim, mid, parse_any, replace, right, rtrim,
    };
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{int, to_integer, to_long, to_quad};
    #[allow(unused_imports)]
    pub(crate) use crate::state::Globals;
    #[allow(unused_imports)]
    pub(crate) use crate::ui::Ui;

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
    }

    /// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
    #[allow(dead_code)]
    pub(crate) fn fmt_trim(x: Ext, mask: &[u8]) -> PbStr {
        trim(&format(x, mask))
    }

    /// `FORMAT$(x, mask)`.
    #[allow(dead_code)]
    pub(crate) fn fmt(x: Ext, mask: &[u8]) -> PbStr {
        PbStr(format(x, mask))
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value.
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(s: &[u8]) -> PbStr {
        rtrim(ltrim(s).as_bytes())
    }
}

#[allow(unused_imports)]
use prelude::*;

/// The locals of `AiComputerPlayerTurn` (stack frame offsets from EBP; zeroed at entry), shared by all
/// GOSUB blocks of one call.
#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct Frame {
    /// Routines (game addresses) the caller asked to skip (tests: steps / callees not ported yet).
    pub skip: Vec<u32>,
    /// Seconds of the clock (`TIME$`), passed to the callees that read it (CalcStockPrice,
    /// StartNewCorpOrExpand, AiFileAntitrustSuits, BuyStock, MergeCompanies, ...).
    pub time_secs: i32,
    /// The local time (`DATE$` / `TIME$` of LboTenderOffer).
    pub now: crate::logic::newgame::LocalTime,

    // ---- LONG
    pub tmp_val: i32,          // -0x2c
    pub bought_flag: i32,      // -0x88
    pub lender_ctrl_id: i32,   // -0xa0
    pub buyer_corp: i32,       // -0xa8
    pub trade_count: i32,      // -0xb8
    pub defense_cost: i32,     // -0xbc
    pub best_corp_id: i32,     // -0xcc
    pub suit_corp_id: i32,     // -0xd4
    pub law_firm_adj: i32,     // -0xdc
    pub fire_mgmt_corp: i32,   // -0xec
    pub corps_controlled: i32, // -0xf0
    pub player_id: i32,        // -0xf4
    pub ent_id: i32,           // -0xf8
    pub slot: i32,             // -0xfc
    pub other_corp_id: i32,    // -0x100
    pub buy_slot: i32,         // -0x104
    pub hold_slot: i32,        // -0x108
    pub new_corp_id: i32,      // -0x10c
    pub found: i32,            // -0x114
    pub own_ceo_corp: i32,     // -0x118
    pub allow_toxic: i32,      // -0x11c
    pub acquirer_id: i32,      // -0x120
    pub target_pct: i32,       // -0x124
    pub cur_pct: i32,          // -0x128
    /// -0x12c (unnamed).
    pub x_12c: i32,
    pub suit_slot: i32,    // -0x130
    pub suit_target: i32,  // -0x134
    pub pick_id: i32,      // -0x138
    pub other_id: i32,     // -0x13c
    pub defendant_id: i32, // -0x140
    pub bond_slot: i32,    // -0x170
    pub target_id: i32,    // -0x174
    /// -0x190 (unnamed).
    pub x_190: i32,

    // ---- INTEGER
    pub largest_hold_id: i16, // -0xa4
    pub startup_ind: i16,     // -0xd0
    pub industry: i16,        // -0xd8
    pub buyout_mode: i16,     // -0x110

    // ---- QUAD
    pub rnd_val: i64,     // -0x5c
    pub debt_paydown: i64, // -0x9c
    pub cash_amt: i64,    // -0x148
    pub bond_amt: i64,    // -0x150
    pub result_amt: i64,  // -0x158

    // ---- SINGLE
    pub cur_outlook: f32, // -0x15c
    pub fut_outlook: f32, // -0x160

    // ---- DOUBLE
    pub leverage_rate: Ext, // -0x1b0

    // ---- EXT
    pub call_amt: Ext, // -0x94
    pub pay_amt: Ext,  // -0xb4
    /// -0xc8 (unnamed).
    pub x_c8: Ext,
    pub max_liquid: Ext,      // -0xe8
    pub shortfall: Ext,       // -0x16c
    pub net_worth_ref: Ext,   // -0x180
    pub year_income: Ext,     // -0x18c
    pub own_net_worth: Ext,   // -0x19c
    pub rival_net_worth: Ext, // -0x1a8
    pub amount: Ext,          // -0x1bc

    // ---- STRING
    pub pending_msg: PbStr, // -0x1c8
    pub field_text: PbStr,  // -0x1cc
    /// -0x1d0: the progress label the ON ERROR handler shows with ShowRuntimeError.
    pub trace_tag: PbStr,
}

impl Frame {
    /// True if the routine at `addr` is to be skipped (tests).
    pub fn skips(&self, addr: u32) -> bool {
        self.skip.contains(&addr)
    }
}

use crate::logic::finance::calc_entity_financials;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::turn::enable_main_controls;

/// Port of AiComputerPlayerTurn (0x5c3172).
/// The turn of a computer player (`CurPlayer`, called once per quarter tick by the turn loop): sets the
/// player's auto settings, picks its main holding (`AiTargetCorp` = first stock holding) and runs the
/// strategy steps (squeeze-outs, holding-company purchase, bond trading, stock purchases, start-ups,
/// bond issues, lawsuits, rumors, management changes, mergers, offerings, credit-rating repair, buyouts,
/// antitrust divestitures, end-game debt repayment), each limited by `TurnActionCount`.
///
/// The ON ERROR handler (0x5d1b1e) only shows ShowRuntimeError with `traceTag` and leaves the routine
/// (not ported: an out-of-range subscript continues on the dummy element in the port).
///
/// `now` = the local time, for the callees that read `TIME$` / `DATE$`.
pub fn ai_computer_player_turn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime) {
    ai_computer_player_turn_skipping(g, ui, rng, now, &[]);
}

/// [`ai_computer_player_turn`] with routines (game addresses of callees or GOSUB blocks) skipped like the
/// test harness does (`--skip`): for differential tests of single steps.
#[doc(hidden)]
pub fn ai_computer_player_turn_skipping(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    now: &crate::logic::newgame::LocalTime,
    skip: &[u32],
) {
    let mut f = Frame { skip: skip.to_vec(), time_secs: now.time_secs(), now: *now, ..Frame::default() };
    main(g, ui, rng, &mut f);
}

/// Port of AiComputerPlayerTurn_Exit (0x5d1ba8): the routine's exit label (frame release only).
fn exit() {}

fn main(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if g.entity_status[g.cur_player] != 1 {
        return exit();
    }
    if g.is_computer_player[g.cur_player] == 0 {
        return exit();
    }
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 501 {
        return exit();
    }
    g.turn_action_count = 0;
    g.action_code = PbStr::from("AUTO");
    let p = g.cur_player;
    g.player_exercise_options[p] = g.cur_player;
    g.cur_exercise_options = g.player_exercise_options[p];
    g.cur_sweep_cash = 0;
    g.sweep_cash_to_loan[p] = 0;
    if g.start_year + 2 < g.cur_year {
        g.cur_sweep_cash = 1;
        g.sweep_cash_to_loan[p] = 1;
    }
    enable_main_controls(g);
    // (UI) CONTROL DISABLE hDlgMain, 170 / 168
    f.player_id = g.cur_player;
    pack_holding_arrays(g, ui);
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.ai_target_corp = cast::<i32, _>(g.hold_stock_id[(1, f.player_id)]);
    if 10 < g.ai_target_corp {
        let t = g.ai_target_corp;
        if g.market_cap[t] < Ext::from_f64(200.0) && g.game_end_year - 1 <= g.cur_year && g.industry_num[t] < 70 {
            g.hot_stock_corp = cast::<i16, _>(t);
            g.investor_sentiment[t] += 10;
        }
    }
    if g.cur_year != g.auto_buyout_year && g.cur_year - 1 != g.auto_buyout_year {
        g.auto_buyout_year = 0;
    }
    macro_rules! step {
        ($addr:expr, $f:path) => {
            if !f.skips($addr) {
                $f(g, ui, rng, f);
            }
        };
    }
    step!(0x5c3730, position::squeeze_out_minority);
    step!(0x5c3c65, position::assess_position);
    step!(0x5c3fc7, position::freeze_rival_credit);
    if g.ai_target_corp == 0 && g.hold_stock_id[(1, f.player_id)] == 0 {
        step!(0x5c4828, position::acquire_holding_co);
    }
    if 10 < g.ai_target_corp && f.buyout_mode == 1 {
        step!(0x5c4828, position::acquire_holding_co);
    }
    step!(0x5c5a9d, position::buy_corp_bonds);
    step!(0x5c5e5c, position::sell_corp_bonds);
    step!(0x5c603a, position::repay_frozen_loan);
    calc_entity_financials(g, ui, rng, f.player_id);
    step!(0x5c67af, invest::use_excess_cash);
    step!(0x5c7240, invest::pick_antitrust_target);
    step!(0x5c7855, invest::buy_target_stock);
    step!(0x5c8c0b, invest::review_sub_holdings);
    step!(0x5c8e00, invest::advance_funds);
    step!(0x5c9506, invest::enter_growth_industry);
    if g.cur_quarter != g.ai_adv_call_quarter {
        step!(0x5c9b05, invest::call_in_advances);
    }
    calc_entity_financials(g, ui, rng, f.player_id);
    step!(0x5c9f3f, actions::startup_company);
    if g.turn_action_count < 5 && 10 < g.ai_target_corp && f.player_id == g.corp_controller[g.ai_target_corp] {
        step!(0x5caa87, actions::insurer_buy_bonds);
        step!(0x5caa87, actions::insurer_buy_bonds);
    }
    if 0 < g.ai_target_corp && f.player_id == g.corp_controller[g.ai_target_corp] {
        f.buyer_corp = g.ai_target_corp;
        step!(0x5cb017, actions::issue_bonds);
        f.buyer_corp = 0;
        step!(0x5cb017, actions::issue_bonds);
    }
    calc_entity_financials(g, ui, rng, f.player_id);
    if g.turn_action_count < 7 {
        step!(0x5cb6ea, actions::file_lawsuit);
    }
    calc_entity_financials(g, ui, rng, f.player_id);
    if g.turn_action_count < 7 {
        step!(0x5cc20c, actions::spread_rumors);
    }
    if g.turn_action_count < 7 {
        step!(0x5cc888, mgmt::tune_controlled_corps);
    }
    if g.turn_action_count < 7 && 130 < g.ai_target_corp {
        step!(0x5cce66, actions::write_options);
    }
    if g.turn_action_count < 8 {
        step!(0x5cd0cd, mgmt::fire_ceos);
    }
    if g.turn_action_count < 9 {
        step!(0x5cd435, merge::merge_controlled_corps);
    }
    step!(0x5cdf0e, merge::hostile_merger);
    if g.turn_action_count < 9 {
        step!(0x5ce762, merge::issue_stock_offering);
    }
    step!(0x5cebd9, merge::improve_credit_rating);
    if g.turn_action_count < 9 {
        step!(0x5cf5ba, merge::buyout_public_shares);
    }
    if g.turn_action_count < 9 {
        step!(0x5cfad1, merge::merge_tax_loss_sub);
    }
    step!(0x5cfe21, merge::antitrust_scan);
    if g.turn_action_count < 9 {
        step!(0x5d020b, portfolio::buy_more_stock);
    }
    if g.turn_action_count < 9 {
        step!(0x5d0c41, portfolio::liquidate_idle_holding_co);
    }
    if g.turn_action_count < 9 {
        step!(0x5d0eb9, portfolio::consolidate_top_holdings);
    }
    if 10 < g.ai_target_corp {
        step!(0x5d12ad, portfolio::sell_stock_raise_cash);
    }
    if g.turn_action_count < 9 {
        step!(0x5d177e, portfolio::buy_ethereum);
    }
    step!(0x5d18dd, portfolio::end_game_pay_debt);
    g.action_code = PbStr::new();
    exit()
}
