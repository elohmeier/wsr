//! Fund housekeeping: the bond funds' portfolio management (`BondEtfManagePortfolio`), the index funds'
//! maintenance (`IndexEtfMaintenance`), the sector funds' performance-bonus scheme
//! (`EtfUpdateBonusEligibility`, `EtfPayPerformanceBonus`, `PromptReduceEtfFee`), the adviser reminder
//! (`RemindEtfManagerDuties`) and `EtfSellIneligibleHolding`.
//!
//! Funds: 1581 government-bond fund, 1582 investment-grade and 1583 high-yield corporate-bond funds,
//! 1584/1585 index funds, 1586..1600 sector/country funds (all industry 71). SINGLE pool constants are
//! written `sf(..)`; the error paths of the `ON ERROR` handlers (ShowRuntimeError only) are not
//! reproduced.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else
)]
use crate::pb::real::{Ext, cast};


use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings};
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::{find_bond_holding_slot, find_stock_holding, pack_holding_arrays};
use crate::logic::newgame_helpers::pick_etf_holding_stock;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::divest_stock;
use crate::logic::sim_entity::corp_ops::trade_gov_bonds;
use crate::logic::sim_entity::fin_inst::{buy_corp_bonds, sell_corp_bonds};
use crate::logic::sim_entity::prelude::*;
use crate::logic::sim_entity::transactions::sell_stock_holding;
use crate::logic::valuation::{calc_stock_price, price_pct_change_months};

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn field(rec: &PbStr, n: i32) -> Ext {
    val(parse_any(rec.as_bytes(), b"|", n).as_bytes())
}

/// `INSTR(1, BondIssueInfo(id), "cv")`: the issuer [runtime resource wording] .
fn is_convertible(g: &Globals, id: i32) -> bool {
    instr(1, g.bond_issue_info[id].as_bytes(), b"cv") > 0
}

/// `VAL(MID$(s, 1, 4))`: the maturity year of a bond description.
fn year_of(s: &PbStr) -> Ext {
    val(mid(s.as_bytes(), 1, 4).as_bytes())
}

/// The locals of `BondEtfManagePortfolio` its GOSUBs use (zeroed once at entry; `smallest_value`
/// survives from fund 1582 to 1583).
#[derive(Default)]
struct BondFrame {
    etf_id: i32,
    slot: i32,
    bond_issuer_id: i32,
    bond_corp_id: i32,
    face_amt: i64,
    bond_value: Ext,
    max_bond_value: i64,
    smallest_value: Ext,
    convert_count: i32,
}

/// Port of BondEtfManagePortfolio (0x655197) with its GOSUBs and exit block BondEtf_Exit (0x656f89).
///
/// For the bond funds 1581..1583 (industry 71), after `CalcEntityFinancials`:
/// * 1581 (government bonds): trims long T-bonds when rates are low (< 4%) and they exceed half the
///   assets, sells short T-notes when rates are high (> 7%), near maturity/game end, above 1.2x cost or
///   with bank debt; repays bank debt (to the global `LenderId`, see BUGS.md #185); invests surplus cash
///   (above 5% of the gov-bond value, less 3% and any `ExtraordinaryPL`) in long bonds (rates > 4.5%) or
///   short notes.
/// * 1582/1583 (corporate bonds): sells one holding picked by `BondEtf_PickBondToSell`, repays debt
///   (AI-advised funds), then buys one issue from up to 500 random draws (convertibles first; 1582 only
///   ratings >= 7, 1583 ratings <= 6 or 7 late, 1/2 only with improving earnings at a price < 55), or
///   sells the smallest holding of a full portfolio.
///
/// `time_secs`: seconds of `TIME$` (offset of the random issuer draw).
pub fn bond_etf_manage_portfolio(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let mut f = BondFrame::default();
    let mut short_horizon = 0;
    let mut bond_qty: i64;
    let mut investable: i64;
    f.etf_id = 1581;
    loop {
        let etf_id = f.etf_id;
        'etf: {
            if g.industry_num[etf_id] != 71 {
                break 'etf;
            }
            calc_entity_financials(g, ui, rng, etf_id);
            if etf_id == 1581 {
                if year_of(&g.short_bond_desc) - (cast::<Ext, _>(g.cur_year)) < Ext::from_f64(3.0) {
                    short_horizon = 1;
                }
                if g.game_end_year - 1 == g.cur_year {
                    short_horizon = 1;
                }
                let long_bond_value = cast::<Ext, _>(g.long_bond_price) * cast::<Ext, _>(g.long_gov_bond_face[etf_id]) * sf(0.01f32);
                let short_bond_value = cast::<Ext, _>(g.short_bond_price) * cast::<Ext, _>(g.short_gov_bond_face[etf_id]) * sf(0.01f32);
                let bir = cast::<Ext, _>(g.base_interest_rate);
                if Ext::from_f64(4.0) > bir && long_bond_value > g.total_assets * sf(0.5f32) && short_horizon == 0 {
                    bond_qty = to_quad(sf(0.1f32) * cast::<Ext, _>(g.long_gov_bond_face[etf_id]));
                    if (cast::<Ext, _>(bond_qty)) < Ext::from_f64(1.0) {
                        bond_qty = 1;
                    }
                    if cast::<Ext, _>(g.long_gov_bond_face[etf_id]) > Ext::from_f64(1.0) {
                        trade_gov_bonds(g, etf_id, bond_qty, 1, 2);
                    }
                }
                let sell_short = Ext::from_f64(7.0) < bir
                    || short_horizon == 1
                    || short_bond_value > g.st_gov_bond_cost[etf_id] * sf(1.2f32)
                    || g.bank_debt[etf_id] > Ext::from_f64(0.0);
                if sell_short && short_bond_value > g.total_assets * sf(0.15f32) {
                    let face = cast::<Ext, _>(g.short_gov_bond_face[etf_id]);
                    bond_qty = to_quad(sf(0.1f32) * face);
                    if Ext::from_f64(10.0) < bir {
                        bond_qty = to_quad(sf(0.15f32) * face);
                    }
                    if Ext::from_f64(12.0) < bir {
                        bond_qty = to_quad(sf(0.2f32) * face);
                    }
                    if short_horizon == 1 && Ext::from_f64(4.5) < bir && (cast::<Ext, _>(g.short_bond_yield)) < cast::<Ext, _>(g.long_bond_yield) {
                        bond_qty = to_quad(sf(0.5f32) * face);
                    }
                    if (cast::<Ext, _>(bond_qty)) < Ext::from_f64(1.0) {
                        bond_qty = 1;
                    }
                    if face > Ext::from_f64(1.0) {
                        trade_gov_bonds(g, etf_id, bond_qty, 2, 2);
                    }
                }
                if g.bank_debt[etf_id] > Ext::from_f64(0.0) {
                    let lender = g.lender_id;
                    if g.bank_debt[etf_id] >= g.t_bills[etf_id] + g.cash[etf_id] {
                        g.liquid_funds = g.t_bills[etf_id] + g.cash[etf_id];
                        g.bank_debt[etf_id] = g.bank_debt[etf_id] - g.liquid_funds;
                        g.cash[etf_id] = g.cash[etf_id] - g.liquid_funds;
                        g.cash[lender] = g.cash[lender] + g.liquid_funds;
                    } else {
                        g.cash[etf_id] = g.cash[etf_id] - g.bank_debt[etf_id];
                        g.cash[lender] = g.cash[lender] + g.bank_debt[etf_id];
                        g.bank_debt[etf_id] = Ext::from_f64(0.0);
                    }
                    balance_cash_t_bills(g, etf_id);
                }
                if g.tmp_gov_bonds_value * sf(0.05f32) < g.t_bills[etf_id] + g.cash[etf_id] {
                    investable = to_quad((g.t_bills[etf_id] + g.cash[etf_id]) - g.tmp_gov_bonds_value * sf(0.03f32));
                    if cast::<Ext, _>(g.extraordinary_pl[etf_id]) > Ext::from_f64(0.0) {
                        investable = to_quad(cast::<Ext, _>(investable) - cast::<Ext, _>(g.extraordinary_pl[etf_id]));
                    }
                    if Ext::from_f64(4.5) < bir && long_bond_value < g.total_assets * sf(0.85f32) && cast::<Ext, _>(investable) > Ext::from_f64(0.0) {
                        // long bonds
                        if !(short_horizon == 1 && !(g.short_bond_yield >= g.long_bond_yield)) {
                            if !(Ext::from_f64(8.5) > bir) {
                                investable = to_quad(sf(0.3f32) * cast::<Ext, _>(investable));
                            } else {
                                investable = to_quad(sf(0.2f32) * cast::<Ext, _>(investable));
                            }
                        }
                        if g.long_bond_price == Ext::from_f32(0.0f32) {
                            g.long_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
                        }
                        bond_qty = to_quad(cast::<Ext, _>(investable) / (sf(0.01f32) * cast::<Ext, _>(g.long_bond_price)));
                        if !((cast::<Ext, _>(bond_qty)) < Ext::from_f64(2.0)) {
                            trade_gov_bonds(g, etf_id, bond_qty, 1, 1);
                        }
                    } else if Ext::from_f64(4.5) < bir
                        && short_bond_value < g.total_assets * sf(0.25f32)
                        && cast::<Ext, _>(investable) > Ext::from_f64(0.0)
                        && short_horizon != 1
                    {
                        investable = to_quad(sf(0.1f32) * cast::<Ext, _>(investable));
                        if g.short_bond_price == Ext::from_f32(0.0f32) {
                            g.short_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
                        }
                        bond_qty = to_quad(cast::<Ext, _>(investable) / (sf(0.01f32) * cast::<Ext, _>(g.short_bond_price)));
                        if !((cast::<Ext, _>(bond_qty)) < Ext::from_f64(50.0)) {
                            trade_gov_bonds(g, etf_id, bond_qty, 2, 1);
                        }
                    }
                }
            } else if etf_id == 1582 || etf_id == 1583 {
                bond_corp_etf(g, ui, rng, &mut f, time_secs);
            }
        }
        f.etf_id += 1;
        if f.etf_id > 1583 {
            // BondEtf_Exit (0x656f89): leaves the routine
            return;
        }
    }
}

/// The corporate-bond part of [`bond_etf_manage_portfolio`] for fund `f.etf_id` (1582/1583).
fn bond_corp_etf(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BondFrame, time_secs: i32) {
    let etf_id = f.etf_id;
    let mut buy_budget = g.total_assets * sf(0.1f32);
    if g.tmp_corp_bonds_value < g.total_assets * sf(0.75f32) && sf(7.5f32) < cast::<Ext, _>(g.base_interest_rate) {
        buy_budget = sf(1.2f32) * buy_budget;
    }
    let mut min_buy = g.total_assets * sf(0.04f32);
    f.bond_corp_id = 0;
    f.face_amt = 0;
    f.max_bond_value = to_quad(g.total_assets * sf(0.25f32));
    pick_bond_to_sell(g, ui, rng, f);
    if f.bond_corp_id > 0 && cast::<Ext, _>(f.face_amt) > Ext::from_f64(0.0) {
        sell_selected_bond(g, ui, rng, f);
    }
    calc_entity_financials(g, ui, rng, etf_id);
    let total_assets = g.total_assets;
    g.liquid_funds = g.t_bills[etf_id] + g.cash[etf_id];
    let mut short_only = 0;
    if g.liquid_funds > g.total_assets * sf(0.15f32)
        && rng.rnd() * sf(0.5f32) + Ext::from_f64(5.0) > cast::<Ext, _>(g.base_interest_rate)
        && g.tmp_total_debt == Ext::from_f64(0.0)
    {
        short_only = 1;
    }
    if cast::<Ext, _>(g.extraordinary_pl[etf_id]) > Ext::from_f64(0.0) {
        g.liquid_funds = g.liquid_funds - cast::<Ext, _>(g.extraordinary_pl[etf_id]);
    }
    if g.hold_bond_id[(0, etf_id)] > 12 && g.liquid_funds > buy_budget {
        min_buy = sf(1.75f32) * min_buy;
    } else if g.hold_bond_id[(0, etf_id)] < 8
        && g.liquid_funds > g.total_assets * sf(0.15f32)
        && sf(9.5f32) < cast::<Ext, _>(g.base_interest_rate)
    {
        min_buy = sf(0.5f32) * min_buy;
    }
    if g.bank_debt[etf_id] > Ext::from_f64(100.0) && get_etf_adviser_human_status(g, ui, rng, etf_id) == 0 {
        let mut repay_amt = g.liquid_funds;
        if repay_amt > g.bank_debt[etf_id] {
            repay_amt = g.bank_debt[etf_id];
        }
        g.liquid_funds = g.liquid_funds - repay_amt;
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] + repay_amt;
        g.bank_debt[etf_id] = g.bank_debt[etf_id] - repay_amt;
        g.cash[etf_id] = g.cash[etf_id] - repay_amt;
        balance_cash_t_bills(g, etf_id);
        if g.liquid_funds < g.exchange_rate * Ext::from_f64(100.0) {
            return;
        }
    }
    if g.tmp_corp_bonds_value > g.total_assets * sf(0.5f32) && Ext::from_f64(5.0) > cast::<Ext, _>(g.base_interest_rate) {
        return;
    }
    if g.liquid_funds > min_buy && g.hold_bond_id[(0, etf_id)] < 15 && g.credit_rating_num[etf_id] > 7 {
        if buy_budget > g.liquid_funds {
            buy_budget = g.liquid_funds;
        }
        let mut search_limit = to_long((Ext::from_f64(7.0) - cast::<Ext, _>(g.base_interest_rate)) * Ext::from_f64(10.0));
        if search_limit < 6 {
            search_limit = 6;
        }
        count_convertibles(g, f);
        if f.convert_count < 2 {
            search_limit = 100;
        }
        if etf_id == 1582 {
            search_limit = to_long(sf(0.5f32) * cast::<Ext, _>(search_limit));
        }
        let secs = time_secs;
        for attempt in 1..=500 {
            f.bond_corp_id = rng.rnd_range(1541, 11) + secs;
            let bc = f.bond_corp_id;
            if g.entity_status[bc] != 1 || g.bonds_issued[bc] == 0 || etf_id == bc {
                continue;
            }
            if !is_convertible(g, bc) && search_limit >= attempt {
                continue;
            }
            if Ext::from_f64(9.0) < cast::<Ext, _>(g.base_interest_rate) && year_of(&g.bond_issue_info[bc]) < Ext::from_f64(10.0) + cast::<Ext, _>(g.cur_year) {
                continue;
            }
            let crn = g.credit_rating_num[bc];
            if etf_id == 1582 {
                if crn < 7 {
                    continue;
                }
            } else if etf_id == 1583 {
                if crn > 7 {
                    continue;
                }
                if crn == 7 && attempt < 450 {
                    continue;
                }
                if crn < 1 {
                    continue;
                }
                if crn == 1 || crn == 2 {
                    let f3 = field(&g.ent_misc_data[bc], 3);
                    if !(f3 > Ext::from_f64(0.0)) {
                        continue;
                    }
                    if !(g.proj_qtr_earnings[bc] > field(&g.ent_misc_data[bc], 3)) {
                        continue;
                    }
                    if !(g.bond_price[bc].get() < Ext::from_f64(55.0)) {
                        continue;
                    }
                }
            }
            let slot = find_bond_holding_slot(g, etf_id, bc);
            if slot == 0 {
                continue;
            }
            if g.hold_bond_id[(slot, etf_id)] == bc
                && g.hold_bond_id[(0, etf_id)] < 12
                && g.bond_price[bc].get() * cast::<Ext, _>(g.hold_bond_face[(slot, etf_id)]) * sf(0.01f32) > total_assets * sf(0.13f32)
            {
                continue;
            }
            if short_only == 1 {
                let maturity = to_long(year_of(&g.bond_issue_info[bc]));
                if 3 + g.cur_year < maturity {
                    continue;
                }
            }
            calc_entity_financials(g, ui, rng, bc);
            if (cast::<Ext, _>(g.bonds_public_held)) < min_buy || g.bond_price[bc].get() > Ext::from_f64(110.0) {
                continue;
            }
            let mut bond_qty = to_quad(buy_budget / (sf(0.01f32) * g.bond_price[bc].get()));
            if cast::<Ext, _>(bond_qty) > cast::<Ext, _>(g.bonds_public_held) {
                bond_qty = to_quad(cast::<Ext, _>(g.bonds_public_held) * sf(0.9f32));
            }
            g.tmp_ext1 = cast::<Ext, _>(bond_qty);
            g.tmp_ext2 = Ext::from_f64(2.0);
            buy_corp_bonds(g, ui, rng, etf_id, bc);
            break;
        }
        return;
    }
    if g.hold_bond_id[(0, etf_id)] > 10 && g.sim_co_count > 1000 {
        if !(g.bank_debt[etf_id] > Ext::from_f64(0.0)) && g.hold_bond_id[(0, etf_id)] != 15 {
            return;
        }
        find_smallest_bond(g, ui, rng, f);
        if f.bond_corp_id > 10 {
            sell_selected_bond(g, ui, rng, f);
        }
    }
}

/// Port of BondEtf_PickBondToSell (0x6567bc), GOSUB of [`bond_etf_manage_portfolio`].
/// Picks the first holding (`bond_corp_id`, else 0) to sell: one worth more than `max_bond_value`
/// (then only 30% of it), a downgraded or failing issue (by fund), one priced above `150 + 100 x RND`, or
/// (`RandSeed MOD 3 > 1`) a non-convertible above 112; `face_amt` is the face to sell.
fn pick_bond_to_sell(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BondFrame) {
    let etf_id = f.etf_id;
    f.bond_corp_id = 0;
    f.slot = 1;
    loop {
        f.bond_issuer_id = g.hold_bond_id[(f.slot, etf_id)];
        let id = f.bond_issuer_id;
        if id != 0 {
            f.face_amt = g.hold_bond_face[(f.slot, etf_id)];
            f.bond_value = sf(0.01f32) * g.bond_price[id].get() * cast::<Ext, _>(f.face_amt);
            if f.bond_value > cast::<Ext, _>(f.max_bond_value) {
                f.face_amt = to_quad(sf(0.3f32) * cast::<Ext, _>(f.face_amt));
                f.bond_corp_id = id;
                return;
            }
            calc_entity_financials(g, ui, rng, id);
            let crn = g.credit_rating_num[id];
            if etf_id == 1582 {
                if crn < 6 {
                    f.bond_corp_id = id;
                    return;
                }
            } else if etf_id == 1583 {
                if crn < 1 {
                    f.bond_corp_id = id;
                    return;
                }
                if crn == 1 {
                    let improving = field(&g.ent_misc_data[id], 3) > Ext::from_f64(0.0) && g.proj_qtr_earnings[id] > Ext::from_f64(0.0);
                    if !improving {
                        if id >= 86 || g.bond_price[id].get() >= Ext::from_f64(25.0) {
                            f.bond_corp_id = id;
                            return;
                        }
                        if g.bond_unrealized_loss > (g.tmp_gov_bonds_value + g.tmp_corp_bonds_value) * sf(0.15f32) {
                            f.bond_corp_id = id;
                            return;
                        }
                    }
                } else if crn > 6 {
                    // convertibles rated 7 go on to the price checks
                    if !(is_convertible(g, id) && crn < 8) && (g.rand_seed % 5 == 1 || crn > 7) {
                        f.bond_corp_id = id;
                        return;
                    }
                }
            }
            let limit = rng.rnd() * Ext::from_f64(100.0) + Ext::from_f64(150.0);
            if g.bond_price[id].get() > limit {
                f.bond_corp_id = id;
                return;
            }
            if g.rand_seed % 3 > 1 && g.bond_price[id].get() > Ext::from_f64(112.0) && !is_convertible(g, id) {
                f.bond_corp_id = id;
                return;
            }
        }
        f.slot += 1;
        if f.slot > 15 {
            return;
        }
    }
}

/// Port of BondEtf_FindSmallestBond (0x656c8a), GOSUB of [`bond_etf_manage_portfolio`].
/// Picks the holding with the smallest positive value (`bond_corp_id`, its whole face in `face_amt`),
/// compared against `smallest_value`, which starts at the first holding's value — so the first holding
/// itself is never picked — and is not reset between the two funds (BUGS.md #186).
fn find_smallest_bond(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BondFrame) {
    let etf_id = f.etf_id;
    f.bond_corp_id = 0;
    f.slot = 1;
    while f.slot <= 15 {
        f.bond_issuer_id = g.hold_bond_id[(f.slot, etf_id)];
        let id = f.bond_issuer_id;
        if id != 0 {
            calc_entity_financials(g, ui, rng, id);
            f.bond_value = sf(0.01f32) * g.bond_price[id].get() * cast::<Ext, _>(g.hold_bond_face[(f.slot, etf_id)]);
            if f.smallest_value == Ext::from_f64(0.0) {
                f.smallest_value = f.bond_value;
            }
            if f.bond_value > Ext::from_f64(0.0) && f.smallest_value > Ext::from_f64(0.0) && f.bond_value < f.smallest_value {
                f.bond_corp_id = id;
                f.smallest_value = f.bond_value;
                f.face_amt = g.hold_bond_face[(f.slot, etf_id)];
            }
        }
        f.slot += 1;
    }
}

/// Port of BondEtf_SellSelectedBond (0x656de8), GOSUB of [`bond_etf_manage_portfolio`]: sells
/// `face_amt` of `bond_corp_id` (> 10) with `SellCorpBonds` (mode 2).
fn sell_selected_bond(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BondFrame) {
    if f.bond_corp_id > 10 {
        g.tmp_ext1 = cast::<Ext, _>(f.face_amt);
        g.tmp_ext2 = Ext::from_f64(2.0);
        sell_corp_bonds(g, ui, rng, f.etf_id, f.bond_corp_id);
    }
}

/// Port of BondEtf_CountConvertibles (0x656e1f), GOSUB of [`bond_etf_manage_portfolio`]: number of
/// convertible issues among the fund's bond holdings 1..`HoldBondId(0)`.
fn count_convertibles(g: &Globals, f: &mut BondFrame) {
    let etf_id = f.etf_id;
    f.convert_count = 0;
    let n = g.hold_bond_id[(0, etf_id)];
    f.slot = 1;
    while f.slot <= n {
        if g.hold_bond_id[(f.slot, etf_id)] != 0 {
            f.bond_issuer_id = g.hold_bond_id[(f.slot, etf_id)];
            if is_convertible(g, f.bond_issuer_id) {
                f.convert_count += 1;
            }
        }
        f.slot += 1;
    }
}

/// Port of EtfPayPerformanceBonus (0x6520bf).
/// In Q4, for every sector fund 1586..1600 at the end of its [runtime resource wording] (alternating
/// years by odd/even id) with the minimum fee (<= 0.21%) and `FundBonusYear` set: shows its 2-year NAV
/// change (human adviser); if over 20% (and not falling over 12 and 6 months unless >= 30%), pays the
/// adviser (> 85) 20% of the gain above 20% (gain capped at 120%) of the market cap 24 months ago, with
/// news (pop-up for player-controlled advisers) and a repricing of the adviser for large fees.
pub fn etf_pay_performance_bonus(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    if g.cur_quarter != 4 {
        return;
    }
    for etf_id in 1586..=1600 {
        if g.entity_status[etf_id] != 1 || g.industry_num[etf_id] != 71 {
            continue;
        }
        if g.ctrl_since_year[etf_id] == 0 {
            g.ctrl_since_year[etf_id] = g.cur_year - 3;
        }
        if sf(0.21f32) < cast::<Ext, _>(g.fund_fee_pct[etf_id]) || g.fund_bonus_year[etf_id] == 0 {
            continue;
        }
        if g.cur_year % 2 == 1 && etf_id % 2 == 0 {
            continue;
        }
        if g.cur_year % 2 == 0 && etf_id % 2 == 1 {
            continue;
        }
        let pct_change = price_pct_change_months(g, etf_id, 24);
        if get_etf_adviser_human_status(g, ui, rng, etf_id) > 0 {
            let mut m = g.ent_name[etf_id].clone()
                + __content.string(4858)
                + &str_s(cast::<Ext, _>(pct_change))
                + "%";
            show_quick_message(g, ui, &mut m);
        }
        if pct_change < 21 {
            continue;
        }
        if price_pct_change_months(g, etf_id, 12) < -10
            && price_pct_change_months(g, etf_id, 6) < 0
            && pct_change < 30
        {
            continue;
        }
        let cur_cap = to_quad(g.market_cap[etf_id]);
        let mut hist_idx = to_long(g.cap_hist_close[(0, etf_id)] - Ext::from_f64(24.0));
        if hist_idx < 1 {
            hist_idx += 60;
        }
        let start_cap = to_quad(g.cap_hist_close[(hist_idx, etf_id)]);
        let mut excess_gain = (cast::<Ext, _>(cur_cap) - cast::<Ext, _>(start_cap)) * Ext::from_f64(100.0);
        if excess_gain > cast::<Ext, _>(start_cap) * Ext::from_f64(120.0) {
            excess_gain = cast::<Ext, _>(start_cap) * Ext::from_f64(120.0);
        }
        excess_gain = excess_gain - cast::<Ext, _>(start_cap) * Ext::from_f64(20.0);
        let bonus_fee = excess_gain * sf(0.2f32);
        if !(bonus_fee >= Ext::from_f64(0.5)) {
            continue;
        }
        let adviser_id = g.fund_adviser[etf_id];
        if adviser_id <= 85 {
            continue;
        }
        g.cash[etf_id] = g.cash[etf_id] - bonus_fee;
        g.accrued_pl[etf_id] = g.accrued_pl[etf_id] - bonus_fee;
        g.cash[adviser_id] = g.cash[adviser_id] + bonus_fee;
        g.accrued_pl[adviser_id] = g.accrued_pl[adviser_id] + bonus_fee;
        if bonus_fee > Ext::from_f64(1.0) {
            g.news_msg = g.ent_symbol[adviser_id].clone()
                + __content.string(4859)
                + &g.ent_symbol[etf_id]
                + " of "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(bonus_fee, b"* ###,")
                + " "
                + &g.million_prefix_uc
                + "il."
                + &g.currency_suffix;
            if g.corp_controller[adviser_id] < 11 {
                let mut m = std::mem::take(&mut g.news_msg);
                show_quick_message(g, ui, &mut m);
                g.news_msg = m;
            }
            post_news(g, ui, 0, 0);
            if bonus_fee > g.exchange_rate * Ext::from_f64(100.0) {
                calc_projected_earnings(g, ui, rng, adviser_id);
                g.valuation_mode = 2;
                calc_stock_price(g, ui, rng, adviser_id, time_secs);
            }
        }
    }
}

/// Port of IndexEtfMaintenance (0x652f2e).
/// For the active index fund `SimCoCount` (1581..1585, industry 71): fee 0.2%; every other index fund
/// (1581..1585) divests its first holding; the first corp-controlled (> 10) holder of a 1584/1585 fund
/// divests it (reason 3). The swap-termination loop reads `SwapContracts(etfId)` instead of
/// `SwapContracts(idx)` (BUGS.md #2), an element outside `DIM (0 TO 1000)`: it never matches.
pub fn index_etf_maintenance(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let etf_id = g.sim_co_count;
    if !(g.entity_status[etf_id] == 1 && g.industry_num[etf_id] == 71 && 1580 < etf_id && etf_id < 1586) {
        return;
    }
    g.fund_fee_pct[etf_id] = cast::<f32, _>(0.2f32);
    for idx in 11..=1600 {
        if g.entity_status[idx] != 1 || etf_id == idx {
            continue;
        }
        if 1580 < idx && idx < 1586 && g.hold_stock_id[(0, idx)] != 0 && g.industry_num[idx] == 71 {
            let held_id = cast::<i32, _>(g.hold_stock_id[(1, idx)]);
            if 10 < held_id {
                divest_stock(g, ui, rng, held_id, idx, 3);
            }
        }
        if find_stock_holding(g, idx, etf_id) > 0 && g.corp_controller[idx] > 10 && etf_id > 1583 {
            divest_stock(g, ui, rng, etf_id, idx, 3);
            break;
        }
    }
    let count = to_long(val(g.swap_contracts[0].as_bytes()));
    let mut idx = 1;
    while idx <= count {
        if !g.swap_contracts[idx].is_empty() {
            // BUG (BUGS.md #2): reads the record of etfId, not idx
            let swap_rec = g.swap_contracts[etf_id].clone();
            let long_party = to_long(field(&swap_rec, 1));
            let short_party = to_long(field(&swap_rec, 2));
            if etf_id == long_party || etf_id == short_party {
                let other = if etf_id == long_party { short_party } else { long_party };
                if g.corp_controller[other] >= 11 {
                    g.news_msg = g.ent_symbol[other].clone()
                        + " and "
                        + &g.ent_symbol[etf_id]
                        + __content.string(4860);
                    post_news(g, ui, long_party, short_party);
                    g.swap_contracts[idx] = PbStr::new();
                    let _ = pack_holding_arrays(g);
                    idx -= 1;
                }
            }
        }
        idx += 1;
    }
}

/// Port of PromptReduceEtfFee (0x652b9a).
/// In Q4, for the fund `SimCoCount + 100` advised by the current player: before a new 2-year bonus
/// period (`(CurYear + id + 1) MOD 2 = 1`) with a fee above 0.21%, asks whether to cut the fee to 0.2%;
/// YES counts as a turn action.
pub fn prompt_reduce_etf_fee(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.cur_quarter != 4 {
        return;
    }
    let etf_id = g.sim_co_count + 100;
    if !(g.entity_status[etf_id] == 1 && g.industry_num[etf_id] == 71) {
        return;
    }
    if get_etf_adviser_human_status(g, ui, rng, etf_id) != 2 {
        return;
    }
    let adviser_id = g.fund_adviser[etf_id];
    // PricePctChangeMonths(etfId, 11): result unused (pure function)
    if (g.cur_year + etf_id + 1) % 2 == 1 && sf(0.21f32) < cast::<Ext, _>(g.fund_fee_pct[etf_id]) {
        let msg_text = PbStr::from(__content.string(4861))
            + "year ("
            + &str_s(Ext::from_f64(1.0) + cast::<Ext, _>(g.cur_year))
            + "), for "
            + &g.ent_name[etf_id]
            + __content.string(4862)
            + &str_s(Ext::from_f64(1.0) + cast::<Ext, _>(g.cur_year))
            + "-"
            + &trim(str_s(Ext::from_f64(2.0) + cast::<Ext, _>(g.cur_year)).as_bytes())
            + "."
            + "\r"
            + "\r"
            + __content.string(4863)
            + &g.ent_name[adviser_id]
            + __content.string(4864)
            + __content.string(4865)
            + __content.string(4866)
            + &g.ent_name[etf_id]
            + __content.string(4867)
            + __content.string(4868)
            + &fmt_trim(cast::<Ext, _>(g.fund_fee_pct[etf_id]), b"0.0")
            + "%."
            + "\r"
            + "\r"
            + __content.string(4869);
        let answer = ui.msgbox(&msg_text, 8227, &PbStr::from(__content.string(4870)));
        if answer == 6 {
            g.turn_action_count += 1;
            g.fund_fee_pct[etf_id] = cast::<f32, _>(0.2f32);
        }
    }
}

/// Port of RemindEtfManagerDuties (0x6527da).
/// Shows one reminder for the first sector fund 1586..1600 whose bonus year is at least 2 years old
/// (and not before the game start), advised by the current player's company that is not on AutoPilot.
/// The text claims a year without ETF transactions, which is never checked (BUGS.md #187).
pub fn remind_etf_manager_duties(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    for etf_id in 1586..=1600 {
        if !(g.entity_status[etf_id] == 1
            && g.industry_num[etf_id] == 71
            && g.fund_bonus_year[etf_id] <= g.cur_year - 2
            && g.fund_bonus_year[etf_id] != 0
            && g.start_year <= g.fund_bonus_year[etf_id])
        {
            continue;
        }
        if get_etf_adviser_human_status(g, ui, rng, etf_id) != 2 {
            continue;
        }
        let adviser_id = g.fund_adviser[etf_id];
        if g.auto_pilot[adviser_id] != 1 {
            let msg_text = PbStr::from(__content.string(4871))
                + "\r"
                + "\r"
                + __content.string(4872)
                + &g.ent_name[adviser_id]
                + " ("
                + &g.ent_symbol[adviser_id]
                + __content.string(4873)
                + __content.string(4874)
                + &g.ent_name[etf_id]
                + __content.string(4875)
                + &g.ent_symbol[adviser_id]
                + " is "
                + __content.string(4876)
                + "\r"
                + "\r"
                + __content.string(4877)
                + __content.string(4878)
                + __content.string(4879)
                + &g.ent_symbol[adviser_id]
                + __content.string(4880)
                + __content.string(4881);
            ui.msgbox(&msg_text, 8256, &PbStr::from(__content.string(4882)));
            break;
        }
    }
}

/// Port of EtfUpdateBonusEligibility (0x651df1).
/// In Q4, for the sector funds 1586..1600: inactive/non-fund slots get `FundBonusYear = 0`; a fund
/// without a bonus year and a fee <= 0.21% starts its period this year (in alternating years by
/// odd/even id); a fund with a bonus year whose fee rose above 0.21% loses it (news for a human-player
/// adviser).
pub fn etf_update_bonus_eligibility(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    if g.cur_quarter != 4 {
        return;
    }
    for etf_id in 1586..=1600 {
        if !(g.entity_status[etf_id] == 1 && g.industry_num[etf_id] == 71) {
            g.fund_bonus_year[etf_id] = 0;
            continue;
        }
        if g.fund_bonus_year[etf_id] < 1 {
            if !(sf(0.21f32) < cast::<Ext, _>(g.fund_fee_pct[etf_id])) {
                if (g.cur_year + 1) % 2 == 1 && etf_id % 2 == 0 {
                    g.fund_bonus_year[etf_id] = g.cur_year;
                } else if (g.cur_year + 1) % 2 == 0 && etf_id % 2 == 1 {
                    g.fund_bonus_year[etf_id] = g.cur_year;
                }
            }
        } else if sf(0.21f32) < cast::<Ext, _>(g.fund_fee_pct[etf_id]) {
            let adviser_id = g.fund_adviser[etf_id];
            g.fund_bonus_year[etf_id] = 0;
            if 85 < adviser_id {
                g.news_msg = g.ent_symbol[adviser_id].clone()
                    + __content.string(4883)
                    + &g.ent_symbol[etf_id]
                    + __content.string(4884);
                if get_controller_type(g, adviser_id) == 1 {
                    post_news(g, ui, adviser_id, etf_id);
                }
            }
        }
    }
}

/// Port of EtfSellIneligibleHolding (0x64f724).
/// For an active sector fund 1586..1600: sells (mode 2) the first holding that no longer fits the
/// fund's sectors (`PickEtfHoldingStock` returns 0); industry-70 holdings are kept when the adviser is
/// human-controlled.
pub fn etf_sell_ineligible_holding(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, etf_id: i32) {
    let fund_id = etf_id;
    if !(1585 < fund_id && fund_id < 1601 && g.entity_status[fund_id] == 1 && g.industry_num[fund_id] == 71) {
        return;
    }
    for slot in 1..=15 {
        let stock_id = cast::<i32, _>(g.hold_stock_id[(slot, fund_id)]);
        if stock_id == 0 || !(g.hold_stock_pct[(slot, fund_id)] > 0) {
            continue;
        }
        if 10 < stock_id && g.industry_num[stock_id] == 70 && get_etf_adviser_human_status(g, ui, rng, fund_id) > 0 {
            continue;
        }
        if pick_etf_holding_stock(g, ui, rng, fund_id, stock_id) == 0 {
            let sell_id = stock_id;
            g.tmp_ext1 = cast::<Ext, _>(fund_id);
            g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(slot, fund_id)]);
            g.tmp_ext3 = Ext::from_f64(2.0);
            sell_stock_holding(g, ui, rng, sell_id);
            return;
        }
    }
}
