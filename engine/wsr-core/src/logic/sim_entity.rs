//! The per-entity quarterly simulation: `SimProcessNextEntity` (0x4c6bc1) — one call per timer tick,
//! advancing `SimCoCount` over players 1..NumPlayers and entities 11..1600 — with its GOSUB chain
//! (`SimEntity_*` / `SimPlayer_*`, 0x4c89e0..0x4dfeeb) and the callees it needs.
//!
//! The GOSUB blocks run on the caller's stack frame: they are ports as private fns over [`Frame`] (the
//! routine's locals, zeroed at entry) in the submodules [`economy_tick`], [`operating`], [`accounting`],
//! [`earnings_news`], [`bank_ops`], [`player`]. Callee SUBs/FUNCTIONs that were not ported elsewhere live
//! in [`asset_sales`], [`bonds`], [`corp_ops`], [`dividends`], [`fin_inst`], [`quarter_start`], [`ai_corp`], [`transactions`].
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.05)`; SINGLE globals
//! and locals are rounded with `as f32` on every store (`fstp dword`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init
)]
use crate::pb::real::{Ext, cast};


pub mod acquisitions;
pub mod ai_corp;
pub mod asset_sales;
pub mod bonds;
pub mod corp_ops;
pub mod dividends;
pub mod etf;
pub mod fin_inst;
pub mod liquidation;
pub mod mgmt;
pub mod quarter_start;
pub mod transactions;

mod accounting;
mod bank_ops;
mod earnings_news;
mod economy_tick;
mod operating;
mod player;

/// Common imports of the submodules.
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

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

/// The locals of `SimProcessNextEntity` (stack frame offsets from EBP; zeroed at entry), shared by all
/// GOSUB blocks of one call.
#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct Frame {
    /// Routines (game addresses) the caller asked to skip (tests: callees not ported yet).
    pub skip: Vec<u32>,
    /// Seconds of `TIME$` (the game reads `VAL(RIGHT$(TIME$, n))`).
    pub time_secs: i32,
    /// The local time (`TIME$` / `DATE$` of the interactive callees).
    pub now: crate::logic::newgame::LocalTime,

    // ---- LONG
    pub cmp_tmp: i32,           // -0x2c
    pub tries: i32,             // -0xac
    pub special_div_flag: i32,  // -0xb0
    pub bonds_high: i32,        // -0xb4
    pub other_id: i32,          // -0xcc
    pub cash_flow_score: i32,   // -0xd4
    pub ent_id: i32,            // -0xd8
    pub idx: i32,               // -0xdc
    pub slot: i32,              // -0xe0
    pub holder_id: i32,         // -0xe4
    pub maturity_year: i32,     // -0xe8
    pub ok_flag: i32,           // -0xec
    pub answer: i32,            // -0xf0
    pub write_down_qtr: i32,    // -0xf4
    pub bankrupt_code: i32,     // -0x140
    pub holding_pick: i32,      // -0x144
    pub financing_mode: i32,    // -0x14c
    pub credit_rating: i32,     // -0x150
    pub managing_player: i32,   // -0x160
    pub lbo_done: i32,          // -0x170
    pub ctrl_type: i32,         // -0x204
    pub tax_flags: i32,         // -0x3ac
    pub opt_pos_result: i32,    // -0x3e0
    pub close_mode: i32,        // -0x3f0
    pub base_margin: i32,       // -0x450
    pub etf_add_result: i32,    // -0x45c
    pub margin_call_count: i32, // -0x460
    pub player_count: i32,      // -0x474

    // ---- INTEGER
    pub tmp_short: i16,        // -0xd0
    pub tax_parent_id: i16,    // -0xf8
    pub disc_spend: i16,       // -0xfc
    pub industry: i16,         // -0x100
    pub special_div_mode: i16, // -0x174
    pub market_share: i16,     // -0x178
    pub ins_margin: i16,       // -0x180
    pub industry_orig: i16,    // -0x184
    pub start_growth: i16,     // -0x330
    pub public_pct: i16,       // -0x454

    // ---- QUAD
    pub tmp_long: i64, // -0xb8
    pub amt_long: i64, // -0xc0
    /// -0xc8 (unnamed): rounded dividend amount for the news in PayDividends.
    pub div_news_amt: i64,
    pub bond_op_code3: i64, // -0x23c
    pub bond_op_code2: i64, // -0x244
    pub swap_pnl: i64,      // -0x440

    // ---- SINGLE
    pub need_funding: f32,    // -0x198
    pub div_change: f32,      // -0x334
    pub prime_rate_save: f32, // -0x344
    pub tmp_pct: f32,         // -0x348
    pub coupon_rate: f32,     // -0x34c
    pub ind_outlook: f32,     // -0x350
    pub margin_frac: f32,     // -0x354
    pub net_margin_frac: f32, // -0x358
    pub misc_income: f32,     // -0x360

    // ---- DOUBLE
    pub growth_adj: Ext, // -0x13c
    /// -0x1f4 (unnamed): number of 100-unit asset blocks sold in DistressedAssetSale.
    pub assets_sold: Ext,
    pub ind_outlook2: Ext, // -0x2a0

    // ---- EXT
    pub sel_case: Ext,            // -0x40
    pub cap_gain_share: Ext,      // -0x110
    pub tax_payment: Ext,         // -0x11c
    pub asset_amt: Ext,           // -0x128
    pub amount: Ext,              // -0x134
    pub total_assets_save: Ext,   // -0x15c
    pub growth_invest: Ext,       // -0x16c
    pub cap_change: Ext,          // -0x194
    pub reserve_provision: Ext,   // -0x1a4
    pub bond_int_status: Ext,     // -0x1b0
    pub unused_amt: Ext,          // -0x1bc
    pub extra_pl_adj: Ext,        // -0x1c8
    pub child_assets: Ext,        // -0x1d4
    pub combined_assets: Ext,     // -0x1e0
    pub liquid_cash: Ext,         // -0x1ec
    pub consumer_write_off: Ext,  // -0x200
    pub uw_profit_ytd: Ext,       // -0x210
    pub bank_debt_now: Ext,       // -0x21c
    pub misc_debt_field: Ext,     // -0x228
    pub subprime_write_down: Ext, // -0x234
    pub div_payout: Ext,          // -0x250
    pub bond_ops_total: Ext,      // -0x25c
    pub required_reserve: Ext,    // -0x268
    pub prior_yr_earnings: Ext,   // -0x274
    pub net_earnings: Ext,        // -0x280
    pub covenant_pay: Ext,        // -0x28c
    pub reserve_adj: Ext,         // -0x298
    pub naked_liab: Ext,          // -0x2ac
    /// -0x2bc (unnamed): option strike adjustment passed to AdjustOptionsForCorpAction.
    pub opt_adj: Ext,
    pub tax_due: Ext,           // -0x2c8
    pub income_tax: Ext,        // -0x2d4
    pub consol_tax_cap: Ext,    // -0x2e0
    pub consol_tax1: Ext,       // -0x2ec
    pub tax_credit_used: Ext,   // -0x2f8
    pub liabilities: Ext,       // -0x304
    pub taxable_income: Ext,    // -0x310
    pub cap_gain_dist: Ext,     // -0x31c
    pub earnings_ytd_save: Ext, // -0x328
    pub start_mkt_cap: Ext,     // -0x340
    pub interest_amt: Ext,      // -0x36c
    /// -0x384 (unnamed): the income tax being computed in CalcIncomeTax.
    pub tax_amt: Ext,
    pub pretax_income: Ext,       // -0x390
    pub extraord_tax: Ext,        // -0x39c
    pub consol_tax3: Ext,         // -0x3a8
    pub goodwill_charge: Ext,     // -0x3b8
    pub cust_deposits: Ext,       // -0x3c4
    pub start_div_per_share: Ext, // -0x3d0
    pub wc_released: Ext,         // -0x3dc
    pub capital_taxes: Ext,       // -0x3ec
    pub carbon_tax: Ext,          // -0x3fc
    pub oil_windfall_tax: Ext,    // -0x408
    pub holding_co_tax: Ext,      // -0x414
    pub corp_shares_tax: Ext,     // -0x420
    pub health_care_tax: Ext,     // -0x42c
    pub return_of_capital: Ext,   // -0x438
    pub proj_earnings: Ext,       // -0x44c

    // ---- STRING
    pub msg_part1: PbStr,       // -0x88
    pub msg_part2: PbStr,       // -0x8c
    pub msg_part3: PbStr,       // -0x90
    pub name_and_symbol: PbStr, // -0x94
    pub curr_sym: PbStr,        // -0x98
    pub fmt_money: PbStr,       // -0x9c
    pub fmt_whole: PbStr,       // -0xa0
    pub asset_sale_text: PbStr, // -0xa4
    pub rating_str: PbStr,      // -0xa8
    pub ent_name: PbStr,        // -0x104
    pub sold_list: PbStr,       // -0x32c
    pub margin_text: PbStr,     // -0x35c
    pub bond_int_paid: PbStr,   // -0x458
    pub line_marker: PbStr,     // -0x464
    /// `dimArr(1 TO 90)` (-0x4f0): the fields of the earnings record (BuildEarningsRecord).
    pub dim_arr: Vec<PbStr>,
}

impl Frame {
    /// True if the routine at `addr` is to be skipped (tests).
    pub fn skips(&self, addr: u32) -> bool {
        self.skip.contains(&addr)
    }
}

use crate::logic::control::find_option_positions;
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings};
use crate::logic::finance::calc_entity_financials;
use crate::logic::options::close_option_positions;
use crate::logic::reports_corp::check_cross_ownership;
use crate::logic::shorts::find_short_positions;
use crate::logic::swaps::{process_swap_payments, swap_pnl_accumulator};
use crate::logic::valuation::calc_stock_price;
use crate::pb::string::{CaseMode, ucase};

/// Port of SimProcessNextEntity (0x4c6bc1).
/// One step of the quarterly simulation: advances `SimCoCount` (1..1600, wrapping with the auto-save
/// check) and simulates that player or entity. At `SimCoCount = NumPlayers + 1` the quarter ends
/// (swap settlement and swap P&L news, wealth tax, quarter/year roll-over, insider-trading probe,
/// expiring asset-sale offers, [runtime resource wording] , the libel suit of a rumor victim, analysts'
/// estimate upgrade). Every second tick reprices one industry; players pay living expenses, CEO pay and
/// taxes and are checked for solvency; entities 11..1600 run the economy step (every 290th), bond
/// maturities, operating results, financing, accounting, earnings and dividends, and AI deals.
///
/// `now` = the local time (`VAL(RIGHT$(TIME$, 2))`, the seconds, is what the routine itself reads: the last
/// digit; interactive callees read the full time).
/// The ON ERROR handler (0x4dfe47) only shows ShowRuntimeError and leaves the routine (not ported:
/// an out-of-range subscript aborts the original's tick, the port continues with the dummy element).
pub fn sim_process_next_entity(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &crate::logic::newgame::LocalTime) {
    sim_process_next_entity_skipping(g, ui, rng, now, &[]);
}

/// [`sim_process_next_entity`] with routines (game addresses of callees or GOSUB blocks) skipped like the
/// test harness does (`--skip`): for differential tests of partial ports.
#[doc(hidden)]
pub fn sim_process_next_entity_skipping(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    now: &crate::logic::newgame::LocalTime,
    skip: &[u32],
) {
    let mut f = Frame {
        skip: skip.to_vec(),
        time_secs: now.time_secs(),
        now: *now,
        dim_arr: vec![PbStr::new(); 91],
        ..Frame::default()
    };
    main(g, ui, rng, &mut f);
}

fn main(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.fmt_money = PbStr::from("##############.00");
    f.fmt_whole = PbStr::from("##############");
    g.sim_co_count += 1;
    g.co_cnt_backup = g.sim_co_count;
    if g.sim_co_count == 1601 {
        let mut handled = false;
        if g.cur_auto_save == 1 {
            if !(g.game_end_flag == "Y") && g.auto_save_state == 0 {
                handled = true;
                if g.cur_year < g.game_end_year - 1 || g.cur_quarter != 4 {
                    g.sim_co_count = 1600;
                    // (UI) CONTROL SET TEXT hDlgMain, 123, " [runtime resource wording] "
                    g.auto_save_state = 1;
                    if !f.skips(0x40c01b) {
                        let env = crate::logic::shell::Env::skipping(&f.now, &f.skip);
                        crate::logic::shell::app::save_game_mode(g, ui, rng, &env, 2);
                        for (name, bytes) in env.files.borrow().written.iter() {
                            if let Some(bytes) = bytes {
                                ui.write_file(&PbStr::from(name.as_str()), bytes);
                            }
                        }
                    }
                    g.auto_save_state = 0;
                }
            }
        }
        if !handled && g.auto_save_state == 2 {
            g.auto_save_state = 0;
        }
        g.sim_co_count = 1;
    }
    if g.sim_co_count == 775 && g.turn_action_count < 5 {
        g.turn_action_count += 1;
    }
    if g.num_players + 1 == g.sim_co_count && quarter_end(g, ui, rng, f) {
        return;
    }
    if g.sim_co_count % 2 == 0 {
        g.ind_reprice_cycle_idx += 1;
        if 71 < g.ind_reprice_cycle_idx {
            g.ind_reprice_cycle_idx = 1;
        }
        f.idx = 11;
        while f.idx <= 1600 {
            let i = f.idx;
            if g.entity_status[i] == 1 {
                if cast::<i32, _>(g.industry_num[i]) == g.ind_reprice_cycle_idx {
                    calc_stock_price(g, ui, rng, i, f.time_secs);
                }
            } else if g.has_options_outstanding[i] != 0 {
                close_option_positions(g, ui, rng, 0, i, 2, 0);
            }
            f.idx += 1;
        }
    }
    if g.num_players < g.sim_co_count && g.sim_co_count < 11 {
        return;
    }
    f.tmp_long = 0;
    if 100 < g.sim_co_count {
        f.tmp_long = to_quad((cast::<Ext, _>(g.sim_co_count)) % Ext::from_f64(100.0));
    }
    if 0 < f.tmp_long && f.tmp_long <= cast::<i64, _>(g.num_players) {
        f.ent_id = cast::<i32, _>(f.tmp_long);
        if !(g.player_name[f.ent_id] == "...OUT...") {
            f.ent_name = g.player_name[f.ent_id].clone();
            find_short_positions(g, ui, rng, 0, 0);
            if !f.skips(0x4daa4b) {
                player::check_solvency(g, ui, rng, f);
            }
            f.bond_int_status = Ext::from_f64(0.0);
            f.unused_amt = Ext::from_f64(0.0);
            f.bond_ops_total = Ext::from_f64(0.0);
            f.sold_list = PbStr::new();
            if g.player_name[f.ent_id] == "...OUT..." {
                g.sim_co_count -= 1;
                return;
            }
        }
    }
    f.ent_id = g.sim_co_count;
    f.ent_name = g.ent_name[g.sim_co_count].clone();
    if g.sim_co_count <= g.num_players {
        f.managing_player = g.sim_co_count;
        let e = f.ent_id;
        if !(g.player_name[e] == "...OUT...") {
            f.amount = g.exchange_rate * sf(0.25f32);
            g.cash[e] = g.cash[e] - f.amount;
            balance_cash_t_bills(g, e);
            if g.cash[e] < Ext::from_f64(0.0) {
                calc_entity_financials(g, ui, rng, e);
                if g.net_worth < Ext::from_f64(0.0) {
                    g.news_msg = g.player_name[e].clone()
                        + __content.string(10447);
                    post_news(g, ui, e, 0);
                }
            }
            f.amount = Ext::from_f64(0.0);
        }
        if !f.skips(0x4c9a4b) {
            economy_tick::clean_holdings(g, f);
        }
        if !f.skips(0x4d9853) {
            player::quarterly_pay(g, ui, rng, f);
        }
        return;
    }
    let e = f.ent_id;
    f.proj_earnings = g.proj_qtr_earnings[e];
    f.industry = g.industry_num[e];
    f.industry_orig = f.industry;
    g.tmp_symbol = g.ent_symbol[e].clone();
    f.name_and_symbol = g.ent_name[e].clone() + " (" + &g.ent_symbol[e] + ")";
    if g.entity_status[e] == 1 {
        f.start_mkt_cap = g.market_cap[e];
        if 1 < f.industry {
            if g.exchange_rate * Ext::from_f64(15000.0) < g.cash[e] + g.t_bills[e] && rng.rnd() * Ext::from_f64(4.0) < Ext::from_f64(1.0) {
                g.cash_rich_corp = cast::<i16, _>(e);
            }
        }
        if 2 < f.industry && f.industry < 70 {
            if g.exchange_rate * Ext::from_f64(1300.0) < g.cash[e] + g.t_bills[e]
                && g.business_assets[e] * sf(0.6f32) < g.cash[e] + g.t_bills[e]
            {
                g.cash_rich_corp = cast::<i16, _>(e);
            }
        }
        if f.industry == 70 {
            if g.exchange_rate * Ext::from_f64(1000.0) < g.cash[e] + g.t_bills[e] && 7 < g.credit_rating_num[e] {
                g.cash_rich_corp = cast::<i16, _>(e);
            }
        }
        find_option_positions(g, ui, 0, e, 0, 0);
    }
    f.cmp_tmp = 20;
    if f.cmp_tmp < len(g.ent_misc_data[e].as_bytes()) {
        f.margin_text = parse_any(g.ent_misc_data[e].as_bytes(), b"|", 30);
        f.misc_debt_field = val(parse_any(g.ent_misc_data[e].as_bytes(), b"|", 20).as_bytes());
        if f.industry == 71 {
            if g.cur_quarter == 4 {
                f.cap_gain_dist = cast::<Ext, _>(g.extraordinary_pl[e]);
            }
            g.tax_loss_carry[e] = cast::<Ext, _>(g.extraordinary_pl[e]);
            if Ext::from_f64(0.0) < g.tax_loss_carry[e] {
                g.tax_loss_carry[e] = Ext::from_f64(0.0);
            }
        }
        g.ent_misc_data[e] = parse_any(g.ent_misc_data[e].as_bytes(), b"|", 3);
    } else if g.cur_quarter == 1 && g.earnings_y1[e] != Ext::from_f64(0.0) {
        if g.entity_status[e] == 1 {
            g.ent_misc_data[e] = fmt_trim(g.earnings_y1[e] * sf(0.25f32), f.fmt_money.as_bytes());
            if g.start_year == g.cur_year {
                f.misc_debt_field = g.bank_debt[e];
            }
        }
    } else if g.entity_status[e] == 1 {
        g.ent_misc_data[e] = PbStr::from("0");
    }
    g.rand_seed = to_long((cast::<Ext, _>(f.time_secs % 10) + cast::<Ext, _>(g.rand_seed)) + int(rng.rnd() * Ext::from_f64(5.0)));
    if 32000 < g.rand_seed {
        g.rand_seed = to_long(int(rng.rnd() * Ext::from_f64(5000.0)));
    }
    if !f.skips(0x4c89e0) {
        economy_tick::update_economy(g, ui, rng, f);
    }
    if rng.rnd() * Ext::from_f64(15.0) < Ext::from_f64(1.0) && g.sim_co_count < 131 && !f.skips(0x4f721b) {
        fin_inst::ai_rand_sell_junk_bonds(g, ui, rng);
    }
    if !f.skips(0x4d83dc) {
        earnings_news::bond_maturity(g, ui, rng, f);
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    if g.rand_seed % 5 == 3 && !f.skips(0x4f41e5) {
        {
            let now = f.now;
            ai_corp::ai_random_lbo(g, ui, rng, 0, &now);
        }
    }
    if g.tmp_ext1 == Ext::from_f64(1.0) {
        f.lbo_done = 1;
        g.tmp_ext1 = Ext::from_f64(0.0);
    }
    if g.sim_co_count % 100 == 1 && !f.skips(0x4f4a0f) {
        ai_corp::process_negative_cash(g, ui, rng);
    }
    if !f.skips(0x4c9a4b) {
        economy_tick::clean_holdings(g, f);
    }
    if !f.skips(0x4c9b79) {
        economy_tick::asbestos_superfund(g, ui, rng, f);
    }
    if !f.skips(0x4c9e42) {
        operating::operating_results(g, ui, rng, f);
    }
    if !f.skips(0x4cd5cf) {
        accounting::repay_bank_debt(g, ui, rng, f);
    }
    if !f.skips(0x4cd774) {
        accounting::quarter_end_accounting(g, ui, rng, f);
    }
    if !f.skips(0x4ce6a0) {
        accounting::close_quarter_books(g, ui, rng, f);
    }
    if !f.skips(0x4cff86) {
        earnings_news::earnings_news(g, ui, rng, f);
    }
    let e = f.ent_id;
    if 20 < f.market_share && f.managing_player == 0 && g.entity_status[e] == 1 {
        g.tmp_ext1 = cast::<Ext, _>(f.market_share);
        if !f.skips(0x4f9975) {
            ai_corp::ai_consolidate_subsidiary(g, ui, rng, e, f.time_secs);
        }
    }
    if g.sim_co_count % 100 == 95 && !f.skips(0x4e635d) {
        fin_inst::bank_system_liquidity(g, ui, rng, 0);
    }
    if g.cur_quarter == 2 && g.sim_co_count == 775 && !f.skips(0x4f8f90) {
        crate::logic::economy::adjust_ind_long_term_growth(g, rng);
    }
    if g.market_cap[g.sim_co_count] < rng.rnd() * Ext::from_f64(3.0) && !f.skips(0x4f9223) {
        ai_corp::ai_voluntary_liquidation(g, ui, rng, f.ent_id);
    }
    if g.entity_status[f.ent_id] == 1 {
        check_cross_ownership(g, ui, rng, f.ent_id, 1);
        if g.sim_co_count != g.co_cnt_backup {
            g.sim_co_count = g.co_cnt_backup;
        }
        f.ent_id = g.sim_co_count;
        if f.managing_player == 0 && g.industry_num[g.sim_co_count] == 70 {
            if !f.skips(0x4f9fd4) {
                let c = g.sim_co_count;
                ai_corp::ai_corp_reenter_industry(g, ui, rng, c, f.time_secs);
            }
        }
    }
}

/// The quarter-end block of SimProcessNextEntity (`SimCoCount = NumPlayers + 1`). Returns true when the
/// original's ON ERROR handler would end the tick (see below).
fn quarter_end(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) -> bool {
    let __content = crate::content::current();
    process_swap_payments(g, ui, rng, 0, &PbStr::new());
    if g.industry_num[1595] == 71 && g.entity_status[1595] == 1 {
        f.swap_pnl = to_quad(swap_pnl_accumulator(g, 1595, 2, Ext::from_f64(0.0)));
        if 50 < f.swap_pnl.abs() {
            f.msg_part1 = PbStr::from("loss");
            if 0 < f.swap_pnl {
                f.msg_part1 = PbStr::from("profit");
            }
            g.news_msg = g.ent_symbol[1595].clone()
                + __content.string(10448)
                + &f.msg_part1
                + " of "
                + &fmt_trim(cast::<Ext, _>(f.swap_pnl.abs()), b"* ###,")
                + " "
                + &g.million_prefix_uc
                + __content.string(10449)
                + &trim(str_s(cast::<Ext, _>(g.cur_quarter)).as_bytes());
            post_news(g, ui, 1595, 0);
        }
    }
    f.idx = g.cur_player;
    f.swap_pnl = to_quad(swap_pnl_accumulator(g, f.idx, 2, Ext::from_f64(0.0)));
    if f.swap_pnl.abs() <= 50 && 10 < g.sel_entity {
        f.idx = g.sel_entity;
        f.swap_pnl = to_quad(swap_pnl_accumulator(g, f.idx, 2, Ext::from_f64(0.0)));
    }
    if 50 < f.swap_pnl.abs() && g.entity_status[f.idx] == 1 && f.idx != 1595 {
        f.msg_part1 = PbStr::from(__content.string(10450));
        if 0 < f.swap_pnl {
            f.msg_part1 = PbStr::from(__content.string(10451));
        }
        g.news_msg = g.ent_symbol[f.idx].clone()
            + &f.msg_part1
            + " of "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(f.swap_pnl.abs()), b"* ###,")
            + " "
            + &g.million_prefix_lc
            + __content.string(10449)
            + &trim(str_s(cast::<Ext, _>(g.cur_quarter)).as_bytes());
        post_news(g, ui, f.idx, 0);
    }
    if !f.skips(0x5082ae) {
        quarter_start::pay_annual_wealth_tax(g, ui, rng, f.time_secs);
    }
    f.msg_part1 = PbStr::from(__content.string(10452)) + &str_s(cast::<Ext, _>(g.cur_quarter)) + " ended ";
    g.cur_quarter += 1;
    if g.turn_action_count < 5 {
        g.turn_action_count += 1;
    }
    for _ in 0..2 {
        for i in 1..=9 {
            g.recent_antitrust_filers[i] = g.recent_antitrust_filers[i + 1];
            g.recent_offerings[i] = g.recent_offerings[i + 1];
        }
        g.recent_antitrust_filers[10] = 0;
        g.recent_offerings[10] = 0;
    }
    f.slot = 0;
    f.player_count = g.num_players;
    f.idx = 1;
    while f.idx <= f.player_count {
        let i = f.idx;
        if -3 < g.player_recent_bond_issues[i] {
            g.player_recent_bond_issues[i] -= 1;
        }
        if g.player_loan_offers_avail[i] < 5 {
            g.player_loan_offers_avail[i] += 1;
        }
        f.idx += 1;
    }
    if 4 < g.cur_quarter {
        g.bank_merger_player = 0;
        g.cur_quarter = 1;
        f.msg_part1 = PbStr::from("...Year") + &str_s(cast::<Ext, _>(g.cur_year)) + " ended ";
        g.cur_year += 1;
        f.player_count = g.num_players;
        f.idx = 1;
        while f.idx <= f.player_count {
            if 0 < g.player_startup_count[f.idx] {
                g.player_startup_count[f.idx] -= 1;
            }
            f.idx += 1;
        }
        if g.cur_year % 3 == 0 {
            g.adv_offering_corp = 0;
            g.cheat_eq_fund_target = 0;
        }
    }
    g.ticker_text = g.ticker_text.clone() + "   " + &f.msg_part1;
    if !f.skips(0x67fbfa) {
        quarter_start::insider_trading_investigation(g, ui, rng);
    }
    if !f.skips(0x6694c4) {
        quarter_start::expire_asset_sale_offers(g, ui);
    }
    if !f.skips(0x4e6b8c) {
        quarter_start::gov_bond_maturity_and_auction(g, ui, rng);
    }
    if 10 < g.rumor_victim_id {
        let v = g.rumor_victim_id;
        if g.entity_status[v] == 1 {
            let d = cast::<i32, _>(g.libel_suit_defendant);
            if !(1..=5).contains(&d) {
                // PlayerName(d) / PlayerNotoriety(d) raise error 9 (DIM 1 TO 5): the ON ERROR handler shows
                // ShowRuntimeError and leaves SimProcessNextEntity.
                return true;
            }
            if 7 < g.player_notoriety[d] {
                // judgment against the rumor-monger
                if !(g.player_name[d] == "...OUT...") {
                    f.slot = d;
                    calc_entity_financials(g, ui, rng, f.slot);
                    f.tmp_long = to_quad(g.total_assets * sf(0.2f32));
                    f.tmp_long =
                        to_quad(cast::<Ext, _>(f.tmp_long) * (sf(0.5f32) * rng.rnd()) + cast::<Ext, _>(f.tmp_long));
                    g.cash[v] = cast::<Ext, _>(f.tmp_long) + g.cash[v];
                    g.equity_income_ytd[v] = cast::<Ext, _>(f.tmp_long) + g.equity_income_ytd[v];
                    g.cash[d] = g.cash[d] - cast::<Ext, _>(f.tmp_long);
                    f.msg_part1 = PbStr::from("slander");
                    if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                        f.msg_part1 = PbStr::from("libel");
                    }
                    g.news_msg = PbStr::from(__content.string(10453))
                        + &g.ent_symbol[v]
                        + " large "
                        + &f.msg_part1
                        + __content.string(10454)
                        + &g.ent_name[d];
                    f.msg_part1 = g.news_msg.clone()
                        + __content.string(10455)
                        + &trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(cast::<Ext, _>(f.tmp_long), b"* ###,.00")
                        + " "
                        + &g.million_prefix_lc
                        + __content.string(10456);
                    f.msg_part1 = ucase(f.msg_part1.as_bytes(), CaseMode::Ansi);
                    ui.msgbox(
                        &f.msg_part1,
                        0x2040,
                        &PbStr::from(__content.string(10457)),
                    );
                }
            } else {
                if g.industry_num[v] < 70 {
                    g.profit_margin[v] -= 7;
                    g.player_notoriety[d] += 1;
                }
                g.news_msg = g.ent_symbol[v].clone()
                    + __content.string(10458);
                if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() || g.industry_num[v] == 70 {
                    g.news_msg = g.ent_symbol[v].clone()
                        + __content.string(10459);
                }
                if -40 < g.investor_sentiment[v] {
                    g.investor_sentiment[v] -= 15;
                }
            }
            post_news(g, ui, 0, 0);
        }
        g.rumor_victim_id = 0;
    }
    f.tmp_long = 0;
    f.amt_long = -10;
    f.idx = 11;
    while f.idx <= 1600 {
        let i = f.idx;
        if g.entity_status[i] == 1 && g.industry_num[i] <= 69 {
            if (cast::<Ext, _>(f.amt_long)) > cast::<Ext, _>(g.profit_margin[i])
                && (cast::<Ext, _>(g.insider_earn_tip_corp)).abs() != cast::<Ext, _>(i)
            {
                f.amt_long = cast::<i64, _>(g.profit_margin[i]);
                f.tmp_long = cast::<i64, _>(i);
            }
        }
        f.idx += 1;
    }
    if f.amt_long < -10 && 10 < f.tmp_long {
        let c = cast::<i32, _>(f.tmp_long);
        g.profit_margin[c] += 12;
        g.news_msg =
            PbStr::from(__content.string(10460)) + &g.ent_symbol[c];
        f.idx = c;
        calc_projected_earnings(g, ui, rng, f.idx);
    }
    false
}
