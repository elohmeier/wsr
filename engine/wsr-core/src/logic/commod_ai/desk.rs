//! The commodity desk's futures orders: `FuturesBuyOrCover` and `FuturesSellOrShort`. Ported are the
//! closing trades (`tran_type` 2: buy back a short / sell a long position, interactive), which the margin
//! call and negative-cash flows use; opening a position (`tran_type` 1) is ported in
//! [`crate::logic::trading::futures_trade`] and called from here.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::neg_multiply,
    clippy::needless_range_loop,
    clippy::manual_clamp,
    clippy::manual_range_contains,
    clippy::field_reassign_with_default,
    clippy::too_many_arguments
)]
use crate::pb::real::{Ext, cast};


use super::limits::apply_futures_trade_impact;
use super::prelude::*;
use crate::logic::finance::calc_entity_financials;
use crate::logic::futures::{calc_futures_contract_value, get_commod_data, get_futures_price, recalc_commod_margin_balances};
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::show_futures_positions;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::sim_entity::transactions::{book_futures_gain_loss, settle_futures_contract};
use crate::pb::numeric::Cux;

/// Commodity label (`STOCK INDEX` for STOCKS) and its lower-case text (`MCASE$` for crypto).
fn labels(g: &Globals) -> (PbStr, PbStr) {
    let __content = crate::content::current();
    let mut commod_label = g.cur_commodity.clone();
    if commod_label == "STOCKS" {
        commod_label = s(__content.string(2499));
    }
    let mut commod_lc = lc(&commod_label);
    if is_crypto(&g.cur_commodity) {
        commod_lc = mc(&g.cur_commodity);
    }
    (commod_label, commod_lc)
}

/// The confirmation text's gain/loss sentence and the label replacement shared by both routines.
fn finish_confirm(g: &Globals, mut confirm_text: PbStr, gain_loss: Ext, commod_label: &PbStr) -> PbStr {
    let __content = crate::content::current();
    let mut gain_word = s("gain");
    if gain_loss < Ext::from_f64(0.0) {
        gain_word = s("loss");
    }
    if gain_loss.abs() < sf(0.1f32) {
        gain_word = PbStr::new();
    }
    if !gain_word.is_empty() {
        confirm_text = confirm_text
            + "A "
            + &gain_word
            + " of "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(gain_loss, b"* ###.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(2500);
    }
    if *commod_label == __content.string(2499) || *commod_label == "BITCOIN" || *commod_label == "ETHEREUM" {
        confirm_text = replace(confirm_text.as_bytes(), b"COMMODITY", commod_label.as_bytes());
    }
    confirm_text
}

/// `"$" + price` (USA) or `"US$ " + price`, the price formatted with 2 decimals (crypto: none).
fn price_text(g: &Globals, commod: &PbStr, cur_price: Ext) -> PbStr {
    let mut t = fmt_trim(cur_price, b"* ###,.00");
    if is_crypto(commod) {
        t = fmt_trim(cur_price, b"* ###,");
    }
    if g.currency_code == "USA" { s("$") + &t } else { s("US$ ") + &t }
}

/// Port of FuturesBuyOrCover (0x61cebb) ("FrontBuyCommod"), closing path (`tran_type` 2).
/// Buys back the short futures record `rec_no` of `entity_id` (or the one picked in
/// `ShowFuturesPositions(entity, 3)` when `rec_no < 1`): one contract after an OK/Cancel MSGBOX, several
/// via INPUTBOX$ (all: `SettleFuturesContract`; part: pays the cost share minus the value share and 1%,
/// books the gain, rewrites the record, news). Then a confirmation MSGBOX, the market impact (direction
/// 1) and packing of the record lists. Other `tran_type`s show "No TranType specified …".
///
/// `tran_type` 1 (open a long position) is [`crate::logic::trading::futures_trade::futures_buy_open`].
pub fn futures_buy_or_cover(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, tran_type: i32, entity_id: i32, rec_no: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError only
    let mode = tran_type;
    let buyer_id = entity_id;
    let commod = g.cur_commodity.clone();
    let (commod_label, commod_lc) = labels(g);
    recalc_commod_margin_balances(g, ui, rng, buyer_id);
    if mode == 1 {
        // opening purchase: logic::trading::futures_trade
        crate::logic::trading::futures_trade::futures_buy_open(g, ui, rng, buyer_id);
        return;
    }
    if mode != 2 {
        let mut t = s(__content.string(2501));
        show_quick_message(g, ui, &mut t);
        return;
    }
    let mut msg_text = PbStr::new();
    let rec_idx = if rec_no < 1 { show_futures_positions(g, ui, rng, buyer_id, 3) } else { rec_no };
    'trade: {
        if rec_idx == 0 || !(cast::<Ext, _>(rec_idx) <= val(g.futures_recs[0].as_bytes())) {
            break 'trade;
        }
        let contract_sel = g.futures_recs[rec_idx].clone();
        let short_qty = to_quad(field_val(&contract_sel, 3) * -Ext::from_f64(1.0));
        let deliv_month = to_long(field_val(&contract_sel, 4));
        let deliv_year = to_long(field_val(&contract_sel, 5));
        if commod.is_empty() || deliv_month == 0 {
            return;
        }
        let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 2);
        if p == "ERROR" {
            return;
        }
        let mut cur_price_str = price_text(g, &commod, val(p.as_bytes()));
        let gain_loss: Ext;
        let qty: i64;
        if short_qty == 1 {
            let mut subj_verb = s("You are");
            if 10 < buyer_id {
                subj_verb = g.ent_symbol[buyer_id].clone() + " is";
            }
            msg_text = subj_verb
                + __content.string(2502)
                + &commod_lc
                + __content.string(2503)
                + &g.month_name[deliv_month]
                + ","
                + &str_s(cast::<Ext, _>(deliv_year))
                + __content.string(2504)
                + &cur_price_str;
            if right(msg_text.as_bytes(), 1) != "." {
                msg_text = msg_text + ".";
            }
            msg_text = msg_text + "\r" + "\r" + "Click " + "\"" + "OK" + "\"" + __content.string(2505);
            let title = s(__content.string(2506)) + &mc(&commod_label) + __content.string(2507);
            if ui.msgbox(&msg_text, 8257, &title) == 2 {
                return;
            }
            g.futures_settle_gain = Ext::from_f64(0.0);
            settle_futures_contract(g, ui, rng, rec_idx, 1, 0, 0);
            gain_loss = g.futures_settle_gain;
            qty = 1;
        } else {
            let mut info_text = s(" do you");
            if 10 < buyer_id {
                info_text = s(" does ") + &g.ent_name[buyer_id];
            }
            msg_text = s(__content.string(2508))
                + &commod_lc
                + __content.string(2509)
                + &g.month_name[deliv_month]
                + ","
                + &str_s(cast::<Ext, _>(deliv_year))
                + &info_text
                + __content.string(2510)
                + &str_s(cast::<Ext, _>(short_qty))
                + __content.string(2511)
                + &trim(field(&contract_sel, 7).as_bytes())
                + __content.string(2512)
                + &cur_price_str
                + ".";
            // (UI) DisableMainControls
            let title = g.ent_symbol[buyer_id].clone() + " -- " + &mc(&commod_label) + __content.string(2513);
            let n = loop {
                let default = fmt_trim(cast::<Ext, _>(short_qty), b"* ###,") + __content.string(2514);
                let input_str = ui.inputbox(&msg_text, &title, &default);
                let n = to_quad(val(remove_any(input_str.as_bytes(), b"$ ,").as_bytes()).abs());
                if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
                    msg_text = PbStr::new();
                    break 'trade;
                }
                if cast::<Ext, _>(n) <= cast::<Ext, _>(short_qty) {
                    break n;
                }
            };
            qty = n;
            if qty == short_qty {
                g.futures_settle_gain = Ext::from_f64(0.0);
                settle_futures_contract(g, ui, rng, rec_idx, 1, 0, 0);
                gain_loss = g.futures_settle_gain;
            } else {
                let mut cover_frac: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE
                if cast::<Ext, _>(short_qty) != Ext::from_f64(0.0) {
                    cover_frac = crate::pb::numeric::double(cast::<Ext, _>(qty) / cast::<Ext, _>(short_qty));
                }
                let remaining_qty = to_quad(cast::<Ext, _>(short_qty) - cast::<Ext, _>(qty));
                let total_value = val_ext(field(&contract_sel, 6).as_bytes());
                let cost_portion_x = total_value.fmul(Ext80::from_ext(cover_frac));
                let cost_portion = cost_portion_x.into_ext();
                let cur_value = calc_futures_contract_value(g, ui, rng, &contract_sel).abs();
                let mut gain = cost_portion - cover_frac * cur_value;
                g.cash[buyer_id] = gain + g.cash[buyer_id];
                gain = gain - (cover_frac * cur_value + cost_portion) * sf(0.01f32);
                g.cash[buyer_id] = g.cash[buyer_id] - cur_value * sf(0.01f32) * cover_frac;
                book_futures_gain_loss(g, ui, rng, gain, buyer_id, &commod, 1);
                gain_loss = gain;
                let mut contract_price = Cux(0);
                contract_price.set(field_val(&contract_sel, 7));
                g.futures_recs[rec_idx] = commod.clone()
                    + "|"
                    + &str_trim(cast::<Ext, _>(buyer_id))
                    + "|"
                    + &str_trim(-(cast::<Ext, _>(remaining_qty)))
                    + "|"
                    + &str_trim(cast::<Ext, _>(deliv_month))
                    + "|"
                    + &str_trim(cast::<Ext, _>(deliv_year))
                    + "|"
                    + &str_trim_ext(total_value.fsub(cost_portion_x))
                    + "|"
                    + &str_trim(contract_price.get());
                g.news_msg = g.ent_symbol[buyer_id].clone()
                    + __content.string(2515)
                    + &str_s(cast::<Ext, _>(deliv_month))
                    + "/"
                    + &str_trim(cast::<Ext, _>(deliv_year))
                    + " "
                    + &commod_lc
                    + __content.string(2516)
                    + &cur_price_str;
                post_news(g, ui, 0, 0);
            }
        }
        recalc_commod_margin_balances(g, ui, rng, buyer_id);
        let data = get_commod_data(g, ui, &commod, 4);
        if data == "ERROR" {
            return;
        }
        let unit_name = field(&data, 2);
        if commod.is_empty() {
            return;
        }
        let p = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 3);
        if p == "ERROR" {
            return;
        }
        cur_price_str = price_text(g, &commod, val(p.as_bytes()));
        msg_text = s(" (");
        if Ext::from_f64(1.0) < cast::<Ext, _>(short_qty) {
            msg_text = s("s (");
        }
        let mut subj_verb = s(__content.string(1506));
        if 10 < buyer_id {
            subj_verb = s("Buyer (") + &g.ent_symbol[buyer_id] + ") has";
        }
        let confirm_text = s(__content.string(2517))
            + "\r"
            + "\r"
            + __content.string(2518)
            + &uc(&g.ent_name[buyer_id])
            + "\r"
            + "\r"
            + &subj_verb
            + __content.string(2519)
            + &fmt_trim(cast::<Ext, _>(qty), b"* ##,")
            + " "
            + &commod_lc
            + __content.string(2520)
            + __content.string(2521)
            + &msg_text
            + __content.string(2522)
            + &g.month_name[deliv_month]
            + ", "
            + &str_trim(cast::<Ext, _>(deliv_year))
            + ")"
            + __content.string(1881)
            + __content.string(2523)
            + &cur_price_str
            + " per "
            + &unit_name
            + "."
            + "\r"
            + "\r";
        let confirm_text = finish_confirm(g, confirm_text, gain_loss, &commod_label);
        ui.msgbox(&confirm_text, 8256, &(commod_label.clone() + __content.string(2524)));
        pack_holding_arrays(g, ui);
        g.turn_action_count += 1;
        apply_futures_trade_impact(g, ui, rng, &commod, 1, qty);
        pack_holding_arrays(g, ui);
        return;
    }
    // LAB_00620dc5: DIALOG END DlgCommodDesk
    ui.end_dialog(crate::ui::DialogKind::CommodityDesk);
    if !msg_text.is_empty() {
        show_quick_message(g, ui, &mut msg_text);
    }
    pack_holding_arrays(g, ui);
}

/// Port of FuturesSellOrShort (0x620f39) ("FrontSellCommod"), closing path (`tran_type` 2).
/// Sells the long futures record `rec_no` of `entity_id` (or the one picked in
/// `ShowFuturesPositions(entity, 2)` when `rec_no < 1`): one contract after an OK/Cancel MSGBOX, several
/// via INPUTBOX$ (all: `SettleFuturesContract`; part: receives the value share minus the cost share and
/// 1%, books the gain, rewrites the record, news). Then a confirmation MSGBOX, packing and the market
/// impact (direction -1). Recomputes the entity's margin and the current player's financials first.
///
/// `tran_type` 1 (open a short position) is [`crate::logic::trading::futures_trade::futures_sell_open`].
pub fn futures_sell_or_short(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, tran_type: i32, entity_id: i32, rec_no: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError only
    let mode = tran_type;
    let seller_id = entity_id;
    let commod = g.cur_commodity.clone();
    let (commod_label, commod_lc) = labels(g);
    recalc_commod_margin_balances(g, ui, rng, seller_id);
    let cp = g.cur_player;
    calc_entity_financials(g, ui, rng, cp);
    let player_net_worth = g.net_worth;
    if mode == 1 {
        // opening short sale: logic::trading::futures_trade
        crate::logic::trading::futures_trade::futures_sell_open(g, ui, rng, seller_id, player_net_worth);
        return;
    }
    if mode != 2 {
        let mut t = s(__content.string(2525));
        show_quick_message(g, ui, &mut t);
        return;
    }
    let mut msg_text = PbStr::new();
    let rec_idx = if rec_no < 1 { show_futures_positions(g, ui, rng, seller_id, 2) } else { rec_no };
    'trade: {
        if rec_idx == 0 || !(cast::<Ext, _>(rec_idx) <= val(g.futures_recs[0].as_bytes())) {
            break 'trade;
        }
        let fut_rec = g.futures_recs[rec_idx].clone();
        let long_qty = to_quad(field_val(&fut_rec, 3));
        let deliv_month = to_long(field_val(&fut_rec, 4));
        let deliv_year = to_long(field_val(&fut_rec, 5));
        if commod.is_empty() || deliv_month == 0 {
            return;
        }
        let cur_price_str = get_futures_price(g, ui, rng, &commod, deliv_month, deliv_year, 4);
        if cur_price_str == "ERROR" {
            return;
        }
        let gain_loss: Ext;
        let qty: i64;
        if long_qty == 1 {
            let mut subj_verb = s("You are");
            if 10 < seller_id {
                subj_verb = g.ent_symbol[seller_id].clone() + " is";
            }
            msg_text = subj_verb
                + __content.string(2526)
                + &commod_lc
                + __content.string(2503)
                + &g.month_name[deliv_month]
                + ","
                + &str_s(cast::<Ext, _>(deliv_year))
                + __content.string(2527)
                + &cur_price_str
                + "."
                + "\r"
                + "\r"
                + "Click "
                + "\""
                + "OK"
                + "\""
                + __content.string(2528);
            let title = s(__content.string(2529)) + &mc(&commod_label) + __content.string(2507);
            if ui.msgbox(&msg_text, 8257, &title) == 2 {
                return;
            }
            g.futures_settle_gain = Ext::from_f64(0.0);
            settle_futures_contract(g, ui, rng, rec_idx, 1, 0, 0);
            gain_loss = g.futures_settle_gain;
            qty = 1;
        } else {
            let mut info_text = s(" do you");
            if 10 < seller_id {
                info_text = s(" does ") + &g.ent_name[seller_id];
            }
            msg_text = s(__content.string(2508))
                + &commod_lc
                + __content.string(2509)
                + &g.month_name[deliv_month]
                + ","
                + &str_s(cast::<Ext, _>(deliv_year))
                + &info_text
                + __content.string(2530)
                + &str_s(cast::<Ext, _>(long_qty))
                + __content.string(2511)
                + &trim(field(&fut_rec, 7).as_bytes())
                + __content.string(2531)
                + &cur_price_str
                + ".";
            // (UI) DisableMainControls
            let title = g.ent_symbol[seller_id].clone() + " -- " + &mc(&commod_label) + __content.string(2532);
            let n = loop {
                let default = fmt_trim(cast::<Ext, _>(long_qty), b"* ###,") + __content.string(2514);
                let input_str = ui.inputbox(&msg_text, &title, &default);
                let n = to_quad(val(remove_any(input_str.as_bytes(), b"$ ,").as_bytes()).abs());
                if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
                    msg_text = PbStr::new();
                    break 'trade;
                }
                if cast::<Ext, _>(n) <= cast::<Ext, _>(long_qty) {
                    break n;
                }
            };
            qty = n;
            if qty == long_qty {
                g.futures_settle_gain = Ext::from_f64(0.0);
                settle_futures_contract(g, ui, rng, rec_idx, 1, 0, 0);
                gain_loss = g.futures_settle_gain;
            } else {
                let mut sell_frac: Ext = crate::pb::numeric::double(Ext::from_f64(0.0)); // DOUBLE
                if cast::<Ext, _>(long_qty) != Ext::from_f64(0.0) {
                    sell_frac = crate::pb::numeric::double(cast::<Ext, _>(qty) / cast::<Ext, _>(long_qty));
                }
                let remaining_qty = to_quad(cast::<Ext, _>(long_qty) - cast::<Ext, _>(qty));
                let total_value = val_ext(field(&fut_rec, 6).as_bytes());
                let cost_portion_x = total_value.fmul(Ext80::from_ext(sell_frac));
                let cost_portion = cost_portion_x.into_ext();
                let cur_value = calc_futures_contract_value(g, ui, rng, &fut_rec);
                let mut gain = sell_frac * cur_value - cost_portion;
                g.cash[seller_id] = gain + g.cash[seller_id];
                gain = gain - (sell_frac * cur_value + cost_portion) * sf(0.01f32);
                g.cash[seller_id] = g.cash[seller_id] - cur_value * sf(0.01f32) * sell_frac;
                book_futures_gain_loss(g, ui, rng, gain, seller_id, &commod, 1);
                gain_loss = gain;
                let mut contract_price = Cux(0);
                contract_price.set(field_val(&fut_rec, 7));
                g.futures_recs[rec_idx] = commod.clone()
                    + "|"
                    + &str_trim(cast::<Ext, _>(seller_id))
                    + "|"
                    + &str_trim(cast::<Ext, _>(remaining_qty))
                    + "|"
                    + &str_trim(cast::<Ext, _>(deliv_month))
                    + "|"
                    + &str_trim(cast::<Ext, _>(deliv_year))
                    + "|"
                    + &str_trim_ext(total_value.fsub(cost_portion_x))
                    + "|"
                    + &str_trim(contract_price.get());
                g.news_msg = g.ent_symbol[seller_id].clone()
                    + __content.string(2533)
                    + &str_s(cast::<Ext, _>(deliv_month))
                    + "/"
                    + &str_trim(cast::<Ext, _>(deliv_year))
                    + " "
                    + &commod_lc
                    + __content.string(2534)
                    + &cur_price_str
                    + ".";
                post_news(g, ui, 0, 0);
            }
        }
        let data = get_commod_data(g, ui, &commod, 6);
        if data == "ERROR" {
            return;
        }
        let unit_name = field(&data, 2);
        msg_text = s(" (");
        if Ext::from_f64(1.0) < cast::<Ext, _>(long_qty) {
            msg_text = s("s (");
        }
        let mut subj_verb = s(__content.string(1506));
        if 10 < seller_id {
            subj_verb = s(__content.string(2535)) + &g.ent_symbol[seller_id] + ") has";
        }
        let confirm_text = s(__content.string(2536))
            + "\r"
            + "\r"
            + __content.string(2537)
            + &uc(&g.ent_name[seller_id])
            + "\r"
            + "\r"
            + &subj_verb
            + " sold "
            + &fmt_trim(cast::<Ext, _>(qty), b"* ##,")
            + " "
            + &commod_lc
            + __content.string(2538)
            + &msg_text
            + __content.string(2539)
            + "of "
            + &g.month_name[deliv_month]
            + ", "
            + &str_trim(cast::<Ext, _>(deliv_year))
            + ") "
            + __content.string(2540)
            + &cur_price_str
            + " per "
            + &unit_name
            + "."
            + "\r"
            + "\r";
        let confirm_text = finish_confirm(g, confirm_text, gain_loss, &commod_label);
        ui.msgbox(&confirm_text, 8256, &(commod_label.clone() + __content.string(2524)));
        pack_holding_arrays(g, ui);
        g.turn_action_count += 1;
        apply_futures_trade_impact(g, ui, rng, &commod, -1, qty);
        return;
    }
    // LAB_00624abc: DIALOG END DlgCommodDesk
    ui.end_dialog(crate::ui::DialogKind::CommodityDesk);
    if !msg_text.is_empty() {
        show_quick_message(g, ui, &mut msg_text);
    }
    pack_holding_arrays(g, ui);
}
