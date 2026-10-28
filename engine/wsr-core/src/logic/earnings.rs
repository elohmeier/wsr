//! Earnings projection and the small per-entity money routines it uses: `CalcProjectedEarnings`
//! (0x494edb, with its GOSUBs `ProjEarn_FindIfPaid` / `ProjEarn_GetInterim`), cash/T-bill balancing,
//! adviser fees, bank deposit cost, CEO pay, goodwill write-downs, accrued bond interest and the
//! calendar helpers behind the "next earnings date" label.
//!
//! Numeric conventions as in `logic::finance`: EXT retains x87 precision. SINGLE constants widen
//! from f32, SINGLE fields round at explicit stores and DOUBLE locals use `numeric::double`.
//! Every float → integer store
//! uses `to_long` / `to_quad`. Association order follows the machine code; comments marked "asm:" note
//! where the decompiled view is misleading.

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
    clippy::neg_multiply
)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::finance::{
    calc_capital_taxes, calc_entity_financials, get_bank_cd_rate, get_controller_type, get_industry_category,
    get_managing_player, get_market_interest_rate,
};
use crate::logic::futures::{calc_phys_commod_storage_cost, estimate_corp_hedge_gains};
use crate::logic::news::post_news;
use crate::logic::options::calc_vested_options_value;
use crate::logic::swaps::{cancel_entity_swaps, process_swap_payments};
use crate::logic::valuation::calc_industry_outlook;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{mid, parse_any, replace, right};
use crate::pb::numeric::{ceil, frac, int, to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// SINGLE constant of the original's pool.
#[inline]
fn s(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

// ---------------------------------------------------------------------------------------------------------
// Cash / T-bills
// ---------------------------------------------------------------------------------------------------------

/// Port of BalanceCashTBills (0x69bb51).
/// Covers a cash overdraft from T-bills, then (for companies and computer players) moves 15% of the
/// T-bills to cash when T-bills exceed 80% of cash + T-bills (not for player-managed companies), or 20%
/// of cash into T-bills when cash exceeds 40%.
pub fn balance_cash_t_bills(g: &mut Globals, entity_ref: i32) {
    let e = entity_ref;
    if g.num_players < e && e < 11 {
        return;
    }
    if g.entity_status[e] != 1 {
        return;
    }
    if !(g.cash[e] > Ext::from_f64(0.0)) && !(g.t_bills[e] > Ext::from_f64(0.0)) {
        return;
    }
    if g.cash[e] < Ext::from_f64(0.0) && g.t_bills[e] > Ext::from_f64(0.0) {
        let overdraft = g.cash[e].abs();
        if !(g.t_bills[e] < overdraft) {
            g.t_bills[e] = g.t_bills[e] - overdraft;
            g.cash[e] = Ext::from_f64(0.0);
        } else {
            let t = g.t_bills[e];
            g.cash[e] = t + g.cash[e];
            g.t_bills[e] = Ext::from_f64(0.0);
            return;
        }
    }
    if e < 11 && g.is_computer_player[e] == 0 {
        return;
    }
    let total = g.t_bills[e] + g.cash[e];
    if g.t_bills[e] > total * s(0.8f32) {
        if e > 10 && get_managing_player(g, e) > 0 {
            return;
        }
        let t = g.t_bills[e] * s(0.15f32);
        g.t_bills[e] = g.t_bills[e] - t;
        g.cash[e] = t + g.cash[e];
    } else if g.cash[e] > total * s(0.4f32) {
        let t = g.cash[e] * s(0.2f32);
        g.cash[e] = g.cash[e] - t;
        g.t_bills[e] = t + g.t_bills[e];
    }
}

/// Port of SellTBillsToCash (0x69bf97).
/// Sells `amount` of an active entity's T-bills (at most all of them) and adds the proceeds to cash.
pub fn sell_t_bills_to_cash(g: &mut Globals, entity_ref: i32, amount: Ext) {
    let i = entity_ref;
    if g.entity_status[i] == 1 && g.t_bills[i] > Ext::from_f64(0.0) && amount > Ext::from_f64(0.0) {
        let mut sold = amount;
        if sold > g.t_bills[i] {
            sold = g.t_bills[i];
            g.t_bills[i] = Ext::from_f64(0.0);
        } else {
            g.t_bills[i] = g.t_bills[i] - sold;
        }
        g.cash[i] = sold + g.cash[i];
    }
}

// ---------------------------------------------------------------------------------------------------------
// Fees, deposit cost, CEO pay, goodwill
// ---------------------------------------------------------------------------------------------------------

/// Port of GetFundsAdvisedBy (0x64d4fe).
/// Counts the active index funds (1581..1600, industry 71) advised by `adviser_id`: returns the fund's id
/// if there is exactly one, the count if more, else 0.
pub fn get_funds_advised_by(g: &Globals, adviser_id: i32) -> i32 {
    let mut last_fund = 0;
    let mut fund_count = 0;
    for etf_id in 1581..=1600 {
        if adviser_id == g.fund_adviser[etf_id] && g.industry_num[etf_id] == 71 && g.entity_status[etf_id] == 1 {
            last_fund = etf_id;
            fund_count += 1;
        }
    }
    if fund_count > 1 { fund_count } else { last_fund }
}

/// Port of CalcAdviserFeeIncome (0x64d612).
/// Quarterly fee income of a fund adviser: `MarketCap * FundFeePct / 4` over its active funds whose fee
/// is within 0.2..1.0 %.
pub fn calc_adviser_fee_income(g: &Globals, adviser_id: i32) -> Ext {
    let mut total_fee = Ext::from_f64(0.0);
    for etf_id in 1581..=1600 {
        if adviser_id == g.fund_adviser[etf_id] && g.industry_num[etf_id] == 71 && g.entity_status[etf_id] == 1 {
            let mut fund_fee = Ext::from_f64(0.0);
            let pct = cast::<Ext, _>(g.fund_fee_pct[etf_id]);
            if !(s(0.2f32) > pct) && !(Ext::from_f64(1.0) < pct) {
                fund_fee = (s(0.25f32) * pct) * g.market_cap[etf_id];
            }
            total_fee = fund_fee + total_fee;
        }
    }
    total_fee
}

/// Port of CalcBankDepositCost (0x4a34a1).
/// A bank's quarterly overhead on deposits: `(demand + CDs + max(cust, 0)) * 0.25% * f` with the factor
/// `f = (50 - (ProfitMargin - DiscSpendPct)) / 25`, set to 3.8 above 4 and to 0.9 below 0.8.
pub fn calc_bank_deposit_cost(g: &Globals, bank_id: i32) -> Ext {
    let i = bank_id;
    let d = cast::<i32, _>(g.profit_margin[i]) - cast::<i32, _>(g.disc_spend_pct[i]);
    let mut factor: Ext = crate::pb::numeric::double(cast::<Ext, _>(50 - d) / Ext::from_f64(25.0)); // DOUBLE local
    if !(Ext::from_f64(4.0) >= factor) {
        factor = crate::pb::numeric::double(s(3.8f32));
    }
    if s(0.8f32) > factor {
        factor = crate::pb::numeric::double(s(0.9f32));
    }
    let mut cust = g.bank_cust_deposits[i];
    if cust < Ext::from_f64(0.0) {
        cust = Ext::from_f64(0.0);
    }
    let deposits = (g.bank_demand_deposits[i] + g.bank_c_ds[i]) + cust;
    (s(0.0025f32) * factor) * deposits
}

/// Port of CalcCeoPay (0x49287b).
/// The CEO's quarterly base salary for `corp_id` (0.06% of market value net of cash; banks, insurers and
/// industry 70 half that with a floor on reserves / net worth; at least `0.2 * ExchangeRate`). Sets
/// `CeoBonusMultiple` (0, 1, 1.5, 3, 5 …, doubled for a majority-owning player CEO) from the earnings
/// growth record during the player's tenure. `mode` 1 before Q4 returns the salary only.
pub fn calc_ceo_pay(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, mode: i32) -> Ext {
    g.ceo_bonus_multiple = Ext::from_f64(0.0);
    if 11 > corp_id || g.entity_status[corp_id] != 1 {
        return Ext::from_f64(0.0);
    }
    let mkt_value = g.market_cap[corp_id] * Ext::from_f64(100.0);
    calc_entity_financials(g, ui, rng, corp_id);
    let mut net_cash = (g.cash[corp_id] + g.t_bills[corp_id]) - g.tmp_total_debt;
    if g.industry_num[corp_id] != 2 {
        net_cash = g.tmp_subsid_stock_value * s(0.8f32) + net_cash;
    }
    if net_cash < Ext::from_f64(0.0) {
        net_cash = Ext::from_f64(0.0);
    }
    let mut base_salary = (mkt_value - net_cash) * s(0.0006f32);
    let ind = g.industry_num[corp_id];
    if ind < 3 || ind == 70 {
        let mut min_salary = Ext::from_f64(0.0);
        base_salary = s(0.5f32) * base_salary;
        if ind == 2 {
            min_salary = g.ins_reserves[corp_id] * s(0.0005f32);
        } else if ind == 1 || ind == 70 {
            min_salary = g.net_worth * s(0.00027f32);
        }
        if min_salary > base_salary {
            base_salary = min_salary;
        }
    }
    if base_salary < g.exchange_rate * s(0.2f32) {
        base_salary = g.exchange_rate * s(0.2f32);
    }
    if g.cur_quarter < 4 && mode == 1 {
        return base_salary;
    }

    let mut ceo_since_year = 0;
    let mut ceo_player = 0;
    let n = g.num_players;
    for player_id in 1..=n {
        if corp_id == g.player_ceo_of[player_id] {
            ceo_since_year = g.player_ceo_since_year[player_id];
            ceo_player = player_id;
        }
    }
    if ceo_player == 0 {
        return base_salary;
    }
    if ceo_since_year == 0 {
        ceo_since_year = g.cur_year;
    }

    let mut bonus_years = 0;
    let c = corp_id;
    if mode == 0 {
        if g.earnings_ytd[c] == Ext::from_f64(0.0) {
            if g.proj_qtr_earnings[c] > Ext::from_f64(0.0) && g.earnings_y1[c] * s(1.15f32) < Ext::from_f64(4.0) * g.proj_qtr_earnings[c] {
                bonus_years = 1;
            }
            if g.cur_quarter == 4 {
                if !(g.earnings_y1[c] < Ext::from_f64(0.0)) && !(g.earnings_y1[c] < g.earnings_yr2_ago[c] * s(1.15f32)) {
                    bonus_years = 1;
                } else {
                    bonus_years = 0;
                }
            }
        } else {
            let mut qtrs_done = g.cur_quarter - 1;
            if g.sim_co_count > corp_id {
                qtrs_done = g.cur_quarter;
            }
            let qtrs_left = 4 - qtrs_done;
            let proj = cast::<Ext, _>(qtrs_left) * g.proj_qtr_earnings[c] + g.earnings_ytd[c];
            if g.earnings_y1[c] * s(1.15f32) < proj {
                bonus_years = 1;
            }
        }
    } else if g.cur_quarter == 4
        && g.earnings_ytd[c] == Ext::from_f64(0.0)
        && g.earnings_y1[c] > Ext::from_f64(0.0)
        && g.earnings_y1[c] > g.earnings_yr2_ago[c] * s(1.15f32)
    {
        bonus_years = 1;
    }

    let mut hist_mode = if mode == 0 {
        if g.earnings_ytd[c] == Ext::from_f64(0.0) { 2 } else { 1 }
    } else {
        2
    };
    if g.cur_year == ceo_since_year {
        bonus_years = 0;
    }
    if bonus_years == 0 {
        hist_mode = 0;
    }
    if hist_mode == 1 {
        if g.earnings_y1[c] > g.earnings_yr2_ago[c] * s(1.15f32) && g.cur_year - 2 > ceo_since_year {
            bonus_years = 3;
        }
        if bonus_years == 3 && g.earnings_yr2_ago[c] > g.earnings_yr3_ago[c] * s(1.15f32) && g.cur_year - 3 > ceo_since_year {
            bonus_years = 5;
        }
    } else if hist_mode == 2 {
        let q1 = if g.cur_quarter == 1 { 1 } else { 0 };
        if g.earnings_yr2_ago[c] > g.earnings_yr3_ago[c] * s(1.15f32) && g.cur_year - (q1 + 2) > ceo_since_year {
            bonus_years = 3;
        }
        if bonus_years == 3
            && g.earnings_yr3_ago[c] > g.earnings_yr4_ago[c] * s(1.15f32)
            && g.cur_year - (q1 + 3) > ceo_since_year
        {
            bonus_years = 5;
        }
    }
    g.ceo_bonus_multiple = if bonus_years == 1 { s(1.5f32) } else { cast::<Ext, _>(bonus_years) };

    update_corp_control(g, ui, rng, corp_id);
    if g.voting_controller <= 0
        || g.num_players < g.voting_controller
        || g.largest_holder_pct <= 50
        || g.voting_controller != ceo_player
    {
        return base_salary;
    }
    if bonus_years > 1 {
        g.ceo_bonus_multiple = cast::<Ext, _>(bonus_years) * Ext::from_f64(2.0);
    }
    if bonus_years == 1 {
        g.ceo_bonus_multiple = Ext::from_f64(3.0);
    }
    if bonus_years == 0 {
        g.ceo_bonus_multiple = Ext::from_f64(1.0);
    }
    base_salary
}

/// Port of GoodwillWriteDown (0x5053f8).
/// The quarter's goodwill amortization of company `corp_id` (> 130): all of it when small (< 10 *
/// ExchangeRate, < 1% of business assets or < 1), 5.4% when within twice the business assets, else a
/// "large write-down" of 10% (profitable) / 20% (at least 1) with news unless `preview_only`. Negative
/// goodwill is booked to `ExtraordinaryPL` and cleared (unless `preview_only`), returning 0. RND is drawn
/// only for a large write-down when not previewing.
pub fn goodwill_write_down(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, preview_only: i32) -> Ext {
    let __content = crate::content::current();
    // ON ERROR -> ShowRuntimeError (not ported)
    if 131 > corp_id || g.goodwill[corp_id] == Ext::from_f64(0.0) {
        return Ext::from_f64(0.0);
    }
    let c = corp_id;
    let mut write_down = Ext::from_f64(0.0);
    if g.goodwill[c] < Ext::from_f64(0.0) {
        if preview_only == 0 {
            g.extraordinary_pl[c] = to_quad(cast::<Ext, _>(g.extraordinary_pl[c]) - g.goodwill[c]);
            g.goodwill[c] = Ext::from_f64(0.0);
        }
        return write_down;
    }
    let mut msg_text = PbStr::new();
    if g.goodwill[c] < g.exchange_rate * Ext::from_f64(10.0)
        || g.goodwill[c] < g.business_assets[c] * s(0.01f32)
        || g.goodwill[c] < Ext::from_f64(1.0)
    {
        write_down = g.goodwill[c];
    } else if !(g.goodwill[c] > g.business_assets[c] * Ext::from_f64(2.0)) {
        write_down = g.goodwill[c] * s(0.054f32);
    } else {
        let mut write_rate: Ext = crate::pb::numeric::double(s(0.2f32)); // DOUBLE local
        if g.earnings_y1[c] > Ext::from_f64(0.0) || g.earnings_ytd[c] > Ext::from_f64(0.0) {
            write_rate = crate::pb::numeric::double(s(0.1f32));
        }
        write_down = g.goodwill[c] * write_rate;
        if write_down < Ext::from_f64(1.0) {
            write_down = Ext::from_f64(1.0);
        }
        if preview_only == 0 {
            if 7 > g.credit_rating_num[c] {
                msg_text = g.ent_symbol[c].clone() + __content.string(4364);
            }
            if s(0.15f32) > write_rate {
                msg_text = PbStr::from(__content.string(4365)) + &g.ent_symbol[c] + __content.string(4366);
            }
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                msg_text = replace(msg_text.as_bytes(), __content.bytes(4367), __content.bytes(4368));
            }
        }
    }
    if !msg_text.is_empty() {
        g.news_msg = msg_text;
        post_news(g, ui, c, 0);
    }
    write_down
}

/// Port of CalcAccruedBondInterest (0x4a3616).
/// Interest accrued in the current quarter on `face_value` of a bond: issuer 1 / 2 = the long / short
/// government bond, else the corporate bond of `issuer_ref` (0 if rated below 2). The accrual fraction
/// is the simulation position (`SimCoCount`, capped at 1500) relative to the issuer's slot in the
/// 1500-step quarter cycle.
pub fn calc_accrued_bond_interest(g: &Globals, face_value: i64, issuer_ref: i32) -> Ext {
    let mut cur_tick = g.sim_co_count;
    if cur_tick > 1500 {
        cur_tick = 1500;
    }
    let mut issuer_id = issuer_ref;
    if issuer_id > 1500 {
        issuer_id = 1500;
    }
    let ticks_elapsed;
    let coupon_str;
    if issuer_id < 3 {
        if cur_tick == 1500 {
            cur_tick = 1496;
        }
        if g.sim_co_count < 11 {
            cur_tick = 4;
        }
        ticks_elapsed = cur_tick;
        coupon_str = if issuer_id == 1 {
            right(g.long_bond_desc.as_bytes(), 5)
        } else {
            right(g.short_bond_desc.as_bytes(), 5)
        };
    } else {
        if 2 > g.credit_rating_num[issuer_ref] {
            return Ext::from_f64(0.0);
        }
        if issuer_id == 1500 && cur_tick == 1500 {
            ticks_elapsed = if g.sim_co_count > issuer_id { 4 } else { 1496 };
        } else if issuer_id > cur_tick {
            ticks_elapsed = cur_tick + (1500 - issuer_id);
        } else {
            ticks_elapsed = cur_tick - issuer_id;
        }
        coupon_str = right(g.bond_issue_info[issuer_ref].as_bytes(), 5);
    }
    let coupon_rate: Ext = crate::pb::numeric::double(val(coupon_str.as_bytes())); // DOUBLE local
    let period_interest = crate::pb::numeric::double((s(0.0025f32) * coupon_rate) * cast::<Ext, _>(face_value));
    let accrual_frac: Ext = crate::pb::numeric::double(cast::<Ext, _>(ticks_elapsed) / Ext::from_f64(1500.0)); // DOUBLE local
    period_interest * accrual_frac
}

// ---------------------------------------------------------------------------------------------------------
// Calendar
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcCurMonthOfYear (0x5f3697).
/// Month position (0..12, DOUBLE) of simulation position `progress` (500 steps per month, 3 months per
/// quarter). Side effect: a `CurQuarter` below 2 is set to 1.
pub fn calc_cur_month_of_year(g: &mut Globals, progress: i32) -> Ext {
    if g.cur_quarter < 2 {
        g.cur_quarter = 1;
    }
    let mut month_pos: Ext = crate::pb::numeric::double(cast::<Ext, _>(progress) / Ext::from_f64(500.0));
    if s(0.01f32) > month_pos && g.cur_quarter < 2 {
        month_pos = crate::pb::numeric::double(s(0.01f32));
    }
    if !(Ext::from_f64(3.0) >= month_pos) {
        month_pos = crate::pb::numeric::double(Ext::from_f64(3.0));
    }
    if g.num_players + 1 > progress {
        month_pos = crate::pb::numeric::double(Ext::from_f64(3.0));
    }
    if g.cur_quarter > 1 {
        month_pos = crate::pb::numeric::double((cast::<Ext, _>(g.cur_quarter) - Ext::from_f64(1.0)) * Ext::from_f64(3.0) + month_pos);
    }
    crate::pb::numeric::double(month_pos)
}

/// Port of CalcDayOfMonth (0x4a0383).
/// Day of `month` reached at fraction `month_frac` of the month (at least 1); leaves the month's length in
/// `TmpExt1` (February has 29 days when `CurYear MOD 4 = 0`).
pub fn calc_day_of_month(g: &mut Globals, month: i32, month_frac: Ext) -> i32 {
    g.tmp_ext1 = Ext::from_f64(0.0);
    let m = cast::<Ext, _>(month);
    let day_raw: Ext; // DOUBLE local
    let days_in_month;
    if m == Ext::from_f64(4.0) || m == Ext::from_f64(6.0) || m == Ext::from_f64(9.0) || m == Ext::from_f64(11.0) {
        day_raw = crate::pb::numeric::double((Ext::from_f64(500.0) * month_frac) / Ext::from_f64(16.66667));
        days_in_month = crate::pb::numeric::double(30);
    } else if m == Ext::from_f64(2.0) {
        if g.cur_year % 4 == 0 {
            day_raw = crate::pb::numeric::double((Ext::from_f64(500.0) * month_frac) / s(17.2415f32));
            days_in_month = crate::pb::numeric::double(29);
        } else {
            day_raw = crate::pb::numeric::double((Ext::from_f64(500.0) * month_frac) / Ext::from_f64(17.85715));
            days_in_month = crate::pb::numeric::double(28);
        }
    } else {
        day_raw = crate::pb::numeric::double((Ext::from_f64(500.0) * month_frac) / s(16.1291f32));
        days_in_month = crate::pb::numeric::double(31);
    }
    let mut day = to_long(ceil(day_raw));
    if day == 0 {
        day = 1;
    }
    g.tmp_ext1 = cast::<Ext, _>(days_in_month);
    day
}

/// Calendar readout of UpdateDateDisplay (0x4a054a), shared with the financial reports.
/// Integer month positions and the player/quarter-end slots belong to the month's last day.
/// Like the original display routine, leaves `TmpExt1` cleared.
pub fn current_game_date(g: &mut Globals) -> (i32, i32) {
    let progress = g.sim_co_count;
    let pos = calc_cur_month_of_year(g, progress);
    let month = to_long(ceil(pos));
    let mut day = calc_day_of_month(g, month, frac(pos));
    if progress > 1499 || progress <= g.num_players || progress == 500 || progress == 1000 {
        day = to_long(g.tmp_ext1);
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    (month, day)
}

/// Port of GetNextEarningsDateStr (0x4a07e4).
/// Date of company `corp_id`'s next earnings release ("Month day, year"): its slot in the quarter cycle,
/// moved to next quarter once the simulation has passed it; companies 500, 1000 and >= 1500 report on the
/// last day of the month. Side effect: `TmpExt1` = 0 (and `CurQuarter` < 2 set to 1).
pub fn get_next_earnings_date_str(g: &mut Globals, corp_id: i32) -> PbStr {
    let cur_month_pos = calc_cur_month_of_year(g, g.sim_co_count);
    let mut corp_month_pos = calc_cur_month_of_year(g, corp_id);
    if !(cur_month_pos < corp_month_pos) && g.num_players + 1 != g.sim_co_count {
        corp_month_pos = Ext::from_f64(3.0) + corp_month_pos;
    }
    let mut year = g.cur_year;
    if !(Ext::from_f64(12.0) >= corp_month_pos) {
        corp_month_pos = corp_month_pos - Ext::from_f64(12.0);
        year += 1;
    }
    let month = to_long(ceil(corp_month_pos));
    let month_frac: Ext = frac(corp_month_pos);
    let mut day = calc_day_of_month(g, month, month_frac);
    let days_in_month = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if corp_id >= 1500 {
        day = days_in_month;
    }
    if corp_id == 500 || corp_id == 1000 {
        day = days_in_month;
    }
    g.month_name[month].clone()
        + str_num(cast::<Ext, _>(day), 15).as_str()
        + ","
        + str_num(cast::<Ext, _>(year), 15).as_str()
}

// ---------------------------------------------------------------------------------------------------------
// CalcProjectedEarnings
// ---------------------------------------------------------------------------------------------------------

/// Locals of CalcProjectedEarnings shared with its GOSUBs.
#[derive(Default)]
struct Frame {
    corp_id: i32,
    /// QUAD: the held company / bond issuer id; also the GOSUB result of `ProjEarn_GetInterim`.
    held_id: i64,
    /// QUAD flag set by `ProjEarn_FindIfPaid`.
    incl_earn: i64,
}

/// Port of ProjEarn_FindIfPaid (0x498782), GOSUB of [`calc_projected_earnings`]: whether the held company
/// still reports earnings this quarter (`inclEarn`). Companies report in id order up to `SimCoCount`.
fn proj_earn_find_if_paid(g: &Globals, f: &mut Frame) {
    let held = cast::<Ext, _>(f.held_id);
    let sim = cast::<Ext, _>(g.sim_co_count);
    let corp = cast::<Ext, _>(f.corp_id);
    if f.corp_id <= g.sim_co_count {
        if !(held < sim) || held < corp {
            f.incl_earn = 1;
        } else if held > corp && held < sim {
            f.incl_earn = 0;
        }
    } else if !(held > sim) || held > corp {
        f.incl_earn = 0;
    } else if held > sim && held < corp {
        f.incl_earn = 1;
    }
    // (debug MSGBOX when debugMode = 1: the local is never set)
}

/// Port of ProjEarn_GetInterim (0x4988cb), GOSUB of [`calc_projected_earnings`]: the number of 290-id
/// processing loops left until the company reports (into `heldId`).
fn proj_earn_get_interim(g: &Globals, f: &mut Frame) {
    let mut corp_loop = to_long(int(cast::<Ext, _>(f.corp_id) / Ext::from_f64(290.0)));
    let cur_loop = to_long(int(cast::<Ext, _>(g.sim_co_count) / Ext::from_f64(290.0)));
    if f.corp_id <= g.sim_co_count {
        corp_loop += 5;
    }
    f.held_id = to_quad(cast::<Ext, _>(corp_loop - cur_loop));
}

/// Port of CalcProjectedEarnings (0x494edb) with its GOSUBs ProjEarn_FindIfPaid (0x498782) and
/// ProjEarn_GetInterim (0x4988cb).
/// Projects company `entity_id`'s net [runtime resource wording]  (equity income and dividends from holdings
/// not yet reported, bond and T-bill interest, operating earnings, storage, bank/insurance income, interest
/// expense, CEO pay, options, taxes, randomized ±10% and interpolated from the previous projection) and
/// stores it in `ProjQtrEarnings`. Side effects: `ReserveAdjPending` (earnings smoothing via the
/// contingency reserve), `CeoBonusMultiple`, swap cancellation for big swap losses of weak companies,
/// cash/T-bill balancing, the financial summary globals (CalcEntityFinancials) and control globals
/// (UpdateCorpControl). Companies just reported (`SimCoCount < id < SimCoCount + 12`) return the stored
/// projection unchanged; players, inactive entities and funds (industry 71) return 0.
///
/// The original's `debugMode` local (MSGBOX traces) is never set; the traces are omitted.
pub fn calc_projected_earnings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) -> Ext {
    // ON ERROR -> ShowRuntimeError (not ported)
    let mut f = Frame { corp_id: entity_id, ..Default::default() };
    let corp_id = entity_id;
    if corp_id < 11 || g.entity_status[corp_id] != 1 {
        return Ext::from_f64(0.0);
    }
    if g.industry_num[corp_id] == 71 {
        return Ext::from_f64(0.0);
    }
    if g.sim_co_count < corp_id && corp_id - g.sim_co_count < 12 {
        return g.proj_qtr_earnings[corp_id];
    }

    // EXT locals (zeroed by the frame setup)
    let mut divd1 = Ext::from_f64(0.0);
    let mut divd2 = Ext::from_f64(0.0);
    let mut proj_equity = Ext::from_f64(0.0);
    let mut proj_reg = Ext::from_f64(0.0);
    let mut amt;
    let mut amt_b;
    let mut amt_c;
    let mut equity_share;
    let mut qtrs_left: i64;
    let mut coupon_rate: f32;

    // ---- equity income and dividends from stock holdings (FOR idx = 1 TO 15)
    for idx in 1..=15 {
        let mut use_equity_method: i64 = 1;
        f.incl_earn = 0;
        f.held_id = cast::<i64, _>(g.hold_stock_id[(idx, corp_id)]);
        let held_pct = cast::<i32, _>(g.hold_stock_pct[(idx, corp_id)]);
        if f.held_id == 0 {
            continue;
        }
        if held_pct < 20 {
            use_equity_method = 0;
        }
        proj_earn_find_if_paid(g, &mut f);
        if f.incl_earn == 0 {
            continue;
        }
        let h = cast::<i32, _>(f.held_id);
        if use_equity_method == 1 {
            if g.proj_qtr_earnings[h] == Ext::from_f64(0.0) {
                let a = if g.ent_misc_data[h].len() > 20 {
                    val(parse_any(g.ent_misc_data[h].as_bytes(), b"|", 3).as_bytes())
                } else {
                    g.earnings_y1[h] * s(0.25f32)
                };
                equity_share = (a * s(0.01f32)) * cast::<Ext, _>(held_pct);
            } else {
                equity_share = (g.proj_qtr_earnings[h] * s(0.01f32)) * cast::<Ext, _>(held_pct);
            }
            proj_equity = equity_share + proj_equity;
            amt = (g.dividend_per_share[h] * Ext::from_f64(25.0)) * g.shares_outstanding[h];
            amt_b = (amt * s(0.01f32)) * cast::<Ext, _>(held_pct);
            if held_pct < 20 {
                divd2 = amt_b + divd2;
            } else if held_pct < 80 {
                divd1 = amt_b + divd1;
            }
        } else {
            amt = (g.dividend_per_share[h] * Ext::from_f64(25.0)) * g.shares_outstanding[h];
            amt_b = (amt * s(0.01f32)) * cast::<Ext, _>(held_pct);
            divd2 = amt_b + divd2;
        }
    }
    // asm: the view folds these into temporaries; both are plain accumulations.
    divd1 = g.div_recvd1[corp_id] + divd1;
    divd2 = g.div_recvd2[corp_id] + divd2;

    // ---- interest rate swaps (projection mode "P")
    let swap_profit = to_quad(process_swap_payments(g, ui, rng, f.corp_id, &PbStr::from("P")));
    if corp_id <= g.sim_co_count {
        proj_reg = cast::<Ext, _>(swap_profit) + proj_reg;
    }

    // ---- bond interest and amortization (banks and insurers hold bonds)
    if corp_id < 131 {
        for idx in 1..=15 {
            f.incl_earn = 0;
            f.held_id = cast::<i64, _>(g.hold_bond_id[(idx, corp_id)]);
            if f.held_id == 0 {
                continue;
            }
            proj_earn_find_if_paid(g, &mut f);
            if f.incl_earn == 0 {
                continue;
            }
            let h = cast::<i32, _>(f.held_id);
            coupon_rate = cast::<f32, _>(s(0.01f32) * val(right(g.bond_issue_info[h].as_bytes(), 5).as_bytes()));
            amt_b = (cast::<Ext, _>(coupon_rate) * cast::<Ext, _>(g.hold_bond_face[(idx, corp_id)])) * s(0.25f32);
            proj_reg = amt_b + proj_reg;
            let maturity_year = to_long(val(mid(g.bond_issue_info[h].as_bytes(), 1, 4).as_bytes()));
            qtrs_left = to_quad(cast::<Ext, _>(maturity_year - g.cur_year) * Ext::from_f64(4.0));
            qtrs_left = to_quad(cast::<Ext, _>(g.cur_quarter - 1) + cast::<Ext, _>(qtrs_left));
            amt_b = cast::<Ext, _>(g.hold_bond_face[(idx, corp_id)]) - g.bond_hold_cost[(idx, corp_id)];
            amt = if amt_b == Ext::from_f64(0.0) || !(cast::<Ext, _>(qtrs_left) > Ext::from_f64(0.0)) { Ext::from_f64(0.0) } else { amt_b / cast::<Ext, _>(qtrs_left) };
            proj_reg = amt + proj_reg;
        }
        if corp_id < g.sim_co_count {
            coupon_rate = cast::<f32, _>(s(0.01f32) * val(right(g.long_bond_desc.as_bytes(), 5).as_bytes()));
            amt_b = (cast::<Ext, _>(coupon_rate) * cast::<Ext, _>(g.long_gov_bond_face[corp_id])) * s(0.25f32);
            coupon_rate = cast::<f32, _>(s(0.01f32) * val(right(g.short_bond_desc.as_bytes(), 5).as_bytes()));
            amt_c = (cast::<Ext, _>(coupon_rate) * cast::<Ext, _>(g.short_gov_bond_face[corp_id])) * s(0.25f32);
            proj_reg = (amt_b + proj_reg) + amt_c; // asm: (ProjReg + TBond) + SBond
            let maturity_year = to_long(val(mid(g.long_bond_desc.as_bytes(), 1, 4).as_bytes()));
            qtrs_left = to_quad(cast::<Ext, _>(maturity_year - g.cur_year) * Ext::from_f64(4.0));
            qtrs_left = to_quad(cast::<Ext, _>(g.cur_quarter - 1) + cast::<Ext, _>(qtrs_left));
            amt_b = cast::<Ext, _>(g.long_gov_bond_face[corp_id]) - g.lt_gov_bond_cost[corp_id];
            amt = if amt_b == Ext::from_f64(0.0) || !(cast::<Ext, _>(qtrs_left) > Ext::from_f64(0.0)) { Ext::from_f64(0.0) } else { amt_b / cast::<Ext, _>(qtrs_left) };
            proj_reg = amt + proj_reg;
            qtrs_left = to_quad(cast::<Ext, _>(qtrs_left) - Ext::from_f64(40.0));
            amt_b = cast::<Ext, _>(g.short_gov_bond_face[corp_id]) - g.st_gov_bond_cost[corp_id];
            amt = if amt_b == Ext::from_f64(0.0) || !(cast::<Ext, _>(qtrs_left) > Ext::from_f64(0.0)) { Ext::from_f64(0.0) } else { amt_b / cast::<Ext, _>(qtrs_left) };
            proj_reg = amt + proj_reg;
        }
    }

    // ---- quarters left to report
    proj_earn_get_interim(g, &mut f);
    let int_calcs = to_long(cast::<Ext, _>(f.held_id));

    // ---- operating earnings (and goodwill amortization) of operating companies
    if corp_id > 130 && g.industry_num[corp_id] < 70 {
        if g.ent_misc_data[corp_id].len() > 20 {
            amt = val(parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 30).as_bytes());
        } else {
            let ind = g.industry_num[corp_id];
            calc_industry_outlook(g, rng, ind);
            amt = g.tmp_ext1;
            amt = cast::<Ext, _>(g.profit_margin[corp_id]) + amt;
            amt = amt - cast::<Ext, _>(g.disc_spend_pct[corp_id]);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
        }
        amt = (amt * s(0.01f32)) * s(0.04f32);
        amt_b = (g.business_assets[corp_id] * amt) * cast::<Ext, _>(int_calcs);
        amt_c = (amt * s(1.25f32)) * g.business_assets[corp_id];
        amt = amt_c + amt_b;
        if g.growth_rate[corp_id] > 30 && int_calcs > 3 {
            amt = s(1.1f32) * amt;
        }
        proj_reg = amt + proj_reg;
        if g.goodwill[corp_id] > Ext::from_f64(0.0) {
            amt = goodwill_write_down(g, ui, rng, f.corp_id, 1);
            proj_reg = proj_reg - amt;
        }
    }

    // ---- physical commodity storage fees
    if corp_id > 130 && g.industry_num[corp_id] < 71 {
        amt = calc_phys_commod_storage_cost(g, f.corp_id);
        if amt != Ext::from_f64(0.0) {
            amt = amt / Ext::from_f64(12.0);
            if int_calcs < 2 {
                amt = Ext::from_f64(0.0);
            } else if int_calcs < 3 {
            } else if int_calcs < 5 {
                amt = Ext::from_f64(2.0) * amt;
            } else {
                amt = Ext::from_f64(3.0) * amt;
            }
            proj_reg = proj_reg - amt;
        }
    }

    calc_entity_financials(g, ui, rng, f.corp_id);
    // weak companies with large projected swap losses get out of their swaps
    if (cast::<Ext, _>(swap_profit)) < g.exchange_rate * -Ext::from_f64(100.0) {
        if g.credit_rating_num[corp_id] == 0 {
            cancel_entity_swaps(g, ui, rng, f.corp_id, -1, 2);
        } else if corp_id > 130 && g.total_assets < g.tmp_total_debt * s(0.9f32) {
            cancel_entity_swaps(g, ui, rng, f.corp_id, -1, 2);
        } else if corp_id < 131 && g.credit_rating_num[corp_id] == 1 {
            cancel_entity_swaps(g, ui, rng, f.corp_id, -1, 2);
        }
    }
    let futures_value = g.futures_net_value;
    let net_worth = g.net_worth;
    let mut borrow_rate: f32 = cast::<f32, _>(g.borrow_rate);
    if g.industry_num[corp_id] == 1 {
        borrow_rate = g.entity_borrow_rate;
        // custDeposits = max(BankCustDeposits, 0): computed but never used
    }

    // ---- T-bill interest
    balance_cash_t_bills(g, f.corp_id);
    amt = if g.t_bills[corp_id] > Ext::from_f64(0.0) { s(0.0025f32) * cast::<Ext, _>(g.t_bill_rate) } else { Ext::from_f64(0.0) };
    amt_b = (g.t_bills[corp_id] * amt) * s(0.2f32);
    amt_b = amt_b * cast::<Ext, _>(int_calcs);
    proj_reg = amt_b + proj_reg;

    // ---- subprime securities (banks and insurers)
    equity_share = Ext::from_f64(0.0);
    if g.industry_num[corp_id] < 3 {
        if g.subprime_crisis_state > 0 && g.industry_num[corp_id] == 2 {
            amt_b = cast::<Ext, _>(g.subprime_secs[corp_id]) * s(0.05f32);
            proj_reg = proj_reg - amt_b;
        }
        if cast::<Ext, _>(g.subprime_secs[corp_id]) > Ext::from_f64(0.0) {
            let rate = get_market_interest_rate(g, rng, 3);
            equity_share = (s(0.0005f32) * rate) * cast::<Ext, _>(g.subprime_secs[corp_id]);
        }
    }

    // ---- insurance underwriting profit
    if g.industry_num[corp_id] == 2 {
        qtrs_left = cast::<i64, _>(g.growth_rate[corp_id]);
        amt_c = (cast::<Ext, _>(qtrs_left) * s(0.7f32)) * -Ext::from_f64(1.0);
        let costs = (Ext::from_f64(1.0) + cast::<Ext, _>(g.disc_spend_pct[corp_id])) + cast::<Ext, _>(g.difficulty_level);
        amt_c = (amt_c + cast::<Ext, _>(g.profit_margin[corp_id])) - costs;
        amt = (g.ins_reserves[corp_id] * (s(0.01f32) * amt_c)) * s(0.05f32);
        amt = (equity_share + amt) * cast::<Ext, _>(int_calcs);
        proj_reg = amt + proj_reg;
    }

    // ---- bank: loan interest, bad debt provision, CD interest, overhead
    if g.industry_num[corp_id] == 1 {
        amt_b = s(0.0005f32) * get_market_interest_rate(g, rng, 1);
        amt = amt_b * cast::<Ext, _>(g.bank_consumer_loans[corp_id]);
        amt_c = s(0.0005f32) * get_market_interest_rate(g, rng, 2);
        amt = amt_c * cast::<Ext, _>(g.bank_mortgage_loans[corp_id]) + amt;
        amt = equity_share + amt;
        let mut good_loans: i64 = 0; // heldId: loans to borrowers rated 0/1 (the weakest)
        let mut doubtful_loans: i64 = 0; // qtrsLeft: loans to borrowers rated 2
        amt_b = Ext::from_f64(0.0);
        for idx in 1..=1600 {
            if cast::<i32, _>(g.lending_bank[idx]) != corp_id || !(g.bank_debt[idx] > Ext::from_f64(0.0)) {
                continue;
            }
            calc_entity_financials(g, ui, rng, idx);
            if 2 > g.credit_rating_num[idx] {
                good_loans = to_quad(cast::<Ext, _>(good_loans) + g.bank_debt[idx]);
            }
            if g.credit_rating_num[idx] == 2 {
                doubtful_loans = to_quad(cast::<Ext, _>(doubtful_loans) + g.bank_debt[idx]);
            }
            coupon_rate = g.borrow_rate;
            if idx > 10 && idx < 86 {
                coupon_rate = g.entity_borrow_rate;
            }
            amt_c = (cast::<Ext, _>(coupon_rate) * g.bank_debt[idx]) * s(0.0005f32);
            amt_b = amt_c + amt_b;
        }
        calc_entity_financials(g, ui, rng, f.corp_id);
        g.bank_largest_loan = g.bank_largest_loan - g.bank_loans_total * s(0.25f32);
        if g.bank_largest_loan < Ext::from_f64(0.0) {
            g.bank_largest_loan = Ext::from_f64(0.0);
        }
        let mut bad_debt_need = cast::<Ext, _>(good_loans) * s(0.2f32) + g.tmp_bank_total_loans * s(0.005f32);
        bad_debt_need = cast::<Ext, _>(doubtful_loans) * s(0.03f32) + bad_debt_need;
        bad_debt_need = cast::<Ext, _>(g.bank_consumer_loans[corp_id]) * s(0.06f32) + bad_debt_need;
        bad_debt_need = g.bank_largest_loan * s(0.1f32) + bad_debt_need;
        bad_debt_need = cast::<Ext, _>(g.subprime_secs[corp_id]) * s(0.03f32) + bad_debt_need;
        amt = amt_b + amt;
        let loan_qtr: i64 = if corp_id <= g.sim_co_count { 2 } else { 1 };
        if cast::<Ext, _>(loan_qtr) + (cast::<Ext, _>(corp_id) % Ext::from_f64(4.0)) == cast::<Ext, _>(g.cur_quarter) {
            amt_b = (s(0.01f32) * ((Ext::from_f64(7.0) - cast::<Ext, _>(g.gdp_growth)) + Ext::from_f64(5.0))) * cast::<Ext, _>(g.bank_consumer_loans[corp_id]);
            amt_c = (s(0.01f32) * ((Ext::from_f64(6.0) - cast::<Ext, _>(g.gdp_growth)) * s(0.1f32))) * cast::<Ext, _>(g.subprime_secs[corp_id])
                + s(0.3f32);
            bad_debt_need = (amt_b + bad_debt_need) + amt_c;
        }
        if g.subprime_crisis_state > 0 {
            bad_debt_need = cast::<Ext, _>(g.subprime_secs[corp_id]) * s(0.05f32) + bad_debt_need;
        }
        amt_b = (s(0.0005f32) * get_bank_cd_rate(g, f.corp_id)) * g.bank_c_ds[corp_id];
        amt = amt - amt_b;
        amt = amt * cast::<Ext, _>(int_calcs);
        proj_reg = amt + proj_reg;
        amt_b = Ext::from_f64(0.0);
        let mut bad_debt_res = g.bad_debt_reserve[corp_id];
        if bad_debt_res < Ext::from_f64(0.0) {
            amt_b = bad_debt_res - Ext::from_f64(250.0);
            bad_debt_res = Ext::from_f64(0.0);
        }
        amt = (bad_debt_need - bad_debt_res) * s(0.25f32);
        amt = amt - amt_b;
        proj_reg = proj_reg - amt;
        amt = s(0.2f32) * calc_bank_deposit_cost(g, f.corp_id);
        amt = amt * cast::<Ext, _>(int_calcs);
        proj_reg = proj_reg - amt;
    }

    // ---- interest expense: bank loans, player advances, bonds issued
    if g.bank_debt[corp_id] > Ext::from_f64(0.0) {
        amt_b = (((s(0.01f32) * cast::<Ext, _>(borrow_rate)) * s(0.25f32)) * s(0.2f32)) * g.bank_debt[corp_id];
        amt = amt_b * cast::<Ext, _>(int_calcs);
        proj_reg = proj_reg - amt;
    }
    if g.tmp_player_advances > Ext::from_f64(0.0) {
        amt_b = (s(0.0005f32) * cast::<Ext, _>(g.prime_rate)) * g.tmp_player_advances;
        amt = amt_b * cast::<Ext, _>(int_calcs);
        proj_reg = proj_reg - amt;
    }
    if cast::<Ext, _>(g.bonds_issued[corp_id]) > Ext::from_f64(0.0) {
        coupon_rate = cast::<f32, _>(s(0.01f32) * val(right(g.bond_issue_info[corp_id].as_bytes(), 5).as_bytes()));
        amt = (s(0.25f32) * cast::<Ext, _>(coupon_rate)) * cast::<Ext, _>(g.bonds_issued[corp_id]);
        proj_reg = proj_reg - amt;
    }

    // ---- a player's CEO salary and bonus
    let n = g.num_players;
    for idx in 1..=n {
        if g.player_name[idx] == "...OUT..." || corp_id != g.player_ceo_of[idx] {
            continue;
        }
        amt = calc_ceo_pay(g, ui, rng, f.corp_id, 0);
        g.ceo_bonus_multiple = amt * g.ceo_bonus_multiple;
        amt = s(0.25f32) * amt;
        proj_reg = proj_reg - amt;
        if g.cur_quarter == 4 {
            proj_reg = proj_reg - g.ceo_bonus_multiple;
        }
        break;
    }

    amt = calc_vested_options_value(g, f.corp_id);
    proj_reg = proj_reg - amt;
    if cast::<i32, _>(g.superfund_corp) == corp_id {
        proj_reg = proj_reg - g.market_cap[corp_id] * (cast::<Ext, _>(g.difficulty_level) + Ext::from_f64(3.0));
    }
    proj_reg = g.accrued_pl[corp_id] + proj_reg;
    if futures_value != Ext::from_f64(0.0) {
        proj_reg = estimate_corp_hedge_gains(g, ui, rng, f.corp_id) + proj_reg;
    }
    if g.industry_num[corp_id] == 37 || g.industry_num[corp_id] == 2 {
        let mut fee_income = calc_adviser_fee_income(g, f.corp_id);
        if g.sim_co_count < corp_id {
            fee_income = Ext::from_f64(0.0);
        }
        proj_reg = fee_income + proj_reg;
    }
    proj_equity = g.equity_income_ytd[corp_id] + proj_equity;

    // ---- taxes
    // asm: TxblInc = (Divd2 * 0.5 + ProjReg) + Divd1 * 0.35 (the view's DivRecvd1 term is already in Divd1)
    let mut taxable = (divd2 * s(0.5f32) + proj_reg) + divd1 * s(0.35f32);
    proj_reg = divd2 + proj_reg;
    let mut proj_tax = (s(0.01f32) * cast::<Ext, _>(g.corp_tax_rate)) * proj_reg;
    taxable = cast::<Ext, _>(g.extraordinary_pl[corp_id]) + taxable;
    let max_tax_credit = (s(0.01f32) * cast::<Ext, _>(g.corp_tax_rate)) * taxable;
    qtrs_left = if g.ent_misc_data[corp_id].is_empty() {
        cast::<i64, _>(g.disc_spend_pct[corp_id])
    } else {
        to_quad(val(parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 29).as_bytes()))
    };
    let industry_id: i16 = g.industry_num[corp_id];
    let mut proj_tax_cred = Ext::from_f64(0.0);
    if corp_id > 130 && cast::<Ext, _>(qtrs_left) > Ext::from_f64(10.0) && get_industry_category(industry_id) == 1 {
        qtrs_left = to_quad(cast::<Ext, _>(qtrs_left) - Ext::from_f64(10.0));
        if cast::<Ext, _>(qtrs_left) > Ext::from_f64(0.0) {
            proj_tax_cred = ((s(0.01f32) * cast::<Ext, _>(qtrs_left)) * g.business_assets[corp_id]) * s(0.2f32);
            proj_tax_cred = s(0.25f32) * proj_tax_cred;
        }
    }
    let mut growth_credit = Ext::from_f64(0.0);
    if corp_id > 130 {
        growth_credit = ((g.business_assets[corp_id] * cast::<Ext, _>(g.growth_rate[corp_id])) * s(0.0025f32)) * s(0.2f32);
    }
    if growth_credit > Ext::from_f64(0.0) {
        growth_credit = cast::<Ext, _>(int_calcs) * growth_credit;
    } else {
        growth_credit = Ext::from_f64(0.0);
    }
    proj_tax_cred = s(0.07f32) * growth_credit + proj_tax_cred;
    if proj_tax_cred > Ext::from_f64(0.0) || g.tax_credits[corp_id] > Ext::from_f64(0.0) {
        // asm: UpdateCorpControl(corpId) — sets Parent80Owner, tested below
        update_corp_control(g, ui, rng, f.corp_id);
    }
    proj_tax_cred = g.tax_credits[corp_id] + proj_tax_cred;
    if g.parent80_owner == 0 {
        if proj_tax_cred > max_tax_credit && !(max_tax_credit < Ext::from_f64(0.0)) {
            proj_tax_cred = max_tax_credit;
        } else if max_tax_credit < Ext::from_f64(0.0) {
            proj_tax_cred = Ext::from_f64(0.0);
        }
    }
    proj_tax = proj_tax - proj_tax_cred;
    let mut proj_net = (proj_reg + proj_equity) - proj_tax;
    amt = calc_capital_taxes(g, f.corp_id);
    proj_net = proj_net - amt;
    let extraordinary = cast::<Ext, _>(g.extraordinary_pl[corp_id]);
    if extraordinary.abs() < proj_net.abs() * s(0.05f32) {
        proj_net = extraordinary + proj_net;
    }

    // ---- randomize
    if g.country_idx[corp_id] == 53 && proj_net > Ext::from_f64(0.0) {
        proj_net = (s(1.5f32) * rng.rnd() + s(0.5f32)) * proj_net;
    } else {
        proj_net = (s(0.2f32) * rng.rnd() + s(0.9f32)) * proj_net;
    }

    // ---- earnings smoothing through the contingency reserve (computer-run companies with positive net worth)
    let c = corp_id;
    if get_controller_type(g, f.corp_id) != 1 && net_worth > Ext::from_f64(0.0) {
        if proj_net > Ext::from_f64(0.0) && g.reserve_adj_pending[c] == Ext::from_f64(0.0) {
            let prior_est3 = val(parse_any(g.ent_misc_data[c].as_bytes(), b"|", 3).as_bytes());
            let prior_est2 = val(parse_any(g.ent_misc_data[c].as_bytes(), b"|", 2).as_bytes());
            if proj_net > prior_est3 * s(1.2f32) && proj_net > prior_est2 * s(1.2f32) {
                g.reserve_adj_pending[c] = proj_net * s(0.05f32);
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    g.reserve_adj_pending[c] = Ext::from_f64(0.0);
                    if g.contingency_reserve[c] > proj_net * s(0.2f32) && Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
                        g.reserve_adj_pending[c] = g.contingency_reserve[c] * -s(0.15f32);
                    }
                }
            } else if proj_net > prior_est2 {
            } else if c > 85 && prior_est2 > Ext::from_f64(0.0) && proj_net > prior_est2 * s(0.8f32) {
                if g.contingency_reserve[c] > prior_est2 * s(0.3f32) {
                    g.reserve_adj_pending[c] = prior_est2 * -s(0.3f32);
                }
            } else if c > 85 && prior_est3 > Ext::from_f64(0.0) && proj_net > prior_est3 * s(0.8f32) {
                if g.contingency_reserve[c] > prior_est3.abs() * s(0.3f32) {
                    g.reserve_adj_pending[c] = prior_est3.abs() * -s(0.3f32);
                }
            } else if proj_net > Ext::from_f64(0.0) {
                g.reserve_adj_pending[c] = proj_net * s(0.01f32);
            }
            if industry_id == 2 && g.reserve_adj_pending[c] < Ext::from_f64(0.0) {
                g.reserve_adj_pending[c] = Ext::from_f64(0.0);
            }
        } else if !(proj_net > Ext::from_f64(0.0))
            && net_worth > proj_net.abs() * s(0.05f32)
            && g.reserve_adj_pending[c] == Ext::from_f64(0.0)
            && g.contingency_reserve[c] == Ext::from_f64(0.0)
        {
            g.reserve_adj_pending[c] = proj_net * -s(0.05f32);
        }
    } else if !(proj_net > Ext::from_f64(0.0))
        && get_controller_type(g, f.corp_id) != 1
        && net_worth < Ext::from_f64(0.0)
        && g.reserve_adj_pending[c] > Ext::from_f64(0.0)
    {
        g.reserve_adj_pending[c] = Ext::from_f64(0.0);
    }

    // ---- interpolate from the previous projection
    let prev_proj = g.proj_qtr_earnings[c];
    let mut no_interpolate = 0;
    if g.rand_seed % 3 == 2 {
        no_interpolate = 1;
    }
    if g.sim_co_count < c && c - g.sim_co_count < 600 {
        no_interpolate = 1;
    } else if g.sim_co_count > 1310 && c < 300 {
        no_interpolate = 1;
    }
    if g.country_idx[c] == 53 {
        no_interpolate = 1;
    }
    if no_interpolate == 0 {
        amt = proj_net - prev_proj;
        amt_b = amt.abs();
        amt_c = (amt_b * s(0.3f32)) * rng.rnd() + amt_b * s(0.4f32);
        if proj_net > prev_proj {
            proj_net = amt_c + prev_proj;
        } else {
            proj_net = amt_c + proj_net;
        }
    }
    g.proj_qtr_earnings[c] = proj_net;
    proj_net
}
