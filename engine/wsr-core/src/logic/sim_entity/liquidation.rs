//! Liquidations: `LiquidateCorporation` (0x51b258, taxable liquidation — interactive or automatic),
//! `BankruptcyLiquidation` (0x4e7250, bankruptcy reorganization / liquidation / bank and insurer
//! failures) with their GOSUB blocks, and the small callees they need (`ClearAllHoldings`,
//! `LiquidateAllPhysCommod`, `ForceCoverShorts`).
use crate::pb::real::{Ext, cast};


pub use super::mgmt::force_cover_shorts;
use super::prelude::*;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::get_funds_advised_by;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_phys_commod_value};
use crate::logic::futures::{count_futures_positions, recalc_commod_margin_balances};
use crate::logic::holdings::find_holding_slot;
use crate::logic::market::rebuild_streaming_quotes_panel;
use crate::logic::newgame_helpers::clear_entity_record;
use crate::logic::sim_entity::transactions::etf_drop_one_adviser_contract;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::close_option_positions;
use crate::logic::report::show_quick_message;
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::dividends::calc_consolidated_tax_payment;
use crate::logic::sim_entity::quarter_start::{
    cancel_pending_merger, clear_insider_tip, pack_holding_arrays, recapture_excess_loss,
};
use crate::logic::sim_entity::transactions::{liquidate_futures_positions, sell_stock_holding};
use crate::logic::swaps::cancel_entity_swaps;
use crate::logic::valuation::{calc_stock_price, clear_price_history};
use crate::pb::string::{CaseMode, mcase, ucase};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

// ---------------------------------------------------------------------------------------------------------
// Small callees
// ---------------------------------------------------------------------------------------------------------

/// Port of ClearAllHoldings (0x49261c).
/// Clears the stock holdings (slots 0..15) of every entity 1..1600 with a nonzero status — with `mode = 2`
/// only those of inactive (status <> 1) entities — and, for entities 1..130, their bond holdings too.
pub fn clear_all_holdings(g: &mut Globals, mode: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    for entity_id in 1..=1600 {
        if g.entity_status[entity_id] == 0 {
            continue;
        }
        if g.entity_status[entity_id] == 1 && mode == 2 {
            continue;
        }
        for slot in 0..=15 {
            g.hold_stock_id[(slot, entity_id)] = 0;
            g.hold_stock_pct[(slot, entity_id)] = 0;
            g.hold_stock_basis[(slot, entity_id)] = Ext::from_f64(0.0);
            if entity_id < 131 {
                g.hold_bond_id[(slot, entity_id)] = 0;
                g.hold_bond_face[(slot, entity_id)] = 0;
                g.bond_hold_cost[(slot, entity_id)] = Ext::from_f64(0.0);
            }
        }
    }
}

/// Port of LiquidateAllPhysCommod (0x663453).
/// Sells all [runtime resource wording] of `entity_id` (records 1..1000 whose field 2 is the entity):
/// `SellPhysCommod(entity, rec, 2, 100!, 0)` per record, re-checking the record index afterwards.
///
/// Port deviation: the original re-scans the same index after each sale; when the sale leaves the record
/// unchanged (a defunct entity) it would never end, so the port then moves on.
pub fn liquidate_all_phys_commod(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) {
    let ent_id = entity_id;
    let mut rec_idx = 1;
    while rec_idx <= 1000 {
        let rec_text = g.phys_commod_rec[rec_idx].clone();
        if !rec_text.is_empty()
            && val(parse_any(rec_text.as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(ent_id)
        {
            crate::logic::commod_ai::physical::sell_phys_commod(g, ui, rng, ent_id, rec_idx, 2, 100.0f32, 0);
            if g.phys_commod_rec[rec_idx] != rec_text {
                rec_idx -= 1;
            }
        }
        rec_idx += 1;
    }
}

// ---------------------------------------------------------------------------------------------------------
// LiquidateCorporation
// ---------------------------------------------------------------------------------------------------------

/// Locals of LiquidateCorporation shared with its GOSUB blocks.
#[derive(Default)]
struct LiqFrame {
    corp_id: i32,
    auto_mode: i32,
    corp_name: PbStr,
    trust_amt: i64,
    final_tax_paid: Ext,
    final_net_income: Ext,
    final_extra_items: Ext,
    tax_credit_used: Ext,
}

/// Port of LiquidateCorp_RefundTrustSetAside (GOSUB 0x51f101).
/// Gives back a tentative trust set-aside (cash and accrued P&L).
fn liq_refund_trust(g: &mut Globals, f: &LiqFrame) {
    let c = f.corp_id;
    if 0 < f.trust_amt {
        g.accrued_pl[c] = cast::<Ext, _>(f.trust_amt) + g.accrued_pl[c];
        g.cash[c] = cast::<Ext, _>(f.trust_amt) + g.cash[c];
    }
}

/// Port of LiquidateCorp_CheckSolvency (GOSUB 0x51e86d).
/// Recomputes the financials (without the contingency reserve; net worth plus capital taxes); a company
/// rated below 2 with a negative net worth can't liquidate: message (interactive mode), refund of the
/// trust set-aside and exit of the routine (returns `true`).
fn liq_check_solvency(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &LiqFrame) -> bool {
    let __content = crate::content::current();
    let c = f.corp_id;
    let saved_contingency = g.contingency_reserve[c];
    g.contingency_reserve[c] = Ext::from_f64(0.0);
    calc_entity_financials(g, ui, rng, c);
    g.net_worth = g.capital_taxes_total + g.net_worth;
    if g.credit_rating_num[c] < 2 && g.net_worth < Ext::from_f64(0.0) {
        if f.auto_mode == 0 {
            let mut m = s(__content.string(10862))
                + &f.corp_name
                + __content.string(10863)
                + __content.string(10864)
                + __content.string(10865);
            show_quick_message(g, ui, &mut m);
        }
        g.contingency_reserve[c] = saved_contingency;
        liq_refund_trust(g, f);
        return true;
    }
    g.contingency_reserve[c] = saved_contingency;
    false
}

/// Port of LiquidateCorp_FinalConsolTax (GOSUB 0x51e9e8).
/// Final income tax of the liquidating corp: T-bills to cash, `CalcConsolidatedTaxPayment` on the
/// ordinary income (dividends 35% / 50% taxable) plus extraordinary items and consolidated income, the
/// cash paid = `finalTaxPaid`; with a consolidating parent (news, maybe a pop-up) its tax credits are
/// sold to the parent. Computes the final net income and extraordinary items for the equity holders.
fn liq_final_consol_tax(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut LiqFrame) {
    let __content = crate::content::current();
    let c = f.corp_id;
    let ordinary_inc = g.div_recvd1[c] * sf(0.35f32) + g.accrued_pl[c] + g.div_recvd2[c] * sf(0.5f32);
    let total_taxable_inc = cast::<Ext, _>(g.extraordinary_pl[c]) + ordinary_inc + g.consol_tax_inc_ytd[c];
    g.tmp_ext1 = total_taxable_inc;
    g.parent80_owner = 0;
    g.cash[c] = g.t_bills[c] + g.cash[c];
    g.t_bills[c] = Ext::from_f64(0.0);
    let cash_before = g.cash[c];
    calc_consolidated_tax_payment(g, ui, rng, c);
    f.final_tax_paid = cash_before - g.cash[c];
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let p80 = cast::<i32, _>(g.parent80_owner);
    if 10 < p80 {
        g.news_msg = g.ent_symbol[c].clone()
            + __content.string(10866)
            + &g.ent_symbol[p80];
        if g.cur_player == g.corp_controller[p80] {
            let mut m = g.news_msg.clone()
                + ", as "
                + &g.ent_name[c]
                + " and "
                + &g.ent_name[p80]
                + __content.string(10867);
            show_quick_message(g, ui, &mut m);
        }
        post_news(g, ui, 0, 0);
    }
    let mut ordinary_tax = sf(0.01f32) * ordinary_inc * cast::<Ext, _>(g.corp_tax_rate);
    let extra_tax = sf(0.01f32) * cast::<Ext, _>(g.extraordinary_pl[c]) * cast::<Ext, _>(g.corp_tax_rate);
    let _total_tax = extra_tax + ordinary_tax;
    let p80 = cast::<i32, _>(g.parent80_owner);
    if 10 < p80 && Ext::from_f64(0.0) < g.tax_credits[c] {
        g.tax_credits[p80] = g.tax_credits[c] + g.tax_credits[p80];
        g.cash[c] = g.tax_credits[c] + g.cash[c];
        g.cash[p80] = g.cash[p80] - g.tax_credits[c];
        f.tax_credit_used = g.tax_credits[c];
        g.equity_income_ytd[p80] = g.equity_income_ytd[p80] - f.tax_credit_used;
    } else if Ext::from_f64(0.0) < g.tax_credits[c] {
        f.tax_credit_used = g.tax_credits[c];
        if f.tax_credit_used > f.final_tax_paid {
            f.tax_credit_used = f.final_tax_paid;
        }
        if f.tax_credit_used < Ext::from_f64(0.0) {
            f.tax_credit_used = Ext::from_f64(0.0);
        }
    }
    f.final_tax_paid = f.final_tax_paid - f.tax_credit_used;
    ordinary_tax = ordinary_tax - f.tax_credit_used;
    g.tax_credits[c] = Ext::from_f64(0.0);
    f.final_net_income = g.div_recvd2[c] + g.accrued_pl[c] + g.equity_income_ytd[c] - ordinary_tax;
    f.final_extra_items = g.extra_items_ytd[c] + cast::<Ext, _>(g.extraordinary_pl[c]) - extra_tax;
    if f.final_extra_items.abs() < f.final_net_income.abs() * sf(0.05f32) {
        f.final_net_income = f.final_extra_items + f.final_net_income;
        f.final_extra_items = Ext::from_f64(0.0);
    }
}

/// Port of LiquidateCorporation (0x51b258) with its GOSUBs LiquidateCorp_CheckSolvency (0x51e86d),
/// LiquidateCorp_FinalConsolTax (0x51e9e8) and LiquidateCorp_RefundTrustSetAside (0x51f101).
/// Taxable (complete) liquidation of corp `entity_id` (> 130, active).
///
/// * `auto_mode > 0` (AI): only a debt-free shell qualifies — no business assets, subsidiaries' stock,
///   bonds, player advances, options, futures, swaps (`CancelEntitySwaps(.., 2, auto_mode)`) or physical
///   commodities, not player-controlled, not the asbestos/SuperFund defendant, net worth >= -5 (x
///   exchange rate), and as the AI's target only with a small loss carryforward and net worth >= 100 x
///   cap; goodwill is written off and futures closed.
/// * `auto_mode = 0` (player): explains the consequences and asks for confirmation, sets aside a trust
///   for asbestos/SuperFund claims, then walks the player through buying back bonds
///   (`BondBuybackMenu`), selling business assets (`SellBusinessAssets`), closing options, futures, [runtime resource wording]  swaps and selling the stock
///   holdings (each step can be declined, which cancels and refunds the trust).
///
/// Then (both modes; the solvency check can cancel at each step): final tax (industries < 71), bank
/// loan payoff ( [runtime resource wording] difficulty 3 when the bank has another controller), player
/// advances repaid, shorts covered, the remaining cash distributed to the stockholders as a liquidating
/// dividend (gain/loss to capital gains or extraordinary P&L, equity-method shares of the final income),
/// options on the stock settled, a pending merger canceled, and the slot cleared (status 2, industry 70,
/// `ClearEntityRecord`, `ActiveCorpCount - 1`). `EntMiscData` keeps a 100% corporate parent's id.
///
/// Bug (reproduced, BUGS.md): the trust set-aside is taken from cash only (`AccruedPL = AccruedPL`), but
/// a refund adds it to both cash and accrued P&L. The bank-loan payoff is credited to the stale global
/// `LenderId` before it is set to the company's lending bank.
pub fn liquidate_corporation(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    auto_mode: i32,
    now: &crate::logic::newgame::LocalTime,
) {
    let __content = crate::content::current();
    let time_secs = now.time_secs();
    // ON ERROR GOTO -> ShowRuntimeError("Liquidate2") (error path not ported)
    if entity_id < 131 {
        return;
    }
    if g.entity_status[entity_id] != 1 {
        return;
    }
    let mut f = LiqFrame {
        corp_id: entity_id,
        auto_mode,
        ..LiqFrame::default()
    };
    let c = entity_id;
    f.corp_name = g.ent_name[c].clone();
    let industry = g.industry_num[c];
    calc_entity_financials(g, ui, rng, c);
    g.net_worth = g.capital_taxes_total + g.net_worth + g.tmp_accrued_inc_tax;
    g.net_worth = g.contingency_reserve[c] + g.net_worth;
    let mut msg_text: PbStr;
    let mut amt_text: PbStr;
    let mut tax_news_msg = PbStr::new();
    let mut prev_bonds_issued: i64 = 0;
    let mut answer: i32;
    let mut sole_corp_holder = 0;
    let mut prepay_penalty: Ext = Ext::from_f64(0.0);
    if 0 < auto_mode {
        if g.ai_target_corp == c {
            if g.tax_loss_carry[c] < g.exchange_rate * -Ext::from_f64(50.0) {
                return;
            }
            if g.net_worth < g.market_cap[c] * Ext::from_f64(100.0) {
                return;
            }
        }
        if c < 131 {
            return;
        }
        if get_controller_type(g, c) == 1 {
            return;
        }
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) || cast::<i32, _>(g.superfund_corp) == c {
            return;
        }
        if g.net_worth < g.exchange_rate * -Ext::from_f64(5.0) {
            return;
        }
        if g.tmp_subsid_stock_value != Ext::from_f64(0.0) {
            return;
        }
        if g.business_assets[c] != Ext::from_f64(0.0) {
            return;
        }
        if g.tax_loss_carry[c] < g.exchange_rate * -Ext::from_f64(1000.0) {
            update_corp_control(g, ui, rng, c);
            if c != g.corp_controller[c] {
                return;
            }
        }
        if g.bonds_issued[c] != 0 {
            return;
        }
        if g.tmp_player_advances > sf(0.01f32) {
            return;
        }
        if find_option_positions(g, ui, c, 0, 0, 0) != 0 {
            return;
        }
        find_option_positions(g, ui, 0, c, 0, 0);
        if g.has_options_outstanding[c] == 1 {
            return;
        }
        if count_futures_positions(g, c, 0) > 0 {
            return;
        }
        if cancel_entity_swaps(g, ui, rng, c, 2, auto_mode) == 1 {
            return;
        }
        if get_phys_commod_value(g, c, &PbStr::new()) != Ext::from_f64(0.0) {
            return;
        }
        g.extraordinary_pl[c] = to_quad(cast::<Ext, _>(g.extraordinary_pl[c]) - g.goodwill[c]);
        if g.industry_num[c] > 2 {
            recalc_commod_margin_balances(g, ui, rng, c);
            liquidate_futures_positions(g, ui, rng, c, 2, 1);
        }
    }
    if auto_mode == 0 {
        msg_text = s(__content.string(10868))
            + &f.corp_name
            + "..."
            + "\r"
            + "\r"
            + __content.string(10869)
            + __content.string(10870)
            + __content.string(10871)
            + &g.ent_symbol[c]
            + "."
            + "\r"
            + "\r"
            + __content.string(10872)
            + __content.string(10873)
            + __content.string(10874)
            + __content.string(10875)
            + __content.string(10876);
        let mut trust_purpose = PbStr::new();
        let mut trust_amt_text = PbStr::new();
        let mut trust_note = PbStr::new();
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) {
            f.trust_amt = to_quad(g.total_assets * sf(0.05f32));
            if (cast::<Ext, _>(f.trust_amt)) < g.exchange_rate * Ext::from_f64(200.0) {
                f.trust_amt = to_quad(g.exchange_rate * Ext::from_f64(200.0));
            }
            trust_purpose = s(__content.string(10877));
        } else if cast::<i32, _>(g.superfund_corp) == c {
            f.trust_amt = to_quad(g.total_assets * sf(0.05f32));
            if (cast::<Ext, _>(f.trust_amt)) < g.exchange_rate * Ext::from_f64(100.0) {
                f.trust_amt = to_quad(g.exchange_rate * Ext::from_f64(100.0));
            }
            trust_purpose = s(__content.string(10878));
        }
        if 0 < f.trust_amt {
            trust_amt_text = trim(g.currency_symbol.as_bytes())
                + &fmt_trim(cast::<Ext, _>(f.trust_amt), b"* ###,")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix;
        }
        if 0 < f.trust_amt && (cast::<Ext, _>(f.trust_amt)) > g.net_worth {
            msg_text = s(__content.string(10879))
                + &trust_amt_text
                + " "
                + __content.string(10880)
                + &trust_purpose
                + __content.string(10881)
                + __content.string(10882)
                + __content.string(10883);
            ui.msgbox(&msg_text, 8208, &s(__content.string(9638)));
            return;
        }
        if 0 < f.trust_amt {
            trust_note = s(__content.string(10884))
                + &trust_purpose
                + __content.string(10455)
                + &trust_amt_text
                + __content.string(10885);
        }
        if g.total_assets * sf(0.95f32) > g.t_bills[c] + g.cash[c]
            || cast::<Ext, _>(g.bonds_issued[c]) > g.total_assets * sf(0.1f32)
        {
            msg_text = msg_text
                + __content.string(10886)
                + __content.string(10887)
                + &g.ent_name[c]
                + ". ";
        }
        msg_text = msg_text + &trust_note + "\r" + "\r";
        if g.tax_loss_carry[c] < g.exchange_rate * -Ext::from_f64(25.0)
            || g.tax_credits[c] > g.exchange_rate * Ext::from_f64(10.0)
        {
            update_corp_control(g, ui, rng, c);
            if g.parent80_owner > 10 {
                if g.tax_loss_carry[c] < Ext::from_f64(0.0) {
                    msg_text = msg_text
                        + __content.string(10888)
                        + "on "
                        + &g.ent_symbol[c]
                        + __content.string(10889);
                }
                if g.tax_credits[c] > Ext::from_f64(0.0) {
                    msg_text = msg_text
                        + __content.string(10890);
                }
            } else {
                msg_text = msg_text
                    + __content.string(10891)
                    + __content.string(10892);
            }
            msg_text = msg_text + "\r" + "\r";
        }
        if industry == 37 {
            let advised_fund_id = get_funds_advised_by(g, c);
            if 0 < advised_fund_id {
                msg_text = msg_text
                    + __content.string(10893)
                    + __content.string(10894);
                if advised_fund_id > 85 {
                    msg_text = msg_text + " with " + &g.ent_name[advised_fund_id];
                } else {
                    msg_text = msg_text + __content.string(10895);
                }
                if g.business_assets[c] > Ext::from_f64(0.0) {
                    msg_text = msg_text
                        + __content.string(10896);
                }
                msg_text = msg_text + "." + "\r" + "\r";
            }
        }
        msg_text = msg_text + __content.string(10897);
        answer = ui.msgbox(
            &msg_text,
            8484,
            &(s(__content.string(10898)) + &g.ent_name[c]),
        );
        if answer != 6 {
            return;
        }
        if 0 < f.trust_amt {
            msg_text = PbStr::new();
            if g.bonds_issued[c] == 0 && g.business_assets[c] == Ext::from_f64(0.0) {
                msg_text = s("First, ");
            }
            msg_text = msg_text
                + &f.corp_name
                + __content.string(10899)
                + &trust_purpose
                + __content.string(10455)
                + &trust_amt_text
                + ". "
                + __content.string(10900)
                + "\r"
                + "\r"
                + __content.string(10901);
            ui.msgbox(&msg_text, 8256, &s(__content.string(10902)));
            g.cash[c] = g.cash[c] - cast::<Ext, _>(f.trust_amt);
            // AccruedPL(corpId) = AccruedPL(corpId): no-op in the original (bug, see BUGS.md)
        }
        if g.bonds_issued[c] > 0 {
            msg_text = s("First, ")
                + &f.corp_name
                + __content.string(10903)
                + __content.string(10904);
            show_quick_message(g, ui, &mut msg_text);
            loop {
                prev_bonds_issued = g.bonds_issued[c];
                crate::logic::corp_finance::bonds::bond_buyback_menu(g, ui, rng, 0, time_secs);
                if cast::<Ext, _>(g.bonds_issued[c]) == cast::<Ext, _>(prev_bonds_issued) {
                    liq_refund_trust(g, &f);
                    return;
                }
                if g.bonds_issued[c] > 0 {
                    answer = ui.msgbox(
                        &(f.corp_name.clone()
                            + __content.string(10905)
                            + __content.string(10906)),
                        8227,
                        &(s(__content.string(10907)) + &g.ent_name[c]),
                    );
                    if answer != 6 {
                        liq_refund_trust(g, &f);
                        return;
                    }
                    prev_bonds_issued = -1;
                }
                if prev_bonds_issued != -1 {
                    break;
                }
            }
        }
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
        if 130 < c && g.business_assets[entity_id] > Ext::from_f64(0.0) {
            msg_text = f.corp_name.clone()
                + __content.string(10908)
                + __content.string(10909);
            if prev_bonds_issued == 0 {
                msg_text = s("First, ") + &msg_text;
            } else {
                msg_text = s("Next, ") + &msg_text;
            }
            show_quick_message(g, ui, &mut msg_text);
            loop {
                let prev_biz_assets = g.business_assets[c];
                crate::logic::assets::sell::sell_business_assets(g, ui, rng, now);
                if g.business_assets[entity_id] >= sf(0.5f32)
                    && g.business_assets[entity_id] == prev_biz_assets
                {
                    liq_refund_trust(g, &f);
                    return;
                }
                if g.business_assets[entity_id] < sf(0.5f32) {
                    g.business_assets[entity_id] = Ext::from_f64(0.0);
                }
                if g.business_assets[entity_id] == Ext::from_f64(0.0) {
                    break;
                }
            }
        }
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
        if find_option_positions(g, ui, c, 0, 0, 0) != 0 {
            msg_text = f.corp_name.clone()
                + __content.string(10910)
                + __content.string(10911)
                + __content.string(10912);
            answer = ui.msgbox(&msg_text, 8484, &s(__content.string(10913)));
            if answer != 6 {
                liq_refund_trust(g, &f);
                return;
            }
            let mut opt_flag = 1;
            while opt_flag != 0 {
                opt_flag = find_option_positions(g, ui, c, 0, 0, 0);
                if opt_flag == 0 {
                    break;
                }
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                close_option_positions(g, ui, rng, c, 0, 0, 0);
            }
        }
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
        let mut has_futures = 0;
        for i in 1..=1000 {
            if val(parse_any(g.futures_recs[i].as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(c) {
                has_futures = 1;
                break;
            }
        }
        if has_futures == 1 {
            recalc_commod_margin_balances(g, ui, rng, c);
            msg_text = f.corp_name.clone()
                + __content.string(10914)
                + __content.string(10915)
                + __content.string(10916);
            answer = ui.msgbox(&msg_text, 8484, &s(__content.string(10917)));
            if answer != 6 {
                liq_refund_trust(g, &f);
                return;
            }
            liquidate_futures_positions(g, ui, rng, c, 2, 1);
            let mut m = s(__content.string(10918)) + &f.corp_name + __content.string(10919);
            show_quick_message(g, ui, &mut m);
        }
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
        if get_phys_commod_value(g, c, &PbStr::new()) > Ext::from_f64(0.0) {
            msg_text = f.corp_name.clone()
                + __content.string(10920)
                + __content.string(10921)
                + __content.string(10922);
            answer = ui.msgbox(&msg_text, 8484, &s(__content.string(9509)));
            if answer != 6 {
                liq_refund_trust(g, &f);
                return;
            }
            liquidate_all_phys_commod(g, ui, rng, c);
            let mut m = s(__content.string(9510)) + &f.corp_name + __content.string(9511);
            show_quick_message(g, ui, &mut m);
        }
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
        if cancel_entity_swaps(g, ui, rng, c, 2, auto_mode) == 1 {
            liq_refund_trust(g, &f);
            return;
        }
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
        let mut taxable_inc: Ext;
        if g.tax_loss_carry[c] > Ext::from_f64(0.0) {
            taxable_inc = cast::<Ext, _>(g.extraordinary_pl[c]) + g.accrued_pl[c];
            taxable_inc = g.div_recvd1[c] * sf(0.35f32) + taxable_inc;
            taxable_inc = g.div_recvd2[c] * sf(0.5f32) + taxable_inc;
            taxable_inc = g.consol_tax_inc_ytd[c] + taxable_inc;
        } else {
            taxable_inc = g.tax_loss_carry[c];
            taxable_inc = g.accrued_pl[c] + taxable_inc;
            taxable_inc = cast::<Ext, _>(g.extraordinary_pl[c]) + taxable_inc;
            taxable_inc = g.div_recvd1[c] * sf(0.35f32) + taxable_inc;
            taxable_inc = g.div_recvd2[c] * sf(0.5f32) + taxable_inc;
            taxable_inc = g.consol_tax_inc_ytd[c] + taxable_inc;
        }
        for i in 1..=15 {
            if liq_check_solvency(g, ui, rng, &f) {
                return;
            }
            if g.hold_stock_id[(i, c)] > 0 {
                let held_corp_id = cast::<i32, _>(g.hold_stock_id[(i, c)]);
                if g.hold_stock_pct[(i, c)] == 100 && g.difficulty_level > 1 {
                    msg_text = f.corp_name.clone()
                        + __content.string(10923)
                        + &g.ent_symbol[held_corp_id]
                        + __content.string(10924)
                        + __content.string(10925)
                        + "by "
                        + &g.ent_symbol[c]
                        + __content.string(10926)
                        + &g.ent_symbol[c]
                        + __content.string(10927)
                        + &g.ent_symbol[held_corp_id]
                        + __content.string(10928);
                    show_quick_message(g, ui, &mut msg_text);
                    liq_refund_trust(g, &f);
                    return;
                }
                msg_text = s(__content.string(9551))
                    + &g.ent_name[c]
                    + " "
                    + __content.string(10929)
                    + "in "
                    + &g.ent_name[held_corp_id]
                    + "...."
                    + "\r"
                    + "\r";
                let sell_slot = find_holding_slot(g, c, held_corp_id);
                let mut est_tax_on_gain = to_quad(
                    cast::<Ext, _>(g.hold_stock_pct[(sell_slot, c)]) * g.market_cap[held_corp_id] * sf(0.85f32)
                        - g.hold_stock_basis[(sell_slot, c)],
                );
                let sale_gain = cast::<Ext, _>(est_tax_on_gain);
                if taxable_inc < Ext::from_f64(0.0) {
                    est_tax_on_gain = to_quad(taxable_inc + cast::<Ext, _>(est_tax_on_gain));
                }
                taxable_inc = sale_gain + taxable_inc;
                if cast::<Ext, _>(est_tax_on_gain) > g.exchange_rate * Ext::from_f64(50.0) {
                    est_tax_on_gain =
                        to_quad(sf(0.01f32) * cast::<Ext, _>(g.corp_tax_rate) * cast::<Ext, _>(est_tax_on_gain));
                    amt_text = trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(cast::<Ext, _>(est_tax_on_gain), b"* ###,");
                    msg_text = msg_text
                        + __content.string(9397)
                        + &g.ent_name[c]
                        + __content.string(10930)
                        + __content.string(10931)
                        + __content.string(10932)
                        + &amt_text
                        + " "
                        + &g.million_prefix_lc
                        + "illion."
                        + "\r"
                        + "\r";
                    if g.hold_stock_basis[(sell_slot, c)] < Ext::from_f64(0.0)
                        && g.hold_stock_basis[(sell_slot, c)].abs()
                            > g.market_cap[held_corp_id]
                                * sf(0.15f32)
                                * cast::<Ext, _>(g.hold_stock_pct[(sell_slot, c)])
                    {
                        msg_text = msg_text
                            + &g.ent_name[c]
                            + __content.string(10933)
                            + __content.string(10934)
                            + &g.ent_name[held_corp_id]
                            + __content.string(10935)
                            + __content.string(10936)
                            + "\r"
                            + "\r";
                    }
                }
                msg_text = msg_text + __content.string(10937);
                answer = ui.msgbox(
                    &msg_text,
                    8484,
                    &(s(__content.string(10938)) + &g.ent_name[held_corp_id]),
                );
                if answer != 6 {
                    liq_refund_trust(g, &f);
                    return;
                }
                g.tmp_ext1 = cast::<Ext, _>(c);
                g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(i, c)]);
                g.tmp_ext3 = Ext::from_f64(0.0);
                sell_stock_holding(g, ui, rng, held_corp_id);
            }
        }
    }
    if liq_check_solvency(g, ui, rng, &f) {
        return;
    }
    if g.industry_num[entity_id] < 71 {
        liq_final_consol_tax(g, ui, rng, &mut f);
    }
    g.accrued_pl[c] = Ext::from_f64(0.0);
    g.extraordinary_pl[c] = 0;
    g.consol_tax_inc_ytd[c] = Ext::from_f64(0.0);
    g.div_recvd1[c] = Ext::from_f64(0.0);
    g.div_recvd2[c] = Ext::from_f64(0.0);
    g.tmp_accrued_inc_tax = f.final_tax_paid;
    g.tmp_bank_loan = g.bank_debt[entity_id];
    if g.tmp_bank_loan > g.cash[entity_id] {
        if liq_check_solvency(g, ui, rng, &f) {
            return;
        }
    }
    g.cash[entity_id] = g.cash[entity_id] - g.tmp_bank_loan;
    let lender = g.lender_id;
    g.cash[lender] = g.tmp_bank_loan + g.cash[lender];
    g.bank_debt[entity_id] = Ext::from_f64(0.0);
    g.cash[entity_id] = g.t_bills[entity_id] + g.cash[entity_id];
    g.t_bills[entity_id] = Ext::from_f64(0.0);
    g.lender_id = cast::<i32, _>(g.lending_bank[entity_id]);
    if g.tmp_bank_loan > Ext::from_f64(0.0)
        && g.corp_controller[entity_id] != g.corp_controller[g.lender_id]
        && g.difficulty_level == 3
    {
        prepay_penalty = g.tmp_bank_loan * sf(0.02f32);
        if g.cash[entity_id] < prepay_penalty && g.cash[entity_id] > Ext::from_f64(0.0) {
            prepay_penalty = g.cash[entity_id] * sf(1.01f32);
        }
        let mut penalty_tax_saving = Ext::from_f64(0.0);
        if g.tmp_accrued_inc_tax > Ext::from_f64(0.0) {
            penalty_tax_saving = cast::<Ext, _>(g.corp_tax_rate) * sf(0.01f32) * prepay_penalty;
            if penalty_tax_saving > g.tmp_accrued_inc_tax {
                penalty_tax_saving = g.tmp_accrued_inc_tax;
            }
            g.cash[entity_id] = penalty_tax_saving + g.cash[entity_id];
            g.tmp_accrued_inc_tax = g.tmp_accrued_inc_tax - penalty_tax_saving;
        }
        g.cash[entity_id] = g.cash[entity_id] - prepay_penalty;
        let l = g.lender_id;
        g.cash[l] = prepay_penalty + g.cash[l];
        g.accrued_pl[l] = prepay_penalty + g.accrued_pl[l];
        prepay_penalty = prepay_penalty - penalty_tax_saving;
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        g.news_msg = g.ent_symbol[entity_id].clone()
            + __content.string(10939)
            + &g.ent_symbol[g.lender_id];
        post_news(g, ui, 0, 0);
    }
    if g.tmp_accrued_inc_tax > Ext::from_f64(10.0) {
        tax_news_msg = s(__content.string(10940))
            + &g.ent_symbol[entity_id]
            + " was "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(g.tmp_accrued_inc_tax, b"* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "il."
            + &g.currency_suffix;
    } else if g.tmp_accrued_inc_tax < -Ext::from_f64(10.0) {
        tax_news_msg = s(__content.string(10941))
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(g.tmp_accrued_inc_tax.abs(), b"* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "il."
            + &g.currency_suffix
            + __content.string(10942)
            + &g.ent_symbol[entity_id];
        update_corp_control(g, ui, rng, entity_id);
        if g.parent80_owner > 10 {
            tax_news_msg = replace(tax_news_msg.as_bytes(), b"refund", __content.bytes(10943));
        }
    }
    if liq_check_solvency(g, ui, rng, &f) {
        return;
    }
    for i in 1..=5 {
        // msgText = ""
        if g.player_advances[(i, entity_id)] != Ext::from_f64(0.0) {
            let mut msg_text = f.corp_name.clone() + __content.string(10944) + &g.ent_symbol[i];
            g.tmp_bank_loan = g.player_advances[(i, entity_id)];
            g.cash[entity_id] = g.cash[entity_id] - g.tmp_bank_loan;
            g.cash[i] = g.tmp_bank_loan + g.cash[i];
            g.player_advances[(i, entity_id)] = Ext::from_f64(0.0);
            if !msg_text.is_empty() {
                show_quick_message(g, ui, &mut msg_text);
            }
        }
    }
    if g.cash[entity_id] < Ext::from_f64(0.0) {
        g.cash[entity_id] = Ext::from_f64(0.0);
    }
    let distrib_cash = to_quad(g.cash[entity_id]);
    if find_short_positions(g, ui, rng, c, 0) == 2 {
        force_cover_shorts(g, ui, rng, 1, 0, c, 0);
    }
    f.final_net_income = g.contingency_reserve[c] + f.final_net_income;
    f.final_net_income = f.final_net_income - prepay_penalty;
    for i in 1..=1600 {
        if !(i >= 11 || g.num_players >= i) {
            continue;
        }
        if g.entity_status[i] != 1 || c == i {
            continue;
        }
        for hold_slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(hold_slot, i)]) != c || g.hold_stock_pct[(hold_slot, i)] < 1 {
                continue;
            }
            let holder_pct = g.hold_stock_pct[(hold_slot, i)];
            let holder_payout = sf(0.01f32) * cast::<Ext, _>(distrib_cash) * cast::<Ext, _>(holder_pct);
            g.cash[i] = holder_payout + g.cash[i];
            if i < 11 {
                g.player_cap_gains[i] =
                    holder_payout - g.hold_stock_basis[(hold_slot, i)] + g.player_cap_gains[i];
            } else {
                g.extraordinary_pl[i] = to_quad(
                    holder_payout - g.hold_stock_basis[(hold_slot, i)]
                        + cast::<Ext, _>(g.extraordinary_pl[i]),
                );
                if holder_pct == 100 {
                    sole_corp_holder = i;
                }
            }
            if i > 10 && holder_pct > 19 {
                g.equity_income_ytd[i] =
                    sf(0.01f32) * f.final_net_income * cast::<Ext, _>(holder_pct) + g.equity_income_ytd[i];
                g.extra_items_ytd[i] =
                    sf(0.01f32) * f.final_extra_items * cast::<Ext, _>(holder_pct) + g.extra_items_ytd[i];
            }
            g.hold_stock_id[(hold_slot, i)] = 0;
            g.hold_stock_pct[(hold_slot, i)] = 0;
            g.hold_stock_basis[(hold_slot, i)] = Ext::from_f64(0.0);
            amt_text = fmt_trim(holder_payout, b"* ###,.00");
            if holder_payout >= sf(0.01f32) {
                g.news_msg = g.ent_symbol[i].clone()
                    + __content.string(10945)
                    + &trim(g.currency_symbol.as_bytes())
                    + &amt_text
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(4359)
                    + &g.ent_symbol[c];
                post_news(g, ui, 0, 0);
            }
        }
    }
    g.news_msg = g.ent_name[entity_id].clone()
        + " ("
        + &g.ent_symbol[entity_id]
        + __content.string(10946);
    post_news(g, ui, entity_id, 0);
    let opt_flag;
    if distrib_cash == 0 {
        g.news_msg = s(__content.string(10947))
            + &g.ent_symbol[entity_id]
            + __content.string(10948);
        post_news(g, ui, entity_id, 0);
        opt_flag = 2;
    } else {
        g.market_cap[entity_id] = cast::<Ext, _>(distrib_cash) / Ext::from_f64(100.0);
        opt_flag = 1;
    }
    if close_option_positions(g, ui, rng, 0, entity_id, opt_flag, 0) == 1 {
        g.news_msg = s(__content.string(2782))
            + &g.ent_symbol[entity_id]
            + __content.string(10949);
        let mut m = g.news_msg.clone() + __content.string(9815);
        show_quick_message(g, ui, &mut m);
        post_news(g, ui, 0, 0);
    }
    if g.merger_acquirer == entity_id || g.merger_target == entity_id {
        cancel_pending_merger(g, ui, entity_id);
    }
    g.market_cap[entity_id] = Ext::from_f64(0.0);
    g.ent_misc_data[entity_id] = PbStr::new();
    if sole_corp_holder > 10 {
        g.ent_misc_data[entity_id] = trim(str_num(cast::<Ext, _>(sole_corp_holder), 15).as_bytes());
    }
    if g.tax_loss_carry[entity_id] < Ext::from_f64(0.0) && auto_mode < 2 {
        tax_news_msg = s(__content.string(9528))
            + &g.ent_symbol[entity_id]
            + __content.string(10950);
    }
    g.industry_num[entity_id] = 70;
    g.entity_status[c] = 2;
    clear_all_holdings(g, 2);
    if auto_mode < 2 {
        g.turn_action_count += 1;
        let mut m = s(__content.string(10951)) + &f.corp_name + __content.string(10952);
        show_quick_message(g, ui, &mut m);
    }
    if !tax_news_msg.is_empty() {
        g.news_msg = tax_news_msg;
        post_news(g, ui, entity_id, 0);
    }
    if c == g.last_sel_entity {
        // (UI) CONTROL SET TEXT hDlgMain, 173, "Se&lect Last"
        g.last_sel_entity = 0;
    }
    clear_entity_record(g, ui, rng, c);
    g.active_corp_count -= 1;
    rebuild_streaming_quotes_panel(g, ui, 2);
    pack_holding_arrays(g, ui);
}

// ---------------------------------------------------------------------------------------------------------
// BankruptcyLiquidation
// ---------------------------------------------------------------------------------------------------------

/// Locals of BankruptcyLiquidation shared with its GOSUB blocks.
#[derive(Default)]
struct BkFrame {
    corp_id: i32,
    bk_mode: i32,
    /// 2 = wipe out completely, 1 = haircut.
    wipe_mode: i32,
    quiet_news: i32,
    stock_haircut: Ext,
    bond_haircut: f32,
    advance_haircut: Ext,
    /// `holderId` (-0x9c): holder in WipeOutStockholders, the lender's holding slot in LenderTakesEquity.
    holder_id: i32,
    msg_text: PbStr,
    msg_text2: PbStr,
    tmp_amt: Ext,
    loss_amt: Ext,
}

/// Port of Bankruptcy_WipeOutStockholders (GOSUB 0x4eb6a2).
/// Writes down every holding of the bankrupt stock: `wipeMode = 2` cancels it (price history cleared,
/// news "All old stock of … has been canceled"), else the percentage is cut by `stockHaircut`. The lost
/// basis is booked to capital gains (players) or extraordinary P&L; players' short positions profit
/// (mode 2: closed via `ForceCoverShorts(5, …)`); a remaining negative basis below 80% is recaptured.
/// Headlines unless `quietNews`.
fn bk_wipe_out_stockholders(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BkFrame) {
    let __content = crate::content::current();
    let c = f.corp_id;
    if f.wipe_mode == 2 {
        clear_price_history(g, c);
        if f.bk_mode != 7 {
            g.news_msg =
                s(__content.string(10953)) + &g.ent_symbol[c] + __content.string(10954);
            post_news(g, ui, 0, 0);
        }
    }
    for owner_loop in 1..=1600 {
        let mut recaptured_flag: i16 = 0;
        for idx in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(idx, owner_loop)]) != c {
                continue;
            }
            f.holder_id = owner_loop;
            let h = f.holder_id;
            let mut slot = 1;
            while slot <= 15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == c {
                    let mut is_short = 0;
                    if g.hold_stock_pct[(slot, h)] < 0 {
                        is_short = 1;
                    }
                    if (cast::<Ext, _>(g.hold_stock_pct[(slot, h)])).abs() == Ext::from_f64(1.0) {
                        g.hold_stock_pct[(slot, h)] = 0;
                    }
                    let pct = cast::<Ext, _>(g.hold_stock_pct[(slot, h)]);
                    g.hold_stock_pct[(slot, h)] = cast::<i16, _>(to_quad(pct - pct * f.stock_haircut));
                    if f.wipe_mode == 2 {
                        g.hold_stock_pct[(slot, h)] = 0;
                    }
                    f.tmp_amt = f.stock_haircut;
                    if f.wipe_mode == 2 || g.hold_stock_pct[(slot, h)] == 0 {
                        f.tmp_amt = Ext::from_f64(1.0);
                        if owner_loop <= g.num_players && is_short == 1 {
                            force_cover_shorts(g, ui, rng, 5, owner_loop, c, 0);
                            slot = 16;
                            continue;
                        }
                        g.hold_stock_id[(slot, h)] = 0;
                    }
                    if h < 11 {
                        if is_short == 0 {
                            g.player_cap_gains[h] =
                                g.player_cap_gains[h] - g.hold_stock_basis[(slot, h)] * f.tmp_amt;
                        } else {
                            g.player_cap_gains[h] =
                                g.hold_stock_basis[(slot, h)] * f.tmp_amt + g.player_cap_gains[h];
                            if f.quiet_news != 1 || g.cur_player == h {
                                g.news_msg = g.ent_symbol[h].clone()
                                    + __content.string(10955)
                                    + &g.ent_symbol[c]
                                    + __content.string(10956);
                                post_news(g, ui, c, h);
                            }
                        }
                    } else {
                        g.extraordinary_pl[h] = to_quad(
                            cast::<Ext, _>(g.extraordinary_pl[h])
                                - g.hold_stock_basis[(slot, h)] * f.tmp_amt,
                        );
                    }
                    if f.wipe_mode == 2 || g.hold_stock_pct[(slot, h)] == 0 || f.tmp_amt == Ext::from_f64(1.0) {
                        g.hold_stock_basis[(slot, h)] = Ext::from_f64(0.0);
                    } else {
                        if g.hold_stock_basis[(slot, h)] > Ext::from_f64(0.0) {
                            g.hold_stock_basis[(slot, h)] =
                                (Ext::from_f64(1.0) - f.tmp_amt) * g.hold_stock_basis[(slot, h)];
                        }
                        if g.hold_stock_pct[(slot, h)] < 80 && g.hold_stock_basis[(slot, h)] < Ext::from_f64(0.0) {
                            recapture_excess_loss(g, ui, c, h);
                            recaptured_flag = 1;
                        }
                    }
                    if f.quiet_news != 1 && cast::<i32, _>(g.lending_bank[c]) != h && is_short == 0 {
                        if !g.news_msg.is_empty() {
                            post_news(g, ui, 0, 0);
                        }
                        f.msg_text2 = s("loss");
                        if recaptured_flag == 1 {
                            f.msg_text2 = s(__content.string(10957));
                        }
                        g.news_msg = g.ent_symbol[h].clone()
                            + " has "
                            + &f.msg_text2
                            + __content.string(10958)
                            + &g.ent_symbol[c];
                        post_news(g, ui, h, 0);
                    }
                    slot = 15;
                }
                slot += 1;
            }
        }
    }
}

/// Port of Bankruptcy_BondholderLosses (GOSUB 0x4ebf26).
/// Cuts the face value and cost of every holding [runtime resource wording] by `bondHaircut` (wiped out
/// completely with `wipeMode = 2`), booking the loss to capital gains or extraordinary P&L (news per
/// holder, filtered against recent headlines), shrinks `BondsIssued` (cleared with its issue info and
/// price when gone) and credits the debt reduction against a loss carryforward (corps > 85).
fn bk_bondholder_losses(g: &mut Globals, ui: &mut dyn Ui, f: &mut BkFrame) {
    let __content = crate::content::current();
    let c = f.corp_id;
    if g.bonds_issued[c] == 0 {
        return;
    }
    if f.quiet_news != 1 {
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        g.news_msg = s(__content.string(9520)) + &g.ent_symbol[c] + __content.string(10959);
    }
    if f.wipe_mode == 2 {
        g.news_msg = s(__content.string(10960))
            + &g.ent_symbol[c]
            + __content.string(10961);
        post_news(g, ui, c, 0);
    }
    let keep = Ext::from_f64(1.0) - cast::<Ext, _>(f.bond_haircut);
    for idx in 1..=1583 {
        for slot in 1..=15 {
            if g.hold_bond_id[(slot, idx)] != c {
                continue;
            }
            g.hold_bond_face[(slot, idx)] = to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, idx)]) * keep);
            if f.wipe_mode == 2 {
                g.hold_bond_face[(slot, idx)] = 0;
            }
            if g.hold_bond_face[(slot, idx)] == 0 {
                g.hold_bond_id[(slot, idx)] = 0;
            }
            f.loss_amt = cast::<Ext, _>(f.bond_haircut);
            if f.wipe_mode == 2 {
                f.loss_amt = Ext::from_f64(1.0);
            }
            if idx < 11 {
                g.player_cap_gains[idx] =
                    g.player_cap_gains[idx] - g.bond_hold_cost[(slot, idx)] * f.loss_amt;
            } else {
                g.extraordinary_pl[idx] = to_quad(
                    cast::<Ext, _>(g.extraordinary_pl[idx]) - g.bond_hold_cost[(slot, idx)] * f.loss_amt,
                );
            }
            g.bond_hold_cost[(slot, idx)] = keep * g.bond_hold_cost[(slot, idx)];
            if f.wipe_mode == 2 || g.hold_bond_face[(slot, idx)] == 0 {
                g.bond_hold_cost[(slot, idx)] = Ext::from_f64(0.0);
            }
            if f.quiet_news != 1 || g.cur_player == idx {
                f.msg_text = s(" incurs");
                if g.rand_seed % 3 == 2 {
                    f.msg_text = s(__content.string(10962));
                }
                g.news_msg = g.ent_symbol[idx].clone()
                    + &f.msg_text
                    + __content.string(10963)
                    + &g.ent_symbol[c];
                if instr(
                    1,
                    g.news_headline[60].as_bytes(),
                    g.ent_symbol[idx].as_bytes(),
                ) > 0
                    && instr(1, g.news_headline[60].as_bytes(), __content.bytes(10964)) > 0
                {
                    g.news_msg = PbStr::new();
                } else if is_in_recent_headlines(
                    g,
                    (g.ent_symbol[idx].clone() + " incurs").as_bytes(),
                ) > 0
                    || is_in_recent_headlines(
                        g,
                        (g.ent_symbol[idx].clone() + __content.string(10962)).as_bytes(),
                    ) > 0
                {
                    g.news_msg = PbStr::new();
                } else if idx < 11 {
                    post_news(g, ui, idx, c);
                }
                g.rand_seed += 1;
            }
        }
    }
    f.tmp_amt = cast::<Ext, _>(g.bonds_issued[c]);
    g.bonds_issued[c] = to_quad(keep * cast::<Ext, _>(g.bonds_issued[c]));
    if f.wipe_mode == 2 || g.bonds_issued[c] == 0 {
        g.bonds_issued[c] = 0;
        g.bond_issue_info[c] = PbStr::new();
        g.bond_price[c] = crate::pb::numeric::Cur::from_f64(Ext::from_f64(0.0));
    }
    if g.tax_loss_carry[c] < Ext::from_f64(0.0) && c > 85 {
        g.tax_loss_carry[c] = f.tmp_amt - cast::<Ext, _>(g.bonds_issued[c]) + g.tax_loss_carry[c];
        if g.tax_loss_carry[c] > Ext::from_f64(0.0) {
            g.tax_loss_carry[c] = Ext::from_f64(0.0);
        }
    }
    if f.quiet_news != 1 {
        post_news(g, ui, 0, 0);
    }
}

/// Port of Bankruptcy_AdvanceLosses (GOSUB 0x4ec853).
/// Writes down the players' loan advances to the company by `advanceHaircut` (at most 100%; all with
/// `wipeMode = 2`): capital loss for the player, extraordinary gain for the company (also reducing its
/// loss carryforward unless `bkMode = 7`), news per player.
fn bk_advance_losses(g: &mut Globals, ui: &mut dyn Ui, f: &mut BkFrame) {
    let __content = crate::content::current();
    let c = f.corp_id;
    if f.advance_haircut > Ext::from_f64(1.0) {
        f.advance_haircut = Ext::from_f64(1.0);
    }
    for idx in 1..=5 {
        if g.player_advances[(idx, c)] < Ext::from_f64(0.0) {
            g.player_advances[(idx, c)] = Ext::from_f64(0.0);
        }
        if g.player_advances[(idx, c)] == Ext::from_f64(0.0) {
            continue;
        }
        f.loss_amt = g.player_advances[(idx, c)] * f.advance_haircut;
        if f.wipe_mode == 2 {
            f.loss_amt = g.player_advances[(idx, c)];
        }
        g.player_cap_gains[idx] = g.player_cap_gains[idx] - f.loss_amt;
        g.extra_items_ytd[c] = f.loss_amt + g.extra_items_ytd[c];
        g.player_advances[(idx, c)] = g.player_advances[(idx, c)] - f.loss_amt;
        if f.bk_mode != 7 && g.tax_loss_carry[c] < Ext::from_f64(0.0) && c > 85 {
            g.tax_loss_carry[c] = f.loss_amt + g.tax_loss_carry[c];
            if g.tax_loss_carry[c] > Ext::from_f64(0.0) {
                g.tax_loss_carry[c] = Ext::from_f64(0.0);
            }
        }
        if f.wipe_mode == 2 {
            g.player_advances[(idx, c)] = Ext::from_f64(0.0);
        }
        if g.player_advances[(idx, c)] < Ext::from_f64(0.0) {
            g.player_advances[(idx, c)] = Ext::from_f64(0.0);
        }
        if f.quiet_news != 1 || g.cur_player == idx {
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            g.news_msg = g.ent_name[idx].clone()
                + __content.string(10965)
                + &g.ent_symbol[c];
            post_news(g, ui, idx, 0);
        }
    }
}

/// Port of Bankruptcy_LenderTakesEquity (GOSUB 0x4ecccd).
/// The lending bank (`LenderId`) swaps defaulted debt for 5..60% of the reorganized company's public
/// float (all of it with `wipeMode = 2`; capped at 19% when the company holds 20%+ of the bank) at the
/// market cap, crediting the value to its bad-debt reserve; control is re-evaluated (a human player
/// losing control hands the autopilot setting to the new controller) and a randomly worded headline is
/// posted.
fn bk_lender_takes_equity(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BkFrame) {
    let __content = crate::content::current();
    let c = f.corp_id;
    f.holder_id = find_holding_slot(g, g.lender_id, c);
    if f.holder_id == 0 {
        return;
    }
    let old_controller = g.corp_controller[c];
    if g.corp_controller[c] == g.corp_controller[g.lender_id] {
        g.ctrl_change_msg_mode = 1;
    }
    update_corp_control(g, ui, rng, c);
    if 0 < old_controller
        && old_controller <= g.num_players
        && g.is_computer_player[old_controller] == 0
        && g.corp_controller[c] != old_controller
        && get_controller_type(g, c) == 1
    {
        g.auto_pilot[c] = g.auto_pilot[g.corp_controller[c]];
    }
    if g.public_pct == 0 {
        return;
    }
    let mut idx = cast::<i32, _>(g.public_pct);
    if f.wipe_mode != 2 && idx > 60 {
        idx = 60;
    }
    let mut pct_cap = idx;
    if f.wipe_mode != 2 && idx > 20 && idx < 61 {
        idx = rng.rnd_range(pct_cap, 20);
    }
    if cast::<Ext, _>(pct_cap) * sf(0.3f32) > cast::<Ext, _>(idx) {
        idx = to_long(cast::<Ext, _>(pct_cap) * sf(0.4f32));
    }
    if idx > 19 {
        pct_cap = find_holding_slot(g, c, g.lender_id);
        if 0 < pct_cap && g.hold_stock_pct[(pct_cap, c)] >= 20 {
            idx = 19;
        }
    }
    if idx < 5 {
        idx = 5;
    }
    if (cast::<i32, _>(g.public_pct)) < idx {
        idx = cast::<i32, _>(g.public_pct);
    }
    let value = to_long(g.market_cap[c] * cast::<Ext, _>(idx));
    let (hs, l) = (f.holder_id, g.lender_id);
    g.hold_stock_id[(hs, l)] = cast::<i16, _>(c);
    g.hold_stock_pct[(hs, l)] = (cast::<i16, _>(idx)).wrapping_add(g.hold_stock_pct[(hs, l)]);
    g.hold_stock_basis[(hs, l)] = cast::<Ext, _>(value) + g.hold_stock_basis[(hs, l)];
    g.bad_debt_reserve[l] = cast::<Ext, _>(value) + g.bad_debt_reserve[l];
    f.msg_text = s(__content.string(10966));
    if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
        f.msg_text = s(" (bank)");
    }
    f.msg_text2 = s(" swaps");
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) && f.msg_text.len() < 10 {
        f.msg_text2 = s(__content.string(10967));
    }
    if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
        f.msg_text2 = s(__content.string(10968));
    }
    let pct_s = str_s(cast::<Ext, _>(idx));
    g.news_msg = g.ent_symbol[l].clone()
        + &f.msg_text
        + &f.msg_text2
        + __content.string(10969)
        + &pct_s
        + __content.string(10970)
        + &g.ent_symbol[c];
    if g.rand_seed % 5 == 4 {
        f.msg_text = s(" takes");
        if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
            f.msg_text = s(__content.string(10971));
        }
        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
            f.msg_text = s(__content.string(10972));
        }
        if rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
            f.msg_text = s(__content.string(10973));
        }
        if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) {
            f.msg_text = s(__content.string(2861));
        }
        f.msg_text2 = s(__content.string(10974));
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            f.msg_text2 = s(__content.string(10975));
        }
        g.news_msg = f.msg_text2.clone()
            + &g.ent_symbol[l]
            + &f.msg_text
            + &pct_s
            + "% of "
            + &g.ent_symbol[c]
            + __content.string(10976);
    }
    if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) && idx == 100 {
        g.news_msg = g.ent_symbol[l].clone()
            + __content.string(10977)
            + &g.ent_symbol[c]
            + __content.string(10978);
    }
    if g.rand_seed % 5 == 3 && idx < 100 {
        g.news_msg = g.ent_symbol[l].clone()
            + __content.string(10979)
            + &g.ent_symbol[c]
            + __content.string(10980)
            + &pct_s
            + __content.string(10981);
    }
    if g.rand_seed % 5 == 2 {
        g.news_msg = s(__content.string(10982))
            + &g.ent_symbol[c]
            + __content.string(10983)
            + &pct_s
            + __content.string(10984)
            + &g.ent_symbol[l]
            + __content.string(10985);
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            g.news_msg = g.ent_symbol[c].clone()
                + " issues"
                + &pct_s
                + __content.string(10986)
                + &g.ent_symbol[l]
                + __content.string(10987);
        }
    }
    if g.rand_seed % 5 == 1 {
        f.msg_text = s("court");
        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
            f.msg_text = s("referee");
        }
        g.news_msg = s(__content.string(10988))
            + &f.msg_text
            + " awards"
            + &pct_s
            + "% of "
            + &g.ent_symbol[c]
            + __content.string(4295)
            + &g.ent_symbol[l];
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            g.news_msg = g.news_msg.clone() + __content.string(10989);
        }
    }
    post_news(g, ui, 0, 0);
}

/// Port of BankruptcyLiquidation (0x4e7250) with its GOSUBs Bankruptcy_WipeOutStockholders (0x4eb6a2),
/// Bankruptcy_BondholderLosses (0x4ebf26), Bankruptcy_AdvanceLosses (0x4ec853) and
/// Bankruptcy_LenderTakesEquity (0x4ecccd).
/// Bankruptcy of active entity `corp_id`; mode in `TmpExt1` (rating mode in `TmpExt2`, both cleared). A
/// contingency reserve goes to equity income first.
///
/// * `TmpExt1 = 7`: complete liquidation — shorts covered, swaps/futures closed, all balances cleared,
///   status 2 ("BKRUP"); the bank loan is written off against the lender's bad-debt reserve (pop-up
///   data in `PendingPopup*`), stock, bonds and advances wiped out, asbestos/SuperFund/cheat/merger/tip
///   references dropped, `ClearEntityRecord`, `ActiveCorpCount - 1`.
/// * `TmpExt1 = 11`: reorganization. Banks (industry 1) are taken over by the regulator: bondholders get
///   10%, stock and advances wiped out, the parent bank (id - 1) loses 10..30% of a loan above 500;
///   while capital is short, depositors of the bank (players and corps > 85 with it as lender) lose 1%
///   per round (up to 200), the payout percentage is reported and a big loss depresses the market;
///   the bank is recapitalized. Insurers (industry 2) without debt are [runtime resource wording]
///   (reserves cut 30%, cash restored). Others emerge from bankruptcy: stock, advances and bonds take
///   haircuts, then up to 4 more rounds of 8% loan write-downs while the credit rating stays below 4;
///   a "C"/"D" rating costs the lender another 30% (stock and advances wiped out) and, below a rating of
///   3, the rest of the loan.
///
/// Then: options on the stock are closed, forgiven debt reduces the loss carryforward / tax credits, the
/// stock is repriced, the lender may take equity (big losses), ETF adviser contracts are dropped, a fund
/// that went private becomes industry 70, and the company joins `NewMgmtList`.
///
/// After the liquidation of the index funds 1584/1585 it runs `ReviveDefunctEtfs`. `LenderId` is the global's current value outside mode 7 (the caller sets it).
pub fn bankruptcy_liquidation(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    time_secs: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("Bankruptcy"): status <> 1 raises ERROR 99 and ends the routine.
    let c = corp_id;
    if g.entity_status[c] != 1 {
        return;
    }
    let mut f = BkFrame {
        corp_id: c,
        ..BkFrame::default()
    };
    let industry = g.industry_num[c];
    f.bk_mode = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let rating_mode = to_long(g.tmp_ext2);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let mut advances_total = Ext::from_f64(0.0);
    for idx in 1..=5 {
        if g.player_advances[(idx, c)] > Ext::from_f64(0.0) {
            advances_total = g.player_advances[(idx, c)] + advances_total;
        }
    }
    let orig_bank_debt = g.bank_debt[c];
    if g.contingency_reserve[c] > Ext::from_f64(0.0) {
        g.equity_income_ytd[c] = g.contingency_reserve[c] + g.equity_income_ytd[c];
        g.contingency_reserve[c] = Ext::from_f64(0.0);
        g.reserve_adj_pending[c] = Ext::from_f64(0.0);
    }
    let mut debt_forgiven: Ext = Ext::from_f64(0.0);
    let mut lender_loss_total: i64 = 0;
    'done: {
        if f.bk_mode == 7 {
            g.pending_popup_entity = c;
            g.pending_popup_pic = 481;
            if find_short_positions(g, ui, rng, c, 0) == 2 {
                force_cover_shorts(g, ui, rng, 5, 0, c, 0);
            }
            cancel_entity_swaps(g, ui, rng, c, 1, 2);
            recalc_commod_margin_balances(g, ui, rng, c);
            liquidate_futures_positions(g, ui, rng, c, 2, 1);
            g.t_bills[c] = Ext::from_f64(0.0);
            g.disc_spend_pct[c] = 0;
            g.cash[c] = Ext::from_f64(0.0);
            g.profit_margin[c] = 0;
            g.market_cap[c] = Ext::from_f64(0.0);
            g.dividend_per_share[c] = Ext::from_f64(0.0);
            g.shares_outstanding[c] = Ext::from_f64(1.0);
            g.earnings_ytd[c] = Ext::from_f64(0.0);
            g.earnings_y1[c] = Ext::from_f64(0.0);
            g.earnings_yr2_ago[c] = Ext::from_f64(0.0);
            g.earnings_yr3_ago[c] = Ext::from_f64(0.0);
            g.earnings_yr4_ago[c] = Ext::from_f64(0.0);
            g.div_recvd1[c] = Ext::from_f64(0.0);
            g.div_recvd2[c] = Ext::from_f64(0.0);
            g.equity_income_ytd[c] = Ext::from_f64(0.0);
            g.extra_items_ytd[c] = Ext::from_f64(0.0);
            g.consol_tax_inc_ytd[c] = Ext::from_f64(0.0);
            g.bond_price[c] = crate::pb::numeric::Cur::from_f64(Ext::from_f64(0.0));
            g.ent_misc_data[c] = s("BKRUP");
            g.entity_status[c] = 2;
            clear_all_holdings(g, 2);
            g.proj_qtr_earnings[c] = Ext::from_f64(0.0);
            g.corp_controller[c] = 0;
            g.hold_stock_id[(0, c)] = 0;
            if industry > 1 {
                g.growth_rate[c] = 0;
            }
            if industry > 2 {
                g.business_assets[c] = Ext::from_f64(0.0);
                g.working_capital[c] = Ext::from_f64(0.0);
            }
            if industry == 2 {
                g.ins_reserves[c] = Ext::from_f64(0.0);
            }
            g.lender_id = cast::<i32, _>(g.lending_bank[c]);
            let bad_debt = g.bank_debt[c];
            g.bank_debt[c] = Ext::from_f64(0.0);
            g.tax_loss_carry[c] = Ext::from_f64(0.0);
            if bad_debt > Ext::from_f64(0.0) {
                let l = g.lender_id;
                g.bad_debt_reserve[l] = g.bad_debt_reserve[l] - bad_debt;
                if g.bad_debt_reserve[l] < Ext::from_f64(0.0) {
                    g.accrued_pl[l] = g.bad_debt_reserve[l] + g.accrued_pl[l] - Ext::from_f64(3.0);
                    g.bad_debt_reserve[l] = Ext::from_f64(3.0);
                }
                g.pending_popup_pic = 483;
                g.pending_popup_text = g.ent_name[l].clone() + __content.string(10990);
                g.pending_event_amount = to_quad(bad_debt);
            }
            if (c == 1584 || c == 1585) && g.industry_num[c] == 71 {
                g.pending_popup_entity = c;
                g.pending_popup_text = s("LONG");
                if c == 1585 {
                    g.pending_popup_text = s("SHORT");
                }
                g.pending_popup_pic = 469;
            }
            f.wipe_mode = 2;
            bk_wipe_out_stockholders(g, ui, rng, &mut f);
            if g.bonds_issued[c] > 0 {
                g.pending_popup_pic += 1;
                f.wipe_mode = 2;
                bk_bondholder_losses(g, ui, &mut f);
            }
            if advances_total > Ext::from_f64(0.0) {
                bk_advance_losses(g, ui, &mut f);
            }
            g.accrued_pl[c] = Ext::from_f64(0.0);
            g.extraordinary_pl[c] = 0;
            g.corp_controller[c] = 0;
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            g.news_msg = g.ent_symbol[c].clone() + __content.string(10991);
            post_news(g, ui, 0, 0);
            if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) {
                g.news_msg =
                    g.ent_symbol[c].clone() + __content.string(10992);
                post_news(g, ui, 0, 0);
                g.asbestos_corp = 0;
            }
            if cast::<i32, _>(g.superfund_corp) == c {
                g.superfund_corp = 0;
                g.superfund_end_year = 0;
                g.news_msg = g.ent_symbol[c].clone()
                    + __content.string(10993);
                post_news(g, ui, 0, 0);
            }
            if cast::<i32, _>(g.cheat_eq_fund_target) == c {
                g.cheat_eq_fund_target = 0;
                g.event_status[101] = 2;
            }
            if g.merger_acquirer == c || g.merger_target == c {
                cancel_pending_merger(g, ui, c);
            }
            if cast::<i32, _>(g.merger_tip_corp) == c || (cast::<Ext, _>(g.insider_earn_tip_corp)).abs() == cast::<Ext, _>(c) {
                clear_insider_tip(g);
            }
            clear_entity_record(g, ui, rng, c);
            g.active_corp_count -= 1;
            rebuild_streaming_quotes_panel(g, ui, 2);
            break 'done;
        }
        if f.bk_mode != 11 {
            break 'done;
        }
        cancel_entity_swaps(g, ui, rng, c, 1, 2);
        if industry == 1 {
            bk_bank_failure(g, ui, rng, &mut f);
            break 'done;
        }
        if industry == 2 && g.bonds_issued[c] == 0 && g.bank_debt[c] == Ext::from_f64(0.0) && advances_total == Ext::from_f64(0.0)
        {
            recalc_commod_margin_balances(g, ui, rng, c);
            liquidate_futures_positions(g, ui, rng, c, 2, 1);
            f.wipe_mode = 2;
            bk_wipe_out_stockholders(g, ui, rng, &mut f);
            f.msg_text = s(__content.string(10994));
            f.msg_text2 = g.ent_name[c].clone() + " (" + &g.ent_symbol[c] + ")";
            if f.msg_text2.len() > 17 {
                f.msg_text2 = g.ent_symbol[c].clone();
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            g.news_msg = f.msg_text2.clone() + &f.msg_text;
            post_news(g, ui, 0, 0);
            if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) || cast::<i32, _>(g.superfund_corp) == c {
                g.news_msg =
                    g.ent_symbol[c].clone() + __content.string(10992);
                if cast::<i32, _>(g.superfund_corp) == c {
                    g.news_msg = replace(
                        g.news_msg.as_bytes(),
                        __content.bytes(10995),
                        __content.bytes(10996),
                    );
                    g.superfund_corp = 0;
                    g.superfund_end_year = 0;
                }
                post_news(g, ui, 0, 0);
                if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) {
                    g.asbestos_corp = 0;
                }
            }
            g.pending_popup_entity = c;
            g.pending_popup_pic = 489;
            g.pending_popup_text = s("FLOUNDERING");
            if g.rand_seed % 3 == 1 {
                g.pending_popup_text = s("STRUGGLING");
            }
            if g.rand_seed % 3 == 2 {
                g.pending_popup_text = s("INCOMPETENT");
            }
            debt_forgiven = g.ins_reserves[c] * sf(0.3f32);
            g.ins_reserves[c] = g.ins_reserves[c] * sf(0.7f32);
            if g.cash[c] < Ext::from_f64(0.0) && g.t_bills[c] > Ext::from_f64(0.0) {
                g.cash[c] = g.t_bills[c] + g.cash[c];
                g.t_bills[c] = Ext::from_f64(0.0);
            }
            f.loss_amt = g.cash[c];
            if g.cash[c] < Ext::from_f64(0.0) {
                g.cash[c] = g.exchange_rate * (rng.rnd() * Ext::from_f64(2000.0) + Ext::from_f64(1000.0));
                debt_forgiven = g.cash[c] - f.loss_amt + debt_forgiven;
            }
            calc_entity_financials(g, ui, rng, c);
            while g.net_worth < Ext::from_f64(0.0) {
                g.cash[c] = Ext::from_f64(1000.0) + g.cash[c];
                debt_forgiven = Ext::from_f64(1000.0) + debt_forgiven;
                g.net_worth = Ext::from_f64(1000.0) + g.net_worth;
            }
            if g.net_worth > Ext::from_f64(1000.0) && g.ins_reserves[c] < Ext::from_f64(400.0) {
                g.ins_reserves[c] = Ext::from_f64(300.0) + g.ins_reserves[c];
                debt_forgiven = debt_forgiven - Ext::from_f64(300.0);
                if debt_forgiven < Ext::from_f64(0.0) {
                    debt_forgiven = Ext::from_f64(0.0);
                }
            }
            g.profit_margin[c] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(5.0) + cast::<Ext, _>(g.profit_margin[c])));
            for idx in 86..=130 {
                if g.entity_status[idx] == 1 && g.profit_margin[idx] < 20 {
                    let pm = Ext::from_f64(1.0) + cast::<Ext, _>(g.profit_margin[idx]);
                    g.profit_margin[idx] = cast::<i16, _>(to_quad(pm + rng.rnd() * Ext::from_f64(2.0)));
                }
            }
            g.growth_rate[c] = 2;
            break 'done;
        }
        // reorganization (Chapter 11)
        recalc_commod_margin_balances(g, ui, rng, c);
        if g.t_bills[c] > Ext::from_f64(0.0) {
            g.cash[c] = g.t_bills[c] + g.cash[c];
            g.t_bills[c] = Ext::from_f64(0.0);
        }
        liquidate_futures_positions(g, ui, rng, c, 2, 1);
        if c > 130 && g.goodwill[c] > Ext::from_f64(0.0) {
            g.extraordinary_pl[c] = to_quad(cast::<Ext, _>(g.extraordinary_pl[c]) - g.goodwill[c]);
            g.goodwill[c] = Ext::from_f64(0.0);
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        g.news_msg = g.ent_name[c].clone() + " (" + &g.ent_symbol[c] + __content.string(10997);
        if g.news_msg.len() < 46 {
            f.msg_text = s(__content.string(10998));
        } else {
            f.msg_text = s(".");
        }
        g.news_msg = g.news_msg.clone() + &f.msg_text;
        post_news(g, ui, c, 0);
        f.wipe_mode = 1;
        f.stock_haircut = rng.rnd() * sf(0.4f32) + sf(0.3f32);
        f.bond_haircut = cast::<f32, _>(f.stock_haircut * sf(0.4f32));
        f.advance_haircut = cast::<Ext, _>(f.bond_haircut) * sf(1.5f32);
        bk_wipe_out_stockholders(g, ui, rng, &mut f);
        bk_advance_losses(g, ui, &mut f);
        bk_bondholder_losses(g, ui, &mut f);
        if g.industry_num[c] < 70 {
            let pm = Ext::from_f64(3.0) + cast::<Ext, _>(g.profit_margin[c]);
            g.profit_margin[c] = cast::<i16, _>(to_quad(pm + rng.rnd() * Ext::from_f64(12.0)));
        }
        g.disc_spend_pct[c] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(17.0) + Ext::from_f64(4.0)));
        let mut lender_news = PbStr::new();
        let mut reorg_round = 0;
        while reorg_round < 4 {
            reorg_round += 1;
            calc_entity_financials(g, ui, rng, c);
            let rating_text = g.credit_rating_str.clone();
            if rating_mode == 0 && instr(1, rating_text.as_bytes(), b"CC") > 0 {
                g.credit_rating_num[c] = 5;
            }
            if g.credit_rating_num[c] >= 4 {
                if !lender_news.is_empty() && cast::<Ext, _>(lender_loss_total) >= g.exchange_rate * Ext::from_f64(20.0) {
                    g.news_msg = lender_news.clone();
                }
                if cast::<Ext, _>(lender_loss_total) > g.exchange_rate * Ext::from_f64(20.0) {
                    post_news(g, ui, 0, 0);
                }
                break 'done;
            }
            f.tmp_amt = g.bank_debt[c] * sf(0.08f32);
            g.bank_debt[c] = g.bank_debt[c] - f.tmp_amt;
            lender_loss_total = to_quad(f.tmp_amt + cast::<Ext, _>(lender_loss_total));
            let l = g.lender_id;
            if cast::<Ext, _>(lender_loss_total) > g.exchange_rate * Ext::from_f64(20.0) {
                lender_news = g.ent_symbol[l].clone()
                    + __content.string(5347)
                    + &g.currency_symbol
                    + &fmt_trim(cast::<Ext, _>(lender_loss_total), b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(10999)
                    + &g.ent_symbol[c];
            }
            g.bad_debt_reserve[l] = g.bad_debt_reserve[l] - f.tmp_amt;
            if g.bad_debt_reserve[l] < Ext::from_f64(0.0) {
                g.accrued_pl[l] = g.bad_debt_reserve[l] + g.accrued_pl[l] - Ext::from_f64(2.0);
                g.bad_debt_reserve[l] = Ext::from_f64(2.0);
            }
            f.quiet_news = 1;
            f.stock_haircut = rng.rnd() * sf(0.1f32) + sf(0.25f32);
            f.bond_haircut = cast::<f32, _>(f.stock_haircut * sf(0.65f32));
            f.advance_haircut = cast::<Ext, _>(f.bond_haircut) * sf(1.3f32);
            bk_wipe_out_stockholders(g, ui, rng, &mut f);
            bk_advance_losses(g, ui, &mut f);
            bk_bondholder_losses(g, ui, &mut f);
        }
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) {
            g.news_msg = g.ent_symbol[c].clone() + __content.string(10992);
            post_news(g, ui, 0, 0);
            if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                g.asbestos_corp = 0;
            }
        }
        calc_entity_financials(g, ui, rng, c);
        let mut rating_text = g.credit_rating_str.clone();
        if rating_text == "CC" && rating_mode == 1 && g.bank_debt[c] > cast::<Ext, _>(g.bonds_issued[c]) {
            rating_text = s("D");
            g.credit_rating_num[c] = 1;
        }
        if !(rating_text == "D") && !(rating_text == "C") {
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            break 'done;
        }
        f.tmp_amt = g.bank_debt[c] * sf(0.3f32);
        lender_loss_total = to_quad(f.tmp_amt + cast::<Ext, _>(lender_loss_total));
        let l = g.lender_id;
        if cast::<Ext, _>(lender_loss_total) > g.exchange_rate * Ext::from_f64(20.0) {
            lender_news = g.ent_symbol[l].clone()
                + __content.string(11000)
                + &g.currency_symbol
                + &fmt_trim(cast::<Ext, _>(lender_loss_total), b"* ###,")
                + " "
                + &g.million_prefix_lc
                + __content.string(11001)
                + &g.ent_symbol[c];
        }
        g.bank_debt[c] = g.bank_debt[c] - f.tmp_amt;
        g.bad_debt_reserve[l] = g.bad_debt_reserve[l] - f.tmp_amt;
        if g.bad_debt_reserve[l] < Ext::from_f64(0.0) {
            g.accrued_pl[l] = g.bad_debt_reserve[l] - Ext::from_f64(3.0) + g.accrued_pl[l];
            g.bad_debt_reserve[l] = Ext::from_f64(3.0);
        }
        f.wipe_mode = 2;
        bk_wipe_out_stockholders(g, ui, rng, &mut f);
        bk_advance_losses(g, ui, &mut f);
        if g.bank_debt[c] < sf(0.01f32) && industry != 2 {
            f.wipe_mode = 1;
            f.bond_haircut = 0.5f32;
        }
        bk_bondholder_losses(g, ui, &mut f);
        calc_entity_financials(g, ui, rng, c);
        if g.credit_rating_num[c] <= 2 {
            if g.bank_debt[c] > Ext::from_f64(0.0) {
                let written_off = to_long(g.bank_debt[c]);
                lender_loss_total = to_quad(cast::<Ext, _>(written_off) + cast::<Ext, _>(lender_loss_total));
                if cast::<Ext, _>(lender_loss_total) > g.exchange_rate * Ext::from_f64(20.0) {
                    f.msg_text = s(__content.string(11002));
                    if g.rand_seed % 3 == 1 {
                        f.msg_text = s(__content.string(5609));
                    }
                    g.news_msg = g.ent_symbol[l].clone()
                        + &f.msg_text
                        + &g.currency_symbol
                        + &fmt_trim(cast::<Ext, _>(lender_loss_total), b"* ###,")
                        + " "
                        + &g.million_prefix_lc
                        + __content.string(11003)
                        + &g.ent_symbol[c];
                    post_news(g, ui, g.lender_id, 0);
                    g.rand_seed += 1;
                }
                g.bank_debt[c] = Ext::from_f64(0.0);
                g.valuation_mode = 3;
                calc_stock_price(g, ui, rng, c, time_secs);
                let l = g.lender_id;
                g.bad_debt_reserve[l] = g.bad_debt_reserve[l] - cast::<Ext, _>(written_off);
                if g.bad_debt_reserve[l] < Ext::from_f64(0.0) {
                    g.accrued_pl[l] = g.bad_debt_reserve[l] + g.accrued_pl[l] - Ext::from_f64(2.0);
                    g.bad_debt_reserve[l] = Ext::from_f64(2.0);
                }
            }
        } else {
            if !lender_news.is_empty() {
                g.news_msg = lender_news.clone();
            }
            post_news(g, ui, 0, 0);
        }
    }
    // LAB_004eb00c
    find_option_positions(g, ui, 0, c, 0, 0);
    if g.has_options_outstanding[c] == 1 {
        close_option_positions(g, ui, rng, 0, c, 2, 0);
    }
    if g.industry_num[c] > 2 {
        debt_forgiven = orig_bank_debt - g.bank_debt[c];
    }
    if debt_forgiven > Ext::from_f64(0.0) {
        if g.tax_loss_carry[c] < Ext::from_f64(0.0) {
            g.tax_loss_carry[c] = debt_forgiven + g.tax_loss_carry[c];
            if g.tax_loss_carry[c] > Ext::from_f64(0.0) {
                debt_forgiven = sf(0.5f32) * g.tax_loss_carry[c];
                g.tax_credits[c] = g.tax_credits[c] - debt_forgiven;
                if g.tax_credits[c] < Ext::from_f64(0.0) {
                    g.tax_credits[c] = Ext::from_f64(0.0);
                }
                g.tax_loss_carry[c] = Ext::from_f64(0.0);
            }
        } else if g.tax_credits[c] > Ext::from_f64(0.0) {
            debt_forgiven = sf(0.5f32) * debt_forgiven;
            g.tax_credits[c] = g.tax_credits[c] - debt_forgiven;
            if g.tax_credits[c] < Ext::from_f64(0.0) {
                g.tax_credits[c] = Ext::from_f64(0.0);
            }
        }
    }
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, c, time_secs);
    if f.bk_mode != 7 && industry != 1 && cast::<Ext, _>(lender_loss_total) > g.exchange_rate * Ext::from_f64(20.0) {
        bk_lender_takes_equity(g, ui, rng, &mut f);
    }
    if g.entity_status[c] == 1 {
        if c > 1580 {
            etf_drop_one_adviser_contract(g, ui, rng, c, 1);
            update_corp_control(g, ui, rng, c);
            if g.industry_num[c] == 71 {
                g.industry_num[c] = 70;
            }
        } else if c > 85 {
            etf_drop_one_adviser_contract(g, ui, rng, c, 1);
        }
        if g.industry_num[c] < 70 {
            let corp_code = right(
                (s("000") + &trim(str_num(cast::<Ext, _>(c), 15).as_bytes())).as_bytes(),
                4,
            );
            if instr(1, g.new_mgmt_list.as_bytes(), corp_code.as_bytes()) < 1 {
                if g.new_mgmt_list.len() < 31 {
                    g.new_mgmt_list = g.new_mgmt_list.clone() + &corp_code + "/";
                }
                if g.new_mgmt_list.len() < 11 {
                    g.new_mgmt_list = s("XXXX/") + &g.new_mgmt_list;
                }
            }
        }
    } else if c == 1584 || c == 1585 {
        crate::logic::etf_derivs::etf_mgmt::revive_defunct_etfs(g, ui, rng, time_secs);
    }
}

/// The bank-failure branch of BankruptcyLiquidation (`TmpExt1 = 11`, industry 1); inline in the
/// original, split out for readability.
fn bk_bank_failure(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut BkFrame) {
    let __content = crate::content::current();
    let c = f.corp_id;
    let mut regulator_name = s("GOVT.");
    if g.country_idx[c] == 0 {
        regulator_name = s("FDIC");
    }
    g.pending_popup_entity = c;
    g.pending_popup_pic = 485;
    g.pending_popup_text = regulator_name.clone();
    if g.bonds_issued[c] > 0 {
        g.pending_popup_pic = 486;
        for idx in 1..=1583 {
            let mut slot = 1;
            while slot <= 15 {
                if c == g.hold_bond_id[(slot, idx)] {
                    f.tmp_amt = cast::<Ext, _>(g.hold_bond_face[(slot, idx)]) * sf(0.1f32);
                    g.cash[idx] = f.tmp_amt + g.cash[idx];
                    if idx < 11 {
                        g.player_cap_gains[idx] = f.tmp_amt + g.player_cap_gains[idx];
                    } else {
                        g.extraordinary_pl[idx] =
                            to_quad(f.tmp_amt + cast::<Ext, _>(g.extraordinary_pl[idx]));
                    }
                    slot = 15;
                }
                slot += 1;
            }
        }
        f.wipe_mode = 2;
        bk_bondholder_losses(g, ui, f);
    }
    f.wipe_mode = 2;
    bk_wipe_out_stockholders(g, ui, rng, f);
    bk_advance_losses(g, ui, f);
    let mut lender_loss: i64 = 0;
    if g.bank_debt[c] > Ext::from_f64(500.0) {
        lender_loss = to_quad((rng.rnd() * Ext::from_f64(20.0) + Ext::from_f64(10.0)) * g.bank_debt[c] * sf(0.01f32));
        g.bank_debt[c] = g.bank_debt[c] - cast::<Ext, _>(lender_loss);
        if c > 11 {
            let p = c - 1;
            g.bad_debt_reserve[p] = g.bad_debt_reserve[p] - cast::<Ext, _>(lender_loss);
            if g.bad_debt_reserve[p] < Ext::from_f64(0.0) {
                g.extraordinary_pl[p] =
                    to_quad(g.bad_debt_reserve[p] - Ext::from_f64(2.0) + cast::<Ext, _>(g.extraordinary_pl[p]));
                g.bad_debt_reserve[p] = Ext::from_f64(2.0);
            }
            g.news_msg = g.ent_symbol[p].clone()
                + __content.string(5351)
                + &g.currency_symbol
                + &fmt_trim(cast::<Ext, _>(lender_loss), b"* ###,")
                + " "
                + &g.million_prefix_lc
                + "il."
                + &g.currency_suffix
                + __content.string(11004)
                + &g.ent_symbol[c];
            post_news(g, ui, 0, 0);
        }
    }
    calc_entity_financials(g, ui, rng, c);
    let prior_net_worth = g.net_worth - cast::<Ext, _>(lender_loss);
    if g.net_worth < Ext::from_f64(0.0) {
        g.cash[c] = g.exchange_rate * Ext::from_f64(2000.0) + g.cash[c];
        g.net_worth = g.exchange_rate * Ext::from_f64(2000.0) + g.net_worth;
    }
    let mut payout_pct_f: Ext = Ext::from_f64(100.0);
    if g.net_worth < g.exchange_rate * Ext::from_f64(2000.0) {
        let mut rounds = 200;
        loop {
            payout_pct_f = sf(0.99f32) * payout_pct_f;
            f.tmp_amt =
                (g.bank_c_ds[c] + g.bank_cust_deposits[c] + g.bank_demand_deposits[c]) * sf(0.01f32);
            g.net_worth = f.tmp_amt + g.net_worth;
            g.cash[c] = g.bank_cust_deposits[c] * sf(0.01f32) + g.cash[c];
            g.bank_cust_deposits[c] = sf(0.99f32) * g.bank_cust_deposits[c];
            g.bank_c_ds[c] = sf(0.99f32) * g.bank_c_ds[c];
            g.bank_demand_deposits[c] = sf(0.99f32) * g.bank_demand_deposits[c];
            for slot in 1..=1600 {
                if g.entity_status[slot] != 1 || cast::<i32, _>(g.lending_bank[slot]) != c {
                    continue;
                }
                if !(g.num_players >= slot || slot >= 86) {
                    continue;
                }
                if g.cash[slot] > Ext::from_f64(0.0) {
                    f.loss_amt = g.cash[slot] * sf(0.01f32);
                    if g.num_players >= slot {
                        g.player_cap_gains[slot] = g.player_cap_gains[slot] - f.loss_amt;
                    } else if slot > 10 {
                        g.extraordinary_pl[slot] =
                            to_quad(cast::<Ext, _>(g.extraordinary_pl[slot]) - f.loss_amt);
                    }
                    g.cash[slot] = sf(0.99f32) * g.cash[slot];
                }
            }
            if g.net_worth > g.exchange_rate * Ext::from_f64(1000.0) {
                break;
            }
            rounds -= 1;
            if rounds == 0 {
                break;
            }
        }
        calc_entity_financials(g, ui, rng, c);
        if g.net_worth < g.tmp_bank_total_loans * sf(0.05f32) {
            g.cash[c] = g.tmp_bank_total_loans * sf(0.05f32) + g.cash[c];
            g.bank_consumer_loans[c] = to_quad(sf(1.02f32) * cast::<Ext, _>(g.bank_consumer_loans[c]));
            g.bank_mortgage_loans[c] = to_quad(sf(1.04f32) * cast::<Ext, _>(g.bank_mortgage_loans[c]));
            g.subprime_secs[c] = to_quad(sf(1.03f32) * cast::<Ext, _>(g.subprime_secs[c]));
            calc_entity_financials(g, ui, rng, c);
        }
    }
    let payout_pct = to_integer(payout_pct_f);
    if g.net_worth <= Ext::from_f64(0.0) {
        g.cash[c] = g.cash[c] - g.net_worth;
        g.cash[c] = g.exchange_rate * Ext::from_f64(2000.0) + g.cash[c];
        while g.cash[c] > g.exchange_rate * Ext::from_f64(3000.0) && g.bank_c_ds[c] > g.exchange_rate * Ext::from_f64(8000.0) {
            f.tmp_amt = g.cash[c] * sf(0.1f32);
            if g.bank_c_ds[c] < g.cash[c] {
                f.tmp_amt = g.bank_c_ds[c] * sf(0.1f32);
            }
            if f.tmp_amt < g.exchange_rate * Ext::from_f64(1000.0) {
                f.tmp_amt = g.exchange_rate * Ext::from_f64(1000.0);
            }
            g.cash[c] = g.cash[c] - f.tmp_amt;
            g.bank_c_ds[c] = g.bank_c_ds[c] - f.tmp_amt;
        }
    }
    calc_entity_financials(g, ui, rng, c);
    if g.net_worth > prior_net_worth {
        g.tax_loss_carry[c] = g.net_worth - prior_net_worth + g.tax_loss_carry[c];
        if g.tax_loss_carry[c] > Ext::from_f64(0.0) {
            g.tax_loss_carry[c] = Ext::from_f64(0.0);
        }
    }
    g.shares_outstanding[c] = Ext::from_f64(4.0);
    g.profit_margin[c] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(12.0) + cast::<Ext, _>(g.profit_margin[c])));
    g.disc_spend_pct[c] = cast::<i16, _>(to_quad(rng.rnd() * Ext::from_f64(11.0) + Ext::from_f64(10.0)));
    for idx in 11..=85 {
        if g.entity_status[idx] == 1 && rng.rnd() * Ext::from_f64(6.0) + Ext::from_f64(15.0) > cast::<Ext, _>(g.profit_margin[idx]) {
            g.profit_margin[idx] += 1;
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    f.msg_text = regulator_name.clone();
    if !(f.msg_text == "FDIC") {
        f.msg_text = s(__content.string(2879));
    }
    g.news_msg =
        f.msg_text.clone() + __content.string(11005) + &g.ent_symbol[c] + __content.string(11006);
    post_news(g, ui, 0, 0);
    if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) || cast::<i32, _>(g.superfund_corp) == c {
        g.news_msg = g.ent_symbol[c].clone() + __content.string(11007);
        if cast::<i32, _>(g.superfund_corp) == c {
            g.news_msg = replace(
                g.news_msg.as_bytes(),
                __content.bytes(11008),
                __content.bytes(11009),
            );
            g.superfund_corp = 0;
            g.superfund_end_year = 0;
        }
        post_news(g, ui, 0, 0);
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(c) {
            g.asbestos_corp = 0;
        }
    }
    if payout_pct == 100 {
        g.news_msg = s(__content.string(11010)) + &g.ent_symbol[c] + __content.string(11011);
        post_news(g, ui, c, 0);
    } else {
        let player_count = g.num_players;
        for idx in 1..=player_count {
            if g.entity_status[idx] == 1 && cast::<i32, _>(g.lending_bank[idx]) == c && g.cash[idx] > Ext::from_f64(0.0) {
                g.news_msg = ucase(g.ent_name[idx].as_bytes(), CaseMode::Ansi)
                    + __content.string(11012)
                    + &g.ent_symbol[c]
                    + __content.string(11013);
                post_news(g, ui, idx, c);
            }
        }
        g.pending_popup_pic += 2;
        f.msg_text2 = s(" only");
        if payout_pct > 79 {
            f.msg_text2 = PbStr::new();
        } else if payout_pct < 90 {
            g.market_valuation = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.market_valuation) - sf(0.1f32)));
            g.market_val_target = cast::<f64, _>(g.market_val_target - sf(0.2f32));
        }
        f.msg_text = regulator_name.clone();
        if !(f.msg_text == "FDIC") {
            let country = g.country_idx[c];
            f.msg_text =
                s(__content.string(9807)) + &mcase(g.country_name[country].as_bytes(), CaseMode::Ansi);
        }
        g.news_msg = f.msg_text.clone()
            + __content.string(11014)
            + &f.msg_text2
            + &PbStr::from(str_num(cast::<Ext, _>(payout_pct), 7).as_str())
            + __content.string(11015)
            + &g.ent_symbol[c]
            + __content.string(11016);
    }
}
