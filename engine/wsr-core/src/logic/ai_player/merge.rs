//! AiComputerPlayerTurn steps: mergers, stock offerings, credit-rating repair, public-share buyouts,
//! tax-loss mergers, antitrust divestitures.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::buyer::{add_recent_offering, ai_find_affordable_buyer, validate_buyer_corp};
use crate::logic::control::update_corp_control;
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials};
use crate::logic::holdings::get_largest_holding_id;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::divest_stock;
use crate::logic::reports_lists::update_control_chain;
use crate::logic::sim_entity::ai_corp::{ai_random_lbo, is_recent_offering};
use crate::logic::sim_entity::bonds::{issue_stock, normalize_holding_pcts};
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::sim_entity::transactions::sell_stock_holding;
use crate::logic::valuation::calc_industry_market_share;

/// Port of AiPlayer_MergeControlledCorps (0x5cd435), GOSUB of AiComputerPlayerTurn ("11000").
/// For a player controlling its `AiTargetCorp`:
/// 1. a company of the lender `lenderCtrlId` (from FreezeRivalCredit): buys a stake above its largest
///    holder through `AiFindAffordableBuyer` / `ValidateBuyerCorp` (done when bought);
/// 2. with more than 60% of `AiTargetCorp` and a lender that is 75%+ public and worth less than 1.25x
///    `AiTargetCorp`: merges the lender into `AiTargetCorp` (done unless control was lost);
/// 3. from a random start (131..1100), two controlled companies of the same industry (the one holding
///    the other becomes the target; never a 100% / [runtime resource wording] ): merges them, and liquidates
///    a now wholly-owned target into its parent;
/// 4. else the controlled company 86..1600 (sic: `EntityStatus = playerId`, see BUGS.md #273) with the
///    largest market cap that is over 50% held and cheap (`5 * NetWorth <= 100 * MarketCap`) merges a
///    random 75%+ public company of good credit (rating >= 5, `0.77 * NetWorth >= 100 * MarketCap`)
///    whose `100 * MarketCap` is at most its controller stake value.
pub(crate) fn merge_controlled_corps(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if g.ai_target_corp == 0 {
        return;
    }
    if f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    pack_holding_arrays(g, ui);
    if 10 < f.lender_ctrl_id {
        f.target_id = f.lender_ctrl_id;
        update_corp_control(g, ui, rng, f.target_id);
        f.target_pct = 20;
        if 19 < g.largest_holder_pct {
            f.target_pct = cast::<i32, _>(g.largest_holder_pct) + 1;
        }
        if (cast::<i32, _>(g.public_pct)) < f.target_pct {
            return;
        }
        f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, f.target_id, f.target_pct, 0);
        if 0 < f.buyer_corp {
            validate_buyer_corp(g, ui, rng, f);
            if 0 < f.buyer_corp {
                return;
            }
        }
    }
    let t = g.ai_target_corp;
    if cast::<i32, _>(g.hold_stock_id[(1, f.player_id)]) == t
        && 60 < g.hold_stock_pct[(1, f.player_id)]
        && 10 < f.lender_ctrl_id
    {
        update_corp_control(g, ui, rng, f.lender_ctrl_id);
        if 75 < g.public_pct && g.market_cap[f.lender_ctrl_id] * sf(0.8f32) < g.market_cap[g.ai_target_corp] {
            crate::logic::restructure::merge_companies(g, ui, rng, g.ai_target_corp, f.lender_ctrl_id, 1, 0, f.time_secs);
            let a = g.ai_target_corp;
            update_control_chain(g, ui, rng, a);
            update_control_chain(g, ui, rng, f.lender_ctrl_id);
            if f.player_id == g.corp_controller[f.lender_ctrl_id] {
                return;
            }
            if f.player_id != g.corp_controller[g.ai_target_corp] {
                return;
            }
        }
    }
    f.acquirer_id = 0;
    f.target_id = 0;
    f.industry = 0;
    f.other_id = rng.rnd_range(1100, 131);
    f.ent_id = f.other_id;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] && g.ai_target_corp != e {
            if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) || f.corps_controlled <= 5 {
                if f.acquirer_id == 0 {
                    f.acquirer_id = e;
                    f.industry = g.industry_num[f.acquirer_id];
                } else if g.industry_num[e] == f.industry {
                    f.target_id = e;
                    update_corp_control(g, ui, rng, f.target_id);
                    if cast::<i32, _>(g.wholly_owned_by) == f.acquirer_id {
                        f.target_id = 0;
                    } else {
                        f.slot = 1;
                        while f.slot <= 15 {
                            if cast::<i32, _>(g.hold_stock_id[(f.slot, f.target_id)]) == f.acquirer_id {
                                if g.hold_stock_pct[(f.slot, f.target_id)] == 100 {
                                    f.target_id = 0;
                                } else {
                                    std::mem::swap(&mut f.acquirer_id, &mut f.target_id);
                                }
                            }
                            f.slot += 1;
                        }
                    }
                    if 10 < f.target_id {
                        f.ent_id = 1600;
                    }
                }
            }
        }
        f.ent_id += 1;
    }
    if f.target_id == f.acquirer_id {
        f.target_id = 0;
    }
    if 10 < f.acquirer_id && 10 < f.target_id {
        crate::logic::restructure::merge_companies(g, ui, rng, f.acquirer_id, f.target_id, 1, 0, f.time_secs);
        update_control_chain(g, ui, rng, f.acquirer_id);
        if g.entity_status[f.target_id] != 1 {
            return;
        }
        update_control_chain(g, ui, rng, f.target_id);
        update_corp_control(g, ui, rng, f.target_id);
        if cast::<i32, _>(g.wholly_owned_by) == f.acquirer_id {
            g.tmp_ext1 = Ext::from_f64(1.0);
            crate::logic::restructure::liquidate_sub_into_parent(g, ui, rng, f.acquirer_id, f.target_id, f.time_secs);
            return;
        }
    }
    f.x_c8 = Ext::from_f64(0.0);
    f.best_corp_id = 0;
    f.acquirer_id = 0;
    f.target_id = 0;
    f.bond_amt = 0;
    f.result_amt = 0;
    f.ent_id = 86;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        // sic: EntityStatus compared with the player's id (BUGS.md #273)
        if cast::<i32, _>(g.entity_status[e]) == f.player_id && f.player_id == g.corp_controller[e] && g.ai_target_corp != e {
            f.result_amt = to_quad(g.market_cap[e]);
            if f.x_c8 < cast::<Ext, _>(f.result_amt) {
                calc_entity_financials(g, ui, rng, e);
                if g.net_worth * Ext::from_f64(5.0) <= g.market_cap[e] * Ext::from_f64(100.0) {
                    update_corp_control(g, ui, rng, e);
                    if 50 < g.largest_holder_pct {
                        if (cast::<Ext, _>(f.bond_amt)) < g.market_cap[e] * cast::<Ext, _>(g.largest_holder_pct) {
                            f.best_corp_id = e;
                            f.x_c8 = cast::<Ext, _>(f.result_amt);
                            f.bond_amt = to_quad(g.market_cap[e] * cast::<Ext, _>(g.largest_holder_pct));
                        }
                    }
                }
            }
        }
        f.ent_id += 1;
    }
    f.acquirer_id = f.best_corp_id;
    if f.acquirer_id < 86 {
        return;
    }
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1
            && g.market_cap[e] * Ext::from_f64(100.0) <= cast::<Ext, _>(f.bond_amt)
            && rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0)
        {
            update_corp_control(g, ui, rng, e);
            if !(g.public_pct < 75 || cast::<i32, _>(g.parent80_owner) == f.acquirer_id) {
                f.target_id = e;
                f.slot = 1;
                while f.slot <= 15 {
                    if cast::<i32, _>(g.hold_stock_id[(f.slot, e)]) == f.acquirer_id && 10 < g.hold_stock_pct[(f.slot, e)] {
                        f.target_id = 0;
                    }
                    f.slot += 1;
                }
                if f.target_id != 0 {
                    calc_entity_financials(g, ui, rng, e);
                    if g.credit_rating_num[e] < 5 || g.net_worth * sf(0.77f32) < g.market_cap[e] * Ext::from_f64(100.0) {
                        f.target_id = 0;
                    }
                    if 0 < f.target_id {
                        f.ent_id = 1600;
                    }
                }
            }
        }
        f.ent_id += 1;
    }
    if 10 < f.acquirer_id && 10 < f.target_id {
        crate::logic::restructure::merge_companies(g, ui, rng, f.acquirer_id, f.target_id, 1, 0, f.time_secs);
        update_control_chain(g, ui, rng, f.acquirer_id);
        update_control_chain(g, ui, rng, f.target_id);
    }
}

/// Port of AiPlayer_HostileMerger (0x5cdf0e), GOSUB of AiComputerPlayerTurn ("11500").
/// At difficulty 2+, at most once a quarter (`AiHostileMergerQtr`), and only while there is an
/// asbestos / superfund company: picks a random human rival (2..NumPlayers), its largest holding (or,
/// if the rival controls that one, its largest holding when weakly held and worth more than half the
/// company's assets), and dumps the toxic company on it: an AI company buys 20% of the toxic company
/// (`AiFindAffordableBuyer` for 21%, `ValidateBuyerCorp`), and if the current player then controls it,
/// it merges the rival's company into the toxic one ("… has acquired … in a hostile merger."); the
/// stake bought is sold again.
pub(crate) fn hostile_merger(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    if g.difficulty_level < 2 || g.ai_target_corp == 0 || g.ai_hostile_merger_qtr == -1 {
        return;
    }
    if g.cur_quarter == g.ai_hostile_merger_qtr {
        return;
    }
    g.ai_hostile_merger_qtr = 0;
    if (cast::<Ext, _>(g.asbestos_corp)).abs() == Ext::from_f64(0.0) && g.superfund_corp == 0 {
        return;
    }
    f.other_id = rng.rnd_range(g.num_players, 2);
    if g.is_computer_player[f.other_id] == 1 {
        return;
    }
    f.largest_hold_id = cast::<i16, _>(get_largest_holding_id(g, f.other_id));
    f.target_id = cast::<i32, _>(f.largest_hold_id);
    if f.target_id == 0 {
        return;
    }
    if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(f.target_id) || cast::<i32, _>(g.superfund_corp) == f.target_id {
        return;
    }
    pack_holding_arrays(g, ui);
    update_corp_control(g, ui, rng, f.target_id);
    let weak = g.largest_holder_pct < 29 && 54 < g.public_pct;
    if weak && f.other_id == g.corp_controller[f.target_id] {
        // the rival controls its weakly held largest holding: the target stays
    } else if f.other_id == g.corp_controller[f.target_id] {
        f.largest_hold_id = cast::<i16, _>(get_largest_holding_id(g, f.target_id));
        f.pick_id = cast::<i32, _>(f.largest_hold_id);
        if 10 < f.pick_id {
            update_corp_control(g, ui, rng, f.pick_id);
            if !(g.largest_holder_pct < 29 && 54 < g.public_pct && f.other_id == g.corp_controller[f.pick_id]) {
                f.pick_id = 0;
            }
        }
        if f.pick_id < 11 {
            f.target_id = 0;
        } else {
            f.amount = Ext::from_f64(0.0);
            f.ent_id = 1;
            while f.ent_id <= 15 {
                if cast::<i32, _>(g.hold_stock_id[(f.ent_id, f.target_id)]) == f.pick_id {
                    f.amount = g.market_cap[f.pick_id] * cast::<Ext, _>(g.hold_stock_pct[(f.ent_id, f.target_id)]);
                }
                f.ent_id += 1;
            }
            calc_entity_financials(g, ui, rng, f.target_id);
            if f.amount <= g.total_assets * sf(0.5f32) {
                f.target_id = 0;
            } else {
                f.target_id = f.pick_id;
            }
        }
    } else {
        f.target_id = 0;
    }
    if f.target_id == 0 {
        return;
    }
    f.largest_hold_id = cast::<i16, _>(f.target_id);
    f.pick_id = to_long((cast::<Ext, _>(g.asbestos_corp)).abs());
    toxic_check(g, ui, rng, f);
    if f.pick_id == 0 {
        f.pick_id = cast::<i32, _>(g.superfund_corp);
    }
    toxic_check(g, ui, rng, f);
    if f.pick_id == 0 {
        return;
    }
    calc_entity_financials(g, ui, rng, f.pick_id);
    if g.net_worth < Ext::from_f64(0.0) {
        return;
    }
    f.allow_toxic = 1;
    f.target_pct = 21;
    f.target_id = f.pick_id;
    f.buyer_corp = ai_find_affordable_buyer(g, ui, rng, f.target_id, f.target_pct, f.allow_toxic);
    if f.buyer_corp == 0 {
        return;
    }
    f.target_pct = 20;
    validate_buyer_corp(g, ui, rng, f);
    f.trace_tag = PbStr::from(__content.string(1822))
        + &str_s(cast::<Ext, _>(f.buyer_corp))
        + "  TGT ="
        + &str_s(cast::<Ext, _>(f.target_id));
    if f.buyer_corp == 0 {
        return;
    }
    update_corp_control(g, ui, rng, f.target_id);
    if g.cur_player == g.corp_controller[f.target_id] {
        g.tmp_ext1 = Ext::from_f64(1.0);
        f.slot = cast::<i32, _>(f.largest_hold_id);
        crate::logic::restructure::merge_companies(g, ui, rng, f.target_id, f.slot, 1, 0, f.time_secs);
        update_control_chain(g, ui, rng, f.target_id);
        update_control_chain(g, ui, rng, f.slot);
        if g.corp_controller[f.slot] == 1 || f.target_id == g.corp_controller[f.slot] {
            g.ai_hostile_merger_qtr = -5;
        } else {
            g.ai_hostile_merger_qtr = g.cur_quarter;
        }
        if g.ai_hostile_merger_qtr < 0 {
            let mut msg = g.ent_symbol[f.target_id].clone()
                + __content.string(1823)
                + &g.ent_symbol[f.slot]
                + __content.string(1824);
            show_quick_message(g, ui, &mut msg);
        }
    }
    f.target_pct = 0;
    f.ent_id = 1;
    while f.ent_id <= 15 {
        if cast::<i32, _>(g.hold_stock_id[(f.ent_id, f.buyer_corp)]) == f.target_id {
            f.target_pct = cast::<i32, _>(g.hold_stock_pct[(f.ent_id, f.buyer_corp)]);
        }
        f.ent_id += 1;
    }
    if 0 < f.target_pct {
        g.tmp_ext1 = cast::<Ext, _>(f.buyer_corp);
        g.tmp_ext2 = cast::<Ext, _>(f.target_pct);
        g.tmp_ext3 = Ext::from_f64(1.0);
        sell_stock_holding(g, ui, rng, f.target_id);
        g.tmp_ext1 = Ext::from_f64(0.0);
    }
}

/// The toxic-company test of HostileMerger (inline twice): `pickId` (> 10) must be worth 0.3..7 times
/// the target and weakly held (largest holder <= 20%, public >= 21%), else `pickId = 0`.
fn toxic_check(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if 10 < f.pick_id {
        if g.market_cap[f.pick_id] < g.market_cap[f.target_id] * sf(0.3f32)
            || !(g.market_cap[f.pick_id] <= g.market_cap[f.target_id] * Ext::from_f64(7.0))
        {
            f.pick_id = 0;
        }
        if 10 < f.pick_id {
            update_corp_control(g, ui, rng, f.pick_id);
            if 20 < g.largest_holder_pct || g.public_pct < 21 {
                f.pick_id = 0;
            }
        }
    }
}

/// Port of AiPlayer_IssueStockOffering (0x5ce762), GOSUB of AiComputerPlayerTurn ("12000").
/// When the market is not depressed (`MarketValuation >= 0.95`): the player's first holding (70%+,
/// no recent offering, with an earnings history and `NetWorth <= 100 * MarketCap`), else the last
/// [runtime resource wording] `AiTargetCorp`) with `MarketCap >= 3%` of its net worth, rating 2+, a 25%+
/// largest holder (skipped 2 times in 3 with more than 5 companies) and no recent offering, sells
/// RND(25, 40)% new stock (`IssueStock`; a private placement for a company without earnings 3 years
/// ago), recorded in `RecentOfferings`; one turn action.
pub(crate) fn issue_stock_offering(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if g.ai_target_corp == 0 {
        return;
    }
    pack_holding_arrays(g, ui);
    f.pick_id = cast::<i32, _>(g.hold_stock_id[(1, f.player_id)]);
    if f.pick_id < 11 || !(sf(0.95f32) <= cast::<Ext, _>(g.market_valuation)) {
        return;
    }
    calc_entity_financials(g, ui, rng, f.pick_id);
    f.target_id = f.pick_id;
    f.target_pct = cast::<i32, _>(g.hold_stock_pct[(1, f.player_id)]);
    f.other_id = is_recent_offering(g, f.target_id);
    let direct = f.other_id == 0
        && 69 < f.target_pct
        && g.earnings_yr2_ago[f.pick_id] != Ext::from_f64(0.0)
        && g.net_worth <= g.market_cap[f.pick_id] * Ext::from_f64(100.0);
    if !direct {
        f.target_id = 0;
        f.ent_id = 11;
        while f.ent_id <= 1600 {
            let e = f.ent_id;
            if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] && g.ai_target_corp != e {
                calc_entity_financials(g, ui, rng, e);
                if !(g.market_cap[e] < g.net_worth * sf(0.03f32)) && !(g.credit_rating_num[e] < 2) {
                    update_corp_control(g, ui, rng, e);
                    if 24 < g.largest_holder_pct
                        && !(5 < f.corps_controlled && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0))
                    {
                        f.other_id = is_recent_offering(g, e);
                        if f.other_id != 1 {
                            f.target_id = e;
                            f.target_pct = cast::<i32, _>(g.largest_holder_pct);
                            if g.public_pct == 0 {
                                f.x_12c = 1;
                            }
                        }
                    }
                }
            }
            f.ent_id += 1;
        }
    }
    if f.target_id == 0 {
        return;
    }
    if g.market_cap[f.target_id] < g.net_worth * sf(0.03f32) && f.target_pct < 100 && g.ai_target_corp != f.target_id {
        return;
    }
    if !(g.net_worth * sf(0.012f32) <= g.market_cap[f.target_id]) {
        return;
    }
    if g.ai_target_corp == f.target_id {
        update_corp_control(g, ui, rng, f.target_id);
        if g.public_pct == 0 {
            f.x_12c = 1;
        }
    }
    g.tmp_ext1 = cast::<Ext, _>(rng.rnd_range(40, 25));
    f.pick_id = 1;
    if g.ai_target_corp != f.target_id && g.earnings_yr3_ago[f.target_id] == Ext::from_f64(0.0) {
        f.pick_id = 2;
    }
    issue_stock(g, ui, rng, f.target_id, f.pick_id);
    add_recent_offering(g, f.target_id, f.pick_id);
    if f.pick_id == 2 {
        normalize_holding_pcts(g, ui, rng, f.target_id, f.x_12c);
    }
    g.turn_action_count += 1;
}

/// Port of AiPlayer_ImproveCreditRating (0x5cebd9), GOSUB of AiComputerPlayerTurn ("13000").
/// Picks a weak controlled company (`newCorpId` from a purchase; else one time in 3 the target
/// `AiTargetCorp` when its margin is under 7%, or the first controlled non-fund company with a margin of
/// at most 15%, banks/insurers one time in 3, favouring loss makers). With a credit rating of 6+ and a
/// margin of at most `3 + 7 * RND` it restructures: a write-off of 10% of the business assets
/// (operating companies with more than 40) or of 3% of the net worth (insurers: of the reserves) from
/// cash, booked to `ExtraordinaryPL`, and raises the margin by `2 + DifficultyLevel`, with news for the
/// player's own companies. A low-rated (<= 5) heavy loss maker (margin <= -13) of a player with 4+
/// companies gets +1 margin once a quarter (`AiCreditFixQuarter`) when the rival net worth exceeds ten
/// times its own (one time in 3).
pub(crate) fn improve_credit_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.pick_id = 0;
    'pick: {
        if 10 < f.new_corp_id && g.industry_num[f.new_corp_id] < 70 {
            f.pick_id = f.new_corp_id;
            break 'pick;
        }
        if g.ai_target_corp == 0 {
            return;
        }
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
            return;
        }
        let t = g.ai_target_corp;
        if g.industry_num[t] < 70 && g.profit_margin[t] < 7 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            f.pick_id = g.ai_target_corp;
            break 'pick;
        }
        f.ent_id = 11;
        while f.ent_id < 1601 {
            let e = f.ent_id;
            if g.entity_status[e] == 1
                && f.player_id == g.corp_controller[e]
                && g.industry_num[e] <= 69
                && g.profit_margin[e] <= 15
                && !(e < 131 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0))
            {
                let skip = rng.rnd() * Ext::from_f64(4.0) < Ext::from_f64(3.0) && 3 < f.corps_controlled && -2 < g.profit_margin[e];
                if !skip {
                    f.pick_id = e;
                    f.ent_id = 1600;
                }
            }
            f.ent_id += 1;
        }
    }
    if f.pick_id < 11 {
        return;
    }
    let p = f.pick_id;
    calc_entity_financials(g, ui, rng, p);
    f.cash_amt = 0;
    if g.credit_rating_num[p] <= 5 {
        if f.corps_controlled <= 3 {
            return;
        }
        if g.cur_quarter == g.ai_credit_fix_quarter {
            return;
        }
        if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(2.0) {
            return;
        }
        if -13 < g.profit_margin[p] {
            return;
        }
        if f.own_net_worth * Ext::from_f64(10.0) >= f.rival_net_worth {
            return;
        }
        g.profit_margin[p] = g.profit_margin[p].wrapping_add(1);
        g.ai_credit_fix_quarter = g.cur_quarter;
        return;
    }
    if !(rng.rnd() * Ext::from_f64(7.0) + Ext::from_f64(3.0) >= cast::<Ext, _>(g.profit_margin[p])) {
        return;
    }
    if 130 < p && Ext::from_f64(40.0) < g.business_assets[p] {
        f.cash_amt = to_quad(g.business_assets[p] * sf(0.1f32));
        g.business_assets[p] = g.business_assets[p] - cast::<Ext, _>(f.cash_amt);
    } else if p < 131 {
        f.cash_amt = to_quad(g.net_worth * sf(0.03f32));
        if g.industry_num[p] == 2 {
            f.cash_amt = to_quad(g.ins_reserves[p] * sf(0.03f32));
        }
        g.cash[p] = g.cash[p] - cast::<Ext, _>(f.cash_amt);
    }
    g.extraordinary_pl[p] = g.extraordinary_pl[p] - f.cash_amt;
    f.defendant_id = g.difficulty_level;
    g.profit_margin[p] = cast::<i16, _>(2 + cast::<i32, _>(g.profit_margin[p]) + f.defendant_id);
    update_corp_control(g, ui, rng, p);
    if f.player_id == g.voting_controller {
        f.pending_msg = PbStr::from(__content.string(1825));
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) && 1 < f.cash_amt {
            f.pending_msg = PbStr::from(__content.string(1826))
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(cast::<Ext, _>(f.cash_amt), b"#################")
                + " "
                + &g.million_prefix_lc
                + "il."
                + &g.currency_suffix;
        }
        if p < 131 {
            f.pending_msg = PbStr::from(__content.string(1827));
            if f.fire_mgmt_corp == p {
                f.pending_msg = PbStr::new();
            }
        }
        if !f.pending_msg.is_empty() {
            g.news_msg = g.ent_symbol[p].clone() + &f.pending_msg;
            post_news(g, ui, 0, 0);
        }
    }
}

/// Port of AiPlayer_BuyoutPublicShares (0x5cf5ba), GOSUB of AiComputerPlayerTurn.
/// For `AiTargetCorp` (its market cap temporarily x0.9 at difficulty 3+, then x1.11): when the player
/// controls it with no public float, holds less than 100% and there was no recent offering, buys 10%
/// (20% when its cash covers 25x the market cap) in a private placement at 95% of the market value
/// (`IssueStock` type 2 with news "… to <player>", `NormalizeHoldingPcts` to 100%), provided it is
/// cheap (`MarketCap < 1.5%` of net worth). Then a leveraged buyout of it (`AiRandomLBO`) is tried
/// when it owes no holding-company tax.
pub(crate) fn buyout_public_shares(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    g.public_pct = 100;
    f.other_id = 0;
    if !(10 < g.ai_target_corp) {
        return;
    }
    let t = g.ai_target_corp;
    update_corp_control(g, ui, rng, t);
    if 2 < g.difficulty_level {
        g.market_cap[t] = g.market_cap[t] * sf(0.9f32);
    }
    f.other_id = is_recent_offering(g, t);
    let p = f.player_id;
    if g.public_pct == 0
        && g.hold_stock_pct[(1, p)] < 100
        && f.other_id == 0
        && p == g.corp_controller[t]
    {
        f.target_pct = 10;
        if !(g.market_cap[t] * Ext::from_f64(25.0) >= g.t_bills[p] + g.cash[p]) {
            f.target_pct = 20;
        }
        if g.market_cap[t] * (cast::<Ext, _>(f.target_pct)) < g.t_bills[p] + g.cash[p] {
            calc_entity_financials(g, ui, rng, t);
            if g.market_cap[t] < g.net_worth * sf(0.015f32) {
                f.pay_amt = sf(0.95f32) * cast::<Ext, _>(f.target_pct) * g.market_cap[t];
                g.hold_stock_pct[(1, p)] = cast::<i16, _>(f.target_pct + cast::<i32, _>(g.hold_stock_pct[(1, p)]));
                g.hold_stock_basis[(1, p)] = f.pay_amt + g.hold_stock_basis[(1, p)];
                g.cash[p] = g.cash[p] - f.pay_amt;
                g.tmp_ext1 = cast::<Ext, _>(f.target_pct);
                g.action_code = PbStr::from("AUTO");
                issue_stock(g, ui, rng, t, 2);
                if !g.news_msg.is_empty() && right(g.news_msg.as_bytes(), 8) == __content.string(1828) {
                    g.news_msg = g.news_msg.clone() + " to " + &g.player_name[p];
                    post_news(g, ui, 0, 0);
                }
                add_recent_offering(g, t, 2);
                normalize_holding_pcts(g, ui, rng, t, 1);
            }
        }
    }
    calc_entity_financials(g, ui, rng, t);
    f.amount = calc_capital_taxes(g, t);
    if g.turn_action_count < 9 && g.holding_co_tax_due == Ext::from_f64(0.0) {
        ai_random_lbo(g, ui, rng, t, &f.now.clone());
    }
    if 2 < g.difficulty_level {
        g.market_cap[t] = sf(1.11f32) * g.market_cap[t];
    }
}

/// Port of AiPlayer_MergeTaxLossSub (0x5cfad1), GOSUB of AiComputerPlayerTurn.
/// Resets `AiTargetCorp` to the player's first holding; when the player controls it (51%+ largest
/// holder), it has 4+ holdings and a tax-loss carry of at least `3000 * ExchangeRate`, merges the first
/// of its holdings that is smaller, profitable for tax (`TaxLossCarry >= 0`), held 80% or less and with
/// `PublicPct + stake >= 85` (another player's company only below `CpuMaxCorps`) to use the losses.
pub(crate) fn merge_tax_loss_sub(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    g.ai_target_corp = cast::<i32, _>(g.hold_stock_id[(1, f.player_id)]);
    if g.ai_target_corp == 0 {
        return;
    }
    let a = g.ai_target_corp;
    if f.player_id != g.corp_controller[a] {
        return;
    }
    if g.hold_stock_id[(0, a)] < 4 || !(g.tax_loss_carry[a] <= g.exchange_rate * -Ext::from_f64(3000.0)) {
        return;
    }
    update_corp_control(g, ui, rng, a);
    if g.largest_holder_pct < 51 {
        return;
    }
    f.target_id = 0;
    f.trade_count = 0;
    pack_holding_arrays(g, ui);
    while f.target_id == 0 {
        f.trade_count += 1;
        f.target_id = cast::<i32, _>(g.hold_stock_id[(f.trade_count, a)]);
        if 10 < f.target_id {
            update_corp_control(g, ui, rng, f.target_id);
            if f.player_id != g.corp_controller[f.target_id]
                && g.cpu_max_corps <= g.player_corps_controlled[f.player_id]
            {
                f.target_id = 0;
                continue;
            }
            if g.market_cap[f.target_id] > g.market_cap[a] || g.tax_loss_carry[f.target_id] < Ext::from_f64(0.0) {
                f.target_id = 0;
            }
            if (cast::<i32, _>(g.public_pct) + cast::<i32, _>(g.hold_stock_pct[(f.trade_count, a)])) < 85 {
                f.target_id = 0;
            }
            if 79 < g.hold_stock_pct[(f.trade_count, a)] {
                f.target_id = 0;
            }
        }
        if f.target_id == 0 && cast::<i32, _>(g.hold_stock_id[(0, a)]) == f.trade_count {
            f.target_id = -1;
        }
        if f.target_id == 0 && 14 < f.trade_count {
            f.target_id = -1;
        }
    }
    if f.target_id < 11 {
        return;
    }
    crate::logic::restructure::merge_companies(g, ui, rng, g.ai_target_corp, f.target_id, 1, 0, f.time_secs);
    update_control_chain(g, ui, rng, a);
}

/// Port of AiPlayer_AntitrustScan (0x5cfe21), GOSUB of AiComputerPlayerTurn.
/// Unless [runtime resource wording]  are off: in the first industry 3..69 where the player's group has a
/// market share of 56%+, if it controls more than one company there (other than `AiTargetCorp`) with
/// business assets above `150 * ExchangeRate`, the smallest of them is divested (`DivestStock`, reason 2)
/// by every controlled holder 11..1600 until the player no longer controls it.
///
/// The original loops forever when no controlled company 11..1600 holds the divested company any more
/// (e.g. the player holds it directly), see BUGS.md #274; the port returns instead.
pub(crate) fn antitrust_scan(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    if g.private_antitrust_off == 1 {
        return;
    }
    if g.ai_target_corp == 0 || g.hold_stock_id[(0, f.player_id)] == 0 {
        return;
    }
    if f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    f.pick_id = 0;
    f.other_id = 0;
    f.result_amt = 0;
    f.ent_id = 3;
    while f.ent_id <= 69 {
        f.industry = cast::<i16, _>(f.ent_id);
        g.tmp_ext1 = cast::<Ext, _>(f.player_id);
        f.pick_id = calc_industry_market_share(g, f.industry);
        if 55 < f.pick_id {
            break;
        }
        f.industry = 0;
        f.ent_id += 1;
    }
    if !(2 < f.industry && f.industry < 70) {
        return;
    }
    f.trade_count = 0;
    f.ent_id = 131;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1
            && f.player_id == g.corp_controller[e]
            && g.industry_num[e] == f.industry
            && g.ai_target_corp != e
            && g.exchange_rate * Ext::from_f64(150.0) < g.business_assets[e]
        {
            if f.result_amt == 0 || g.business_assets[e] < cast::<Ext, _>(f.result_amt) {
                f.result_amt = to_quad(g.business_assets[e]);
                f.other_id = e;
            }
            f.trade_count += 1;
        }
        f.ent_id += 1;
    }
    while 10 < f.other_id && 1 < f.trade_count {
        if f.player_id != g.corp_controller[f.other_id] {
            return;
        }
        let mut progress = false;
        f.ent_id = 11;
        while f.ent_id < 1601 {
            if g.entity_status[f.ent_id] == 1 && f.player_id == g.corp_controller[f.ent_id] {
                f.slot = 1;
                while f.slot <= 15 {
                    if cast::<i32, _>(g.hold_stock_id[(f.slot, f.ent_id)]) == f.other_id {
                        let held = (1..=15).any(|s| {
                            cast::<i32, _>(g.hold_stock_id[(s, f.ent_id)]) == f.other_id && 0 < g.hold_stock_pct[(s, f.ent_id)]
                        });
                        progress |= held && g.entity_status[f.other_id] == 1;
                        divest_stock(g, ui, rng, f.other_id, f.ent_id, 2);
                        f.ent_id = 1600;
                    }
                    f.slot += 1;
                }
            }
            f.ent_id += 1;
        }
        if !progress {
            // the original repeats this pass forever (BUGS.md #274)
            return;
        }
    }
}
