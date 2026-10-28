//! Merger/takeover events (`MergerTakeoverEvents`) and the verdict of an insider-trading probe
//! (`InsiderTradingProbeVerdict`).
//!
//! Every fractional constant of both routines is a SINGLE of the constant pool (`fmul dword`), written
//! `sf(..)`.
//!
//! Error handling: `MergerTakeoverEvents` has an ON ERROR handler that only calls ShowRuntimeError
//! (0x46ea48, a message box) and leaves the routine. The port returns at the places where the original
//! raises error 9 for reachable out-of-range subscripts (a target id of 0 after an insider-tip tick that
//! is not the current one, a human holder id 6..10); the message box is not shown.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::antitrust::antitrust_review_acquisition;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{calc_cur_month_of_year, calc_day_of_month};
use crate::logic::economy::show_misc_news_story;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::holdings::find_holding_slot;
use crate::logic::options::get_plr_naked_liab;
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::restructure::merge_companies;
use crate::logic::trading::buy_stock;
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::etf::ai_corp_sell_naked_puts;
use crate::logic::sim_entity::quarter_start::{cancel_pending_merger, clear_insider_tip};
use crate::logic::valuation::{calc_stock_price, price_pct_change_months, price_trend_direction};
use crate::pb::numeric::{ceil, frac};

/// `s` as a [`PbStr`].
fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// The SELECT CASE on the target's industry: is an acquirer of industry `acq` a plausible buyer of a
/// target of industry `target` (beyond the same industry / a conglomerate, checked by the caller)?
fn industry_fit(target: i16, acq: i16) -> bool {
    let a = acq;
    let any = |l: &[i16]| l.contains(&a);
    match target {
        1 => a == 2,
        2 => a == 37,
        3 | 67 | 68 => any(&[3, 67, 68]),
        4 | 21 => any(&[4, 20, 21, 22, 32]),
        5 | 6 => any(&[5, 6]),
        8 => any(&[7, 8, 11]),
        9 | 50 => any(&[9, 12, 50]),
        10 | 35 | 49 => any(&[10, 14, 35, 49]),
        11 | 25 => any(&[11, 16, 23, 25]),
        12 => any(&[7, 9]),
        13 => any(&[49, 53, 63, 64]),
        14 => a == 10,
        15 => any(&[28, 31, 33, 46, 57]),
        17 | 28 | 31 | 34 | 57 => any(&[17, 28, 31, 34, 57]),
        18 => 52 < a && a < 56,
        19 => any(&[42, 47, 66]),
        20 | 51 => any(&[7, 20, 51]),
        22 | 56 => any(&[22, 56]),
        23 => any(&[25, 52]),
        24 | 38 | 40 | 61 => any(&[24, 35, 38, 61]),
        26 => any(&[14, 26, 30, 35, 39]),
        27 | 29 | 65 => any(&[26, 27, 29, 30, 65]),
        30 => any(&[29, 61, 65]),
        32 => any(&[21, 28]),
        33 => any(&[6, 15]),
        36 => any(&[27, 29, 65]),
        37 => a < 3,
        39 => any(&[24, 26, 30, 61]),
        41..=48 => (40 < a && a < 49) || a == 19 || a == 66,
        52 => any(&[11, 16, 25]),
        53 | 54 => any(&[13, 49, 53, 54]),
        55 => any(&[3, 53, 67, 68]),
        58 => a == 45,
        59 => a == 29,
        60 => any(&[5, 49]),
        62 => a < 3,
        63 | 64 => any(&[8, 13, 63, 64, 69]),
        66 => any(&[19, 42, 47, 48]),
        69 => any(&[49, 63, 64]),
        _ => false,
    }
}

/// The locals of `MergerTakeoverEvents` (zeroed at entry) used across its phases.
#[derive(Default)]
struct Mte {
    target_id: i32,
    acquirer_id: i32,
    holder_id: i32,
    premium_pct: i16,
}

/// Port of MergerTakeoverEvents (0x4b37c8).
/// The [runtime resource wording] machinery, called with `mode` 0 (random AI takeover attempt), 1 (the
/// shareholder vote on the pending merger) or 2 (the insider-tip event: the tipped merger of
/// `MergerTipCorp`).
///
/// * Modes other than 1 pick a target (`TakeoverCandidateId`, a random entity, sometimes a corp of the
///   `CrowdedIndustry`, or the tipped corp on its event tick) and an acquirer (a random entity 86..1580 of
///   matching size, not full of holdings, preferably in the same industry; early in the quarter a rich
///   corp 1471..1500), check control, size, net worth and industry fit and run the antitrust review
///   (`AntitrustReviewAcquisition`); a cash-rich acquirer may buy a stake first (`BuyStock`). If the
///   current human player (or a [runtime resource wording] im) holds the target, the merger offer is made:
///   "Trading Halted" and "Merger Offer" message boxes (with insider-trading commentary in cheat mode),
///   `MergerAcquirer`/`MergerTarget`/`MergerPricePerShare` and the vote date, news, +6% on the target's
///   market cap and sentiment; else (no such holder) the merger is executed at once (below).
/// * Mode 1 postpones the vote when another player made the offer, or cancels it (premium outside
///   3..80%, weak credit, inactive parties) and else executes it.
/// * Execution: for the tipped corp the insider may kill the merger (positions over 75%, or randomly)
///   with a fraud charge (`InsiderProbePlayer`); `MergeCompanies` runs if the acquirer holds target
///   stock, else regulators kill it; the outcome is announced (approval, "big winner", voted down).
///
/// `time_secs`: seconds of `TIME$` for [`calc_stock_price`], `BuyStock` and `MergeCompanies`.
pub fn merger_takeover_events(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, mode: i32) {
    // ON ERROR GOTO -> ShowRuntimeError("... " + STR$(acquirerId) + ... + STR$(targetId)) and exit
    let mut l = Mte::default();
    if mode == 0 && 10 < g.merger_tip_corp && cast::<i32, _>(g.merger_tip_corp) == g.merger_target {
        return;
    }
    if mode == 0 && 10 < g.merger_target {
        return;
    }
    if mode != 1 && !mte_pick_deal(g, ui, rng, time_secs, &mut l, mode) {
        return;
    }
    // LAB_004b5610
    if g.merger_acquirer == 0 && g.merger_target == 0 {
        if g.next_takeover_tick != 0 {
            g.next_takeover_tick = 0;
            return;
        }
        l.holder_id = 1;
        while l.holder_id <= 1600 {
            let holder = l.holder_id;
            if l.acquirer_id != holder && g.hold_stock_id[(0, holder)] != 0 {
                let mut hold_slot = 1;
                while hold_slot < 16 {
                    if cast::<i32, _>(g.hold_stock_id[(hold_slot, holder)]) == l.target_id
                        && 1 <= g.hold_stock_pct[(hold_slot, holder)]
                    {
                        if 0 < holder && holder < 11 && g.cur_player != holder {
                            if cast::<i32, _>(g.merger_tip_corp) != l.target_id {
                                return;
                            }
                            if 19 < g.hold_stock_pct[(hold_slot, holder)] {
                                if 5 < holder {
                                    return; // IsComputerPlayer(1 TO 5): error 9 -> ON ERROR exit
                                }
                                if g.is_computer_player[holder] == 0 {
                                    clear_insider_tip(g);
                                    return;
                                }
                            }
                            hold_slot = 15;
                        }
                        if g.cur_player == holder {
                            g.corp_controller[holder] = g.cur_player;
                        } else {
                            update_corp_control(g, ui, rng, holder);
                        }
                        let mut holder_is_cpu = 0;
                        if g.cur_player == holder && g.is_computer_player[g.cur_player] == 1 {
                            holder_is_cpu = 1;
                        }
                        if g.cur_player == g.corp_controller[holder] && holder_is_cpu == 0 {
                            mte_make_offer(g, ui, rng, time_secs, &mut l);
                            return;
                        }
                    }
                    hold_slot += 1;
                }
            }
            l.holder_id += 1;
        }
        l.premium_pct = 0;
    } else if mode == 1 {
        if !mte_vote(g, ui, rng, &mut l) {
            return;
        }
    } else {
        return;
    }
    mte_execute(g, ui, rng, time_secs, &mut l);
}

/// Target and acquirer selection, fit checks and antitrust review of `MergerTakeoverEvents` (modes
/// other than 1). `true`: continue at `LAB_004b5610` (offer / execution); `false`: exit.
fn mte_pick_deal(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, l: &mut Mte, mode: i32) -> bool {
    let mut pick_target = true;
    if mode == 2 && 10 < g.merger_tip_corp {
        if g.entity_status[cast::<i32, _>(g.merger_tip_corp)] != 1 {
            clear_insider_tip(g);
            return false;
        }
        if cast::<i32, _>(g.insider_tip_event_tick) == g.sim_co_count && g.sim_co_count < 50 {
            g.insider_tip_event_tick = 50;
            return false;
        }
        if cast::<i32, _>(g.insider_tippee) != g.cur_player || g.merger_target != 0 {
            g.insider_tip_event_tick = g.insider_tip_event_tick.wrapping_add(500);
            if 1600 < g.insider_tip_event_tick {
                g.insider_tip_event_tick = 80;
            }
        } else {
            pick_target = false;
        }
    }
    if pick_target {
        // LAB_004b396c
        l.target_id = g.takeover_candidate_id;
        if g.takeover_candidate_id < 11 {
            l.target_id = rng.rnd_range(1600, 11);
        }
        if g.industry_num[l.target_id] == 71 {
            g.takeover_candidate_id = 0;
            return false;
        }
        if g.entity_status[l.target_id] == 1 {
            g.takeover_candidate_id = 0;
        } else {
            l.target_id = 0;
        }
        let r = rng.rnd();
        if r * Ext::from_f64(5.0) < Ext::from_f64(1.0) && 2 < g.crowded_industry && g.crowded_industry < 71 {
            let mut attempt = 1;
            loop {
                l.target_id = rng.rnd_range(1580, 131);
                if !(g.entity_status[l.target_id] == 1
                    && cast::<i32, _>(g.industry_num[l.target_id]) == g.crowded_industry)
                {
                    l.target_id = 0;
                }
                if 130 < l.target_id {
                    attempt = 50;
                }
                attempt += 1;
                if attempt >= 51 {
                    break;
                }
            }
        }
        if l.target_id == 0 {
            return false;
        }
        if g.earnings_yr2_ago[l.target_id] == Ext::from_f64(0.0) && get_controller_type(g, l.target_id) == 1 {
            return false;
        }
        if g.market_cap[l.target_id] * Ext::from_f64(40.0) < g.cash[l.target_id] + g.t_bills[l.target_id] {
            return false;
        }
    }
    let tip = cast::<i32, _>(g.merger_tip_corp);
    if 10 < tip && cast::<i32, _>(g.insider_tippee) == g.cur_player && cast::<i32, _>(g.insider_tip_event_tick) == g.sim_co_count {
        l.target_id = tip;
    }
    let target = l.target_id;
    let mut attempt = 1;
    loop {
        l.acquirer_id = rng.rnd_range(1580, 86);
        let acq = l.acquirer_id;
        if target != acq && g.entity_status[acq] == 1 {
            if !(11..=1600).contains(&target) {
                // BUGS.md #135: MarketCap / IndustryNum(targetId) with targetId = 0: error 9 -> ON ERROR exit
                if tip == target || g.hold_stock_id[(0, acq)] != 15 {
                    return false;
                }
            } else if tip != target || g.market_cap[target] <= g.market_cap[acq] * sf(0.9f32) {
                // LAB_004b3ce2
                if g.hold_stock_id[(0, acq)] != 15 {
                    if tip == target && g.market_cap[target] * sf(1.5f32) < g.market_cap[acq] {
                        attempt = 75;
                    }
                    let mut skip = false;
                    if g.industry_num[acq] == g.industry_num[target] {
                        if g.industry_num[acq] == 2 {
                            let r = rng.rnd();
                            if Ext::from_f64(1.0) < r * Ext::from_f64(10.0) && tip != target {
                                skip = true;
                            }
                        }
                        if !skip {
                            attempt = 75;
                        }
                    }
                    // LAB_004b3e37
                    if cast::<i32, _>(g.industry_num[acq]) == g.fad_industry
                        && 200 < g.fad_valuation_pct
                        && g.fad_valuation_pct % 2 == 0
                    {
                        attempt = 75;
                    }
                }
            }
        }
        attempt += 1;
        if attempt >= 76 {
            break;
        }
    }
    if l.acquirer_id < 86 {
        return false;
    }
    let first_acquirer = l.acquirer_id;
    let alt_search = if g.entity_status[l.acquirer_id] == 1 {
        get_controller_type(g, l.acquirer_id) == 1 || target == l.acquirer_id
    } else {
        true
    };
    if alt_search && g.sim_co_count < 800 {
        // LAB_004b3f24
        let mut candidates_left = 10;
        let mut tries = 0;
        while 0 < candidates_left && {
            tries += 1;
            tries < 1001
        } {
            l.acquirer_id = rng.rnd_range(1500, 1471);
            let acq = l.acquirer_id;
            if g.entity_status[acq] == 1 {
                if g.hold_stock_id[(0, acq)] != 15 && g.market_cap[target] * sf(0.8f32) <= g.market_cap[acq] {
                    if g.cash[acq] + g.t_bills[acq] <= g.exchange_rate * Ext::from_f64(50000.0) {
                        candidates_left -= 1;
                    } else {
                        candidates_left = 0;
                    }
                }
            } else {
                l.acquirer_id = 0;
            }
        }
    }
    if l.acquirer_id == 0 {
        l.acquirer_id = first_acquirer;
    }
    if 10 < l.acquirer_id && 10 < first_acquirer && g.market_cap[l.acquirer_id] < g.market_cap[first_acquirer] {
        l.acquirer_id = first_acquirer;
    }
    if g.entity_status[l.acquirer_id] != 1 || l.target_id == l.acquirer_id {
        return false;
    }
    update_corp_control(g, ui, rng, l.acquirer_id);
    if 10 < g.voting_controller {
        let v = g.voting_controller;
        update_corp_control(g, ui, rng, v);
    }
    update_corp_control(g, ui, rng, l.acquirer_id);
    if cast::<i32, _>(g.largest_holder) == l.target_id {
        return false;
    }
    if get_controller_type(g, l.acquirer_id) == 1 {
        return false;
    }
    if l.target_id == g.corp_controller[l.acquirer_id] {
        return false;
    }
    if 10 < l.acquirer_id && get_controller_type(g, l.acquirer_id) == 2 {
        let ctrl = g.corp_controller[l.acquirer_id];
        if ctrl != g.corp_controller[l.target_id] && g.cpu_max_corps <= g.player_corps_controlled[ctrl] {
            return false;
        }
    }
    update_corp_control(g, ui, rng, l.target_id);
    if cast::<i32, _>(g.parent80_owner) == l.acquirer_id || cast::<i32, _>(g.largest_holder) == l.acquirer_id {
        return false;
    }
    let ratio = rng.rnd() * sf(0.75f32) + Ext::from_f64(1.0);
    if g.market_cap[l.acquirer_id] * ratio < g.market_cap[l.target_id]
        && 10 < g.corp_controller[l.target_id]
        && cast::<i32, _>(g.merger_tip_corp) != l.target_id
    {
        std::mem::swap(&mut l.target_id, &mut l.acquirer_id);
    }
    let (acq, target) = (l.acquirer_id, l.target_id);
    if g.market_cap[acq] < g.market_cap[target] * sf(0.2f32) {
        return false;
    }
    let acq_industry = g.industry_num[acq];
    calc_entity_financials(g, ui, rng, acq);
    if g.market_cap[acq] * Ext::from_f64(125.0) < g.net_worth {
        return false;
    }
    let mut ok = acq_industry == 70;
    if acq_industry == g.industry_num[target] {
        ok = true;
    }
    if industry_fit(g.industry_num[target], acq_industry) {
        ok = true;
    }
    if g.rand_seed % 25 == 7 {
        ok = true;
    }
    if cast::<i32, _>(g.merger_tip_corp) == target
        && cast::<i32, _>(g.insider_tip_event_tick) == g.sim_co_count
        && 10 < target
        && g.market_cap[target] < g.market_cap[acq]
    {
        ok = true;
    }
    if !ok {
        return false;
    }
    g.tmp_ext1 = cast::<Ext, _>(acq);
    g.tmp_ext2 = cast::<Ext, _>(target);
    g.tmp_ext3 = Ext::from_f64(2.0);
    let ok_flag = antitrust_review_acquisition(g, ui, rng, 1);
    if ok_flag != 1 {
        return false;
    }
    if g.market_cap[acq] < g.market_cap[target] * sf(0.2f32) && cast::<i32, _>(g.merger_tip_corp) != target {
        return false;
    }
    update_corp_control(g, ui, rng, target);
    if g.market_cap[target] * Ext::from_f64(50.0) * sf(1.2f32) < g.cash[acq] + g.t_bills[acq] && g.merger_acquirer == 0 {
        let r = rng.rnd();
        if Ext::from_f64(1.0) < r * Ext::from_f64(5.0) && 51 < g.public_pct {
            g.tmp_ext1 = int(rng.rnd() * Ext::from_f64(36.0));
            g.tmp_ext1 += Ext::from_f64(15.0);
            g.tmp_ext2 = Ext::from_f64(2.0);
            buy_stock(g, ui, rng, acq, target, time_secs);
        }
    }
    true
}

/// The merger offer of `MergerTakeoverEvents` to the current human player holding the target (directly
/// or through `l.holder_id`).
fn mte_make_offer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, l: &mut Mte) {
    let __content = crate::content::current();
    let (acq, target, holder) = (l.acquirer_id, l.target_id, l.holder_id);
    let r = rng.rnd();
    l.premium_pct = to_integer(r * Ext::from_f64(25.0) + Ext::from_f64(25.0));
    if 35 < l.premium_pct {
        update_corp_control(g, ui, rng, target);
        if 85 < g.public_pct {
            l.premium_pct -= 10;
        }
    }
    if cast::<i32, _>(g.merger_tip_corp) == target {
        l.premium_pct = to_integer(sf(1.15f32) * cast::<Ext, _>(l.premium_pct));
    }
    let price = g.market_cap[target] * (cast::<Ext, _>(l.premium_pct) * sf(0.01f32) + Ext::from_f64(1.0));
    g.merger_price_per_share.set(price);
    let mut offer_price = 0;
    if Ext::from_f64(0.0) < g.shares_outstanding[target] {
        offer_price = to_long(g.merger_price_per_share.get() / g.shares_outstanding[target]);
    }
    if offer_price < 10 {
        cancel_pending_merger(g, ui, 0);
        return;
    }
    let price_text = fmt_trim(cast::<Ext, _>(offer_price), b"* ###,.00");
    let mut msg = s(__content.string(5110))
        + &g.ent_name[target]
        + " ("
        + &g.ent_symbol[target]
        + __content.string(5216)
        + __content.string(5217)
        + "\r"
        + "\r"
        + __content.string(5094);
    let price_chg_pct = price_pct_change_months(g, target, 2);
    let mut suspicious = 0;
    if 10 < price_chg_pct && 0 < price_trend_direction(g, target, 2, 1) {
        suspicious = 1;
    }
    let mut phrase = s(" was up");
    if 19 < price_chg_pct {
        phrase = s(__content.string(5218));
    }
    if price_chg_pct < -8 && price_trend_direction(g, target, 2, 1) < 0 {
        suspicious = 1;
        phrase = s(__content.string(5219));
    }
    if price_chg_pct < -15 {
        phrase = replace(phrase.as_bytes(), b"fallen", b"plunged");
    }
    if cast::<i32, _>(g.merger_tip_corp) != target {
        let r = rng.rnd();
        if Ext::from_f64(1.0) < r * Ext::from_f64(9.0) {
            suspicious = 0;
        }
    }
    if suspicious == 1 && g.cheat_mode == "Y" && g.halt_comment_count1 == 0 {
        msg = msg
            + "\r"
            + "\r"
            + __content.string(5220)
            + "the "
            + &g.ent_symbol[target]
            + " stock"
            + &phrase
            + &number_to_words(price_chg_pct, 0, 1)
            + __content.string(5221)
            + __content.string(5222)
            + __content.string(5223)
            + __content.string(5224);
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            msg = replace(msg.as_bytes(), __content.bytes(5225), __content.bytes(5226));
        }
        if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
            msg = replace(msg.as_bytes(), __content.bytes(5227), __content.bytes(5228));
        }
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            msg = replace(msg.as_bytes(), __content.bytes(5229), __content.bytes(5230));
        }
        g.halt_comment_count1 += 1;
    }
    let title = s(__content.string(5113)) + &g.ent_name[target];
    ui.msgbox(&msg, 8208, &title);
    update_corp_control(g, ui, rng, target);
    if 5 < g.public_pct && 130 < acq {
        let naked_liab = get_plr_naked_liab(g, ui, rng, acq, 0, 0);
        if Ext::from_f64(0.0) < g.shares_outstanding[target]
            && 6 < g.credit_rating_num[acq]
            && naked_liab == Ext::from_f64(0.0)
            && Ext::from_f64(5.0) < g.market_cap[target] / g.shares_outstanding[target]
            && g.market_cap[target] * Ext::from_f64(6.0) < g.liquid_funds
        {
            ai_corp_sell_naked_puts(g, ui, rng, acq, target);
        }
    }
    g.merger_acquirer = acq;
    g.merger_target = target;
    g.merger_proposer_player = g.cur_player;
    g.next_takeover_tick = g.sim_co_count + 500;
    if 1500 < g.next_takeover_tick {
        g.next_takeover_tick = g.sim_co_count - 1000;
        if g.next_takeover_tick < 50 {
            g.next_takeover_tick = 50;
        }
    }
    let month_pos = calc_cur_month_of_year(g, g.sim_co_count);
    let mut month = to_long(ceil(month_pos));
    let month_frac = frac(month_pos);
    let mut day = calc_day_of_month(g, month, month_frac);
    let days_in_month = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if 1499 < g.sim_co_count || g.sim_co_count <= g.num_players {
        day = days_in_month;
    }
    month += 1;
    if 12 < month {
        month = 1;
    }
    if 30 < day {
        day = 30;
    }
    if month == 2 && 28 < day {
        day = 28;
    }
    let vote_date = g.month_name[month].clone() + &str_s(cast::<Ext, _>(day));
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, target, time_secs);
    g.stock_price_str = trim(g.currency_symbol.as_bytes()) + &g.stock_price_str;
    phrase = s("You own");
    if 10 < holder {
        phrase = g.ent_name[holder].clone() + " owns";
    }
    let slot = find_holding_slot(g, holder, target);
    let hold_pct = g.hold_stock_pct[(slot, holder)];
    let group_pct = calc_group_stake_pct(g, g.cur_player, target);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut group_stake_text = PbStr::new();
    if (cast::<i32, _>(hold_pct)) < group_pct {
        group_stake_text = s(__content.string(5231))
            + &number_to_words(group_pct, 0, 1)
            + __content.string(5232);
    }
    let msg = g.ent_name[acq].clone()
        + " ("
        + &g.ent_symbol[acq]
        + __content.string(5233)
        + __content.string(5234)
        + &g.ent_name[target]
        + " ("
        + &g.ent_symbol[target]
        + __content.string(5235)
        + __content.string(5236)
        + &trim(g.currency_symbol.as_bytes())
        + &price_text
        + __content.string(5237)
        + &str_s(cast::<Ext, _>(l.premium_pct))
        + __content.string(5238)
        + __content.string(5239)
        + &g.stock_price_str
        + __content.string(5240)
        + &g.ent_symbol[target]
        + "."
        + "\r"
        + "\r"
        + &phrase
        + &str_s(cast::<Ext, _>(hold_pct))
        + "% of "
        + &g.ent_name[target]
        + ". "
        + &group_stake_text
        + "\r"
        + "\r"
        + __content.string(5241)
        + &vote_date
        + __content.string(5242)
        + "of "
        + &g.ent_symbol[target]
        + __content.string(5243);
    let title = s(__content.string(5244)) + &g.ent_name[target];
    ui.msgbox(&msg, 8256, &title);
    g.news_msg = g.ent_symbol[target].clone()
        + __content.string(5245)
        + &g.ent_symbol[acq]
        + __content.string(5246)
        + &vote_date
        + ".";
    post_news(g, ui, 0, 0);
    g.news_msg = g.ent_symbol[acq].clone()
        + __content.string(5247)
        + &trim(g.currency_symbol.as_bytes())
        + &price_text
        + __content.string(5248)
        + &g.ent_symbol[target];
    post_news(g, ui, 0, 0);
    g.market_cap[target] = g.market_cap[target] * sf(1.06f32);
    let new_val = to_quad(cast::<Ext, _>(l.premium_pct) * sf(0.3f32) + cast::<Ext, _>(g.investor_sentiment[target]));
    g.investor_sentiment[target] = cast::<i32, _>(new_val);
    if g.market_cap[target] * Ext::from_f64(8.0) < g.cash[acq] + g.t_bills[acq] {
        update_corp_control(g, ui, rng, target);
        if 51 < g.public_pct {
            g.tmp_ext1 = Ext::from_f64(5.0);
            g.tmp_ext2 = Ext::from_f64(2.0);
            buy_stock(g, ui, rng, acq, target, time_secs);
        }
    }
    if cast::<i32, _>(g.merger_tip_corp) == g.merger_target {
        g.insider_tip_event_tick = 0;
    }
}

/// The vote on the pending merger (mode 1) of `MergerTakeoverEvents`: postponement when another player
/// proposed it, cancellation on a premium outside 3..80% / weak credit / inactive parties. `true`: go on
/// to execute the merger.
fn mte_vote(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, l: &mut Mte) -> bool {
    let __content = crate::content::current();
    let old_tick = g.next_takeover_tick;
    if 0 < g.merger_proposer_player && g.cur_player != g.merger_proposer_player {
        g.next_takeover_tick += 500;
        if 1500 < g.next_takeover_tick {
            g.next_takeover_tick = old_tick - 1000;
        }
        if g.next_takeover_tick < 50 {
            g.next_takeover_tick = 50;
        }
        let tail = if g.merger_target + g.merger_acquirer == g.merger_postpone_key {
            s(" again.")
        } else {
            s(__content.string(5249))
        };
        g.merger_postpone_key = g.merger_target + g.merger_acquirer;
        g.news_msg = g.ent_symbol[g.merger_target].clone()
            + __content.string(5250)
            + &g.ent_symbol[g.merger_acquirer]
            + &tail;
        let proposer = g.merger_proposer_player;
        if g.is_computer_player[proposer] != 1
            && g.entity_status[proposer] != 2
            && g.entity_status[g.merger_acquirer] == 1
            && g.entity_status[g.merger_target] == 1
        {
            let (a, b) = (g.merger_acquirer, g.merger_target);
            post_news(g, ui, a, b);
            return false;
        }
        g.news_msg = s(__content.string(5251))
            + &g.ent_symbol[g.merger_acquirer]
            + " and "
            + &g.ent_symbol[g.merger_target]
            + __content.string(5252);
        let (a, b) = (g.merger_acquirer, g.merger_target);
        post_news(g, ui, a, b);
        cancel_pending_merger(g, ui, 0);
        return false;
    }
    let mut ok_flag = 1;
    l.acquirer_id = g.merger_acquirer;
    l.target_id = g.merger_target;
    let (acq, target) = (l.acquirer_id, l.target_id);
    if !(g.entity_status[acq] == 1 && g.entity_status[target] == 1) {
        ok_flag = -1;
    }
    if g.market_cap[target] == Ext::from_f64(0.0) {
        ok_flag = 0;
    }
    if ok_flag == 1 {
        let ratio = (g.merger_price_per_share.get() / g.market_cap[target] - Ext::from_f64(1.0)) * Ext::from_f64(100.0);
        l.premium_pct = to_integer(ceil(ratio));
        if l.premium_pct < 3 || 80 < l.premium_pct {
            ok_flag = 0;
        }
        if g.credit_rating_num[target] < 2 {
            ok_flag = 0;
        }
        if ok_flag == 0 {
            if 80 < l.premium_pct {
                g.market_cap[target] = g.market_cap[target] * sf(0.9f32);
            }
            g.investor_sentiment[target] -= 10;
            g.news_msg = s(__content.string(5253))
                + &g.ent_symbol[acq]
                + __content.string(5254)
                + &g.ent_symbol[target];
            if g.credit_rating_num[target] < 2 {
                g.news_msg = s("Citing ")
                    + &g.ent_symbol[target]
                    + __content.string(5255)
                    + &g.ent_symbol[acq]
                    + __content.string(5256);
            }
            post_news(g, ui, 0, 0);
            let msg = g.ent_name[acq].clone()
                + __content.string(5257)
                + "with "
                + &g.ent_name[target]
                + ", ";
            let mut phrase = s(__content.string(5258));
            if 80 < l.premium_pct && 1 < g.credit_rating_num[target] {
                phrase = s(__content.string(5259)) + &g.ent_symbol[target] + __content.string(5260);
            }
            g.investor_sentiment[target] -= 20;
            g.market_cap[target] = g.market_cap[target] * sf(0.92f32);
            if cast::<i32, _>(g.merger_tip_corp) == target {
                let r = rng.rnd();
                if Ext::from_f64(1.0) < r * Ext::from_f64(3.0) {
                    clear_insider_tip(g);
                }
            }
            let mut qm = msg + &phrase;
            show_quick_message(g, ui, &mut qm);
        }
    }
    cancel_pending_merger(g, ui, 0);
    ok_flag >= 1
}

/// Execution of the merger (`MergerTakeoverEvents` after the vote or for an AI deal): the insider check
/// for the tipped corp, `MergeCompanies` and the announcements.
fn mte_execute(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, l: &mut Mte) {
    let __content = crate::content::current();
    let (acq, target) = (l.acquirer_id, l.target_id);
    let tip = cast::<i32, _>(g.merger_tip_corp);
    if tip == target && 10 < tip {
        if cast::<i32, _>(g.insider_tippee) != g.cur_player {
            return;
        }
        let month = calc_group_stake_pct(g, g.cur_player, target);
        let option_pct = to_long(g.tmp_ext1);
        g.tmp_ext1 = Ext::from_f64(0.0);
        let mut insider_kill = 0;
        let r = rng.rnd();
        let check75 = if (cast::<Ext, _>(option_pct) + cast::<Ext, _>(month)) <= r * Ext::from_f64(50.0) + Ext::from_f64(25.0) {
            true
        } else {
            let r2 = rng.rnd();
            if r2 * Ext::from_f64(2.0) <= Ext::from_f64(1.0) {
                true
            } else {
                if 1 < g.tipster_state[g.cur_player] || cast::<i32, _>(g.insider_tippee) == g.insider_probe_player {
                    insider_kill = 1;
                }
                false
            }
        };
        if check75 && 75 < option_pct + month {
            insider_kill = 1;
        }
        if insider_kill == 1 {
            g.news_msg = s(__content.string(5261))
                + &str_s(cast::<Ext, _>(option_pct) + cast::<Ext, _>(month))
                + "% of "
                + &g.ent_symbol[target]
                + __content.string(5262)
                + &g.ent_symbol[acq];
            g.tmp_ext2 = cast::<Ext, _>(acq);
            g.tmp_news_str = g.ent_name[g.cur_player].clone();
            let name = g.ent_name[target].clone();
            show_misc_news_story(g, ui, rng, 493, name.as_bytes());
            post_news(g, ui, acq, target);
            g.market_cap[target] = g.market_cap[target] * sf(0.82f32);
            g.investor_sentiment[target] -= 20;
            if g.industry_num[target] < 70 {
                let r = rng.rnd();
                let v = to_quad(cast::<Ext, _>(g.profit_margin[target]) - (r * Ext::from_f64(5.0) + Ext::from_f64(5.0)));
                g.profit_margin[target] = cast::<i16, _>(v);
            }
            if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                g.insider_probe_player = cast::<i32, _>(g.insider_tippee);
                g.news_msg = s(__content.string(5263)) + &g.cur_player_name + __content.string(5264);
                let mut qm = g.news_msg.clone()
                    + __content.string(5265)
                    + &g.ent_symbol[acq]
                    + __content.string(5266)
                    + &g.ent_symbol[target]
                    + ".";
                show_quick_message(g, ui, &mut qm);
                post_news(g, ui, 0, 0);
            }
            clear_insider_tip(g);
            return;
        }
        if month < 1
            && option_pct == 0
            && 0 < g.tipster_state[g.cur_player]
            && cast::<i32, _>(g.insider_tippee) == g.insider_probe_player
        {
            g.insider_probe_player = 0;
        }
        g.news_msg = g.ent_symbol[target].clone()
            + __content.string(5267)
            + &g.ent_symbol[acq];
        let mut qm = g.news_msg.clone()
            + __content.string(5268)
            + __content.string(3292)
            + &g.ent_name[target]
            + __content.string(5269);
        show_quick_message(g, ui, &mut qm);
        post_news(g, ui, 0, 0);
    }
    let hold_pct = cast::<i16, _>(calc_group_stake_pct(g, g.cur_player, target));
    let group_pct = to_long(g.tmp_ext1 + cast::<Ext, _>(hold_pct));
    g.tmp_ext1 = Ext::from_f64(0.0);
    let (acq_arg, target_arg) = (acq, target);
    if 0 < find_holding_slot(g, acq_arg, target_arg) {
        merge_companies(g, ui, rng, acq_arg, target_arg, 2, l.premium_pct, time_secs);
    } else {
        g.news_msg = s(__content.string(5270))
            + &g.ent_symbol[acq_arg]
            + " and "
            + &g.ent_symbol[target_arg];
        post_news(g, ui, acq_arg, target_arg);
    }
    update_corp_control(g, ui, rng, target);
    let phrase = g.ent_symbol[target].clone() + __content.string(5271) + &g.ent_symbol[acq];
    if tip == target && 10 < tip {
        if cast::<i32, _>(g.wholly_owned_by) == acq || g.entity_status[target] != 1 {
            // LAB_004b7b2b
            let mut msg = s(__content.string(5272))
                + &g.ent_name[acq]
                + " and "
                + &g.ent_name[target]
                + " has "
                + __content.string(5273)
                + &g.ent_name[target]
                + __content.string(5274)
                + __content.string(5275)
                + &g.ent_name[target];
            if g.entity_status[target] == 2 {
                msg = msg + __content.string(5276) + &g.ent_name[acq] + __content.string(5277);
            } else if cast::<i32, _>(g.wholly_owned_by) == acq {
                msg = msg + __content.string(5278) + &g.ent_name[acq] + ".";
            }
            show_quick_message(g, ui, &mut msg);
            if 0 < group_pct {
                let mut winner = false;
                if 0 < calc_group_stake_pct(g, g.cur_player, acq) || Ext::from_f64(0.0) < g.tmp_ext1 {
                    winner = !(g.entity_status[target] == 1 && calc_group_stake_pct(g, g.cur_player, target) != 0);
                }
                if winner {
                    g.news_msg = g.cur_player_name.clone() + __content.string(5279) + &phrase;
                    if g.rand_seed % 3 == 1 {
                        g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(5280), __content.bytes(5281));
                    }
                    if g.rand_seed % 3 == 2 {
                        g.news_msg = g.cur_player_name.clone() + __content.string(5282) + &phrase;
                    }
                }
                // LAB_004b7e88
                let slot = find_holding_slot(g, g.cur_player, acq);
                let cp = g.cur_player;
                if cast::<i32, _>(g.hold_stock_id[(slot, cp)]) == acq {
                    let ratio = g.market_cap[acq] * cast::<Ext, _>(g.hold_stock_pct[(slot, cp)]);
                    if ratio < g.hold_stock_basis[(slot, cp)] * sf(1.1f32) {
                        g.news_msg = PbStr::new();
                    }
                }
                if 0 < calc_group_stake_pct(g, g.cur_player, target) {
                    g.news_msg = PbStr::new();
                }
                g.tmp_ext1 = Ext::from_f64(0.0);
            }
        } else {
            g.investor_sentiment[target] -= 20;
            g.market_cap[target] = g.market_cap[target] * sf(0.87f32);
            g.news_msg = s("Tycoon ") + &g.cur_player_name + __content.string(5283) + &phrase + __content.string(5284);
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                g.news_msg = replace(g.news_msg.as_bytes(), b"stunned", __content.bytes(5285));
            }
            if hold_pct < 51 {
                if group_pct == 0 {
                    g.news_msg = s(__content.string(5286)) + &phrase + __content.string(5287);
                }
            } else {
                g.news_msg = s("Raider ") + &g.cur_player_name + __content.string(5288) + &phrase + ".";
            }
            crate::logic::report::show_quick_news(g, ui);
        }
        clear_insider_tip(g);
    } else if 0 < group_pct && cast::<i32, _>(g.wholly_owned_by) != acq && g.entity_status[target] == 1 {
        g.investor_sentiment[target] -= 10;
        g.market_cap[target] = g.market_cap[target] * sf(0.93f32);
        g.news_msg = s(__content.string(5289)) + &phrase + __content.string(5290);
        crate::logic::report::show_quick_news(g, ui);
    }
    post_news(g, ui, 0, 0);
}

/// Port of InsiderTradingProbeVerdict (0x67c24a).
/// Verdict on the insider-trading probe of human player `InsiderProbePlayer` (cleared for computer and
/// retired players; nothing happens while that player is still the current tippee). With 95%
/// probability a fine of 32..95% of total assets (over 250,000 x ExchangeRate: about that amount or
/// 20..30% of assets; at most 93% of net worth, at least 100 x ExchangeRate) is taken from cash, the
/// tipster is cut off, "fined ... for securities fraud" is reported with news story 492 and a pop-up
/// quote (plus a 10 x ExchangeRate university donation for fines over 100 x ExchangeRate), and
/// `PlayerNotoriety(CurPlayer)` drops by 1; else the player is acquitted "on a technicality".
pub fn insider_trading_probe_verdict(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let player_id = g.insider_probe_player;
    if !(0 < player_id && player_id <= g.num_players) {
        return;
    }
    if g.is_computer_player[player_id] == 1 {
        g.insider_probe_player = 0;
        return;
    }
    if g.player_name[player_id] == "...OUT..." {
        g.insider_probe_player = 0;
        return;
    }
    if cast::<i32, _>(g.insider_tippee) == player_id {
        return;
    }
    let r = rng.rnd();
    if r * Ext::from_f64(20.0) <= Ext::from_f64(19.0) {
        g.insider_probe_player = 0;
        g.tipster_state[player_id] = -1;
        calc_entity_financials(g, ui, rng, player_id);
        let total_assets = g.total_assets;
        let mut fine_amt = to_quad((rng.rnd() * sf(0.63f32) + sf(0.32f32)) * total_assets);
        if g.exchange_rate * Ext::from_f64(250000.0) < cast::<Ext, _>(fine_amt) {
            let base = to_quad(g.exchange_rate * Ext::from_f64(250000.0));
            let r = rng.rnd();
            fine_amt = to_quad(r * sf(0.15f32) * cast::<Ext, _>(base) + cast::<Ext, _>(base));
            if (cast::<Ext, _>(fine_amt)) < g.total_assets * sf(0.2f32) {
                let r = rng.rnd();
                fine_amt = to_quad((r * sf(0.1f32) + sf(0.2f32)) * g.total_assets);
            }
        }
        if g.net_worth * sf(0.95f32) < cast::<Ext, _>(fine_amt) {
            fine_amt = to_quad(g.net_worth * sf(0.93f32));
        }
        if (cast::<Ext, _>(fine_amt)) < g.exchange_rate * Ext::from_f64(100.0) {
            fine_amt = to_quad(g.exchange_rate * Ext::from_f64(100.0));
        }
        g.cash[player_id] = g.cash[player_id] - cast::<Ext, _>(fine_amt);
        g.tmp_ext2 = cast::<Ext, _>(player_id);
        g.tmp_news_str = s("S.E.C.");
        if g.currency_code != "USA" {
            g.tmp_news_str = s(__content.string(1550));
        }
        let mut fine_text = trim(g.currency_symbol.as_bytes()) + &fmt_trim(cast::<Ext, _>(fine_amt), b"* #########,");
        g.news_msg = g.ent_symbol[player_id].clone()
            + __content.string(5291)
            + &fine_text
            + " "
            + &g.million_prefix_lc
            + "il."
            + &g.currency_suffix
            + __content.string(5292);
        let mut story_text = g.currency_suffix.clone();
        if story_text.is_empty() {
            story_text = s(" ") + &g.currency_name;
            if right(story_text.as_bytes(), 1) != "s" {
                story_text = story_text + "s";
            }
        }
        if g.currency_code == "JAP" {
            story_text = s(" Yen");
        }
        fine_text = fine_text + " " + &g.million_prefix_lc + "illion" + &story_text;
        show_misc_news_story(g, ui, rng, 492, fine_text.as_bytes());
        story_text = g.news_msg.clone()
            + "\r\n"
            + "\r\n"
            + __content.string(5293)
            + &g.ent_symbol[player_id]
            + __content.string(5294)
            + __content.string(5295)
            + "\"";
        if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(2.0) {
            if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                story_text = replace(
                    story_text.as_bytes(),
                    __content.bytes(5296),
                    __content.bytes(5297),
                );
            }
        } else {
            story_text = replace(story_text.as_bytes(), __content.bytes(5298), __content.bytes(5299));
        }
        let mut quote_text = s(__content.string(5300));
        if g.cur_quarter == 2 {
            quote_text = s(__content.string(5301));
        }
        if g.cur_quarter == 3 {
            quote_text = s(__content.string(5302));
        }
        if g.cur_quarter == 4 {
            quote_text = s(
                __content.string(5303),
            );
        }
        story_text = story_text + &quote_text + "\"";
        if g.exchange_rate * Ext::from_f64(100.0) < cast::<Ext, _>(fine_amt) {
            g.cash[player_id] = g.cash[player_id] - g.exchange_rate * Ext::from_f64(10.0);
            quote_text = s("Harvard");
            let mut donation_text = trim(g.currency_symbol.as_bytes()) + __content.string(5304);
            if g.currency_code != "USA" {
                quote_text = s("Oxford");
                donation_text = trim(g.currency_symbol.as_bytes()) + __content.string(5305);
            }
            story_text = story_text
                + "\r\n"
                + "\r\n"
                + __content.string(5306)
                + __content.string(5307)
                + &quote_text
                + __content.string(5308)
                + __content.string(5309)
                + &donation_text
                + __content.string(5310);
        }
        show_quick_message(g, ui, &mut story_text);
        post_news(g, ui, 0, 0);
        let cp = g.cur_player; // BUGS.md #136: CurPlayer, not playerId
        g.player_notoriety[cp] = g.player_notoriety[cp].wrapping_sub(1);
    } else {
        g.news_msg = s(__content.string(1862)) + &g.ent_name[player_id] + __content.string(5311);
        let mut qm = g.news_msg.clone()
            + __content.string(5312);
        show_quick_message(g, ui, &mut qm);
        post_news(g, ui, 0, 0);
        g.insider_probe_player = 0;
        if g.tipster_state[player_id] == 4 {
            g.tipster_state[player_id] = -1;
        }
    }
}
