//! Computer-controlled corporate moves called from the per-entity simulation: random LBOs
//! (`AiRandomLBO`), the cash-deficit sweep (`ProcessNegativeCash`), takeovers by cash-rich companies
//! (`AiCashRichTakeover`), consolidation of subsidiaries, re-entering an industry, voluntary
//! liquidation of shells, tax-sheltered profit taking, capital infusions and the recent-offerings test.
//!
//! SINGLE pool constants of the original (`fmul dword [0x75e…]`) are written `sf(0.5)`.

#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::if_same_then_else,
    clippy::neg_multiply
)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

use super::corp_ops::draw_credit_line_for_cash;
use super::transactions::{liquidate_futures_positions, sell_stock_holding};
use crate::logic::commod_ai::desk::{futures_buy_or_cover, futures_sell_or_short};
use crate::logic::control::find_option_positions;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::futures::{count_futures_positions, recalc_commod_margin_balances};
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::newgame::LocalTime;
use crate::logic::newgame_helpers::clear_entity_record;
use crate::logic::restructure::{lbo_tender_offer, liquidate_sub_into_parent, merge_companies};
use crate::logic::trading::buy_stock;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::{check_cross_ownership, divest_stock};
use crate::logic::reports_lists::show_futures_positions;
use crate::logic::shorts::find_short_positions;
use crate::logic::valuation::update_price_history_tick;
use crate::pb::string::{CaseMode, mcase};

/// Port of IsRecentOffering (0x498a15).
/// 1 if `corp_id` is in `RecentOfferings(1..10)`, else -1 if `-corp_id` is, else 0.
pub fn is_recent_offering(g: &Globals, corp_id: i32) -> i32 {
    let mut result = 0;
    for i in 1..=10 {
        if corp_id == g.recent_offerings[i] {
            result = 1;
        }
        if corp_id.wrapping_mul(-1) == g.recent_offerings[i] && result == 0 {
            result = -1;
        }
    }
    result
}

/// Port of AiTakeProfitsTaxShelter (0x4c4739).
/// For the entity `SimCoCount` (an active, not human-controlled corp, no fund): sells part of the
/// first holding whose gain is at least half its cost basis — and at least half the tax-loss carry,
/// unless the stake is under 80% — through `SellStockHolding` (all of it, or 20% when the gain exceeds
/// twice the tax-loss carry; at least 1 point), with a "takes profits … [runtime resource wording] " headline
/// (dropped two times in three unless the current player holds either stock).
pub fn ai_take_profits_tax_shelter(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let corp_id = g.sim_co_count;
    if corp_id < 11 || corp_id > 1600 {
        return;
    }
    if get_controller_type(g, corp_id) == 1 {
        return;
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if g.industry_num[corp_id] == 71 {
        return;
    }
    let tax_loss_amt = to_quad(g.tax_loss_carry[corp_id].abs());
    for slot in 1..=15 {
        if (cast::<i32, _>(g.hold_stock_id[(slot, corp_id)])) < 11 {
            continue;
        }
        let held_id = cast::<i32, _>(g.hold_stock_id[(slot, corp_id)]);
        let holding_value =
            to_quad(cast::<Ext, _>(g.hold_stock_pct[(slot, corp_id)]) * g.market_cap[held_id]);
        let cost_basis = to_quad(g.hold_stock_basis[(slot, corp_id)]);
        let gain = to_quad(cast::<Ext, _>(holding_value - cost_basis));
        if !(cast::<Ext, _>(gain) >= cast::<Ext, _>(cost_basis) * sf(0.5f32)) {
            continue;
        }
        let mut pct_to_sell = cast::<i32, _>(g.hold_stock_pct[(slot, corp_id)]);
        if !(cast::<Ext, _>(gain) >= cast::<Ext, _>(tax_loss_amt) * sf(0.5f32)) && pct_to_sell >= 80 {
            continue;
        }
        if cast::<Ext, _>(gain) > cast::<Ext, _>(tax_loss_amt * 2) {
            pct_to_sell = to_long(cast::<Ext, _>(pct_to_sell) * sf(0.2f32));
        }
        if pct_to_sell < 1 {
            pct_to_sell = 1;
        }
        g.tmp_ext1 = cast::<Ext, _>(corp_id);
        g.tmp_ext2 = cast::<Ext, _>(pct_to_sell);
        g.tmp_ext3 = Ext::from_f64(2.0);
        // SellStockHolding takes heldId BYREF; it leaves a nonzero id as it is.
        sell_stock_holding(g, ui, rng, held_id);
        g.news_msg = g.ent_symbol[corp_id].clone()
            + __content.string(10481)
            + &g.ent_symbol[held_id]
            + __content.string(10482);
        if !(rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0))
            && find_stock_holding(g, g.cur_player, corp_id) == 0
            && find_stock_holding(g, g.cur_player, held_id) == 0
        {
            g.news_msg = PbStr::new();
            return;
        }
        if rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
            g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(10483), __content.bytes(10484));
        }
        post_news(g, ui, corp_id, held_id);
        return;
    }
}

/// Port of AiRandomLBO (0x4f41e5).
/// A leveraged buyout of corp `target_id` (> 0), or of a random corp 131..1600 (`target_id = 0`, which
/// adds the original's random filters): an active non-fund corp with equity over 66% of its assets and
/// over 300 (x exchange rate), not human-controlled, cheap relative to its equity (cap <= 1.5% of net
/// worth), with a public float and a largest holder; a partner is the first computer-controlled holder
/// of under 36% (scanning 300 random holders, or all for a given target). The stake to buy is the
/// partner's, else 1..15% when the target is cash-rich, else up to 10% for a 90%+ float; the deal goes
/// to `LboTenderOffer` with `TmpExt1` = 2 (1 for a given target > 10) and `TmpExt2` = stake.
/// Afterwards `TmpExt1` = 1 for a random LBO (the caller sets its lboDone flag), else 0. `now`: the
/// local time (for `LboTenderOffer`).
pub fn ai_random_lbo(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, target_id: i32, now: &LocalTime) {
    // ON ERROR GOTO -> ShowRuntimeError("RandomLBO", …) (error path not ported)
    let mut pct_to_buy: i16 = 0;
    let mut partner_id: i32 = 0;
    let mut target = rng.rnd_range(1600, 131);
    if 0 < target_id {
        target = target_id;
    }
    if g.entity_status[target] != 1 || g.industry_num[target] == 71 {
        return;
    }
    let cut = rng.rnd() * Ext::from_f64(50.0) + Ext::from_f64(50.0);
    if g.market_cap[target] < cut || !(g.earnings_ytd[target] > Ext::from_f64(0.0)) {
        if target_id == 0 {
            return;
        }
    }
    calc_entity_financials(g, ui, rng, target);
    if !(g.net_worth > g.total_assets * sf(0.66f32)) {
        return;
    }
    if !(g.net_worth > g.exchange_rate * Ext::from_f64(300.0)) {
        return;
    }
    if get_controller_type(g, target) == 1 {
        return;
    }
    if !(g.market_cap[target] <= g.net_worth * sf(0.015f32)) {
        return;
    }
    update_corp_control(g, ui, rng, target);
    if g.public_pct < 20 || g.largest_holder == 0 {
        if target_id == 0 {
            return;
        }
    }
    if g.public_pct < 90 {
        let mut first_holder = rng.rnd_range(1300, 131);
        let mut last_holder = first_holder + 300;
        if 0 < target_id {
            first_holder = 11;
            last_holder = 1600;
        }
        let limit = last_holder;
        let mut holder_id = first_holder;
        while holder_id <= limit {
            if g.entity_status[holder_id] == 1 {
                for slot in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == target {
                        pct_to_buy = g.hold_stock_pct[(slot, holder_id)];
                        if pct_to_buy > 35 {
                            pct_to_buy = 0;
                        } else {
                            partner_id = holder_id;
                            if 11 > g.corp_controller[partner_id] {
                                partner_id = 0;
                                pct_to_buy = 0;
                            } else {
                                holder_id = last_holder;
                            }
                        }
                    }
                }
            }
            holder_id += 1;
        }
    }
    if partner_id == 0 && g.industry_num[target] == 70 && target_id == 0 {
        return;
    }
    if partner_id == 0
        && !(g.net_worth * sf(0.5f32) >= g.t_bills[target] + g.cash[target])
        && !(g.total_assets * sf(0.4f32) >= g.t_bills[target] + g.cash[target])
    {
        pct_to_buy = cast::<i16, _>(g.rand_seed % 15) + 1;
        if pct_to_buy > g.public_pct {
            pct_to_buy = g.public_pct;
        }
        if g.market_cap[target] < Ext::from_f64(25.0) || pct_to_buy < 1 {
            return;
        }
    }
    if g.public_pct > 90 && pct_to_buy == 0 {
        pct_to_buy = to_integer(rng.rnd() * Ext::from_f64(10.0));
    }
    if rng.rnd() * Ext::from_f64(8.0) + Ext::from_f64(5.0) > cast::<Ext, _>(pct_to_buy) && target_id == 0 {
        return;
    }
    if pct_to_buy == 0 {
        return;
    }
    if g.market_cap[target] * cast::<Ext, _>(pct_to_buy) * sf(1.5f32) > g.liquid_funds {
        if target_id == 0 {
            return;
        }
        if g.market_cap[target] > g.net_worth * sf(0.0105f32) {
            return;
        }
    }
    g.tmp_ext1 = Ext::from_f64(2.0);
    if 10 < target_id {
        g.tmp_ext1 = Ext::from_f64(1.0);
    }
    g.tmp_ext2 = cast::<Ext, _>(pct_to_buy);
    lbo_tender_offer(g, ui, rng, target, partner_id, now);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if target_id == 0 {
        g.tmp_ext1 = Ext::from_f64(1.0);
    }
}

/// Port of AiVoluntaryLiquidation (0x4f9223).
/// A shell (industry 70, id > 130) nobody is short, with [runtime resource wording] each at most 30 (x
/// exchange rate), no bonds, no subsidiaries, 100% public and no options outstanding: if its liquid
/// funds exceed its bank debt it is refloated (cash +300..800, cap 8..13); otherwise it is wound up —
/// liquid funds repay the lender (`LendingBank`), a remaining debt is charged to the bad-debt reserve of
/// `LenderId` (sic), the slot is cleared (`ClearEntityRecord`, status 2) with a news headline.
pub fn ai_voluntary_liquidation(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if !(corp_id >= 131) {
        return;
    }
    if g.industry_num[corp_id] != 70 {
        return;
    }
    if find_short_positions(g, ui, rng, corp_id, 0) == 2 {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    let liabilities = g.total_assets - g.net_worth;
    if g.total_assets > g.exchange_rate * Ext::from_f64(30.0) || liabilities > g.exchange_rate * Ext::from_f64(30.0) {
        return;
    }
    if cast::<Ext, _>(g.bonds_issued[corp_id]) > Ext::from_f64(0.0) || g.tmp_subsid_stock_value > Ext::from_f64(0.0) {
        return;
    }
    update_corp_control(g, ui, rng, corp_id);
    if g.public_pct < 100 {
        return;
    }
    find_option_positions(g, ui, 0, corp_id, 0, 0);
    if g.has_options_outstanding[corp_id] == 1 {
        return;
    }
    if !(g.bank_debt[corp_id] >= g.t_bills[corp_id] + g.cash[corp_id]) {
        g.cash[corp_id] = (Ext::from_f64(300.0) + g.cash[corp_id]) + rng.rnd() * Ext::from_f64(500.0);
        g.market_cap[corp_id] = rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(8.0);
        update_price_history_tick(g, ui, rng, corp_id);
        return;
    }
    g.growth_rate[corp_id] = 0;
    g.profit_margin[corp_id] = 0;
    g.disc_spend_pct[corp_id] = 0;
    g.shares_outstanding[corp_id] = Ext::from_f64(1.0);
    if g.t_bills[corp_id] + g.cash[corp_id] > Ext::from_f64(0.0) {
        g.cash[corp_id] = g.t_bills[corp_id] + g.cash[corp_id];
        g.t_bills[corp_id] = Ext::from_f64(0.0);
        g.bank_debt[corp_id] = g.bank_debt[corp_id] - g.cash[corp_id];
        let lb = cast::<i32, _>(g.lending_bank[corp_id]);
        g.cash[lb] = g.cash[lb] + g.cash[corp_id];
    }
    g.cash[corp_id] = Ext::from_f64(0.0);
    if g.bank_debt[corp_id] > Ext::from_f64(0.0) {
        let l = g.lender_id;
        g.bad_debt_reserve[l] = g.bad_debt_reserve[l] - g.bank_debt[corp_id];
    }
    g.ent_misc_data[corp_id] = PbStr::new();
    g.entity_status[corp_id] = 2;
    clear_entity_record(g, ui, rng, corp_id);
    g.news_msg =
        g.ent_symbol[corp_id].clone() + __content.string(10485);
    post_news(g, ui, corp_id, 0);
}

/// Port of AiConsolidateSubsidiary (0x4f9975).
/// For an active, not human-controlled operating corp `parent_id` (> 130) and a random holding slot
/// holding a corp > 130 of the same industry: `TmpExt1` (read and cleared) is the parent's market share.
/// If the combined share would exceed 54 and both are under the same controller, the subsidiary is
/// divested (2/3, `DivestStock`) or both are told to shrink (growth -10..-7 / -10). A 100% subsidiary
/// is merged into the parent (`LiquidateSubIntoParent`, `TmpExt1` = 2); otherwise, below a 50 share, the
/// parent buys up to 40% of the float when it can afford 1.8x the price (`BuyStock`, `TmpExt1` =
/// points, `TmpExt2` = 2), unless a computer player is at its corp limit. `time_secs`: seconds of
/// `TIME$` (for the callees).
pub fn ai_consolidate_subsidiary(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, parent_id: i32, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let threshold = to_integer(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut sub_id: i32 = 0;
    let mut pct_to_buy: i32 = 0;
    let mut controller_id: i32 = 0;
    if g.entity_status[parent_id] != 1 {
        return;
    }
    if 131 > parent_id {
        return;
    }
    if g.industry_num[parent_id] > 69 {
        return;
    }
    if get_controller_type(g, parent_id) == 1 {
        return;
    }
    let slot = rng.rnd_range(15, 1);
    if g.hold_stock_id[(slot, parent_id)] > 130 {
        sub_id = cast::<i32, _>(g.hold_stock_id[(slot, parent_id)]);
        pct_to_buy = cast::<i32, _>(g.hold_stock_pct[(slot, parent_id)]);
    }
    if pct_to_buy == 0 {
        return;
    }
    if g.industry_num[sub_id] != g.industry_num[parent_id] {
        return;
    }
    if g.business_assets[parent_id] == Ext::from_f64(0.0) {
        return;
    }
    let ratio = g.business_assets[sub_id] / g.business_assets[parent_id];
    if to_long(ratio * cast::<Ext, _>(threshold) + cast::<Ext, _>(threshold)) > 54
        && g.corp_controller[parent_id] == g.corp_controller[sub_id]
    {
        if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(2.0) {
            divest_stock(g, ui, rng, sub_id, parent_id, 1);
            return;
        }
        g.growth_rate[sub_id] = cast::<i16, _>(to_quad(-Ext::from_f64(10.0) + rng.rnd() * Ext::from_f64(3.0)));
        g.growth_rate[parent_id] = -10;
    }
    if pct_to_buy == 100 {
        g.tmp_ext1 = Ext::from_f64(2.0);
        liquidate_sub_into_parent(g, ui, rng, parent_id, sub_id, time_secs);
        return;
    }
    if threshold >= 50 {
        return;
    }
    if get_controller_type(g, parent_id) == 2 {
        controller_id = g.corp_controller[parent_id];
    }
    update_corp_control(g, ui, rng, sub_id);
    if controller_id > 0
        && g.corp_controller[parent_id] != g.corp_controller[sub_id]
        && !(g.cpu_max_corps > g.player_corps_controlled[controller_id])
    {
        return;
    }
    if g.public_pct == 0 {
        return;
    }
    pct_to_buy = cast::<i32, _>(g.public_pct);
    if g.public_pct > 40 {
        pct_to_buy = to_long(cast::<Ext, _>(g.public_pct) * sf(0.4f32));
    }
    let cost = sf(1.2f32) * cast::<Ext, _>(pct_to_buy) * g.market_cap[sub_id];
    calc_entity_financials(g, ui, rng, parent_id);
    let avail = g.cash[parent_id] + g.unused_credit_line + g.t_bills[parent_id];
    if cost * sf(1.5f32) >= avail {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(pct_to_buy);
    g.tmp_ext2 = Ext::from_f64(2.0);
    buy_stock(g, ui, rng, parent_id, sub_id, time_secs);
}

/// Port of AiCorpReenterIndustry (0x4f9fd4).
/// One time in 15, for an active corp `corp_id` (normally a shell, industry 70) with a net worth of at
/// least 10: a corp worth 35+ with unused credit over 3000 or liquid funds over 4000 (x exchange rate)
/// re-enters an industry (its neighbours' if they agree, else a random one 3..69 with demand of at
/// least 100000 x exchange rate) with 2000..3000 (x exchange rate) of business assets bought for cash,
/// growth 20 (news only for a neighbour industry; slots 1471..1500 lose a negative tax-loss carry). A smaller one instead merges into some larger active corp (`MergeCompanies`, one in five
/// candidates scanned from 131). `time_secs`: seconds of `TIME$` (for the merger).
pub fn ai_corp_reenter_industry(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut industry_id: i32 = 0;
    let mut neighbour = false;
    if rng.rnd() * Ext::from_f64(15.0) > Ext::from_f64(1.0) {
        return;
    }
    if g.entity_status[corp_id] != 1 {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    if !(g.net_worth >= Ext::from_f64(10.0)) {
        return;
    }
    if !(g.market_cap[corp_id] >= Ext::from_f64(35.0)) {
        let mut target_id = 131;
        loop {
            if g.entity_status[target_id] == 1
                && corp_id != target_id
                && g.market_cap[target_id] <= g.market_cap[corp_id]
                && g.industry_num[target_id] != 71
                && !(rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0))
            {
                let cc = g.corp_controller[target_id];
                if 0 >= cc || target_id == cc || corp_id == cc {
                    calc_entity_financials(g, ui, rng, target_id);
                    if g.net_worth >= Ext::from_f64(10.0) {
                        merge_companies(g, ui, rng, corp_id, target_id, 2, 0, time_secs);
                        target_id = 1600;
                    }
                }
            }
            target_id += 1;
            if target_id > 1600 {
                return;
            }
        }
    }
    if 1470 < corp_id && corp_id < 1501 && rng.rnd() * Ext::from_f64(25.0) > Ext::from_f64(1.0) {
        return;
    }
    if !(g.unused_credit_line > g.exchange_rate * Ext::from_f64(3000.0))
        && g.exchange_rate * Ext::from_f64(4000.0) >= g.t_bills[corp_id] + g.cash[corp_id]
    {
        return;
    }
    if 131 < corp_id && corp_id < 1470 {
        industry_id = cast::<i32, _>(g.industry_num[corp_id - 1]);
        if cast::<i32, _>(g.industry_num[corp_id + 1]) != industry_id {
            industry_id = 0;
        }
        if industry_id > 69 {
            industry_id = 0;
        }
        if industry_id > 2 {
            neighbour = true;
        }
    }
    if industry_id == 0 {
        industry_id = rng.rnd_range(69, 3);
    }
    if !(cast::<Ext, _>(g.ind_demand[industry_id]) >= g.exchange_rate * Ext::from_f64(100000.0)) {
        return;
    }
    g.industry_num[corp_id] = cast::<i16, _>(industry_id);
    let mut assets = to_long(rng.rnd() * Ext::from_f64(1000.0) + Ext::from_f64(2000.0));
    assets = to_long(g.exchange_rate * cast::<Ext, _>(assets));
    g.business_assets[corp_id] = cast::<Ext, _>(assets);
    g.cash[corp_id] = g.cash[corp_id] - cast::<Ext, _>(assets);
    g.growth_rate[corp_id] = 20;
    if neighbour {
        let ind = cast::<i32, _>(g.industry_num[corp_id]);
        g.news_msg = g.ent_symbol[corp_id].clone()
            + __content.string(10486)
            + &mcase(g.ind_name[ind].as_bytes(), CaseMode::Ansi)
            + __content.string(1547);
        post_news(g, ui, 0, 0);
    }
    if 1470 < corp_id && corp_id < 1501 && g.tax_loss_carry[corp_id] < Ext::from_f64(0.0) {
        g.tax_loss_carry[corp_id] = Ext::from_f64(0.0);
    }
}

/// Port of CapitalInfusion (0x4e51b1).
/// Investor `investor_id` (> 10, no human manager, non-negative tax-loss carry) puts 500..1500 (x
/// exchange rate, steps of 50) of cash into corp `target_id` (> 10, active, no fund, no human manager,
/// industry > 2, no convertible bonds) when the target needs it: a shell with tax losses, an operating
/// corp short of cash and growing fast or with tax losses over 300, but not a big one (assets over
/// 10000) without tax losses. Operating investors keep 20% of their business assets liquid. Raises the
/// target's cap by 0.8% of the amount, adds it to the investor's cost basis, posts the news and checks
/// cross-ownership.
pub fn capital_infusion(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    investor_id: i32,
    target_id: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("Infusion", …) (error path not ported)
    if 11 > target_id {
        return;
    }
    if g.industry_num[target_id] == 71 {
        return;
    }
    if g.entity_status[target_id] != 1 {
        return;
    }
    if get_managing_player(g, target_id) > 0 {
        return;
    }
    if get_managing_player(g, investor_id) > 0 {
        return;
    }
    let investor = investor_id;
    if investor < 11 {
        return;
    }
    if cast::<Ext, _>(g.bonds_issued[target_id]) > Ext::from_f64(0.0)
        && instr(1, g.bond_issue_info[target_id].as_bytes(), b"cv") > 0
    {
        return;
    }
    if g.industry_num[target_id] < 3 {
        return;
    }
    if !(g.tax_loss_carry[investor] >= Ext::from_f64(0.0)) {
        return;
    }
    if g.industry_num[target_id] == 70 && !(g.tax_loss_carry[target_id] < Ext::from_f64(0.0)) {
        return;
    }
    if g.industry_num[target_id] != 70 && g.industry_num[target_id] > 2 {
        let liquid = g.t_bills[target_id] + g.cash[target_id];
        let check_loss = if g.business_assets[target_id] * sf(0.2f32) < liquid {
            true
        } else {
            g.growth_rate[target_id] < 12
        };
        if check_loss && !(g.tax_loss_carry[target_id] <= g.exchange_rate * -Ext::from_f64(300.0)) {
            return;
        }
    }
    if g.business_assets[target_id] > g.exchange_rate * Ext::from_f64(10000.0)
        && !(g.tax_loss_carry[target_id] < Ext::from_f64(0.0))
    {
        return;
    }
    let amount = to_quad(g.exchange_rate * (int(rng.rnd() * Ext::from_f64(21.0)) * Ext::from_f64(50.0) + Ext::from_f64(500.0)));
    let mut avail_funds = to_quad(g.t_bills[investor] + g.cash[investor]);
    if g.industry_num[investor] > 2 && g.industry_num[investor] < 70 {
        avail_funds = to_quad(
            (g.t_bills[investor] + g.cash[investor]) - g.business_assets[investor] * sf(0.2f32),
        );
    }
    if !(cast::<Ext, _>(avail_funds) >= cast::<Ext, _>(amount)) {
        return;
    }
    g.cash[target_id] = cast::<Ext, _>(amount) + g.cash[target_id];
    g.cash[investor] = g.cash[investor] - cast::<Ext, _>(amount);
    g.market_cap[target_id] = cast::<Ext, _>(amount) * sf(0.008f32) + g.market_cap[target_id];
    let hold_slot = find_holding_slot(g, investor, target_id);
    g.hold_stock_basis[(hold_slot, investor)] =
        cast::<Ext, _>(amount) + g.hold_stock_basis[(hold_slot, investor)];
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    let amt_text = trim(&format(cast::<Ext, _>(amount), b"* ###,"));
    g.news_msg = g.ent_symbol[investor].clone()
        + __content.string(10487)
        + &g.currency_symbol
        + &amt_text
        + " "
        + &g.million_prefix_lc
        + __content.string(10488)
        + &g.ent_symbol[target_id];
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) || g.cur_player == g.corp_controller[target_id] {
        g.news_msg = g.ent_symbol[target_id].clone()
            + __content.string(10489)
            + &g.currency_symbol
            + &amt_text
            + " "
            + &g.million_prefix_lc
            + __content.string(8291)
            + &g.ent_symbol[investor];
    }
    post_news(g, ui, target_id, investor);
    check_cross_ownership(g, ui, rng, target_id, 1);
}

/// Port of AiCashRichTakeover (0x4fb4da).
/// Unless the market is overvalued (`MarketValuation` > 1.1): picks a target (`TakeoverCandidateId`,
/// else one time in 11 a random corp 86..1470; the candidate is cleared) — active, no fund, rated 2+,
/// shells only one time in 5, with a public float of 10%+ (20%+ four times in five) — and a buyer: one
/// time in 4 a corp near the target's id or `CashRichCorp`, else a computer-controlled holder of the
/// target, sized against 27x (other industry: 13.5x, 10 points) the target's cap (funds: 10x, no debt);
/// otherwise the first of 50 random corps 86..1580 (the computer players' first holdings come first)
/// with liquid funds over 40x the cap, normally an insurer or shell (30 points). Banks never buy;
/// insurers rarely buy insurers. The buyer must not already hold or be tied to the target's holders and
/// a computer player must be under its corp limit. The stake is scaled by the buyer's cash, the float
/// and chance (insurers stay at 27 or below, funds buy 1), then `BuyStock(buyer, target)` with
/// `TmpExt1` = points and `TmpExt2` = 2 (0 for the current player's corp).
///
/// Bug (reproduced, BUGS.md): the cash ratio is `Cash + TBills / cashNeeded` instead of
/// `(Cash + TBills) / cashNeeded`.
///
/// `time_secs`: seconds of `TIME$` (for AskAutopilotPermission and BuyStock).
pub fn ai_cash_rich_takeover(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut target_id: i32 = 0;
    let mut buyer_id: i32;
    let mut pct_to_buy: i64 = 0;
    let mut cash_needed: Ext = Ext::from_f64(0.0);
    let mut skip_pct_adjust = 0;
    if !(1.1f32 >= g.market_valuation) {
        return;
    }
    if g.takeover_candidate_id > 10 {
        target_id = g.takeover_candidate_id;
    } else if rng.rnd() * Ext::from_f64(11.0) > Ext::from_f64(10.0) {
        target_id = rng.rnd_range(1470, 86);
    }
    g.takeover_candidate_id = 0;
    if target_id == 0 {
        return;
    }
    if g.entity_status[target_id] != 1 {
        return;
    }
    if g.industry_num[target_id] == 71 {
        return;
    }
    if !(2 <= g.credit_rating_num[target_id]) {
        return;
    }
    if g.industry_num[target_id] == 70 && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) {
        return;
    }
    update_corp_control(g, ui, rng, target_id);
    if g.public_pct < 10 {
        return;
    }
    if g.public_pct < 20 && g.rand_seed % 5 > 0 {
        return;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    buyer_id = 0;
    if rng.rnd() * Ext::from_f64(4.0) < Ext::from_f64(1.0) {
        let candidate_id = to_long(-Ext::from_f64(9.0) + rng.rnd() * Ext::from_f64(20.0));
        buyer_id = candidate_id + target_id;
        if g.cash_rich_corp > 0 {
            let c = cast::<i32, _>(g.cash_rich_corp);
            if !(g.exchange_rate * Ext::from_f64(12000.0) <= g.t_bills[c] + g.cash[c]) {
                g.cash_rich_corp = 0;
            }
        }
        if g.rand_seed % 2 == 1 && g.cash_rich_corp > 85 {
            buyer_id = cast::<i32, _>(g.cash_rich_corp);
        }
        if buyer_id < 86 || buyer_id > 1600 {
            buyer_id = 0;
        } else if g.entity_status[buyer_id] != 1 {
            buyer_id = 0;
        }
        if target_id == buyer_id {
            buyer_id = 0;
        }
        if buyer_id > 0 && get_managing_player(g, buyer_id) > 0 {
            buyer_id = 0;
        }
        if buyer_id == 0 {
            let mut holder_id = 86;
            while holder_id <= 1600 {
                if g.entity_status[holder_id] == 1 && !(rng.rnd() * Ext::from_f64(30.0) > Ext::from_f64(1.0)) {
                    let n = cast::<i32, _>(g.hold_stock_id[(0, holder_id)]);
                    let mut idx = 1;
                    while idx <= n {
                        if cast::<i32, _>(g.hold_stock_id[(idx, holder_id)]) == target_id
                            && 11 <= g.corp_controller[holder_id]
                            && !(g.hold_stock_pct[(idx, holder_id)] > 19
                                && holder_id > 130
                                && rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0))
                        {
                            let hold_pct = g.hold_stock_pct[(idx, holder_id)];
                            if !(g.industry_num[holder_id] == 71 && hold_pct > 9) {
                                buyer_id = holder_id;
                                holder_id = 1600;
                                break;
                            }
                        }
                        idx += 1;
                    }
                }
                holder_id += 1;
            }
        }
        let mut fund_buyer = false;
        if buyer_id > 10 && g.industry_num[buyer_id] == 71 {
            fund_buyer = true;
            cash_needed = g.market_cap[target_id] * Ext::from_f64(10.0);
            if cash_needed > g.t_bills[buyer_id] + g.cash[buyer_id]
                || g.bank_debt[buyer_id] > Ext::from_f64(0.0)
                || cast::<Ext, _>(g.bonds_issued[buyer_id]) > Ext::from_f64(0.0)
            {
                buyer_id = 0;
            }
        }
        if !fund_buyer {
            if buyer_id > 10 && g.industry_num[buyer_id] == 1 {
                return;
            }
            if buyer_id > 0 {
                cash_needed = g.market_cap[target_id] * Ext::from_f64(27.0);
                if g.industry_num[buyer_id] != g.industry_num[target_id] {
                    cash_needed = cash_needed * sf(0.5f32);
                    pct_to_buy = 10;
                }
                if !(cash_needed <= g.t_bills[buyer_id] + g.cash[buyer_id]) {
                    buyer_id = 0;
                }
            }
            if buyer_id > 0 {
                if pct_to_buy == 0 {
                    pct_to_buy = 20;
                }
                if cast::<i32, _>(g.cash_rich_corp) != buyer_id {
                    skip_pct_adjust = 1;
                }
            }
        }
    }
    if !(buyer_id > 10) {
        cash_needed = g.market_cap[target_id] * Ext::from_f64(40.0);
        let mut idx = 1;
        loop {
            let mut candidate_id = rng.rnd_range(1580, 86);
            if g.num_players >= idx {
                candidate_id = 0;
                if g.is_computer_player[idx] == 1 && g.player_name[idx] != "...OUT..." {
                    let player_corp_id = cast::<i32, _>(g.hold_stock_id[(1, idx)]);
                    if player_corp_id > 10
                        && idx == g.corp_controller[player_corp_id]
                        && !(g.exchange_rate * Ext::from_f64(1500.0)
                            >= g.t_bills[player_corp_id] + g.cash[player_corp_id])
                    {
                        candidate_id = player_corp_id;
                    }
                }
            }
            if candidate_id != 0
                && g.entity_status[candidate_id] == 1
                && target_id != candidate_id
                && cash_needed <= g.t_bills[candidate_id] + g.cash[candidate_id]
                && (g.industry_num[candidate_id] == 2
                    || g.industry_num[candidate_id] == 70
                    || idx >= 40)
                && !(cash_needed >= g.t_bills[candidate_id] + g.cash[candidate_id])
                && get_managing_player(g, candidate_id) == 0
            {
                buyer_id = candidate_id;
                idx = 50;
            }
            idx += 1;
            if idx > 50 {
                break;
            }
        }
        if buyer_id > 85 && target_id > 85 {
            if rng.rnd() * Ext::from_f64(20.0) > Ext::from_f64(1.0)
                && g.industry_num[buyer_id] == 2
                && g.industry_num[target_id] == 2
            {
                buyer_id = 0;
            }
            if buyer_id > 85
                && rng.rnd() * Ext::from_f64(9.0) > Ext::from_f64(1.0)
                && g.industry_num[buyer_id] != 71
                && g.industry_num[target_id] == 2
            {
                buyer_id = 0;
            }
        }
        if buyer_id == 0 {
            return;
        }
        pct_to_buy = 30;
    }
    let mut can_buy = 0;
    let mut existing_pct = 0;
    let slot = find_holding_slot(g, buyer_id, target_id);
    if slot > 0 {
        can_buy = 1;
        existing_pct = cast::<i32, _>(g.hold_stock_pct[(slot, buyer_id)]);
    }
    for idx in 1..=15 {
        let candidate_id = cast::<i32, _>(g.hold_stock_id[(idx, target_id)]);
        if candidate_id != 0 {
            if buyer_id == candidate_id {
                can_buy = 0;
            }
            if g.corp_controller[buyer_id] == g.corp_controller[candidate_id] {
                can_buy = 0;
            }
            if buyer_id == g.corp_controller[candidate_id] {
                can_buy = 0;
            }
        }
    }
    if 0 < g.corp_controller[buyer_id]
        && g.num_players >= g.corp_controller[buyer_id]
        && get_controller_type(g, buyer_id) == 2
    {
        let ctrl = g.corp_controller[buyer_id];
        if ctrl != g.corp_controller[target_id]
            && !(g.cpu_max_corps > g.player_corps_controlled[ctrl])
        {
            can_buy = 0;
        }
    }
    if can_buy < 1 {
        return;
    }
    if skip_pct_adjust == 0 {
        // sic: Cash + TBills / cashNeeded
        let cash_ratio = to_quad(g.cash[buyer_id] + g.t_bills[buyer_id] / cash_needed);
        if rng.rnd() * Ext::from_f64(5.0) < Ext::from_f64(1.0) && cast::<Ext, _>(pct_to_buy) == Ext::from_f64(20.0) {
            pct_to_buy = to_quad(cast::<Ext, _>(cash_ratio) * cast::<Ext, _>(pct_to_buy));
        }
        if cast::<Ext, _>(pct_to_buy) > cast::<Ext, _>(g.public_pct) {
            pct_to_buy = cast::<i64, _>(g.public_pct);
        }
        pct_to_buy = to_quad((rng.rnd() * sf(0.3f32) + sf(0.7f32)) * cast::<Ext, _>(pct_to_buy));
    }
    if cast::<Ext, _>(pct_to_buy) > cast::<Ext, _>(g.public_pct) {
        pct_to_buy = cast::<i64, _>(g.public_pct);
    }
    if pct_to_buy == 0 {
        return;
    }
    if !(g.market_cap[target_id] * Ext::from_f64(135.0) >= g.t_bills[buyer_id] + g.cash[buyer_id]) {
        pct_to_buy = cast::<i64, _>(g.public_pct);
    }
    if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0) && g.industry_num[buyer_id] == 2 && cast::<Ext, _>(pct_to_buy) > Ext::from_f64(27.0) {
        while cast::<Ext, _>(pct_to_buy) > Ext::from_f64(27.0) {
            pct_to_buy = to_quad(cast::<Ext, _>(pct_to_buy) - rng.rnd() * Ext::from_f64(7.0));
        }
    }
    let pct_arg = to_integer(cast::<Ext, _>(pct_to_buy));
    if buyer_id > 10 && get_controller_type(g, buyer_id) == 1 {
        let permitted = super::asset_sales::ask_autopilot_permission(
            g,
            ui,
            rng,
            buyer_id,
            target_id,
            pct_arg,
            existing_pct,
            0,
            time_secs,
        );
        if permitted == 0 {
            return;
        }
    }
    if buyer_id > 10 && g.industry_num[buyer_id] == 71 {
        pct_to_buy = 1;
    }
    if !(target_id >= 11) {
        return;
    }
    if target_id > 10 && buyer_id > 10 && g.industry_num[target_id] == 71 {
        return;
    }
    if buyer_id > 1580 && buyer_id < 1586 && g.industry_num[buyer_id] == 71 {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(pct_to_buy);
    g.tmp_ext2 = Ext::from_f64(2.0);
    if g.cur_player == g.corp_controller[buyer_id] {
        g.tmp_ext2 = Ext::from_f64(0.0);
    }
    buy_stock(g, ui, rng, buyer_id, target_id, time_secs);
}

/// Port of ProcessNegativeCash (0x4f4a0f).
/// Unless the game has ended: every active entity 86..1600 with negative cash first sells T-bills,
/// then (operating corps) takes the deficit out of working capital when that exceeds 1.7x the deficit,
/// then draws on its credit line (`DrawCreditLineForCash`), recalculates its commodity margins and sells
/// T-bills again. If cash is still negative and it holds futures, the futures are liquidated
/// (`LiquidateFuturesPositions(entity, 1, 1)`, news, `ClockState = "STOP"` for the current player's
/// corp). The current player's corp first gets a margin-call MSGBOX and, with several positions, may
/// choose (YES) the contracts to close in the futures list (`ShowFuturesPositions(…, 4)`), repeated
/// until the deficit is gone.
/// The closing trade itself is `FuturesSellOrShort(2, …)` / `FuturesBuyOrCover(2, …)`.
pub fn process_negative_cash(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("NegCash", …) (error path not ported)
    if g.game_end_flag == "Y" {
        return;
    }
    let mut credit_drawn: Ext;
    let mut deficit: i64;
    for e in 86..=1600 {
        if g.entity_status[e] != 1 {
            continue;
        }
        if g.cash[e] < Ext::from_f64(0.0) && g.t_bills[e] > Ext::from_f64(0.0) {
            balance_cash_t_bills(g, e);
        }
        if !(g.cash[e] < Ext::from_f64(0.0)) {
            continue;
        }
        credit_drawn = -Ext::from_f64(1.0) * g.cash[e];
        if g.industry_num[e] > 2
            && g.industry_num[e] < 70
            && g.working_capital[e] > credit_drawn * sf(1.7f32)
        {
            g.working_capital[e] = g.working_capital[e] - credit_drawn;
            g.cash[e] = Ext::from_f64(0.0);
        }
        if !(g.cash[e] < Ext::from_f64(0.0)) {
            continue;
        }
        credit_drawn = draw_credit_line_for_cash(g, ui, rng, e);
        'retry: loop {
            if !(g.cash[e] < Ext::from_f64(0.0)) {
                break 'retry;
            }
            recalc_commod_margin_balances(g, ui, rng, e);
            if g.cash[e] < Ext::from_f64(0.0) && g.t_bills[e] > Ext::from_f64(0.0) {
                balance_cash_t_bills(g, e);
            }
            if !(g.cash[e] < Ext::from_f64(0.0)) {
                break 'retry;
            }
            if count_futures_positions(g, e, 0) == 0 {
                break 'retry;
            }
            deficit = to_quad(g.cash[e].abs());
            if g.cur_player == g.corp_controller[e] && cast::<Ext, _>(deficit) > Ext::from_f64(1.0) {
                calc_entity_financials(g, ui, rng, e);
                let cr = PbStr::from("\r");
                let mut msg_text = PbStr::from(__content.string(10490))
                    + &g.ent_name[e]
                    + " ("
                    + &g.ent_symbol[e]
                    + "):"
                    + &cr
                    + &cr
                    + __content.string(10491)
                    + &trim(g.currency_symbol.as_bytes())
                    + &trim(&format(cast::<Ext, _>(deficit), b"* ###,"))
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ". "
                    + &cr
                    + &cr
                    + __content.string(10492)
                    + __content.string(10493)
                    + __content.string(10494);
                if !(credit_drawn > Ext::from_f64(1.0)) {
                    msg_text = msg_text + ", ";
                } else {
                    msg_text = msg_text
                        + __content.string(10495)
                        + &trim(g.currency_symbol.as_bytes())
                        + &trim(&format(credit_drawn, b"* ###,"))
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix
                        + " from "
                        + &g.lender_bank_name
                        + "), ";
                }
                msg_text = msg_text
                    + __content.string(10496)
                    + &cr
                    + &cr;
                if 2 > g.credit_rating_num[e] {
                    msg_text = PbStr::from(
                        __content.string(10497),
                    ) + &g.ent_name[e]
                        + " ("
                        + &g.ent_symbol[e]
                        + "):"
                        + &cr
                        + &cr
                        + __content.string(10498)
                        + "\""
                        + "D"
                        + "\""
                        + __content.string(10499)
                        + __content.string(10500)
                        + __content.string(10501)
                        + &cr
                        + &cr;
                }
                msg_text = msg_text + __content.string(10502);
                ui.msgbox(
                    &msg_text,
                    8208,
                    &PbStr::from(__content.string(10503)),
                );
            }
            loop {
                let mut futures_count = count_futures_positions(g, e, 0);
                if futures_count > 0 {
                    recalc_commod_margin_balances(g, ui, rng, e);
                }
                if futures_count > 1 && g.cur_player == g.corp_controller[e] && cast::<Ext, _>(deficit) > Ext::from_f64(1.0)
                {
                    let cr = PbStr::from("\r");
                    let msg_text = g.ent_name[e].clone()
                        + __content.string(10504)
                        + &cr
                        + &cr
                        + __content.string(10505);
                    let answer = ui.msgbox(
                        &msg_text,
                        8228,
                        &PbStr::from(__content.string(10506)),
                    );
                    if answer == 7 {
                        futures_count = 1;
                    }
                }
                if futures_count == 1 || g.cur_player != g.corp_controller[e] {
                    let closed_count = liquidate_futures_positions(g, ui, rng, e, 1, 1);
                    if closed_count > 0 {
                        g.news_msg =
                            g.ent_symbol[e].clone() + __content.string(10471);
                    }
                    if closed_count > 1 {
                        g.news_msg = g.news_msg.clone() + "s";
                    }
                    post_news(g, ui, e, 0);
                    recalc_commod_margin_balances(g, ui, rng, e);
                    if g.cur_player == g.corp_controller[e] {
                        g.clock_state = PbStr::from("STOP");
                    }
                    break 'retry;
                }
                if futures_count <= 1 {
                    break 'retry;
                }
                let saved_active = g.active_entity;
                let saved_sel = g.sel_entity;
                g.active_entity = e;
                g.sel_entity = e;
                let futures_sel = show_futures_positions(g, ui, rng, g.sel_entity, 4);
                if futures_sel == 0 {
                    continue 'retry;
                }
                if futures_sel < 1 {
                    break 'retry;
                }
                if !(val(g.futures_recs[0].as_bytes()) >= cast::<Ext, _>(futures_sel)) {
                    break 'retry;
                }
                if val(parse_any(g.futures_recs[futures_sel].as_bytes(), b"|", 2).as_bytes())
                    != cast::<Ext, _>(g.active_entity)
                {
                    break 'retry;
                }
                let msg_text = g.futures_recs[futures_sel].clone();
                // (UI) DIALOG END hDlgTransMenu
                g.action_menu_open = 0;
                // (UI) DIALOG SHOW STATE hDlgMain, 0
                g.cur_commodity = trim(parse_any(msg_text.as_bytes(), b"|", 1).as_bytes());
                let active = g.active_entity;
                if val(parse_any(msg_text.as_bytes(), b"|", 3).as_bytes()) >= Ext::from_f64(0.0) {
                    futures_sell_or_short(g, ui, rng, 2, active, futures_sel);
                } else {
                    futures_buy_or_cover(g, ui, rng, 2, active, futures_sel);
                }
                // (UI) EnableMainControls; DIALOG SHOW STATE hDlgMain, 5
                recalc_commod_margin_balances(g, ui, rng, e);
                g.active_entity = saved_active;
                if g.active_entity > 0 {
                    g.active_entity_name = g.ent_name[g.active_entity].clone();
                }
                g.sel_entity = saved_sel;
                if g.sel_entity > 0 {
                    g.sel_corp_name = g.ent_name[g.sel_entity].clone();
                }
                if g.cash[e] > Ext::from_f64(0.0) {
                    g.clock_state = PbStr::from("STOP");
                    break 'retry;
                }
                let mut msg = g.ent_name[e].clone()
                    + __content.string(10507)
                    + &trim(g.currency_symbol.as_bytes())
                    + &trim(&format(g.cash[e].abs(), b"* ###,.00"))
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix;
                show_quick_message(g, ui, &mut msg);
            }
        }
    }
}
