//! Menu advice texts: `BuildMenuAdviceText` (0x557bbf) builds `HelpText`, the " [runtime resource wording] "
//! text of the transactions menu (`ShowActionMenu`), for the menu `ActionMenuMode` 1..7 and the
//! selected / active entity, plus its GOSUB `MenuAdvice_HoldingCoTaxNote` (0x56f024) and the helpers
//! `FindIndustryLeaderPair` (0x4a30c0) and `CalcRequiredAnnualReturn` (0x588c72).
//!
//! The routine's locals live in [`Ctx`] (zeroed at entry, like the original's frame); the menu cases are
//! ported in submodules: [`player`] (mode 1, a player selected: financial situation, game-end score,
//! taxes), [`corp`] (mode 1, a company selected), [`menus`] (modes 3..7), [`economy`] and [`markets`]
//! (mode 2: economy, commodity and market commentary).
//!
//! Numeric conventions: `EXT` uses software x87 arithmetic; SINGLE pool constants (`fmul dword [0x75e…]`) are written
//! `sf(0.05)`; every float → integer store uses `to_long` / `to_integer` / `to_quad`; comparisons are
//! written so that the branch taken for unordered (NaN) operands matches the machine code.
//!
//! Error handling: the ON ERROR handler (0x56f28d) calls ShowRuntimeError (0x46ea48) and leaves the
//! routine (without the final clean-up of `HelpText`). Sections return [`R`]; an [`Abort`] stands for
//! such a runtime error.

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
    clippy::nonminimal_bool
)]
use crate::pb::real::{Ext, cast};


mod corp;
mod economy;
mod markets;
mod market_commodities;
mod menus;
mod player;

/// Common imports of the submodules.
pub(crate) mod prelude {
use crate::pb::real::{Ext, cast};

    #[allow(unused_imports)]
    pub(crate) use super::{Abort, Ctx, R};
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
        instr, len, ltrim, mcase, mid, parse_any, replace, right, rtrim, ucase, CaseMode,
    };
    #[allow(unused_imports)]
    pub(crate) use crate::pb::numeric::{ceil, fix, frac, int, to_integer, to_long, to_quad};
    #[allow(unused_imports)]
    pub(crate) use crate::state::Globals;
    #[allow(unused_imports)]
    pub(crate) use crate::ui::Ui;

    /// A string literal.
    #[allow(dead_code)]
    pub(crate) fn s(x: &str) -> PbStr {
        PbStr::from(x)
    }

    /// A SINGLE constant of the original's pool (`fmul dword [..]`): the f32 value, widened.
    #[allow(dead_code)]
    pub(crate) fn sf(x: f32) -> Ext {
        cast::<Ext, _>(x)
    }

    /// `FORMAT$(x, mask)`.
    #[allow(dead_code)]
    pub(crate) fn fmt(x: Ext, mask: &str) -> PbStr {
        PbStr(format(x, mask.as_bytes()))
    }

    /// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
    #[allow(dead_code)]
    pub(crate) fn fmt_trim(x: Ext, mask: &str) -> PbStr {
        trim(&fmt(x, mask))
    }

    /// `RTRIM$(LTRIM$(s))`.
    #[allow(dead_code)]
    pub(crate) fn trim(x: &PbStr) -> PbStr {
        rtrim(ltrim(x.as_bytes()).as_bytes())
    }

    /// `STR$(x)` of a LONG/QUAD/EXT value.
    #[allow(dead_code)]
    pub(crate) fn str_s(x: Ext) -> PbStr {
        PbStr::from(str_num(x, 15).as_str())
    }

    /// `REPLACE$(x, a, b)` in place.
    #[allow(dead_code)]
    pub(crate) fn repl(x: &mut PbStr, a: &str, b: &str) {
        *x = replace(x.as_bytes(), a.as_bytes(), b.as_bytes());
    }

    /// `INSTR(1, x, find)`.
    #[allow(dead_code)]
    pub(crate) fn has(x: &PbStr, find: &str) -> i32 {
        instr(1, x.as_bytes(), find.as_bytes())
    }

    /// `CHR$(34)`.
    #[allow(dead_code)]
    pub(crate) const Q: &str = "\"";
    /// `$CRLF` twice (a paragraph break).
    #[allow(dead_code)]
    pub(crate) const CRLF2: &str = "\r\n\r\n";

    /// The x87 "equal" outcome (ZF = 1): equal, or unordered.
    #[allow(dead_code)]
    pub(crate) fn eq87(a: Ext, b: Ext) -> bool {
        a == b || a.is_nan() || b.is_nan()
    }
}

use crate::logic::earnings::calc_cur_month_of_year;
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials};
use crate::logic::market::update_market_display;
use crate::logic::restructure::show_runtime_error;
use crate::logic::trading::get_selected_player_etf;
use prelude::*;

/// A runtime error of the original (its ON ERROR handler shows ShowRuntimeError and leaves).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Abort(pub i32);

/// Result of a section: `Err` = runtime error (error number).
pub(crate) type R = Result<(), Abort>;

/// The locals of `BuildMenuAdviceText` (zeroed at entry; frame offsets from EBP). The GOSUB
/// `MenuAdvice_HoldingCoTaxNote` and every menu case work on them.
#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct Ctx {
    // ---- STRING
    pub phrase: PbStr,             // -0x180
    pub clause: PbStr,             // -0x184
    pub extra_text: PbStr,         // -0x188
    pub snippet: PbStr,            // -0x18c
    pub paragraph: PbStr,          // -0x190
    pub fmt_decimal: PbStr,        // -0x194
    pub fmt_money: PbStr,          // -0x198
    pub currency_prefix: PbStr,    // -0x19c
    pub currency_suffix: PbStr,    // -0x1a0
    pub industry_name: PbStr,      // -0x1a4
    pub holding_co_tax_note: PbStr, // -0x1a8
    pub leader_label: PbStr,       // -0x1ac
    pub gdp_sustainable: PbStr,    // -0x1b0

    // ---- LONG
    pub cmp_tmp: i32,            // -0x2c
    pub entity_id: i32,          // -0x88
    pub idx: i32,                // -0x8c
    pub slot: i32,               // -0x90
    pub hold_slot: i32,          // -0x94
    pub trend_a: i32,            // -0xb0
    pub trend_b: i32,            // -0xb4
    pub pct_change: i32,         // -0xb8
    pub pct_change2: i32,        // -0x108
    pub analyst_rating: i32,     // -0x10c
    pub gdp_trend: i32,          // -0x110
    pub best_ceo_corp: i32,      // -0x114
    pub cur_month: i32,          // -0x138
    pub price_streak: i32,       // -0x13c
    pub gdp_para_shown: i32,     // -0x140
    pub day_of_month: i32,       // -0x150
    pub override_day: i32,       // -0x154
    pub under_investigation: i32, // -0x160
    pub tax_crisis: i32,         // -0x164
    pub loop_guard: i32,         // -0x1b4
    pub unk_1b8: i32,            // -0x1b8
    pub not_controlled: i32,     // -0x1bc

    // ---- INTEGER
    pub largest_holding_id: i16, // -0x144
    pub industry_id: i16,        // -0x17c

    // ---- QUAD
    pub abs_streak: i64,       // -0x5c
    pub swap_payment_abs: i64, // -0xc0
    pub swap_payment: i64,     // -0x14c

    // ---- EXT
    pub case_val: Ext,        // -0x40
    pub amount_a: Ext,        // -0xa0
    pub amount_b: Ext,        // -0xac
    pub income_tax: Ext,      // -0xcc
    pub taxable_income: Ext,  // -0xd8
    pub ordinary_income: Ext, // -0xe4
    pub amount: Ext,          // -0xf0
    pub wealth_ratio: Ext,    // -0xfc
    pub best_ceo_pay: Ext,    // -0x120
    pub neg_dividends: Ext,   // -0x12c
    pub other_taxes: Ext,     // -0x170

    // ---- DOUBLE
    pub month_frac: Ext,    // -0x104
    pub cap_gains_tax: Ext, // -0x134
    pub month_in_qtr: Ext,  // -0x15c
    pub annual_return: Ext, // -0x178
}

/// How a menu case ends: `Cleanup` falls through to the final clean-up of `HelpText` (0x56eef8),
/// `Exit` leaves the routine at once (`EXIT SUB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    Cleanup,
    Exit,
}

/// Port of BuildMenuAdviceText (0x557bbf).
/// Runs `UpdateMarketDisplay` and builds the advice text `HelpText` for the transactions menu
/// `ActionMenuMode` (see [`build_menu_advice_text_body`]). `time_secs` = `VAL(RIGHT$(TIME$, 2))` for
/// `UpdateMarketDisplay`.
pub fn build_menu_advice_text(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    update_market_display(g, ui, rng, time_secs);
    build_menu_advice_text_body(g, ui, rng);
}

/// Port of BuildMenuAdviceText (0x557bbf) after its `UpdateMarketDisplay` call: builds `HelpText`.
/// For a menu of an entity the player does not control (or an ETF advised by a company of the player)
/// it explains what may be done; else mode 1 describes the selected player / company, mode 2 the
/// economy and markets, modes 3..7 give advice for the borrowing / trading / financing / management /
/// other menus. At the end, `£`/`¥` are removed, double blanks and `"$ "` are collapsed.
pub fn build_menu_advice_text_body(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let mut c = Ctx::default();
    match body(g, ui, rng, &mut c) {
        Ok(End::Cleanup) => cleanup(g),
        Ok(End::Exit) => {}
        Err(Abort(err)) => {
            // PORT-NOTE: the handler's procedure name / diagnostic literals (0x706e50 / 0x706e3c)
            show_runtime_error(g, ui, err, &s("BuildMenuAdviceText"), &PbStr::new(), 0);
        }
    }
}

/// The final clean-up (0x56eef8): `REMOVE$(HelpText, ANY CHR$(163, 165))`, then collapse `"  "` and
/// `"$ "` until neither occurs.
fn cleanup(g: &mut Globals) {
    let pound_yen = [163u8, 165u8];
    g.help_text = crate::pb::string::remove_any(g.help_text.as_bytes(), &pound_yen);
    loop {
        if instr(1, g.help_text.as_bytes(), b"  ") < 1 && instr(1, g.help_text.as_bytes(), b"$ ") < 1 {
            return;
        }
        repl(&mut g.help_text, "  ", " ");
        repl(&mut g.help_text, "$ ", "$");
    }
}

fn body(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> Result<End, Abort> {
    let __content = crate::content::current();
    c.fmt_decimal = s("#################.00");
    c.fmt_money = s("#################,.00");
    c.entity_id = g.sel_entity;
    c.currency_prefix = trim(&g.currency_symbol);
    c.currency_suffix = g.currency_suffix.clone();
    if g.currency_code == "UK" {
        c.currency_prefix = PbStr::new();
        c.currency_suffix = s(" pounds");
    } else if g.currency_code == "JAP" {
        c.currency_prefix = PbStr::new();
        c.currency_suffix = s(" yen");
    } else if g.currency_code == "SWI" {
        c.currency_suffix = s(__content.string(1000));
    }
    g.help_text = PbStr::new();
    c.amount = calc_cur_month_of_year(g, g.sim_co_count);
    c.cur_month = to_long(ceil(c.amount));
    let e = ent(c.entity_id)?;
    c.extra_text = g.ent_name[e].clone() + " (" + &g.ent_symbol[e] + __content.string(1001) + CRLF2;
    c.clause = s(__content.string(1002))
        + __content.string(1003)
        + "by ";
    c.phrase = s(__content.string(1004));
    if 10 < g.active_entity {
        c.phrase = g.active_entity_name.clone();
    }
    c.clause = c.clause.clone() + &c.phrase + ".";
    if 10 < c.entity_id && g.active_entity != c.entity_id && g.action_menu_mode == 3 {
        let skip = g.industry_num[e] == 71 && get_selected_player_etf(g, ui, rng) == c.entity_id;
        if !skip {
            c.clause = c.clause.clone()
                + CRLF2
                + __content.string(1005)
                + &g.sel_corp_name
                + __content.string(1006);
        }
    }
    if g.active_entity != c.entity_id && 2 < g.action_menu_mode {
        g.help_text = c.extra_text.clone() + &c.clause;
        c.clause = PbStr::new();
        c.not_controlled = 1;
    }
    let etf = get_selected_player_etf(g, ui, rng);
    if 1580 < etf {
        let ef = ent(etf)?;
        if etf < 1586 && g.action_menu_mode == 4 && 0 < g.hold_stock_id[(0, ef)] {
            g.help_text = g.help_text.clone() + &c.clause;
            if 10 < g.active_entity {
                g.help_text = g.help_text.clone() + __content.string(1007);
            }
            g.help_text = g.help_text.clone()
                + CRLF2
                + __content.string(1008)
                + &g.ent_name[ef]
                + __content.string(1009)
                + __content.string(1010);
            if 10 < g.active_entity {
                g.help_text = g.help_text.clone() + __content.string(1011);
            }
            g.help_text = g.help_text.clone()
                + __content.string(1012);
        }
        let mode = g.action_menu_mode;
        if (1585 < etf || mode != 4) && (mode == 3 || mode == 4 || mode == 5) {
            g.help_text = g.help_text.clone()
                + &c.clause
                + __content.string(1007)
                + CRLF2
                + __content.string(1013)
                + &g.ent_name[ef]
                + " ("
                + &g.ent_symbol[ef]
                + ") is "
                + __content.string(1014)
                + __content.string(1015);
            if mode == 3 {
                let bank = ent(cast::<i32, _>(g.lending_bank[ef]))?;
                g.help_text = g.help_text.clone()
                    + __content.string(1016)
                    + &g.ent_name[bank]
                    + " on "
                    + &g.ent_symbol[ef]
                    + __content.string(1017);
                if g.sel_entity == etf {
                    repl(&mut g.help_text, "two", "three");
                    g.help_text = g.help_text.clone()
                        + __content.string(1018)
                        + &g.sel_corp_name
                        + ". ";
                } else {
                    g.help_text = g.help_text.clone() + ". ";
                }
            } else if mode == 4 {
                if g.active_entity < 11 && 1585 < etf {
                    repl(&mut g.help_text, __content.string(1019), __content.string(1020));
                    g.help_text = g.help_text.clone() + __content.string(1021);
                    g.help_text = g.help_text.clone()
                        + CRLF2
                        + __content.string(1022)
                        + __content.string(1023)
                        + __content.string(1024)
                        + __content.string(1025)
                        + __content.string(1026);
                } else {
                    repl(&mut g.help_text, "two", "six");
                    g.help_text = g.help_text.clone() + __content.string(1027);
                }
            } else {
                repl(&mut g.help_text, "two", "four");
                g.help_text = g.help_text.clone()
                    + __content.string(1028)
                    + __content.string(1029);
            }
            return Ok(End::Exit);
        }
    }
    if c.not_controlled == 1 {
        return Ok(End::Exit);
    }
    if g.start_year == g.cur_year {
        g.help_text = s(__content.string(1030));
        if 3 < g.action_menu_mode {
            let a = ent(g.active_entity)?;
            g.help_text = g.help_text.clone()
                + __content.string(1031)
                + &g.ent_symbol[a]
                + __content.string(1032)
                + __content.string(1033)
                + Q
                + __content.string(1034)
                + Q
                + CRLF2;
        }
    }
    c.case_val = cast::<Ext, _>(g.action_menu_mode);
    match g.action_menu_mode {
        1 => mode1(g, ui, rng, c),
        2 => {
            economy::mode2_economy(g, ui, rng, c)?;
            markets::mode2_markets(g, ui, rng, c)?;
            Ok(End::Cleanup)
        }
        3 => {
            menus::mode3(g, ui, rng, c)?;
            Ok(End::Cleanup)
        }
        4 => menus::mode4(g, ui, rng, c),
        5 => {
            menus::mode5(g, ui, rng, c)?;
            Ok(End::Cleanup)
        }
        6 => {
            menus::mode6(g, ui, rng, c)?;
            Ok(End::Cleanup)
        }
        7 => {
            menus::mode7(g, ui, rng, c)?;
            Ok(End::Cleanup)
        }
        _ => Ok(End::Cleanup),
    }
}

/// An entity index for `EntName` etc. (`DIM (0 TO 1600)`): error 9 outside.
pub(crate) fn ent(id: i32) -> Result<i32, Abort> {
    if (0..=1600).contains(&id) { Ok(id) } else { Err(Abort(9)) }
}

/// Mode 1 (entity information menu): head, the player / company part, and the recent headlines.
fn mode1(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> Result<End, Abort> {
    let __content = crate::content::current();
    let sel = ent(g.sel_entity)?;
    if 10 < g.sel_entity {
        let ind = cast::<i32, _>(g.industry_num[sel]);
        if !(0..=71).contains(&ind) {
            return Err(Abort(9));
        }
        c.industry_name = mcase(g.ind_name[ind].as_bytes(), CaseMode::Ansi);
        if 0 < has(&c.industry_name, "&") {
            repl(&mut c.industry_name, "&", "and");
        }
    }
    if g.start_year == g.cur_year {
        c.phrase = s(". ");
        if 10 < g.sel_entity {
            c.phrase = s(__content.string(1035)) + __content.string(1036) + &c.industry_name + __content.string(1037);
        }
        g.help_text = g.help_text.clone()
            + __content.string(1038)
            + __content.string(1039)
            + &g.ent_symbol[sel]
            + &c.phrase;
        g.help_text = g.help_text.clone() + __content.string(1040);
        g.help_text = g.help_text.clone() + CRLF2;
    }
    if g.sel_entity < 11 {
        if player::mode1_player(g, ui, rng, c)? == End::Exit {
            return Ok(End::Exit);
        }
    } else {
        corp::mode1_corp(g, ui, rng, c)?;
    }
    let e = ent(c.entity_id)?;
    c.clause = s(" ") + &g.ent_symbol[e] + " ";
    c.extra_text = s("(") + &g.ent_symbol[e] + ")";
    c.snippet = s(" ") + &g.ent_symbol[e] + ".";
    c.phrase = PbStr::new();
    c.idx = 1;
    loop {
        c.paragraph = s(" ") + &g.news_headline[c.idx] + " ";
        let p = c.paragraph.as_bytes();
        if instr(1, p, c.clause.as_bytes()) >= 1
            || 0 < instr(1, p, c.extra_text.as_bytes())
            || 0 < instr(1, p, c.snippet.as_bytes())
        {
            if has(&c.paragraph, __content.string(1041)) == 0 {
                c.phrase = rtrim(c.paragraph.as_bytes());
            }
        }
        c.idx += 1;
        if c.idx >= 61 {
            break;
        }
    }
    if !c.phrase.is_empty() {
        c.phrase = s(__content.string(1042)) + &c.phrase;
        g.help_text = g.help_text.clone() + &c.phrase;
    }
    Ok(End::Cleanup)
}

/// Port of MenuAdvice_HoldingCoTaxNote (0x56f024), a GOSUB of BuildMenuAdviceText.
/// For a company as active entity that owes capital taxes and [runtime resource wording]
/// (`HoldingCoTaxDue > 0`), sets `holdingCoTaxNote` to the warning (else leaves it unchanged).
pub(crate) fn holding_co_tax_note(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) {
    let __content = crate::content::current();
    if 10 < g.active_entity {
        calc_entity_financials(g, ui, rng, c.entity_id);
        let tax = calc_capital_taxes(g, c.entity_id);
        if tax > Ext::from_f64(0.0) && g.holding_co_tax_due > Ext::from_f64(0.0) {
            let a = g.active_entity;
            c.holding_co_tax_note = g.active_entity_name.clone()
                + __content.string(1043)
                + __content.string(1044);
            if g.industry_num[a] == 70 {
                c.holding_co_tax_note = c.holding_co_tax_note.clone()
                    + __content.string(1045);
            } else {
                c.holding_co_tax_note = c.holding_co_tax_note.clone()
                    + __content.string(1046)
                    + __content.string(1047);
            }
            c.holding_co_tax_note = c.holding_co_tax_note.clone()
                + __content.string(1048)
                + &g.ent_symbol[c.entity_id]
                + __content.string(1049);
            if g.industry_num[a] == 70 {
                c.holding_co_tax_note = c.holding_co_tax_note.clone() + "." + CRLF2;
            } else {
                c.holding_co_tax_note = c.holding_co_tax_note.clone()
                    + __content.string(1050)
                    + __content.string(1051)
                    + CRLF2;
            }
        }
    }
}

/// Port of MenuAdvice_Stub (0x56f301): the routine's epilogue (`EXIT SUB` target); nothing to port.
#[allow(dead_code)]
fn menu_advice_stub() {}

/// Port of FindIndustryLeaderPair (0x4a30c0).
/// For an operating industry (3..69): the active company (131..1600) with the largest business assets,
/// if the runner-up has at least a third of its assets; the runner-up is returned in `TmpExt2`
/// (0 otherwise). Returns 0 when there is no such pair.
pub fn find_industry_leader_pair(g: &mut Globals, industry: i16) -> i32 {
    let mut top_assets = Ext::from_f64(0.0f64);
    let mut second_assets = Ext::from_f64(0.0f64);
    let mut leader_corp = 0i32;
    let mut second_corp = 0i32;
    let mut result = 0i32;
    g.tmp_ext2 = Ext::from_f64(0.0);
    if 2 < industry && industry < 70 {
        for corp_id in 131..=1600i32 {
            if g.entity_status[corp_id] == 1 && g.industry_num[corp_id] == industry && top_assets < g.business_assets[corp_id] {
                top_assets = g.business_assets[corp_id];
                leader_corp = corp_id;
            }
        }
        for corp_id in 131..=1600i32 {
            if g.entity_status[corp_id] == 1
                && g.industry_num[corp_id] == industry
                && leader_corp != corp_id
                && second_assets < g.business_assets[corp_id]
            {
                second_assets = g.business_assets[corp_id];
                second_corp = corp_id;
            }
        }
        if 130 < leader_corp && 130 < second_corp {
            if g.business_assets[leader_corp] <= g.business_assets[second_corp] * Ext::from_f64(3.0) {
                g.tmp_ext2 = cast::<Ext, _>(second_corp);
                result = leader_corp;
            }
        }
    }
    result
}

/// `PB_Pow` (0x6bcabc): `base ^ exp`.
fn pow(base: Ext, exp: Ext) -> Ext {
    base.powf(exp)
}

/// Port of CalcRequiredAnnualReturn (0x588c72).
/// The compound annual return (percent) that turns the starting net worth (`StartNetWorth * 0.1 *
/// ExchangeRate`, rounded) into `portfolio_value * 0.1` over `num_years`, found by multiplicative search
/// from 6% (1,500 coarse then fine steps, at most 2,000). 0 when the value did not grow by more than 5%
/// or `num_years < 2`; -1 for a rate above 1,000%.
pub fn calc_required_annual_return(g: &Globals, portfolio_value: Ext, num_years: i32) -> Ext {
    let target = portfolio_value * sf(0.1f32);
    let start = to_long(sf(0.1f32) * cast::<Ext, _>(g.start_net_worth) * g.exchange_rate);
    let mut result = Ext::from_f64(0.0);
    if (cast::<Ext, _>(start) * sf(1.05f32)) < target {
        let years = num_years;
        if years >= 2 {
            let mut rate_pct = Ext::from_f64(6.0f64);
            let mut iter_count = 0i32;
            loop {
                let f4 = target / pow(rate_pct * sf(0.01f32) + Ext::from_f64(1.0), cast::<Ext, _>(years));
                iter_count += 1;
                if iter_count > 2000 {
                    result = rate_pct;
                    break;
                }
                let fine = if iter_count <= 1500 {
                    if !(f4 >= cast::<Ext, _>(start) * sf(0.95f32)) {
                        rate_pct = rate_pct * sf(0.96f32);
                        continue;
                    }
                    if f4 > cast::<Ext, _>(start) * sf(1.05f32) {
                        rate_pct = rate_pct * sf(1.04f32);
                        continue;
                    }
                    if !(f4 >= cast::<Ext, _>(start) * sf(0.99f32)) {
                        rate_pct = rate_pct * sf(0.992f32);
                        continue;
                    }
                    if f4 > cast::<Ext, _>(start) * sf(1.01f32) {
                        rate_pct = rate_pct * sf(1.008f32);
                        continue;
                    }
                    true
                } else {
                    true
                };
                if fine {
                    if !(f4 >= cast::<Ext, _>(start) * sf(0.995f32)) {
                        rate_pct = rate_pct * sf(0.999f32);
                        continue;
                    }
                    if f4 > cast::<Ext, _>(start) * sf(1.005f32) {
                        rate_pct = rate_pct * sf(1.001f32);
                        continue;
                    }
                    if rate_pct > Ext::from_f64(1000.0) {
                        rate_pct = -Ext::from_f64(1.0);
                    }
                    result = rate_pct;
                    break;
                }
            }
        }
    }
    result
}
