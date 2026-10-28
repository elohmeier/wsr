//! [runtime resource wording] s and market impact: `CalcCommodTradeLimit` (global, per-trade and per-group
//! limits with their notices), `CalcFuturesPositionRoom` (AI group cap), `ApplyFuturesTradeImpact`
//! (price/basis nudge of a trade) and `EnforceCommodityPositionLimits` (forced settlement of excess
//! player positions).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::needless_late_init,
    clippy::neg_multiply,
    clippy::needless_range_loop,
    clippy::manual_clamp,
    clippy::manual_range_contains,
    clippy::field_reassign_with_default,
    clippy::too_many_arguments
)]
use crate::pb::real::{Ext, cast};


use super::physical::sell_phys_commod;
use super::prelude::*;
use crate::logic::futures::{
    calc_futures_contract_value, get_commod_data, set_commodity_price_ranges, sum_commodity_holdings,
};
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::transactions::{book_futures_gain_loss, settle_futures_contract};
use crate::pb::numeric::Cux;

/// Port of CalcFuturesPositionRoom (0x634bd2).
/// Sums the |contracts| of `commod_name` futures in direction `action` ("BUY": longs, "SELL": shorts,
/// other: all) held by `entity_id` or any holder with the same controller, and returns the room left
/// under the AI group cap (OIL 175,000 / max 35,000 per call, GOLD 8,000/1,600, SILVER 10,000/2,000,
/// WHEAT 25,000/5,000, CORN 40,000/8,000, STOCKS 5,000/1,000; other commodities 0), clamped at 0.
/// Side effect of the original: `CorpController(CurPlayer) = CurPlayer`.
pub fn calc_futures_position_room(g: &mut Globals, entity_id: i32, commod_name: &PbStr, action: &PbStr) -> i32 {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let ent_id = entity_id;
    let commod = commod_name.clone();
    let cp = g.cur_player;
    g.corp_controller[cp] = cp;
    let mut held_qty: i32 = 0;
    let mut room: i32 = 0;
    for rec_idx in 1..=1000 {
        if g.futures_recs[rec_idx].is_empty() {
            continue;
        }
        let fut_rec = g.futures_recs[rec_idx].clone();
        if field(&fut_rec, 1) != commod {
            continue;
        }
        if *action == "BUY" && field_val(&fut_rec, 3) < Ext::from_f64(0.0) {
            continue;
        }
        if *action == "SELL" && Ext::from_f64(0.0) < field_val(&fut_rec, 3) {
            continue;
        }
        let holder = to_long(field_val(&fut_rec, 2));
        if holder != ent_id && g.corp_controller[holder] != g.corp_controller[ent_id] {
            continue;
        }
        held_qty = to_long(field_val(&fut_rec, 3).abs() + cast::<Ext, _>(held_qty));
    }
    let (cap, max) = match commod.as_bytes() {
        b"OIL" => (175000, 35000),
        b"GOLD" => (8000, 1600),
        b"SILVER" => (10000, 2000),
        b"WHEAT" => (25000, 5000),
        b"CORN" => (40000, 8000),
        b"STOCKS" => (5000, 1000),
        _ => (0, 0),
    };
    if max != 0 {
        room = cap - held_qty;
        if max < room {
            room = max;
        }
    }
    if room < 0 {
        room = 0;
    }
    room
}

/// Port of ApplyFuturesTradeImpact (0x6343f9).
/// Market impact of a trade of `qty` units/contracts of `commod_name` in `direction` (+1 buy, -1 sell):
/// STOCKS moves `FuturesBasis` by 0.005 per contract (only above 100 contracts); the other commodities
/// move it by 5 x the trade's share of the market depth (OIL 35,000, GOLD 1,600, SILVER 2,000, WHEAT
/// 5,000, CORN 8,000, BITCOIN 400, ETHEREUM 1,000; capped at 1) and move the spot price by 1.5%/2.5%/
/// 2.5%/2%/2%/3%/3% of that share (the target price the other way), then recompute the price ranges
/// (`SetCommodityPriceRanges`) and re-seed an oil target below 0.15. Unknown names do nothing.
pub fn apply_futures_trade_impact(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    commod_name: &PbStr,
    direction: i32,
    qty: i64,
) {
    // ON ERROR GOTO handler: ShowRuntimeError only
    let commod = commod_name.clone();
    let mut abs_qty: i64 = qty.wrapping_abs();
    let commod_data = get_commod_data(g, ui, &commod, 10);
    let i = to_long(field_val(&commod_data, 3));
    if commod == "STOCKS" {
        if Ext::from_f64(100.0) < (cast::<Ext, _>(abs_qty)).abs() {
            let new_basis = to_quad(sf(0.005f32) * cast::<Ext, _>(abs_qty) * cast::<Ext, _>(direction) + cast::<Ext, _>(g.futures_basis[i]));
            g.futures_basis[i] = cast::<i16, _>(new_basis);
        }
        return;
    }
    let market_depth: i32 = match commod.as_bytes() {
        b"OIL" => 35000,
        b"GOLD" => 1600,
        b"SILVER" => 2000,
        b"WHEAT" => 5000,
        b"CORN" => 8000,
        b"BITCOIN" => 400,
        b"ETHEREUM" => 1000,
        _ => return,
    };
    if (cast::<Ext, _>(abs_qty)) > cast::<Ext, _>(market_depth) {
        abs_qty = cast::<i64, _>(market_depth);
    }
    let mut ratio: Ext = crate::pb::numeric::double(cast::<Ext, _>(abs_qty) / cast::<Ext, _>(market_depth)); // DOUBLE local
    ratio = crate::pb::numeric::double(cast::<Ext, _>(direction) * ratio);
    let step = cast::<i32, _>(Ext80::from_i64(5).fmul(Ext80::from_ext(ratio)).to_i64());
    g.futures_basis[i] = cast::<i16, _>(step.wrapping_add(cast::<i32, _>(g.futures_basis[i])));
    match commod.as_bytes() {
        b"OIL" => {
            let delta: Ext = sf(0.015f32) * cast::<Ext, _>(g.oil_price) * ratio;
            g.oil_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.oil_price)));
            g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) - delta));
        }
        b"GOLD" => {
            let delta: Ext = sf(0.025f32) * cast::<Ext, _>(g.gold_price) * ratio;
            g.gold_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.gold_price)));
            g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) - delta));
        }
        b"SILVER" => {
            let delta: Ext = sf(0.025f32) * cast::<Ext, _>(g.silver_price) * ratio;
            g.silver_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.silver_price)));
            g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) - delta));
        }
        b"WHEAT" => {
            let delta: Ext = sf(0.02f32) * cast::<Ext, _>(g.wheat_price) * ratio;
            g.wheat_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.wheat_price)));
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) - delta));
        }
        b"CORN" => {
            let delta: Ext = sf(0.02f32) * cast::<Ext, _>(g.corn_price) * ratio;
            g.corn_price = cast::<f32, _>(cast::<f32, _>(delta + cast::<Ext, _>(g.corn_price)));
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_target_price) - delta));
        }
        b"BITCOIN" => {
            let delta: Ext = sf(0.03f32) * cast::<Ext, _>(g.bitcoin_price) * ratio;
            g.bitcoin_price = to_long(delta + cast::<Ext, _>(g.bitcoin_price));
            g.bitcoin_target_price = to_long(cast::<Ext, _>(g.bitcoin_target_price) - delta);
        }
        _ => {
            let delta: Ext = sf(0.03f32) * cast::<Ext, _>(g.ethereum_price) * ratio;
            g.ethereum_price = to_long(delta + cast::<Ext, _>(g.ethereum_price));
            g.ethereum_target_price = to_long(cast::<Ext, _>(g.ethereum_target_price) - delta);
        }
    }
    set_commodity_price_ranges(g, ui, rng);
    if (cast::<Ext, _>(g.oil_target_price)) < sf(0.15f32) {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.06f32) * rng.rnd() + sf(0.15f32)));
    }
}

/// Locals of CalcCommodTradeLimit used by its GOSUB.
struct LimitFrame {
    commod: PbStr,
    production_limit: i64,
    storage_limit: i64,
}

/// Port of PromptCommodityQuantity_SetLimits (GOSUB 0x639331).
/// Real-world annual production and storage (in contracts/units) per commodity.
fn set_limits(f: &mut LimitFrame) {
    let (p, st) = match f.commod.as_bytes() {
        b"OIL" => (3500000, 300000),
        b"GOLD" => (80000, 50000),
        b"SILVER" => (100000, 200000),
        b"WHEAT" => (250000, 100000),
        b"CORN" => (400000, 100000),
        b"BITCOIN" => (20000, 20000),
        b"ETHEREUM" => (50000, 50000),
        _ => return,
    };
    f.production_limit = p;
    f.storage_limit = st;
}

/// `CurPlayer = entId OR CurPlayer = CorpController(entId)`.
fn shown_to_player(g: &Globals, ent_id: i32) -> bool {
    g.cur_player == ent_id || g.cur_player == g.corp_controller[ent_id]
}

/// Port of CalcCommodTradeLimit (0x636c54) with its GOSUBs PromptCommodityQuantity_SetLimits (0x639331)
/// and PromptCommodityQuantity_Exit (0x6395b8).
/// The number of `CurCommodity` contracts (`hold_kind` 0) or physical units (`hold_kind` 1) `entity_id`
/// may trade, at most `requested_qty`: 0 when all holders together reach the global limit (annual
/// production resp. storage; STOCKS 100,000 contracts) or the entity's controller group reaches 5 x the
/// per-trade limit (2% of production/storage, OIL 1%, STOCKS 1,000); otherwise limited by the per-trade
/// limit (futures) and the group's remaining room. Notices (quick message / MSGBOX) go to the human
/// player controlling the entity, unless the current player is a computer player.
///
/// Probe mode via `TmpExt1` (reset to 0): 99 returns 99 unless the global limit is reached; -99 returns
/// -99 unless the group limit is reached. Entities < 1 give 0 (unless probing with 99).
pub fn calc_commod_trade_limit(g: &mut Globals, ui: &mut dyn Ui, entity_id: i32, hold_kind: i32, requested_qty: i64) -> i64 {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError only
    let ent_id = entity_id;
    let mut f = LimitFrame { commod: g.cur_commodity.clone(), production_limit: 0, storage_limit: 0 };
    let commod = f.commod.clone();
    let probe_mode = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut result: i64 = 0;
    let mut global_limit_hit = 0;
    if ent_id < 1 && 99 != probe_mode {
        return result;
    }
    if commod != "STOCKS" {
        set_limits(&mut f);
    }
    let commod_data = get_commod_data(g, ui, &commod, 11);
    if commod_data == "ERROR" {
        return result;
    }
    let units_per_contract = to_long(field_val(&commod_data, 1));
    let unit_name = field(&commod_data, 2);
    let mut global_units = to_quad(cast::<Ext, _>(f.production_limit) * cast::<Ext, _>(units_per_contract));
    let mut supply_note = s(__content.string(2577))
        + &lc(&commod)
        + __content.string(2578)
        + &fmt_trim(cast::<Ext, _>(global_units), b"* ###,")
        + " "
        + &unit_name
        + "s.) ";
    if commod == "SILVER" {
        supply_note = replace(supply_note.as_bytes(), __content.bytes(2579), __content.bytes(2580));
    }
    if is_crypto(&commod) {
        supply_note = PbStr::new();
    }
    let mut s_coins = PbStr::new(); // ebp-0x114
    let mut s_units: PbStr; // ebp-0x110
    let mut s_limit; // ebp-0x10c
    if hold_kind == 1 {
        global_units = to_quad(cast::<Ext, _>(f.storage_limit) * cast::<Ext, _>(units_per_contract));
        s_coins = s(__content.string(2581));
        if commod == "ETHEREUM" {
            s_coins = s(__content.string(2582));
        }
        supply_note = s(__content.string(2583));
        if !is_crypto(&commod) {
            supply_note = supply_note
                + &lc(&commod)
                + __content.string(2584)
                + &fmt_trim(cast::<Ext, _>(global_units), b"* ###,")
                + " "
                + &unit_name
                + "s.) ";
            if commod == "GOLD" {
                supply_note = replace(
                    supply_note.as_bytes(),
                    b"storage",
                    __content.bytes(2585),
                );
            }
        } else {
            supply_note = supply_note + &mc(&commod) + " that " + &s_coins + __content.string(2586);
            if commod == "ETHEREUM" {
                supply_note = remove(supply_note.as_bytes(), __content.bytes(2587));
            }
        }
    }
    let _ = &s_coins;
    let mut per_trade_limit = to_quad(cast::<Ext, _>(f.production_limit) * sf(0.02f32));
    if commod == "OIL" {
        per_trade_limit = to_quad(cast::<Ext, _>(f.production_limit) * sf(0.01f32));
    }
    if commod == "STOCKS" {
        per_trade_limit = 1000;
        f.production_limit = 100000;
        supply_note = PbStr::new();
    }
    if hold_kind == 1 {
        per_trade_limit = to_quad(cast::<Ext, _>(f.storage_limit) * sf(0.02f32));
        if commod == "OIL" {
            per_trade_limit = to_quad(cast::<Ext, _>(f.storage_limit) * sf(0.01f32));
        }
    }
    let group_limit = to_quad(cast::<Ext, _>(per_trade_limit) * Ext::from_f64(5.0));
    let mut held_total: i64;
    if hold_kind == 0 {
        held_total = sum_commodity_holdings(g, 0, 3, 0);
        if cast::<Ext, _>(f.production_limit) <= cast::<Ext, _>(held_total) {
            global_limit_hit = 1;
        }
    } else {
        held_total = sum_commodity_holdings(g, 0, 0, 3);
        if cast::<Ext, _>(f.storage_limit) <= cast::<Ext, _>(held_total) {
            global_limit_hit = 1;
        }
    }
    let mut commod_label = lc(&commod);
    if is_crypto(&commod) {
        commod_label = mc(&commod);
    }
    if commod == "OIL" {
        commod_label = s(__content.string(2588));
    }
    if commod == "STOCKS" {
        commod_label = s(__content.string(2589));
    }
    let mut msg_text: PbStr;
    if global_limit_hit != 1 {
        if probe_mode == 99 {
            return 99;
        }
        if hold_kind == 0 {
            held_total = sum_commodity_holdings(g, ent_id, 2, 0);
            if (cast::<Ext, _>(held_total)) < cast::<Ext, _>(group_limit) {
                if probe_mode == -99 {
                    return -99;
                }
                let mut group_room = to_quad(cast::<Ext, _>(group_limit) - cast::<Ext, _>(held_total));
                if cast::<Ext, _>(requested_qty) <= cast::<Ext, _>(group_room) {
                    group_room = requested_qty;
                }
                let mut trade_cap = per_trade_limit;
                if cast::<Ext, _>(requested_qty) <= cast::<Ext, _>(trade_cap) {
                    trade_cap = requested_qty;
                }
                if cast::<Ext, _>(requested_qty) <= cast::<Ext, _>(trade_cap) && cast::<Ext, _>(requested_qty) <= cast::<Ext, _>(group_room) {
                    return requested_qty;
                }
                if cast::<Ext, _>(group_room) <= cast::<Ext, _>(trade_cap) {
                    result = group_room;
                    s_limit = fmt_trim(cast::<Ext, _>(group_limit), b"* ###,");
                } else {
                    result = trade_cap;
                    s_limit = fmt_trim(cast::<Ext, _>(per_trade_limit), b"* ###,");
                }
                if g.is_computer_player[g.cur_player] == 1 {
                    return result;
                }
                if !shown_to_player(g, ent_id) {
                    return result;
                }
                msg_text = s(__content.string(2590));
                if cast::<Ext, _>(group_room) <= cast::<Ext, _>(trade_cap) {
                    msg_text = msg_text + __content.string(2591);
                } else {
                    msg_text = msg_text + __content.string(2592);
                }
                msg_text = msg_text + "to " + &s_limit + __content.string(2514);
                if cast::<Ext, _>(group_room) <= cast::<Ext, _>(trade_cap) && Ext::from_f64(0.0) < cast::<Ext, _>(held_total) {
                    msg_text = msg_text
                        + ", less "
                        + &fmt_trim(cast::<Ext, _>(held_total), b"* ###,")
                        + __content.string(2593)
                        + &commod_label
                        + __content.string(2594)
                        + &fmt_trim(cast::<Ext, _>(group_room), b"* ###,")
                        + __content.string(2595);
                    show_quick_message(g, ui, &mut msg_text);
                }
                return result;
            }
            if g.is_computer_player[g.cur_player] == 1 {
                return result;
            }
            msg_text = s(__content.string(2596))
                + &commod_label
                + __content.string(2597)
                + __content.string(2598)
                + &fmt_trim(cast::<Ext, _>(group_limit), b"* ###,")
                + __content.string(2514);
            if is_crypto(&commod) {
                msg_text = msg_text + ". ";
                if commod == "BITCOIN" {
                    msg_text = msg_text
                        + __content.string(2599)
                        + &fmt_trim(cast::<Ext, _>(group_limit), b"* ###,")
                        + " "
                        + __content.string(2600)
                        + __content.string(2601);
                } else {
                    msg_text = msg_text
                        + __content.string(2602)
                        + "\""
                        + "coins."
                        + "\""
                        + " (The "
                        + __content.string(2603);
                }
            } else if commod == "STOCKS" {
                msg_text = msg_text + ".";
            } else {
                msg_text = msg_text
                    + __content.string(2604)
                    + &supply_note
                    + __content.string(2605)
                    + __content.string(2606)
                    + &fmt_trim(cast::<Ext, _>(units_per_contract), b"* ###,")
                    + " "
                    + &unit_name
                    + "s of "
                    + &commod_label
                    + ". (The "
                    + &fmt_trim(cast::<Ext, _>(group_limit), b"* ###,")
                    + __content.string(2607);
                s_units = fmt_trim(cast::<Ext, _>(global_units) / Ext::from_f64(10.0), b"* ###,");
                if commod == "OIL" {
                    s_units = fmt_trim(cast::<Ext, _>(global_units) / Ext::from_f64(20.0), b"* ###,");
                }
                if right(s_units.as_bytes(), 1) == "1" {
                    let str_len = len(s_units.as_bytes());
                    s_units = mid(s_units.as_bytes(), 1, str_len - 1);
                }
                msg_text = msg_text + &s_units + " " + &unit_name + "s of " + &commod_label + ".)";
                if commod == "OIL" {
                    msg_text = replace(msg_text.as_bytes(), b"10%", b"5%");
                }
            }
            if commod_label == __content.string(2589) {
                commod_label = s(__content.string(2547));
            }
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(2608)
                + &commod_label
                + __content.string(2609)
                + __content.string(2610)
                + &g.ent_name[ent_id]
                + __content.string(2611);
            if shown_to_player(g, ent_id) {
                ui.msgbox(&msg_text, 0x2010, &s(__content.string(2612)));
            }
            return 0;
        }
        held_total = sum_commodity_holdings(g, ent_id, 0, 2);
        if (cast::<Ext, _>(held_total)) < cast::<Ext, _>(group_limit) {
            if probe_mode == -99 {
                return -99;
            }
            let group_room = to_quad(cast::<Ext, _>(group_limit) - cast::<Ext, _>(held_total));
            if cast::<Ext, _>(requested_qty) <= cast::<Ext, _>(group_room) {
                return requested_qty;
            }
            result = group_room;
            if g.is_computer_player[g.cur_player] == 1 {
                return result;
            }
            if !shown_to_player(g, ent_id) {
                return result;
            }
            s_limit = fmt_trim(cast::<Ext, _>(group_limit), b"* ###,");
            let mut storage_phrase = s(__content.string(2613));
            if commod_label == "Bitcoin" || commod_label == __content.string(1272) {
                storage_phrase = s("of");
            }
            msg_text = s(__content.string(2614))
                + &storage_phrase
                + " "
                + &commod_label
                + ", you "
                + __content.string(2615)
                + __content.string(2616)
                + &s_limit
                + __content.string(2617);
            if commod == "OIL" {
                msg_text = replace(msg_text.as_bytes(), b"10%", b"5%");
            }
            msg_text = msg_text
                + __content.string(2618)
                + &fmt_trim(cast::<Ext, _>(units_per_contract), b"* ###,")
                + " "
                + &unit_name
                + "s of "
                + &commod_label
                + ". ";
            let title;
            let style;
            if cast::<Ext, _>(held_total) <= Ext::from_f64(0.0) {
                msg_text = msg_text + "\r" + "\r" + &supply_note;
                title = __content.string(2619);
                style = 0x2040;
            } else {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(2620)
                    + &fmt_trim(cast::<Ext, _>(held_total), b"* ###,")
                    + " "
                    + &commod_label
                    + __content.string(2621)
                    + &fmt_trim(cast::<Ext, _>(group_room), b"* ###,")
                    + " units."
                    + "\r"
                    + "\r"
                    + &supply_note;
                if is_crypto(&commod) {
                    msg_text = replace(msg_text.as_bytes(), __content.bytes(2622), b"acquir");
                }
                title = __content.string(2623);
                style = 0x2010;
            }
            if ent_id <= g.num_players && g.is_computer_player[ent_id] == 1 {
                return result;
            }
            if g.ai_target_corp == ent_id {
                return result;
            }
            if shown_to_player(g, ent_id) {
                ui.msgbox(&msg_text, style, &s(title));
            }
            return result;
        }
        if g.is_computer_player[g.cur_player] == 1 {
            return result;
        }
        msg_text = s(__content.string(2624))
            + &commod_label
            + " in "
            + __content.string(2625)
            + &fmt_trim(cast::<Ext, _>(group_limit), b"* ###,")
            + __content.string(2626)
            + __content.string(2627);
        if cast::<Ext, _>(held_total) == cast::<Ext, _>(group_limit) {
            msg_text = remove(msg_text.as_bytes(), __content.bytes(2628));
        }
        if is_crypto(&commod) {
            if commod == "BITCOIN" {
                msg_text = replace(msg_text.as_bytes(), b"10%", __content.bytes(2629));
            }
            msg_text = msg_text + __content.string(2630) + &commod_label + __content.string(2631);
        } else {
            msg_text = msg_text + __content.string(2632);
        }
        msg_text = msg_text
            + &supply_note
            + __content.string(2633)
            + &fmt_trim(cast::<Ext, _>(units_per_contract), b"* ###,")
            + " "
            + &unit_name
            + "s of "
            + &commod_label
            + __content.string(2634)
            + &commod_label
            + " that "
            + __content.string(2635);
        s_units = fmt_trim(cast::<Ext, _>(global_units) / Ext::from_f64(10.0), b"* ###,");
        if commod == "OIL" {
            s_units = fmt_trim(cast::<Ext, _>(global_units) / Ext::from_f64(20.0), b"* ###,");
        }
        msg_text = msg_text + &s_units + " " + &unit_name + __content.string(2636);
        if commod == "OIL" {
            msg_text = replace(msg_text.as_bytes(), b"10%", b"5%");
        }
        msg_text = msg_text
            + "\r"
            + "\r"
            + __content.string(2637)
            + &commod_label
            + __content.string(2638)
            + __content.string(2639)
            + &g.ent_name[ent_id]
            + __content.string(2611);
        if is_crypto(&commod) {
            msg_text = remove(msg_text.as_bytes(), __content.bytes(2640));
            msg_text = remove(msg_text.as_bytes(), __content.bytes(2641));
            msg_text = remove(msg_text.as_bytes(), __content.bytes(2642));
            msg_text = remove(msg_text.as_bytes(), __content.bytes(2643));
            msg_text = replace(msg_text.as_bytes(), __content.bytes(2644), __content.bytes(2645));
        }
        if ent_id <= g.num_players && g.is_computer_player[ent_id] == 1 {
            return result;
        }
        if g.ai_target_corp == ent_id {
            return result;
        }
        if shown_to_player(g, ent_id) {
            ui.msgbox(&msg_text, 0x2010, &s(__content.string(2646)));
        }
        return 0;
    }
    msg_text = s(__content.string(2647)) + &commod_label;
    if commod == "STOCKS" {
        msg_text = msg_text + __content.string(2648);
    } else if !is_crypto(&commod) {
        msg_text = msg_text
            + __content.string(2649)
            + __content.string(2650);
    } else {
        msg_text = msg_text
            + __content.string(2651)
            + __content.string(2652)
            + __content.string(2653)
            + &commod_label
            + __content.string(2654)
            + __content.string(2655);
    }
    if hold_kind == 1 {
        msg_text = s(__content.string(2656))
            + &commod_label
            + __content.string(2657)
            + __content.string(2658)
            + &commod_label
            + __content.string(2659)
            + __content.string(2660);
        if commod == "GOLD" {
            msg_text = replace(msg_text.as_bytes(), __content.bytes(2661), __content.bytes(2662));
        }
    }
    msg_text = msg_text + "\r" + "\r" + &supply_note;
    if commod == "STOCKS" {
        msg_text = msg_text
            + __content.string(2663)
            + __content.string(2664);
    } else {
        msg_text = msg_text
            + __content.string(2665);
    }
    msg_text = msg_text + "\r" + "\r" + __content.string(2666) + &g.ent_name[ent_id] + __content.string(2611);
    s_limit = s(__content.string(2667));
    if hold_kind == 1 {
        s_limit = s(__content.string(2668));
    }
    let mut msg_title = s(__content.string(2669)) + &s_limit;
    if commod == "STOCKS" {
        msg_title = s(__content.string(2670));
    }
    if is_crypto(&commod) {
        msg_title = s(__content.string(2671)) + &commod_label + __content.string(2672);
        if hold_kind == 1 {
            msg_title = replace(msg_title.as_bytes(), b"Futures", __content.bytes(2673));
        }
    }
    if g.is_computer_player[g.cur_player] == 1 {
        return result;
    }
    if shown_to_player(g, ent_id) {
        ui.msgbox(&msg_text, 0x2010, &msg_title);
    }
    result
}

/// Locals of EnforceCommodityPositionLimits used by its GOSUB.
struct EnforceFrame {
    commod_code: PbStr,
    rec_text: PbStr,
    owner_id: i32,
    rec_idx: i32,
    contract_qty: i64,
    excess_qty: i64,
}

/// Port of EnforcePositionLimits_PartialSettle (GOSUB 0x63a5c1).
/// Settles `excess_qty` contracts of record `rec_idx`: pays the owner the pro-rata value minus cost,
/// books the gain net of a 1% commission (book mode 0) and rewrites the record with the remaining
/// quantity and cost.
fn partial_settle(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut EnforceFrame) {
    let rec_field4 = to_long(field_val(&f.rec_text, 4));
    let rec_field5 = to_long(field_val(&f.rec_text, 5));
    let contract_cost = val_ext(field(&f.rec_text, 6).as_bytes()); // EXT
    let mut rec_field7 = Cux(0);
    rec_field7.set(field_val(&f.rec_text, 7));
    if !(cast::<Ext, _>(f.contract_qty) != Ext::from_f64(0.0) && Ext::from_f64(1.0) <= cast::<Ext, _>(f.excess_qty)) {
        return;
    }
    let mut new_qty = to_quad((cast::<Ext, _>(f.contract_qty)).abs() - cast::<Ext, _>(f.excess_qty));
    if (cast::<Ext, _>(f.contract_qty)).abs() <= cast::<Ext, _>(new_qty) {
        return;
    }
    if (cast::<Ext, _>(f.contract_qty)) < Ext::from_f64(0.0) {
        new_qty = to_quad(cast::<Ext, _>(new_qty) * -Ext::from_f64(1.0));
    }
    let keep_frac: Ext = crate::pb::numeric::double(cast::<Ext, _>(new_qty) / cast::<Ext, _>(f.contract_qty)); // DOUBLE
    let sold_frac = Ext80::ONE.fsub(Ext80::from_ext(keep_frac)).into_ext();
    let settled_cost_x = contract_cost.fmul(Ext80::ONE.fsub(Ext80::from_ext(keep_frac)));
    let settled_cost = settled_cost_x.into_ext();
    let contract_value = calc_futures_contract_value(g, ui, rng, &f.rec_text).abs();
    let mut settle_gain = if Ext::from_f64(0.0) <= cast::<Ext, _>(f.contract_qty) {
        contract_value * sold_frac - settled_cost
    } else {
        settled_cost - contract_value * sold_frac
    };
    let o = f.owner_id;
    g.cash[o] = g.cash[o] + settle_gain;
    settle_gain = settle_gain - (settled_cost + contract_value * sold_frac) * sf(0.01f32);
    book_futures_gain_loss(g, ui, rng, settle_gain, o, &f.commod_code, 0);
    g.futures_recs[f.rec_idx] = f.commod_code.clone()
        + "|"
        + &str_trim(cast::<Ext, _>(o))
        + "|"
        + &str_trim(cast::<Ext, _>(new_qty))
        + "|"
        + &str_trim(cast::<Ext, _>(rec_field4))
        + "|"
        + &str_trim(cast::<Ext, _>(rec_field5))
        + "|"
        + &str_trim_ext(contract_cost.fsub(settled_cost_x))
        + "|"
        + &str_trim(rec_field7.get());
}

/// Port of EnforceCommodityPositionLimits (0x639a64) with its GOSUB EnforcePositionLimits_PartialSettle
/// (0x63a5c1) and exit stub EmptyStub_63a930 (0x63a930).
/// On ticks where `SimCoCount MOD 250` is 1..8, checks commodity 1..8 (OIL, GOLD, SILVER, WHEAT, CORN,
/// STOCKS, BITCOIN, ETHEREUM; sets `CurCommodity`) for every active player: futures held by the player
/// and the companies they control above the exchange position limit (`GetCommodData` field 5) are
/// settled (whole records, the last one partially) with news; physical holdings above the storage limit
/// (field 4) are sold 20% at a time (`SellPhysCommod`). The current player gets a MSGBOX for each.
pub fn enforce_commodity_position_limits(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let commod_idx = g.sim_co_count % 250;
    if commod_idx < 1 || 8 < commod_idx {
        return;
    }
    g.cur_commodity = s(["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"]
        [cast::<usize, _>(commod_idx - 1)]);
    let mut commod_label = lc(&g.cur_commodity);
    let mut f = EnforceFrame {
        commod_code: g.cur_commodity.clone(),
        rec_text: PbStr::new(),
        owner_id: 0,
        rec_idx: 0,
        contract_qty: 0,
        excess_qty: 0,
    };
    if g.cur_commodity == "STOCKS" {
        commod_label = s(__content.string(2547));
    }
    if is_crypto(&g.cur_commodity) {
        commod_label = mc(&g.cur_commodity);
    }
    let mut fut_settler_id = 0;
    let mut phys_seller_id = 0;
    let num_players = g.num_players;
    let mut player_id = 1;
    while player_id <= num_players {
        if g.player_name[player_id] != "...OUT..." && g.entity_status[player_id] == 1 {
            let mut settled_count = 0;
            let mut holdings = to_quad((cast::<Ext, _>(sum_commodity_holdings(g, player_id, 2, 0))).abs());
            let commod_data = get_commod_data(g, ui, &f.commod_code.clone(), 12);
            if commod_data == "ERROR" {
                return;
            }
            let position_limit = to_quad(field_val(&commod_data, 5));
            if (cast::<Ext, _>(position_limit)) < cast::<Ext, _>(holdings) {
                let n = to_long(val(g.futures_recs[0].as_bytes()));
                f.rec_idx = 1;
                while f.rec_idx <= n {
                    f.rec_text = g.futures_recs[f.rec_idx].clone();
                    if !f.rec_text.is_empty() && field(&f.rec_text, 1) == f.commod_code {
                        f.owner_id = to_long(field_val(&f.rec_text, 2));
                        if player_id == f.owner_id || player_id == g.corp_controller[f.owner_id] {
                            f.contract_qty = to_quad(field_val(&f.rec_text, 3));
                            if (cast::<Ext, _>(holdings) - cast::<Ext, _>(position_limit)) < (cast::<Ext, _>(f.contract_qty)).abs() {
                                f.excess_qty = to_quad(cast::<Ext, _>(holdings) - cast::<Ext, _>(position_limit));
                                partial_settle(g, ui, rng, &mut f);
                            } else {
                                settle_futures_contract(g, ui, rng, f.rec_idx, 2, 0, 0);
                            }
                            g.news_msg = g.ent_symbol[f.owner_id].clone()
                                + __content.string(2674)
                                + &commod_label
                                + __content.string(2675);
                            if g.cur_player == player_id {
                                fut_settler_id = f.owner_id;
                                settled_count += 1;
                            }
                            holdings = sum_commodity_holdings(g, player_id, 2, 0);
                            if cast::<Ext, _>(holdings) <= cast::<Ext, _>(position_limit) {
                                break;
                            }
                            f.rec_idx -= 1;
                        }
                    }
                    f.rec_idx += 1;
                }
            }
            if !g.news_msg.is_empty() {
                post_news(g, ui, 0, 0);
            }
            if 0 < fut_settler_id {
                let mut msg_text = s(__content.string(2676))
                    + &commod_label
                    + __content.string(2677)
                    + __content.string(2678)
                    + "of "
                    + &g.ent_name[fut_settler_id];
                if 1 < settled_count {
                    msg_text = msg_text + __content.string(2679);
                }
                msg_text = msg_text + __content.string(2680);
                ui.msgbox(&msg_text, 0x2010, &s(__content.string(2681)));
                fut_settler_id = 0;
            }
            if g.cur_commodity != "STOCKS" {
                let storage_limit = to_quad(field_val(&commod_data, 4));
                loop {
                    holdings = sum_commodity_holdings(g, player_id, 0, 2);
                    if (cast::<Ext, _>(storage_limit)) < cast::<Ext, _>(holdings) {
                        let n = to_long(val(g.phys_commod_rec[0].as_bytes()));
                        f.rec_idx = 1;
                        while f.rec_idx <= n {
                            f.rec_text = g.phys_commod_rec[f.rec_idx].clone();
                            if !f.rec_text.is_empty() && field(&f.rec_text, 1) == f.commod_code {
                                f.owner_id = to_long(field_val(&f.rec_text, 2));
                                if player_id == f.owner_id || player_id == g.corp_controller[f.owner_id] {
                                    sell_phys_commod(g, ui, rng, f.owner_id, f.rec_idx, 2, 20.0f32, 0);
                                    g.news_msg = g.ent_symbol[f.owner_id].clone()
                                        + __content.string(2682)
                                        + &commod_label
                                        + __content.string(2683);
                                    if g.cur_player == player_id {
                                        phys_seller_id = f.owner_id;
                                    }
                                    holdings = sum_commodity_holdings(g, player_id, 0, 2);
                                    if cast::<Ext, _>(holdings) <= cast::<Ext, _>(storage_limit) {
                                        break;
                                    }
                                }
                            }
                            f.rec_idx += 1;
                        }
                    }
                    if !((cast::<Ext, _>(storage_limit)) < cast::<Ext, _>(holdings)) {
                        break;
                    }
                }
                if 0 < phys_seller_id {
                    let msg_text = s(__content.string(2684))
                        + &commod_label
                        + __content.string(2685)
                        + __content.string(2686)
                        + &g.ent_name[phys_seller_id]
                        + ".";
                    let title = s(__content.string(2687)) + &mc(&f.commod_code) + __content.string(2688);
                    ui.msgbox(&msg_text, 0x2010, &title);
                    phys_seller_id = 0;
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, 0, 0);
                }
            }
        }
        player_id += 1;
    }
}
