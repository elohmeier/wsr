//! AiComputerPlayerTurn steps: excess cash, antitrust target, target-stock purchases, subsidiary review,
//! fund advances, growth industries, calling in advances.
//!
//! Callees that read `TIME$` (CalcStockPrice, AiFileAntitrustSuits, StartNewCorpOrExpand) get the turn's
//! `Frame::time_secs`.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::buyer::validate_buyer_corp;
use crate::logic::ai_player::ai_find_affordable_buyer;
use crate::logic::control::update_corp_control;
use crate::logic::corp_ai::distress::ai_manage_small_holdings;
use crate::logic::events::antitrust::{ai_file_antitrust_suits, calc_antitrust_damages};
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials};
use crate::logic::sim_entity::ai_corp::ai_consolidate_subsidiary;
use crate::logic::sim_entity::asset_sales::start_new_corp_or_expand;
use crate::logic::sim_entity::quarter_start::{credit_rating_after_repayment, pack_holding_arrays};
use crate::logic::valuation::{
    adjust_index_divisor, calc_dividend_yield, calc_industry_market_share, calc_industry_outlook,
    calc_stock_price,
};

/// Port of AiPlayer_UseExcessCash (0x5c67af), GOSUB of AiComputerPlayerTurn ("3000"), from Q2 on.
/// With this year's income (accrued P&L, extraordinary P&L, dividends received, positive capital gains)
/// positive: a highly rated cash-rich controlled main holding (`AiTargetCorp`, public float, rating 9+,
/// more than `20000 * ExchangeRate` liquid) of an indebted player sets a 1% dividend (yield under 1%,
/// no loss carry) / raises it by 2..15% (sic: `TaxLossCarry > 0`); otherwise (no buyout planned) the
/// player borrows up to `NetWorth * (0.03 + 0.02 * DifficultyLevel)` from `LenderId` (none when the debt
/// exceeds 10% of net worth) and trims a main holding's dividend (yield over 2%: x0.8..0.9; with a loss
/// carry and yield over 1.2%: x0.85). Without income: repays up to half its cash (at most the debt) and
/// sets/raises the main holding's dividend (yield under 15% / 1%). Finally a player owning 100% of its
/// main holding (with income and cash over 25) moves 40% of its cash into it and revalues it.
pub(crate) fn use_excess_cash(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("3000");
    if g.cur_quarter < 2 {
        return;
    }
    let p = f.player_id;
    f.year_income = g.accrued_pl[p] + cast::<Ext, _>(g.extraordinary_pl[p]) + g.div_recvd1[p] + g.div_recvd2[p];
    if Ext::from_f64(0.0) < g.player_cap_gains[p] {
        f.year_income = g.player_cap_gains[p] + f.year_income;
    }
    f.cash_amt = 0;
    if 10 < g.ai_target_corp {
        update_corp_control(g, ui, rng, g.ai_target_corp);
    }
    if !(Ext::from_f64(0.0) < f.year_income) {
        if Ext::from_f64(0.0) < g.cash[p] {
            f.cash_amt = to_quad(g.cash[p] * sf(0.5f32));
        }
        if (cast::<Ext, _>(f.cash_amt)) > g.bank_debt[p] {
            f.cash_amt = to_quad(g.bank_debt[p]);
        }
        if 0 < f.cash_amt {
            f.cash_amt = to_quad(-(cast::<Ext, _>(f.cash_amt)));
        }
        let t = g.ai_target_corp;
        if 10 < t && f.player_id == g.corp_controller[t] && calc_dividend_yield(g, t) < Ext::from_f64(15.0) && !(g.tax_loss_carry[t] < Ext::from_f64(0.0))
        {
            if calc_dividend_yield(g, t) < Ext::from_f64(1.0) {
                g.dividend_per_share[t] = g.market_cap[t] * sf(0.01f32) / g.shares_outstanding[t];
            }
            if Ext::from_f64(0.0) < g.tax_loss_carry[t] {
                g.dividend_per_share[t] = g.dividend_per_share[t] * (sf(0.13f32) * rng.rnd() + sf(1.02f32));
            }
        }
    } else {
        let t = g.ai_target_corp;
        let mut done = false;
        if 10 < t
            && 0 < g.public_pct
            && 8 < g.credit_rating_num[t]
            && g.exchange_rate * Ext::from_f64(20000.0) < g.t_bills[t] + g.cash[t]
            && f.player_id == g.corp_controller[t]
            && (g.t_bills[p] + g.cash[p]) * sf(0.3f32) < g.bank_debt[p]
        {
            if calc_dividend_yield(g, t) < Ext::from_f64(1.0) && !(g.tax_loss_carry[t] < Ext::from_f64(0.0)) {
                g.dividend_per_share[t] = sf(0.01f32) * g.market_cap[t];
            }
            if Ext::from_f64(0.0) < g.tax_loss_carry[t] {
                g.dividend_per_share[t] =
                    (sf(0.13f32) * rng.rnd() + sf(1.02f32)) * g.dividend_per_share[t] / g.shares_outstanding[t];
            }
            done = true;
        }
        if !done && f.buyout_mode == 0 && Ext::from_f64(0.0) < f.year_income {
            calc_entity_financials(g, ui, rng, f.player_id);
            if Ext::from_f64(0.0) < g.unused_credit_line {
                f.leverage_rate = crate::pb::numeric::double(sf(0.03f32) + sf(0.02f32) * cast::<Ext, _>(g.difficulty_level));
                f.cash_amt = to_quad(g.net_worth * f.leverage_rate - g.bank_debt[p]);
                if g.net_worth * sf(0.1f32) < g.bank_debt[p] {
                    f.cash_amt = 0;
                }
                if g.unused_credit_line < cast::<Ext, _>(f.cash_amt) {
                    f.cash_amt = to_quad(g.unused_credit_line);
                }
                if (cast::<Ext, _>(f.cash_amt)) < Ext::from_f64(0.0) {
                    f.cash_amt = 0;
                }
            }
            if 10 < t && f.player_id == g.corp_controller[t] {
                if Ext::from_f64(2.0) < calc_dividend_yield(g, t) {
                    g.dividend_per_share[t] = (sf(0.1f32) * rng.rnd() + sf(0.8f32)) * g.dividend_per_share[t];
                }
                if g.tax_loss_carry[t] < Ext::from_f64(0.0) && sf(1.2f32) < calc_dividend_yield(g, t) {
                    g.dividend_per_share[t] = sf(0.85f32) * g.dividend_per_share[t];
                }
            }
        }
    }
    let c = cast::<Ext, _>(f.cash_amt);
    g.cash[p] = c + g.cash[p];
    let l = g.lender_id;
    g.cash[l] = g.cash[l] - c;
    g.bank_debt[p] = c + g.bank_debt[p];
    if Ext::from_f64(0.0) < f.year_income && g.hold_stock_pct[(1, p)] == 100 && Ext::from_f64(25.0) < g.cash[p] {
        f.cash_amt = to_quad(g.cash[p] * sf(0.4f32));
        let c = cast::<Ext, _>(f.cash_amt);
        let h = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        g.cash[h] = c + g.cash[h];
        g.cash[p] = g.cash[p] - c;
        g.hold_stock_basis[(1, p)] = c + g.hold_stock_basis[(1, p)];
        g.valuation_mode = 3;
        g.ai_target_corp = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        calc_stock_price(g, ui, rng, g.ai_target_corp, f.time_secs);
    }
}

/// Port of AiPlayer_PickAntitrustTarget (0x5c7240), GOSUB of AiComputerPlayerTurn ("4000").
/// Unless [runtime resource wording]  are off: runs AiFileAntitrustSuits (its plaintiff = `suitTarget`) and,
/// for a player controlling its main holding, picks a company to buy into for an antitrust suit:
/// the plaintiff (with an earnings history, not the current player's, its industry leader's share above
/// `54..58`) → `suitTarget`/`targetPct`; the plaintiff of the pending government case (defendant not the
/// player's, share above `55..58`, 20% public float) → `targetId`; then the pending private suits
/// (`LawsuitRec`, share above `targetPct` and `53..58`; a share above 58 marks the defendant as
/// `suitCorpId`) whose defendant and plaintiff are not the player's (defendant with 20% float).
pub(crate) fn pick_antitrust_target(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("4000");
    if g.private_antitrust_off == 1 {
        return;
    }
    f.target_pct = 0;
    f.target_id = 0;
    f.suit_target = ai_file_antitrust_suits(g, ui, rng, f.time_secs);
    if g.ai_target_corp == 0 || f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    if 130 < f.suit_target
        && g.earnings_yr2_ago[f.suit_target] == Ext::from_f64(0.0)
        && g.earnings_yr3_ago[f.suit_target] == Ext::from_f64(0.0)
    {
        f.suit_target = 0;
    }
    if 130 < f.suit_target {
        f.industry = g.industry_num[f.suit_target];
        f.other_id = calc_industry_outlook(g, rng, f.industry);
        f.pick_id = f.other_id;
        if 130 < f.other_id {
            f.pick_id = g.corp_controller[f.other_id];
        }
        g.tmp_ext2 = Ext::from_f64(0.0);
        g.tmp_ext3 = Ext::from_f64(0.0);
        g.tmp_ext1 = cast::<Ext, _>(f.pick_id);
        f.target_pct = calc_industry_market_share(g, f.industry);
        if g.cur_player == g.corp_controller[f.suit_target] {
            f.suit_target = 0;
        }
        if rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(54.0) > cast::<Ext, _>(f.target_pct) {
            f.suit_target = 0;
            f.target_pct = 0;
        }
    }
    let d = g.antitrust_defendant;
    if 130 < d && f.player_id != g.corp_controller[d] {
        if rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(55.0) < cast::<Ext, _>(g.antitrust_market_share) {
            let pl = g.antitrust_plaintiff;
            if 130 < pl && f.player_id != g.corp_controller[pl] {
                f.target_id = pl;
                update_corp_control(g, ui, rng, f.target_id);
                if g.public_pct < 20 {
                    f.target_id = 0;
                }
            }
        }
    }
    if f.target_id == 0 && 130 < f.suit_target {
        f.target_id = f.suit_target;
        f.suit_target = 0;
    }
    if 130 < f.target_id && 130 < f.suit_target {
        if g.antitrust_market_share < f.target_pct {
            std::mem::swap(&mut f.target_id, &mut f.suit_target);
        } else {
            f.target_pct = g.antitrust_market_share;
        }
    }
    if f.target_id == 0 && f.suit_target == 0 {
        f.target_pct = 0;
    }
    f.suit_corp_id = 0;
    f.ent_id = 1;
    while f.ent_id <= 10 {
        f.other_id = 0;
        f.defendant_id = 0;
        f.slot = 0;
        f.pending_msg = g.lawsuit_rec[f.ent_id].clone();
        if mid(f.pending_msg.as_bytes(), 1, 1) == "1" {
            f.field_text = parse_any(f.pending_msg.as_bytes(), b"|", 4);
            f.other_id = to_long(val(f.field_text.as_bytes()));
            if f.target_pct < f.other_id && rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(53.0) < cast::<Ext, _>(f.other_id) {
                f.field_text = parse_any(f.pending_msg.as_bytes(), b"|", 3);
                f.defendant_id = to_long(val(f.field_text.as_bytes()));
                f.field_text = parse_any(f.pending_msg.as_bytes(), b"|", 2);
                f.slot = to_long(val(f.field_text.as_bytes()));
                if 58 < f.other_id {
                    f.suit_corp_id = f.slot;
                }
                update_corp_control(g, ui, rng, f.defendant_id);
                if f.player_id != g.corp_controller[f.defendant_id] && 19 < g.public_pct {
                    update_corp_control(g, ui, rng, f.slot);
                    if f.player_id != g.corp_controller[f.slot] {
                        f.target_id = f.defendant_id;
                        f.target_pct = f.other_id;
                        f.ent_id = 10;
                    }
                }
            }
        }
        f.ent_id += 1;
    }
}

/// Port of AiPlayer_BuyTargetStock (0x5c7855), GOSUB of AiComputerPlayerTurn ("4500").
/// For a player controlling its main holding (with fewer than 15 holdings) and an antitrust target from
/// PickAntitrustTarget: each of its companies (banks only without stock holdings) with liquid funds of
/// 21x the target's market cap tries to buy 20% of `targetId` (else of `suitTarget`) through
/// ValidateBuyerCorp. Without such purchases and with 100 in liquid funds, when a monopolist
/// (`suitCorpId`) was found and a lawsuit slot is free: a player without holdings founds a company in the
/// monopolist's industry (100% owned, cash `min(Cash/2, 500 * ExchangeRate)`, counts as a start-up);
/// otherwise the main holding (same industry or a shell, not a recent antitrust filer/plaintiff, no large
/// loss carry, `50 * ExchangeRate` liquid) becomes the plaintiff — a shell (industry 70) re-enters the
/// industry with 100 (50) of business assets. A plaintiff with an earnings history files a private suit
/// (`LawsuitRec`: `"1 |defendant|plaintiff|share|damages|0|"`).
pub(crate) fn buy_target_stock(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("4500");
    if g.private_antitrust_off == 1 {
        return;
    }
    if g.ai_target_corp == 0 || f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    if f.suit_target == 0 && f.target_id == 0 {
        return;
    }
    f.trade_count = 0;
    let t = g.ai_target_corp;
    if 0 < f.target_id && 0 < g.hold_stock_id[(0, t)] && g.hold_stock_id[(0, t)] < 15 {
        f.pay_amt = g.market_cap[f.target_id] * Ext::from_f64(21.0);
        f.ent_id = 11;
        while f.ent_id < 1601 {
            let e = f.ent_id;
            if g.entity_status[e] == 1
                && f.player_id == g.corp_controller[e]
                && !(e < 86 && 0 < g.hold_stock_id[(1, e)])
            {
                calc_entity_financials(g, ui, rng, e);
                g.liquid_funds = g.t_bills[e] + g.cash[e] + g.unused_credit_line * sf(0.25f32);
                if f.pay_amt <= g.liquid_funds {
                    f.buyer_corp = e;
                    f.target_pct = 20;
                    update_corp_control(g, ui, rng, f.target_id);
                    if g.public_pct < 20 {
                        f.buyer_corp = 0;
                    } else {
                        validate_buyer_corp(g, ui, rng, f);
                        f.trace_tag = PbStr::from(__content.string(1818));
                        f.trade_count += 1;
                        if 4 < g.turn_action_count {
                            f.ent_id = 1600;
                        }
                    }
                }
            }
            f.ent_id += 1;
        }
    }
    let t = g.ai_target_corp;
    if f.trade_count == 0 && 0 < f.suit_target && 0 < g.hold_stock_id[(0, t)] && g.hold_stock_id[(0, t)] < 15 {
        f.pay_amt = g.market_cap[f.suit_target] * Ext::from_f64(21.0);
        f.ent_id = 11;
        while f.ent_id < 1601 {
            let e = f.ent_id;
            if g.entity_status[e] == 1
                && f.player_id == g.corp_controller[e]
                && !(e < 86 && 0 < g.hold_stock_id[(1, e)])
            {
                calc_entity_financials(g, ui, rng, e);
                g.liquid_funds = g.t_bills[e] + g.cash[e] + g.unused_credit_line * sf(0.25f32);
                if f.pay_amt <= g.liquid_funds {
                    f.buyer_corp = e;
                    f.target_id = f.suit_target;
                    f.target_pct = 20;
                    update_corp_control(g, ui, rng, f.target_id);
                    if 19 < g.public_pct {
                        validate_buyer_corp(g, ui, rng, f);
                        f.trade_count += 1;
                        if 4 < g.turn_action_count {
                            f.ent_id = 1600;
                        }
                    }
                }
            }
            f.ent_id += 1;
        }
    }
    if 0 < f.trade_count {
        return;
    }
    let p = f.player_id;
    if !(g.t_bills[p] + g.cash[p] >= Ext::from_f64(100.0)) {
        return;
    }
    if f.suit_corp_id < 131 {
        return;
    }
    f.found = 0;
    f.ent_id = 1;
    while f.ent_id <= 10 {
        if g.lawsuit_rec[f.ent_id].is_empty() {
            f.suit_slot = f.ent_id;
            f.ent_id = 10;
            f.found = 1;
        }
        f.ent_id += 1;
    }
    if f.found == 0 {
        return;
    }
    f.industry = g.industry_num[f.suit_corp_id];
    f.suit_target = 0;
    g.ai_target_corp = cast::<i32, _>(g.hold_stock_id[(1, p)]);
    if g.ai_target_corp < 1 {
        g.tmp_ext1 = Ext::from_f64(100.0);
        f.suit_target = start_new_corp_or_expand(g, ui, rng, f.industry, 1, f.time_secs);
        g.tmp_ext1 = Ext::from_f64(0.0);
        if f.suit_target < 131 {
            return;
        }
        let s = f.suit_target;
        g.shares_outstanding[s] = sf(0.5f32);
        g.hold_stock_id[(1, p)] = cast::<i16, _>(s);
        g.hold_stock_pct[(1, p)] = 100;
        g.ai_target_corp = s;
        update_corp_control(g, ui, rng, s);
        g.cash[p] = g.cash[p] - Ext::from_f64(100.0);
        f.cash_amt = to_quad(g.cash[p] * sf(0.5f32));
        if g.exchange_rate * Ext::from_f64(500.0) < cast::<Ext, _>(f.cash_amt) {
            f.cash_amt = to_quad(g.exchange_rate * Ext::from_f64(500.0));
        }
        let c = cast::<Ext, _>(f.cash_amt);
        g.cash[p] = g.cash[p] - c;
        g.cash[s] = c + g.cash[s];
        g.hold_stock_basis[(1, p)] = c + Ext::from_f64(100.0);
        pack_holding_arrays(g, ui);
        g.valuation_mode = 3;
        calc_stock_price(g, ui, rng, s, f.time_secs);
        g.tmp_ext3 = c;
        adjust_index_divisor(g, s, 0);
        g.turn_action_count += 1;
        let cp = g.cur_player;
        g.player_startup_count[cp] += 1;
    } else {
        let t = g.ai_target_corp;
        f.found = 1;
        if g.industry_num[t] != 70 && g.industry_num[t] != f.industry {
            f.found = 0;
        }
        if g.antitrust_plaintiff == t {
            f.found = 0;
        }
        f.slot = 1;
        while f.slot <= 10 {
            if t == g.recent_antitrust_filers[f.slot] {
                f.found = 0;
            }
            f.pending_msg = g.lawsuit_rec[f.slot].clone();
            if mid(f.pending_msg.as_bytes(), 1, 1) == "1" {
                f.field_text = parse_any(f.pending_msg.as_bytes(), b"|", 3);
                if val(f.field_text.as_bytes()) == cast::<Ext, _>(t) {
                    f.found = 0;
                }
            }
            f.slot += 1;
        }
        calc_entity_financials(g, ui, rng, t);
        g.liquid_funds = g.t_bills[t] + g.cash[t] + g.unused_credit_line * sf(0.25f32);
        if g.tax_loss_carry[t] < g.exchange_rate * -Ext::from_f64(100.0)
            && Ext::from_f64(0.0) < g.net_worth
            && g.net_worth * sf(0.25f32) < g.tax_loss_carry[t].abs()
        {
            f.found = 0;
        }
        if f.found == 0 || g.liquid_funds < g.exchange_rate * Ext::from_f64(50.0) {
            f.suit_target = 0;
        } else if 130 < t && g.industry_num[t] == 70 {
            f.suit_target = t;
            g.industry_num[t] = f.industry;
            f.pick_id = to_long(g.exchange_rate * Ext::from_f64(100.0));
            if g.liquid_funds < cast::<Ext, _>(f.pick_id) {
                f.pick_id = to_long(g.exchange_rate * Ext::from_f64(50.0));
            }
            g.business_assets[t] = cast::<Ext, _>(f.pick_id);
            g.cash[t] = g.cash[t] - cast::<Ext, _>(f.pick_id);
            if g.tax_loss_carry[t] < Ext::from_f64(0.0) {
                g.tax_loss_carry[t] = Ext::from_f64(0.0);
            }
        }
    }
    if 130 < f.suit_target
        && g.earnings_yr2_ago[f.suit_target] == Ext::from_f64(0.0)
        && g.earnings_yr3_ago[f.suit_target] == Ext::from_f64(0.0)
    {
        f.suit_target = 0;
    }
    if f.suit_target == 0 {
        return;
    }
    g.tmp_ext1 = cast::<Ext, _>(g.corp_controller[f.suit_corp_id]);
    f.target_pct = calc_industry_market_share(g, f.industry);
    f.pending_msg = PbStr::from("1 |")
        + &str_s(cast::<Ext, _>(f.suit_corp_id))
        + "|"
        + &str_s(cast::<Ext, _>(f.suit_target))
        + "|"
        + &str_s(cast::<Ext, _>(f.target_pct))
        + "|";
    f.result_amt = calc_antitrust_damages(g, ui, rng, f.suit_target, f.suit_corp_id, f.target_pct);
    f.pending_msg = f.pending_msg.clone() + &trim(str_s(cast::<Ext, _>(f.result_amt)).as_bytes()) + "|0|";
    g.lawsuit_rec[f.suit_slot] = f.pending_msg.clone();
    calc_entity_financials(g, ui, rng, f.player_id);
}

/// Port of AiPlayer_ReviewSubHoldings (0x5c8c0b), GOSUB of AiComputerPlayerTurn ("5100").
/// When the current player controls its main holding: AiManageSmallHoldings for it, then five random
/// holding slots of it: an operating company (industry 3..69) that is the player's or controlled by a
/// company gets AiConsolidateSubsidiary with its share of the industry's assets (`TmpExt1`, after
/// CalcIndustryOutlook).
pub(crate) fn review_sub_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("5100");
    if g.ai_target_corp == 0 {
        return;
    }
    update_corp_control(g, ui, rng, g.ai_target_corp);
    if g.cur_player != g.voting_controller {
        return;
    }
    ai_manage_small_holdings(g, ui, rng, g.ai_target_corp, f.time_secs);
    f.slot = 5;
    loop {
        f.pick_id = rng.rnd_range(15, 1);
        f.other_id = cast::<i32, _>(g.hold_stock_id[(f.pick_id, g.ai_target_corp)]);
        let o = f.other_id;
        if o != 0
            && 3 <= g.industry_num[o]
            && g.industry_num[o] <= 69
            && (f.player_id == g.corp_controller[o] || 11 <= g.corp_controller[o])
        {
            f.industry = g.industry_num[o];
            calc_industry_outlook(g, rng, f.industry);
            g.ind_total_assets = g.tmp_ext1;
            g.tmp_ext1 = g.business_assets[o] / g.ind_total_assets * Ext::from_f64(100.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            ai_consolidate_subsidiary(g, ui, rng, o, f.time_secs);
        }
        f.slot -= 1;
        if f.slot == 0 {
            return;
        }
    }
}

/// Port of AiPlayer_AdvanceFunds (0x5c8e00), GOSUB of AiComputerPlayerTurn ("5200").
/// A rich player (liquid funds over 25x the main holding's market cap and `2000 * ExchangeRate`)
/// advances 30% of them to its controlled, well-rated (6+) non-bank main holding once a year (news).
/// Then, unless the market is overvalued (`MarketValuation > 1.1`) or the main holding owes holding-company
/// tax (checked with the player's stake raised by one point), a player rated 8+ owning 18..99% of its
/// main holding buys more: up to three purchases of 1..5% (`BuyStock`) while its liquid funds exceed 7x
/// the market cap and 5% are public, then 1% more when they exceed 1.5x and it holds 5%+.
pub(crate) fn advance_funds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("5200");
    if g.ai_target_corp == 0 {
        return;
    }
    let p = f.player_id;
    let t = g.ai_target_corp;
    if 85 < t
        && f.player_id == g.corp_controller[t]
        && 5 < g.credit_rating_num[t]
        && g.cur_year != g.ai_advance_year
        && g.market_cap[t] * Ext::from_f64(25.0) < g.t_bills[p] + g.cash[p]
        && g.exchange_rate * Ext::from_f64(2000.0) < g.t_bills[p] + g.cash[p]
    {
        f.result_amt = to_quad((g.t_bills[p] + g.cash[p]) * sf(0.3f32));
        let r = cast::<Ext, _>(f.result_amt);
        g.cash[p] = g.cash[p] - r;
        g.cash[t] = r + g.cash[t];
        g.player_advances[(p, t)] = r + g.player_advances[(p, t)];
        g.ai_advance_year = g.cur_year;
        g.news_msg = g.ent_symbol[p].clone()
            + __content.string(1819)
            + &g.ent_name[t]
            + " ("
            + &g.ent_symbol[t]
            + ")";
        post_news(g, ui, 0, 0);
    }
    if !(sf(1.1f32) >= cast::<Ext, _>(g.market_valuation)) {
        return;
    }
    if 0 < g.holding_co_tax_rate && 130 < g.ai_target_corp {
        let t = g.ai_target_corp;
        calc_entity_financials(g, ui, rng, t);
        if cast::<i32, _>(g.hold_stock_id[(1, p)]) == t {
            g.hold_stock_pct[(1, p)] += 1;
        }
        f.amount = calc_capital_taxes(g, t);
        if cast::<i32, _>(g.hold_stock_id[(1, p)]) == t {
            g.hold_stock_pct[(1, p)] -= 1;
        }
        if Ext::from_f64(0.0) < g.holding_co_tax_due {
            return;
        }
    }
    calc_entity_financials(g, ui, rng, f.player_id);
    if g.credit_rating_num[p] < 8 {
        return;
    }
    if !(g.hold_stock_pct[(1, p)] < 100 && 17 < g.hold_stock_pct[(1, p)]) {
        return;
    }
    g.public_pct = 10;
    f.target_id = g.ai_target_corp;
    f.trade_count = 0;
    loop {
        if !(g.market_cap[f.target_id] * Ext::from_f64(7.0) < g.t_bills[p] + g.cash[p]) || g.public_pct <= 4 {
            break;
        }
        update_corp_control(g, ui, rng, f.target_id);
        if g.public_pct == 0 {
            break;
        }
        f.cur_pct = cast::<i32, _>(g.hold_stock_pct[(1, p)]);
        g.tmp_ext1 = int(Ext::from_f64(5.0) * rng.rnd()) + Ext::from_f64(1.0);
        if g.tmp_ext1 > cast::<Ext, _>(g.public_pct) {
            g.tmp_ext1 = cast::<Ext, _>(g.public_pct);
        }
        if g.tmp_ext1 + cast::<Ext, _>(f.cur_pct) > Ext::from_f64(15.0) {
            f.trade_count = 2;
        }
        g.tmp_ext2 = Ext::from_f64(1.0);
        crate::logic::trading::buy_stock(g, ui, rng, p, f.target_id, f.time_secs);
        f.trade_count += 1;
        if f.trade_count == 3 {
            g.public_pct = 0;
        }
    }
    update_corp_control(g, ui, rng, f.target_id);
    f.cur_pct = cast::<i32, _>(g.hold_stock_pct[(1, p)]);
    if 0 < g.public_pct && g.market_cap[f.target_id] * sf(1.5f32) < g.t_bills[p] + g.cash[p] && 5 <= f.cur_pct {
        g.tmp_ext1 = Ext::from_f64(1.0);
        g.tmp_ext2 = Ext::from_f64(1.0);
        crate::logic::trading::buy_stock(g, ui, rng, p, f.target_id, f.time_secs);
    }
}

/// Port of AiPlayer_EnterGrowthIndustry (0x5c9506), GOSUB of AiComputerPlayerTurn ("5300").
/// For a player controlling its main holding that bought nothing this turn and controls fewer than
/// `CpuMaxCorps` companies: picks the last industry whose forecast exceeds its growth by 10+ and whose
/// projected demand gap grows above `7..16` (not the crowded one), then scans all companies (rated 6+,
/// positive net worth, value `MarketCap * 100` at most 1.25x net worth and affordable relative to
/// `maxLiquid`) not already firmly the player's for the cheapest one (value / net worth below the best so
/// far, starting at 5) that a company of player 1 (AiFindAffordableBuyer, sic) can buy 25% (10%) of;
/// companies in the picked industry only count while no earlier pick exists (sic). The chosen company
/// is bought through ValidateBuyerCorp.
pub(crate) fn enter_growth_industry(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("5300");
    if !(0 < g.ai_target_corp && f.player_id == g.corp_controller[g.ai_target_corp] && f.bought_flag <= 0) {
        return;
    }
    if !(g.cpu_max_corps > g.player_corps_controlled[f.player_id]) {
        return;
    }
    f.other_id = 0;
    f.industry = 3;
    while f.industry <= 69 {
        let i = cast::<i32, _>(f.industry);
        if cast::<i32, _>(g.ind_growth_forecast[i]) > cast::<i32, _>(g.ind_growth_rate[i]) + 10 {
            calc_industry_outlook(g, rng, f.industry);
            if cast::<i32, _>(f.industry) != g.crowded_industry {
                f.cur_outlook = cast::<f32, _>(g.tmp_ext1);
                f.fut_outlook = cast::<f32, _>(g.tmp_ext2);
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                g.tmp_ext3 = Ext::from_f64(0.0);
                if f.cur_outlook < f.fut_outlook && rng.rnd() * Ext::from_f64(9.0) + Ext::from_f64(7.0) < cast::<Ext, _>(f.fut_outlook) {
                    f.other_id = cast::<i32, _>(f.industry);
                }
            }
        }
        f.industry += 1;
    }
    f.industry = cast::<i16, _>(f.other_id);
    f.leverage_rate = crate::pb::numeric::double(Ext::from_f64(5.0));
    f.x_190 = 0;
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && g.industry_num[e] != 71 {
            calc_entity_financials(g, ui, rng, e);
            if !(6 > g.credit_rating_num[e]) && Ext::from_f64(0.0) < g.net_worth {
                f.net_worth_ref = g.net_worth;
                f.cash_amt = to_quad(Ext::from_f64(100.0) * g.market_cap[e]);
                let cash = cast::<Ext, _>(f.cash_amt);
                let affordable = cash <= g.net_worth * sf(1.25f32)
                    && (f.max_liquid * sf(0.06f32) <= cash * sf(0.25f32) || cash <= g.net_worth * sf(0.6f32))
                    && f.max_liquid >= cash * sf(0.26f32);
                if affordable {
                    update_corp_control(g, ui, rng, e);
                    if !(f.player_id == g.corp_controller[e] && 25 < g.largest_holder_pct) {
                        f.target_pct = 25;
                        f.target_id = e;
                        f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, f.target_id, f.target_pct, 0);
                        if g.industry_num[e] == f.industry && !(6 > g.credit_rating_num[e]) {
                            if 10 < f.x_190 {
                                f.buyer_corp = 0;
                            }
                        } else if !(cash / f.net_worth_ref < f.leverage_rate) {
                            f.buyer_corp = 0;
                        } else {
                            if f.buyer_corp == 0 {
                                f.target_pct = 10;
                                f.target_id = e;
                                f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, f.target_id, f.target_pct, 0);
                            }
                            if f.buyer_corp > 10 && cash / f.net_worth_ref < f.leverage_rate {
                                f.x_190 = e;
                            } else {
                                f.buyer_corp = 0;
                            }
                        }
                        if f.buyer_corp != 0 && cash / f.net_worth_ref < f.leverage_rate {
                            f.leverage_rate = crate::pb::numeric::double(cash / f.net_worth_ref);
                        }
                    }
                }
            }
        }
        f.ent_id += 1;
    }
    if f.x_190 == 0 {
        return;
    }
    f.target_pct = 25;
    if f.industry == 0 {
        f.target_id = f.x_190;
        f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, f.target_id, f.target_pct, 0);
        if f.buyer_corp == 0 {
            f.target_pct = 10;
        }
    }
    f.target_id = f.x_190;
    f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, f.target_id, f.target_pct, 0);
    if 10 < f.buyer_corp {
        f.target_id = f.x_190;
        validate_buyer_corp(g, ui, rng, f);
        f.trace_tag = PbStr::from(__content.string(1820));
    }
}

/// Port of AiPlayer_CallInAdvances (0x5c9b05), GOSUB of AiComputerPlayerTurn.
/// Calls in (half of, or all when that is under `10 * ExchangeRate`) the first advance the player made
/// to an active company it no longer controls, when the company is rated 6+ (after the repayment,
/// CreditRatingAfterRepayment) or is an operating company without bonds and bank debt whose cash plus
/// the player's (sic) T-bills cover 1/1.2 of the call or whose total assets are 5x the call. A partial
/// call is announced. Sets `AiAdvCallQuarter`.
pub(crate) fn call_in_advances(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let p = f.player_id;
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && Ext::from_f64(0.0) < g.player_advances[(p, e)] && f.player_id != g.corp_controller[e] {
            f.call_amt = sf(0.5f32) * g.player_advances[(p, e)];
            if f.call_amt < g.exchange_rate * Ext::from_f64(10.0) {
                f.call_amt = g.player_advances[(p, e)];
            }
            f.field_text = credit_rating_after_repayment(g, ui, rng, f.player_id, e, f.call_amt);
            let pay = if 6 <= g.credit_rating_num[e] {
                true
            } else if 131 <= e && g.bonds_issued[e] <= 0 && !(Ext::from_f64(0.0) < g.bank_debt[e]) {
                !(f.call_amt > (g.t_bills[p] + g.cash[e]) * sf(1.2f32)) || !(f.call_amt > g.total_assets * sf(0.2f32))
            } else {
                false
            };
            if pay {
                g.cash[p] = f.call_amt + g.cash[p];
                g.cash[e] = g.cash[e] - f.call_amt;
                if f.call_amt == g.player_advances[(p, e)] * sf(0.5f32) {
                    g.news_msg = g.ent_symbol[p].clone()
                        + __content.string(1821)
                        + &g.ent_symbol[e];
                    post_news(g, ui, 0, 0);
                    g.player_advances[(p, e)] = g.player_advances[(p, e)] - f.call_amt;
                } else {
                    g.player_advances[(p, e)] = Ext::from_f64(0.0);
                }
                g.ai_adv_call_quarter = g.cur_quarter;
                return;
            }
        }
        f.ent_id += 1;
    }
}
