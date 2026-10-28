//! Company reports: the financial profile `ShowBalanceSheet` (0x430898, GOSUB `ShowBalanceSheet_FmtNum`
//! 0x439750) and the latest earnings report `ShowEarningsReport` (0x462f1e, GOSUBs
//! `ShowEarningsReport_BondBuyback` 0x46bbc3 and `ShowEarningsReport_GetMiscField` 0x46bea1), plus the
//! routines they run first: `CheckCrossOwnership` (0x481079) with `DivestStock` (0x511e37),
//! `ConsolidatePhysCommodRecs` (0x665a58), `UpdateAntitrustReserves` (0x5469ca), `GetEntitySoleCommodity`
//! (0x6358eb), `AssignEtfAdviser` (0x64cdb6) and `CalcBondCallPricePct` (0x4f8ed2).
//!
//! Numeric conventions as in `logic::finance`: `EXT` uses software x87 arithmetic, SINGLE stores are rounded with `as f32`,
//! SINGLE pool constants (`fmul dword [0x75e…]`) are written `s(0.01)`, every float → integer store uses
//! `to_long` / `to_quad`, CURRENCY locals are [`Cur`].

// Statements keep the original BASIC shape on purpose; negated float comparisons (`!(a > b)`) mirror the
// x87 branch taken for unordered (NaN) operands.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::too_many_lines,
    clippy::neg_multiply,
    clippy::needless_late_init
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::earnings::{
    balance_cash_t_bills, calc_adviser_fee_income, calc_ceo_pay, calc_cur_month_of_year, calc_day_of_month,
    calc_projected_earnings, get_funds_advised_by, get_next_earnings_date_str,
};
use crate::logic::finance::{
    calc_capital_taxes, calc_entity_financials, get_bank_cd_rate, get_controller_type, get_industry_category,
};
use crate::logic::holdings::{find_holding_slot, find_stock_holding, pack_holding_arrays_with_news as pack_holding_arrays};
use crate::logic::news::{is_in_recent_headlines, post_news};
use crate::logic::report::{describe_defunct_entity, show_quick_message, show_text_report};
use crate::logic::swaps::{get_sole_swap_counterparty, sum_swap_notional, swap_pnl_accumulator};
use crate::logic::valuation::{calc_dividend_yield, calc_stock_price, price_pct_change_months, update_price_history_tick};
use crate::pb::numeric::format;
use crate::pb::numeric::{format_num, str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{
    instr, ltrim, mcase, mid, parse_any, remove, replace, right, rtrim, string_fill, CaseMode,
};
use crate::pb::numeric::{ceil, frac, to_integer, to_long, to_quad, Cur};
use crate::state::Globals;
use crate::ui::Ui;

/// SINGLE constant of the original's pool.
#[inline]
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

fn t(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `FORMAT$(x, mask)`.
fn fmt(x: Ext, mask: &[u8]) -> PbStr {
    PbStr(format(x, mask))
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
fn fmt_trim(x: Ext, mask: &[u8]) -> PbStr {
    trim(&fmt(x, mask))
}

/// `STR$(n)` of a LONG / QUAD / EXT value (15 digits).
fn str15(x: Ext) -> PbStr {
    PbStr::from(str_num(x, 15).as_str())
}

/// `RTRIM$(LTRIM$(STR$(n)))`.
fn str_trim(x: Ext) -> PbStr {
    trim(&str15(x))
}

/// `RIGHT$("000" + RTRIM$(LTRIM$(STR$(id))), 4)`: the 4-digit id of a `@C` link code.
fn link_code(id: i32) -> PbStr {
    let x = t("000") + &str_trim(cast::<Ext, _>(id));
    right(x.as_bytes(), 4)
}

/// `PARSE$(text, ANY "|", n)`.
fn field(text: &PbStr, n: i32) -> PbStr {
    parse_any(text.as_bytes(), b"|", n)
}

/// A local `DIM lines(1 TO hi) AS STRING`. Writing outside is error 9 in the original, whose ON ERROR
/// handler shows `ShowRuntimeError` and leaves the routine; [`Lines::set`] returns `Err` so the port
/// leaves at the same point.
struct Lines {
    v: Vec<PbStr>,
}

struct OutOfRange;

impl Lines {
    fn new(hi: i32) -> Self {
        Lines { v: vec![PbStr::new(); hi as usize + 1] }
    }
    fn get(&self, i: i32) -> Result<PbStr, OutOfRange> {
        if i < 1 || cast::<usize, _>(i) >= self.v.len() { Err(OutOfRange) } else { Ok(self.v[cast::<usize, _>(i)].clone()) }
    }
    fn set(&mut self, i: i32, x: PbStr) -> Result<(), OutOfRange> {
        if i < 1 || cast::<usize, _>(i) >= self.v.len() {
            return Err(OutOfRange);
        }
        self.v[cast::<usize, _>(i)] = x;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------
// Small helpers called by the reports
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcBondCallPricePct (0x4f8ed2).
/// Call price (% of par) of a bond issue `years_to_maturity` years before maturity: 100 in the last two
/// years (0 or 1), `99 + years` for 2..5, else 105.
pub fn calc_bond_call_price_pct(years_to_maturity: i32) -> i32 {
    let y = cast::<Ext, _>(years_to_maturity);
    if y == Ext::from_f64(0.0) || y == Ext::from_f64(1.0) {
        100
    } else if y < Ext::from_f64(2.0) || Ext::from_f64(5.0) < y {
        105
    } else {
        years_to_maturity + 99
    }
}

/// Port of GetEntitySoleCommodity (0x6358eb).
/// The single commodity name (trimmed field 1) of all of `entity_id`'s records in `FuturesRecs`
/// (`list_kind = 1`) or `PhysCommodRec` (else), scanning records 1..1000; `"NO"` if it holds none or
/// more than one commodity.
pub fn get_entity_sole_commodity(g: &Globals, entity_id: i32, list_kind: i32) -> PbStr {
    let mut sole_commod = PbStr::new();
    for rec_idx in 1..=1000 {
        let rec = if list_kind == 1 { g.futures_recs[rec_idx].clone() } else { g.phys_commod_rec[rec_idx].clone() };
        if rec.is_empty() {
            continue;
        }
        if val(field(&rec, 2).as_bytes()) == cast::<Ext, _>(entity_id) {
            let commod = field(&rec, 1);
            if sole_commod.is_empty() {
                sole_commod = trim(&commod);
            } else if !(sole_commod == trim(&commod)) {
                sole_commod = t("NO");
                break;
            }
        }
    }
    if sole_commod.is_empty() {
        sole_commod = t("NO");
    }
    sole_commod
}

/// Port of ConsolidatePhysCommodRecs (0x665a58) with its GOSUB ConsolidatePhysCommod_Merge (0x665d09).
/// Merges `entity_id`'s physical-commodity records of the same commodity (`name|holder|units|cost`):
/// for each record the next later record of the same name is added into it (units as QUAD, cost as EXT,
/// both rewritten with `STR$`) and cleared. Two passes over records 1..1000; entities < 131 are skipped.
pub fn consolidate_phys_commod_recs(g: &mut Globals, entity_id: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let ent_id = entity_id;
    if ent_id < 131 {
        return;
    }
    let mut pass = 0;
    loop {
        for rec_idx in 1..=1000 {
            let mut merged = 0;
            let rec_text = g.phys_commod_rec[rec_idx].clone();
            if rec_text.is_empty() {
                continue;
            }
            if val(field(&rec_text, 2).as_bytes()) != cast::<Ext, _>(ent_id) {
                continue;
            }
            let commod_field = trim(&field(&rec_text, 1)) + "|";
            let mut units_field = trim(&field(&rec_text, 3)) + "|";
            let mut cost_field = trim(&field(&rec_text, 4));
            // GOSUB ConsolidatePhysCommod_Merge
            let mut other_idx = rec_idx + 1;
            while other_idx <= 1000 {
                let other_rec = g.phys_commod_rec[other_idx].clone();
                if !other_rec.is_empty()
                    && val(field(&other_rec, 2).as_bytes()) == cast::<Ext, _>(ent_id)
                    && commod_field == trim(&field(&other_rec, 1)) + "|"
                {
                    let sum_units = to_quad(val(units_field.as_bytes()) + val(field(&other_rec, 3).as_bytes()));
                    units_field = str_trim(cast::<Ext, _>(sum_units)) + "|";
                    let sum_cost = val(cost_field.as_bytes()) + val(field(&other_rec, 4).as_bytes());
                    cost_field = str_trim(sum_cost);
                    g.phys_commod_rec[other_idx] = PbStr::new();
                    merged = 1;
                    break;
                }
                other_idx += 1;
            }
            if merged != 0 {
                g.phys_commod_rec[rec_idx] = commod_field + &str_trim(cast::<Ext, _>(ent_id)) + "|" + &units_field + &cost_field;
            }
        }
        pass += 1;
        if pass != 1 {
            break;
        }
    }
}

/// Port of UpdateAntitrustReserves (0x5469ca) with its GOSUB UpdateAntitrustReserves_Corp (0x546b54).
/// Sets up the contingency reserve of company `corp_id_arg` (0 = all active operating companies
/// 131..1600) against pending antitrust damages (`AntitrustDefendant` / `AntitrustDamages` and the open
/// `LawsuitRec` suits): the needed after-tax reserve is a market-share-dependent fraction of the damages;
/// a shortfall is either booked at once (`ContingencyReserve` up, `EquityIncomeYTD` down) or, for
/// companies not yet reported this quarter, left in `ReserveAdjPending`. Without damages, a
/// corp-controlled company releases 10% of a reserve above `ExchangeRate * 100` via `ReserveAdjPending`.
/// May set `NewsMsg` ("… [runtime resource wording] ").
///
/// Returns the FUNCTION result (QUAD): for a single company its total damages claimed, else 0.
pub fn update_antitrust_reserves(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id_arg: i32) -> i64 {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut result: i64 = 0;
    if g.private_antitrust_off == 1 {
        return result;
    }
    let corp_id = corp_id_arg;
    if corp_id != 0 {
        if corp_id < 130 || 1600 < corp_id {
            return result;
        }
        if g.entity_status[corp_id] == 1 && g.industry_num[corp_id] <= 69 {
            result = antitrust_reserve_corp(g, ui, rng, corp_id);
        }
        return result;
    }
    for corp_idx in 131..=1600 {
        if g.entity_status[corp_idx] == 1 && g.industry_num[corp_idx] <= 69 {
            antitrust_reserve_corp(g, ui, rng, corp_idx);
        }
    }
    result
}

/// Port of UpdateAntitrustReserves_Corp (0x546b54), GOSUB of [`update_antitrust_reserves`]. Returns the
/// caller's QUAD `damagesSum`.
fn antitrust_reserve_corp(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) -> i64 {
    let __content = crate::content::current();
    let mut reserve_needed: Ext = Ext::from_f64(0.0);
    let mut damages_sum: i64 = 0;
    // fild share; fmul dword c; fild damages; fmulp
    let part = |damages: i64, share: Ext| -> Ext {
        let c = if share < Ext::from_f64(50.0) {
            s(0.0015f32)
        } else if share < Ext::from_f64(60.0) {
            s(0.0025f32)
        } else {
            s(0.0035f32)
        };
        cast::<Ext, _>(damages) * (share * c)
    };
    if corp_id == g.antitrust_defendant {
        damages_sum = g.antitrust_damages;
        reserve_needed = part(damages_sum, cast::<Ext, _>(g.antitrust_market_share));
    }
    for slot in 1..=10 {
        if mid(g.lawsuit_rec[slot].as_bytes(), 1, 1) == "1" {
            let suit_rec = g.lawsuit_rec[slot].clone();
            if val(field(&suit_rec, 2).as_bytes()) == cast::<Ext, _>(corp_id) {
                let suit_damages = to_quad(val(field(&suit_rec, 5).as_bytes()));
                damages_sum = to_quad(cast::<Ext, _>(damages_sum) + cast::<Ext, _>(suit_damages));
                let suit_share = to_integer(val(field(&suit_rec, 4).as_bytes()));
                // Quirk: the running damages total (not this suit's damages) is weighted.
                reserve_needed = reserve_needed + part(damages_sum, cast::<Ext, _>(suit_share));
            }
        }
    }
    if reserve_needed == Ext::from_f64(0.0) && 10 < g.corp_controller[corp_id] {
        if g.exchange_rate * Ext::from_f64(100.0) < g.contingency_reserve[corp_id] {
            if g.reserve_adj_pending[corp_id] == Ext::from_f64(0.0) {
                g.reserve_adj_pending[corp_id] = s(-0.1f32) * g.contingency_reserve[corp_id];
            }
            return damages_sum;
        }
    }
    reserve_needed = reserve_needed * (s(0.01f32) * cast::<Ext, _>(100 - g.corp_tax_rate));
    if reserve_needed < g.exchange_rate * Ext::from_f64(10.0) {
        return damages_sum;
    }
    if reserve_needed * s(0.95f32) < g.contingency_reserve[corp_id] {
        return damages_sum;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if !(g.net_worth >= g.exchange_rate * Ext::from_f64(0.0)) {
        return damages_sum;
    }
    let mut reserve_add = reserve_needed - g.contingency_reserve[corp_id];
    if g.net_worth * s(0.95f32) < reserve_add {
        reserve_add = g.net_worth * s(0.95f32);
    }
    if !(reserve_add >= g.exchange_rate * Ext::from_f64(10.0)) {
        return damages_sum;
    }
    let mut booked = reserve_add;
    if booked <= Ext::from_f64(50.0) {
        return damages_sum;
    }
    let mut deferred = false;
    if g.antitrust_defendant != corp_id && g.sim_co_count < corp_id && corp_id - g.sim_co_count < 300 {
        if g.reserve_adj_pending[corp_id] == Ext::from_f64(0.0) {
            g.reserve_adj_pending[corp_id] = booked;
            booked = Ext::from_f64(0.0);
            deferred = true;
        }
    }
    if !deferred {
        g.equity_income_ytd[corp_id] = g.equity_income_ytd[corp_id] - booked;
        g.contingency_reserve[corp_id] = g.contingency_reserve[corp_id] + booked;
    }
    calc_projected_earnings(g, ui, rng, corp_id);
    if is_in_recent_headlines(g, __content.bytes(8066)) == 0 && g.exchange_rate * Ext::from_f64(100.0) < booked {
        g.news_msg = g.ent_symbol[corp_id].clone() + __content.string(8067);
    }
    damages_sum
}

/// Port of AssignEtfAdviser (0x64cdb6) with its GOSUBs AssignEtfAdviser_PickInvestCo (0x64d1bf),
/// AssignEtfAdviser_PickInsurer (0x64d2f5) and AssignEtfAdviser_CheckUnique (0x64d42c).
/// Gives fund `etf_id` a new adviser and fee: `FundFeePct` = `RND(2, 10)` tenths of a percent (`RND(2, 4)`
/// when a corp adviser left voluntarily; 0.2 for funds 1581..1585); a random investment company
/// (industry 37, from 131 on) or insurer (86..130) not already advising another fund and rated 7+ (any
/// rating at the very start) is picked. Dismissal / hiring news when a corp-controlled `old_adviser`
/// was replaced; `CtrlSinceYear` / `FundBonusYear` reset when an adviser was replaced or `voluntary`.
pub fn assign_etf_adviser(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32, old_adviser: i32, voluntary: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let fund_id = etf_id;
    let prev_adviser = old_adviser;
    let vol = voluntary;
    let mut new_adviser = 0;
    g.fund_adviser[fund_id] = 0;
    let mut fee_tenths = rng.rnd_range(10, 2);
    if vol == 1 && 130 < prev_adviser {
        fee_tenths = rng.rnd_range(4, 2);
    }
    g.fund_fee_pct[fund_id] = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(fee_tenths) * s(0.1f32)));
    if fund_id < 1586 {
        g.fund_fee_pct[fund_id] = cast::<f32, _>(0.2f32);
    }
    if fee_tenths < 6 {
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(4.0) && vol == 0 {
            etf_pick_adviser(g, rng, 131, 1600, 37, 130, prev_adviser, &mut new_adviser);
        }
    } else {
        etf_pick_adviser(g, rng, 131, 1600, 37, 130, prev_adviser, &mut new_adviser);
    }
    if new_adviser == 0 {
        etf_pick_adviser(g, rng, 86, 130, 2, 85, prev_adviser, &mut new_adviser);
    }
    if new_adviser == 0 {
        etf_pick_adviser(g, rng, 131, 1600, 37, 130, prev_adviser, &mut new_adviser);
    }
    if 85 < new_adviser {
        g.fund_adviser[fund_id] = new_adviser;
    }
    if 85 < prev_adviser && g.corp_controller[prev_adviser] < 11 {
        g.news_msg = t(__content.string(8068)) + &g.ent_symbol[fund_id] + " fire " + &g.ent_symbol[prev_adviser] + __content.string(8069);
        if instr(1, g.news_headline[60].as_bytes(), __content.bytes(8070)) == 0 {
            post_news(g, ui, prev_adviser, fund_id);
        }
        if 85 < new_adviser {
            g.news_msg = g.ent_symbol[fund_id].clone()
                + " hires "
                + &g.ent_name[new_adviser]
                + " ("
                + &g.ent_symbol[new_adviser]
                + __content.string(8071);
            post_news(g, ui, new_adviser, fund_id);
        }
    }
    // (the BYREF parameters are read again here)
    if old_adviser < 1 && voluntary < 1 {
        return;
    }
    g.ctrl_since_year[fund_id] = g.cur_year;
    g.fund_bonus_year[fund_id] = 0;
}

/// AssignEtfAdviser_PickInvestCo (0x64d1bf) / AssignEtfAdviser_PickInsurer (0x64d2f5): scan `lo..=hi`
/// for an active company of `industry`, each taken with probability 1/4 (`RND * 4 <= 1`, drawn for every
/// candidate of the industry), not the previous adviser, rated 7+ unless it is the first quarter of the
/// game, and not advising another fund (AssignEtfAdviser_CheckUnique 0x64d42c); stops once
/// `new_adviser > min_ok`.
#[allow(clippy::too_many_arguments)]
fn etf_pick_adviser(
    g: &Globals,
    rng: &mut Rng,
    lo: i32,
    hi: i32,
    industry: i16,
    min_ok: i32,
    prev_adviser: i32,
    new_adviser: &mut i32,
) {
    let mut candidate_id = lo;
    loop {
        if g.entity_status[candidate_id] == 1 && g.industry_num[candidate_id] == industry && rng.rnd() * Ext::from_f64(4.0) <= Ext::from_f64(1.0) {
            if prev_adviser != candidate_id {
                let rated_ok = !((g.start_year != g.cur_year || g.cur_quarter != 1) && g.credit_rating_num[candidate_id] < 7);
                if rated_ok {
                    *new_adviser = candidate_id;
                    // GOSUB AssignEtfAdviser_CheckUnique
                    for etf_idx in 1581..=1600 {
                        if *new_adviser == g.fund_adviser[etf_idx] {
                            *new_adviser = 0;
                            break;
                        }
                    }
                    if min_ok < *new_adviser {
                        return;
                    }
                }
            }
        }
        candidate_id += 1;
        if hi < candidate_id {
            return;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Cross ownership
// ---------------------------------------------------------------------------------------------------------

/// Error 9 inside CheckCrossOwnership (a short holding's negative slot used as an index): the ON ERROR
/// handler shows `ShowRuntimeError` and leaves the routine.
struct CrossOwnError;

/// Locals of CheckCrossOwnership shared with its GOSUBs.
struct CrossOwn {
    /// FUNCTION result: 1 once a holding was divested.
    result: i32,
    node_id: i32,
    holder_id: i32,
    walk_id: i32,
}

/// Port of CheckCrossOwnership (0x481079) with its GOSUBs CrossOwn_BreakIllegalChain (0x4820fe),
/// CrossOwn_FindLargestGroupHolder (0x4822db) and CrossOwn_Exit (0x482520).
/// Enforces the securities rules on interlocking stock ownership of active company `corp_id` (only if it
/// holds stock):
///
/// 1. Mutual holdings: a company holding over 10% of `corp_id` while `corp_id` holds over 10% of it
///    (each side either 20%+ or under the same controller) — the smaller stake is divested
///    ([`divest_stock`]), with the news "Cross-ownership … was eliminated".
/// 2. Unless `force_check` (or with probability 1/3 per call, one `RND`): with `PublicPct <= 80` and
///    `CorpPctNotCorpHeld <= 90`, the chain of largest holders (of the same controller group) is walked
///    up to 30 steps; a loop back to `corp_id` (or 30 steps) breaks the chain, and the holdings
///    of `corp_id` are searched (6 levels, stakes > 10% within the controller group) for a circle back to
///    `corp_id`. A circle is broken up by divesting (CrossOwn_BreakIllegalChain, with popups).
///
/// Returns 1 if a holding was divested, else 0.
pub fn check_cross_ownership(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, force_check: i32) -> i32 {
    let mut f = CrossOwn { result: 0, node_id: 0, holder_id: 0, walk_id: 0 };
    let _ = cross_ownership(g, ui, rng, corp_id, force_check, &mut f);
    f.result
}

fn cross_ownership(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    force_check: i32,
    f: &mut CrossOwn,
) -> Result<(), CrossOwnError> {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError, exit (see CrossOwnError)
    if corp_id < 11 {
        return Ok(());
    }
    if g.entity_status[corp_id] != 1 || g.hold_stock_id[(0, corp_id)] == 0 {
        return Ok(());
    }
    f.node_id = corp_id;
    for other_id in 11..=1600 {
        if g.entity_status[other_id] != 1 || f.node_id == other_id {
            continue;
        }
        if find_stock_holding(g, other_id, f.node_id) == 0 {
            continue;
        }
        let slot = find_holding_slot(g, other_id, f.node_id);
        let other_pct = pct_at(g, slot, other_id)?;
        if other_pct < 20 && g.corp_controller[f.node_id] != g.corp_controller[other_id] {
            continue;
        }
        f.holder_id = other_id;
        let slot = find_stock_holding(g, f.node_id, f.holder_id);
        if slot == 0 {
            continue;
        }
        let reverse_pct = pct_at(g, slot, f.node_id)?;
        if reverse_pct < 20 && g.corp_controller[f.node_id] != g.corp_controller[other_id] {
            continue;
        }
        if 10 < other_pct && 10 < reverse_pct {
            if reverse_pct <= other_pct {
                divest_stock(g, ui, rng, f.holder_id, f.node_id, 0);
            } else {
                divest_stock(g, ui, rng, f.node_id, f.holder_id, 0);
            }
            f.result = 1;
            if find_stock_holding(g, f.node_id, f.holder_id) == 0 || find_stock_holding(g, f.holder_id, f.node_id) == 0 {
                g.news_msg = t(__content.string(8072))
                    + &g.ent_symbol[f.holder_id]
                    + " and "
                    + &g.ent_symbol[f.node_id]
                    + __content.string(8073);
                post_news(g, ui, f.holder_id, f.node_id);
                update_corp_control(g, ui, rng, f.node_id);
                update_corp_control(g, ui, rng, f.holder_id);
                return Ok(());
            }
        }
    }
    if Ext::from_f64(2.0) < Ext::from_f64(3.0) * rng.rnd() && force_check == 0 {
        return Ok(());
    }
    update_corp_control(g, ui, rng, f.node_id);
    if 80 < g.public_pct {
        return Ok(());
    }
    if 90 < g.corp_pct_not_corp_held[f.node_id] {
        return Ok(());
    }
    if g.largest_holder < 11 {
        f.walk_id = corp_id;
        cross_own_find_largest_group_holder(g, corp_id, f)?;
        if g.largest_holder == 0 {
            return Ok(());
        }
    }
    f.walk_id = cast::<i32, _>(g.largest_holder);
    f.holder_id = f.walk_id;
    let mut iter_count = 0;
    while 0 < f.walk_id {
        iter_count += 1;
        if corp_id == f.walk_id {
            f.walk_id = -1;
            f.node_id = corp_id;
        } else {
            if iter_count == 30 {
                f.node_id = f.walk_id;
            }
            update_corp_control(g, ui, rng, f.walk_id);
            if g.largest_holder < 11 {
                cross_own_find_largest_group_holder(g, corp_id, f)?;
            } else if g.corp_controller[corp_id] != g.corp_controller[cast::<i32, _>(g.largest_holder)] {
                cross_own_find_largest_group_holder(g, corp_id, f)?;
            }
            f.walk_id = cast::<i32, _>(g.largest_holder);
            if f.walk_id < 11 {
                f.walk_id = -2;
                break;
            }
            if iter_count == 30 {
                f.walk_id = -1;
                break;
            }
        }
        if 29 < iter_count {
            f.walk_id = -2;
        }
    }
    if f.walk_id == -1 {
        cross_own_break_illegal_chain(g, ui, rng, f);
        return Ok(());
    }
    // Search corp_id's holdings for a circle back to corp_id (6 levels).
    f.holder_id = corp_id;
    let count = cast::<i32, _>(g.hold_stock_id[(0, corp_id)]);
    for slot in 1..=count {
        if let Some(holder) = chain_search(g, corp_id, corp_id, slot, 1) {
            f.node_id = corp_id;
            f.holder_id = holder;
            cross_own_break_illegal_chain(g, ui, rng, f);
            return Ok(());
        }
    }
    Ok(())
}

/// `HoldStockPct(slot, holder)` as read by CheckCrossOwnership; a negative slot (short holding from
/// FindStockHolding) is error 9.
fn pct_at(g: &Globals, slot: i32, holder: i32) -> Result<i32, CrossOwnError> {
    if slot < 0 || 15 < slot {
        return Err(CrossOwnError);
    }
    Ok(cast::<i32, _>(g.hold_stock_pct[(slot, holder)]))
}

/// One level of CheckCrossOwnership's nested holding search: holding `slot` of `chain` (level `depth`,
/// 1 = `corp_id` itself). A held company of the same controller as `corp_id`, held over 10% and itself
/// holding stock, is `corp_id` (levels 2..6: returns `Some(chain)`, the holder to divest from) or is
/// searched one level deeper (up to level 6).
fn chain_search(g: &Globals, corp_id: i32, chain: i32, slot: i32, depth: i32) -> Option<i32> {
    let node_id = cast::<i32, _>(g.hold_stock_id[(slot, chain)]);
    if node_id <= 10 {
        return None;
    }
    if g.corp_controller[corp_id] != g.corp_controller[node_id] || g.hold_stock_pct[(slot, chain)] <= 10 {
        return None;
    }
    if g.hold_stock_id[(0, node_id)] == 0 {
        return None;
    }
    if 1 < depth && corp_id == node_id {
        return Some(chain);
    }
    if depth == 6 {
        return None;
    }
    let count = cast::<i32, _>(g.hold_stock_id[(0, node_id)]);
    for k in 1..=count {
        if let Some(h) = chain_search(g, corp_id, node_id, k, depth + 1) {
            return Some(h);
        }
    }
    None
}

/// Port of CrossOwn_FindLargestGroupHolder (0x4822db), GOSUB of [`check_cross_ownership`]:
/// `LargestHolder` = the active company (11..1600) of `corp_id`'s controller holding the most (over 10%)
/// of `walkId`, else 0.
fn cross_own_find_largest_group_holder(g: &mut Globals, corp_id: i32, f: &CrossOwn) -> Result<(), CrossOwnError> {
    let mut best_pct: i16 = 0;
    let mut best_holder_id: i16 = 0;
    g.largest_holder = 0;
    for scan_id in 11..=1600 {
        if g.entity_status[scan_id] != 1 {
            continue;
        }
        if g.corp_controller[corp_id] != g.corp_controller[scan_id] {
            continue;
        }
        if g.hold_stock_id[(0, scan_id)] == 0 {
            continue;
        }
        let slot = find_stock_holding(g, scan_id, f.walk_id);
        if slot == 0 {
            continue;
        }
        let pct = cast::<i16, _>(pct_at(g, slot, scan_id)?);
        if 10 < pct && best_pct < pct {
            best_pct = pct;
            best_holder_id = cast::<i16, _>(scan_id);
        }
    }
    if 10 < best_holder_id {
        g.largest_holder = best_holder_id;
    }
    Ok(())
}

/// Port of CrossOwn_BreakIllegalChain (0x4820fe), GOSUB of [`check_cross_ownership`]: `holderId`
/// divests its `nodeId` stock (popup " [runtime resource wording] …"; a second popup when the player
/// is concerned and the holding is gone), then control is updated for both.
fn cross_own_break_illegal_chain(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut CrossOwn) {
    let __content = crate::content::current();
    let divest_corp_id = f.node_id;
    let divest_holder_id = f.holder_id;
    if divest_corp_id < 11 || divest_holder_id < 11 {
        return;
    }
    let mut msg = t(__content.string(8074))
        + &g.ent_name[f.holder_id]
        + __content.string(8075)
        + __content.string(8076);
    show_quick_message(g, ui, &mut msg);
    divest_stock(g, ui, rng, divest_corp_id, divest_holder_id, 0);
    if g.sel_entity == divest_holder_id
        || g.sel_entity == divest_corp_id
        || g.cur_player == g.corp_controller[divest_holder_id]
    {
        if find_stock_holding(g, divest_holder_id, divest_corp_id) == 0 {
            let mut msg = t(__content.string(8077))
                + &g.ent_name[divest_corp_id]
                + " by "
                + &g.ent_name[divest_holder_id]
                + __content.string(8078);
            show_quick_message(g, ui, &mut msg);
        }
    }
    f.result = 1;
    update_corp_control(g, ui, rng, divest_holder_id);
    update_corp_control(g, ui, rng, f.holder_id);
}

/// Port of DivestStock (0x511e37).
/// Forced sale of `holder_id`'s (long) holding of `stock_id` (`reason`: 0 government decree,
/// 1 antitrust, 2 silent prefix, 3 no popup and no buyer): the stock is re-valued (`ValuationMode = 2`,
/// sentiment down by the stake, cap -10% unless funds are involved), the seller gets
/// `pct × MarketCap` in cash (capital gain booked to `PlayerCapGains` / `ExtraordinaryPL`), news
/// "… divests stock of …" (popup for reasons 0..2), and — except for reason 3 — a random
/// corp-controlled company (from `86 + INT(RND * 1500)` up) that can afford it buys the stake (borrowing
/// a cash shortfall from its bank), news "… acquires … [runtime resource wording] …". Ends with
/// `PackHoldingArrays` and `UpdateCorpControl(stock)`.
pub fn divest_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, stock_id: i32, holder_id: i32, reason: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if stock_id < 11 || holder_id < 1 {
        return;
    }
    if g.entity_status[stock_id] != 1 || g.entity_status[holder_id] != 1 {
        return;
    }
    let seller_id = holder_id;
    let mut found = 0;
    let mut pct_held = 0;
    let mut cost_basis = Ext::from_f64(0.0);
    for slot in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == stock_id && 0 < g.hold_stock_pct[(slot, holder_id)] {
            found = 1;
            g.hold_stock_id[(slot, holder_id)] = 0;
            pct_held = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
            g.hold_stock_pct[(slot, holder_id)] = 0;
            cost_basis = g.hold_stock_basis[(slot, holder_id)];
            g.hold_stock_basis[(slot, holder_id)] = Ext::from_f64(0.0);
        }
    }
    if found == 0 {
        return;
    }
    g.valuation_mode = 2;
    calc_stock_price(g, ui, rng, stock_id, 0);
    g.investor_sentiment[stock_id] = g.investor_sentiment[stock_id].wrapping_sub(pct_held);
    if 10 < seller_id {
        calc_projected_earnings(g, ui, rng, seller_id);
    }
    if g.industry_num[stock_id] != 71 && 10 < seller_id && g.industry_num[seller_id] != 71 {
        g.market_cap[stock_id] = s(0.9f32) * g.market_cap[stock_id];
    }
    let sale_value = cast::<Ext, _>(pct_held) * g.market_cap[stock_id];
    update_price_history_tick(g, ui, rng, stock_id);
    g.cash[seller_id] = sale_value + g.cash[seller_id];
    if seller_id < 11 {
        g.player_cap_gains[seller_id] = (sale_value - cost_basis) + g.player_cap_gains[seller_id];
    } else {
        g.extraordinary_pl[seller_id] = to_quad((sale_value - cost_basis) + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
    }
    g.news_msg = g.ent_symbol[seller_id].clone()
        + __content.string(8079)
        + &g.ent_name[stock_id]
        + " ("
        + &g.ent_symbol[stock_id]
        + ") for "
        + &trim(&g.currency_symbol)
        + &fmt_trim(sale_value, b"#############,")
        + " "
        + &g.million_prefix_lc
        + "il.";
    let mut reason_prefix = t(__content.string(8080));
    if reason == 1 {
        reason_prefix = t(__content.string(8081));
    }
    if 1 < reason {
        reason_prefix = PbStr::new();
    }
    if reason < 3 {
        let mut msg = t(__content.string(8082)) + &reason_prefix + &g.news_msg;
        show_quick_message(g, ui, &mut msg);
    }
    post_news(g, ui, 0, 0);
    let start_offset = to_long(Ext::from_f64(1500.0) * rng.rnd());
    let mut buyer_id = start_offset + 86;
    while buyer_id <= 1600 {
        if reason == 3 {
            break;
        }
        if g.entity_status[buyer_id] == 1
            && 11 <= g.corp_controller[buyer_id]
            && seller_id != buyer_id
            && g.industry_num[buyer_id] != 71
            && sale_value * Ext::from_f64(3.0) <= Ext::from_f64(100.0) * g.market_cap[buyer_id]
        {
            let buyer_slot = find_holding_slot(g, buyer_id, stock_id);
            if buyer_slot != 0 {
                calc_entity_financials(g, ui, rng, buyer_id);
                if g.liquid_funds >= sale_value {
                    update_corp_control(g, ui, rng, buyer_id);
                    if 11 <= g.corp_controller[buyer_id] {
                        g.hold_stock_id[(buyer_slot, buyer_id)] = cast::<i16, _>(stock_id);
                        g.hold_stock_pct[(buyer_slot, buyer_id)] =
                            cast::<i16, _>(pct_held + cast::<i32, _>(g.hold_stock_pct[(buyer_slot, buyer_id)]));
                        g.hold_stock_basis[(buyer_slot, buyer_id)] = sale_value + g.hold_stock_basis[(buyer_slot, buyer_id)];
                        g.cash[buyer_id] = g.cash[buyer_id] - sale_value;
                        if g.cash[buyer_id] + g.t_bills[buyer_id] < Ext::from_f64(0.0) {
                            balance_cash_t_bills(g, buyer_id);
                            let shortfall = g.cash[buyer_id].abs();
                            g.cash[buyer_id] = Ext::from_f64(0.0);
                            g.bank_debt[buyer_id] = shortfall + g.bank_debt[buyer_id];
                            let lender = g.lender_id;
                            g.cash[lender] = g.cash[lender] - shortfall;
                        }
                        g.news_msg = g.ent_symbol[buyer_id].clone()
                            + __content.string(2062)
                            + &g.ent_symbol[stock_id]
                            + __content.string(8083)
                            + &g.ent_symbol[seller_id];
                        post_news(g, ui, 0, 0);
                        buyer_id = 1600;
                    }
                }
            }
        }
        buyer_id += 1;
    }
    pack_holding_arrays(g, ui);
    update_corp_control(g, ui, rng, stock_id);
}

// ---------------------------------------------------------------------------------------------------------
// ShowBalanceSheet
// ---------------------------------------------------------------------------------------------------------

/// Port of ShowBalanceSheet_FmtNum (0x439750), GOSUB of [`show_balance_sheet`]:
/// `fmtText = RIGHT$(STRING$(19, " ") + RTRIM$(LTRIM$(FORMAT$(fmtValue, numFmt))), 19)`.
fn bs_fmt_num(num_fmt: &PbStr, fmt_value: Ext) -> PbStr {
    let x = string_fill(19, b" ") + &fmt_trim(fmt_value, num_fmt.as_bytes());
    right(x.as_bytes(), 19)
}

/// Port of ShowBalanceSheet (0x430898) with its GOSUB ShowBalanceSheet_FmtNum (0x439750).
/// The "FINANCIAL PROFILE" report of company `corp_ref` (11..1600, active; a liquidated one gets
/// [`describe_defunct_entity`]): balance sheet (assets by industry: banks, insurers, operating companies,
/// holding companies and funds), liabilities with footnote marks, stock and earnings data (net worth per
/// share, price, 52-week range, EPS history), ratios, credit info, variable settings, miscellaneous info
/// (control, CEO, fund manager, dates), taxes on capital and the footnotes (swaps, bonds, antitrust
/// litigation). Lines link to companies (`@C<id>`).
///
/// State effects before the report: `CheckCrossOwnership(corp, 1)` (unless the game has ended),
/// `ConsolidatePhysCommodRecs`, `UpdateAntitrustReserves`, `BalanceCashTBills`, [runtime resource wording]
/// is covered from the credit line (+5% if room) or, for operating companies, from working capital,
/// `PackHoldingArrays`, `CalcStockPrice` with `ValuationMode = 0`, `UpdatePriceHistoryTick`,
/// `UpdateCorpControl`, `AssignEtfAdviser` for a fund without a valid adviser/fee, `PlayerCeoSinceYear`,
/// `TmpSharePrice`, a negative `BondsPublicHeld` reset to 0, `TmpExt1 = 0`; a pending `NewsMsg` is
/// posted at the end (`PostNews(0, 0)`).
///
/// Runs `UpdateMarketDisplay` ([`crate::logic::market::update_market_display`]): mostly main-window
/// labels, but it also re-values a random fund and random companies (`RND`), extends the ticker text and
/// recomputes `StockIndex` / `PrevMarketIndex` (with market-move news).
/// UpdateDateDisplay (0x4a054a) and ShowNextEarningsDate (0x4a070a) only set main-window labels; their
/// state effects (the calendar helpers' `TmpExt1` / `CurQuarter`) are kept.
pub fn show_balance_sheet(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_ref: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error 9 paths leave the routine like the handler)
    let _ = balance_sheet(g, ui, rng, corp_ref);
}

fn balance_sheet(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_ref: i32) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
    let mut lines = Lines::new(1500);
    // (UI) DIALOG END DlgWhoOwnsWhat
    let mut num_fmt = t("* #################,");
    let fmt_decimal = t("* ##############.00");
    let dash_line = string_fill(67, b"-");
    if corp_ref < 1 || 1600 < corp_ref {
        return Err(OutOfRange); // EntityStatus(corpRef): error 9
    }
    if g.entity_status[corp_ref] < 1 {
        return Ok(());
    }
    if corp_ref < 11 {
        return Ok(());
    }
    if g.entity_status[corp_ref] == 2 {
        describe_defunct_entity(g, ui, corp_ref);
        return Ok(());
    }
    let c = corp_ref;
    if !(g.game_end_flag == "Y") {
        check_cross_ownership(g, ui, rng, c, 1);
    }
    consolidate_phys_commod_recs(g, c);
    update_antitrust_reserves(g, ui, rng, c);
    calc_entity_financials(g, ui, rng, c);
    let mut commod_label = get_entity_sole_commodity(g, c, 2);
    if commod_label == "NO" {
        commod_label = t(__content.string(8084));
    } else {
        if commod_label == "OIL" {
            commod_label = t(__content.string(5722));
        } else if commod_label == "ETHEREUM" {
            commod_label = t(__content.string(8085));
        } else {
            commod_label = t("(") + &commod_label + ")    ";
            commod_label = mid(commod_label.as_bytes(), 1, 9);
        }
        commod_label = t(__content.string(8086)) + &mcase(commod_label.as_bytes(), CaseMode::Ansi);
    }
    balance_cash_t_bills(g, c);
    if g.cash[c] < Ext::from_f64(0.0) && g.cash[c].abs() < g.unused_credit_line {
        let mut draw_amt = g.cash[c].abs();
        if draw_amt * s(1.05f32) < g.unused_credit_line {
            draw_amt = s(1.05f32) * draw_amt;
        }
        g.cash[c] = draw_amt + g.cash[c];
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] - draw_amt;
        g.bank_debt[c] = draw_amt + g.bank_debt[c];
    }
    if 130 < c && g.cash[c] < Ext::from_f64(0.0) && g.cash[c].abs() < g.working_capital[c] {
        g.working_capital[c] = g.cash[c] + g.working_capital[c];
        g.cash[c] = Ext::from_f64(0.0);
    }
    calc_entity_financials(g, ui, rng, c);
    if g.market_cap[c] < Ext::from_f64(1000.0) {
        num_fmt = fmt_decimal.clone();
    }
    let seconds = ui.local_time().time_secs();
    crate::logic::market::update_market_display(g, ui, rng, seconds); // TIME$ seconds: see calc_stock_price
    pack_holding_arrays(g, ui);
    calc_entity_financials(g, ui, rng, c);
    let bond_ytm = g.bond_ytm;
    let industry = g.industry_num[c];
    let mut qtrs_reported = g.cur_quarter;
    if g.sim_co_count < c {
        qtrs_reported = g.cur_quarter - 1;
    }
    let fmt_num = |x: Ext, nf: &PbStr| bs_fmt_num(nf, x);
    let mut has_links = 0;
    let mut stock_line = PbStr::new();
    let mut bond_line = PbStr::new();
    let mut footnote_no = 0;
    let mut bonds_past_due = 0;
    let mut lawsuit_damages: i64 = 0;
    let mut cust_deposits = Ext::from_f64(0.0);
    let mut roe_pct: Ext = Ext::from_f64(0.0);
    let mut ref_idx = 0;
    let mut bond_price = Cur(0);
    let mut line_no: i32;

    lines.set(1, t(__content.string(8087)) + &g.ent_name[c] + " (" + &g.ent_symbol[c] + "):")?;
    let work_text = lines.get(1)?;
    lines.set(2, string_fill(work_text.len(), b"-"))?;
    let mut field_text = g.currency_suffix.clone();
    if field_text.is_empty() || 0 < instr(1, field_text.as_bytes(), b"SwFr") {
        field_text = t(" ") + &g.currency_name;
    }
    if 1 <= instr(1, field_text.as_bytes(), b"Pound")
        || 0 < instr(1, field_text.as_bytes(), b"Dollar")
        || 0 < instr(1, field_text.as_bytes(), b"Franc")
    {
        if !(right(field_text.as_bytes(), 1) == "s") {
            field_text = field_text + "s";
        }
    }
    lines.set(3, t(__content.string(8088)) + &g.million_prefix_lc + __content.string(8089) + &field_text + __content.string(8090))?;
    lines.set(5, t("ASSETS:"))?;
    if industry < 3 && g.bond_unrealized_loss != Ext::from_f64(0.0) {
        lines.set(5, lines.get(5)? + __content.string(8091))?;
    }
    lines.set(7, t(__content.string(8092)) + &fmt_num(g.cash[c], &num_fmt))?;
    lines.set(8, t(__content.string(8093)) + &fmt_num(g.t_bills[c], &num_fmt))?;
    let mut fmt_text = fmt_num(g.tmp_subsid_stock_value, &num_fmt);
    lines.set(9, t(__content.string(8094)) + &fmt_text)?;
    if g.industry_num[c] == 71 {
        lines.set(9, replace(lines.get(9)?.as_bytes(), __content.bytes(8095), __content.bytes(8096)))?;
    }
    if g.hold_stock_id[(0, c)] == 1 {
        let held = cast::<i32, _>(g.hold_stock_id[(1, c)]);
        if g.ent_name[held].len() < 24 {
            lines.set(9, t("  ") + &g.ent_name[held] + " Stock" + &string_fill(32, b" "))?;
            lines.set(9, mid(lines.get(9)?.as_bytes(), 1, 31) + &fmt_text)?;
            has_links = 1;
            let code = link_code(cast::<i32, _>(g.hold_stock_id[(1, c)]));
            lines.set(9, lines.get(9)? + &string_fill(55, b" ") + "@C" + &code)?;
            stock_line = lines.get(9)?;
        }
    }
    if (industry == 1 || industry == 2) && g.hold_bond_id[(0, c)] == 1 {
        let held = g.hold_bond_id[(1, c)];
        if g.ent_name[held].len() < 24 {
            bond_line = t("  ") + &g.ent_name[held] + " Bonds" + &string_fill(32, b" ");
            fmt_text = fmt_num(g.tmp_corp_bonds_value, &num_fmt);
            bond_line = mid(bond_line.as_bytes(), 1, 31) + &fmt_text;
            has_links = 1;
            let code = link_code(g.hold_bond_id[(1, c)]);
            bond_line = bond_line + &string_fill(55, b" ") + "@C" + &code;
        }
    }
    lines.set(10, t(__content.string(8097)) + &fmt_num(g.options_net_value, &num_fmt))?;
    let mut skip_commod = false;
    if industry == 1 {
        lines.set(11, t(__content.string(8098)) + &fmt_num(g.tmp_gov_bonds_value, &num_fmt))?;
        lines.set(12, t(__content.string(8099)) + &fmt_num(g.tmp_corp_bonds_value, &num_fmt))?;
        if !bond_line.is_empty() {
            lines.set(12, bond_line.clone())?;
        }
        lines.set(13, t(__content.string(8100)) + &fmt_num(g.bank_loans_total, &num_fmt))?;
        lines.set(14, t(__content.string(8101)) + &fmt_num(cast::<Ext, _>(g.bank_consumer_loans[c]), &num_fmt))?;
        lines.set(
            15,
            t(__content.string(8102))
                + &fmt_num(cast::<Ext, _>(g.bank_mortgage_loans[c] + g.subprime_secs[c]), &num_fmt),
        )?;
        if c < 85 && Ext::from_f64(0.0) < g.bank_debt[c + 1] {
            field_text = g.ent_symbol[c + 1].clone() + "   ";
            field_text = mid(field_text.as_bytes(), 1, 4) + "       ";
            lines.set(16, t(__content.string(8103)) + &field_text + &fmt_num(g.bank_debt[c + 1], &num_fmt))?;
            has_links = 1;
            let code = link_code(1 + c);
            lines.set(16, lines.get(16)? + &string_fill(55, b" ") + "@C" + &code)?;
            lines.set(17, t(__content.string(8104)) + &fmt_num(g.bad_debt_reserve[c] * -Ext::from_f64(1.0), &num_fmt))?;
            line_no = 18;
            skip_commod = true; // goto LAB_0043247b (industry 1: the commodity block does not apply anyway)
        } else {
            lines.set(16, t(__content.string(8104)) + &fmt_num(g.bad_debt_reserve[c] * -Ext::from_f64(1.0), &num_fmt))?;
            line_no = 17;
        }
    } else if industry == 2 {
        lines.set(11, t(__content.string(8098)) + &fmt_num(g.tmp_gov_bonds_value, &num_fmt))?;
        lines.set(12, t(__content.string(8099)) + &fmt_num(g.tmp_corp_bonds_value, &num_fmt))?;
        if !bond_line.is_empty() {
            lines.set(12, bond_line.clone())?;
        }
        lines.set(13, t(__content.string(8105)) + &fmt_num(cast::<Ext, _>(g.subprime_secs[c]), &num_fmt))?;
        lines.set(14, t(__content.string(8106)) + &fmt_num(g.futures_net_value, &num_fmt))?;
        lines.set(15, t(__content.string(8107)) + &fmt_num(g.commod_margin_bal[c], &num_fmt))?;
        line_no = 16;
    } else if industry < 70 {
        lines.set(9, t(__content.string(8108)) + &fmt_num(g.working_capital[c], &num_fmt))?;
        lines.set(10, t(__content.string(8109)) + &fmt_num(g.business_assets[c], &num_fmt))?;
        lines.set(11, t(__content.string(8094)) + &fmt_num(g.tmp_subsid_stock_value, &num_fmt))?;
        if !stock_line.is_empty() {
            lines.set(11, stock_line.clone())?;
        }
        lines.set(12, t(__content.string(8097)) + &fmt_num(g.options_net_value, &num_fmt))?;
        lines.set(
            13,
            t(__content.string(8110)) + &fmt_num(g.futures_net_value + g.phys_commod_value, &num_fmt),
        )?;
        lines.set(14, t(__content.string(8107)) + &fmt_num(g.commod_margin_bal[c], &num_fmt))?;
        line_no = 15;
    } else {
        if c < 1581 || 1583 < c {
            lines.set(
                11,
                t(__content.string(8110)) + &fmt_num(g.futures_net_value + g.phys_commod_value, &num_fmt),
            )?;
            lines.set(12, t(__content.string(8107)) + &fmt_num(g.commod_margin_bal[c], &num_fmt))?;
        } else {
            lines.set(11, t(__content.string(8111)) + &fmt_num(g.tmp_gov_bonds_value, &num_fmt))?;
            lines.set(12, t(__content.string(8112)) + &fmt_num(g.tmp_corp_bonds_value, &num_fmt))?;
        }
        line_no = 13;
    }
    // LAB_0043247b
    let _ = skip_commod;
    if 2 < industry && !(industry == 71 && c < 1584) {
        if !(g.futures_net_value == Ext::from_f64(0.0) || g.phys_commod_value != Ext::from_f64(0.0)) {
            fmt_text = fmt_num(g.futures_net_value, &num_fmt);
            lines.set(line_no - 2, t(__content.string(8113)) + &fmt_text)?;
        } else if g.futures_net_value == Ext::from_f64(0.0) && Ext::from_f64(0.0) < g.phys_commod_value {
            fmt_text = fmt_num(g.phys_commod_value, &num_fmt);
            lines.set(line_no - 2, t(__content.string(8114)) + &commod_label + &fmt_text)?;
            if 0 < instr(1, commod_label.as_bytes(), __content.bytes(1272)) {
                lines.set(line_no - 2, t(__content.string(8115)) + &fmt_text)?;
            }
            if 0 < instr(1, commod_label.as_bytes(), b"Bitcoin") {
                lines.set(line_no - 2, t(__content.string(8116)) + &fmt_text)?;
            }
        }
        if g.futures_net_value == Ext::from_f64(0.0) && g.commod_margin_bal[c] == Ext::from_f64(0.0) && g.phys_commod_value == Ext::from_f64(0.0) {
            line_no = line_no - 2;
        }
    }
    // LAB_00432705
    if 130 < c && Ext::from_f64(0.0) < g.goodwill[c] {
        lines.set(line_no, t(__content.string(8117)) + &fmt_num(g.goodwill[c], &num_fmt))?;
        line_no = line_no + 1;
    }
    lines.set(line_no, t("                                  ----------------"))?;
    lines.set(line_no + 1, t(__content.string(8118)) + &fmt_num(g.total_assets, &num_fmt))?;
    lines.set(line_no + 2, t("                                  ----------------"))?;
    lines.set(line_no + 3, t(__content.string(8119)))?;
    field_text = right(g.bond_issue_info[c].as_bytes(), 5);
    if mid(field_text.as_bytes(), 1, 1) == "0" {
        field_text = right(field_text.as_bytes(), 4);
    }
    let mut aux_text = field_text.clone() + __content.string(8120) + &mid(g.bond_issue_info[c].as_bytes(), 1, 4) + "         ";
    if 0 < instr(1, g.bond_issue_info[c].as_bytes(), b"cv") {
        aux_text = replace(aux_text.as_bytes(), __content.bytes(8121), __content.bytes(8122));
    }
    aux_text = mid(aux_text.as_bytes(), 1, 29);
    let mut work_text;
    if g.bonds_issued[c] <= 0 {
        lines.set(line_no + 4, t(__content.string(8123)) + &fmt_num(cast::<Ext, _>(g.bonds_issued[c]), &num_fmt))?;
    } else {
        bond_price = g.bond_price[c];
        work_text = t(__content.string(8124)) + &fmt_trim(bond_price.get(), b"* ###.00") + ") [2]";
        fmt_text = fmt_num(cast::<Ext, _>(g.bonds_issued[c]), &num_fmt);
        lines.set(line_no + 4, t("  ") + &aux_text + &fmt_text + &work_text)?;
        footnote_no = 1;
    }
    let total_liabs;
    if industry == 1 {
        cust_deposits = g.bank_cust_deposits[c];
        if cust_deposits < Ext::from_f64(0.0) {
            cust_deposits = Ext::from_f64(0.0);
        }
        lines.set(
            line_no + 5,
            t(__content.string(8125)) + &fmt_num(cust_deposits + g.bank_demand_deposits[c], &num_fmt),
        )?;
        lines.set(line_no + 6, t(__content.string(8126)) + &fmt_num(g.bank_c_ds[c], &num_fmt))?;
        if g.currency_code == "USA" {
            lines.set(line_no + 7, t(__content.string(8127)) + &fmt_num(g.bank_debt[c], &num_fmt))?;
        } else {
            if c < 12 || g.bank_debt[c] <= Ext::from_f64(0.0) {
                field_text = t("s");
            } else {
                field_text = t(" from ") + &g.ent_symbol[c - 1];
            }
            field_text = field_text + &string_fill(9, b" ");
            field_text = mid(field_text.as_bytes(), 1, 10);
            lines.set(line_no + 7, t(__content.string(8128)) + &field_text + &fmt_num(g.bank_debt[c], &num_fmt))?;
            if 11 < c && Ext::from_f64(0.0) < g.bank_debt[c] {
                has_links = 1;
                let code = link_code(c - 1);
                lines.set(line_no + 7, lines.get(line_no + 7)? + &string_fill(55, b" ") + "@C" + &code)?;
            }
        }
        total_liabs = ((g.bank_demand_deposits[c] + g.bank_c_ds[c]) + cust_deposits) + g.tmp_total_debt;
        line_no = line_no + 8;
    } else if industry == 2 {
        lines.set(line_no + 5, t(__content.string(8129)) + &fmt_num(g.bank_debt[c], &num_fmt))?;
        lines.set(line_no + 6, t(__content.string(8130)) + &fmt_num(g.ins_reserves[c], &num_fmt))?;
        total_liabs = g.tmp_total_debt + g.ins_reserves[c];
        line_no = line_no + 7;
    } else {
        lines.set(line_no + 5, t(__content.string(8129)) + &fmt_num(g.bank_debt[c], &num_fmt))?;
        total_liabs = g.tmp_total_debt;
        line_no = line_no + 6;
    }
    if Ext::from_f64(0.0) < g.tmp_player_advances {
        field_text = t(__content.string(8131));
        let mut advancer_count = 0;
        let player_count = g.num_players;
        for loop_idx in 1..=player_count {
            if g.entity_status[loop_idx] == 1 && Ext::from_f64(0.0) < g.player_advances[(loop_idx, c)] {
                advancer_count += 1;
                ref_idx = loop_idx;
            }
        }
        if advancer_count == 1 && g.ent_symbol[ref_idx].len() < 15 {
            field_text = g.ent_symbol[ref_idx].clone() + &string_fill(15, b" ");
            field_text = mid(field_text.as_bytes(), 1, 15);
        } else if advancer_count == 1 {
            field_text = replace(field_text.as_bytes(), b"Players", b"Player ");
        }
        lines.set(line_no, t(__content.string(8132)) + &field_text + &fmt_num(g.tmp_player_advances, &num_fmt))?;
        line_no = line_no + 1;
    }
    lines.set(line_no, t(__content.string(8133)) + &fmt_num(g.tmp_accrued_inc_tax, &num_fmt))?;
    line_no = line_no + 1;
    if Ext::from_f64(0.0) < g.capital_taxes_total {
        lines.set(line_no, t(__content.string(8134)) + &fmt_num(g.capital_taxes_total, &num_fmt))?;
        line_no = line_no + 1;
    }
    if s(0.005f32) < g.contingency_reserve[c] {
        lines.set(line_no, t(__content.string(8135)) + &fmt_num(g.contingency_reserve[c], &num_fmt))?;
        line_no = line_no + 1;
    }
    lines.set(line_no, t("                                  ----------------"))?;
    lines.set(
        line_no + 1,
        t(__content.string(8136)) + &fmt_num(total_liabs + g.contingency_reserve[c], &num_fmt),
    )?;
    field_text = PbStr::new();
    if c == g.antitrust_defendant {
        field_text = t("[2]");
        lawsuit_damages = g.antitrust_damages;
    }
    for loop_idx in 1..=10 {
        if mid(g.lawsuit_rec[loop_idx].as_bytes(), 1, 1) == "1" {
            let rec = g.lawsuit_rec[loop_idx].clone();
            if val(field(&rec, 2).as_bytes()) == cast::<Ext, _>(c) {
                field_text = t("[2]");
                lawsuit_damages = to_quad(val(field(&rec, 5).as_bytes()) + cast::<Ext, _>(lawsuit_damages));
            }
        }
    }
    if lawsuit_damages <= 0 {
        lines.set(line_no + 1, lines.get(line_no + 1)? + " [1]")?;
    } else {
        if field_text == "[2]" && footnote_no == 1 {
            field_text = t("[3]");
        }
        lines.set(line_no + 1, lines.get(line_no + 1)? + " [1], " + &field_text)?;
        footnote_no = footnote_no + 2;
    }
    lines.set(line_no + 2, t("                                  ----------------"))?;
    lines.set(line_no + 3, t(__content.string(8137)) + &fmt_num(g.net_worth, &num_fmt))?;
    lines.set(line_no + 4, t("                                  ================"))?;
    line_no = line_no + 5;
    lines.set(line_no, t(__content.string(8138)))?;
    lines.set(line_no + 1, t("--------------------------"))?;
    let nw_per_share = (s(0.01f32) * g.net_worth) / g.shares_outstanding[c];
    lines.set(line_no + 2, t(__content.string(8139)) + &fmt(nw_per_share, fmt_decimal.as_bytes()))?;
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, c, 0);
    g.stock_price_str = string_fill(20, b" ") + &g.stock_price_str;
    g.stock_price_str = right(g.stock_price_str.as_bytes(), 17);
    lines.set(line_no + 3, t(__content.string(8140)) + &g.stock_price_str)?;
    ref_idx = to_long(g.cap_hist_close[(0, c)]);
    if 0 < ref_idx && Ext::from_f64(0.0) < g.cap_hist_close[(ref_idx, c)] {
        update_price_history_tick(g, ui, rng, c);
        let mut low_cap = g.price_hist_low[(ref_idx, c)];
        let mut high_cap = g.price_hist_high[(ref_idx, c)];
        for _ in 0..11 {
            ref_idx = ref_idx - 1;
            if ref_idx == 0 {
                ref_idx = 60;
            }
            if high_cap < g.price_hist_high[(ref_idx, c)] {
                high_cap = g.price_hist_high[(ref_idx, c)];
            }
            if g.price_hist_low[(ref_idx, c)] < low_cap && Ext::from_f64(0.0) < g.price_hist_low[(ref_idx, c)] {
                low_cap = g.price_hist_low[(ref_idx, c)];
            }
        }
        if Ext::from_f64(0.0) < low_cap && low_cap < high_cap {
            low_cap = low_cap / g.shares_outstanding[c];
            high_cap = high_cap / g.shares_outstanding[c];
            field_text = fmt_trim(high_cap, fmt_decimal.as_bytes());
            aux_text = fmt_trim(low_cap, fmt_decimal.as_bytes());
            work_text = field_text.clone() + " / " + &aux_text;
            let tmp_num = work_text.len();
            if tmp_num < 14 {
                work_text = string_fill(14 - tmp_num, b" ") + &work_text;
            }
            lines.set(line_no + 4, t(__content.string(8141)) + &work_text)?;
        }
        line_no = line_no + 1;
    }
    field_text = fmt_trim(g.shares_outstanding[c] * Ext::from_f64(100.0), b"* ##########,.00");
    field_text = t("                    ") + &field_text;
    field_text = right(field_text.as_bytes(), 14);
    lines.set(
        line_no + 4,
        t(__content.string(8142)) + &field_text + " " + &g.million_prefix_lc + "illion",
    )?;
    field_text = fmt_trim(g.market_cap[c] * Ext::from_f64(100.0), b"* ##########,.00");
    field_text = t("                     ") + &field_text;
    field_text = right(field_text.as_bytes(), 14);
    lines.set(
        line_no + 5,
        t(__content.string(8143)) + &field_text + " " + &g.million_prefix_lc + "illion",
    )?;
    let mut year_shift = 0;
    if g.industry_num[c] == 71 {
        lines.set(line_no + 6, dash_line.clone())?;
        line_no = line_no - 6;
    } else {
        if c <= g.sim_co_count && g.cur_quarter == 4 {
            year_shift = 1;
        }
        let eps_line = |g: &Globals, back: i32, earnings: Ext| -> PbStr {
            t("  ")
                + &PbStr::from(format_num(cast::<Ext, _>(year_shift + g.cur_year) - cast::<Ext, _>(back), 15).as_str())
                + __content.string(8144)
                + &fmt((s(0.01f32) * earnings) / g.shares_outstanding[c], fmt_decimal.as_bytes())
                + __content.string(8145)
        };
        lines.set(line_no + 7, eps_line(g, 4, g.earnings_yr4_ago[c]))?;
        lines.set(line_no + 8, eps_line(g, 3, g.earnings_yr3_ago[c]))?;
        lines.set(line_no + 9, eps_line(g, 2, g.earnings_yr2_ago[c]))?;
        lines.set(line_no + 10, eps_line(g, 1, g.earnings_y1[c]))?;
        work_text = fmt((s(0.01f32) * g.earnings_ytd[c]) / g.shares_outstanding[c], fmt_decimal.as_bytes());
        work_text = right(work_text.as_bytes(), 17) + __content.string(8145);
        if g.earnings_ytd[c] == Ext::from_f64(0.0) || qtrs_reported == 0 {
            work_text = t(__content.string(8146));
        }
        if year_shift == 0 {
            if qtrs_reported == 0 {
                qtrs_reported = 1;
            }
            lines.set(
                line_no + 11,
                t("  ")
                    + &PbStr::from(format_num(cast::<Ext, _>(g.cur_year), 15).as_str())
                    + __content.string(8147)
                    + &PbStr::from(format_num(cast::<Ext, _>(qtrs_reported), 15).as_str())
                    + &work_text,
            )?;
            lines.set(line_no + 12, dash_line.clone())?;
        } else {
            lines.set(line_no + 11, dash_line.clone())?;
        }
    }
    lines.set(line_no + 14, t(__content.string(8148)))?;
    lines.set(line_no + 15, t("-------------------------"))?;
    let mut annual_earnings = g.earnings_ytd[c];
    qtrs_reported = g.cur_quarter;
    if g.sim_co_count < c {
        qtrs_reported = qtrs_reported - 1;
    }
    if qtrs_reported == 1 {
        annual_earnings = annual_earnings + g.earnings_y1[c] * s(0.75f32);
    }
    if qtrs_reported == 2 {
        annual_earnings = annual_earnings + g.earnings_y1[c] * s(0.5f32);
    }
    if qtrs_reported == 3 {
        annual_earnings = annual_earnings + g.earnings_y1[c] * s(0.25f32);
    }
    if qtrs_reported == 4 || qtrs_reported == 0 {
        annual_earnings = g.earnings_y1[c];
    }
    let mut pe_ratio: f32;
    if annual_earnings <= Ext::from_f64(0.0) {
        pe_ratio = 0.0f32;
    } else {
        pe_ratio = cast::<f32, _>(g.market_cap[c] / (s(0.01f32) * annual_earnings));
    }
    if Ext::from_f64(999.0) < cast::<Ext, _>(pe_ratio) || g.industry_num[c] == 71 {
        pe_ratio = 0.0f32;
    }
    if pe_ratio == Ext::from_f32(0.0f32) {
        work_text = t(__content.string(8149));
    } else {
        work_text = fmt(cast::<Ext, _>(pe_ratio), b"* ######.0") + __content.string(8150);
    }
    lines.set(line_no + 16, t(__content.string(8151)) + &work_text)?;
    let mut div_yield = cast::<f32, _>(calc_dividend_yield(g, c));
    if (cast::<Ext, _>(div_yield)) < s(0.05f32) {
        div_yield = 0.0f32;
    }
    lines.set(line_no + 17, t(__content.string(8152)) + &fmt(cast::<Ext, _>(div_yield), b"* ###.00") + "%")?;
    let mut debt_equity: Ext;
    if g.net_worth <= Ext::from_f64(0.0) {
        debt_equity = -Ext::from_f64(1.0);
    } else {
        debt_equity = g.tmp_total_debt / g.net_worth;
    }
    if !(Ext::from_f64(0.0) <= debt_equity) || !(debt_equity < Ext::from_f64(1000.0)) {
        debt_equity = -Ext::from_f64(1.0);
    } else {
        work_text = fmt(debt_equity, b"* ####.00") + " to 1";
    }
    if debt_equity < Ext::from_f64(0.0) {
        work_text = t(__content.string(8153));
    }
    lines.set(line_no + 18, t(__content.string(8154)) + &work_text)?;
    if industry != 1 || g.tmp_bank_total_loans <= Ext::from_f64(0.0) {
        line_no = line_no + 18;
    } else {
        work_text = fmt((g.bad_debt_reserve[c] / g.tmp_bank_total_loans) * Ext::from_f64(100.0), b"* ####.00");
        lines.set(line_no + 19, t(__content.string(8155)) + &work_text + "%")?;
        let deposits = (g.bank_c_ds[c] + g.bank_demand_deposits[c]) + cust_deposits;
        if deposits <= Ext::from_f64(0.0) {
            work_text = t("N/A");
            lines.set(line_no + 20, t(__content.string(8156)) + &work_text)?;
            lines.set(line_no + 21, t(__content.string(8157)) + &work_text)?;
        } else {
            work_text = fmt(((g.net_worth + g.bad_debt_reserve[c]) / deposits) * Ext::from_f64(100.0), b"* ####.00");
            lines.set(line_no + 20, t(__content.string(8158)) + &work_text + "%")?;
            let loans = (((cast::<Ext, _>(g.bank_mortgage_loans[c]) + g.bank_loans_total) + cast::<Ext, _>(g.subprime_secs[c]))
                + cast::<Ext, _>(g.bank_consumer_loans[c]))
                + g.tmp_corp_bonds_value;
            work_text = fmt((loans / deposits) * Ext::from_f64(100.0), b"* ####.00");
            lines.set(line_no + 21, t(__content.string(8159)) + &work_text + "%")?;
        }
        line_no = line_no + 21;
    }
    if Ext::from_f64(0.0) < g.corp_prior_net_worth[c] && g.earnings_y1[c] != Ext::from_f64(0.0) {
        roe_pct = (g.earnings_y1[c] / g.corp_prior_net_worth[c]) * Ext::from_f64(100.0);
        if Ext::from_f64(900.0) < roe_pct || roe_pct < -Ext::from_f64(99.0) {
            roe_pct = Ext::from_f64(0.0);
        }
        work_text = t(" ") + &fmt(roe_pct, b"* ###.00") + "%";
    }
    if roe_pct == Ext::from_f64(0.0) || g.industry_num[c] == 71 {
        work_text = t(__content.string(8153));
    }
    lines.set(
        line_no + 1,
        t(__content.string(8160)) + &str_trim(cast::<Ext, _>((year_shift + g.cur_year) - 1)) + "): " + &work_text,
    )?;
    line_no = line_no + 1;
    lines.set(line_no + 1, dash_line.clone())?;
    lines.set(line_no + 2, t(__content.string(8161)))?;
    lines.set(line_no + 3, dash_line.clone())?;
    lines.set(line_no + 4, t(__content.string(8162)) + &g.credit_rating_str)?;
    if industry == 1 {
        lines.set(
            line_no + 5,
            t(__content.string(8163))
                + &fmt(cast::<Ext, _>(g.entity_borrow_rate), b"* ##.00")
                + __content.string(8164)
                + &fmt(cast::<Ext, _>(g.sofr_rate), b"* ##.00")
                + "%)",
        )?;
        let cd_rate = get_bank_cd_rate(g, c);
        lines.set(line_no + 6, t(__content.string(8165)) + &fmt(cd_rate, b"* ##.00") + "%")?;
        line_no = line_no + 7;
    } else {
        work_text = fmt_trim(g.unused_credit_line, num_fmt.as_bytes()) + " " + &g.million_prefix_uc + "illion";
        if g.unused_credit_line <= Ext::from_f64(0.0) {
            work_text = t("ZERO");
            if g.credit_frozen[c] == 1 {
                work_text = work_text + __content.string(8166);
            }
        }
        lines.set(line_no + 5, t(__content.string(8167)) + &work_text)?;
        lines.set(
            line_no + 6,
            t(__content.string(8168))
                + &fmt(cast::<Ext, _>(g.borrow_rate), b"* ##.00")
                + __content.string(8169)
                + &fmt(cast::<Ext, _>(g.prime_rate), b"* ##.00")
                + "%)",
        )?;
        let bank = cast::<i32, _>(g.lending_bank[c]);
        lines.set(
            line_no + 7,
            t(__content.string(8170)) + &g.ent_name[bank] + " (" + &g.ent_symbol[bank] + ")",
        )?;
        g.lender_id = cast::<i32, _>(g.lending_bank[c]);
        if 10 < g.lender_id {
            has_links = 1;
            let code = link_code(g.lender_id);
            lines.set(line_no + 7, lines.get(line_no + 7)? + &string_fill(55, b" ") + "@C" + &code)?;
        }
        line_no = line_no + 8;
    }
    lines.set(line_no, dash_line.clone())?;
    lines.set(line_no + 1, t(__content.string(8171)))?;
    if industry == 71 {
        lines.set(line_no + 1, t(__content.string(8172)))?;
    }
    lines.set(line_no + 2, dash_line.clone())?;
    field_text = fmt(g.dividend_per_share[c], b"* ###.00");
    if industry == 71 {
        let portfolio_income = val(field(&g.ent_misc_data[c], 12).as_bytes());
        field_text = fmt_trim((portfolio_income * s(0.04f32)) / g.shares_outstanding[c], b"* ###.00");
        if val(field_text.as_bytes()) <= Ext::from_f64(0.0) {
            field_text = t(__content.string(8173));
        } else {
            field_text = field_text + __content.string(8174);
        }
    }
    lines.set(line_no + 3, t(__content.string(8175)) + &field_text)?;
    line_no = line_no + 4;
    if industry < 70 {
        let category = get_industry_category(industry);
        field_text = t(__content.string(8176));
        if category == 2 {
            field_text = t(__content.string(8177));
        }
        if industry == 1 {
            field_text = t(__content.string(8178));
        }
        if industry == 2 {
            field_text = t(__content.string(8179));
        }
        lines.set(line_no, field_text.clone() + &fmt(cast::<Ext, _>(g.disc_spend_pct[c]), b"* ###") + "%")?;
        line_no = line_no + 1;
    }
    if industry == 1 {
        lines.set(line_no, t(__content.string(8180)) + &fmt(cast::<Ext, _>(g.growth_rate[c]), b"* ###") + "%")?;
        line_no = line_no + 1;
    }
    if industry == 2 {
        lines.set(line_no, t(__content.string(8181)) + &fmt(cast::<Ext, _>(g.growth_rate[c]), b"* ###") + "%")?;
        line_no = line_no + 1;
    }
    if 2 < industry && industry < 70 {
        lines.set(line_no, t(__content.string(8182)) + &fmt(cast::<Ext, _>(g.growth_rate[c]), b"* ###") + "%")?;
        line_no = line_no + 1;
    }
    lines.set(line_no + 1, t(__content.string(8183)))?;
    lines.set(line_no + 2, t("-------------------"))?;
    lines.set(line_no + 3, t(__content.string(8184)) + &g.ind_name[cast::<i32, _>(g.industry_num[c])])?;
    lines.set(line_no + 4, t(__content.string(7961)) + &g.country_name[g.country_idx[c]])?;
    update_corp_control(g, ui, rng, c);
    if 0 < g.ctrl_since_year[c] && 10 < g.parent80_owner && g.industry_num[c] < 70 {
        line_no = line_no + 1;
        work_text = str15(cast::<Ext, _>(g.ctrl_since_year[c]));
        if g.ctrl_since_year[c] < 2000 {
            work_text = t(__content.string(8185));
        }
        lines.set(
            line_no + 4,
            t(__content.string(8186)) + &g.ent_symbol[cast::<i32, _>(g.parent80_owner)] + " since:" + &work_text,
        )?;
    }
    if g.voting_controller == 0 || c == g.voting_controller {
        work_text = t("NO ONE");
    } else {
        work_text = g.ent_name[g.voting_controller].clone();
    }
    lines.set(line_no + 5, t(__content.string(8187)) + &work_text)?;
    if 0 < get_controller_type(g, c) {
        lines.set(line_no + 5, replace(lines.get(line_no + 5)?.as_bytes(), b"by:   ", __content.bytes(8188)))?;
        if g.auto_pilot[c] == 1 {
            lines.set(line_no + 5, replace(lines.get(line_no + 5)?.as_bytes(), b"and", b"not"))?;
        }
    }
    if c != g.voting_controller && (10 < g.voting_controller || g.cur_player == g.voting_controller) {
        has_links = 1;
        let code = link_code(g.voting_controller);
        aux_text = t(" (") + &g.ent_symbol[g.voting_controller] + ")";
        if g.cur_player == g.voting_controller {
            aux_text = string_fill(7, b" ");
        }
        lines.set(line_no + 5, lines.get(line_no + 5)? + &aux_text + &string_fill(55, b" ") + "@C" + &code)?;
    }
    lines.set(
        line_no + 6,
        t(__content.string(8189)) + &PbStr::from(str_num(cast::<Ext, _>(g.public_pct), 7).as_str()) + __content.string(8190),
    )?;
    if g.public_pct == 0 && g.wholly_owned_by == 0 {
        lines.set(line_no + 6, t(__content.string(8191)))?;
    }
    if 0 < g.wholly_owned_by {
        let owner = cast::<i32, _>(g.wholly_owned_by);
        lines.set(line_no + 6, t(__content.string(8192)) + &g.ent_name[owner])?;
        if 10 < owner {
            lines.set(line_no + 6, lines.get(line_no + 6)? + " (" + &g.ent_symbol[owner] + ")")?;
        }
        if get_controller_type(g, c) < 2 {
            has_links = 1;
            let code = link_code(owner);
            lines.set(line_no + 6, lines.get(line_no + 6)? + &string_fill(52, b" ") + "@C" + &code)?;
        }
    }
    line_no = line_no + 7;
    let mut fund_branch_done = false;
    if g.industry_num[c] == 71 {
        let cap_loss = g.extraordinary_pl[c];
        if Ext::from_f64(1.0) < (cast::<Ext, _>(cap_loss)).abs() {
            work_text = fmt_trim(cast::<Ext, _>(cap_loss), num_fmt.as_bytes());
            lines.set(
                line_no,
                t(__content.string(8193))
                    + &trim(&g.currency_symbol)
                    + &work_text
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix,
            )?;
            if 0 < cap_loss {
                let with = t(__content.string(8194)) + &str15(cast::<Ext, _>(g.cur_year)) + __content.string(8195);
                lines.set(line_no, replace(lines.get(line_no)?.as_bytes(), __content.bytes(8196), with.as_bytes()))?;
            }
            line_no = line_no + 1;
        }
        let fee = g.fund_fee_pct[c];
        if (cast::<Ext, _>(fee)) < s(0.2f32) || Ext::from_f64(1.0) < cast::<Ext, _>(fee) || g.fund_adviser[c] == 0 {
            assign_etf_adviser(g, ui, rng, c, 0, 0);
        }
        ref_idx = g.fund_adviser[c];
        if 85 < ref_idx {
            if g.industry_num[ref_idx] != 37 && g.industry_num[ref_idx] != 2 {
                fund_branch_done = true; // goto LAB_00436aff
            } else {
                lines.set(
                    line_no,
                    t(__content.string(8197)) + &g.ent_name[ref_idx] + " (" + &g.ent_symbol[ref_idx] + ")",
                )?;
                if 1900 < g.ctrl_since_year[c] {
                    lines.set(line_no, lines.get(line_no)? + " since" + &str15(cast::<Ext, _>(g.ctrl_since_year[c])))?;
                }
                has_links = 1;
                let code = link_code(ref_idx);
                lines.set(line_no, lines.get(line_no)? + &string_fill(55, b" ") + "@C" + &code)?;
                line_no = line_no + 1;
            }
        }
    } else if g.tax_loss_carry[c] < Ext::from_f64(0.0) {
        work_text = fmt_trim(g.tax_loss_carry[c] * -Ext::from_f64(1.0), num_fmt.as_bytes());
        lines.set(
            line_no,
            t(__content.string(8198))
                + &trim(&g.currency_symbol)
                + &work_text
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix,
        )?;
        line_no = line_no + 1;
    }
    let _ = fund_branch_done;
    // LAB_00436aff
    if g.industry_num[c] == 37 || g.industry_num[c] == 2 {
        ref_idx = get_funds_advised_by(g, c);
        if 1 < ref_idx {
            lines.set(line_no, t(__content.string(8199)))?;
            if ref_idx < 1581 {
                aux_text = PbStr::new();
                if ref_idx < 6 {
                    for loop_idx in 1581..=1600 {
                        if c == g.fund_adviser[loop_idx] {
                            if aux_text.is_empty() {
                                aux_text = g.ent_symbol[loop_idx].clone();
                            } else {
                                aux_text = aux_text + ", " + &g.ent_symbol[loop_idx];
                            }
                        }
                    }
                    lines.set(line_no, lines.get(line_no)? + &aux_text + " (ETFs)")?;
                } else {
                    lines.set(line_no, lines.get(line_no)? + "(" + &str_trim(cast::<Ext, _>(ref_idx)) + __content.string(8200))?;
                }
            } else {
                aux_text = g.ent_name[ref_idx].clone() + " (" + &g.ent_symbol[ref_idx] + ")";
                has_links = 1;
                let code = link_code(ref_idx);
                lines.set(line_no, lines.get(line_no)? + &aux_text + &string_fill(55, b" ") + "@C" + &code)?;
            }
            line_no = line_no + 1;
        }
    }
    if Ext::from_f64(0.0) < g.tax_credits[c] {
        work_text = fmt_trim(g.tax_credits[c], num_fmt.as_bytes());
        lines.set(
            line_no,
            t(__content.string(8201))
                + &trim(&g.currency_symbol)
                + &work_text
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix,
        )?;
        if g.tax_loss_carry[c] < Ext::from_f64(0.0) {
            lines.set(line_no, replace(lines.get(line_no)?.as_bytes(), b"Accru", b"Unus"))?;
        }
        line_no = line_no + 1;
    }
    if g.industry_num[c] == 1 && Ext::from_f64(10.0) < g.bank_cust_deposits[c] {
        lines.set(
            line_no,
            t(__content.string(8202))
                + &trim(&g.currency_symbol)
                + &fmt_trim(g.bank_cust_deposits[c], num_fmt.as_bytes())
                + " "
                + &g.million_prefix_uc
                + "."
                + &g.currency_suffix,
        )?;
        line_no = line_no + 1;
    }
    if g.industry_num[c] < 71 {
        let mut ceo_player = 0;
        let player_count = g.num_players;
        for loop_idx in 1..=player_count {
            if c == g.player_ceo_of[loop_idx] {
                ceo_player = loop_idx;
            }
        }
        if 0 < ceo_player {
            if g.player_ceo_since_year[ceo_player] == 0 {
                field_text = PbStr::new();
                g.player_ceo_since_year[ceo_player] = g.cur_year - 3;
            } else {
                field_text = t(" (since") + &str15(cast::<Ext, _>(g.player_ceo_since_year[ceo_player])) + ")";
            }
            lines.set(line_no, t(__content.string(8203)) + &g.ent_name[ceo_player] + &field_text)?;
            line_no = line_no + 1;
        }
        let amount_m = calc_ceo_pay(g, ui, rng, c, 0);
        lines.set(
            line_no,
            t(__content.string(8204))
                + &g.currency_symbol
                + &fmt_trim(amount_m, fmt_decimal.as_bytes())
                + " "
                + &g.million_prefix_uc
                + "."
                + &g.currency_suffix,
        )?;
        line_no = line_no + 1;
        field_text = t(__content.string(8205)) + &trim(&g.currency_symbol);
        aux_text = fmt_trim(g.ceo_bonus_multiple * amount_m, b"* ###,.00");
        let mut suffix_text = t(")");
        if 0 < g.public_pct && ceo_player == g.corp_controller[c] {
            suffix_text = t(__content.string(8206));
        }
        lines.set(line_no, field_text.clone() + &aux_text + " " + &g.million_prefix_uc + "." + &suffix_text)?;
        if val(aux_text.as_bytes()) == Ext::from_f64(0.0) {
            if instr(1, suffix_text.as_bytes(), b"options") < 1 {
                lines.set(line_no, PbStr::new())?;
            } else {
                lines.set(line_no, t(__content.string(8207)))?;
                if val(field(&g.ent_misc_data[c], 3).as_bytes()) <= Ext::from_f64(0.0) {
                    lines.set(line_no, t(__content.string(8208)))?;
                }
            }
        }
        if ceo_player == 0 {
            suffix_text = t(")");
            if 0 < g.public_pct {
                suffix_text = t(__content.string(8209));
            }
            lines.set(line_no, t(__content.string(8210)) + &suffix_text)?;
        }
    }
    if !(lines.get(line_no)?.is_empty()) {
        line_no = line_no + 1;
    }
    let (tmp_num, day_of_month) = crate::logic::earnings::current_game_date(g);
    lines.set(
        line_no,
        t(__content.string(8211))
            + &g.month_name[tmp_num]
            + &str15(cast::<Ext, _>(day_of_month))
            + ","
            + &str15(cast::<Ext, _>(g.cur_year)),
    )?;
    line_no = line_no + 1;
    // UpdateDateDisplay (0x4a054a): the date label of the main window (UI); state effects only.
    {
        let progress = g.sim_co_count;
        let pos = calc_cur_month_of_year(g, progress);
        let month = to_long(ceil(pos));
        calc_day_of_month(g, month, frac(pos));
        g.tmp_ext1 = Ext::from_f64(0.0);
    }
    // ShowNextEarningsDate (0x4a070a): the next-earnings label (UI); state effects only.
    if 11 <= g.sel_entity {
        let sel = g.sel_entity;
        get_next_earnings_date_str(g, sel);
    }
    let date_str = get_next_earnings_date_str(g, c);
    lines.set(line_no, t(__content.string(8212)) + &date_str)?;
    if g.industry_num[c] == 71 {
        let date_str = get_next_earnings_date_str(g, c);
        lines.set(line_no, t(__content.string(8213)) + &date_str)?;
    }
    line_no = line_no + 1;
    let amount_m = calc_capital_taxes(g, c);
    if Ext::from_f64(0.0) < amount_m {
        let name_len = g.sel_corp_name.len();
        aux_text = t(" ") + &g.million_prefix_lc + "illion" + &g.currency_suffix;
        lines.set(line_no + 1, t(__content.string(8214)) + &g.sel_corp_name + ":")?;
        lines.set(line_no + 2, t("-------------------------------------") + &string_fill(name_len, b"-"))?;
        lines.set(
            line_no + 3,
            t("  (Quarterly total of \"other\" taxes: ")
                + &g.currency_symbol
                + &fmt_trim(amount_m, num_fmt.as_bytes())
                + &rtrim(aux_text.as_bytes())
                + ")",
        )?;
        if g.capital_tax_count == 1 {
            line_no = line_no - 1;
        }
        line_no = line_no + 4;
        let taxes: [(Ext, &str); 4] = [
            (g.health_care_tax_qtr, __content.string(8215)),
            (g.carbon_tax_qtr, __content.string(8216)),
            (g.oil_windfall_tax_qtr, __content.string(8217)),
            (g.corp_shares_tax_due, __content.string(8218)),
        ];
        for (amount, label) in taxes {
            if Ext::from_f64(0.0) < amount {
                lines.set(
                    line_no,
                    t(label) + &g.currency_symbol + &fmt_trim(amount, num_fmt.as_bytes()) + &aux_text,
                )?;
                line_no = line_no + 1;
            }
        }
        if Ext::from_f64(0.0) < g.holding_co_tax_due {
            lines.set(
                line_no,
                t(__content.string(8219))
                    + &g.currency_symbol
                    + &fmt_trim(g.holding_co_tax_due, num_fmt.as_bytes())
                    + &aux_text,
            )?;
            line_no = line_no + 1;
        }
        if g.capital_tax_count == 1 {
            lines.set(line_no - 1, lines.get(line_no - 1)? + __content.string(8220))?;
        }
    }
    field_text = t("FOOTNOTE");
    if 0 < footnote_no {
        field_text = field_text + "S";
    }
    lines.set(line_no + 1, field_text.clone() + __content.string(8221))?;
    lines.set(line_no + 2, dash_line.clone())?;
    field_text = t("company");
    if g.industry_num[c] == 71 {
        field_text = t("fund");
    }
    lines.set(line_no + 3, t(__content.string(8222)) + &field_text + __content.string(8223))?;
    lines.set(line_no + 4, t(__content.string(8224)))?;
    let swap_notional = to_quad(sum_swap_notional(g, c, 0));
    if 0 < swap_notional {
        ref_idx = get_sole_swap_counterparty(g, c);
        lines.set(line_no + 3, t(__content.string(8225)))?;
        lines.set(
            line_no + 4,
            t(__content.string(8226))
                + &trim(&g.currency_symbol)
                + &fmt_trim(cast::<Ext, _>(swap_notional), b"* ####,")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".",
        )?;
        if 0 < ref_idx && ref_idx < 1601 {
            lines.set(
                line_no + 3,
                t("[1] ") + &g.ent_symbol[c] + __content.string(8227) + &g.ent_name[ref_idx] + ",",
            )?;
            if 22 < g.ent_symbol[c].len() + g.ent_name[ref_idx].len() {
                lines.set(line_no + 3, replace(lines.get(line_no + 3)?.as_bytes(), __content.bytes(8228), b" "))?;
            }
            lines.set(line_no + 4, replace(lines.get(line_no + 4)?.as_bytes(), b" total ", b" "))?;
        }
    }
    lines.set(line_no + 5, PbStr::new())?;
    line_no = line_no + 3;
    if 0 < g.bonds_issued[c] {
        let info = g.bond_issue_info[c].clone();
        work_text = right(info.as_bytes(), 5);
        if mid(work_text.as_bytes(), 1, 1) == "0" {
            work_text = right(work_text.as_bytes(), 4);
        }
        work_text = work_text + __content.string(8229);
        lines.set(line_no + 3, t(__content.string(8230)) + &work_text)?;
        if 0 < instr(1, info.as_bytes(), b"cv") {
            lines.set(line_no + 3, replace(lines.get(line_no + 3)?.as_bytes(), __content.bytes(8231), __content.bytes(8232)))?;
        }
        work_text = trim(&g.currency_symbol)
            + &fmt_trim(cast::<Ext, _>(g.bonds_issued[c]), b"* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ", ";
        field_text = t("January");
        if 500 < c {
            field_text = t(__content.string(6796));
        }
        if 1000 < c {
            field_text = t("March");
        }
        aux_text = t(__content.string(8233));
        let maturity = val(mid(info.as_bytes(), 1, 4).as_bytes());
        if maturity <= cast::<Ext, _>(g.cur_year) {
            aux_text = t(__content.string(8234));
            if maturity == cast::<Ext, _>(g.cur_year) && g.cur_quarter == 1 && g.sim_co_count < c {
                aux_text = t("coming ");
            } else {
                bonds_past_due = 1;
            }
        }
        if 27 < field_text.len() + work_text.len() {
            aux_text = PbStr::new();
        }
        lines.set(
            line_no + 4,
            t(__content.string(8235)) + &work_text + &aux_text + "due in " + &field_text + ", " + &mid(info.as_bytes(), 1, 4) + ".",
        )?;
        if 0 < instr(1, info.as_bytes(), b"cv") {
            line_no = line_no + 1;
            let conv_price = val(field(&info, 2).as_bytes()) / g.shares_outstanding[c];
            let shares_per_bond = Cur::from_f64(Ext::from_f64(1000.0) / conv_price);
            lines.set(
                line_no + 4,
                t(__content.string(8236))
                    + &g.ent_symbol[c]
                    + __content.string(8237)
                    + &trim(&g.currency_symbol)
                    + &fmt_trim(conv_price, b"* ###,.00")
                    + __content.string(8238),
            )?;
            line_no = line_no + 1;
            lines.set(
                line_no + 4,
                t("    or ")
                    + &fmt_trim(shares_per_bond.get(), b"* ###,.000")
                    + __content.string(8239)
                    + &trim(&g.currency_symbol)
                    + __content.string(8240),
            )?;
            g.tmp_share_price = g.market_cap[c] / g.shares_outstanding[c];
            let conv_value = shares_per_bond.get() * g.tmp_share_price;
            bond_price = Cur::from_f64(bond_price.get() * Ext::from_f64(10.0));
            let conv_premium = Ext::from_f64(100.0) * ((bond_price.get() - conv_value) / conv_value);
            if Ext::from_f64(0.0) < conv_premium && conv_premium < Ext::from_f64(2000.0) {
                lines.set(line_no + 4, lines.get(line_no + 4)? + "They")?;
                line_no = line_no + 1;
                lines.set(
                    line_no + 4,
                    t(__content.string(8241))
                        + &fmt_trim(conv_premium, b"* ###,.0")
                        + __content.string(8242),
                )?;
            }
        }
        let mut years_left = to_long(val(mid(info.as_bytes(), 1, 4).as_bytes()));
        years_left = years_left - g.cur_year;
        field_text = PbStr::new();
        if 9 < years_left {
            field_text = t(__content.string(8243))
                + &g.ent_symbol[c]
                + __content.string(8244)
                + &str15(cast::<Ext, _>((g.cur_year + years_left) - 9))
                + ".";
            if 0 < instr(1, info.as_bytes(), b"cv") && Ext::from_f64(400.0) < g.bond_price[c].get() {
                field_text = t(__content.string(8245));
            }
        }
        if field_text.is_empty() {
            let call_pct = calc_bond_call_price_pct(years_left);
            field_text = t(__content.string(8246)) + &str15(cast::<Ext, _>(call_pct)) + __content.string(8247);
        }
        lines.set(line_no + 5, field_text.clone())?;
        line_no = line_no + 6;
        if g.bonds_public_held < 0 {
            g.bonds_public_held = 0;
        }
        work_text = trim(&g.currency_symbol)
            + &fmt_trim(cast::<Ext, _>(g.bonds_public_held), b"* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ".";
        if 0 < g.largest_bondholder {
            let lb = g.largest_bondholder;
            field_text = PbStr::new();
            if 10 < lb || g.cur_player == lb {
                has_links = 1;
                aux_text = link_code(lb);
                aux_text = string_fill(55, b" ") + "@C" + &aux_text;
                field_text = t(" (") + &g.ent_symbol[lb] + "). " + &aux_text;
                if g.cur_player == lb {
                    field_text = t(".          ") + &aux_text;
                }
            }
            lines.set(line_no, t(__content.string(8248)) + &g.ent_name[lb] + &field_text)?;
            line_no = line_no + 1;
        }
        lines.set(line_no, t(__content.string(8249)) + &work_text)?;
        line_no = line_no + 1;
        field_text = fmt_trim(cast::<Ext, _>(bond_ytm), b"* #####.00") + "%";
        if g.credit_rating_str == "D" {
            field_text = t(__content.string(8250));
        } else if bonds_past_due == 1 {
            field_text = t(__content.string(8251));
        }
        lines.set(line_no, t(__content.string(8252)) + &field_text)?;
        line_no = line_no + 1;
    }
    // LAB_00439231
    if 1 < footnote_no {
        aux_text = t("[2]");
        if footnote_no == 3 {
            line_no = line_no + 1;
            aux_text = t("[3]");
        } else {
            line_no = line_no + 3;
        }
        work_text = trim(&g.currency_symbol)
            + &trim(&g.currency_suffix)
            + " "
            + &fmt_trim(cast::<Ext, _>(lawsuit_damages), b"* #############,")
            + " "
            + &g.million_prefix_lc
            + "illion.";
        lines.set(line_no, aux_text.clone() + __content.string(8253))?;
        lines.set(line_no + 1, t(__content.string(8254)))?;
        lines.set(line_no + 2, t(__content.string(8255)) + &work_text)?;
        lines.set(line_no + 4, t(__content.string(8256)))?;
        lines.set(line_no + 5, t(__content.string(8257)))?;
        lines.set(line_no + 6, t(__content.string(8258)))?;
        lines.set(line_no + 7, t(__content.string(8259)))?;
        if g.contingency_reserve[c] < Ext::from_f64(1.0) {
            lines.set(line_no + 7, replace(lines.get(line_no + 7)?.as_bytes(), __content.bytes(8260), b"will be"))?;
        }
        line_no = line_no + 8;
    }
    if !lines.get(line_no + 2)?.is_empty() {
        line_no = line_no + 2;
    }
    lines.set(line_no + 2, t("@END"))?;
    if has_links == 1 {
        g.entity_pick_mode = t(__content.string(6093));
    }
    for k in 1..=900 {
        let l = lines.get(k)?;
        let end = l == "@END";
        g.text_lines[k] = l;
        if end {
            break;
        }
    }
    g.text_box_title = t(__content.string(8261)) + &g.ent_name[c] + " (" + &g.ent_symbol[c] + ") ===";
    g.text_has_links = 1;
    g.text_box_mode = t("20");
    show_text_report(g, ui, rng);
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// ShowEarningsReport
// ---------------------------------------------------------------------------------------------------------

/// Locals of ShowEarningsReport shared with its GOSUBs.
struct Er {
    corp_id: i32,
    lines: Lines,
    line_no: i32,
    field_idx: i32,
    field_text: PbStr,
    field_val: Ext,
    phrase1: PbStr,
    indent: PbStr,
}

impl Er {
    /// Port of ShowEarningsReport_GetMiscField (0x46bea1): `fieldText = PARSE$(EntMiscData(corpId), ANY "|",
    /// fieldIdx)`, `fieldVal = VAL(fieldText)`.
    fn get_misc_field(&mut self, g: &Globals) {
        self.field_text = field(&g.ent_misc_data[self.corp_id], self.field_idx);
        self.field_val = val(self.field_text.as_bytes());
    }

    fn set(&mut self, i: i32, x: PbStr) -> Result<(), OutOfRange> {
        self.lines.set(i, x)
    }

    fn get(&self, i: i32) -> Result<PbStr, OutOfRange> {
        self.lines.get(i)
    }

    /// Port of ShowEarningsReport_BondBuyback (0x46bbc3): the "prepaid/retired … of its bonds of <year>, at a
    /// discount" lines from fields 24 (face retired) and 25 (gain).
    fn bond_buyback(&mut self, g: &Globals) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
        let c = self.corp_id;
        self.field_idx = 24;
        self.get_misc_field(g);
        if Ext::from_f64(0.0) < self.field_val {
            self.phrase1 = t(__content.string(8262));
            if c % 2 == 1 {
                self.phrase1 = t(__content.string(8263));
            }
            let x = self.indent.clone()
                + &g.ent_symbol[c]
                + &self.phrase1
                + &trim(&g.currency_symbol)
                + &self.field_text
                + " "
                + &g.million_prefix_lc
                + __content.string(8264)
                + &mid(g.bond_issue_info[c].as_bytes(), 1, 4)
                + __content.string(8265);
            self.set(self.line_no, x)?;
            self.line_no += 1;
            self.field_idx = 25;
            self.get_misc_field(g);
            if self.field_val <= Ext::from_f64(1.0) {
                let x = self.get(self.line_no - 1)? + ".";
                self.set(self.line_no - 1, x)?;
            } else {
                let x = self.indent.clone()
                    + __content.string(8266)
                    + &trim(&g.currency_symbol)
                    + &self.field_text
                    + " "
                    + &g.million_prefix_lc
                    + "illion";
                self.set(self.line_no, x)?;
                let x = self.indent.clone() + __content.string(8267);
                self.set(self.line_no + 1, x)?;
                self.line_no += 2;
            }
            self.line_no += 1;
        }
        Ok(())
    }
}

/// Port of ShowEarningsReport (0x462f1e) with its GOSUBs ShowEarningsReport_BondBuyback (0x46bbc3) and
/// ShowEarningsReport_GetMiscField (0x46bea1).
/// The "LATEST EARNINGS REPORT" of company `corp_id` written as a financial news story from the fields of
/// its last quarterly report kept in `EntMiscData` (`|`-separated: 1 previous quarter's income, 2 prior
/// year EPS base, 3 quarter's income, 5 year-to-date, 6 quarter number, 8..9 asset/stock sales, 10 sales
/// growth, 11..12 dividends, 13 credit line change, 14 margin, 15 bonds sold, 16 industry, 17..25 bank /
/// insurer / buyback items, 26..28 extraordinary items, tax provision and parent, 31..41 tax mode, goodwill,
/// taxes on capital, hedges, credits, reserve change and the analysts' estimate), followed by the recent
/// headlines mentioning the company. Draws `RND` for wording variants.
///
/// Not for `ActionCode = "AUTO"` or (before the game ends) a computer player's turn. State effects:
/// `CheckCrossOwnership(corp, 1)` unless the game has ended, `CarbonTaxQtr`, `OilWindfallTaxQtr`,
/// `HoldingCoTaxDue`, `CorpSharesTaxDue`, `HealthCareTaxQtr` set from the report's fields,
/// `CalcEntityFinancials`, and `UpdateCorpControl` for insurers with underwriting results.
pub fn show_earnings_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error 9 paths leave the routine like the handler)
    let _ = earnings_report(g, ui, rng, corp_id);
}

fn earnings_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
    let fmt_millions: &[u8] = b"################,";
    if corp_id < 1 || 1600 < corp_id {
        // EntName(corpId): error 9 before anything else
        return Err(OutOfRange);
    }
    let corp_name = g.ent_name[corp_id].clone();
    let mut e = Er {
        corp_id,
        lines: Lines::new(2500),
        line_no: 0,
        field_idx: 0,
        field_text: PbStr::new(),
        field_val: Ext::from_f64(0.0),
        phrase1: PbStr::new(),
        indent: string_fill(7, b" "),
    };
    let indent = e.indent.clone();
    // (UI) DIALOG END DlgWhoOwnsWhat
    if g.action_code == "AUTO" {
        return Ok(());
    }
    if g.is_computer_player[g.cur_player] == 1 && !(g.game_end_flag == "Y") {
        return Ok(());
    }
    if !(g.game_end_flag == "Y") {
        check_cross_ownership(g, ui, rng, corp_id, 1);
    }
    let c = corp_id;
    if c < 11 {
        let mut phrase1 = g.cur_player_name.clone();
        if phrase1 == "...OUT..." {
            phrase1 = t(__content.string(8268));
        }
        let mut msg = t(__content.string(8269)) + &phrase1 + ".";
        show_quick_message(g, ui, &mut msg);
        return Ok(());
    }
    if g.entity_status[c] == 0 {
        return Ok(());
    }
    if g.entity_status[c] == 2 {
        describe_defunct_entity(g, ui, c);
        return Ok(());
    }
    let cur = |g: &Globals| trim(&g.currency_symbol);
    let shares = |g: &Globals| g.shares_outstanding[c];
    let mut phrase2 = PbStr::new();
    let mut phrase3 = PbStr::new();
    let mut est_eps_text = PbStr::new();
    let mut tmp_int: i32;
    if g.ent_misc_data[c].len() < 20 {
        e.line_no = 3;
        e.set(e.line_no, t(__content.string(8270)) + &corp_name + ".")?;
        e.line_no = 6;
    } else {
        let mut qtr_income: Ext = Ext::from_f64(0.0);
        let quarter_num: i32;
        let mut industry: i16;
        let mut period_label: PbStr;
        e.set(3, t(__content.string(8271)))?;
        e.field_idx = 16;
        e.get_misc_field(g);
        industry = to_integer(e.field_val);
        if industry < 1 || 71 < industry {
            industry = g.industry_num[c];
        }
        e.field_idx = 6;
        e.get_misc_field(g);
        quarter_num = to_long(e.field_val);
        let report_year;
        if quarter_num == 4 {
            e.field_idx = cast::<i32, _>(g.cur_quarter == 1);
            period_label = fmt(cast::<Ext, _>(g.cur_year) - cast::<Ext, _>(e.field_idx), b"####");
            report_year = g.cur_year - e.field_idx;
        } else {
            period_label = t("QUARTERLY");
            report_year = g.cur_year;
        }
        if c % 2 == 0 {
            let n = corp_name.len() + 31;
            e.field_idx = period_label.len() + n;
            tmp_int = 40 - e.field_idx / 2;
            e.set(
                5,
                string_fill(tmp_int, b" ") + &period_label + __content.string(8272) + &corp_name + __content.string(8273),
            )?;
            if industry == 71 {
                let x = replace(e.get(5)?.as_bytes(), b"EARNINGS", b"RESULTS");
                e.set(5, x)?;
            }
        } else {
            phrase2 = t("YEAR");
            if quarter_num < 4 {
                phrase2 = t("QUARTER");
            }
            let n = corp_name.len() + 29;
            e.field_idx = phrase2.len() + n;
            tmp_int = 40 - e.field_idx / 2;
            e.set(5, string_fill(tmp_int, b" ") + &corp_name + __content.string(8274) + &phrase2 + __content.string(8275))?;
        }
        e.set(6, string_fill(tmp_int, b" ") + &string_fill(e.field_idx - 7, b"-"))?;
        if industry == 71 {
            e.line_no = 7;
        } else {
            earnings_body(g, ui, rng, &mut e, industry, quarter_num, report_year, &mut qtr_income, &mut phrase2, &mut phrase3, &mut est_eps_text, &mut tmp_int)?;
        }
        // fieldIdx = 8: asset sales and cash flow
        e.field_idx = 8;
        e.get_misc_field(g);
        let asset_sale_gain = e.field_val;
        phrase2 = remove(e.field_text.as_bytes(), b"-");
        e.field_idx = 9;
        e.get_misc_field(g);
        let sold_shares_list = e.field_text.clone();
        e.field_idx = 13;
        e.get_misc_field(g);
        let credit_line_change = e.field_val;
        e.phrase1 = fmt_trim(e.field_val.abs(), b"* ###,");
        e.field_idx = 15;
        e.get_misc_field(g);
        let mut bonds_sold = e.field_val;
        let mut bond_status: i16 = 0;
        if e.field_val < Ext::from_f64(0.0) {
            bond_status = 1;
            if e.field_val == -Ext::from_f64(999.0) {
                bond_status = 2;
            }
            if e.field_val == -Ext::from_f64(9999.0) || e.field_val == -Ext::from_f64(999.0) {
                bonds_sold = Ext::from_f64(0.0);
            } else {
                bonds_sold = e.field_val.abs();
            }
        }
        if bonds_sold != Ext::from_f64(0.0) {
            bonds_sold = bonds_sold.abs();
            phrase3 = str_trim(bonds_sold);
        }
        let mut skip_to_dividends = false;
        if 1 < industry {
            if !e.get(e.line_no - 1)?.is_empty() {
                e.line_no += 1;
            }
            if Ext::from_f64(0.0) <= credit_line_change && bonds_sold == Ext::from_f64(0.0) && asset_sale_gain == Ext::from_f64(0.0) && val(sold_shares_list.as_bytes()) == Ext::from_f64(0.0) {
                e.phrase1 = t(__content.string(8276));
                if bond_status == 1 {
                    e.phrase1 = t(__content.string(8277));
                }
                if bond_status == 2 {
                    e.phrase1 = t(__content.string(8278));
                }
                if industry == 71 {
                    e.phrase1 = replace(e.phrase1.as_bytes(), b"company", b"ETF");
                }
                e.set(e.line_no, indent.clone() + &e.phrase1)?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8279))?;
                e.field_idx = 19;
                e.get_misc_field(g);
                e.set(
                    e.line_no + 2,
                    indent.clone() + "of " + &cur(g) + &fmt_trim(e.field_val, fmt_millions) + " " + &g.million_prefix_lc + "illion",
                )?;
                if credit_line_change == Ext::from_f64(0.0) {
                    e.field_idx = 20;
                    e.get_misc_field(g);
                    if e.field_val == Ext::from_f64(0.0) {
                        let x = e.get(e.line_no + 2)? + __content.string(8280);
                        e.set(e.line_no + 2, x)?;
                        e.set(e.line_no + 3, indent.clone() + __content.string(8281))?;
                        e.line_no += 5;
                    } else {
                        let x = e.get(e.line_no + 2)? + &g.currency_suffix + ".";
                        e.set(e.line_no + 2, x)?;
                        e.line_no += 4;
                    }
                } else {
                    e.field_idx = 13;
                    e.get_misc_field(g);
                    let x = e.get(e.line_no + 2)?
                        + __content.string(8282)
                        + &cur(g)
                        + &fmt_trim(e.field_val, fmt_millions)
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + " on";
                    e.set(e.line_no + 2, x)?;
                    e.field_idx = 20;
                    e.get_misc_field(g);
                    e.set(e.line_no + 3, indent.clone() + __content.string(8283))?;
                    e.set(
                        e.line_no + 4,
                        indent.clone()
                            + "to "
                            + &cur(g)
                            + &fmt_trim(e.field_val, fmt_millions)
                            + " "
                            + &g.million_prefix_lc
                            + "illion"
                            + &g.currency_suffix,
                    )?;
                    if val(e.field_text.as_bytes()) == Ext::from_f64(0.0) {
                        e.set(e.line_no + 4, indent.clone() + "to zero")?;
                    }
                    let x = e.get(e.line_no + 4)? + __content.string(8284);
                    e.set(e.line_no + 4, x)?;
                    e.line_no += 6;
                }
                e.bond_buyback(g)?;
                skip_to_dividends = true;
            }
            if !skip_to_dividends {
                if Ext::from_f64(0.0) < credit_line_change {
                    e.set(
                        e.line_no,
                        indent.clone()
                            + &g.ent_symbol[c]
                            + __content.string(8285)
                            + &cur(g)
                            + &e.phrase1
                            + " "
                            + &g.million_prefix_lc
                            + "illion"
                            + &g.currency_suffix
                            + ".",
                    )?;
                    e.line_no += 1;
                }
                if bonds_sold == Ext::from_f64(0.0)
                    && asset_sale_gain == Ext::from_f64(0.0)
                    && !(val(sold_shares_list.as_bytes()) != Ext::from_f64(0.0) || Ext::from_f64(0.0) <= credit_line_change)
                {
                    e.set(e.line_no, indent.clone() + __content.string(8286))?;
                    e.set(e.line_no + 1, indent.clone() + __content.string(8287))?;
                } else {
                    e.set(e.line_no, indent.clone() + __content.string(8288))?;
                    e.set(e.line_no + 1, indent.clone() + __content.string(8289))?;
                }
                e.line_no += 3;
                if credit_line_change < Ext::from_f64(0.0) {
                    let bank = cast::<i32, _>(g.lending_bank[c]);
                    e.set(
                        e.line_no,
                        t(__content.string(8290))
                            + &cur(g)
                            + &e.phrase1
                            + " "
                            + &g.million_prefix_lc
                            + __content.string(8291)
                            + &g.ent_name[bank]
                            + ".",
                    )?;
                    e.line_no += 1;
                }
                if Ext::from_f64(0.0) < bonds_sold {
                    e.set(
                        e.line_no,
                        t(__content.string(8292)) + &cur(g) + &phrase3 + " " + &g.million_prefix_lc + __content.string(8293),
                    )?;
                    e.line_no += 1;
                }
                if asset_sale_gain != Ext::from_f64(0.0) {
                    e.field_idx = 23;
                    e.get_misc_field(g);
                    e.phrase1 = t("loss.");
                    if Ext::from_f64(0.0) < asset_sale_gain {
                        e.phrase1 = t("gain.");
                    }
                    if e.field_text.len() < 2 {
                        e.field_text = t(__content.string(8294));
                    }
                    e.set(
                        e.line_no,
                        t(__content.string(8295))
                            + &e.field_text
                            + " for a "
                            + &cur(g)
                            + &phrase2
                            + " "
                            + &g.million_prefix_lc
                            + "il. "
                            + &e.phrase1,
                    )?;
                    e.line_no += 1;
                }
                e.field_text = t("0");
                e.field_idx = 1;
                if Ext::from_f64(0.0) < val(sold_shares_list.as_bytes()) {
                    while !(e.field_text == "-1") {
                        e.field_text = parse_any(sold_shares_list.as_bytes(), b"^", e.field_idx);
                        tmp_int = to_long(val(e.field_text.as_bytes()));
                        if tmp_int < 11 {
                            e.field_text = t("-1");
                        } else {
                            e.set(e.line_no, t(__content.string(8296)) + &g.ent_name[tmp_int] + "....")?;
                            e.line_no += 1;
                            e.field_idx += 1;
                        }
                    }
                }
                e.line_no += 1;
            }
        }
        // LAB_00469e1b: dividends
        e.field_idx = 12;
        e.get_misc_field(g);
        let dividend_total = e.field_val;
        e.field_idx = 11;
        e.get_misc_field(g);
        let mut dividend_change = to_long(e.field_val);
        if industry == 71 {
            dividend_change = 0;
        }
        if dividend_change == 0 && dividend_total == Ext::from_f64(0.0) {
            if industry == 71 {
                e.set(e.line_no, indent.clone() + __content.string(8297))?;
            }
        } else {
            if !e.get(e.line_no - 1)?.is_empty() {
                e.line_no += 1;
            }
            if dividend_total <= Ext::from_f64(0.0) {
                if g.industry_num[c] == 71 && !(g.bank_debt[c] <= Ext::from_f64(0.0) && g.bonds_issued[c] <= 0) {
                    e.set(e.line_no, indent.clone() + __content.string(8298))?;
                    e.set(e.line_no + 1, indent.clone() + __content.string(8299))?;
                    e.set(e.line_no + 2, indent.clone() + __content.string(8300))?;
                    e.line_no += 4;
                }
            } else {
                phrase2 = t(__content.string(8301));
                if dividend_change == 2 {
                    phrase2 = t(__content.string(8302));
                }
                e.phrase1 = fmt_trim((dividend_total * s(0.01f32)) / shares(g), b"* ##.00");
                if Ext::from_f64(0.0) < val(e.phrase1.as_bytes()) {
                    e.set(
                        e.line_no,
                        indent.clone()
                            + &g.ent_symbol[c]
                            + &phrase2
                            + __content.string(8303)
                            + &cur(g)
                            + &e.phrase1
                            + __content.string(1071),
                    )?;
                    if industry == 71 {
                        let x = replace(e.get(e.line_no)?.as_bytes(), __content.bytes(8304), b"");
                        e.set(e.line_no, x)?;
                    }
                    e.line_no += 1;
                }
            }
            // LAB_0046a25a
            if dividend_change != 0 && dividend_change != 2 {
                e.phrase1 = t("cut");
                phrase2 = t(" payout");
                if dividend_total == Ext::from_f64(0.0) {
                    e.phrase1 = t(__content.string(8305));
                    if Ext::from_f64(0.0) < g.dividend_per_share[c] {
                        e.phrase1 = t("omitted");
                        phrase2 = PbStr::new();
                    }
                }
                if dividend_change == 1 && Ext::from_f64(0.0) < dividend_total {
                    e.phrase1 = t("raised");
                    phrase2 = t(" rate");
                }
                e.set(e.line_no, indent.clone() + __content.string(8306) + &e.phrase1 + __content.string(8307) + &phrase2)?;
                if dividend_change < 0 && g.cash[c] + g.t_bills[c] < g.tmp_total_debt * s(0.2f32) {
                    let x = e.get(e.line_no)? + __content.string(8308);
                    e.set(e.line_no, x)?;
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        let x = replace(e.get(e.line_no)?.as_bytes(), __content.bytes(2829), __content.bytes(8309));
                        e.set(e.line_no, x)?;
                    }
                    e.line_no += 1;
                    e.set(e.line_no, indent.clone() + __content.string(8310))?;
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        let x = replace(e.get(e.line_no)?.as_bytes(), __content.bytes(8311), b"duction");
                        e.set(e.line_no, x)?;
                    }
                }
                let x = e.get(e.line_no)? + ".";
                e.set(e.line_no, x)?;
            }
        }
        e.line_no += 1;
        if industry == 71 && quarter_num < 4 && g.exchange_rate * Ext::from_f64(50.0) < cast::<Ext, _>(g.extraordinary_pl[c]) {
            e.set(
                e.line_no,
                indent.clone()
                    + &g.ent_symbol[c]
                    + __content.string(8312)
                    + &cur(g)
                    + &fmt_trim(cast::<Ext, _>(g.extraordinary_pl[c]), b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + __content.string(8313),
            )?;
            e.line_no += 1;
            e.set(e.line_no, indent.clone() + __content.string(8314))?;
            e.line_no += 1;
        }
        if industry == 71 && quarter_num == 4 {
            e.field_idx = 26;
            e.get_misc_field(g);
            let extraordinary_items = e.field_val;
            e.phrase1 = PbStr::new();
            if extraordinary_items <= Ext::from_f64(10.0) {
                if extraordinary_items < Ext::from_f64(0.0) {
                    e.set(e.line_no, indent.clone() + __content.string(8315))?;
                    e.set(e.line_no + 1, indent.clone() + __content.string(8316))?;
                    e.line_no += 2;
                }
            } else {
                e.phrase1 = t(__content.string(8317))
                    + &cur(g)
                    + &fmt_trim((extraordinary_items * s(0.01f32)) / shares(g), b"##.00")
                    + " per";
                if dividend_total <= Ext::from_f64(0.0) {
                    e.phrase1 = remove(e.phrase1.as_bytes(), b"also ");
                }
                e.set(e.line_no, indent.clone() + &e.phrase1)?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8318))?;
                e.line_no += 2;
            }
            if extraordinary_items < Ext::from_f64(0.0) || Ext::from_f64(10.0) < extraordinary_items {
                tmp_int = price_pct_change_months(g, c, 3);
                e.phrase1 = t("rose");
                if tmp_int < 0 {
                    e.phrase1 = t("fell");
                }
                if 4 < tmp_int.wrapping_abs() {
                    e.line_no += 1;
                    e.set(
                        e.line_no,
                        indent.clone()
                            + __content.string(8319)
                            + &g.ent_symbol[c]
                            + __content.string(8320)
                            + &e.phrase1
                            + &str15(cast::<Ext, _>(tmp_int.wrapping_abs()))
                            + __content.string(8321),
                    )?;
                }
                e.line_no += 1;
            }
        }
        calc_entity_financials(g, ui, rng, c);
        if 0 < g.bonds_issued[c] && g.credit_rating_num[c] < 8 {
            e.field_idx = 0;
            period_label = mid(g.bond_issue_info[c].as_bytes(), 1, 4);
            if val(period_label.as_bytes()) == Ext::from_f64(1.0) + cast::<Ext, _>(g.cur_year) {
                e.field_idx = 1;
            }
            tmp_int = 0;
            let mut earnings_ytd = to_quad(g.earnings_ytd[c]);
            if earnings_ytd == 0 {
                earnings_ytd = to_quad(-Ext::from_f64(1.0));
            }
            if (cast::<Ext, _>(g.bonds_issued[c])) <= g.liquid_funds {
                tmp_int = 1;
            } else if !(Ext::from_f64(0.0) <= qtr_income) {
                tmp_int = 2;
            } else if earnings_ytd < 0 && g.earnings_y1[c] < Ext::from_f64(0.0) {
                tmp_int = 2;
            }
            if e.field_idx == 1 && 0 < tmp_int {
                if !e.get(e.line_no - 1)?.is_empty() {
                    e.line_no += 1;
                }
                e.set(
                    e.line_no,
                    indent.clone() + &g.ent_symbol[c] + __content.string(8322),
                )?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8323))?;
                if tmp_int == 2 {
                    let x = e.get(e.line_no + 1)? + __content.string(8324);
                    e.set(e.line_no + 1, x)?;
                    if Ext::from_f64(0.0) < qtr_income {
                        let x = replace(e.get(e.line_no + 1)?.as_bytes(), b"ongoing", b"recent");
                        e.set(e.line_no + 1, x)?;
                    }
                    e.set(e.line_no + 2, indent.clone() + __content.string(8325))?;
                    e.set(
                        e.line_no + 3,
                        indent.clone()
                            + __content.string(8326)
                            + &str15(cast::<Ext, _>(g.cur_year))
                            + " and "
                            + &period_label
                            + __content.string(8327),
                    )?;
                    e.set(e.line_no + 4, indent.clone() + __content.string(8328))?;
                    e.line_no += 5;
                } else if tmp_int == 1 {
                    let x = e.get(e.line_no + 1)? + __content.string(8329);
                    e.set(e.line_no + 1, x)?;
                    e.set(e.line_no + 2, indent.clone() + __content.string(8330))?;
                    e.set(
                        e.line_no + 3,
                        indent.clone() + __content.string(8331) + &str15(cast::<Ext, _>(g.cur_year)) + " and " + &period_label + ".\"",
                    )?;
                    e.line_no += 4;
                }
            }
        }
        if !e.get(e.line_no - 1)?.is_empty() {
            e.line_no += 1;
        }
        e.phrase1 = t("in the ") + &g.ind_name[cast::<i32, _>(industry)] + __content.string(1547);
        if industry == 70 {
            e.phrase1 = t(__content.string(8332));
        }
        if industry == 71 {
            e.phrase1 = t(__content.string(8333));
        }
        phrase2 = g.ent_name[c].clone();
        if 50 < e.phrase1.len() + phrase2.len() {
            phrase2 = g.ent_symbol[c].clone();
        }
        e.set(e.line_no, indent.clone() + &phrase2 + " is " + &e.phrase1)?;
        if industry == 71 && g.industry_num[c] != 71 {
            e.line_no += 1;
            e.set(
                e.line_no,
                indent.clone() + "(" + &g.ent_symbol[c] + __content.string(8334),
            )?;
        } else if industry != g.industry_num[c] {
            let x = replace(e.get(e.line_no)?.as_bytes(), b" is ", b" was ");
            e.set(e.line_no, x)?;
            e.line_no += 1;
            e.field_idx = cast::<i32, _>(g.industry_num[c]);
            e.phrase1 = t("in the ") + &g.ind_name[e.field_idx] + __content.string(1157);
            if e.field_idx == 70 {
                e.phrase1 = t(__content.string(8335));
            }
            e.set(e.line_no, indent.clone() + "(" + &g.ent_symbol[c] + __content.string(3965) + &e.phrase1 + ".)")?;
        }
        e.line_no += 2;
        e.field_idx = g.corp_controller[c];
        if 0 < e.field_idx && c != e.field_idx {
            e.phrase1 = PbStr::new();
            phrase2 = g.ent_name[c].clone();
            if e.field_idx < 10 {
                e.phrase1 = t(__content.string(8336));
                phrase2 = g.ent_symbol[c].clone();
            }
            e.set(
                e.line_no,
                indent.clone() + &phrase2 + __content.string(1121) + &e.phrase1 + &g.ent_name[e.field_idx] + ".",
            )?;
            e.line_no += 1;
        }
    }
    // LAB_0046b74a: recent headlines
    tmp_int = e.line_no + 2;
    e.line_no += 3;
    e.field_idx = 0;
    phrase2 = t(" ") + &g.ent_symbol[c] + " ";
    phrase3 = t("(") + &g.ent_symbol[c] + ")";
    est_eps_text = t(" ") + &g.ent_symbol[c] + ".";
    for headline_idx in 1..=60 {
        e.phrase1 = t(" - ") + &g.news_headline[headline_idx] + " ";
        if (0 < instr(1, e.phrase1.as_bytes(), phrase2.as_bytes())
            || 0 < instr(1, e.phrase1.as_bytes(), phrase3.as_bytes())
            || 0 < instr(1, e.phrase1.as_bytes(), est_eps_text.as_bytes()))
            && instr(1, e.phrase1.as_bytes(), __content.bytes(1041)) == 0
        {
            e.field_idx = 1;
            e.line_no += 1;
            e.set(e.line_no, rtrim(e.phrase1.as_bytes()))?;
        }
    }
    if e.field_idx == 1 {
        e.set(tmp_int, t(__content.string(8337)) + &corp_name + ":")?;
        let n = e.get(tmp_int)?.len() - 3;
        e.set(tmp_int + 1, t("   ") + &string_fill(n, b"-"))?;
    }
    e.line_no += 2;
    e.set(e.line_no + 1, t("@END"))?;
    for k in 1..=90 {
        g.text_lines[k] = e.get(k)?;
    }
    g.text_box_title = t(__content.string(8338)) + &corp_name + " (" + &g.ent_symbol[c] + ") ===";
    g.text_has_links = 1;
    g.text_box_mode = t("84");
    show_text_report(g, ui, rng);
    Ok(())
}

/// The middle part of [`show_earnings_report`] for companies other than funds: the results story,
/// analysts' estimates, extraordinary items, taxes, goodwill, margins and hedges, swaps, taxes on capital,
/// insurer and bank items.
#[allow(clippy::too_many_arguments)]
fn earnings_body(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    e: &mut Er,
    industry: i16,
    quarter_num: i32,
    report_year: i32,
    qtr_income: &mut Ext,
    phrase2: &mut PbStr,
    phrase3: &mut PbStr,
    est_eps_text: &mut PbStr,
    tmp_int: &mut i32,
) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
    let fmt_decimal: &[u8] = b"############.00";
    let fmt_millions: &[u8] = b"################,";
    let c = e.corp_id;
    let indent = e.indent.clone();
    let corp_name = g.ent_name[c].clone();
    let cur = |g: &Globals| trim(&g.currency_symbol);
    let eps = |g: &Globals, x: Ext| fmt_trim((s(0.01f32) * x) / g.shares_outstanding[c], fmt_decimal);

    e.field_idx = 3;
    e.get_misc_field(g);
    *qtr_income = e.field_val;
    let qtr_income = *qtr_income;
    let eps_cur_qtr = eps(g, qtr_income);
    e.field_val = g.earnings_y1[c] * s(0.25f32);
    if g.cur_quarter == 4 && c <= g.sim_co_count {
        e.field_idx = 4;
    }
    if g.cur_quarter == 1 && g.sim_co_count < c {
        e.field_idx = 4;
    }
    if e.field_idx == 4 {
        e.get_misc_field(g);
    }
    let eps_prev_year = fmt_trim((e.field_val * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
    e.phrase1 = t(__content.string(8339));
    if val(eps_cur_qtr.as_bytes()) < val(eps_prev_year.as_bytes()) {
        e.phrase1 = t(__content.string(8340));
    }
    if eps_prev_year == eps_cur_qtr {
        e.phrase1 = t(__content.string(8341));
    }
    if e.field_val * s(1.4f32) < qtr_income {
        e.phrase1 = t(__content.string(8342));
    }
    *phrase2 = t("first");
    if quarter_num == 2 {
        *phrase2 = t("second");
    }
    if quarter_num == 3 {
        *phrase2 = t("third");
    } else if quarter_num == 4 {
        *phrase2 = t("fourth");
    }
    let mut tmp_text = g.country_display_name[g.country_idx[c]].clone() + " (FN)--" + &corp_name + " ";
    e.field_idx = tmp_text.len();
    let case_val = cast::<Ext, _>(e.field_idx);
    *phrase3 = if Ext::from_f64(45.0) < case_val {
        t(__content.string(8343))
    } else if Ext::from_f64(41.0) <= case_val {
        t(__content.string(8344))
    } else if Ext::from_f64(38.0) <= case_val {
        t(__content.string(8345))
    } else if Ext::from_f64(33.0) <= case_val {
        t(__content.string(8346))
    } else if Ext::from_f64(28.0) <= case_val {
        t(__content.string(8347))
    } else if Ext::from_f64(23.0) <= case_val {
        t(__content.string(8348))
    } else if Ext::from_f64(19.0) <= case_val {
        t(__content.string(8349))
    } else {
        t(__content.string(8350))
    };
    e.set(8, indent.clone() + &tmp_text + &*phrase3)?;
    if qtr_income < Ext::from_f64(0.0) {
        e.set(
            9,
            indent.clone() + __content.string(8351) + &*phrase2 + __content.string(8352) + &str15(cast::<Ext, _>(report_year)) + __content.string(8353),
        )?;
        e.line_no = 10;
    } else {
        e.set(
            9,
            indent.clone() + &e.phrase1 + __content.string(8354) + &*phrase2 + __content.string(8352) + &str15(cast::<Ext, _>(report_year)) + " as ",
        )?;
        e.set(10, indent.clone() + __content.string(8355) + &*phrase2 + __content.string(8356))?;
        e.line_no = 11;
    }
    e.set(
        e.line_no,
        indent.clone()
            + __content.string(8357)
            + &cur(g)
            + &eps_cur_qtr
            + " vs. "
            + &cur(g)
            + &eps_prev_year
            + __content.string(8358)
            + &str15(cast::<Ext, _>(report_year - 1))
            + __content.string(8359),
    )?;
    if 68 < e.get(e.line_no)?.len() {
        tmp_text = e.get(e.line_no)?;
        e.set(e.line_no, remove(tmp_text.as_bytes(), b"the "))?;
    }
    e.field_idx = 0;
    if g.earnings_y1[c] == Ext::from_f64(0.0) && g.earnings_yr2_ago[c] == Ext::from_f64(0.0) && g.earnings_yr3_ago[c] == Ext::from_f64(0.0) && g.earnings_yr4_ago[c] == Ext::from_f64(0.0) {
        e.field_idx = 1;
    }
    if quarter_num == 4
        && g.earnings_y1[c] != Ext::from_f64(0.0)
        && g.earnings_yr2_ago[c] == Ext::from_f64(0.0)
        && g.earnings_yr3_ago[c] == Ext::from_f64(0.0)
        && g.earnings_yr4_ago[c] == Ext::from_f64(0.0)
    {
        e.field_idx = 1;
    }
    if e.field_idx == 1 {
        e.line_no += 1;
        e.set(
            e.line_no,
            indent.clone() + __content.string(8360) + &str15(cast::<Ext, _>(report_year)) + " year.)",
        )?;
    }
    e.phrase1 = t("profit");
    if qtr_income < Ext::from_f64(0.0) {
        e.phrase1 = t("loss");
    }
    e.field_idx = 3;
    e.get_misc_field(g);
    tmp_text = indent.clone()
        + __content.string(8361)
        + &e.phrase1
        + __content.string(8362)
        + &cur(g)
        + &fmt_trim(e.field_val, b"* ###,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ".";
    if 68 < tmp_text.len() {
        tmp_text = remove(tmp_text.as_bytes(), __content.bytes(8363));
        if tmp_text.len() < 60 {
            tmp_text = replace(tmp_text.as_bytes(), b"quarter", __content.bytes(8364));
        }
    }
    let op_income_val = val(e.field_text.as_bytes());
    e.set(e.line_no + 1, tmp_text.clone())?;
    e.field_idx = 38;
    e.get_misc_field(g);
    let hedge_pnl = val(e.field_text.as_bytes());
    e.field_idx = 40;
    e.get_misc_field(g);
    let reserve_change = val(e.field_text.as_bytes());
    if !(hedge_pnl <= op_income_val * s(0.15f32)) && !(op_income_val <= Ext::from_f64(0.0)) {
        e.phrase1 = t(__content.string(8365));
        if c % 2 == 1 {
            e.phrase1 = t("boosted");
        }
        *phrase2 = t(__content.string(2644));
        if industry == 37 {
            *phrase2 = t("futures");
        }
        e.set(
            e.line_no + 2,
            indent.clone() + __content.string(8366) + &e.phrase1 + __content.string(8367) + &*phrase2 + __content.string(8368),
        )?;
        e.line_no += 1;
    } else if !(Ext::from_f64(0.0) <= hedge_pnl) && !(hedge_pnl.abs() <= op_income_val.abs() * s(0.15f32)) {
        e.phrase1 = t(__content.string(1260));
        if c % 3 == 2 {
            e.phrase1 = t(__content.string(8369));
        }
        *phrase2 = t(__content.string(8370));
        if industry == 37 {
            *phrase2 = t(__content.string(8371));
        }
        e.set(e.line_no + 2, indent.clone() + __content.string(8372) + &e.phrase1 + __content.string(8373) + &*phrase2)?;
        e.line_no += 1;
    } else if qtr_income.abs() * s(0.04f32) < reserve_change.abs() {
        e.phrase1 = t(__content.string(8374));
        *phrase2 = t(__content.string(8375));
        if Ext::from_f64(0.0) < reserve_change {
            e.phrase1 = t(__content.string(8376));
            *phrase2 = t("reduced");
        }
        e.set(
            e.line_no + 2,
            indent.clone()
                + "A "
                + &cur(g)
                + &fmt_trim(reserve_change.abs(), fmt_millions)
                + " "
                + &g.million_prefix_uc
                + ". "
                + &e.phrase1
                + __content.string(8377)
                + &*phrase2
                + " net.",
        )?;
        e.line_no += 1;
    }
    e.line_no += 2;
    if 1 < quarter_num {
        if quarter_num < 4 {
            *phrase3 = t(__content.string(8378));
        } else {
            *phrase3 = g.ent_symbol[c].clone() + __content.string(8379);
        }
        e.field_idx = 5;
        e.get_misc_field(g);
        e.phrase1 = fmt_trim((e.field_val * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
        e.field_idx = 2;
        e.get_misc_field(g);
        *phrase2 = fmt_trim((e.field_val * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
        e.set(
            e.line_no,
            indent.clone() + &*phrase3 + __content.string(8380) + &cur(g) + &e.phrase1 + __content.string(8381) + &cur(g) + &*phrase2,
        )?;
        e.phrase1 = trim(&g.currency_suffix);
        if !e.phrase1.is_empty() {
            e.phrase1 = e.phrase1.clone() + " ";
        }
        e.set(e.line_no + 1, indent.clone() + &e.phrase1 + __content.string(8382))?;
        e.line_no += 2;
    }
    e.field_idx = 1;
    e.get_misc_field(g);
    if e.field_val != Ext::from_f64(0.0) && (quarter_num != 1 || g.start_year != g.cur_year) {
        *phrase3 = t("were ");
        e.phrase1 = t("up");
        if qtr_income <= Ext::from_f64(0.0) {
            e.phrase1 = t(__content.string(1060));
            *phrase3 = PbStr::new();
        }
        if qtr_income < e.field_val {
            e.phrase1 = t("off");
            if qtr_income <= Ext::from_f64(0.0) {
                e.phrase1 = t(__content.string(8383));
            }
        }
        *phrase2 = fmt_trim((e.field_val * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
        if eps_cur_qtr == *phrase2 {
            e.phrase1 = t(__content.string(7716));
        }
        e.phrase1 = phrase3.clone() + &e.phrase1;
        e.set(
            e.line_no + 1,
            indent.clone() + __content.string(8384) + &e.phrase1 + " from " + &cur(g) + &*phrase2 + __content.string(8385),
        )?;
        e.field_idx = 41;
        e.get_misc_field(g);
        let est_earnings = e.field_val;
        if e.field_val == Ext::from_f64(0.0) {
            let x = e.get(e.line_no + 1)? + ".";
            e.set(e.line_no + 1, x)?;
        } else {
            *est_eps_text = fmt_trim((est_earnings * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
            *phrase3 = e.get(e.line_no + 1)?;
            let est = |x: &PbStr| val(x.as_bytes());
            let ve = est(est_eps_text);
            let vc = est(&eps_cur_qtr);
            'est: {
                if vc <= ve {
                    if ve <= vc {
                        e.phrase1 = t(", and");
                        *phrase2 = t(__content.string(8386));
                    } else {
                        *phrase2 = t(__content.string(8387));
                        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                            *phrase2 = replace(phrase2.as_bytes(), b"Street", __content.bytes(8388));
                        }
                        if instr(1, phrase3.as_bytes(), b"up") < 1 && instr(1, phrase3.as_bytes(), __content.bytes(1060)) < 1 {
                            e.phrase1 = t(", and");
                            if qtr_income < Ext::from_f64(0.0) {
                                *phrase2 = t(__content.string(8389));
                            }
                        } else {
                            e.phrase1 = t(", but");
                            if ve < Ext::from_f64(0.0) {
                                *phrase2 = t(__content.string(8390));
                            }
                        }
                        if g.rand_seed % 2 == 1 {
                            *phrase2 = t(__content.string(8391));
                            if rng.rnd() * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                                if Ext::from_f64(0.0) < ve {
                                    if vc < ve * s(0.8f32) && s(0.04f32) < ve - vc {
                                        *phrase2 = replace(phrase2.as_bytes(), b"below", __content.bytes(8392));
                                        break 'est;
                                    }
                                    if ve * s(0.9f32) < vc && ve - vc < s(0.03f32) {
                                        *phrase2 = replace(phrase2.as_bytes(), b"below", __content.bytes(8393));
                                    }
                                }
                            } else {
                                *phrase2 = replace(phrase2.as_bytes(), b"were", b"came in");
                            }
                        }
                    }
                } else {
                    *phrase2 = t(__content.string(8394));
                    if ve < Ext::from_f64(0.0) {
                        *phrase2 = replace(phrase2.as_bytes(), __content.bytes(8395), __content.bytes(8396));
                    }
                    if g.rand_seed % 3 == 1 || ve < Ext::from_f64(0.0) {
                        *phrase2 = replace(phrase2.as_bytes(), __content.bytes(8397), __content.bytes(8398));
                    }
                    if g.rand_seed % 3 == 2 {
                        *phrase2 = t(__content.string(8399));
                        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                            *phrase2 = replace(phrase2.as_bytes(), __content.bytes(8400), __content.bytes(8401));
                        }
                        if ve * s(1.2f32) < vc && s(0.1f32) < vc - ve {
                            *phrase2 = replace(phrase2.as_bytes(), b"beat", __content.bytes(8402));
                        }
                    }
                    if 0 < instr(1, phrase2.as_bytes(), __content.bytes(8397)) && Ext::from_f64(0.0) < qtr_income {
                        if vc * s(0.9f32) < ve && vc - ve < s(0.03f32) {
                            *phrase2 = replace(phrase2.as_bytes(), b"exceed", __content.bytes(8403));
                            if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(2.0) {
                                if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
                                    *phrase2 = replace(phrase2.as_bytes(), __content.bytes(8404), __content.bytes(8405));
                                }
                            } else {
                                *phrase2 = replace(phrase2.as_bytes(), __content.bytes(8404), __content.bytes(8406));
                            }
                        }
                    }
                    if instr(1, phrase3.as_bytes(), b"up") < 1 && instr(1, phrase3.as_bytes(), __content.bytes(1060)) < 1 {
                        e.phrase1 = t(", but");
                        break 'est;
                    }
                    e.phrase1 = t(", and");
                }
            }
            // LAB_00465675
            if 0 < instr(1, phrase3.as_bytes(), __content.bytes(7716)) {
                *phrase2 = right(e.phrase1.as_bytes(), 3) + " " + &*phrase2;
                e.phrase1 = t(",");
            }
            e.set(e.line_no + 1, phrase3.clone() + &e.phrase1)?;
            e.set(e.line_no + 2, indent.clone() + &*phrase2 + &cur(g) + &*est_eps_text + __content.string(1071))?;
            e.line_no += 1;
        }
        e.line_no += 2;
    }
    e.field_idx = 26;
    e.get_misc_field(g);
    let extraordinary_items = e.field_val;
    e.field_idx = 27;
    e.get_misc_field(g);
    let tax_provision = e.field_val;
    e.field_idx = 28;
    e.get_misc_field(g);
    *tmp_int = to_long(val(e.field_text.as_bytes()));
    e.field_idx = 31;
    e.get_misc_field(g);
    let tax_filing_mode = to_long(e.field_val);
    e.field_idx = 39;
    e.get_misc_field(g);
    let tax_credits = to_quad(e.field_val);
    if !e.get(e.line_no - 1)?.is_empty() {
        e.line_no += 1;
    }
    if qtr_income.abs() * s(0.02f32) <= extraordinary_items.abs() {
        e.set(e.line_no, indent.clone() + __content.string(8407))?;
        e.set(e.line_no + 1, indent.clone() + __content.string(8408))?;
        e.set(e.line_no + 2, indent.clone() + __content.string(8409))?;
        e.set(e.line_no + 3, indent.clone() + __content.string(8364))?;
        e.line_no += 3;
        e.phrase1 = fmt_trim(((extraordinary_items + qtr_income) * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
        *phrase2 = fmt_trim(extraordinary_items + qtr_income, fmt_decimal);
        *phrase3 = fmt_trim((extraordinary_items * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
        tmp_text = fmt_trim(extraordinary_items, fmt_decimal);
        if extraordinary_items <= Ext::from_f64(0.0) {
            let x = e.get(e.line_no)? + __content.string(8410);
            e.set(e.line_no, x)?;
            e.set(
                e.line_no + 1,
                indent.clone()
                    + __content.string(8411)
                    + &cur(g)
                    + &tmp_text
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(8412)
                    + &cur(g)
                    + &*phrase3
                    + " per",
            )?;
            e.set(
                e.line_no + 2,
                indent.clone()
                    + __content.string(8413)
                    + &cur(g)
                    + &*phrase2
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(8412)
                    + &cur(g)
                    + &e.phrase1
                    + &g.currency_suffix
                    + __content.string(1071),
            )?;
            e.line_no += 4;
        } else {
            let x = e.get(e.line_no)?
                + " was "
                + &cur(g)
                + &*phrase2
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ", or "
                + &cur(g)
                + &e.phrase1
                + " per";
            e.set(e.line_no, x)?;
            e.set(e.line_no + 1, indent.clone() + __content.string(8414))?;
            e.set(
                e.line_no + 2,
                indent.clone()
                    + __content.string(8415)
                    + &cur(g)
                    + &tmp_text
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(8412)
                    + &cur(g)
                    + &*phrase3
                    + __content.string(1071),
            )?;
            e.line_no += 4;
        }
    } else {
        e.set(e.line_no, indent.clone() + __content.string(8416))?;
        e.set(e.line_no + 1, indent.clone() + __content.string(8417))?;
        e.set(e.line_no + 2, indent.clone() + __content.string(8418))?;
        e.set(e.line_no + 3, indent.clone() + __content.string(8419))?;
        e.set(e.line_no + 4, indent.clone() + __content.string(8420))?;
        e.set(e.line_no + 5, indent.clone() + __content.string(8421))?;
        e.line_no += 7;
        if s(0.5f32) <= tax_provision.abs() {
            *phrase3 = fmt_trim(tax_provision, fmt_millions);
            e.field_text = cur(g) + &*phrase3 + " " + &g.million_prefix_lc + "illion" + &g.currency_suffix;
            *phrase3 = remove(phrase3.as_bytes(), b",");
        } else {
            e.field_text = t("zero");
            *phrase3 = PbStr::new();
        }
        let ti = *tmp_int;
        if tax_filing_mode < 2 || qtr_income <= Ext::from_f64(0.0) || 10 < ti {
            if Ext::from_f64(0.0) <= qtr_income || 10 < ti {
                if ti < 11 {
                    if ti < 11 && Ext::from_f64(0.0) < qtr_income && 0 < tax_credits {
                        e.set(e.line_no, indent.clone() + __content.string(8422))?;
                        e.set(
                            e.line_no + 1,
                            indent.clone()
                                + __content.string(8423)
                                + &cur(g)
                                + &fmt_trim(cast::<Ext, _>(tax_credits), fmt_millions)
                                + " "
                                + &g.million_prefix_lc
                                + "illion"
                                + &g.currency_suffix
                                + ".",
                        )?;
                        e.line_no += 2;
                    }
                } else if tax_filing_mode == 1 || tax_filing_mode == 3 {
                    if qtr_income < Ext::from_f64(0.0) {
                        e.set(e.line_no, indent.clone() + __content.string(8424))?;
                        e.set(
                            e.line_no + 1,
                            indent.clone() + &g.ent_symbol[ti] + __content.string(8425),
                        )?;
                        e.set(e.line_no + 2, indent.clone() + __content.string(8426))?;
                        e.set(e.line_no + 3, indent.clone() + __content.string(8427))?;
                        e.set(e.line_no + 4, indent.clone() + __content.string(8428))?;
                        e.line_no += 6;
                    }
                } else {
                    e.set(
                        e.line_no,
                        indent.clone()
                            + &g.ent_symbol[c]
                            + __content.string(8429)
                            + &g.ent_symbol[ti]
                            + ",",
                    )?;
                    e.set(e.line_no + 1, indent.clone() + __content.string(8430))?;
                    *phrase3 = cur(g) + &fmt_trim(cast::<Ext, _>(tax_credits), fmt_millions) + " " + &g.million_prefix_lc + "illion";
                    if Ext::from_f64(0.0) <= g.tax_loss_carry[c] {
                        if 1 < tax_credits {
                            e.set(e.line_no + 2, indent.clone() + __content.string(8431) + &*phrase3 + __content.string(8432))?;
                            if Ext::from_f64(0.0) < tax_provision {
                                let x = replace(e.get(e.line_no + 2)?.as_bytes(), __content.bytes(8433), b"lower");
                                e.set(e.line_no + 2, x)?;
                            }
                            e.line_no += 1;
                        }
                    } else {
                        e.set(
                            e.line_no + 2,
                            indent.clone() + __content.string(8434),
                        )?;
                        e.line_no += 1;
                        if 1 < tax_credits && tax_provision < -Ext::from_f64(1.0) {
                            e.set(
                                e.line_no + 2,
                                indent.clone() + __content.string(8431) + &*phrase3 + __content.string(8435),
                            )?;
                            e.line_no += 1;
                            e.set(
                                e.line_no + 2,
                                indent.clone() + __content.string(8436),
                            )?;
                            e.line_no += 1;
                        }
                    }
                    e.line_no += 3;
                }
            } else {
                e.set(e.line_no, indent.clone() + __content.string(8437))?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8438))?;
                e.set(e.line_no + 2, indent.clone() + __content.string(8439))?;
                e.set(e.line_no + 3, indent.clone() + __content.string(8440))?;
                e.line_no += 5;
            }
        } else {
            e.phrase1 = t(__content.string(8441));
            *phrase2 = t(__content.string(8442));
            if val(phrase3.as_bytes()) < Ext::from_f64(0.0) {
                e.phrase1 = t(__content.string(8443));
                *phrase2 = t(__content.string(8444));
            }
            if tax_provision != Ext::from_f64(0.0) || tax_credits <= 0 {
                e.set(e.line_no, indent.clone() + __content.string(8445) + &e.phrase1)?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8446) + &*phrase2)?;
                e.set(e.line_no + 2, indent.clone() + __content.string(8447) + &e.field_text + ".")?;
                if g.tax_loss_carry[c] < Ext::from_f64(0.0) && Ext::from_f64(0.0) < val(phrase3.as_bytes()) {
                    e.set(e.line_no + 3, indent.clone() + __content.string(8448))?;
                    e.line_no += 1;
                }
                if 0 < tax_credits && Ext::from_f64(0.0) <= g.tax_loss_carry[c] {
                    *phrase3 = fmt_trim(cast::<Ext, _>(tax_credits), fmt_millions);
                    e.field_text = cur(g) + &*phrase3 + " " + &g.million_prefix_uc + "." + &g.currency_suffix;
                    e.set(e.line_no + 3, indent.clone() + __content.string(8449) + &e.field_text)?;
                    *phrase3 = remove(phrase3.as_bytes(), b",");
                }
                e.line_no += 4;
            } else {
                e.set(e.line_no, indent.clone() + __content.string(8450))?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8451))?;
                e.set(e.line_no + 2, indent.clone() + __content.string(8452))?;
                e.line_no += 3;
            }
        }
    }
    if 130 < c {
        e.field_idx = 32;
        e.get_misc_field(g);
        if Ext::from_f64(0.0) < e.field_val {
            e.phrase1 = t(__content.string(8453));
            if qtr_income < Ext::from_f64(0.0) {
                e.phrase1 = t(__content.string(8454));
            }
            e.set(e.line_no, indent.clone() + __content.string(8455) + &e.phrase1 + __content.string(8456))?;
            e.set(
                e.line_no + 1,
                indent.clone()
                    + __content.string(8457)
                    + &cur(g)
                    + &e.field_text
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".",
            )?;
            e.line_no += 2;
        }
    }
    if 2 < industry && industry < 70 {
        if !e.get(e.line_no - 1)?.is_empty() {
            e.line_no += 1;
        }
        e.field_idx = 14;
        e.get_misc_field(g);
        if val(e.field_text.as_bytes()) == Ext::from_f64(0.0) {
            e.field_text = t("0");
        }
        e.set(e.line_no, indent.clone() + &g.ent_symbol[c] + __content.string(8458))?;
        e.set(e.line_no + 1, indent.clone() + "was " + &e.field_text + __content.string(8459))?;
        e.field_idx = 10;
        e.get_misc_field(g);
        if e.field_val == Ext::from_f64(0.0) {
            let x = e.get(e.line_no + 1)? + __content.string(8460);
            e.set(e.line_no + 1, x)?;
        } else {
            *phrase2 = t("grew");
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                *phrase2 = t("rose");
            }
            if e.field_val < Ext::from_f64(0.0) {
                *phrase2 = t("fell");
            }
            let x = e.get(e.line_no + 1)? + &*phrase2 + __content.string(8461) + &e.field_text + "%.";
            e.set(e.line_no + 1, x)?;
        }
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) || cast::<i32, _>(g.superfund_corp) == c {
            e.phrase1 = t(__content.string(8462));
            if cast::<i32, _>(g.superfund_corp) == c && (cast::<Ext, _>(g.asbestos_corp)).abs() != cast::<Ext, _>(c) {
                e.phrase1 = t(__content.string(8463));
            }
            e.line_no += 1;
            e.set(e.line_no + 1, indent.clone() + __content.string(8464) + &e.phrase1 + __content.string(8465))?;
        }
        e.line_no += 2;
        e.field_idx = 38;
        e.get_misc_field(g);
        let tmp_amount = val(e.field_text.as_bytes());
        if tmp_amount != Ext::from_f64(0.0) {
            *phrase2 = match industry {
                7 | 8 | 11 | 16 | 23 | 25 | 52 | 69 => t(__content.string(2588)),
                15 | 28 => t(__content.string(8466)),
                37 => t(__content.string(8467)),
                50 => t(__content.string(7698)),
                _ => t("SKIP"),
            };
            if !(*phrase2 == "SKIP") {
                e.phrase1 = t("profit");
                if tmp_amount < Ext::from_f64(0.0) {
                    e.phrase1 = t("loss");
                }
                e.field_text = remove(e.field_text.as_bytes(), b"-");
                *phrase3 = t("closed");
                if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                    *phrase3 = t("lifted");
                }
                e.set(e.line_no, indent.clone() + __content.string(8468) + &*phrase2)?;
                e.set(e.line_no + 1, indent.clone() + __content.string(8469) + &*phrase3 + __content.string(8470))?;
                if industry == 37 {
                    e.set(e.line_no + 1, indent.clone() + __content.string(8471))?;
                }
                e.set(
                    e.line_no + 2,
                    indent.clone()
                        + __content.string(8472)
                        + &e.phrase1
                        + " of "
                        + &cur(g)
                        + &e.field_text
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + ".",
                )?;
                e.line_no += 3;
                *phrase3 = t(__content.string(8473));
            }
        }
        if g.industry_num[c] == 37 && Ext::from_f64(5.0) < calc_adviser_fee_income(g, c) {
            if *phrase3 == __content.string(8473) {
                e.line_no += 1;
            }
            e.set(e.line_no, indent.clone() + __content.string(8474))?;
            e.line_no += 1;
        }
    }
    if c <= g.sim_co_count {
        let swap_pnl = to_quad(swap_pnl_accumulator(g, c, 2, Ext::from_f64(0.0)));
        e.phrase1 = t("profits");
        if swap_pnl < 0 {
            e.phrase1 = t("losses");
        }
        if 0 < swap_pnl.wrapping_abs() {
            e.set(
                e.line_no,
                indent.clone()
                    + __content.string(8475)
                    + &e.phrase1
                    + " of "
                    + &cur(g)
                    + &fmt_trim((cast::<Ext, _>(swap_pnl)).abs(), b"* ######,"),
            )?;
            e.set(
                e.line_no + 1,
                indent.clone() + &g.million_prefix_lc + "illion" + &g.currency_suffix + __content.string(8476),
            )?;
            e.line_no += 2;
        }
    }
    let mut cap_tax_count = 0;
    let mut last_cap_tax_kind = 0;
    e.field_idx = 33;
    e.get_misc_field(g);
    g.carbon_tax_qtr = e.field_val;
    if Ext::from_f64(0.0) < e.field_val {
        cap_tax_count = 1;
        last_cap_tax_kind = 1;
    }
    e.field_idx = 34;
    e.get_misc_field(g);
    g.oil_windfall_tax_qtr = e.field_val;
    if Ext::from_f64(0.0) < e.field_val {
        cap_tax_count += 1;
        last_cap_tax_kind = 2;
    }
    e.field_idx = 35;
    e.get_misc_field(g);
    g.holding_co_tax_due = e.field_val;
    if Ext::from_f64(0.0) < e.field_val {
        cap_tax_count += 1;
        last_cap_tax_kind = 3;
    }
    e.field_idx = 36;
    e.get_misc_field(g);
    g.corp_shares_tax_due = e.field_val;
    if Ext::from_f64(0.0) < e.field_val {
        cap_tax_count += 1;
        last_cap_tax_kind = 4;
    }
    e.field_idx = 37;
    e.get_misc_field(g);
    g.health_care_tax_qtr = e.field_val;
    if Ext::from_f64(0.0) < e.field_val {
        cap_tax_count += 1;
        last_cap_tax_kind = 5;
    }
    let cap_tax_total =
        e.field_val + g.corp_shares_tax_due + g.holding_co_tax_due + g.carbon_tax_qtr + g.oil_windfall_tax_qtr;
    if Ext::from_f64(1.0) < cap_tax_total {
        if !e.get(e.line_no - 1)?.is_empty() {
            e.line_no += 1;
        }
        if c < 131 || 69 < g.industry_num[c] {
            e.set(e.line_no, indent.clone() + &g.ent_symbol[c] + __content.string(8477))?;
        } else {
            e.set(e.line_no, indent.clone() + __content.string(8478))?;
            e.line_no += 1;
            e.set(e.line_no, indent.clone() + __content.string(8479))?;
        }
        e.line_no += 1;
        if cap_tax_count == 1 {
            match last_cap_tax_kind {
                1 => {
                    e.phrase1 = t(__content.string(8480));
                    e.field_val = g.carbon_tax_qtr;
                }
                2 => {
                    e.phrase1 = t(__content.string(8481));
                    e.field_val = g.oil_windfall_tax_qtr;
                }
                3 => {
                    e.phrase1 = t(__content.string(8482));
                    e.field_val = g.holding_co_tax_due;
                }
                4 => {
                    e.phrase1 = t(__content.string(8483));
                    e.field_val = g.corp_shares_tax_due;
                }
                5 => {
                    e.phrase1 = t(__content.string(8484));
                }
                _ => {}
            }
            *phrase2 = cur(g) + &fmt_trim(e.field_val, fmt_millions) + " " + &g.million_prefix_lc + "illion";
            if 17 < phrase2.len() {
                let with = g.million_prefix_uc.clone() + ".";
                *phrase2 = replace(phrase2.as_bytes(), b"million", with.as_bytes());
            }
            e.set(e.line_no, indent.clone() + __content.string(8485) + &*phrase2 + " of " + &e.phrase1 + ". ")?;
        } else {
            e.phrase1 = cur(g) + &fmt_trim(cap_tax_total, fmt_millions) + " " + &g.million_prefix_lc + "illion.";
            e.set(e.line_no, indent.clone() + __content.string(8486) + &e.phrase1)?;
        }
        e.phrase1 = fmt_trim(((qtr_income + cap_tax_total) * s(0.01f32)) / g.shares_outstanding[c], fmt_decimal);
        if val(e.phrase1.as_bytes()) != val(eps_cur_qtr.as_bytes()) {
            *phrase2 = t(__content.string(8487));
            if cap_tax_count == 1 {
                let x = e.get(e.line_no)? + &*phrase2;
                e.set(e.line_no, x)?;
                if e.get(e.line_no)?.len() < 48 {
                    let x = e.get(e.line_no)? + &g.ent_symbol[c] + "'s";
                    e.set(e.line_no, x)?;
                }
                *phrase2 = PbStr::new();
            }
            e.line_no += 1;
            e.set(
                e.line_no,
                indent.clone() + &*phrase2 + __content.string(8488) + &cur(g) + &e.phrase1 + __content.string(8489),
            )?;
        }
        e.line_no += 2;
    }
    calc_entity_financials(g, ui, rng, c);
    if industry == 2 {
        e.field_idx = 18;
        e.get_misc_field(g);
        e.phrase1 = t("profit");
        *phrase2 = t("was");
        if e.field_val < Ext::from_f64(0.0) {
            e.phrase1 = t("losses");
            *phrase2 = t("were");
        }
        if e.field_val != Ext::from_f64(0.0) {
            if !e.get(e.line_no - 1)?.is_empty() {
                e.line_no += 1;
            }
            e.field_idx = g.corp_tax_rate;
            update_corp_control(g, ui, rng, c);
            let after_tax = ((e.field_val * s(0.01f32)) * (s(0.01f32) * cast::<Ext, _>(100 - e.field_idx))) / g.shares_outstanding[c];
            tmp_text = fmt_trim(after_tax, fmt_decimal);
            e.set(
                e.line_no,
                indent.clone()
                    + &g.ent_symbol[c]
                    + __content.string(8490)
                    + &e.phrase1
                    + __content.string(8491)
                    + &*phrase2,
            )?;
            e.set(
                e.line_no + 1,
                indent.clone() + &cur(g) + &tmp_text + &g.currency_suffix + __content.string(8492),
            )?;
            e.field_idx = 10;
            e.get_misc_field(g);
            *tmp_int = to_long(e.field_val);
            if *tmp_int != 0 {
                let x = e.get(e.line_no + 1)? + __content.string(8493);
                e.set(e.line_no + 1, x)?;
                *phrase2 = PbStr::new();
                *phrase3 = t("force ");
                if e.get(e.line_no + 1)?.len() < 60 {
                    *phrase2 = t("force");
                }
                let x = e.get(e.line_no + 1)? + &*phrase2;
                e.set(e.line_no + 1, x)?;
                if !phrase2.is_empty() {
                    *phrase3 = PbStr::new();
                }
                e.phrase1 = t("grew");
                if *tmp_int < 0 {
                    e.phrase1 = t("shrank");
                }
                e.set(
                    e.line_no + 2,
                    indent.clone()
                        + &*phrase3
                        + &e.phrase1
                        + __content.string(8494)
                        + &str15(cast::<Ext, _>(tmp_int.wrapping_abs()))
                        + "%.",
                )?;
                e.line_no += 1;
            }
            e.line_no += 3;
        }
        e.field_idx = 22;
        e.get_misc_field(g);
        if qtr_income.abs() * s(0.1f32) < e.field_val && Ext::from_f64(10.0) < e.field_val && g.market_cap[c] < e.field_val {
            e.set(e.line_no, indent.clone() + __content.string(8495))?;
            e.set(
                e.line_no + 1,
                indent.clone()
                    + __content.string(8496)
                    + &cur(g)
                    + &fmt_trim(e.field_val.abs(), b"#########")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".",
            )?;
            e.line_no += 2;
        }
    }
    if industry == 1 {
        e.field_idx = 17;
        e.get_misc_field(g);
        if Ext::from_f64(5.0) < e.field_val.abs() {
            if !e.get(e.line_no - 1)?.is_empty() {
                e.line_no += 1;
            }
            e.phrase1 = t("de");
            if Ext::from_f64(0.0) < e.field_val {
                e.phrase1 = t("in");
            }
            e.set(
                e.line_no,
                indent.clone()
                    + &g.ent_symbol[c]
                    + " "
                    + &e.phrase1
                    + __content.string(8497)
                    + &cur(g)
                    + &fmt_trim(e.field_val.abs(), b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ",",
            )?;
            if e.phrase1 == "de" {
                e.phrase1 = t("in");
            } else if e.phrase1 == "in" {
                e.phrase1 = t("de");
            }
            e.set(e.line_no + 1, indent.clone() + &e.phrase1 + __content.string(8498))?;
            e.line_no += 2;
        }
        e.field_idx = 22;
        e.get_misc_field(g);
        let tmp_amount = e.field_val;
        e.field_idx = 21;
        e.get_misc_field(g);
        if Ext::from_f64(10.0) < e.field_val && Ext::from_f64(10.0) < tmp_amount && g.market_cap[c] < e.field_val + tmp_amount {
            e.set(e.line_no + 1, indent.clone() + __content.string(8499))?;
            e.set(
                e.line_no + 2,
                indent.clone()
                    + __content.string(8500)
                    + &cur(g)
                    + &e.field_text
                    + " "
                    + &g.million_prefix_uc
                    + __content.string(8501),
            )?;
            e.field_idx = 22;
            e.get_misc_field(g);
            e.set(
                e.line_no + 3,
                indent.clone()
                    + &cur(g)
                    + &e.field_text
                    + " "
                    + &g.million_prefix_uc
                    + __content.string(8502),
            )?;
            e.set(e.line_no + 4, indent.clone() + __content.string(8503))?;
            e.line_no += 5;
        }
        e.bond_buyback(g)?;
    }
    Ok(())
}
