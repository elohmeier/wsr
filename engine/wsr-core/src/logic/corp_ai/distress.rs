//! Corporate AI for weak or distressed companies and holding companies: bank bailouts
//! (`BankBailoutInjection`), asset purchases from the distressed corp (`AiBuyDistressedAssets`),
//! forced dividend cuts (`AiDividendCutsCovenants`), liquidating subsidiaries into their parents
//! (`AiLiquidateRandomSubsidiary`), boosting the leading player's rival (`AiBoostLeadersRival`), mergers
//! of holding companies (`AiHoldingCoMergerPlan`), small stakes (`AiManageSmallHoldings`) and loan sales
//! between computer-player banks (`AiBankBuysLoan`).
//!
//! The ON ERROR handlers of these routines only show `ShowRuntimeError` and leave the routine (not
//! ported: the port reads the arrays' dummy element instead, except where noted).

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

use super::spinoff::calc_asset_sale_price_pct;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::holdings::{find_holding_slot, get_largest_holding_id};
use crate::logic::newgame_helpers::{calc_loan_sale_price_pct, sell_loan_to_bank};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::restructure::{liquidate_sub_into_parent, merge_companies};
use crate::logic::trading::buy_stock;
use crate::logic::sim_entity::ai_corp::{capital_infusion, is_recent_offering};
use crate::logic::sim_entity::asset_sales::gen_asset_sale_description;
use crate::logic::sim_entity::bonds::{issue_stock, normalize_holding_pcts};
use crate::logic::sim_entity::dividends::ai_set_dividend;
use crate::logic::sim_entity::transactions::sell_stock_holding;
use crate::logic::valuation::{
    calc_dividend_yield, calc_industry_market_share, calc_industry_outlook, calc_working_capital_ratio,
};

/// Port of BankBailoutInjection (0x4b9ef1).
/// Up to 50 random picks of a bank 11..85 (each at most once): an active bank controlled by a corp,
/// rated 2..`BailoutMaxCreditRating` (at least 6). A bank wholly owned by a corp with liquid funds over
/// 6000 (x exchange rate) gets 3000 of capital from its parent (headline) and the routine ends. Else,
/// unless the bank had a recent public offering, it issues stock: a private placement of 5% (10% when
/// the investor is very liquid) bought by its corp-controlled largest holder for 4.75x (9.5x) its cap
/// when that holder has the funds and the float is at least 5%, otherwise a public offering of 10..25%.
/// The offering counts when the largest holder's stake is under 20% or changed: then the holdings are
/// normalised (placement), the bank is recorded in a free `RecentOfferings` slot (negative for a
/// placement) and `BailoutMaxCreditRating` drops by one; else the next bank is tried. After 50 failed
/// tries `BailoutMaxCreditRating` rises by one (max 10).
///
/// The locals `slot`, `issuePct` and `noPublic` keep their values across tries, as in the original.
pub fn bank_bailout_injection(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut tried = [0i32; 86];
    let mut slot: i32 = 0;
    let mut issue_pct: i32 = 0;
    let mut no_public: i32 = 0;
    let mut inject_amt: Ext;
    let mut issue_mode: i32;
    let mut investor_id: i32;
    if g.bailout_max_credit_rating < 6 {
        g.bailout_max_credit_rating = 6;
    }
    let mut tries_left = 50;
    loop {
        let bank_id = rng.rnd_range(85, 11);
        'attempt: {
            if tried[cast::<usize, _>(bank_id)] == 1 {
                break 'attempt;
            }
            tried[cast::<usize, _>(bank_id)] = 1;
            if g.entity_status[bank_id] != 1 {
                break 'attempt;
            }
            if !(10 < g.corp_controller[bank_id]) {
                break 'attempt;
            }
            calc_entity_financials(g, ui, rng, bank_id);
            let max_rating = g.bailout_max_credit_rating;
            if !(1 < g.credit_rating_num[bank_id]) || !(g.credit_rating_num[bank_id] <= max_rating) {
                break 'attempt;
            }
            update_corp_control(g, ui, rng, bank_id);
            let prev_largest_pct = g.largest_holder_pct;
            if g.public_pct == 0 {
                no_public = 1;
            }
            let wholly = g.wholly_owned_by;
            if 10 < g.corp_controller[bank_id] && 10 < wholly {
                investor_id = cast::<i32, _>(wholly);
                calc_entity_financials(g, ui, rng, investor_id);
                if g.exchange_rate * Ext::from_f64(6000.0) < g.liquid_funds {
                    slot = find_holding_slot(g, investor_id, bank_id);
                    g.hold_stock_basis[(slot, investor_id)] = g.hold_stock_basis[(slot, investor_id)] + Ext::from_f64(3000.0);
                    g.cash[investor_id] = g.cash[investor_id] - Ext::from_f64(3000.0);
                    g.cash[bank_id] = g.cash[bank_id] + Ext::from_f64(3000.0);
                    g.news_msg = g.ent_symbol[bank_id].clone()
                        + __content.string(2819)
                        + &g.ent_symbol[investor_id];
                    post_news(g, ui, bank_id, investor_id);
                    return;
                }
            }
            let recent_offer = is_recent_offering(g, bank_id);
            if recent_offer == 1 {
                break 'attempt;
            }
            issue_mode = 1;
            if 10 < g.largest_holder {
                let holder = cast::<i32, _>(g.largest_holder);
                if 10 < g.corp_controller[holder] && recent_offer == 0 {
                    investor_id = holder;
                    inject_amt = g.market_cap[bank_id] * Ext::from_f64(5.0) * sf(0.95f32);
                    calc_entity_financials(g, ui, rng, investor_id);
                    if inject_amt * sf(1.5f32) < g.liquid_funds {
                        for slot_idx in 1..=15 {
                            if cast::<i32, _>(g.hold_stock_id[(slot_idx, investor_id)]) == bank_id {
                                slot = slot_idx;
                            }
                        }
                        if g.public_pct < 5 {
                            slot = 0;
                        }
                        if 0 < slot {
                            issue_pct = 5;
                            if inject_amt * Ext::from_f64(3.0) < g.liquid_funds {
                                inject_amt = Ext::from_f64(2.0) * inject_amt;
                                issue_pct = 10;
                            }
                            issue_mode = 2;
                            g.cash[investor_id] = g.cash[investor_id] - inject_amt;
                            balance_cash_t_bills(g, investor_id);
                            if g.cash[investor_id] + g.t_bills[investor_id] < Ext::from_f64(0.0) {
                                let overdraft = g.cash[investor_id] * -Ext::from_f64(1.0);
                                g.bank_debt[investor_id] = g.bank_debt[investor_id] + overdraft;
                                g.cash[investor_id] = Ext::from_f64(0.0);
                                let lender = g.lender_id;
                                g.cash[lender] = g.cash[lender] - overdraft;
                            }
                            g.hold_stock_pct[(slot, investor_id)] =
                                (cast::<i16, _>(issue_pct)).wrapping_add(g.hold_stock_pct[(slot, investor_id)]);
                            g.hold_stock_basis[(slot, investor_id)] =
                                g.hold_stock_basis[(slot, investor_id)] + inject_amt;
                            g.shares_outstanding[bank_id] =
                                g.shares_outstanding[bank_id] * (cast::<Ext, _>(issue_pct) * sf(0.01f32) + Ext::from_f64(1.0));
                            g.market_cap[bank_id] = g.market_cap[bank_id] * (cast::<Ext, _>(issue_pct) * sf(0.01f32) + Ext::from_f64(1.0));
                        }
                    }
                }
            }
            if issue_pct == 0 {
                issue_pct = to_long(Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(10.0));
                issue_mode = 1;
            }
            g.tmp_ext1 = cast::<Ext, _>(issue_pct);
            issue_stock(g, ui, rng, bank_id, issue_mode);
            if !(g.largest_holder_pct < 20) {
                update_corp_control(g, ui, rng, bank_id);
                if prev_largest_pct == g.largest_holder_pct {
                    break 'attempt;
                }
            }
            if issue_mode == 2 {
                normalize_holding_pcts(g, ui, rng, bank_id, no_public);
            }
            let k = rng.rnd_range(10, 1);
            if g.recent_offerings[k] == 0 {
                g.recent_offerings[k] = bank_id;
                if issue_mode == 2 {
                    g.recent_offerings[k] = bank_id.wrapping_neg();
                }
            }
            g.bailout_max_credit_rating -= 1;
            return;
        }
        tries_left -= 1;
        if tries_left == 0 {
            if g.bailout_max_credit_rating < 10 {
                g.bailout_max_credit_rating += 1;
            }
            return;
        }
    }
}

/// Port of AiBuyDistressedAssets (0x4b26e0).
/// The operating corp `SimCoCount` (131..1600, industry below 70, no managing player) buys, one time in
/// ten, a slice (10..25%, at least 50..85 [runtime resource wording] `DistressedCorp` when that corp is
/// active (else `DistressedCorp` is cleared), has no managing player, is in the same industry, has assets
/// of at least 200 and a credit rating up to 6, and the buyer's cash and T-bills cover twice the amount.
/// The price is `100 + CalcAssetSalePricePct` percent (max 150%) of the assets: the premium goes to the
/// buyer's goodwill (negative goodwill to its extraordinary P&L), the seller gets 90% and books the gain;
/// working capital moves in proportion (limited by the buyer's cash). Clears `DistressedCorp`, posts a
/// pending `NewsMsg` first, then a "sells … to …" headline.
pub fn ai_buy_distressed_assets(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported); errTag strings have no effect
    let buyer_id = g.sim_co_count;
    if buyer_id < 131 || buyer_id > 1600 {
        return;
    }
    if !(g.industry_num[buyer_id] <= 69) {
        return;
    }
    if get_managing_player(g, buyer_id) > 0 {
        return;
    }
    if g.distressed_corp < 131 || g.sim_co_count == g.distressed_corp {
        return;
    }
    if rng.rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0) {
        return;
    }
    if g.entity_status[g.distressed_corp] != 1 {
        g.distressed_corp = 0;
        return;
    }
    let seller_id = g.distressed_corp;
    if get_managing_player(g, seller_id) > 0 {
        return;
    }
    if g.industry_num[seller_id] != g.industry_num[buyer_id] {
        return;
    }
    if !(g.business_assets[seller_id] >= Ext::from_f64(200.0)) {
        return;
    }
    calc_entity_financials(g, ui, rng, seller_id);
    if !(g.credit_rating_num[seller_id] <= 6) {
        return;
    }
    let mut asset_amt = g.business_assets[seller_id] * sf(0.1f32) + sf(0.15f32) * rng.rnd() * g.business_assets[seller_id];
    if !(asset_amt >= Ext::from_f64(75.0)) {
        asset_amt = Ext::from_f64(35.0) * rng.rnd() + Ext::from_f64(50.0);
    }
    if !(asset_amt * Ext::from_f64(2.0) <= g.cash[buyer_id] + g.t_bills[buyer_id]) {
        return;
    }
    let sale_price_pct: i32 = to_long(calc_asset_sale_price_pct(g, rng, seller_id));
    let mut price_pct: i64 = to_quad(cast::<Ext, _>(100 + sale_price_pct));
    if price_pct > 150 {
        price_pct = 150;
    }
    g.business_assets[buyer_id] = asset_amt + g.business_assets[buyer_id];
    let goodwill_add = sf(0.01f32) * cast::<Ext, _>(sale_price_pct) * asset_amt;
    g.goodwill[buyer_id] = goodwill_add + g.goodwill[buyer_id];
    if g.goodwill[buyer_id] < Ext::from_f64(0.0) {
        g.extraordinary_pl[buyer_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[buyer_id]) - g.goodwill[buyer_id]);
    }
    let seller_proceeds = sf(0.01f32) * cast::<Ext, _>(price_pct) * asset_amt * sf(0.9f32);
    g.cash[buyer_id] = g.cash[buyer_id] - sf(0.01f32) * cast::<Ext, _>(price_pct) * asset_amt;
    g.cash[seller_id] = seller_proceeds + g.cash[seller_id];
    let seller_gain = seller_proceeds - asset_amt;
    g.extraordinary_pl[seller_id] = to_quad(seller_gain + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
    let mut wc_transfer = asset_amt * calc_working_capital_ratio(g, seller_id);
    g.cash[seller_id] = wc_transfer + g.cash[seller_id];
    g.working_capital[seller_id] = g.working_capital[seller_id] - wc_transfer;
    if wc_transfer > g.cash[buyer_id] {
        wc_transfer = g.cash[buyer_id];
    }
    g.cash[buyer_id] = g.cash[buyer_id] - wc_transfer;
    g.working_capital[buyer_id] = wc_transfer + g.working_capital[buyer_id];
    g.distressed_corp = 0;
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.asset_sale_desc = PbStr::new();
    gen_asset_sale_description(g, rng, seller_id);
    g.news_msg = g.ent_symbol[seller_id].clone() + " sells " + &g.asset_sale_desc + " to " + &g.ent_symbol[buyer_id];
    let mut price_text = PbStr::from(".");
    let proceeds_rounded: i64 = to_quad(seller_proceeds);
    if len(g.news_msg.as_bytes()) < 48 {
        price_text = PbStr::from(" for ")
            + &trim(g.currency_symbol.as_bytes())
            + &trim(&format(cast::<Ext, _>(proceeds_rounded), b"* ###,"))
            + " "
            + &g.million_prefix_lc
            + "illion.";
    }
    g.news_msg = g.news_msg.clone() + &price_text;
    if proceeds_rounded != 0 && !g.asset_sale_desc.is_empty() {
        post_news(g, ui, buyer_id, seller_id);
        return;
    }
    g.news_msg = PbStr::new();
}

/// Port of AiDividendCutsCovenants (0x4bb0e6).
/// Scans a third of the corps (11..510; 511..1020 when `SimCoCount` = 1240; 1021..1600 when it is 220):
/// every active non-fund corp paying a dividend with a yield of 10% or more (and not a cash-rich corp,
/// over 100000 x exchange rate, rated 9+) gets `AiSetDividend` with the projected earnings from field 3
/// of its `EntMiscData` (when longer than 20 characters; else the previous text is reused), net worth 0
/// and its managing player (none when rated below 5 with a yield over 6% and two years of falling
/// earnings). A cut of a company controlled by the current player gets a headline ("reduces" / "ends" /
/// "eliminates") naming debt covenants or the bank/insurance regulators, or "to conserve cash", dropped
/// when a similar one is among the recent headlines.
///
/// Bugs reproduced: the net worth passed is 0, so `AiSetDividend` eliminates the dividend of corps rated
/// below 6 (BUGS.md #199); the projected-earnings text is kept from the previous corp (#200).
pub fn ai_dividend_cuts_covenants(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut first_corp = 11;
    let mut last_corp = 510;
    if g.sim_co_count == 1240 {
        first_corp = 511;
        last_corp = 1020;
    }
    if g.sim_co_count == 220 {
        first_corp = 1021;
        last_corp = 1600;
    }
    // locals kept across corps (zeroed at entry)
    let mut reason_text = PbStr::new();
    let mut verb_text: PbStr;
    let mut managing_player: i32;
    for corp_id in first_corp..=last_corp {
        if g.entity_status[corp_id] != 1 {
            continue;
        }
        if g.dividend_per_share[corp_id] == Ext::from_f64(0.0) {
            continue;
        }
        if g.industry_num[corp_id] == 71 {
            continue;
        }
        let div_yield = calc_dividend_yield(g, corp_id);
        if !(Ext::from_f64(10.0) <= div_yield) {
            continue;
        }
        calc_entity_financials(g, ui, rng, corp_id);
        if g.exchange_rate * Ext::from_f64(100000.0) < g.cash[corp_id] + g.t_bills[corp_id] {
            if 8 < g.credit_rating_num[corp_id] {
                continue;
            }
        }
        if 20 < len(g.ent_misc_data[corp_id].as_bytes()) {
            reason_text = parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 3);
        }
        let target_div = val(reason_text.as_bytes());
        managing_player = get_managing_player(g, corp_id);
        if g.credit_rating_num[corp_id] < 5 && Ext::from_f64(6.0) < div_yield {
            if g.earnings_yr2_ago[corp_id] < g.earnings_yr3_ago[corp_id]
                && g.earnings_y1[corp_id] < g.earnings_yr2_ago[corp_id]
            {
                managing_player = 0;
            }
        }
        // the net-worth argument is an EXT local that is never assigned (0)
        let div_result = ai_set_dividend(g, rng, corp_id, target_div, Ext::from_f64(0.0), managing_player);
        if !(div_result < 0) {
            continue;
        }
        reason_text = PbStr::new();
        if g.credit_rating_num[corp_id] < 6 {
            reason_text = PbStr::from(__content.string(2820));
        }
        let reg_text = PbStr::from(__content.string(2821));
        if g.industry_num[corp_id] == 1 {
            reason_text = PbStr::from("banking") + &reg_text;
        }
        if g.industry_num[corp_id] == 2 {
            reason_text = PbStr::from(__content.string(2822)) + &reg_text;
        }
        verb_text = PbStr::from(__content.string(2823));
        if g.dividend_per_share[corp_id] == Ext::from_f64(0.0) {
            verb_text = PbStr::from(" ends");
            if 2 < g.industry_num[corp_id] {
                verb_text = PbStr::from(__content.string(2824));
            }
        }
        if g.cur_player != g.corp_controller[corp_id] {
            continue;
        }
        g.news_msg = g.ent_symbol[corp_id].clone() + &verb_text + __content.string(2825);
        if Ext::from_f64(10.0) <= g.bank_debt[corp_id] || g.credit_rating_num[corp_id] < 6 {
            if is_in_recent_headlines(g, __content.bytes(2328)) <= 0 && is_in_recent_headlines(g, __content.bytes(2826)) <= 0 {
                if !reason_text.is_empty() {
                    g.news_msg = g.news_msg.clone() + __content.string(2827) + &reason_text;
                }
            } else {
                g.news_msg = PbStr::new();
            }
        } else {
            g.news_msg = g.news_msg.clone() + __content.string(2828);
            if 0 < is_in_recent_headlines(g, __content.bytes(2829)) {
                g.news_msg = PbStr::new();
            }
        }
        if !g.news_msg.is_empty() {
            post_news(g, ui, corp_id, 0);
        }
    }
}

/// Port of AiLiquidateRandomSubsidiary (0x4b30f8).
/// A random active non-fund corp 131..1600 with holdings, not human-controlled, picks a random holding
/// slot: a 100% subsidiary in its own industry (or a holding company, industry 70) qualifies. A holding
/// company without such a pick takes the last wholly owned holding company 131+ (any 11+ one stops the
/// scan only from 131) whose tax-loss carry is not below -200 (x exchange rate). Operating subsidiaries
/// (industry 3+) of the same industry, holding companies under a holding company, and (one time in 37,
/// by `RandSeed`) an operating sub of a holding company are liquidated into the parent
/// (`LiquidateSubIntoParent`, mode 2) — except, eleven times in twelve, a sub with a large tax-loss
/// carry when either is a holding company. `time_secs`: seconds of `TIME$` (for the liquidation).
pub fn ai_liquidate_random_subsidiary(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError("RandLiq") (error path not ported)
    let liq_mode = 2;
    let parent_id = rng.rnd_range(1600, 131);
    if g.industry_num[parent_id] == 71 {
        return;
    }
    if g.entity_status[parent_id] != 1 {
        return;
    }
    if g.hold_stock_id[(0, parent_id)] == 0 {
        return;
    }
    if get_controller_type(g, parent_id) == 1 {
        return;
    }
    let parent_industry = cast::<i32, _>(g.industry_num[parent_id]);
    let slot = rng.rnd_range(15, 1);
    let mut sub_id = cast::<i32, _>(g.hold_stock_id[(slot, parent_id)]);
    if g.hold_stock_pct[(slot, parent_id)] != 100 {
        sub_id = 0;
    }
    if 0 < sub_id && parent_industry != 70 {
        if cast::<i32, _>(g.industry_num[sub_id]) != parent_industry && g.industry_num[sub_id] != 70 {
            sub_id = 0;
        }
    }
    if sub_id == 0 && parent_industry == 70 {
        let mut slot_idx = 1;
        while slot_idx < 16 {
            if g.hold_stock_pct[(slot_idx, parent_id)] == 100 {
                sub_id = cast::<i32, _>(g.hold_stock_id[(slot_idx, parent_id)]);
                if 10 < sub_id && g.industry_num[sub_id] != 70 {
                    sub_id = 0;
                }
                if sub_id != 0 {
                    if -Ext::from_f64(200.0) * g.exchange_rate <= g.tax_loss_carry[sub_id] {
                        if 130 < sub_id {
                            slot_idx = 15;
                        }
                    } else {
                        sub_id = 0;
                    }
                }
            }
            slot_idx += 1;
        }
    }
    if sub_id == 0 {
        return;
    }
    if g.industry_num[sub_id] < 3 {
        return;
    }
    if g.industry_num[sub_id] != g.industry_num[parent_id] {
        let both_holding = g.industry_num[parent_id] == 70 && g.industry_num[sub_id] == 70;
        if !both_holding {
            if g.industry_num[parent_id] != 70 && g.industry_num[sub_id] != 70 {
                return;
            }
            if g.industry_num[parent_id] == 70 && g.rand_seed % 37 != 7 {
                return;
            }
        }
    }
    if g.tax_loss_carry[sub_id] < g.exchange_rate * -Ext::from_f64(200.0) {
        if Ext::from_f64(12.0) * rng.rnd() > Ext::from_f64(1.0) {
            if g.industry_num[parent_id] == 70 || g.industry_num[sub_id] == 70 {
                return;
            }
        }
    }
    g.tmp_ext1 = cast::<Ext, _>(liq_mode);
    liquidate_sub_into_parent(g, ui, rng, parent_id, sub_id, time_secs);
}

/// Port of AiBoostLeadersRival (0x494506).
/// At difficulty 2+ (level 2: one time in three), at most once a year: a random human player is picked;
/// unless computer players exist, the target becomes the player with the highest net worth (when that
/// is a computer player, nothing happens). The target's largest holding (through a holding company or
/// insurer: that one's largest operating holding) determines an industry; its outlook leader
/// (`CalcIndustryOutlook`) — a corp-controlled rival other than the holding, in an industry with a
/// market share under 52% and an outlook above 7 — gets +1..20 growth (when under 20% and rated 8+) and,
/// when short of cash (under 25% of its assets, rated below 7), a public offering of 20% (5% for large
/// caps; none above 100000 x exchange rate), which sets `LastRivalOfferingYear`.
///
/// With computer players the target stays the random human, not the leader (BUGS.md #202).
pub fn ai_boost_leaders_rival(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if g.difficulty_level < 2 {
        return;
    }
    if g.difficulty_level == 2 && rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
        return;
    }
    if cast::<i32, _>(g.last_rival_offering_year) == g.cur_year {
        return;
    }
    let mut target_id = rng.rnd_range(g.num_players, 1);
    if g.is_computer_player[target_id] == 1 {
        return;
    }
    let mut leader_id = 1;
    let mut best_net_worth: Ext = Ext::from_f64(0.0);
    let last = g.num_players;
    for player_id in 1..=last {
        if g.player_name[player_id] == "...OUT..." {
            continue;
        }
        calc_entity_financials(g, ui, rng, player_id);
        if best_net_worth < g.net_worth {
            best_net_worth = g.net_worth;
            leader_id = player_id;
        }
    }
    if g.has_cpu_players == "Y" && 0 < leader_id {
        if g.is_computer_player[leader_id] == 1 {
            return;
        }
    } else {
        target_id = leader_id;
    }
    g.last_rival_offering_year = 0;
    let mut holding_id = cast::<i16, _>(get_largest_holding_id(g, target_id));
    if !(10 < holding_id) {
        return;
    }
    let mut industry_id: i16 = g.industry_num[cast::<i32, _>(holding_id)];
    if industry_id == 70 || industry_id == 2 {
        target_id = cast::<i32, _>(holding_id);
        holding_id = cast::<i16, _>(get_largest_holding_id(g, target_id));
        if holding_id < 1 {
            industry_id = 0;
        } else {
            industry_id = g.industry_num[cast::<i32, _>(holding_id)];
        }
        if industry_id < 3 || industry_id == 70 {
            holding_id = 0;
        }
    } else if industry_id < 2 || industry_id == 71 {
        holding_id = 0;
    }
    if !(10 < holding_id) {
        return;
    }
    let rival_id = calc_industry_outlook(g, rng, industry_id);
    let outlook = cast::<f32, _>(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    if rival_id == 0 || cast::<i32, _>(holding_id) == rival_id || !(10 < g.corp_controller[rival_id]) {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(rival_id);
    let market_share = calc_industry_market_share(g, industry_id);
    if market_share > 51 {
        return;
    }
    calc_entity_financials(g, ui, rng, rival_id);
    if !(Ext::from_f64(7.0) < cast::<Ext, _>(outlook)) {
        return;
    }
    if g.growth_rate[rival_id] < 20 && 7 < g.credit_rating_num[rival_id] {
        let add = rng.rnd_range(20, 1);
        g.growth_rate[rival_id] = (cast::<i16, _>(add)).wrapping_add(g.growth_rate[rival_id]);
    }
    if !(g.cash[rival_id] + g.t_bills[rival_id] < g.business_assets[rival_id] * sf(0.25f32)) {
        return;
    }
    if !(g.credit_rating_num[rival_id] < 7) {
        return;
    }
    g.tmp_ext1 = Ext::from_f64(20.0);
    if g.exchange_rate * Ext::from_f64(100000.0) < Ext::from_f64(20.0) * g.market_cap[rival_id] {
        g.tmp_ext1 = Ext::from_f64(5.0);
    }
    if g.exchange_rate * Ext::from_f64(100000.0) < g.tmp_ext1 * g.market_cap[rival_id] {
        g.tmp_ext1 = Ext::from_f64(0.0);
    } else {
        issue_stock(g, ui, rng, rival_id, 1);
        g.last_rival_offering_year = cast::<i16, _>(g.cur_year);
    }
}

/// Port of AiHoldingCoMergerPlan (0x4baad0).
/// Pairs two self-controlled holding companies (industry 70) and merges them. While no pair is set, the
/// corp `SimCoCount` (131+, industry 70, controls itself, rated 3+) becomes the acquirer (when fewer than
/// 15 holdings) or else the target (when its cap is under 50 x exchange rate). With a pair, one time in
/// twenty: the acquirer must be corp-controlled and one of the two must hold the other (they are
/// swapped so that the acquirer is the holder; the pair is dropped when neither holds the other); the
/// larger one (with room for holdings) or the profitable one next to a large tax-loss carry becomes the
/// acquirer; the target must control itself. `MergeCompanies(acquirer, target, 2, 0)`; when the target
/// ends up wholly owned by the acquirer it is either recapitalised (`CapitalInfusion`, for a tax-loss
/// carry over 100x its cap, difficulty below 3) or liquidated into the acquirer (`LiquidateSubIntoParent`
/// mode 2). The pair is then cleared. `time_secs`: seconds of `TIME$` (for the merger / liquidation).
pub fn ai_holding_co_merger_plan(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if 0 < g.ai_merger_acquirer_id && 0 < g.ai_merger_target_id {
        if rng.rnd() * Ext::from_f64(20.0) > Ext::from_f64(1.0) {
            return;
        }
    }
    if g.ai_merger_acquirer_id < 1 || g.ai_merger_target_id < 1 {
        let corp_id = g.sim_co_count;
        if corp_id < 131 {
            return;
        }
        if g.industry_num[corp_id] != 70 || corp_id != g.corp_controller[corp_id] {
            return;
        }
        calc_entity_financials(g, ui, rng, corp_id);
        if g.credit_rating_num[corp_id] < 3 {
            return;
        }
        if g.ai_merger_acquirer_id == 0 && g.hold_stock_id[(0, corp_id)] < 15 {
            g.ai_merger_acquirer_id = corp_id;
            return;
        }
        if g.ai_merger_target_id != 0 {
            return;
        }
        if g.market_cap[corp_id] < g.exchange_rate * Ext::from_f64(50.0) {
            g.ai_merger_target_id = corp_id;
        }
        return;
    }
    'merge: {
        if g.industry_num[g.ai_merger_acquirer_id] != 70 || g.industry_num[g.ai_merger_target_id] != 70 {
            break 'merge;
        }
        if !(10 < g.corp_controller[g.ai_merger_acquirer_id]) {
            break 'merge;
        }
        if find_holding_slot(g, g.ai_merger_acquirer_id, g.ai_merger_target_id) == 0 {
            std::mem::swap(&mut g.ai_merger_acquirer_id, &mut g.ai_merger_target_id);
            if find_holding_slot(g, g.ai_merger_acquirer_id, g.ai_merger_target_id) == 0 {
                g.ai_merger_acquirer_id = 0;
                g.ai_merger_target_id = 0;
                return;
            }
        }
        if g.market_cap[g.ai_merger_acquirer_id] < g.market_cap[g.ai_merger_target_id] {
            if g.hold_stock_id[(0, g.ai_merger_target_id)] < 15 {
                std::mem::swap(&mut g.ai_merger_acquirer_id, &mut g.ai_merger_target_id);
            }
        }
        if g.tax_loss_carry[g.ai_merger_target_id] < -Ext::from_f64(500.0) {
            if Ext::from_f64(0.0) <= g.tax_loss_carry[g.ai_merger_acquirer_id] {
                std::mem::swap(&mut g.ai_merger_acquirer_id, &mut g.ai_merger_target_id);
            }
        }
        let target = g.ai_merger_target_id;
        if target != g.corp_controller[target] {
            g.ai_merger_target_id = 0;
            return;
        }
        merge_companies(g, ui, rng, g.ai_merger_acquirer_id, g.ai_merger_target_id, 2, 0, time_secs);
        g.wholly_owned_by = 0;
        if g.entity_status[g.ai_merger_target_id] == 1 {
            update_corp_control(g, ui, rng, g.ai_merger_target_id);
        }
        if cast::<i32, _>(g.wholly_owned_by) == g.ai_merger_acquirer_id {
            let t = g.ai_merger_target_id;
            if g.tax_loss_carry[t] < -Ext::from_f64(50.0)
                && g.market_cap[t] * Ext::from_f64(100.0) < g.tax_loss_carry[t].abs()
                && g.difficulty_level < 3
            {
                capital_infusion(g, ui, rng, g.ai_merger_acquirer_id, g.ai_merger_target_id);
            } else {
                g.tmp_ext1 = Ext::from_f64(2.0);
                liquidate_sub_into_parent(g, ui, rng, g.ai_merger_acquirer_id, g.ai_merger_target_id, time_secs);
            }
        }
    }
    g.ai_merger_acquirer_id = 0;
    g.ai_merger_target_id = 0;
}

/// Port of AiManageSmallHoldings (0x4b2114).
/// For an active non-fund corp `corp_id` (131+, not human-controlled): a random holding slot with a
/// stake below 10..19.9% is either topped up toward 20% (`BuyStock` of up to 10 points, at most half the
/// gap above 10, `TmpExt2` = 2) when cash and T-bills exceed 1.2x of 1.4x its cost — not by a
/// computer player's corp that would exceed `CpuMaxCorps` for a stock it does not control — or, when
/// unaffordable and under 12%, sold entirely (`SellStockHolding`, mode 2). `time_secs`: seconds of
/// `TIME$` (for the purchase).
pub fn ai_manage_small_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_id: i32, time_secs: i32) {
    // ON ERROR GOTO -> ShowRuntimeError("SmallHoldings") (error path not ported)
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if !(130 < corp_id) {
        return;
    }
    if g.industry_num[corp_id] == 71 {
        return;
    }
    if get_controller_type(g, corp_id) == 1 {
        return;
    }
    calc_entity_financials(g, ui, rng, corp_id);
    let slot = rng.rnd_range(15, 1);
    if !(0 < g.hold_stock_id[(slot, corp_id)]) {
        return;
    }
    let cut = sf(9.9f32) * rng.rnd() + Ext::from_f64(10.0);
    if !((cast::<Ext, _>(g.hold_stock_pct[(slot, corp_id)])) < cut) {
        return;
    }
    let stock_id = cast::<i32, _>(g.hold_stock_id[(slot, corp_id)]);
    let mut pct_to_buy: i32 = 20 - cast::<i32, _>(g.hold_stock_pct[(slot, corp_id)]);
    if 10 < pct_to_buy {
        pct_to_buy = to_long(cast::<Ext, _>(pct_to_buy) * sf(0.5f32));
    }
    let buy_cost = g.market_cap[stock_id] * cast::<Ext, _>(pct_to_buy) * sf(1.4f32);
    if buy_cost * sf(1.2f32) < g.cash[corp_id] + g.t_bills[corp_id] && 0 < pct_to_buy {
        if 10 < corp_id && get_controller_type(g, corp_id) == 2 {
            let cpu_player = g.corp_controller[corp_id];
            if cpu_player != g.corp_controller[stock_id] {
                if !(g.cpu_max_corps > g.player_corps_controlled[cpu_player]) {
                    return;
                }
            }
        }
        g.tmp_ext1 = cast::<Ext, _>(pct_to_buy);
        g.tmp_ext2 = Ext::from_f64(2.0);
        buy_stock(g, ui, rng, corp_id, stock_id, time_secs);
    } else if (cast::<i32, _>(g.hold_stock_pct[(slot, corp_id)])) < 12 {
        g.tmp_ext1 = cast::<Ext, _>(corp_id);
        g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(slot, corp_id)]);
        g.tmp_ext3 = Ext::from_f64(2.0);
        sell_stock_holding(g, ui, rng, stock_id);
    }
}

/// Port of AiBankBuysLoan (0x4bc3e7).
/// With computer players in the game: the loan of corp `SimCoCount` (active; not controlled by a human
/// player; its lender not human-controlled) is bought by a random active bank 11..85 controlled by the
/// same computer player as the borrower. A borrower without debt is simply moved to that bank. Else,
/// when the buyer is another bank and the buyer's cash and T-bills plus the borrower's cash exceed the
/// loan at `CalcLoanSalePricePct`, `SellLoanToBank` moves it (pending headline posted) and both banks'
/// financials are recalculated unless the borrower's cash is 0.
///
/// A buyer bank without a player controller (`CorpController` 0) reads `IsComputerPlayer(0)`: error 9,
/// the ON ERROR handler shows `ShowRuntimeError` and leaves; the port just leaves (no message box;
/// BUGS.md #201).
pub fn ai_bank_buys_loan(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let corp_id = g.sim_co_count;
    if g.entity_status[corp_id] != 1 {
        return;
    }
    if !(g.has_cpu_players == "Y") {
        return;
    }
    let ctrl = g.corp_controller[corp_id];
    if 0 < ctrl && ctrl <= g.num_players {
        if g.is_computer_player[ctrl] == 0 {
            return;
        }
    }
    g.lender_id = cast::<i32, _>(g.lending_bank[corp_id]);
    if get_controller_type(g, g.lender_id) > 0 {
        return;
    }
    let buyer_bank = rng.rnd_range(85, 11);
    if g.entity_status[buyer_bank] != 1 {
        return;
    }
    let buyer_ctrl = g.corp_controller[buyer_bank];
    if !(buyer_ctrl < 11) {
        return;
    }
    if !(1..=5).contains(&buyer_ctrl) {
        return; // IsComputerPlayer(ctrl): error 9 -> ON ERROR handler (ShowRuntimeError)
    }
    if g.is_computer_player[buyer_ctrl] == 0 {
        return;
    }
    if buyer_ctrl != ctrl {
        return;
    }
    if g.bank_debt[corp_id] == Ext::from_f64(0.0) {
        g.lending_bank[corp_id] = cast::<i16, _>(buyer_bank);
        return;
    }
    if buyer_bank == g.lender_id {
        return;
    }
    let price_pct = calc_loan_sale_price_pct(g, ui, rng, corp_id);
    let factor: Ext = sf(0.01f32) * cast::<Ext, _>(price_pct);
    if !(g.bank_debt[corp_id] * factor < g.cash[corp_id] + (g.cash[buyer_bank] + g.t_bills[buyer_bank])) {
        return;
    }
    g.lender_id = cast::<i32, _>(g.lending_bank[corp_id]);
    g.news_msg = PbStr::new();
    sell_loan_to_bank(g, ui, rng, buyer_bank, g.lender_id, corp_id);
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    if g.cash[corp_id] == Ext::from_f64(0.0) {
        return;
    }
    calc_entity_financials(g, ui, rng, g.lender_id);
    calc_entity_financials(g, ui, rng, buyer_bank);
}
