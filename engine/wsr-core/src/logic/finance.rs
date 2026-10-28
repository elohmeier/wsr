//! Per-entity financial calculators: the balance-sheet summary `CalcEntityFinancials` ("CorpInfoItems",
//! 0x426610), credit ratings, government/corporate/convertible bond pricing, capital taxes and a few small
//! lookups (controller type, managing player, market and CD rates, industry category, physical commodities).
//!
//! Numeric conventions of this module (see the crate README): `EXT` locals/globals use software x87 arithmetic; a `SINGLE`
//! store (`fstp dword`) is rounded with `as f32`; SINGLE constants of the original (`fmul dword [0x75e…]`)
//! are written as `0.333f32 as f64`; CURRENCY locals use [`Cur`] (every store rounds to 1/10000). The
//! association order of every float expression follows the machine code, not the decompiled view.
//!
//! The decompiled view of several routines here is misleading (wrong operand order, missing statements,
//! wrongly placed `(float)` casts); comments marked "asm:" record where the port follows the machine code
//! instead.

// Statements keep the original BASIC shape (`x = x + y`, `a < 1 OR 5 < a`) on purpose; negated float
// comparisons (`!(a > b)`) mirror the x87 branch taken for unordered (NaN) operands.
#![allow(
    clippy::manual_range_contains,
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::manual_clamp,
    clippy::if_same_then_else
)]
use crate::pb::real::{Ext, cast};


use crate::logic::holdings::find_stock_holding;
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, mid, parse_any, right, rtrim};
use crate::pb::numeric::{to_integer, to_long, to_quad, Cur};
use crate::state::Globals;
use crate::ui::Ui;

/// `x MOD y` on the x87 (`PB_FMod` 0x6be6ef, `fprem`): truncating remainder.
fn fmod(x: Ext, y: Ext) -> Ext {
    x % y
}

/// `PB_Pow` (0x6bcabc): `base ^ exp`.
fn pow(base: Ext, exp: Ext) -> Ext {
    base.powf(exp)
}

// ---------------------------------------------------------------------------------------------------------
// Small lookups
// ---------------------------------------------------------------------------------------------------------

/// Port of GetControllerType (0x4a09b9).
/// Kind of controller of `entity_id`: 0 = none / a corporation, 1 = human player, 2 = computer player.
///
/// Side effect: for players (1..10) `CorpController(entity_id)` is set to `entity_id` first. Controllers
/// 6..10 index `IsComputerPlayer(1 TO 5)` out of range (dummy element) like the original.
pub fn get_controller_type(g: &mut Globals, entity_id: i32) -> i32 {
    let mut ctrl_type = 0;
    if 0 < entity_id && entity_id < 11 {
        g.corp_controller[entity_id] = entity_id;
    }
    if g.corp_controller[entity_id] != 0 && g.corp_controller[entity_id] < 11 {
        let ctrl = g.corp_controller[entity_id];
        if g.is_computer_player[ctrl] == 1 {
            ctrl_type = 2;
        } else {
            ctrl_type = 1;
        }
    }
    ctrl_type
}

/// Port of GetManagingPlayer (0x4a0ae3).
/// Human player who manages corporation `entity_id` (> 10) personally, else 0.
///
/// A human-controlled corp on autopilot has no managing player, unless the controller is its CEO
/// (`PlayerCeoOf`), in which case autopilot is switched off. Corps not controlled by a human get
/// `AutoPilot = 0`.
pub fn get_managing_player(g: &mut Globals, entity_id: i32) -> i32 {
    let mut result = 0;
    if 10 < entity_id {
        let corp_id = entity_id;
        if get_controller_type(g, corp_id) == 1 {
            let ctrl = g.corp_controller[corp_id];
            let mut player_id = if g.auto_pilot[corp_id] == 1 { 0 } else { ctrl };
            if g.auto_pilot[corp_id] == 1 && 0 < ctrl && ctrl < 11 && corp_id == g.player_ceo_of[ctrl] {
                player_id = ctrl;
                g.auto_pilot[corp_id] = 0;
            }
            result = player_id;
        } else {
            g.auto_pilot[corp_id] = 0;
        }
    }
    result
}

/// Port of GetMarketInterestRate (0x4a2ad7).
/// Market rate for `rate_type` 1 (consumer loans), 2 (mortgages) or 3 (subprime); other types give 0.
/// The result is a DOUBLE local.
pub fn get_market_interest_rate(g: &mut Globals, rng: &mut Rng, rate_type: i32) -> Ext {
    let mut rate: Ext = crate::pb::numeric::double(Ext::from_f64(0.0));
    let sel = cast::<Ext, _>(rate_type); // SELECT CASE on fild (float compares)
    let base = cast::<Ext, _>(g.base_interest_rate);
    if sel == Ext::from_f64(1.0) {
        rate = crate::pb::numeric::double(base * (cast::<Ext, _>(0.3f32)) + Ext::from_f64(10.0));
        if Ext::from_f64(3.0) + base > rate {
            rate = crate::pb::numeric::double((Ext::from_f64(2.0) + base) + Ext::from_f64(3.0) * rng.rnd());
        }
        if -Ext::from_f64(2.0) > cast::<Ext, _>(g.gdp_growth) {
            rate = crate::pb::numeric::double(Ext::from_f64(1.0) + rate);
        }
        if -Ext::from_f64(7.0) > cast::<Ext, _>(g.gdp_growth) {
            rate = crate::pb::numeric::double(Ext::from_f64(1.0) + rate);
        }
    } else if sel == Ext::from_f64(2.0) {
        rate = crate::pb::numeric::double(((cast::<Ext, _>(1.2f32)) + cast::<Ext, _>(g.long_bond_yield)) - (cast::<Ext, _>(0.03f32)) * cast::<Ext, _>(g.cur_quarter));
    } else if sel == Ext::from_f64(3.0) {
        rate = crate::pb::numeric::double((Ext::from_f64(3.0) + cast::<Ext, _>(g.long_bond_yield)) + base * (cast::<Ext, _>(0.15f32)));
        if Ext::from_f64(2.0) + base > rate {
            rate = crate::pb::numeric::double(cast::<Ext, _>(rng.rnd_range(3, 1)) + base);
        }
        // asm: the additions are done on the DOUBLE (the view's `(float)` casts are wrong)
        if 0 < g.subprime_crisis_state {
            rate = crate::pb::numeric::double((cast::<Ext, _>(4.3f32)) + rate);
        } else if g.subprime_crisis_state == -2 {
            rate = crate::pb::numeric::double((cast::<Ext, _>(2.8f32)) + rate);
        } else if g.subprime_crisis_state == -1 {
            rate = crate::pb::numeric::double(rate - Ext::from_f64(1.0));
        }
        if Ext::from_f64(7.0) > rate {
            rate = crate::pb::numeric::double(cast::<Ext, _>(g.cur_quarter) * (cast::<Ext, _>(0.1f32)) + Ext::from_f64(7.0));
        }
    }
    crate::pb::numeric::double(rate)
}

/// Port of GetBankCdRate (0x4a333e).
/// CD rate paid by the bank `entity_id` (11..85; other ids use rating slot 0), from `BankCdRate` (set to 5
/// if still 0) adjusted by the bank's credit rating. Returns the DOUBLE result.
pub fn get_bank_cd_rate(g: &mut Globals, entity_id: i32) -> Ext {
    let mut bank_id = 0;
    if 10 < entity_id && entity_id < 86 {
        bank_id = entity_id;
    }
    if g.bank_cd_rate == Ext::from_f32(0.0f32) {
        g.bank_cd_rate = cast::<f32, _>(Ext::from_f32(5.0f32));
    }
    let cd = cast::<Ext, _>(g.bank_cd_rate);
    let mut rate = crate::pb::numeric::double(cd);
    if 7 < g.credit_rating_num[bank_id] {
        rate = crate::pb::numeric::double(cd * (cast::<Ext, _>(0.98f32)));
    }
    if 1 < g.credit_rating_num[bank_id] && g.credit_rating_num[bank_id] < 5 {
        rate = crate::pb::numeric::double(cd * (cast::<Ext, _>(1.05f32)));
    }
    if g.credit_rating_num[bank_id] < 2 {
        rate = crate::pb::numeric::double(cd * (cast::<Ext, _>(1.1f32)));
    }
    crate::pb::numeric::double(rate)
}

/// Port of GetIndustryCategory (0x45fb6a).
/// 1 = R&D-driven industry, 2 = marketing-driven industry, 0 = other (incl. 70/71).
pub fn get_industry_category(industry: i16) -> i32 {
    let x = cast::<i32, _>(industry);
    match x {
        3 | 4 | 7..=10 | 12 | 13 | 14 | 20..=22 | 24 | 26 | 30 | 35 | 38..=40 | 49..=51 | 53 | 54 | 58 | 61
        | 63..=65 | 67..=69 => 1,
        1 | 2 | 5 | 6 | 11 | 15..=19 | 23 | 25 | 27..=29 | 31..=34 | 36 | 37 | 41..=48 | 52 | 55..=57 | 59
        | 60 | 62 | 66 => 2,
        _ => 0,
    }
}

/// Port of FindPhysCommodRec (0x65be8a).
/// Index of the `PhysCommodRec` record of commodity `commod_name` (field 1, trimmed) owned by `entity_id`
/// (field 2), or 0. Only active entities are searched.
pub fn find_phys_commod_rec(g: &Globals, entity_id: i32, commod_name: &PbStr) -> i32 {
    let mut found_idx = 0;
    if val(g.phys_commod_rec[0].as_bytes()) != Ext::from_f64(0.0) {
        let ent_id = entity_id;
        if g.entity_status[ent_id] == 1 {
            let n = to_long(val(g.phys_commod_rec[0].as_bytes()));
            let mut rec_idx = 1;
            while rec_idx <= n {
                let rec = &g.phys_commod_rec[rec_idx];
                if !rec.is_empty() {
                    let key = rtrim(ltrim(parse_any(rec.as_bytes(), b"|", 1).as_bytes()).as_bytes());
                    if *commod_name == key && val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(ent_id) {
                        found_idx = rec_idx;
                        break;
                    }
                }
                rec_idx += 1;
            }
        }
    }
    found_idx
}

/// Port of GetPhysCommodValue (0x65b912) with its GOSUB `GetPhysCommodValue_PriceUnits` (0x65bb77).
/// Market value [runtime resource wording] ies held by `entity_id` (all, or only those whose 3-letter code
/// equals `MID$(TRIM$(filter), 1, 3)`), times `ExchangeRate`.
///
/// `PhysCommodRec(i)` = "CODE…|owner|units|cost"; `CiPhysCommodCost` receives the summed cost (field 4).
/// With no records (`VAL(PhysCommodRec(0))` rounds to 0) the result is 0.
pub fn get_phys_commod_value(g: &mut Globals, entity_id: i32, filter: &PbStr) -> Ext {
    g.ci_phys_commod_cost = Ext::from_f64(0.0);
    let ent_id = entity_id;
    let mut filt = PbStr::new();
    if !filter.is_empty() {
        filt = rtrim(ltrim(filter.as_bytes()).as_bytes());
        filt = mid(filt.as_bytes(), 1, 3);
    }
    let rec_count = to_long(val(g.phys_commod_rec[0].as_bytes()));
    if rec_count == 0 {
        return Ext::from_f64(0.0);
    }
    let mut total_value = Ext::from_f64(0.0);
    let mut rec_idx = 1;
    while rec_idx <= rec_count {
        let rec_text = rtrim(ltrim(g.phys_commod_rec[rec_idx].as_bytes()).as_bytes());
        let owner_id = to_long(val(parse_any(rec_text.as_bytes(), b"|", 2).as_bytes()));
        if ent_id == owner_id && (filt.is_empty() || filt == mid(rec_text.as_bytes(), 1, 3)) {
            // GOSUB GetPhysCommodValue_PriceUnits (0x65bb77)
            let code = mid(rec_text.as_bytes(), 1, 3);
            let units = cast::<Ext, _>(to_quad(val(parse_any(rec_text.as_bytes(), b"|", 3).as_bytes())));
            if code == "GOL" {
                total_value = (cast::<Ext, _>(0.001f32)) * cast::<Ext, _>(g.gold_price) * units + total_value;
            } else if code == "SIL" {
                total_value = (cast::<Ext, _>(0.01f32)) * cast::<Ext, _>(g.silver_price) * units + total_value;
            } else if code == "OIL" {
                total_value = units * cast::<Ext, _>(g.oil_price) + total_value;
            } else if code == "WHE" {
                total_value = (cast::<Ext, _>(0.1f32)) * cast::<Ext, _>(g.wheat_price) * units + total_value;
            } else if code == "COR" {
                total_value = (cast::<Ext, _>(0.1f32)) * cast::<Ext, _>(g.corn_price) * units + total_value;
            } else if code == "BIT" {
                total_value = (cast::<Ext, _>(0.001f32)) * cast::<Ext, _>(g.bitcoin_price) * units + total_value;
            } else if code == "ETH" {
                total_value = (cast::<Ext, _>(0.01f32)) * cast::<Ext, _>(g.ethereum_price) * units + total_value;
            }
            g.ci_phys_commod_cost = val(parse_any(rec_text.as_bytes(), b"|", 4).as_bytes()) + g.ci_phys_commod_cost;
        }
        rec_idx += 1;
    }
    g.exchange_rate * total_value
}

// ---------------------------------------------------------------------------------------------------------
// Capital taxes
// ---------------------------------------------------------------------------------------------------------

/// Port of CalcCapitalTaxes (0x5077a7).
/// Quarterly capital-type taxes owed by `entity_id`: wealth tax (players, Q4), carbon, health-care and oil
/// windfall taxes on business assets, the holding-company tax and the tax on shares of corporations of
/// `CorpSharesTaxCountry`. Sets the component globals and `CapitalTaxCount`; corporations owing less than
/// `ExchangeRate * 10` in total owe nothing (but `TmpCapitalTax` is not cleared then).
pub fn calc_capital_taxes(g: &mut Globals, entity_id: i32) -> Ext {
    g.tmp_capital_tax = Ext::from_f64(0.0);
    g.carbon_tax_qtr = Ext::from_f64(0.0);
    g.oil_windfall_tax_qtr = Ext::from_f64(0.0);
    g.holding_co_tax_due = Ext::from_f64(0.0);
    g.corp_shares_tax_due = Ext::from_f64(0.0);
    g.health_care_tax_qtr = Ext::from_f64(0.0);
    g.capital_tax_count = 0;
    let taxpayer_id = entity_id;
    if g.entity_status[taxpayer_id] != 1 {
        return Ext::from_f64(0.0);
    }
    if 10 < taxpayer_id && g.industry_num[taxpayer_id] == 71 {
        return Ext::from_f64(0.0);
    }
    if 0 < g.wealth_tax_rate && g.cur_quarter == 4 && taxpayer_id < 11 {
        let taxable_wealth = g.net_worth + g.player_prepaid_tax[taxpayer_id];
        g.tmp_capital_tax =
            (cast::<Ext, _>(g.wealth_tax_rate) * (cast::<Ext, _>(0.01f32))) * (taxable_wealth - g.exchange_rate * Ext::from_f64(1000.0));
        if !(g.tmp_capital_tax > g.exchange_rate * Ext::from_f64(1.0)) {
            g.tmp_capital_tax = Ext::from_f64(0.0);
        }
        if g.tmp_capital_tax > Ext::from_f64(0.0) {
            g.capital_tax_count += 1;
        }
    }
    if 130 < taxpayer_id && 0 < g.carbon_tax_rate {
        let industry_id = g.industry_num[taxpayer_id];
        if g.country_idx[taxpayer_id] != 8 && g.country_idx[taxpayer_id] != 21 {
            if matches!(industry_id, 3 | 7 | 8 | 9 | 11 | 12 | 16 | 18 | 20 | 23 | 25 | 33 | 50 | 51 | 52 | 55 | 67 | 68 | 69) {
                g.carbon_tax_qtr = (cast::<Ext, _>(g.carbon_tax_rate) * g.business_assets[taxpayer_id]) * (cast::<Ext, _>(0.0025f32));
                if g.carbon_tax_qtr > Ext::from_f64(1.0) {
                    g.capital_tax_count += 1;
                } else {
                    g.carbon_tax_qtr = Ext::from_f64(0.0);
                }
            }
        }
    }
    if 0 < g.health_care_tax_rate && 130 < taxpayer_id {
        let industry_id = g.industry_num[taxpayer_id];
        if matches!(industry_id, 4 | 21 | 22 | 56) {
            g.health_care_tax_qtr =
                (cast::<Ext, _>(g.health_care_tax_rate) * g.business_assets[taxpayer_id]) * (cast::<Ext, _>(0.0025f32));
            if g.health_care_tax_qtr > Ext::from_f64(1.0) {
                g.capital_tax_count += 1;
            } else {
                g.health_care_tax_qtr = Ext::from_f64(0.0);
            }
        }
    }
    if 0 < g.oil_windfall_tax_rate && 130 < taxpayer_id {
        let industry_id = g.industry_num[taxpayer_id];
        if (industry_id == 7 || industry_id == 8 || industry_id == 63) && (cast::<Ext, _>(0.8f32)) < cast::<Ext, _>(g.oil_price) {
            let mut oil_tax_rate: Ext = crate::pb::numeric::double(cast::<Ext, _>(g.oil_price) * (cast::<Ext, _>(0.00125f32))); // DOUBLE local
            if (cast::<Ext, _>(0.0025f32)) < oil_tax_rate {
                oil_tax_rate = crate::pb::numeric::double(cast::<Ext, _>(0.0025f32));
            }
            g.oil_windfall_tax_qtr =
                (g.business_assets[taxpayer_id] * cast::<Ext, _>(g.oil_windfall_tax_rate)) * oil_tax_rate;
            if g.oil_windfall_tax_qtr > Ext::from_f64(1.0) {
                g.capital_tax_count += 1;
            } else {
                g.oil_windfall_tax_qtr = Ext::from_f64(0.0);
            }
        }
    }
    if 0 < g.holding_co_tax_rate && 130 < taxpayer_id {
        if 0 < g.corp_controller[taxpayer_id] && g.corp_controller[taxpayer_id] <= g.num_players {
            let controller_id = g.corp_controller[taxpayer_id];
            let slot = find_stock_holding(g, controller_id, taxpayer_id);
            let mut is_holding_co = 0;
            if 0 < slot && 25 < g.hold_stock_pct[(slot, controller_id)] {
                is_holding_co = 1;
            }
            if is_holding_co == 1 {
                let op_assets = to_quad(g.working_capital[taxpayer_id] + g.business_assets[taxpayer_id]);
                if (cast::<Ext, _>(op_assets)) < g.total_assets * (cast::<Ext, _>(0.2f32)) {
                    let fin_assets = to_quad(g.total_assets - cast::<Ext, _>(op_assets));
                    if cast::<Ext, _>(fin_assets) > Ext::from_f64(0.0) {
                        g.holding_co_tax_due = ((cast::<Ext, _>(g.holding_co_tax_rate) * cast::<Ext, _>(fin_assets)) * (cast::<Ext, _>(0.01f32)))
                            * (cast::<Ext, _>(0.25f32));
                        if g.holding_co_tax_due > Ext::from_f64(1.0) {
                            g.capital_tax_count += 1;
                        } else {
                            g.holding_co_tax_due = Ext::from_f64(0.0);
                        }
                    }
                }
            }
        }
    }
    if 0 < g.corp_shares_tax_country {
        for slot in 1..=15 {
            if g.hold_stock_id[(slot, taxpayer_id)] == 0 || g.hold_stock_pct[(slot, taxpayer_id)] < 1 {
                continue;
            }
            let held_id = cast::<i32, _>(g.hold_stock_id[(slot, taxpayer_id)]);
            if 10 < taxpayer_id && 80 <= g.hold_stock_pct[(slot, taxpayer_id)] {
                continue; // consolidated subsidiaries are exempt
            }
            if 0 < g.country_idx[held_id] && g.corp_shares_tax_country != g.country_idx[held_id] {
                continue;
            }
            if g.industry_num[held_id] == 71 {
                continue;
            }
            let hold_pct = cast::<i32, _>(g.hold_stock_pct[(slot, taxpayer_id)]);
            g.corp_shares_tax_due =
                (g.market_cap[held_id] * cast::<Ext, _>(hold_pct)) * (cast::<Ext, _>(0.005f32)) + g.corp_shares_tax_due;
        }
        if g.corp_shares_tax_due > Ext::from_f64(1.0) {
            g.capital_tax_count += 1;
        } else {
            g.corp_shares_tax_due = Ext::from_f64(0.0);
        }
    }
    let mut total_tax = ((((g.carbon_tax_qtr + g.tmp_capital_tax) + g.oil_windfall_tax_qtr) + g.holding_co_tax_due)
        + g.corp_shares_tax_due)
        + g.health_care_tax_qtr;
    if 10 < taxpayer_id && total_tax < g.exchange_rate * Ext::from_f64(10.0) {
        total_tax = Ext::from_f64(0.0);
        g.carbon_tax_qtr = Ext::from_f64(0.0);
        g.oil_windfall_tax_qtr = Ext::from_f64(0.0);
        g.holding_co_tax_due = Ext::from_f64(0.0);
        g.corp_shares_tax_due = Ext::from_f64(0.0);
        g.health_care_tax_qtr = Ext::from_f64(0.0);
    }
    total_tax
}

// ---------------------------------------------------------------------------------------------------------
// Credit rating
// ---------------------------------------------------------------------------------------------------------

fn set_rating(g: &mut Globals, entity_id: i32, s: &str, n: i32) {
    g.credit_rating_str = PbStr::from(s);
    g.credit_rating_num[entity_id] = n;
}

/// Port of CalcCreditRating (0x42b721).
/// Credit rating of `entity_id` from the figures `CalcEntityFinancials` left in the globals
/// (`TmpTotalDebt / TmpCreditNetWorth` for non-banks, capital ratio for banks): sets `CreditRatingStr`,
/// `CreditRatingNum(entity)` (10 = AAA … 1/0 = D) and the borrowing rate `BorrowRate` (prime/SOFR plus a
/// spread, rounded to 1/4 %); for banks `EntityBorrowRate` instead (BorrowRate is then not rounded).
pub fn calc_credit_rating(g: &mut Globals, entity_ref: i32) {
    let e = entity_ref;
    // entName = EntName(entityRef) is read into a local and never used.
    if 10 < e && g.entity_status[e] != 1 {
        return;
    }
    let mut debt_ratio: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    if g.tmp_credit_net_worth != Ext::from_f64(0.0) {
        debt_ratio = cast::<f32, _>(g.tmp_total_debt / g.tmp_credit_net_worth);
    }
    let mut econ_adj: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(2.0) - cast::<Ext, _>(g.gdp_growth)));
    if -Ext::from_f32(1.0f32) > econ_adj {
        econ_adj = -1.0f32;
    }
    if Ext::from_f32(2.0f32) < econ_adj {
        econ_adj = 2.0f32;
    }
    let dr = cast::<Ext, _>(debt_ratio);
    if e < 11 || g.industry_num[e] != 1 {
        set_rating(g, e, "AAA", 10);
        if (cast::<Ext, _>(0.05f32)) < dr {
            set_rating(g, e, "AA", 9);
        }
        if (cast::<Ext, _>(0.2f32)) < dr {
            set_rating(g, e, "A", 8);
        }
        if Ext::from_f64(0.5) < dr {
            set_rating(g, e, "BBB", 7);
        }
        if Ext::from_f64(1.0) < dr {
            set_rating(g, e, "BB", 6);
        }
        if Ext::from_f64(1.5) < dr {
            set_rating(g, e, "B", 5);
        }
        if Ext::from_f64(2.0) < dr {
            set_rating(g, e, "CCC", 4);
        }
        if Ext::from_f64(3.0) < dr {
            set_rating(g, e, "CC", 3);
        }
        if Ext::from_f64(5.0) < dr {
            set_rating(g, e, "C", 2);
        }
        if !(g.tmp_credit_net_worth > Ext::from_f64(0.0)) {
            set_rating(g, e, "D", 1);
            if g.total_assets < g.tmp_total_debt * Ext::from_f64(0.75) {
                g.credit_rating_num[e] = 0;
            }
        }
    } else {
        // bank (industry 1)
        let mut cust_deposits = g.bank_cust_deposits[e];
        if cust_deposits < Ext::from_f64(0.0) {
            cust_deposits = Ext::from_f64(0.0);
        }
        let mut total_liabs = ((g.bank_demand_deposits[e] + g.bank_c_ds[e]) + cust_deposits) + g.tmp_total_debt;
        if !(total_liabs > Ext::from_f64(0.0)) {
            total_liabs = Ext::from_f64(1.0);
        }
        let mut bad_debt_res = g.bad_debt_reserve[e];
        if bad_debt_res < Ext::from_f64(0.0) {
            bad_debt_res = Ext::from_f64(0.0);
        }
        let dr = cast::<Ext, _>(cast::<f32, _>(((g.net_worth + bad_debt_res) / total_liabs) * Ext::from_f64(100.0)));
        set_rating(g, e, "CC", 3);
        if Ext::from_f64(1.5) < dr {
            set_rating(g, e, "CCC", 4);
        }
        if Ext::from_f64(3.0) < dr {
            set_rating(g, e, "B", 5);
        }
        if Ext::from_f64(4.0) < dr {
            set_rating(g, e, "BB", 6);
        }
        if Ext::from_f64(6.0) < dr {
            set_rating(g, e, "BBB", 7);
        }
        if Ext::from_f64(8.0) < dr {
            set_rating(g, e, "A", 8);
        }
        if Ext::from_f64(10.0) < dr {
            set_rating(g, e, "AA", 9);
        }
        if Ext::from_f64(15.0) < dr {
            set_rating(g, e, "AAA", 10);
        }
        if g.net_worth < Ext::from_f64(0.0) {
            set_rating(g, e, "C", 2);
        }
        if g.net_worth + bad_debt_res < Ext::from_f64(0.0) {
            set_rating(g, e, "D", 1);
        }
        if total_liabs * (cast::<Ext, _>(0.9f32)) > (g.total_assets + bad_debt_res) - g.long_options_value * Ext::from_f64(0.5) {
            set_rating(g, e, "D", 0);
        }
    }
    if g.credit_rating_num[e] == 10 && 10 < e && g.industry_num[e] != 71 {
        if !(g.earnings_y1[e] > Ext::from_f64(0.0)
            && g.earnings_yr2_ago[e] > Ext::from_f64(0.0)
            && g.earnings_yr3_ago[e] > Ext::from_f64(0.0)
            && g.earnings_yr4_ago[e] > Ext::from_f64(0.0)
            && g.earnings_ytd[e] >= Ext::from_f64(0.0))
        {
            set_rating(g, e, "AA", 9);
        }
    }
    let mut base_rate: f32 = cast::<f32, _>(g.prime_rate);
    if 10 < e && g.industry_num[e] == 1 {
        base_rate = g.sofr_rate;
    }
    let base = cast::<Ext, _>(base_rate);
    let econ = cast::<Ext, _>(econ_adj);
    let dl = cast::<Ext, _>(g.difficulty_level);
    g.borrow_rate = cast::<f32, _>(base_rate);
    let r = g.credit_rating_str.clone();
    if r == "AA" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(0.5) + base));
    }
    if r == "A" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) + base));
    }
    if r == "BBB" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(((Ext::from_f64(1.5) + base) + econ * (cast::<Ext, _>(0.2f32))) + dl * (cast::<Ext, _>(0.15f32))));
    }
    if r == "BB" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(((Ext::from_f64(2.0) + base) + econ * (cast::<Ext, _>(0.4f32))) + dl * (cast::<Ext, _>(0.35f32))));
    }
    if r == "B" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(((Ext::from_f64(2.5) + base) + econ * (cast::<Ext, _>(0.6f32))) + dl * Ext::from_f64(0.5)));
    }
    if r == "CCC" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(((Ext::from_f64(3.0) + base) + econ * (cast::<Ext, _>(0.8f32))) + dl * (cast::<Ext, _>(0.65f32))));
    }
    if r == "CC" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(((Ext::from_f64(3.5) + base) + econ * Ext::from_f64(1.0)) + dl * Ext::from_f64(0.75)));
    }
    if r == "C" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(((Ext::from_f64(4.0) + base) + econ * (cast::<Ext, _>(1.2f32))) + dl));
    }
    if r == "D" {
        g.borrow_rate = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) + base));
    }
    if 10 < e && g.industry_num[e] == 1 {
        g.entity_borrow_rate = cast::<f32, _>(g.sofr_rate);
        let mut rating_gap: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE local
        if g.credit_rating_num[e] < 10 {
            rating_gap = crate::pb::numeric::double(Ext::from_f64(9.5) - cast::<Ext, _>(g.credit_rating_num[e]));
        }
        g.entity_borrow_rate = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.6f32)) * rating_gap + cast::<Ext, _>(g.entity_borrow_rate)));
        return;
    }
    let rate_qtrs = to_integer(cast::<Ext, _>(g.borrow_rate) * Ext::from_f64(4.0));
    g.borrow_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(rate_qtrs) * Ext::from_f64(0.25)));
}

// ---------------------------------------------------------------------------------------------------------
// Government bonds
// ---------------------------------------------------------------------------------------------------------

/// GOSUB CalcGovBondYields_Price (0x428a45): price per 100 face of a semi-annual bond with the given
/// coupon (% p.a., SINGLE), yield (SINGLE) and number of half-year periods (SINGLE). DOUBLE result.
fn gov_bond_price(yield_pct: f32, coupon_rate: f32, periods: f32) -> Ext {
    let mut half_rate: Ext = (cast::<Ext, _>(0.01f32)) * cast::<Ext, _>(yield_pct);
    half_rate = Ext::from_f64(0.5) * half_rate;
    let coupon_pmt: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) * cast::<Ext, _>(coupon_rate)));
    let disc_factor: Ext = Ext::from_f64(1.0) / pow(half_rate + Ext::from_f64(1.0), cast::<Ext, _>(periods));
    let one_minus_df: Ext = Ext::from_f64(1.0) - disc_factor;
    let annuity_factor: Ext = one_minus_df / half_rate;
    let coupon_pv: Ext = cast::<Ext, _>(coupon_pmt) * annuity_factor;
    let face_df: Ext = Ext::from_f64(1.0) / pow(half_rate + Ext::from_f64(1.0), cast::<Ext, _>(periods));
    let face_pv: Ext = Ext::from_f64(1000.0) * face_df;
    let pv_total: Ext = coupon_pv + face_pv;
    crate::pb::numeric::double((cast::<Ext, _>(0.1f32)) * pv_total)
}

/// `(maturity - CurYear) - (CurQuarter - 1) * 0.25` as two SINGLE stores.
fn years_to_maturity(g: &Globals, maturity_year: f32) -> f32 {
    let y = cast::<f32, _>(cast::<Ext, _>(maturity_year) - cast::<Ext, _>(g.cur_year));
    cast::<f32, _>(cast::<Ext, _>(y) - (cast::<Ext, _>(g.cur_quarter) - Ext::from_f64(1.0)) * Ext::from_f64(0.25))
}

/// Port of CalcGovBondYields (0x428477) with its GOSUB CalcGovBondYields_Price (0x428a45).
/// Long and short government bond yields and prices and the T-bill rate from the base rate, GDP growth,
/// years to maturity (`LongBondDesc`/`ShortBondDesc`: "YYYY…" + coupon in the last 5 chars) and monetary
/// policy. Prices are rounded to CURRENCY before being stored as SINGLE.
///
/// asm: both bonds are priced with `periods = 2 * years` (after the `SimCoCount > 760` adjustment); the
/// decompiled view drops this for the long bond.
pub fn calc_gov_bond_yields(g: &mut Globals) {
    // ---- long bond
    let maturity_year = cast::<f32, _>(val(mid(g.long_bond_desc.as_bytes(), 1, 4).as_bytes()));
    let coupon_rate = cast::<f32, _>(val(right(g.long_bond_desc.as_bytes(), 5).as_bytes()));
    let mut years_left = years_to_maturity(g, maturity_year);
    g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.base_interest_rate) * (cast::<Ext, _>(0.9f32))));
    if Ext::from_f32(14.0f32) > years_left {
        g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.long_bond_yield) * (cast::<Ext, _>(0.94f32))));
    } else if Ext::from_f32(17.0f32) > years_left {
        g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.long_bond_yield) * (cast::<Ext, _>(0.97f32))));
    }
    let mut gdp_adj: f32 = cast::<f32, _>(g.gdp_growth);
    if -Ext::from_f32(8.0f32) > g.gdp_growth {
        gdp_adj = -8.0f32;
    }
    let gdp_factor: Ext = (cast::<Ext, _>(0.01f32)) * (cast::<Ext, _>(gdp_adj) + Ext::from_f64(100.0));
    g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(gdp_factor * cast::<Ext, _>(g.long_bond_yield)));
    let mut yield_pct = g.long_bond_yield;
    if 760 < g.sim_co_count {
        years_left = cast::<f32, _>(cast::<Ext, _>(years_left) - Ext::from_f64(0.125));
    }
    years_left = cast::<f32, _>(Ext::from_f64(2.0) * cast::<Ext, _>(years_left));
    let bond_price = gov_bond_price(yield_pct, coupon_rate, years_left);
    g.long_bond_price = cast::<f32, _>(cast::<f32, _>(Cur::from_f64(bond_price).get()));

    // ---- short bond
    let maturity_year = cast::<f32, _>(val(mid(g.short_bond_desc.as_bytes(), 1, 4).as_bytes()));
    let coupon_rate = cast::<f32, _>(val(right(g.short_bond_desc.as_bytes(), 5).as_bytes()));
    years_left = years_to_maturity(g, maturity_year);
    let yl = cast::<Ext, _>(years_left);
    let lby = cast::<Ext, _>(g.long_bond_yield);
    g.short_bond_yield = cast::<f32, _>(cast::<f32, _>(if yl < Ext::from_f64(2.0) {
        lby * (cast::<Ext, _>(0.95f32))
    } else if yl < Ext::from_f64(3.0) {
        lby * (cast::<Ext, _>(0.91f32))
    } else if yl < Ext::from_f64(4.0) {
        lby * (cast::<Ext, _>(0.83f32))
    } else if yl < Ext::from_f64(6.0) {
        lby * (cast::<Ext, _>(0.88f32))
    } else if yl < Ext::from_f64(8.0) {
        lby * (cast::<Ext, _>(0.9f32))
    } else {
        lby * (cast::<Ext, _>(0.95f32))
    }));
    if Ext::from_f64(3.0) < yl {
        let b = cast::<Ext, _>(g.base_interest_rate);
        let k: f32 = cast::<f32, _>(if (cast::<Ext, _>(4.3f32)) > b {
            Ext::from_f32(0.39f32)
        } else if (cast::<Ext, _>(5.1f32)) > b {
            Ext::from_f32(0.52f32)
        } else if Ext::from_f64(5.5) > b {
            Ext::from_f32(0.7f32)
        } else if (cast::<Ext, _>(6.2f32)) > b {
            Ext::from_f32(0.82f32)
        } else if (cast::<Ext, _>(6.9f32)) > b {
            Ext::from_f32(0.9f32)
        } else if Ext::from_f64(8.0) > b {
            Ext::from_f32(0.93f32)
        } else if Ext::from_f64(11.0) > b {
            Ext::from_f32(0.98f32)
        } else if Ext::from_f64(13.0) > b {
            Ext::from_f32(1.01f32)
        } else if Ext::from_f64(15.0) > b {
            Ext::from_f32(1.05f32)
        } else {
            Ext::from_f32(1.1f32)
        });
        g.short_bond_yield = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(k) * cast::<Ext, _>(g.short_bond_yield)));
    }
    if g.monetary_policy == 0 {
        g.short_bond_yield = cast::<f32, _>(cast::<f32, _>((cast::<Ext, _>(0.8f32)) * cast::<Ext, _>(g.short_bond_yield)));
    }
    if g.monetary_policy == 2 {
        g.short_bond_yield = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.25) * cast::<Ext, _>(g.short_bond_yield)));
    }
    yield_pct = g.short_bond_yield;
    if 760 < g.sim_co_count {
        years_left = cast::<f32, _>(cast::<Ext, _>(years_left) - Ext::from_f64(0.125));
    }
    let tbill_factor: f32 = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(97.0) - cast::<Ext, _>(years_left) * Ext::from_f64(2.0)) * (cast::<Ext, _>(0.01f32))));
    g.t_bill_rate = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(tbill_factor) * cast::<Ext, _>(g.short_bond_yield)));
    years_left = cast::<f32, _>(Ext::from_f64(2.0) * cast::<Ext, _>(years_left));
    let bond_price = gov_bond_price(yield_pct, coupon_rate, years_left);
    g.short_bond_price = cast::<f32, _>(cast::<f32, _>(Cur::from_f64(bond_price).get()));
    // CalcGovBondYields_Exit: empty
}

// ---------------------------------------------------------------------------------------------------------
// Corporate bonds
// ---------------------------------------------------------------------------------------------------------

/// Locals of CalcCorpBondPrice shared with its GOSUB CalcCorpBondPrice_PV.
struct BondFrame {
    corp_id: i32,
    /// DOUBLE yield to maturity (% p.a.).
    yield_pct: Ext,
    /// SINGLE coupon (% p.a.).
    coupon_rate: f32,
    /// SINGLE number of half-year periods.
    periods: f32,
    /// CURRENCY price per 100 face.
    price: Cur,
    /// GOSUB call counter (also the loop variable of the holder scan).
    iter: i32,
    /// EXT local: half-year rate inside the GOSUB, publicly held face later.
    public_face: Ext,
}

/// GOSUB CalcCorpBondPrice_PV (0x42ab88): `price` = present value of the bond at `yield_pct` (coupon
/// halved for rating 1 and quartered for rating 0); increments `iter`.
fn corp_bond_pv(g: &Globals, f: &mut BondFrame) {
    f.iter += 1;
    f.public_face = (cast::<Ext, _>(0.01f32)) * f.yield_pct;
    f.public_face = Ext::from_f64(0.5) * f.public_face;
    let mut coupon_pmt: f32 = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) * cast::<Ext, _>(f.coupon_rate)));
    if g.credit_rating_num[f.corp_id] == 1 {
        coupon_pmt = cast::<f32, _>(Ext::from_f64(0.5) * cast::<Ext, _>(coupon_pmt));
    }
    if g.credit_rating_num[f.corp_id] == 0 {
        coupon_pmt = cast::<f32, _>(Ext::from_f64(0.25) * cast::<Ext, _>(coupon_pmt));
    }
    let disc_factor: Ext = Ext::from_f64(1.0) / pow(f.public_face + Ext::from_f64(1.0), cast::<Ext, _>(f.periods));
    let one_minus_df: Ext = Ext::from_f64(1.0) - disc_factor;
    let annuity_factor: Ext = one_minus_df / f.public_face;
    let coupon_pv: Ext = cast::<Ext, _>(coupon_pmt) * annuity_factor;
    let face_pv: Ext = Ext::from_f64(1000.0) * disc_factor;
    let pv_total: Ext = coupon_pv + face_pv;
    f.price = Cur::from_f64((cast::<Ext, _>(0.1f32)) * pv_total);
}

/// Port of CalcCorpBondPrice (0x428bcf) with its GOSUB CalcCorpBondPrice_PV (0x42ab88).
/// Market price (`BondPrice(corp)`, per 100 face) and yield (`BondYtm`) of `corp_ref`'s bond issue from its
/// coupon, maturity, credit rating (`CreditRatingStr`/`CreditRatingNum` and `BorrowRate` as left by
/// [`calc_credit_rating`]) and the financials in the globals; distressed ("D") bonds get recovery-based
/// prices, convertibles are floored at their conversion value ([`calc_convertible_bond_value`]).
/// Also sets `BondsPublicHeld`, `LargestBondholder`, and may set `PlayerRecentCorp` and
/// `PendingForcedConvCorp`.
///
/// Note: one `RND` is drawn before the corp is checked (so also for inactive corps / corps without bonds);
/// only `WorldGenerated = 0` returns before that.
pub fn calc_corp_bond_price(g: &mut Globals, rng: &mut Rng, corp_ref: i32) {
    if g.world_generated == 0 {
        return;
    }
    let corp_id = corp_ref;
    let r0 = rng.rnd();
    // asm: the whole sum is taken MOD 8: ((CurQuarter + CurYear + corpId) MOD 8) + RND
    let base_price: Ext =
        Ext::from_f64(1.0) * r0 + fmod(cast::<Ext, _>(g.cur_quarter) + cast::<Ext, _>(g.cur_year) + cast::<Ext, _>(corp_id), Ext::from_f64(8.0));
    if corp_id < 11 || g.entity_status[corp_id] != 1 {
        return;
    }
    if g.bonds_issued[corp_id] == 0 {
        g.bond_issue_info[corp_id] = PbStr::new();
        g.bond_price[corp_id] = Cur::from_f64(Ext::from_f64(0.0));
        g.bonds_public_held = 0;
        return;
    }
    if g.game_end_flag == "Y" {
        return;
    }
    let info = g.bond_issue_info[corp_id].clone();
    let mut maturity_year = cast::<f32, _>(val(mid(info.as_bytes(), 1, 4).as_bytes()));
    let coupon_rate = cast::<f32, _>(val(right(info.as_bytes(), 5).as_bytes()));
    let mut f = BondFrame { corp_id, yield_pct: crate::pb::numeric::double(Ext::from_f64(0.0)), coupon_rate, periods: 0.0f32, price: Cur(0), iter: 0, public_face: Ext::from_f64(0.0) };
    let mut price_cap: Ext;

    f.yield_pct = crate::pb::numeric::double(Ext::from_f64(0.5) + cast::<Ext, _>(g.borrow_rate));
    if g.industry_num[corp_id] == 1 {
        f.yield_pct = crate::pb::numeric::double(Ext::from_f64(0.75) + cast::<Ext, _>(g.borrow_rate));
    }
    let r1 = rng.rnd();
    let r2 = rng.rnd();
    // asm: yield = ((Prime - Base) + yield) + (0.04 * RND#2 - 0.04 * RND#1)
    f.yield_pct = crate::pb::numeric::double(((cast::<Ext, _>(g.prime_rate) - cast::<Ext, _>(g.base_interest_rate)) + f.yield_pct)
        + ((cast::<Ext, _>(0.04f32)) * r2 - (cast::<Ext, _>(0.04f32)) * r1));

    let mut matured_par = false;
    if !(cast::<Ext, _>(maturity_year) > cast::<Ext, _>(g.cur_year)) {
        if cast::<Ext, _>(maturity_year) == cast::<Ext, _>(g.cur_year) && g.cur_quarter == 1 && g.sim_co_count < corp_id {
            if 2 < g.credit_rating_num[corp_id] && instr(1, info.as_bytes(), b"cv") == 0 {
                f.price = Cur::from_f64(Ext::from_f64(100.0));
                f.yield_pct = crate::pb::numeric::double(cast::<Ext, _>(coupon_rate));
                matured_par = true; // GOTO LAB_0042a3bb
            }
        } else {
            maturity_year = cast::<f32, _>(Ext::from_f64(1.0) + cast::<Ext, _>(g.cur_year));
        }
    }

    let rating_str;
    if !matured_par {
        f.periods = years_to_maturity(g, maturity_year);
        if corp_id < g.sim_co_count {
            f.periods = cast::<f32, _>(cast::<Ext, _>(f.periods) - Ext::from_f64(0.25));
        }
        if 500 < g.sim_co_count {
            f.periods = cast::<f32, _>((cast::<Ext, _>(0.083f32)) + cast::<Ext, _>(f.periods));
        }
        if 1000 < g.sim_co_count {
            f.periods = cast::<f32, _>((cast::<Ext, _>(0.083f32)) + cast::<Ext, _>(f.periods));
        }
        // asm: the yield is a DOUBLE here (no SINGLE rounding as the view suggests)
        if !(Ext::from_f64(5.0) > cast::<Ext, _>(f.periods)) {
            f.yield_pct = crate::pb::numeric::double(Ext::from_f64(0.25) + f.yield_pct);
        }
        if !(Ext::from_f64(10.0) > cast::<Ext, _>(f.periods)) {
            f.yield_pct = crate::pb::numeric::double(Ext::from_f64(0.25) + f.yield_pct);
        }
        if !(Ext::from_f32(0.0f32) < f.periods) {
            f.periods = 0.25f32;
        }
        f.periods = cast::<f32, _>(Ext::from_f64(2.0) * cast::<Ext, _>(f.periods));
        rating_str = g.credit_rating_str.clone();
        let is_d = rating_str == "D";
        if is_d {
            let base = cast::<Ext, _>(g.base_interest_rate);
            f.yield_pct = crate::pb::numeric::double(Ext::from_f64(12.0) + base);
            if g.credit_rating_num[corp_id] == 0 {
                f.yield_pct = crate::pb::numeric::double(Ext::from_f64(25.0) + base);
            }
            let coupon_due =
                (cast::<Ext, _>(0.0025f32)) * val(right(info.as_bytes(), 5).as_bytes()) * cast::<Ext, _>(g.bonds_issued[corp_id]);
            if g.cash[corp_id] + g.t_bills[corp_id] >= coupon_due
                && 2 < g.industry_num[corp_id]
                && g.bank_debt[corp_id] == Ext::from_f64(0.0)
                && g.credit_rating_num[corp_id] == 1
            {
                f.yield_pct = crate::pb::numeric::double(Ext::from_f64(10.0) + base);
            }
            // (0x4291e9..0x42936e: unreachable code after an unconditional jump — a recovery-yield
            // computation from (TmpTotalDebt + Cash) / TotalAssets that the original never executes.)
        }
        let crn = g.credit_rating_num[corp_id];
        if 1 < crn && crn < 5 && f.price.get() < Ext::from_f64(65.0) && Ext::from_f64(10.0) < f.yield_pct {
            // `price` is still 0 here, so the price test always passes.
            let mut net_debt = g.tmp_total_debt;
            if g.t_bills[corp_id] + g.cash[corp_id] > Ext::from_f64(0.0) {
                net_debt = (g.cash[corp_id] + net_debt) + g.t_bills[corp_id];
            }
            net_debt = net_debt - cast::<Ext, _>(g.bonds_issued[corp_id]);
            let bi = cast::<Ext, _>(g.bonds_issued[corp_id]);
            if bi > net_debt {
                f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.9f32)) * f.yield_pct);
            }
            if bi > net_debt * Ext::from_f64(1.5) {
                f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.9f32)) * f.yield_pct);
            }
            if bi > net_debt * Ext::from_f64(2.0) {
                f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.9f32)) * f.yield_pct);
            }
            if bi > net_debt * Ext::from_f64(3.0) {
                f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.8f32)) * f.yield_pct);
            }
        }
        corp_bond_pv(g, &mut f);
        if f.price.get() > Ext::from_f64(106.5) {
            let mut target_price: Cur;
            let mut years_left = to_long(cast::<Ext, _>(maturity_year) - cast::<Ext, _>(g.cur_year));
            if years_left < 11 {
                target_price = Cur::from_f64(((cast::<Ext, _>(years_left) + Ext::from_f64(100.0)) + Ext::from_f64(1.0)) + Ext::from_f64(1.0) * rng.rnd());
            } else {
                years_left = years_left - 10;
                price_cap = (Ext::from_f64(6.0) + cast::<Ext, _>(years_left)) + Ext::from_f64(1.0) * rng.rnd();
                if price_cap < Ext::from_f64(0.0) {
                    price_cap = Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(1.0);
                }
                price_cap = Ext::from_f64(100.0) + price_cap;
                if price_cap < f.price.get() {
                    target_price = Cur::from_f64(price_cap);
                } else {
                    target_price = Cur::from_f64(f.price.get() - Ext::from_f64(0.5));
                }
            }
            if !(target_price.get() < f.price.get()) {
                target_price = Cur::from_f64(f.price.get() - Ext::from_f64(0.5));
            }
            f.iter = 0;
            while f.price.get() > target_price.get() && f.iter < 1000 {
                f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.1f32)) + f.yield_pct);
                corp_bond_pv(g, &mut f);
            }
        }

        let nw_abs = g.net_worth.abs();
        if Ext::from_f64(5.0) > cast::<Ext, _>(f.periods) && g.credit_rating_num[corp_id] == 1 && f.price.get() >= Ext::from_f64(55.0) {
            price_cap = Ext::from_f64(3.0) * rng.rnd();
            f.iter = 0;
            f.price = Cur::from_f64((price_cap + Ext::from_f64(55.0)) + cast::<Ext, _>(f.periods) * Ext::from_f64(3.0));
        } else if is_d && f.price.get() >= Ext::from_f64(55.0) {
            price_cap = Ext::from_f64(3.0) * rng.rnd();
            f.iter = 0;
            let bi = cast::<Ext, _>(g.bonds_issued[corp_id]);
            if nw_abs > bi {
                f.price = Cur::from_f64(Ext::from_f64(25.0));
            } else if bi < nw_abs * (cast::<Ext, _>(1.2f32)) {
                f.price = Cur::from_f64(Ext::from_f64(35.0));
            } else if bi > nw_abs * Ext::from_f64(2.0) {
                if f.price.get() < Ext::from_f64(60.0) {
                    f.price = Cur::from_f64(Ext::from_f64(60.0));
                }
            } else {
                f.price = Cur::from_f64(Ext::from_f64(45.0));
            }
            f.price = Cur::from_f64((price_cap + f.price.get()) + cast::<Ext, _>(f.periods) * Ext::from_f64(2.0));
            if f.price.get() > Ext::from_f64(77.0) && g.credit_rating_num[corp_id] == 1 {
                f.price = Cur::from_f64(
                    ((cast::<Ext, _>(g.cur_quarter) + Ext::from_f64(70.0)) + fmod(cast::<Ext, _>(g.cur_year) + cast::<Ext, _>(corp_id), Ext::from_f64(5.0))) + Ext::from_f64(3.0) * rng.rnd(),
                );
            }
            if f.price.get() > Ext::from_f64(67.0) && g.credit_rating_num[corp_id] == 0 {
                f.price = Cur::from_f64(
                    ((cast::<Ext, _>(g.cur_quarter) + Ext::from_f64(60.0)) + fmod(cast::<Ext, _>(g.cur_year) + cast::<Ext, _>(corp_id), Ext::from_f64(5.0))) + Ext::from_f64(3.0) * rng.rnd(),
                );
            }
        } else if is_d && f.price.get() < Ext::from_f64(55.0) {
            let mut net_debt = g.tmp_total_debt;
            if g.t_bills[corp_id] + g.cash[corp_id] > Ext::from_f64(0.0) {
                net_debt = (g.cash[corp_id] + net_debt) + g.t_bills[corp_id];
            }
            let bi = cast::<Ext, _>(g.bonds_issued[corp_id]);
            if 85 < corp_id {
                net_debt = net_debt - (g.contingency_reserve[corp_id] + bi);
                let mut asset_cover = g.total_assets * (cast::<Ext, _>(0.95f32)) - net_debt;
                if asset_cover > Ext::from_f64(0.0) {
                    let debt_pct: Ext = Ext::from_f64(100.0) * (asset_cover / bi);
                    if f.price.get() < debt_pct {
                        f.price = Cur::from_f64(debt_pct);
                    }
                    f.price = Cur::from_f64(
                        (fmod(cast::<Ext, _>(g.cur_quarter) + cast::<Ext, _>(corp_id), Ext::from_f64(4.0)) + f.price.get()) + Ext::from_f64(3.0) * rng.rnd(),
                    );
                } else {
                    asset_cover = g.total_assets - net_debt;
                    if asset_cover < bi * (cast::<Ext, _>(0.1f32)) && g.credit_rating_num[corp_id] == 0 {
                        f.price = Cur::from_f64(((cast::<Ext, _>(2.2f32)) * rng.rnd() + Ext::from_f64(9.0)) - cast::<Ext, _>(g.cur_quarter));
                    } else {
                        f.price = Cur::from_f64(
                            ((cast::<Ext, _>(g.cur_quarter) + Ext::from_f64(24.0)) + fmod(cast::<Ext, _>(g.cur_year) + cast::<Ext, _>(corp_id), Ext::from_f64(5.0)))
                                + Ext::from_f64(3.0) * rng.rnd(),
                        );
                    }
                    if g.proj_qtr_earnings[corp_id] > Ext::from_f64(0.0) {
                        f.price = Cur::from_f64(Ext::from_f64(6.0) + f.price.get());
                    }
                }
                if g.credit_rating_num[corp_id] == 0 && f.price.get() > Ext::from_f64(55.0) && g.bank_debt[corp_id] > bi {
                    f.price = Cur::from_f64((cast::<Ext, _>(0.77f32)) * f.price.get());
                }
            } else if net_debt > Ext::from_f64(0.0) {
                // banks (11..85)
                let debt_pct: Ext = Ext::from_f64(100.0) * (bi / net_debt);
                let add = if debt_pct > Ext::from_f64(75.0) {
                    Ext::from_f64(50.0)
                } else if debt_pct > Ext::from_f64(62.0) {
                    Ext::from_f64(40.0)
                } else if debt_pct > Ext::from_f64(50.0) {
                    Ext::from_f64(33.0)
                } else if debt_pct > Ext::from_f64(35.0) {
                    Ext::from_f64(27.0)
                } else if debt_pct > Ext::from_f64(25.0) {
                    Ext::from_f64(21.0)
                } else if debt_pct > Ext::from_f64(15.0) {
                    Ext::from_f64(17.0)
                } else {
                    Ext::from_f64(12.0)
                };
                f.price = Cur::from_f64(base_price + add);
                if g.net_worth < Ext::from_f64(0.0) && nw_abs > g.total_assets * (cast::<Ext, _>(0.4f32)) {
                    f.price = Cur::from_f64((cast::<Ext, _>(0.4f32)) * f.price.get());
                } else if bi < nw_abs * (cast::<Ext, _>(1.2f32)) {
                    f.price = Cur::from_f64((cast::<Ext, _>(0.6f32)) * f.price.get());
                }
            }
            if g.tmp_total_debt * (cast::<Ext, _>(0.05f32)) > g.t_bills[corp_id] + g.cash[corp_id]
                && g.total_assets < g.tmp_total_debt * Ext::from_f64(0.5)
            {
                f.price = Cur::from_f64((cast::<Ext, _>(g.cur_quarter) + Ext::from_f64(9.0)) + Ext::from_f64(2.0) * rng.rnd());
            }
            if f.price.get() > Ext::from_f64(75.0) {
                f.price = Cur::from_f64(Ext::from_f64(75.0) - base_price);
            }
        }

        // LAB_0042a286
        if Ext::from_f64(5.0) > cast::<Ext, _>(f.periods) && f.price.get() > Ext::from_f64(80.0) && rating_str == "C" {
            let holder_face = to_quad(fmod(cast::<Ext, _>(corp_id), Ext::from_f64(5.0)));
            f.iter = 0;
            f.yield_pct = crate::pb::numeric::double(cast::<Ext, _>(g.cur_quarter) + f.yield_pct);
            while f.price.get() > (cast::<Ext, _>(holder_face) + Ext::from_f64(88.0)) - cast::<Ext, _>(f.periods) && f.iter < 1000 {
                f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.2f32)) + f.yield_pct);
                corp_bond_pv(g, &mut f);
            }
        }
        if f.price.get() < Ext::from_f64(0.0) {
            f.price = Cur::from_f64(Ext::from_f64(100.0));
            f.yield_pct = crate::pb::numeric::double(cast::<Ext, _>(coupon_rate));
        }
    }

    // LAB_0042a3bb
    g.bond_ytm = cast::<f32, _>(cast::<f32, _>(f.yield_pct));
    g.bond_price[corp_id] = f.price;
    if 0 < instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") && 1 < g.credit_rating_num[corp_id] {
        let conv_value = calc_convertible_bond_value(g, rng, corp_id, f.price);
        if conv_value.get() > f.price.get() {
            g.bond_price[corp_id] = conv_value;
            f.yield_pct = crate::pb::numeric::double(cast::<Ext, _>(g.bond_ytm) - Ext::from_f64(1.0));
            f.iter = 0;
            loop {
                // asm: the difference is taken before the GOSUB (from the previous price), and the "previous"
                // copies are made in the same iteration, so the two averaging branches are dead.
                let diff: Ext = f.price.get() - conv_value.get();
                let prev_diff = diff;
                let prev_yield = f.yield_pct;
                corp_bond_pv(g, &mut f);
                if -Ext::from_f64(100.0) > diff {
                    f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.98f32)) * f.yield_pct);
                } else if Ext::from_f64(100.0) < diff {
                    f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(1.02f32)) * f.yield_pct);
                } else if -Ext::from_f64(20.0) > diff {
                    f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.99f32)) * f.yield_pct);
                } else if Ext::from_f64(20.0) < diff {
                    f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(1.01f32)) * f.yield_pct);
                } else if -Ext::from_f64(5.0) > diff {
                    f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.995f32)) * f.yield_pct);
                } else if Ext::from_f64(5.0) < diff {
                    f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(1.005f32)) * f.yield_pct);
                } else if Ext::from_f64(0.0) < diff && Ext::from_f64(0.0) > prev_diff {
                    f.yield_pct = crate::pb::numeric::double((prev_yield + f.yield_pct) / Ext::from_f64(2.0));
                } else if Ext::from_f64(0.0) > diff && Ext::from_f64(0.0) < prev_diff {
                    f.yield_pct = crate::pb::numeric::double((prev_yield + f.yield_pct) / Ext::from_f64(2.0));
                } else {
                    if Ext::from_f64(0.0) < diff {
                        f.yield_pct = crate::pb::numeric::double((cast::<Ext, _>(0.02f32)) + f.yield_pct);
                    }
                    if Ext::from_f64(0.0) > diff {
                        f.yield_pct = crate::pb::numeric::double(f.yield_pct - (cast::<Ext, _>(0.018f32)));
                    }
                }
                if !(diff.abs() >= (cast::<Ext, _>(0.005f32)) && f.iter <= 1000) {
                    break;
                }
            }
            g.bond_ytm = cast::<f32, _>(cast::<f32, _>(f.yield_pct));
            if Ext::from_f32(0.0f32) > g.bond_ytm {
                g.bond_ytm = cast::<f32, _>(Ext::from_f32(0.0f32));
            }
            if conv_value.get() > Ext::from_f64(100.0) {
                let maturity_year = cast::<f32, _>(val(mid(g.bond_issue_info[corp_id].as_bytes(), 1, 4).as_bytes()));
                let mut years_left: Ext = cast::<Ext, _>(maturity_year) - cast::<Ext, _>(g.cur_year); // EXT local
                years_left = years_left - Ext::from_f64(0.25) * cast::<Ext, _>(g.cur_quarter);
                if g.sim_co_count < corp_id {
                    years_left = Ext::from_f64(0.25) + years_left;
                }
                if years_left == Ext::from_f64(0.0) {
                    years_left = cast::<Ext, _>(0.01f32);
                }
                if years_left > Ext::from_f64(0.0) && !(cast::<Ext, _>(coupon_rate) * years_left > conv_value.get() - Ext::from_f64(100.0)) {
                    g.bond_ytm = cast::<f32, _>(Ext::from_f32(0.0f32));
                }
            }
            if conv_value.get() > (cast::<Ext, _>(99.9f32)) && conv_value.get() < (cast::<Ext, _>(100.1f32)) {
                g.bond_ytm = cast::<f32, _>(coupon_rate);
            }
        }
    }
    if Ext::from_f32(0.0f32) > g.bond_ytm {
        g.bond_ytm = cast::<f32, _>(Ext::from_f32(0.0f32));
    }

    f.public_face = cast::<Ext, _>(g.bonds_issued[corp_id]);
    g.largest_bondholder = 0;
    let mut largest_face: Ext = Ext::from_f64(0.0);
    f.iter = 1;
    while f.iter <= 1583 {
        let holder = f.iter;
        if holder <= 130 || 1581 <= holder {
            for slot in 1..=15 {
                if g.hold_bond_id[(slot, holder)] == corp_id {
                    let holder_face = g.hold_bond_face[(slot, holder)];
                    f.public_face = f.public_face - cast::<Ext, _>(holder_face);
                    if cast::<Ext, _>(holder_face) > largest_face {
                        g.largest_bondholder = holder;
                        largest_face = cast::<Ext, _>(holder_face);
                    }
                }
            }
        }
        f.iter += 1;
    }
    g.bonds_public_held = to_quad(f.public_face);
    if (cast::<Ext, _>(g.bonds_public_held)) < Ext::from_f64(0.0) && cast::<Ext, _>(g.bonds_issued[corp_id]) > Ext::from_f64(0.0) {
        g.bonds_issued[corp_id] = to_quad(cast::<Ext, _>(g.bonds_issued[corp_id]) + Ext::from_f64(1.0));
    }
    if cast::<Ext, _>(g.bonds_public_held) > Ext::from_f64(250.0)
        && cast::<Ext, _>(g.bonds_public_held) > cast::<Ext, _>(g.bonds_issued[corp_id]) * (cast::<Ext, _>(0.3f32))
        && g.difficulty_level == 3
        && 0 < g.corp_controller[corp_id]
        && g.corp_controller[corp_id] < 11
        && get_controller_type(g, corp_id) == 1
    {
        g.player_recent_corp = cast::<i16, _>(corp_id);
    }
    if g.bond_price[corp_id].get() > Ext::from_f64(1100.0) {
        g.pending_forced_conv_corp = corp_id;
    }
    // CalcCorpBondPrice_Exit: empty
}

/// Port of CalcConvertibleBondValue (0x42ad61).
/// Value per 100 face of `corp_ref`'s convertible bond given its straight price `bond_price`: conversion
/// value (`BondIssueInfo` field 2 = conversion market cap) plus a premium that shrinks towards maturity,
/// with special cases for the last two years. Non-convertible issues return `bond_price` unchanged;
/// defaulted issuers (rating < 2) whose conversion value is below the price get `1.05 * price`.
pub fn calc_convertible_bond_value(g: &mut Globals, rng: &mut Rng, corp_ref: i32, bond_price: Cur) -> Cur {
    let mut value = bond_price;
    let corp_id = corp_ref;
    if instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") <= 0 {
        return value;
    }
    let issue_info = g.bond_issue_info[corp_id].clone();
    let maturity_year = to_long(val(mid(issue_info.as_bytes(), 1, 4).as_bytes()));
    let conv_cap = val(parse_any(issue_info.as_bytes(), b"|", 2).as_bytes());
    let conv_shares = Ext::from_f64(1000.0) / (conv_cap / g.shares_outstanding[corp_id]);
    let conv_value = ((g.market_cap[corp_id] / g.shares_outstanding[corp_id]) * conv_shares) * (cast::<Ext, _>(0.1f32));
    if g.credit_rating_num[corp_id] < 2 && conv_value < value.get() {
        return Cur::from_f64((cast::<Ext, _>(1.05f32)) * value.get());
    }
    let mc = g.market_cap[corp_id];
    let mut premium: Ext = if mc > conv_cap {
        Ext::from_f64(17.0)
    } else if mc > conv_cap * (cast::<Ext, _>(0.9f32)) {
        Ext::from_f64(13.0)
    } else if mc > conv_cap * (cast::<Ext, _>(0.8f32)) {
        Ext::from_f64(8.0)
    } else if mc > conv_cap * (cast::<Ext, _>(0.7f32)) {
        Ext::from_f64(6.0)
    } else if mc > conv_cap * (cast::<Ext, _>(0.6f32)) {
        Ext::from_f64(4.0)
    } else if mc > conv_cap * Ext::from_f64(0.5) {
        Ext::from_f64(3.0)
    } else {
        Ext::from_f64(2.0)
    };
    if premium < Ext::from_f64(4.0) && maturity_year - g.cur_year < 2 && 1 < g.cur_quarter {
        return value;
    }
    let mut qtrs_left: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE local
    if maturity_year - g.cur_year < 3 {
        qtrs_left = crate::pb::numeric::double(Ext::from_f64(8.0) - (cast::<Ext, _>(g.cur_quarter) - Ext::from_f64(1.0)));
        if maturity_year - 1 == g.cur_year {
            qtrs_left = crate::pb::numeric::double(qtrs_left - Ext::from_f64(4.0));
        }
        if maturity_year == g.cur_year {
            qtrs_left = crate::pb::numeric::double(cast::<Ext, _>(0.01f32));
        }
        if corp_id < g.sim_co_count {
            qtrs_left = crate::pb::numeric::double(qtrs_left - Ext::from_f64(0.5)); // asm: DOUBLE arithmetic
        }
        if (cast::<Ext, _>(0.4f32)) < qtrs_left {
            premium = premium - (Ext::from_f64(1.0) / (Ext::from_f64(1.0) + qtrs_left)) * premium;
        } else if Ext::from_f64(0.0) < qtrs_left {
            premium = premium * (cast::<Ext, _>(0.01f32));
        }
    }
    value = Cur::from_f64(premium + value.get());

    premium = if conv_value > Ext::from_f64(135.0) {
        Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(1.5)
    } else if conv_value > Ext::from_f64(120.0) {
        ((Ext::from_f64(4.0) + conv_value) - (cast::<Ext, _>(0.1f32)) * (conv_value - Ext::from_f64(120.0))) - conv_value
    } else if conv_value > Ext::from_f64(110.0) {
        ((Ext::from_f64(7.0) + conv_value) - (cast::<Ext, _>(0.2f32)) * (conv_value - Ext::from_f64(110.0))) - conv_value
    } else if conv_value > Ext::from_f64(100.0) {
        Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(8.0)
    } else if conv_value > Ext::from_f64(95.0) {
        Ext::from_f64(2.5) * rng.rnd() + Ext::from_f64(11.0)
    } else if conv_value > Ext::from_f64(90.0) {
        Ext::from_f64(2.5) * rng.rnd() + Ext::from_f64(13.5)
    } else {
        Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(16.0)
    };
    if (cast::<Ext, _>(0.4f32)) < qtrs_left {
        premium = premium - (Ext::from_f64(1.0) / (Ext::from_f64(1.0) + qtrs_left)) * premium;
    } else if Ext::from_f64(0.0) < qtrs_left {
        premium = premium * (cast::<Ext, _>(0.01f32));
    }
    let cand_price = Cur::from_f64(premium + conv_value);
    if cand_price.get() > value.get() {
        value = cand_price;
    }
    let bp = bond_price.get();
    if maturity_year - g.cur_year < 2 && bp < Ext::from_f64(100.0) {
        if g.cur_quarter < 3 && g.cur_year < maturity_year && mc < conv_cap * Ext::from_f64(0.75) {
            value = Cur::from_f64((Ext::from_f64(3.0) + bp) + Ext::from_f64(1.5) * rng.rnd());
            if value.get() > (cast::<Ext, _>(103.3f32)) {
                value = Cur::from_f64((cast::<Ext, _>(1.4f32)) * rng.rnd() + Ext::from_f64(103.5));
            }
        } else if g.cur_year < maturity_year && 2 < g.cur_quarter && mc < conv_cap * (cast::<Ext, _>(0.9f32)) {
            value = Cur::from_f64((Ext::from_f64(1.0) + bp) + (cast::<Ext, _>(0.8f32)) * rng.rnd());
            if value.get() > Ext::from_f64(100.5) {
                value = Cur::from_f64((cast::<Ext, _>(1.2f32)) * rng.rnd() + Ext::from_f64(99.5));
            }
            if g.cur_quarter == 4 && bp > Ext::from_f64(99.0) {
                value = Cur::from_f64((cast::<Ext, _>(1.3f32)) * rng.rnd() + Ext::from_f64(99.0));
            }
        } else if maturity_year == g.cur_year && mc < conv_cap * (cast::<Ext, _>(0.96f32)) {
            value = Cur::from_f64(((cast::<Ext, _>(0.3f32)) + bp) + Ext::from_f64(1.0) * rng.rnd());
            if value.get() > Ext::from_f64(99.5) {
                value = Cur::from_f64(Ext::from_f64(100.0));
            }
        }
    }
    value
}

// ---------------------------------------------------------------------------------------------------------
// CalcEntityFinancials ("CorpInfoItems")
// ---------------------------------------------------------------------------------------------------------

/// Locals of CalcEntityFinancials shared with its GOSUB blocks (all zero at entry, like PB locals).
#[derive(Default)]
struct FinFrame {
    entity_id: i32,
    /// 0 = player, 1 = bank, 2 = insurer, 3 = other corp / fund (selects the Totals branch).
    entity_kind: i32,
    /// Bank: customer deposits (players' and corps' positive cash with this bank).
    cust_deposits: Ext,
    prev_cust_deposits: Ext,
    /// Bank debt owed to the lending bank by its borrowers.
    cust_loans: Ext,
    largest_loan: Ext,
    largest_debtor: i32,
    /// Lending bank's total loans (customers + consumer + mortgage + subprime).
    total_loans: Ext,
    total_liabs: Ext,
    bank_id: i32,
}

/// Port of CalcEntityFinancials (0x426610), "CorpInfoItems", with its GOSUBs CreditLine (0x426e16),
/// BankLoan (0x4271f2), BondHoldings (0x427494), BankLoanBook (0x427843), SubsidStock (0x427d18),
/// PlayerAdvances (0x427dfb), BankTotals (0x427eda) and Totals (0x428023).
/// Balance-sheet summary of player 1..NumPlayers or entity 11..1600 into the `Tmp*`/`Ci*` globals,
/// `TotalAssets`, `NetWorth`, `TmpTotalDebt`, `UnusedCreditLine`, `LiquidFunds`, credit rating, bond
/// price (corps with bonds) and possibly `TakeoverCandidateId`.
///
/// Side effects of the original kept here: lending bank assigned at random if missing, small player
/// advances settled into cash, bank customer deposits re-totalled (cash adjusted by the change), negative
/// `BankDebt` of borrowers cleared, `BankLargestDebtor`, `ReserveAdjPending` for corps with negative net
/// worth, `CreditFrozen`.
///
/// The GOSUBs Totals/BankTotals value the entity's options (CalcOptionsValue 0x5f23e1), futures
/// (CalcEntityFuturesNetValue 0x61c244) and, for players, re-mark all short positions
/// (RecalcShortMarginBalances 0x5e0ade).
///
/// UI: only through UpdateLoanLimitMultiple, which shows a pending `NewsMsg` (ShowQuickMessage) when the
/// credit line is first computed with `LoanLimitMultiple = 0`.
pub fn calc_entity_financials(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_ref: i32) {
    let entity_id = entity_ref;
    if entity_id < 1 {
        // ON ERROR: ShowRuntimeError(ERR, "CorpInfoItems", "X&=" + STR$(entityId)) — error path ignored.
        return;
    }
    if !(entity_id <= g.num_players || 10 < entity_id) {
        return;
    }
    g.tmp_bank_loan = Ext::from_f64(0.0);
    g.lender_id = 0;
    g.lender_bank_name = PbStr::new();
    g.ci_bonds_outstanding = 0;
    g.tmp_corp_bonds_value = Ext::from_f64(0.0);
    g.tmp_subsid_stock_value = Ext::from_f64(0.0);
    g.ci_gov_long_bond_value = Ext::from_f64(0.0);
    g.short_bond_value = Ext::from_f64(0.0);
    g.tmp_gov_bonds_value = Ext::from_f64(0.0);
    g.bank_largest_loan = Ext::from_f64(0.0);
    g.net_worth = Ext::from_f64(0.0);
    g.tmp_credit_net_worth = Ext::from_f64(0.0);
    g.tmp_bank_total_loans = Ext::from_f64(0.0);
    g.unused_credit_line = Ext::from_f64(0.0);
    g.tmp_total_debt = Ext::from_f64(0.0);
    g.total_assets = Ext::from_f64(0.0);
    g.bank_loans_total = Ext::from_f64(0.0);
    g.tmp_accrued_inc_tax = Ext::from_f64(0.0);
    g.capital_taxes_total = Ext::from_f64(0.0);
    g.tmp_player_advances = Ext::from_f64(0.0);
    g.options_net_value = Ext::from_f64(0.0);
    g.long_options_value = Ext::from_f64(0.0);
    g.options_cost_basis = Ext::from_f64(0.0);
    g.ci_opt_intrinsic_val = Ext::from_f64(0.0);
    g.futures_net_value = Ext::from_f64(0.0);
    g.phys_commod_value = Ext::from_f64(0.0);
    g.bond_unrealized_loss = Ext::from_f64(0.0);
    if g.entity_status[entity_id] != 1 {
        return;
    }
    let mut f = FinFrame { entity_id, ..Default::default() };
    fin_bank_loan(g, rng, &mut f);
    fin_subsid_stock(g, &f);
    g.phys_commod_value = get_phys_commod_value(g, entity_id, &PbStr::new());
    calc_gov_bond_yields(g);
    if entity_id < 131 || 1580 < entity_id {
        fin_bond_holdings(g, &f);
        if 10 < entity_id && entity_id < 86 {
            fin_bank_loan_book(g, &mut f);
            g.tmp_bank_total_loans = f.total_loans;
            g.bank_loans_total = f.cust_loans;
            g.bank_largest_loan = f.largest_loan;
        } else if entity_id < 6 {
            fin_player_advances(g, &f);
        }
    }
    if 10 < entity_id {
        let mut t = cast::<Ext, _>(g.extraordinary_pl[entity_id]) + g.accrued_pl[entity_id];
        t = t + g.div_recvd1[entity_id] * (cast::<Ext, _>(0.35f32));
        t = t + g.div_recvd2[entity_id] * Ext::from_f64(0.5);
        t = t + g.consol_tax_inc_ytd[entity_id];
        g.tmp_accrued_inc_tax = t;
        if g.tax_loss_carry[entity_id] < Ext::from_f64(0.0) {
            g.tmp_accrued_inc_tax = g.tax_loss_carry[entity_id] + g.tmp_accrued_inc_tax;
        }
        if entity_id < 86 {
            // banks: deduct the (rounded) quarter of the reserve shortfall and 0.45% of deposits
            let reserve_adj = to_quad(
                Ext::from_f64(0.25) * (g.tmp_bank_total_loans * (cast::<Ext, _>(0.012f32)) - g.bad_debt_reserve[entity_id]),
            );
            g.tmp_accrued_inc_tax = g.tmp_accrued_inc_tax - cast::<Ext, _>(reserve_adj);
            g.tmp_accrued_inc_tax = g.tmp_accrued_inc_tax
                - ((g.bank_demand_deposits[entity_id] + g.bank_c_ds[entity_id]) + f.cust_deposits)
                    * (cast::<Ext, _>(0.0045f32));
        }
        if g.tmp_accrued_inc_tax > Ext::from_f64(0.0) {
            g.tmp_accrued_inc_tax = g.tmp_accrued_inc_tax * (cast::<Ext, _>(g.corp_tax_rate) * (cast::<Ext, _>(0.01f32)));
        } else {
            g.tmp_accrued_inc_tax = Ext::from_f64(0.0);
        }
        g.tmp_accrued_inc_tax = g.tmp_accrued_inc_tax - g.tax_credits[entity_id];
        if g.tmp_accrued_inc_tax < Ext::from_f64(0.0) {
            g.tmp_accrued_inc_tax = Ext::from_f64(0.0);
        }
        if 2 < g.difficulty_level {
            g.capital_taxes_total = calc_capital_taxes(g, entity_id);
        }
    }
    if 10 < entity_id && g.industry_num[entity_id] == 71 {
        g.tmp_accrued_inc_tax = Ext::from_f64(0.0);
        g.capital_taxes_total = Ext::from_f64(0.0);
    }
    if entity_id < 11 {
        fin_totals(g, ui, rng, &mut f);
    } else if g.industry_num[entity_id] == 1 {
        f.entity_kind = 1;
        fin_bank_totals(g, ui, rng, &mut f);
    } else {
        f.entity_kind = 3;
        if g.industry_num[entity_id] == 2 {
            f.entity_kind = 2;
        }
        fin_totals(g, ui, rng, &mut f);
    }
    g.net_worth = g.total_assets - f.total_liabs;
    if 10 < entity_id && g.net_worth < Ext::from_f64(0.0) && g.contingency_reserve[entity_id] > Ext::from_f64(0.0) {
        g.reserve_adj_pending[entity_id] = -g.contingency_reserve[entity_id]; // * -1
    }
    g.tmp_credit_net_worth = (g.net_worth + g.ci_opt_intrinsic_val) - g.long_options_value;
    fin_credit_line(g, ui, &mut f);
    g.liquid_funds = (g.t_bills[entity_id] + g.cash[entity_id]) + g.unused_credit_line;
    g.bonds_public_held = 0;
    if 10 < entity_id && cast::<Ext, _>(g.bonds_issued[entity_id]) > Ext::from_f64(0.0) {
        // rate the bonds on net debt (debt less cash), then restore TmpTotalDebt
        let saved_debt = g.tmp_total_debt;
        if g.t_bills[entity_id] + g.cash[entity_id] > Ext::from_f64(0.0) {
            g.tmp_total_debt = g.tmp_total_debt - (g.t_bills[entity_id] + g.cash[entity_id]);
        }
        if g.tmp_total_debt < Ext::from_f64(0.0) {
            g.tmp_total_debt = Ext::from_f64(0.0);
        }
        calc_credit_rating(g, entity_id);
        calc_corp_bond_price(g, rng, entity_id);
        g.tmp_total_debt = saved_debt;
    }
    calc_credit_rating(g, entity_id);
    if 10 < entity_id && g.market_cap[entity_id] < ((cast::<Ext, _>(0.01f32)) * g.net_worth) * (cast::<Ext, _>(0.83f32)) {
        if Ext::from_f64(20.0) * rng.rnd() < Ext::from_f64(1.0) {
            g.takeover_candidate_id = entity_id;
        }
    }
}

/// Partial port of UpdateLoanLimitMultiple (0x4bd563): the path taken when `LoanLimitMultiple` is still
/// 0 (the only way CalcEntityFinancials_CreditLine calls it). `LoanLimitMultiple` becomes
/// `ROUND(25 + adj) / 10` with `adj` from the prime rate (-5..+5); a pending `NewsMsg` is shown. The
/// credit-line news part of the original requires a previous multiple > 0 and is therefore unreachable
/// from here.
fn update_loan_limit_multiple_from_zero(g: &mut Globals, ui: &mut dyn Ui) {
    g.loan_limit_multiple = cast::<f64, _>(Ext::from_f64(2.5));
    let prime = cast::<Ext, _>(g.prime_rate);
    let mut mult_adj: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE local
    if Ext::from_f64(8.0) < prime {
        mult_adj = crate::pb::numeric::double((cast::<Ext, _>(0.7f32)) * (prime - Ext::from_f64(8.0)));
        if Ext::from_f64(5.0) < mult_adj {
            mult_adj = crate::pb::numeric::double(Ext::from_f64(5.0));
        }
        mult_adj = crate::pb::numeric::double(-mult_adj); // * -1
    } else if (cast::<Ext, _>(6.75f32)) > prime {
        mult_adj = crate::pb::numeric::double((cast::<Ext, _>(1.7f32)) * (Ext::from_f64(7.0) - prime));
        if Ext::from_f64(5.0) < mult_adj {
            mult_adj = crate::pb::numeric::double(Ext::from_f64(5.0));
        }
    }
    let steps = to_long(mult_adj + Ext::from_f64(25.0));
    g.loan_limit_multiple = cast::<f64, _>(cast::<Ext, _>(steps) * (cast::<Ext, _>(0.1f32)));
    if !g.news_msg.is_empty() {
        crate::logic::report::show_quick_news(g, ui);
    }
}

/// GOSUB CalcEntityFinancials_CreditLine (0x426e16): unused bank credit line of the entity
/// (`UnusedCreditLine`), limited by its net worth, `LoanLimitMultiple` and the lending bank's capacity.
fn fin_credit_line(g: &mut Globals, ui: &mut dyn Ui, f: &mut FinFrame) {
    let id = f.entity_id;
    if !(g.tmp_credit_net_worth >= g.tmp_total_debt * (cast::<Ext, _>(0.333f32))) {
        return;
    }
    g.unused_credit_line = Ext::from_f64(0.0);
    if 10 < id && g.industry_num[id] == 1 {
        return;
    }
    let mut credit_mult: f32 = cast::<f32, _>(Ext::from_f32(1.0f32));
    let lender_ctrl = cast::<i16, _>(g.corp_controller[g.lender_id]); // INTEGER local
    if lender_ctrl == 0 || 10 < lender_ctrl {
        g.credit_frozen[id] = g.credit_frozen[g.lender_id];
    }
    let entity_ctrl = cast::<i16, _>(g.corp_controller[id]);
    if 0 < lender_ctrl && lender_ctrl == entity_ctrl && get_controller_type(g, id) != 1 {
        g.credit_frozen[id] = 0;
    }
    if g.credit_frozen[id] == 1 {
        return;
    }
    if lender_ctrl == entity_ctrl && 0 < lender_ctrl {
        credit_mult = cast::<f32, _>(g.difficulty_level);
    }
    if cast::<i32, _>(lender_ctrl) == id {
        credit_mult = cast::<f32, _>(g.difficulty_level);
    }
    if g.loan_limit_multiple == Ext::from_f64(0.0) && 6 < g.sim_co_count {
        update_loan_limit_multiple_from_zero(g, ui);
    }
    let mut credit_limit: Ext;
    if 85 < id && 2 < g.difficulty_level {
        credit_limit = g.tmp_credit_net_worth * g.loan_limit_multiple;
    } else {
        if 10 < id || Ext::from_f32(1.0f32) > credit_mult {
            credit_mult = cast::<f32, _>(g.loan_limit_multiple - Ext::from_f64(0.5));
        }
        credit_limit = g.tmp_credit_net_worth * cast::<Ext, _>(credit_mult);
    }
    if 1580 < id && g.industry_num[id] == 71 {
        credit_limit = g.tmp_credit_net_worth * (cast::<Ext, _>(0.333f32));
    }
    g.unused_credit_line = credit_limit - g.tmp_total_debt;
    if g.unused_credit_line < Ext::from_f64(0.0) {
        g.unused_credit_line = Ext::from_f64(0.0);
    }
    if g.unused_credit_line == Ext::from_f64(0.0) {
        return;
    }
    fin_bank_loan_book(g, f);
    let min_limit = g.exchange_rate * Ext::from_f64(10000.0);
    let mut lend_limit = f.total_loans * Ext::from_f64(0.25);
    if !(lend_limit < min_limit) {
        let net_loans = f.total_loans - g.tmp_bank_loan;
        lend_limit = net_loans / Ext::from_f64(3.0);
    }
    if lend_limit < min_limit {
        lend_limit = min_limit;
    }
    if credit_limit > lend_limit {
        credit_limit = lend_limit;
        let lend_room = credit_limit - g.tmp_bank_loan;
        if lend_room < g.unused_credit_line {
            g.unused_credit_line = lend_room;
        }
    }
    if g.unused_credit_line < Ext::from_f64(0.0) {
        g.unused_credit_line = Ext::from_f64(0.0);
    }
}

/// GOSUB CalcEntityFinancials_BankLoan (0x4271f2): lending bank (assigned at random 11..50 if none),
/// bank loan, bonds outstanding; for corps also settles player advances below 0.01 back into the
/// players' cash and totals the rest into `TmpPlayerAdvances`.
fn fin_bank_loan(g: &mut Globals, rng: &mut Rng, f: &mut FinFrame) {
    let id = f.entity_id;
    g.lender_id = cast::<i32, _>(g.lending_bank[id]);
    if g.lender_id == 0 {
        let r = rng.rnd_range(40, 1);
        g.lending_bank[id] = cast::<i16, _>(r + 10);
        g.lender_id = cast::<i32, _>(g.lending_bank[id]);
    }
    g.tmp_bank_loan = g.bank_debt[id];
    g.lender_bank_name = g.ent_name[g.lender_id].clone();
    if 10 < id {
        g.ci_bonds_outstanding = g.bonds_issued[id];
        g.tmp_player_advances = Ext::from_f64(0.0);
        for idx in 1..=5 {
            if g.player_advances[(idx, id)] < (cast::<Ext, _>(0.01f32)) {
                g.cash[idx] = g.player_advances[(idx, id)] + g.cash[idx];
                g.cash[id] = g.cash[id] - g.player_advances[(idx, id)];
                g.player_advances[(idx, id)] = Ext::from_f64(0.0);
            }
            if g.player_advances[(idx, id)] > Ext::from_f64(0.0) {
                g.tmp_player_advances = g.player_advances[(idx, id)] + g.tmp_player_advances;
            }
        }
    }
}

/// GOSUB CalcEntityFinancials_BondHoldings (0x427494): value of corporate and government bonds held.
/// Banks and insurers (11..130) carry bonds at cost (corporate bonds of defaulted issuers at the lower of
/// cost and market); `BondUnrealizedLoss` = book value - market value (0 for players and funds' corp bonds).
fn fin_bond_holdings(g: &mut Globals, f: &FinFrame) {
    let id = f.entity_id;
    let mut mkt_value: Ext = Ext::from_f64(0.0); // only assigned for 11..130; keeps its value across slots
    for idx in 1..=15 {
        if 0 < g.hold_bond_id[(idx, id)] {
            let bond_issuer = g.hold_bond_id[(idx, id)];
            let mut hold_value: Ext;
            if 10 < id && id < 131 && !(1 >= g.credit_rating_num[bond_issuer]) {
                hold_value = g.bond_hold_cost[(idx, id)];
                mkt_value =
                    (cast::<Ext, _>(0.01f32)) * g.bond_price[bond_issuer].get() * cast::<Ext, _>(g.hold_bond_face[(idx, id)]);
            } else {
                hold_value =
                    (cast::<Ext, _>(0.01f32)) * g.bond_price[bond_issuer].get() * cast::<Ext, _>(g.hold_bond_face[(idx, id)]);
                if 10 < id && id < 131 {
                    mkt_value = hold_value;
                    if g.bond_hold_cost[(idx, id)] < hold_value {
                        hold_value = g.bond_hold_cost[(idx, id)];
                    }
                }
            }
            g.tmp_corp_bonds_value = hold_value + g.tmp_corp_bonds_value;
            g.bond_unrealized_loss = mkt_value + g.bond_unrealized_loss;
        }
    }
    if 10 < id && id < 131 {
        g.ci_gov_long_bond_value = g.lt_gov_bond_cost[id];
        g.short_bond_value = g.st_gov_bond_cost[id];
        let short_mkt = (cast::<Ext, _>(g.short_bond_price) * cast::<Ext, _>(g.short_gov_bond_face[id])) * (cast::<Ext, _>(0.01f32));
        let long_mkt = (cast::<Ext, _>(g.long_bond_price) * cast::<Ext, _>(g.long_gov_bond_face[id])) * (cast::<Ext, _>(0.01f32));
        mkt_value = short_mkt + long_mkt;
        g.bond_unrealized_loss = mkt_value + g.bond_unrealized_loss;
    } else {
        g.ci_gov_long_bond_value = (cast::<Ext, _>(g.long_bond_price) * cast::<Ext, _>(g.long_gov_bond_face[id])) * (cast::<Ext, _>(0.01f32));
        g.short_bond_value = (cast::<Ext, _>(g.short_bond_price) * cast::<Ext, _>(g.short_gov_bond_face[id])) * (cast::<Ext, _>(0.01f32));
    }
    g.tmp_gov_bonds_value = g.short_bond_value + g.ci_gov_long_bond_value;
    g.bond_unrealized_loss = (g.tmp_corp_bonds_value + g.tmp_gov_bonds_value) - g.bond_unrealized_loss;
}

/// GOSUB CalcEntityFinancials_BankLoanBook (0x427843): loan book of the entity's lending bank (or of the
/// entity itself if it is a bank): customer loans, largest debtor, total loans; for a bank also re-totals
/// its customer deposits (positive cash of its borrowers) and books the change into its cash.
fn fin_bank_loan_book(g: &mut Globals, f: &mut FinFrame) {
    let id = f.entity_id;
    f.cust_loans = Ext::from_f64(0.0);
    f.largest_loan = Ext::from_f64(0.0);
    f.largest_debtor = 0;
    if 10 < id && g.industry_num[id] == 1 {
        f.bank_id = id;
        f.prev_cust_deposits = g.bank_cust_deposits[id];
        g.bank_cust_deposits[id] = Ext::from_f64(0.0);
        f.cust_deposits = Ext::from_f64(0.0);
    } else {
        f.bank_id = cast::<i32, _>(g.lending_bank[id]);
    }
    let bank_id = f.bank_id;
    for idx in 1..=1600 {
        if bank_id == idx || g.entity_status[idx] != 1 {
            continue;
        }
        if !(idx <= g.num_players || 10 < idx) {
            continue;
        }
        if g.bank_debt[idx] < Ext::from_f64(0.0) {
            g.bank_debt[idx] = Ext::from_f64(0.0);
        }
        if 10 < idx && idx < 86 {
            continue; // banks do not borrow from banks here
        }
        if cast::<i32, _>(g.lending_bank[idx]) == bank_id {
            f.cust_loans = g.bank_debt[idx] + f.cust_loans;
            if id == bank_id && g.cash[idx] > Ext::from_f64(0.0) {
                f.cust_deposits = g.cash[idx] + f.cust_deposits;
            }
            if g.bank_debt[idx] > f.largest_loan {
                f.largest_loan = g.bank_debt[idx];
                f.largest_debtor = idx;
            }
        }
    }
    if 10 < id && g.industry_num[id] == 1 {
        g.bank_cust_deposits[id] = f.cust_deposits;
        let deposit_change = f.cust_deposits - f.prev_cust_deposits;
        if deposit_change != Ext::from_f64(0.0) {
            g.cash[id] = deposit_change + g.cash[id];
        }
        if f.cust_deposits < Ext::from_f64(0.0) {
            f.cust_deposits = Ext::from_f64(0.0);
        }
    }
    f.total_loans = ((cast::<Ext, _>(g.bank_consumer_loans[bank_id]) + f.cust_loans)
        + cast::<Ext, _>(g.bank_mortgage_loans[bank_id]))
        + cast::<Ext, _>(g.subprime_secs[bank_id]);
    if bank_id < 85 {
        f.total_loans = g.bank_debt[bank_id + 1] + f.total_loans;
    }
    if f.largest_loan > f.total_loans * Ext::from_f64(0.25) && f.largest_loan > g.exchange_rate * Ext::from_f64(10000.0) {
        g.bank_largest_debtor[bank_id] = f.largest_debtor;
    } else {
        g.bank_largest_debtor[bank_id] = 0;
    }
}

/// GOSUB CalcEntityFinancials_SubsidStock (0x427d18): market value of the stock holdings
/// (`MarketCap * percent`, i.e. $M since MarketCap is in $100M) into `TmpSubsidStockValue`.
fn fin_subsid_stock(g: &mut Globals, f: &FinFrame) {
    let id = f.entity_id;
    g.tmp_subsid_stock_value = Ext::from_f64(0.0);
    for idx in 1..=15 {
        if 0 < g.hold_stock_id[(idx, id)] {
            let subsid_id = cast::<i32, _>(g.hold_stock_id[(idx, id)]);
            let hold_value = g.market_cap[subsid_id] * cast::<Ext, _>(g.hold_stock_pct[(idx, id)]);
            g.tmp_subsid_stock_value = hold_value + g.tmp_subsid_stock_value;
        }
    }
}

/// GOSUB CalcEntityFinancials_PlayerAdvances (0x427dfb): player's advances to corps 11..1600 (amounts
/// below 0.001 are cleared) into `TmpPlayerAdvances`.
fn fin_player_advances(g: &mut Globals, f: &FinFrame) {
    let id = f.entity_id;
    for idx in 11..=1600 {
        if g.player_advances[(id, idx)] < (cast::<Ext, _>(0.001f32)) {
            g.player_advances[(id, idx)] = Ext::from_f64(0.0);
        }
        if g.player_advances[(id, idx)] > Ext::from_f64(0.0) {
            g.tmp_player_advances = g.player_advances[(id, idx)] + g.tmp_player_advances;
        }
    }
}

/// GOSUB CalcEntityFinancials_BankTotals (0x427eda): total assets, debt and liabilities of a bank.
fn fin_bank_totals(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut FinFrame) {
    let id = f.entity_id;
    crate::logic::options::calc_options_value(g, ui, rng, id);
    let mut bad_debt_res = g.bad_debt_reserve[id];
    if g.bad_debt_reserve[id] < Ext::from_f64(0.0) {
        bad_debt_res = Ext::from_f64(0.0);
    }
    g.total_assets = (((((g.t_bills[id] + g.cash[id]) + g.tmp_gov_bonds_value) + g.tmp_corp_bonds_value)
        + (g.tmp_bank_total_loans - bad_debt_res))
        + g.tmp_subsid_stock_value)
        + g.options_net_value;
    g.tmp_total_debt = (((cast::<Ext, _>(g.ci_bonds_outstanding) + g.tmp_bank_loan) + g.tmp_accrued_inc_tax)
        + g.capital_taxes_total)
        + g.tmp_player_advances;
    f.total_liabs = ((f.cust_deposits + g.bank_c_ds[id]) + g.bank_demand_deposits[id]) + g.tmp_total_debt;
}

/// GOSUB CalcEntityFinancials_Totals (0x428023): total assets, debt and liabilities of a player (kind 0),
/// insurer (2) or other corp/fund (3).
fn fin_totals(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut FinFrame) {
    let id = f.entity_id;
    let kind_sel = f.entity_kind;
    crate::logic::options::calc_options_value(g, ui, rng, id);
    if kind_sel == 0 {
        crate::logic::shorts::recalc_short_margin_balances(g);
        g.futures_net_value = crate::logic::futures::calc_entity_futures_net_value(g, ui, rng, id);
        g.total_assets = (((((g.t_bills[id] + g.cash[id]) + g.tmp_gov_bonds_value) + g.tmp_corp_bonds_value)
            + g.tmp_subsid_stock_value)
            + g.tmp_player_advances)
            + g.options_net_value;
        let t = (g.player_short_margin_bal[id] + g.total_assets) + g.futures_net_value;
        g.total_assets = (t + g.commod_margin_bal[id]) + g.phys_commod_value;
        g.tmp_total_debt = g.tmp_bank_loan;
        f.total_liabs = g.tmp_total_debt;
    } else if kind_sel == 2 {
        crate::logic::futures::calc_entity_futures_net_value(g, ui, rng, id);
        g.total_assets = (((((((g.t_bills[id] + g.cash[id]) + g.tmp_gov_bonds_value) + g.tmp_corp_bonds_value)
            + g.tmp_subsid_stock_value)
            + g.options_net_value)
            + g.futures_net_value)
            + g.commod_margin_bal[id])
            + cast::<Ext, _>(g.subprime_secs[id]);
        g.tmp_total_debt = (((cast::<Ext, _>(g.ci_bonds_outstanding) + g.tmp_bank_loan) + g.tmp_accrued_inc_tax)
            + g.capital_taxes_total)
            + g.tmp_player_advances;
        f.total_liabs = (g.ins_reserves[id] + g.tmp_total_debt) + g.contingency_reserve[id];
    } else {
        crate::logic::futures::calc_entity_futures_net_value(g, ui, rng, id);
        g.total_assets = ((((((((g.t_bills[id] + g.cash[id]) + g.working_capital[id]) + g.business_assets[id])
            + g.tmp_subsid_stock_value)
            + g.goodwill[id])
            + g.options_net_value)
            + g.futures_net_value)
            + g.commod_margin_bal[id])
            + g.phys_commod_value;
        if 1580 < id && id < 1584 {
            g.total_assets = (g.tmp_gov_bonds_value + g.total_assets) + g.tmp_corp_bonds_value;
        }
        if 2 < g.difficulty_level {
            g.capital_taxes_total = calc_capital_taxes(g, id);
        }
        g.tmp_total_debt = (((g.tmp_bank_loan + cast::<Ext, _>(g.ci_bonds_outstanding)) + g.tmp_accrued_inc_tax)
            + g.capital_taxes_total)
            + g.tmp_player_advances;
        f.total_liabs = g.contingency_reserve[id] + g.tmp_total_debt;
    }
}
