//! AiComputerPlayerTurn steps: squeeze-outs, position assessment, rival credit freezes, holding-company
//! purchase, corporate bond trading, frozen-loan repayment.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::buyer::{show_pending_msg, validate_buyer_corp};
use crate::logic::control::update_corp_control;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_bond_holding_slot;
use crate::logic::newgame_helpers::{create_insurer, find_free_entity_slot, sell_loan_to_bank};
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::fin_inst::{buy_corp_bonds as fi_buy_corp_bonds, sell_corp_bonds as fi_sell_corp_bonds};
use crate::logic::sim_entity::transactions::sell_stock_holding;
use crate::logic::valuation::{adjust_index_divisor, calc_stock_price, check_stock_split};
use crate::pb::string::{CaseMode, ucase};


fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Port of AiPlayer_SqueezeOutMinority (0x5c3730), GOSUB of AiComputerPlayerTurn.
/// When the player controls `AiTargetCorp` with no public float, the largest holder below 100%, no
/// buyout this or last year and cash above 10x the target's cap (`buyoutMode = 1`): every other company
/// holding target stock that the player controls sells its stake to the target (SellStockHolding mode 1;
/// a sale ends the search, `buyoutMode = 0`); a holder smaller than 1.4x the target's cap (public
/// float >= 72%, or controlled by the player) is merged into the target (MergeCompanies, `AutoBuyoutYear`
/// while still in buyout mode); in buyout mode a random holding of the target (< 100%) is merged into it
/// and the scan ends.
pub(crate) fn squeeze_out_minority(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if !(10 < g.ai_target_corp) {
        return;
    }
    update_corp_control(g, ui, rng, g.ai_target_corp);
    if f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    if !(g.public_pct == 0 && g.largest_holder_pct < 100 && g.auto_buyout_year == 0) {
        return;
    }
    if !(g.market_cap[g.ai_target_corp] * Ext::from_f64(10.0) < g.cash[f.player_id]) {
        return;
    }
    f.buyout_mode = 1;
    f.ent_id = 11;
    while f.ent_id < 1601 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && f.buyout_mode != 0 && g.ai_target_corp != e {
            update_corp_control(g, ui, rng, e);
            f.slot = 1;
            while f.slot < 16 {
                if cast::<i32, _>(g.hold_stock_id[(f.slot, f.ent_id)]) == g.ai_target_corp {
                    if f.player_id == g.corp_controller[f.ent_id] {
                        g.tmp_ext1 = cast::<Ext, _>(f.ent_id);
                        g.tmp_ext2 = cast::<Ext, _>(g.hold_stock_pct[(f.slot, f.ent_id)]);
                        g.tmp_ext3 = Ext::from_f64(1.0);
                        sell_stock_holding(g, ui, rng, g.ai_target_corp);
                        f.result_amt = to_quad(g.tmp_ext1);
                        g.tmp_ext1 = Ext::from_f64(0.0);
                        if 0 < f.result_amt {
                            calc_projected_earnings(g, ui, rng, g.ai_target_corp);
                            f.buyout_mode = 0;
                            f.slot = 16;
                            continue;
                        }
                    }
                    if (!(g.public_float_pct < 72) || f.player_id == g.corp_controller[f.ent_id])
                        && g.market_cap[f.ent_id] < g.market_cap[g.ai_target_corp] * sf(1.4f32)
                    {
                        f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(15.0) + Ext::from_f64(23.0));
                        crate::logic::restructure::merge_companies(g, ui, rng, g.ai_target_corp, f.ent_id, 1, cast::<i16, _>(f.rnd_val), f.time_secs);
                        f.buyout_mode = 0;
                        update_corp_control(g, ui, rng, g.ai_target_corp);
                        if g.public_pct == 0
                            && g.largest_holder_pct < 100
                            && g.market_cap[g.ai_target_corp] * Ext::from_f64(10.0) < g.cash[f.player_id]
                        {
                            f.buyout_mode = 1;
                        }
                        if f.buyout_mode == 1 {
                            g.auto_buyout_year = g.cur_year;
                        }
                    }
                    if f.buyout_mode == 1 {
                        let t = g.ai_target_corp;
                        if 0 < g.hold_stock_id[(0, t)] {
                            f.pick_id = rng.rnd_range(cast::<i32, _>(g.hold_stock_id[(0, t)]), 1);
                            if g.hold_stock_pct[(f.pick_id, t)] < 100 {
                                f.other_corp_id = cast::<i32, _>(g.hold_stock_id[(f.pick_id, t)]);
                                crate::logic::restructure::merge_companies(g, ui, rng, g.ai_target_corp, f.other_corp_id, 1, 0, f.time_secs);
                                update_corp_control(g, ui, rng, g.ai_target_corp);
                                if 0 < g.public_pct {
                                    f.buyout_mode = 0;
                                }
                                f.slot = 15;
                                f.ent_id = 1600;
                            }
                        }
                    }
                }
                f.slot += 1;
            }
        }
        f.ent_id += 1;
    }
}

/// Port of AiPlayer_AssessPosition (0x5c3c65), GOSUB of AiComputerPlayerTurn.
/// Compares the player's net worth with player 2's (sic, BUGS.md #264); at difficulty >= 2 in a
/// two-player game where the AI is not far ahead (`ownNetWorth <= rival * difficulty * 0.05`) the AI
/// cheats (`pickId = 1`). Counts the companies it controls (`PlayerCorpsControlled`), tracks the largest
/// liquid funds of a controlled company with a free holding slot (`maxLiquid`), and per controlled
/// company: when cheating, 1 in 5 frees it of the asbestos/superfund liability; once a year 1 in 4 of
/// its operating companies gets `pickId` added to the profit margin.
pub(crate) fn assess_position(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    calc_entity_financials(g, ui, rng, 2);
    f.rival_net_worth = g.net_worth;
    calc_entity_financials(g, ui, rng, f.player_id);
    f.own_net_worth = g.net_worth;
    f.pick_id = if 1 < g.difficulty_level { 1 } else { 0 };
    if f.rival_net_worth * (cast::<Ext, _>(g.difficulty_level) * sf(0.05f32)) < f.own_net_worth && g.num_players == 2 {
        f.pick_id = 0;
    }
    if 2 < g.num_players {
        f.pick_id = 0;
    }
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] {
            f.corps_controlled += 1;
            calc_entity_financials(g, ui, rng, e);
            if e < 86 && 0 < g.hold_stock_pct[(1, e)] {
                g.liquid_funds = Ext::from_f64(0.0);
            } else {
                g.liquid_funds = g.t_bills[e] + g.cash[e] + g.unused_credit_line * sf(0.25f32);
            }
            if f.max_liquid < g.liquid_funds && g.hold_stock_id[(0, e)] < 15 {
                f.max_liquid = g.liquid_funds;
            }
            let mut boost = true;
            if rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(4.0) && f.pick_id == 1 {
                if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(e) {
                    g.asbestos_corp = 0;
                    boost = false;
                } else if cast::<i32, _>(g.superfund_corp) == e {
                    g.superfund_corp = 0;
                    g.superfund_end_year = 0;
                }
            }
            if boost && g.industry_num[e] < 70 && g.ai_mgmt_boost_year != g.cur_year && rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(3.0) {
                g.profit_margin[e] = (cast::<i16, _>(f.pick_id)).wrapping_add(g.profit_margin[e]);
            }
        }
        f.ent_id += 1;
    }
    g.player_corps_controlled[f.player_id] = f.corps_controlled;
    g.ai_mgmt_boost_year = g.cur_year;
}

/// Port of AiPlayer_FreezeRivalCredit (0x5c3fc7), GOSUB of AiComputerPlayerTurn ("500").
/// Picks a random human player (not itself) when `AiTargetCorp` > 10. If the AI controls the rival's
/// lending bank: freezes the credit line of a rival with bank debt above 3x its cash + T-bills and a
/// rating below 7 (headline and message box), and at difficulty >= 2 with a rating below 6 and cash +
/// T-bills at most a third of the debt calls in half of the loan (headline, message box, one action);
/// a frozen line is renewed for a rating above 8. If not, it only works out who controls the bank
/// (`lenderCtrlId`; unused, see BUGS.md #265).
pub(crate) fn freeze_rival_credit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = s("500");
    if g.ai_target_corp < 11 {
        return;
    }
    f.pick_id = rng.rnd_range(g.num_players, 1);
    if g.entity_status[f.pick_id] != 1 || f.player_id == f.pick_id {
        return;
    }
    if g.is_computer_player[f.pick_id] == 1 {
        return;
    }
    calc_entity_financials(g, ui, rng, f.pick_id);
    f.other_id = cast::<i32, _>(g.lending_bank[f.pick_id]);
    if f.player_id != g.corp_controller[f.other_id] {
        if g.credit_rating_num[f.pick_id] < 6 {
            f.lender_ctrl_id = f.other_id;
            update_corp_control(g, ui, rng, f.other_id);
            if !(g.largest_holder_pct < g.public_pct) {
                f.lender_ctrl_id = 0;
                f.slot = cast::<i32, _>(g.largest_holder);
                if 10 < f.slot && g.corp_controller[f.other_id] == g.corp_controller[f.slot] {
                    update_corp_control(g, ui, rng, f.slot);
                    if g.largest_holder_pct < g.public_pct && f.player_id != g.corp_controller[f.slot] {
                        f.lender_ctrl_id = f.slot;
                    }
                }
                if f.lender_ctrl_id == 0 {
                    f.slot = g.corp_controller[f.other_id];
                    if 10 < f.slot {
                        update_corp_control(g, ui, rng, f.slot);
                        if g.largest_holder_pct < g.public_pct {
                            f.lender_ctrl_id = f.slot;
                        }
                    }
                }
            }
        }
        return;
    }
    f.pending_msg = PbStr::new();
    let p = f.pick_id;
    if (g.t_bills[p] + g.cash[p]) * Ext::from_f64(3.0) < g.bank_debt[p] && g.credit_rating_num[p] < 7 {
        if g.credit_frozen[p] == 0 {
            g.news_msg = s(__content.string(1910)) + &g.ent_symbol[p] + " !!!!!";
            f.pending_msg = ucase(g.news_msg.as_bytes(), CaseMode::Ansi);
            post_news(g, ui, 0, 0);
        }
        g.credit_frozen[p] = 1;
        if !f.pending_msg.is_empty() {
            show_pending_msg(g, ui, f);
        }
        if g.difficulty_level < 2 {
            return;
        }
        if 5 < g.credit_rating_num[p] {
            return;
        }
        if !(g.t_bills[p] + g.cash[p] <= g.bank_debt[p] * sf(0.33f32)) {
            return;
        }
        f.amount = g.bank_debt[p] * sf(0.5f32);
        let o = f.other_id;
        g.cash[o] = f.amount + g.cash[o];
        g.cash[p] = g.cash[p] - f.amount;
        g.bank_debt[p] = g.bank_debt[p] - f.amount;
        f.field_text = g.ent_symbol[1].clone() + "'s bank";
        if g.loan_call_news_count % 2 == 0 {
            f.field_text = s(__content.string(1911)) + &g.ent_symbol[cast::<i32, _>(g.lending_bank[p])] + ")";
        }
        g.news_msg = f.field_text.clone() + __content.string(1912) + &g.ent_symbol[p] + " loan";
        f.pending_msg = s("\r")
            + "\r"
            + __content.string(1913)
            + __content.string(1914);
        if 0 < g.loan_call_news_count {
            f.pending_msg = PbStr::new();
        }
        f.pending_msg = g.news_msg.clone() + "!" + &f.pending_msg;
        g.loan_call_news_count += 1;
        post_news(g, ui, f.pick_id, 0);
        g.turn_action_count += 1;
        show_pending_msg(g, ui, f);
        return;
    }
    if g.credit_frozen[p] == 1 && 8 < g.credit_rating_num[p] {
        g.credit_frozen[p] = 0;
        g.news_msg = g.ent_symbol[f.other_id].clone() + __content.string(1915) + &g.ent_symbol[p];
        post_news(g, ui, f.other_id, f.pick_id);
    }
}

/// The two acquisition scans of AiPlayer_AcquireHoldingCo: candidate `entId` checks and the purchase.
/// Returns true if the player now holds more than 64% of it in slot 1.
fn acquire_scan_candidate(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
    nw_factor: Ext,
    tag: &str,
) -> bool {
    let e = f.ent_id;
    update_corp_control(g, ui, rng, e);
    if !(64 < g.public_pct) {
        return false;
    }
    calc_entity_financials(g, ui, rng, e);
    if g.credit_rating_num[e] < 8 {
        return false;
    }
    if !(g.market_cap[e] * Ext::from_f64(100.0) <= g.net_worth * nw_factor) {
        return false;
    }
    let p = f.player_id;
    if !(g.market_cap[e] * Ext::from_f64(66.0) <= g.t_bills[p] + g.cash[p]) {
        return false;
    }
    f.target_pct = 65;
    f.pick_id = cast::<i32, _>(g.public_pct);
    if 65 < f.pick_id && g.market_cap[e] * (Ext::from_f64(1.0) + cast::<Ext, _>(f.pick_id)) < g.t_bills[p] + g.cash[p] {
        f.target_pct = f.pick_id;
    }
    f.buyer_corp = f.player_id;
    f.target_id = f.ent_id;
    validate_buyer_corp(g, ui, rng, f);
    f.trace_tag = s(tag);
    cast::<i32, _>(g.hold_stock_id[(1, f.player_id)]) == f.ent_id && 64 < g.hold_stock_pct[(1, f.player_id)]
}

/// Port of AiPlayer_AcquireHoldingCo (0x5c4828), GOSUB of AiComputerPlayerTurn ("1000").
/// Gets the player a main company. Draws 10% of the unused credit line (rating > 9) — or, in buyout mode
/// (`buyoutMode = 1`, debt >= 1), repays bank debt from cash and switches to `buyoutMode = 2`. Then buys
/// 65% (or the whole public float when affordable) of a widely held, well rated insurer (86..130) or
/// industry-70 company (when cash + T-bills >= 100 and no target yet) worth at most 0.95 / 0.85 % of its
/// net worth (`AiPlayer_ValidateBuyerCorp`); with none: 2 in 3 found a new insurer (121..130, cash +
/// T-bills >= 1000) with all its cash, else 1 in 6 sets up a holding company (1471..1580, cash >= 100)
/// with all its cash. The purchase is `AiTargetCorp`.
pub(crate) fn acquire_holding_co(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = s("1000");
    if f.buyout_mode == 1 && g.bank_debt[f.player_id] < Ext::from_f64(1.0) {
        return;
    }
    calc_entity_financials(g, ui, rng, f.player_id);
    if Ext::from_f64(0.0) < g.unused_credit_line && 9 < g.credit_rating_num[f.player_id] {
        f.amount = g.unused_credit_line * sf(0.1f32);
    } else {
        f.amount = Ext::from_f64(0.0);
    }
    let p = f.player_id;
    if f.buyout_mode == 1 {
        f.amount = g.bank_debt[p];
        if g.cash[p] < f.amount {
            f.amount = g.cash[p];
        }
        f.amount = -f.amount;
        if Ext::from_f64(0.0) < f.amount {
            f.amount = Ext::from_f64(0.0);
        }
    }
    g.cash[p] = f.amount + g.cash[p];
    let l = g.lender_id;
    g.cash[l] = g.cash[l] - f.amount;
    g.bank_debt[p] = f.amount + g.bank_debt[p];
    if f.buyout_mode == 1 && (g.bank_debt[p] == Ext::from_f64(0.0) || f.amount != Ext::from_f64(0.0)) {
        f.buyout_mode = 2;
        return;
    }
    // insurers
    f.ent_id = 86;
    while f.ent_id <= 130 {
        if g.entity_status[f.ent_id] == 1 && acquire_scan_candidate(g, ui, rng, f, sf(0.95f32), __content.string(1916)) {
            let e = f.ent_id;
            g.ai_target_corp = e;
            if g.profit_margin[e] < -7 {
                g.profit_margin[e] += 1;
            }
            f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(3.0));
            g.growth_rate[e] = cast::<i16, _>(f.rnd_val);
            f.ent_id = 130;
        }
        f.ent_id += 1;
    }
    // holding companies (industry 70)
    if Ext::from_f64(100.0) <= g.t_bills[p] + g.cash[p] && g.ai_target_corp == 0 {
        f.ent_id = 131;
        while f.ent_id <= 1600 {
            if g.entity_status[f.ent_id] == 1
                && g.industry_num[f.ent_id] == 70
                && acquire_scan_candidate(g, ui, rng, f, sf(0.85f32), __content.string(1917))
            {
                g.ai_target_corp = f.ent_id;
                f.ent_id = 1600;
            }
            f.ent_id += 1;
        }
    }
    if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) {
        if rng.rnd() * Ext::from_f64(2.0) < Ext::from_f64(1.0) {
            if Ext::from_f64(0.0) < g.t_bills[p] {
                g.cash[p] = g.t_bills[p] + g.cash[p];
                g.t_bills[p] = Ext::from_f64(0.0);
            }
            if g.exchange_rate * Ext::from_f64(100.0) <= g.cash[p] && g.ai_target_corp == 0 && g.hold_stock_id[(1, p)] == 0 {
                f.ent_id = 1471;
                while f.ent_id <= 1580 {
                    if g.entity_status[f.ent_id] != 1 {
                        set_up_holding_company(g, ui, rng, f);
                        f.ent_id = 1600;
                    }
                    f.ent_id += 1;
                }
            }
        }
    } else if g.exchange_rate * Ext::from_f64(1000.0) <= g.t_bills[p] + g.cash[p] && g.ai_target_corp == 0 && g.hold_stock_id[(1, p)] == 0 {
        f.ent_id = 121;
        while f.ent_id <= 130 {
            if g.entity_status[f.ent_id] != 1 {
                found_insurer(g, ui, rng, f);
                f.ent_id = 130;
            }
            f.ent_id += 1;
        }
    }
}

/// AiPlayer_AcquireHoldingCo: the player founds insurer `entId` with all its cash.
fn found_insurer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let p = f.player_id;
    g.cash[p] = g.cash[p] - g.exchange_rate * Ext::from_f64(1000.0);
    if Ext::from_f64(0.0) < g.t_bills[p] {
        g.cash[p] = g.t_bills[p] + g.cash[p];
        g.t_bills[p] = Ext::from_f64(0.0);
    }
    f.cash_amt = to_quad(g.cash[p]);
    if f.cash_amt < 0 {
        f.cash_amt = 0;
    }
    g.tmp_ext1 = g.exchange_rate * Ext::from_f64(1000.0);
    g.tmp_ext2 = g.exchange_rate * Ext::from_f64(300.0);
    create_insurer(g, ui, rng, f.ent_id, 1);
    let e = f.ent_id;
    g.ai_target_corp = e;
    g.hold_stock_id[(1, p)] = cast::<i16, _>(e);
    g.hold_stock_pct[(1, p)] = 100;
    g.cash[e] = cast::<Ext, _>(f.cash_amt) + g.cash[e];
    if 0 < f.cash_amt {
        g.cash[p] = Ext::from_f64(0.0);
    }
    g.hold_stock_basis[(1, p)] = cast::<Ext, _>(f.cash_amt) + g.exchange_rate * Ext::from_f64(1000.0);
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, e, f.time_secs);
    g.turn_action_count += 1;
    f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(8.0) - Ext::from_f64(4.0));
    g.profit_margin[e] = cast::<i16, _>(f.rnd_val);
    f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(4.0) + Ext::from_f64(2.0));
    g.growth_rate[e] = cast::<i16, _>(f.rnd_val);
}

/// AiPlayer_AcquireHoldingCo: the player [runtime resource wording]  `entId` (industry 70) with all its cash.
fn set_up_holding_company(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let p = f.player_id;
    let e = f.ent_id;
    f.cash_amt = to_quad(g.cash[p]);
    g.cash[e] = cast::<Ext, _>(f.cash_amt);
    g.cash[p] = Ext::from_f64(0.0);
    f.other_id = to_long(g.exchange_rate * Ext::from_f64(10.0));
    g.cash[e] = g.cash[e] - cast::<Ext, _>(f.other_id);
    g.accrued_pl[e] = g.accrued_pl[e] - cast::<Ext, _>(f.other_id);
    g.ent_misc_data[e] = PbStr::new();
    g.industry_num[e] = 70;
    g.market_cap[e] = cast::<Ext, _>(f.cash_amt) * sf(0.01f32);
    while g.lending_bank[f.ent_id] < 11 {
        f.slot = rng.rnd_range(85, 11);
        if g.entity_status[f.slot] == 1 {
            g.lending_bank[f.ent_id] = cast::<i16, _>(f.slot);
        }
    }
    g.shares_outstanding[e] = Ext::from_f64(1.0);
    g.entity_status[e] = 1;
    g.tmp_ext3 = cast::<Ext, _>(f.cash_amt);
    adjust_index_divisor(g, e, 0);
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, e, f.time_secs);
    check_stock_split(g, e);
    f.industry = 70;
    if 1500 < f.ent_id {
        f.other_corp_id = find_free_entity_slot(g, rng, f.ent_id, f.industry);
    }
    let e = f.ent_id;
    g.ai_target_corp = e;
    g.hold_stock_id[(1, p)] = cast::<i16, _>(e);
    g.hold_stock_pct[(1, p)] = 100;
    g.hold_stock_basis[(1, p)] = cast::<Ext, _>(f.cash_amt);
    g.turn_action_count += 1;
    g.news_msg = g.ent_symbol[p].clone()
        + __content.string(1918)
        + &g.ent_name[e]
        + " ("
        + &g.ent_symbol[e]
        + ")";
    post_news(g, ui, 0, 0);
}

/// Port of AiPlayer_BuyCorpBonds (0x5c5a9d), GOSUB of AiComputerPlayerTurn.
/// Only for player 1 (`playerId < 2`, see BUGS.md #266) and when the base rate is at least 9..12: every
/// controlled bank/insurer (11..130) with cash >= 200 and a free bond slot buys 20% of its cash (at least
/// 100) of every convertible bond it does not hold yet whose issuer has positive projected earnings, a
/// price <= 95, a rating 4..8 and publicly held bonds of at least 2.5x the amount (BuyCorpBonds mode 1).
pub(crate) fn buy_corp_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if 1 < f.player_id {
        return;
    }
    if !(rng.rnd() * Ext::from_f64(3.0) + Ext::from_f64(9.0) <= cast::<Ext, _>(g.base_interest_rate)) {
        return;
    }
    f.ent_id = 11;
    while f.ent_id <= 130 {
        let e = f.ent_id;
        if g.entity_status[e] == 1
            && f.player_id == g.corp_controller[e]
            && Ext::from_f64(200.0) <= g.cash[e]
            && g.hold_bond_id[(0, e)] != 15
        {
            f.slot = 11;
            while f.slot <= 1600 {
                let sl = f.slot;
                if f.ent_id != sl
                    && g.entity_status[sl] == 1
                    && g.bonds_issued[sl] != 0
                    && instr(1, g.bond_issue_info[sl].as_bytes(), b"cv") != 0
                    && Ext::from_f64(0.0) < g.proj_qtr_earnings[sl]
                {
                    f.bond_slot = find_bond_holding_slot(g, f.ent_id, sl);
                    if f.slot != g.hold_bond_id[(f.bond_slot, f.ent_id)] {
                        calc_entity_financials(g, ui, rng, sl);
                        if g.bond_price[sl].get() <= Ext::from_f64(95.0) {
                            f.bond_amt = to_quad(g.cash[f.ent_id] * sf(0.2f32));
                            if f.bond_amt < 100 {
                                f.bond_amt = 100;
                            }
                            if cast::<Ext, _>(f.bond_amt) * sf(2.5f32) <= cast::<Ext, _>(g.bonds_public_held)
                                && 3 < g.credit_rating_num[sl]
                                && g.credit_rating_num[sl] < 9
                            {
                                g.tmp_ext1 = cast::<Ext, _>(f.bond_amt);
                                g.tmp_ext2 = Ext::from_f64(1.0);
                                fi_buy_corp_bonds(g, ui, rng, f.ent_id, f.slot);
                            }
                        }
                    }
                }
                f.slot += 1;
            }
        }
        f.ent_id += 1;
    }
    f.bond_slot = 0;
}

/// Port of AiPlayer_SellCorpBonds (0x5c5e5c), GOSUB of AiComputerPlayerTurn ("2000").
/// With the base rate at most 5.5: the player's banks and insurers (11..130) sell each bond holding with
/// probability 1/2 (SellCorpBonds mode 1, whole face), at most 4 sales (after 3 only `AiTargetCorp`
/// sells); one action if anything was sold.
pub(crate) fn sell_corp_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = s("2000");
    if !(sf(5.5f32) >= cast::<Ext, _>(g.base_interest_rate)) {
        return;
    }
    f.trade_count = 0;
    f.slot = 11;
    while f.slot <= 130 {
        let sl = f.slot;
        if g.entity_status[sl] == 1
            && f.player_id == g.corp_controller[sl]
            && (f.trade_count != 3 || g.ai_target_corp == sl)
        {
            f.ent_id = 1;
            while f.ent_id <= 15 {
                if 0 < g.hold_bond_id[(f.ent_id, f.slot)] && rng.rnd() * Ext::from_f64(2.0) > Ext::from_f64(1.0) {
                    f.target_id = g.hold_bond_id[(f.ent_id, f.slot)];
                    g.tmp_ext1 = cast::<Ext, _>(g.hold_bond_face[(f.ent_id, f.slot)]);
                    g.tmp_ext2 = Ext::from_f64(1.0);
                    fi_sell_corp_bonds(g, ui, rng, f.slot, f.target_id);
                    f.trade_count += 1;
                    if f.trade_count == 4 {
                        f.ent_id = 15;
                        f.slot = 130;
                    }
                }
                f.ent_id += 1;
            }
        }
        f.slot += 1;
    }
    if 0 < f.trade_count {
        g.turn_action_count += 1;
    }
}

/// Port of AiPlayer_RepayFrozenLoan (0x5c603a), GOSUB of AiComputerPlayerTurn.
/// With a frozen credit line: repays 10% of the bank debt (all of it if cash + T-bills cover it) when
/// cash + T-bills exceed 30% of it; once the debt is paid off switches to a bank it controls (or the
/// first bank controlled by a company), unfreezes and announces it. Otherwise, if its lending bank
/// (`LenderId`) is not its own: finds a bank it controls (or a company-controlled bank) that can take
/// over the loan (cash of both >= 1.02 x debt) and sells the loan to it (SellLoanToBank), or with no
/// debt just switches banks.
pub(crate) fn repay_frozen_loan(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let p = f.player_id;
    if g.credit_frozen[p] == 1 {
        if g.bank_debt[p] * sf(0.3f32) < g.t_bills[p] + g.cash[p] {
            f.pay_amt = g.bank_debt[p] * sf(0.1f32);
            if g.bank_debt[p] < g.t_bills[p] + g.cash[p] {
                f.pay_amt = g.bank_debt[p];
            }
            g.cash[p] = g.cash[p] - f.pay_amt;
            let lb = cast::<i32, _>(g.lending_bank[p]);
            g.cash[lb] = f.pay_amt + g.cash[lb];
            g.bank_debt[p] = g.bank_debt[p] - f.pay_amt;
        }
        if g.bank_debt[p].abs() < sf(0.0001f32) {
            f.pick_id = 0;
            f.ent_id = 11;
            while f.ent_id <= 85 {
                let e = f.ent_id;
                if g.entity_status[e] == 1 {
                    if 10 < g.corp_controller[e] && f.pick_id == 0 {
                        f.pick_id = e;
                    }
                    if f.player_id == g.corp_controller[e] {
                        f.pick_id = e;
                        f.ent_id = 85;
                    }
                }
                f.ent_id += 1;
            }
            if 10 < f.pick_id && f.pick_id < 86 {
                g.lending_bank[p] = cast::<i16, _>(f.pick_id);
                g.credit_frozen[p] = 0;
                g.news_msg = g.ent_symbol[p].clone() + __content.string(1919) + &g.ent_symbol[f.pick_id];
                let mut m = g.news_msg.clone();
                show_quick_message(g, ui, &mut m);
                g.news_msg = m;
                post_news(g, ui, 0, 0);
            }
        }
        return;
    }
    g.lender_id = cast::<i32, _>(g.lending_bank[p]);
    if f.player_id == g.corp_controller[g.lender_id] {
        return;
    }
    f.pick_id = 0;
    f.other_id = 0;
    f.ent_id = 11;
    while f.ent_id <= 85 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 {
            if 10 < g.corp_controller[e] && f.other_id == 0 && g.bank_debt[p] * sf(1.02f32) <= g.cash[e] + g.cash[p] {
                f.other_id = e;
            }
            if f.player_id == g.corp_controller[e] && f.pick_id == 0 && g.bank_debt[p] * sf(1.02f32) < g.cash[p] + g.cash[e] {
                f.pick_id = e;
                f.ent_id = 85;
            }
        }
        f.ent_id += 1;
    }
    if 10 < f.pick_id {
        f.other_id = f.pick_id;
    }
    if 10 < f.other_id {
        if g.bank_debt[p] <= sf(0.0001f32) {
            let c = g.corp_controller[g.lender_id];
            if 0 < c && c < 11 {
                g.lending_bank[p] = cast::<i16, _>(f.other_id);
                return;
            }
            if f.player_id == g.corp_controller[f.other_id] {
                g.lending_bank[p] = cast::<i16, _>(f.other_id);
            }
        } else {
            calc_entity_financials(g, ui, rng, f.player_id);
            sell_loan_to_bank(g, ui, rng, f.other_id, g.lender_id, f.player_id);
        }
    }
}
