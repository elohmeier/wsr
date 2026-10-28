//! Physical commodities and crypto holdings (`PhysCommodRec`): buying (`BuyPhysCommod`), selling
//! (`SellPhysCommod`), the AI trader (`AiTradePhysCommod`) and the monthly storage charge with grain
//! spoilage (`ChargeCommodStorageCosts`).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::needless_late_init,
    clippy::if_same_then_else,
    clippy::neg_multiply,
    clippy::needless_range_loop,
    clippy::manual_clamp,
    clippy::manual_range_contains,
    clippy::field_reassign_with_default
)]
use crate::pb::real::{Ext, cast};


use super::limits::{apply_futures_trade_impact, calc_commod_trade_limit};
use super::prelude::*;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::economy::world_events::add_remove_phys_commod_rec;
use crate::logic::finance::{
    calc_entity_financials, find_phys_commod_rec, get_controller_type, get_managing_player,
    get_phys_commod_value,
};
use crate::logic::futures::{
    calc_phys_commod_storage_cost, format_commod_price, get_commod_data, sum_commodity_holdings,
    update_metals_price_base,
};
use crate::logic::report::{describe_defunct_entity, show_quick_message};
use crate::logic::sim_entity::transactions::book_futures_gain_loss;
use crate::logic::valuation::price_trend_streak;
use crate::pb::numeric::fix;

// ---------------------------------------------------------------------------------------------------------
// SellPhysCommod
// ---------------------------------------------------------------------------------------------------------

/// Port of SellPhysCommod (0x65db62) ("FrontSellPhysCommod").
/// Sells physical commodity/crypto of `entity_id`: record `rec_idx_arg` (if owned by the entity), or with
/// `rec_idx_arg = 0` its `CurCommodity` record. Interactive (`silent = 0`: INPUTBOX for the units,
/// confirmation MSGBOX, news) or programmatic: `pct_to_sell` percent of the units (0: 20%; 2 units or
/// fewer: everything). Proceeds = the position value (`GetPhysCommodValue`) pro rata, less 1% unless a
/// delivery (`delivery_mode <> 0`); the gain is booked (book mode 3; deliveries mode 2 with
/// `FuturesBatchGain`, 0 for -1). Large sales lower the target price 1% (crypto 2%); non-deliveries also
/// move the market (`ApplyFuturesTradeImpact`, direction -1). A defunct entity only gets its fate shown.
pub fn sell_phys_commod(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    rec_idx_arg: i32,
    silent: i32,
    pct_to_sell: f32,
    delivery_mode: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError only
    let seller_id = entity_id;
    if g.entity_status[seller_id] == 2 {
        describe_defunct_entity(g, ui, seller_id);
        return;
    }
    let rec_arg = rec_idx_arg;
    let mut units_calc: Ext = cast::<Ext, _>(pct_to_sell);
    if val(g.phys_commod_rec[0].as_bytes()) <= Ext::from_f64(0.0) {
        if silent == 0 {
            let t = s("Error: ") + &g.ent_name[seller_id] + __content.string(2689);
            ui.msgbox(&t, 0x2010, &s(__content.string(2690)));
        }
        return;
    }
    let mut rec_idx = 0;
    let mut commod_name = PbStr::new();
    let mut rec_text = PbStr::new();
    if rec_arg == 0 && !g.cur_commodity.is_empty() {
        commod_name = g.cur_commodity.clone();
        rec_idx = find_phys_commod_rec(g, seller_id, &commod_name);
        if 0 < rec_idx {
            rec_text = g.phys_commod_rec[rec_idx].clone();
        }
    } else {
        if val(g.phys_commod_rec[0].as_bytes()) < cast::<Ext, _>(rec_arg) {
            let t = s(__content.string(2691))
                + &g.cur_commodity
                + " Y& ="
                + &str_s(cast::<Ext, _>(rec_arg))
                + __content.string(2692)
                + &g.phys_commod_rec[0];
            ui.msgbox(&t, 0, &PbStr::new());
            return;
        }
        rec_text = g.phys_commod_rec[rec_arg].clone();
        if field_val(&rec_text, 2) == cast::<Ext, _>(seller_id) {
            rec_idx = rec_arg;
            commod_name = trim(field(&rec_text, 1).as_bytes());
        }
    }
    if rec_idx == 0 {
        let mut msg_text = g.active_entity_name.clone() + __content.string(2693);
        let mut commod_label = lc(&commod_name);
        if is_crypto(&commod_name) {
            commod_label = mc(&commod_name);
        }
        if !commod_name.is_empty() {
            msg_text = replace(
                msg_text.as_bytes(),
                __content.bytes(2694),
                (s(__content.string(2695)) + &commod_label).as_bytes(),
            );
        }
        if silent == 0 {
            show_quick_message(g, ui, &mut msg_text);
        }
        return;
    }
    let mut msg_text = get_commod_data(g, ui, &commod_name, 8);
    let unit_name = trim(field(&msg_text, 2).as_bytes());
    let unit_size = to_long(field_val(&msg_text, 1));
    let units_owned = to_quad(field_val(&rec_text, 3));
    let mut total_cost = val_ext(field(&rec_text, 4).as_bytes()); // EXT
    let mut commod_label = lc(&commod_name);
    if is_crypto(&commod_name) {
        commod_label = mc(&commod_name);
    }
    let mut unit_word = s("unit");
    if Ext::from_f64(1.0) < cast::<Ext, _>(units_owned) {
        unit_word = s("units");
    }
    let units_to_sell: i64;
    if silent == 0 {
        loop {
            msg_text = s(__content.string(2696))
                + &fmt_trim(cast::<Ext, _>(units_owned), b"* ##############,")
                + " '"
                + &unit_word
                + __content.string(2697)
                + &commod_label
                + ". "
                + __content.string(2698)
                + &fmt_trim(cast::<Ext, _>(unit_size), b"* ###,")
                + " "
                + &unit_name
                + "s of "
                + &commod_label
                + "."
                + "\r\n"
                + "\r\n"
                + __content.string(2699);
            if is_crypto(&commod_name) {
                msg_text = remove(msg_text.as_bytes(), __content.bytes(2640));
            }
            let title = s(__content.string(2700)) + &mc(&commod_name);
            let default = fmt_trim(cast::<Ext, _>(units_owned), b"* ###,") + " " + &unit_word;
            let input_text = ui.inputbox(&msg_text, &title, &default);
            let n = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,%").as_bytes()));
            if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
                return;
            }
            if !((cast::<Ext, _>(units_owned)) < cast::<Ext, _>(n)) {
                units_to_sell = n;
                break;
            }
        }
    } else {
        if units_calc == Ext::from_f64(0.0) {
            units_calc = cast::<Ext, _>(units_owned) * sf(0.2f32);
        } else {
            units_calc = cast::<Ext, _>(units_owned) * (sf(0.01f32) * units_calc);
        }
        if units_calc <= Ext::from_f64(2.0) {
            units_calc = cast::<Ext, _>(units_owned);
        }
        units_to_sell = to_quad(units_calc);
    }
    let pos_value = get_phys_commod_value(g, seller_id, &commod_name);
    let mut sale_fraction: Ext = crate::pb::numeric::double(Ext::from_f64(1.0)); // DOUBLE
    let cost_basis;
    if cast::<Ext, _>(units_to_sell) == cast::<Ext, _>(units_owned) {
        cost_basis = total_cost.into_ext();
        add_remove_phys_commod_rec(g, b"", rec_idx);
    } else {
        // (units_owned = 0 raises error 151 -> ON ERROR exit; unreachable: the sale would equal the holding)
        sale_fraction = crate::pb::numeric::double(cast::<Ext, _>(units_to_sell) / cast::<Ext, _>(units_owned));
        let cost_basis_x = total_cost.fmul(Ext80::from_ext(sale_fraction));
        cost_basis = cost_basis_x.into_ext();
        let units_left = to_quad(cast::<Ext, _>(units_owned) - cast::<Ext, _>(units_to_sell));
        total_cost = total_cost.fsub(cost_basis_x);
        g.phys_commod_rec[rec_idx] = commod_name.clone()
            + "|"
            + &str_trim(cast::<Ext, _>(seller_id))
            + "|"
            + &str_trim(cast::<Ext, _>(units_left))
            + "|"
            + &str_trim_ext(total_cost);
    }
    let mut proceeds = pos_value * sale_fraction;
    if delivery_mode == 0 {
        proceeds = sf(0.99f32) * proceeds;
    }
    g.cash[seller_id] = proceeds + g.cash[seller_id];
    let gain_loss = proceeds - cost_basis;
    let mut book_type = 3;
    if delivery_mode != 0 {
        book_type = 2;
        g.futures_batch_gain = gain_loss;
        if delivery_mode == -1 {
            g.futures_batch_gain = Ext::from_f64(0.0);
        }
    }
    book_futures_gain_loss(g, ui, rng, gain_loss, seller_id, &commod_name, book_type);
    let price_str = format_commod_price(g, &commod_name);
    let er = g.exchange_rate;
    if commod_name == "OIL" && er * Ext::from_f64(750.0) < proceeds {
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.oil_target_price) * sf(0.99f32)));
    }
    if commod_name == "GOLD" && er * Ext::from_f64(90.0) < proceeds {
        g.gold_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.gold_target_price) * sf(0.99f32)));
    }
    if commod_name == "SILVER" && er * Ext::from_f64(150.0) < proceeds {
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.silver_target_price) * sf(0.99f32)));
    }
    if commod_name == "WHEAT" && er * Ext::from_f64(250.0) < proceeds {
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.wheat_target_price) * sf(0.99f32)));
    }
    if commod_name == "CORN" && er * Ext::from_f64(250.0) < proceeds {
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.corn_target_price) * sf(0.99f32)));
    }
    if commod_name == "BITCOIN" && er * Ext::from_f64(750.0) < proceeds {
        g.bitcoin_target_price = to_long(cast::<Ext, _>(g.bitcoin_target_price) * sf(0.98f32));
    }
    if commod_name == "ETHEREUM" && er * Ext::from_f64(750.0) < proceeds {
        g.ethereum_target_price = to_long(cast::<Ext, _>(g.ethereum_target_price) * sf(0.98f32));
    }
    msg_text = s(" (");
    if Ext::from_f64(1.0) < cast::<Ext, _>(units_to_sell) {
        msg_text = s("s (");
    }
    unit_word = s(__content.string(1506));
    if 10 < seller_id {
        unit_word = s(__content.string(2535)) + &g.ent_symbol[seller_id] + ") has";
    }
    let mut confirm_text = s(__content.string(2701))
        + "\r"
        + "\r"
        + __content.string(2537)
        + &uc(&g.ent_name[seller_id])
        + "\r"
        + "\r"
        + &unit_word
        + " sold "
        + &fmt_trim(cast::<Ext, _>(units_to_sell), b"* ##,")
        + " "
        + &commod_label
        + " unit"
        + &msg_text
        + &fmt_trim(cast::<Ext, _>(unit_size), b"* ##############,")
        + " "
        + &unit_name
        + __content.string(2702)
        + __content.string(2703)
        + &price_str
        + __content.string(2704)
        + &unit_name
        + __content.string(2705)
        + "\r"
        + "\r"
        + __content.string(2706)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(proceeds, b"* #############,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + "\r"
        + "\r";
    if is_crypto(&commod_name) {
        confirm_text = replace(confirm_text.as_bytes(), __content.bytes(2707), commod_name.as_bytes());
    }
    if Ext::from_f64(10.0) < gain_loss.abs() {
        let mut gain_str = fmt_trim(gain_loss, b"* ##############,.00");
        let mut gain_word = s("gain");
        if gain_loss < Ext::from_f64(0.0) {
            gain_word = s("loss");
            gain_str = remove(gain_str.as_bytes(), b"-");
        }
        confirm_text = confirm_text
            + "A "
            + &gain_word
            + " of "
            + &trim(g.currency_symbol.as_bytes())
            + &gain_str
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(2708);
    }
    if silent == 0 {
        let title = s(__content.string(2709)) + &commod_name + __content.string(2710);
        ui.msgbox(&confirm_text, 0x2040, &title);
        g.news_msg = g.ent_symbol[seller_id].clone() + __content.string(2711) + &commod_label + __content.string(2712);
        if is_crypto(&commod_name) {
            g.news_msg = remove(g.news_msg.as_bytes(), __content.bytes(2640));
        }
        if len(g.news_msg.as_bytes()) < 47 {
            g.news_msg = g.news_msg.clone() + " at $" + &price_str + __content.string(2704) + &unit_name;
        }
        post_news(g, ui, 0, 0);
        g.turn_action_count += 1;
    }
    if delivery_mode == 0 {
        apply_futures_trade_impact(g, ui, rng, &commod_name, -1, units_to_sell);
    }
}

// ---------------------------------------------------------------------------------------------------------
// BuyPhysCommod
// ---------------------------------------------------------------------------------------------------------

/// Port of BuyPhysCommod_StorageFeeStr (GOSUB 0x661560).
/// Annual storage fee per unit name as text: OIL 2.50, GOLD 1.00, else 0.25.
fn storage_fee_str(g: &Globals) -> PbStr {
    let storage_fee: Ext = match g.cur_commodity.as_bytes() {
        b"OIL" => sf(2.5f32),
        b"GOLD" => Ext::from_f64(1.0),
        _ => sf(0.25f32),
    };
    fmt_trim(storage_fee, b"#.00")
}

/// Port of BuyPhysCommod (0x65f521) with its GOSUB BuyPhysCommod_StorageFeeStr (0x661560) and exit stub
/// BuyPhysCommod_Exit (0x6616a7).
/// Buys `CurCommodity` (not STOCKS) for `entity_id` at the spot price plus 1% (`ExchangeRate`, millions):
/// interactive for the active entity (`ai_mode = 0`: INPUTBOX, confirmation; `BorrowToCoverPurchase`
/// for a cash shortfall), or for the AI (`ai_mode > 0`: only without an existing record and while the
/// physical holdings are at most 10% of net worth; budget = min(20% of net worth, half the liquid
/// funds)). Limits: the exchange limits (`CalcCommodTradeLimit`, probe 99 first) or, in a peak-oil
/// crisis below 5.3, 100 oil units per group. Adds/updates the record, news (not for crypto) and the
/// market impact (direction 1).
///
/// The AI purchase only pays when cash plus T-bills cover the cost; otherwise the units are added for
/// free (see BUGS.md).
pub fn buy_phys_commod(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32, ai_mode: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError only
    let buyer_id = entity_id;
    let ai_flag = ai_mode;
    if g.active_entity != buyer_id && ai_flag == 0 {
        return;
    }
    if g.cur_commodity == "STOCKS" || g.cur_commodity.is_empty() {
        return;
    }
    if 10 < buyer_id && g.industry_num[buyer_id] < 3 {
        return;
    }
    if 0 < ai_flag {
        calc_entity_financials(g, ui, rng, buyer_id);
        let limit = g.net_worth * sf(0.1f32);
        let v = get_phys_commod_value(g, buyer_id, &PbStr::new());
        if !(v <= limit) {
            return;
        }
    }
    let mut msg_text = g.cur_commodity.clone();
    if get_phys_commod_value(g, buyer_id, &msg_text) == Ext::from_f64(0.0) {
        if Ext::from_f64(998.0) <= val(g.phys_commod_rec[0].as_bytes()) {
            msg_text = s(__content.string(2713))
                + __content.string(2714)
                + __content.string(2715)
                + __content.string(2716);
            if ai_flag == 0 {
                show_quick_message(g, ui, &mut msg_text);
            }
            return;
        }
    }
    let mut phrase_text = get_commod_data(g, ui, &msg_text, 9);
    let unit_size = to_long(val(field(&phrase_text, 1).as_bytes()));
    let unit_size_str = fmt_trim(cast::<Ext, _>(unit_size), b"* ###,");
    let unit_name = field(&phrase_text, 2);
    let mut unit_price: Ext = crate::pb::numeric::double(match g.cur_commodity.as_bytes() {
        b"OIL" => Ext::from_f64(101.0) * cast::<Ext, _>(g.oil_price),
        b"GOLD" => sf(1.01f32) * cast::<Ext, _>(g.gold_price),
        b"SILVER" => sf(1.01f32) * cast::<Ext, _>(g.silver_price),
        b"WHEAT" => sf(1.01f32) * cast::<Ext, _>(g.wheat_price),
        b"CORN" => sf(1.01f32) * cast::<Ext, _>(g.corn_price),
        b"BITCOIN" => sf(1.01f32) * cast::<Ext, _>(g.bitcoin_price),
        b"ETHEREUM" => sf(1.01f32) * cast::<Ext, _>(g.ethereum_price),
        _ => Ext::from_f64(0.0),
    }); // DOUBLE local, FSTP qword [ebp-0xa0], 0x65f8bb..0x65fa5f
    unit_price = crate::pb::numeric::double(sf(1e-06f32) * unit_price * g.exchange_rate);
    let unit_cost: Ext = crate::pb::numeric::double(cast::<Ext, _>(unit_size) * unit_price); // DOUBLE
    calc_entity_financials(g, ui, rng, buyer_id);
    let mut commod_label = lc(&g.cur_commodity);
    if is_crypto(&g.cur_commodity) {
        commod_label = mc(&g.cur_commodity);
    }
    if g.liquid_funds < unit_cost || g.liquid_funds <= Ext::from_f64(0.0) {
        if ai_flag == 0 {
            let mut t = g.active_entity_name.clone()
                + __content.string(2717)
                + __content.string(2718)
                + &commod_label
                + __content.string(2719);
            show_quick_message(g, ui, &mut t);
        }
        return;
    }
    let commod_key = g.cur_commodity.clone(); // ebp-0x164 (passed to FormatCommodPrice)
    let price_str = format_commod_price(g, &commod_key);
    let max_units_f = fix(g.liquid_funds / unit_cost);
    let mut max_units = to_quad(max_units_f);
    let cur = g.cur_commodity.clone();
    let rec_idx = find_phys_commod_rec(g, buyer_id, &cur);
    let mut rec_text = PbStr::new();
    let mut units_owned: i64 = 0;
    if 0 < rec_idx {
        rec_text = g.phys_commod_rec[rec_idx].clone();
        units_owned = to_quad(field_val(&rec_text, 3));
    }
    g.tmp_ext1 = Ext::from_f64(99.0);
    if cast::<Ext, _>(calc_commod_trade_limit(g, ui, buyer_id, 1, 20000)) != Ext::from_f64(99.0) {
        return;
    }
    let total_held = sum_commodity_holdings(g, buyer_id, 0, 2);
    let mut oil_cap_applied = 0;
    let mut qty_limit: i64;
    if !(g.cur_commodity == "OIL" && g.peak_oil_crisis == 1 && (cast::<Ext, _>(g.oil_price)) < sf(5.3f32)) {
        qty_limit = calc_commod_trade_limit(g, ui, buyer_id, 1, max_units);
        if (cast::<Ext, _>(qty_limit)) < cast::<Ext, _>(max_units) {
            if cast::<Ext, _>(qty_limit) <= Ext::from_f64(0.0) {
                return;
            }
            if Ext::from_f64(0.0) < cast::<Ext, _>(max_units) {
                max_units = qty_limit;
            }
        }
    } else {
        oil_cap_applied = 1;
        qty_limit = to_quad(Ext::from_f64(100.0) - cast::<Ext, _>(total_held));
        if (cast::<Ext, _>(qty_limit)) < Ext::from_f64(0.0) {
            qty_limit = 0;
        }
        if (cast::<Ext, _>(qty_limit)) < cast::<Ext, _>(max_units) {
            max_units = qty_limit;
            phrase_text = s(__content.string(2720));
            if g.currency_code != "USA" {
                phrase_text = replace(phrase_text.as_bytes(), __content.bytes(2721), __content.bytes(2722));
            }
            msg_text = s(__content.string(2723)) + &uc(&phrase_text) + "\r" + "\r";
            msg_text = msg_text
                + __content.string(2724)
                + "the "
                + &phrase_text
                + __content.string(2725)
                + __content.string(2726)
                + "\r"
                + "\r"
                + __content.string(2727);
            if Ext::from_f64(0.0) < cast::<Ext, _>(units_owned) {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(2728)
                    + &str_s(cast::<Ext, _>(units_owned))
                    + __content.string(2729);
                if cast::<Ext, _>(max_units) <= Ext::from_f64(0.0) {
                    msg_text = msg_text + __content.string(2730);
                } else {
                    msg_text = msg_text + __content.string(2731) + &str_s(cast::<Ext, _>(max_units)) + " more.";
                }
            }
            if ai_flag == 0 {
                ui.msgbox(&msg_text, 0x2010, &s(__content.string(2732)));
            }
            if cast::<Ext, _>(max_units) == Ext::from_f64(0.0) {
                return;
            }
        }
    }
    let units_to_buy: i64;
    let total_cost: Ext;
    let total_cost_x: Ext80;
    if 0 < ai_flag {
        if 0 < rec_idx {
            return;
        }
        let mut budget = g.net_worth * sf(0.2f32);
        if g.liquid_funds * sf(0.5f32) < budget {
            budget = g.liquid_funds * sf(0.5f32);
        }
        max_units = to_quad(fix(budget) / unit_cost);
        let mut n = max_units;
        if (cast::<Ext, _>(qty_limit)) < cast::<Ext, _>(max_units) {
            n = qty_limit;
        }
        if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
            return;
        }
        units_to_buy = n;
        total_cost_x = Ext80::from_i64(units_to_buy).fmul(Ext80::from_ext(unit_cost));
        total_cost = Ext::from(total_cost_x.into_ext());
        if !(total_cost > g.cash[buyer_id] + g.t_bills[buyer_id]) {
            g.cash[buyer_id] = g.cash[buyer_id] - total_cost;
        }
    } else {
        phrase_text = s("You");
        if 10 < buyer_id {
            phrase_text = g.active_entity_name.clone();
        }
        let mut unit_word = s("unit");
        if Ext::from_f64(1.0) < cast::<Ext, _>(max_units) {
            unit_word = s("units");
        }
        loop {
            msg_text = s(__content.string(2733))
                + &commod_label
                + __content.string(2734)
                + &unit_size_str
                + " "
                + &unit_name
                + "s of "
                + &commod_label
                + ". "
                + &phrase_text
                + __content.string(2735)
                + &fmt_trim(cast::<Ext, _>(max_units), b"* #############,")
                + " units."
                + "\r\n"
                + "\r\n"
                + __content.string(2736)
                + &commod_label
                + ".";
            let title = g.ent_symbol[g.active_entity].clone() + " -- " + &mc(&g.cur_commodity) + __content.string(2737);
            let default = fmt_trim(cast::<Ext, _>(max_units), b"* ###,") + " " + &unit_word;
            let input_text = ui.inputbox(&msg_text, &title, &default);
            let n = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,%").as_bytes()));
            if (cast::<Ext, _>(n)) < Ext::from_f64(1.0) {
                return;
            }
            if !((cast::<Ext, _>(max_units)) < cast::<Ext, _>(n)) {
                units_to_buy = n;
                break;
            }
        }
        phrase_text = s("You are");
        if 10 < buyer_id {
            phrase_text = g.ent_symbol[g.active_entity].clone() + " is";
        }
        unit_word = s("unit");
        if Ext::from_f64(1.0) < cast::<Ext, _>(units_to_buy) {
            unit_word = s("units");
        }
        let storage_fee = storage_fee_str(g);
        msg_text = s(__content.string(2518)) + &g.active_entity_name + "\r" + "\r";
        msg_text = msg_text
            + &phrase_text
            + __content.string(2738)
            + &fmt_trim(cast::<Ext, _>(units_to_buy), b"* ###,")
            + " "
            + &unit_word
            + " of"
            + __content.string(2739)
            + &commod_label
            + " "
            + "("
            + &fmt_trim(cast::<Ext, _>(units_to_buy) * cast::<Ext, _>(unit_size), b"* #############,")
            + " "
            + &unit_name
            + __content.string(2740)
            + &price_str
            + __content.string(2704)
            + &unit_name
            + __content.string(2741)
            + __content.string(2742)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(units_to_buy) * unit_cost, b"* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + "."
            + "\r"
            + "\r";
        if is_crypto(&commod_key) {
            msg_text = remove(msg_text.as_bytes(), __content.bytes(2743));
            msg_text = replace(msg_text.as_bytes(), b"store", b"hold");
        }
        if oil_cap_applied == 0 {
            msg_text = msg_text
                + __content.string(2744)
                + __content.string(2745)
                + __content.string(2746)
                + "\r"
                + "\r";
        }
        if is_crypto(&commod_key) {
            msg_text = msg_text
                + __content.string(2747)
                + &commod_label
                + __content.string(2748)
                + __content.string(2749)
                + &commod_label
                + "."
                + "\r"
                + "\r";
        } else {
            msg_text = msg_text
                + __content.string(2750)
                + __content.string(2751)
                + __content.string(2752)
                + __content.string(2753)
                + "$"
                + &storage_fee
                + " per "
                + &unit_name
                + __content.string(2754)
                + __content.string(2755)
                + "\r"
                + "\r";
        }
        if g.cur_commodity == "WHEAT" || g.cur_commodity == "CORN" {
            msg_text = msg_text
                + __content.string(2756)
                + __content.string(2757)
                + __content.string(2758)
                + "\r"
                + "\r";
        }
        msg_text = msg_text + __content.string(2759);
        let mut title_prefix = s(__content.string(2760));
        if is_crypto(&commod_key) {
            title_prefix = s("Confirm");
        }
        let title = title_prefix + &mc(&g.cur_commodity) + __content.string(2761);
        let answer = ui.msgbox(&msg_text, 0x2023, &title);
        if answer != 6 {
            return;
        }
        total_cost_x = Ext80::from_i64(units_to_buy).fmul(Ext80::from_ext(unit_cost));
        total_cost = Ext::from(total_cost_x.into_ext());
        if g.liquid_funds < total_cost {
            return;
        }
        let mut shortfall = total_cost - (g.cash[buyer_id] + g.t_bills[buyer_id]);
        if Ext::from_f64(0.0) < shortfall {
            let commission = total_cost - (total_cost * Ext::from_f64(100.0)) / Ext::from_f64(101.0);
            if crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, commission, 0) == 7 {
                return;
            }
        }
        g.cash[buyer_id] = g.cash[buyer_id] - total_cost;
    }
    if rec_idx < 1 {
        rec_text = g.cur_commodity.clone()
            + "|"
            + &str_trim(cast::<Ext, _>(buyer_id))
            + "|"
            + &str_trim(cast::<Ext, _>(units_to_buy))
            + "|"
            + &str_trim_ext(total_cost_x);
        add_remove_phys_commod_rec(g, rec_text.as_bytes(), 0);
    } else {
        let new_units = to_long(field_val(&rec_text, 3) + cast::<Ext, _>(units_to_buy));
        let new_cost = val_ext(field(&rec_text, 4).as_bytes()).fadd(total_cost_x);
        rec_text = g.cur_commodity.clone()
            + "|"
            + &str_trim(cast::<Ext, _>(buyer_id))
            + "|"
            + &str_trim(cast::<Ext, _>(new_units))
            + "|"
            + &str_trim_ext(new_cost);
        g.phys_commod_rec[rec_idx] = rec_text.clone();
    }
    if ai_flag == 0 {
        msg_text = s(" (");
        if Ext::from_f64(1.0) < cast::<Ext, _>(units_to_buy) {
            msg_text = s("s (");
        }
        phrase_text = s(__content.string(1506));
        if 10 < buyer_id {
            phrase_text = s("Buyer (") + &g.ent_symbol[buyer_id] + ") has";
        }
        let t = s(__content.string(2762))
            + "\r"
            + "\r"
            + __content.string(2518)
            + &uc(&g.ent_name[buyer_id])
            + "\r"
            + "\r"
            + &phrase_text
            + __content.string(2763)
            + &fmt_trim(cast::<Ext, _>(units_to_buy), b"* ##,")
            + " "
            + &commod_label
            + " unit"
            + &msg_text
            + &fmt_trim(cast::<Ext, _>(unit_size), b"* ##############,")
            + " "
            + &unit_name
            + "s "
            + __content.string(2764)
            + &price_str
            + __content.string(2704)
            + &unit_name
            + ", "
            + __content.string(2765)
            + "\r"
            + "\r"
            + __content.string(2766)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(total_cost, b"* #############,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        let title = s(__content.string(2709)) + &g.cur_commodity + __content.string(2710);
        ui.msgbox(&t, 0x2040, &title);
        g.turn_action_count += 1;
    }
    if !is_crypto(&commod_key) {
        msg_text = PbStr::new();
        if 0 < rec_idx {
            msg_text = s(" more");
        }
        g.news_msg = g.ent_symbol[buyer_id].clone()
            + __content.string(2767)
            + &msg_text
            + __content.string(2768)
            + &commod_label
            + " at $"
            + &price_str
            + " per "
            + &unit_name;
        if 15 < len(g.ent_symbol[buyer_id].as_bytes()) {
            g.news_msg = remove(g.news_msg.as_bytes(), __content.bytes(2769));
        }
        post_news(g, ui, 0, 0);
    }
    let cur = g.cur_commodity.clone();
    apply_futures_trade_impact(g, ui, rng, &cur, 1, units_to_buy);
}

// ---------------------------------------------------------------------------------------------------------
// AiTradePhysCommod
// ---------------------------------------------------------------------------------------------------------

/// Port of AiTradePhysCommod_CheckHolding (GOSUB 0x664bcf).
/// Drops the candidate `commod_name` when selling (`trade_mode` 1) a commodity the trader doesn't hold,
/// or buying (mode 2) one it already holds.
fn check_holding(g: &mut Globals, trader_id: i32, trade_mode: i32, commod_name: &mut PbStr) {
    let holding_value = get_phys_commod_value(g, trader_id, commod_name);
    if trade_mode == 1 && holding_value == Ext::from_f64(0.0) {
        *commod_name = PbStr::new();
    }
    if trade_mode == 2 && Ext::from_f64(0.0) < holding_value {
        *commod_name = PbStr::new();
    }
}

/// Port of AiTradePhysCommod (0x66393c) with its GOSUB AiTradePhysCommod_CheckHolding (0x664bcf) and exit
/// AiTradePhysCommod_Exit (0x664cb2).
/// AI trading [runtime resource wording] . With `entity_id = 0` half the time the owner of a random record
/// (an AI-run company, not twice in a row: `LastCommodSellerEnt`) is chosen, otherwise a random active
/// corporation 131..1580 (5%: fund 1600). Not for human-controlled traders. Sells a holding (20..100%,
/// all below 300 x `ExchangeRate`) when its price is high (OIL > 1.16..1.56, GOLD > 1800..3000 metals
/// base, SILVER, WHEAT, CORN, BITCOIN, ETHEREUM bands) or the record's gain is large; posts the news and
/// returns -1. Otherwise (at most 200 records, cash + T-bills of 300 x `ExchangeRate`) buys a commodity
/// whose price is low (or low and rising for oil/grain, low against the target for metals; gold only for
/// mining companies and fund 1600) via `BuyPhysCommod`, returning 1 if it then holds some. 0 otherwise.
pub fn ai_trade_phys_commod(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO handler: ShowRuntimeError only
    let mut result = 0;
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 501 {
        return result;
    }
    let mut trader_id = entity_id;
    if 10 < trader_id && g.industry_num[trader_id] < 3 {
        return result;
    }
    if trader_id == 1600 && 0 < get_etf_adviser_human_status(g, ui, rng, 1600) {
        return result;
    }
    let rec_count = to_long(val(g.phys_commod_rec[0].as_bytes()));
    let mut held_commod = PbStr::new();
    let mut held_value: Ext = Ext::from_f64(0.0);
    let mut held_cost: Ext = Ext::from_f64(0.0);
    if trader_id == 0 && Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() && 10 < rec_count {
        let rand_rec = rng.rnd_range(1, rec_count);
        let rec_text = g.phys_commod_rec[rand_rec].clone();
        if !rec_text.is_empty() {
            let mut owner_id = to_long(field_val(&rec_text, 2));
            if 0 < owner_id && owner_id <= g.num_players && g.is_computer_player[owner_id] == 0 {
                return result;
            }
            if 0 < owner_id && g.entity_status[owner_id] == 1 && get_managing_player(g, owner_id) == 0 {
                trader_id = owner_id;
            }
            if g.last_commod_seller_ent == trader_id {
                trader_id = 0;
            } else if 0 < trader_id {
                g.last_commod_seller_ent = trader_id;
                held_commod = trim(field(&rec_text, 1).as_bytes());
                held_value = get_phys_commod_value(g, trader_id, &held_commod);
                held_cost = g.ci_phys_commod_cost;
            }
            owner_id = 0;
            let _ = owner_id;
        }
    }
    let mut tries = 0;
    loop {
        tries += 1;
        if 20 < tries {
            return result;
        }
        if trader_id == 0 {
            if Ext::from_f64(20.0) * rng.rnd() <= Ext::from_f64(19.0) {
                trader_id = rng.rnd_range(131, 1580);
            } else {
                trader_id = 1600;
            }
        }
        if g.entity_status[trader_id] != 1 {
            trader_id = 0;
        }
        if 1580 < trader_id && trader_id < 1600 && g.industry_num[trader_id] == 71 {
            trader_id = 0;
        }
        if trader_id != 0 {
            break;
        }
    }
    let controller_type = get_controller_type(g, trader_id);
    if controller_type == 1 {
        return result;
    }
    let silent_mode = if controller_type == 2 { 1 } else { 2 };
    let mut commod_name = PbStr::new();
    let total_value = get_phys_commod_value(g, trader_id, &PbStr::new());
    if !(total_value <= Ext::from_f64(0.0)) {
        let trade_mode = 1;
        if sf(0.4f32) * rng.rnd() + sf(1.16f32) < cast::<Ext, _>(g.oil_price) {
            commod_name = s("OIL");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        let saved = Ext::from_f64(1200.0) * rng.rnd() + Ext::from_f64(1800.0);
        if saved < update_metals_price_base(g, ui) && commod_name.is_empty() {
            commod_name = s("GOLD");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        if cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + Ext::from_f64(21.0) + Ext::from_f64(13.0) * rng.rnd() < cast::<Ext, _>(g.silver_price)
            && commod_name.is_empty()
        {
            commod_name = s("SILVER");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        if Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(8.0) < cast::<Ext, _>(g.wheat_price) && commod_name.is_empty() {
            commod_name = s("WHEAT");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        if sf(3.5f32) * rng.rnd() + sf(7.5f32) < cast::<Ext, _>(g.corn_price) && commod_name.is_empty() {
            commod_name = s("CORN");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        if Ext::from_f64(25000.0) * rng.rnd() + Ext::from_f64(225000.0) < cast::<Ext, _>(g.bitcoin_price) && commod_name.is_empty() {
            commod_name = s("BITCOIN");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        if Ext::from_f64(500.0) * rng.rnd() + Ext::from_f64(6500.0) < cast::<Ext, _>(g.ethereum_price) && commod_name.is_empty() {
            commod_name = s("ETHEREUM");
            check_holding(g, trader_id, trade_mode, &mut commod_name);
        }
        if commod_name.is_empty() && !held_commod.is_empty() && g.last_commod_seller_ent == trader_id {
            let unrealized_gain = held_value - held_cost;
            if g.exchange_rate * Ext::from_f64(100.0) < unrealized_gain {
                let threshold = held_cost * (sf(0.2f32) * rng.rnd()) + held_cost * sf(0.3f32);
                if threshold < unrealized_gain {
                    commod_name = held_commod.clone();
                }
            }
        }
        if !commod_name.is_empty() {
            g.cur_commodity = commod_name.clone();
            let mut price_text = s("$") + &format_commod_price(g, &commod_name);
            if g.currency_code != "USA" {
                price_text = s("US") + &price_text;
            }
            let mut pct_to_sell = cast::<f32, _>(Ext::from_f64(150.0) * rng.rnd() + Ext::from_f64(20.0));
            if Ext::from_f32(100.0f32) < pct_to_sell {
                pct_to_sell = 100.0f32;
            }
            if held_value == Ext::from_f64(0.0) && held_commod.is_empty() && pct_to_sell < Ext::from_f32(100.0f32) {
                held_value = get_phys_commod_value(g, trader_id, &commod_name);
            }
            if held_value < g.exchange_rate * Ext::from_f64(300.0) {
                pct_to_sell = 100.0f32;
            }
            sell_phys_commod(g, ui, rng, trader_id, 0, silent_mode, pct_to_sell, 0);
            result = -1;
            let mut commod_label = lc(&commod_name);
            if is_crypto(&commod_name) {
                commod_label = mc(&commod_name);
            }
            g.news_msg = g.ent_symbol[trader_id].clone()
                + __content.string(2770)
                + &commod_label
                + " @ "
                + &price_text;
            if Ext::from_f64(1.0) < Ext::from_f64(2.0) * rng.rnd() {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(2771), __content.bytes(2772));
            }
            if pct_to_sell < Ext::from_f32(100.0f32) {
                g.news_msg = replace(g.news_msg.as_bytes(), b"entire", b"most of");
            }
            if pct_to_sell < Ext::from_f32(50.0f32) {
                g.news_msg = replace(g.news_msg.as_bytes(), b"most of", b"part of");
            }
            if is_crypto(&commod_name) {
                g.news_msg = remove(g.news_msg.as_bytes(), __content.bytes(2640));
            }
            post_news(g, ui, trader_id, 0);
        }
    }
    // LAB_006645c0: buying
    commod_name = PbStr::new();
    if Ext::from_f64(200.0) < val(g.phys_commod_rec[0].as_bytes()) {
        return result;
    }
    if g.exchange_rate * Ext::from_f64(300.0) > g.cash[trader_id] + g.t_bills[trader_id] {
        return result;
    }
    let trade_mode = 2;
    if sf(0.5f32) * rng.rnd() + sf(0.4f32) > cast::<Ext, _>(g.oil_price) {
        commod_name = s("OIL");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    } else if sf(0.9f32) > cast::<Ext, _>(g.oil_price) && 3 < price_trend_streak(g, 6) {
        commod_name = s("OIL");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    }
    let saved = Ext::from_f64(500.0) * rng.rnd() + Ext::from_f64(600.0);
    if update_metals_price_base(g, ui) < saved {
        commod_name = s("GOLD");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    } else if update_metals_price_base(g, ui) < Ext::from_f64(1250.0)
        && (cast::<Ext, _>(g.gold_price) * sf(1.1f32)) < cast::<Ext, _>(g.gold_target_price)
    {
        commod_name = s("GOLD");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    }
    if cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + Ext::from_f64(9.0) + Ext::from_f64(5.0) * rng.rnd() > cast::<Ext, _>(g.silver_price) {
        commod_name = s("SILVER");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    } else if cast::<Ext, _>(g.metals_price_base) * sf(0.01f32) + Ext::from_f64(16.0) > cast::<Ext, _>(g.silver_price)
        && (cast::<Ext, _>(g.silver_price) * sf(1.1f32)) < cast::<Ext, _>(g.silver_target_price)
    {
        commod_name = s("SILVER");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    }
    if sf(1.75f32) * rng.rnd() + Ext::from_f64(2.0) > cast::<Ext, _>(g.wheat_price) {
        commod_name = s("WHEAT");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    } else if sf(4.5f32) > cast::<Ext, _>(g.wheat_price) && 3 < price_trend_streak(g, 9) {
        commod_name = s("WHEAT");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    }
    if sf(1.6f32) * rng.rnd() + Ext::from_f64(2.0) > cast::<Ext, _>(g.corn_price) {
        commod_name = s("CORN");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    } else if sf(4.5f32) > cast::<Ext, _>(g.corn_price) && 3 < price_trend_streak(g, 10) {
        commod_name = s("CORN");
        check_holding(g, trader_id, trade_mode, &mut commod_name);
    }
    if commod_name.is_empty() {
        return result;
    }
    if commod_name == "GOLD"
        && 10 < trader_id
        && g.industry_num[trader_id] != 50
        && g.industry_num[trader_id] != 71
    {
        return result;
    }
    if trader_id == 1600 && 0 < get_etf_adviser_human_status(g, ui, rng, 1600) {
        return result;
    }
    if 10 < trader_id && g.industry_num[trader_id] == 71 && trader_id != 1600 {
        return result;
    }
    g.cur_commodity = commod_name.clone();
    buy_phys_commod(g, ui, rng, trader_id, silent_mode);
    if Ext::from_f64(0.0) < get_phys_commod_value(g, trader_id, &commod_name) {
        result = 1;
    }
    result
}

// ---------------------------------------------------------------------------------------------------------
// ChargeCommodStorageCosts
// ---------------------------------------------------------------------------------------------------------

/// Locals of ChargeCommodStorageCosts shared with its GOSUBs.
#[derive(Default)]
struct StorageFrame {
    entity_id: i32,
    rec_idx: i32,
    storage_cost: Ext,
    spoiled_entity: i32,
    rec_text: PbStr,
    commod: PbStr,
}

/// Port of ChargeStorage_GrainSpoilage (GOSUB 0x6620f8).
/// Random spoilage of a stored WHEAT/CORN record (at most one per entity per call), more likely for
/// larger stocks in some quarters; rewrites the record with fewer units (same cost).
fn grain_spoilage(g: &mut Globals, rng: &mut Rng, f: &mut StorageFrame) {
    if f.entity_id == f.spoiled_entity {
        return;
    }
    let mut units = to_long(field_val(&f.rec_text, 3));
    let mut spoiled_units = 0;
    let case_key = cast::<Ext, _>(units);
    let q = g.cur_quarter;
    if !(case_key < Ext::from_f64(50.0)) {
        if case_key < Ext::from_f64(100.0) {
            if q == 4 && g.sim_co_count < 600 && Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0) {
                spoiled_units = to_long(Ext::from_f64(3.0) * rng.rnd() + Ext::from_f64(1.0));
            }
        } else if case_key < Ext::from_f64(300.0) {
            if q == 3 && g.sim_co_count > 1500 && Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) {
                spoiled_units = to_long(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(2.0));
            }
        } else if case_key < Ext::from_f64(500.0) {
            if q == 2 && Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0) {
                spoiled_units = to_long(Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(3.0));
            }
        } else if case_key < Ext::from_f64(1000.0) {
            if q == 1 && Ext::from_f64(15.0) * rng.rnd() < Ext::from_f64(1.0) {
                spoiled_units = to_long(Ext::from_f64(15.0) * rng.rnd() + Ext::from_f64(10.0));
            }
        } else if case_key < Ext::from_f64(3000.0) {
            if q < 2 && Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0) {
                spoiled_units = to_long(Ext::from_f64(30.0) * rng.rnd() + Ext::from_f64(14.0));
            }
        } else if case_key < Ext::from_f64(4000.0) {
            if q > 2 && Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0) {
                spoiled_units = to_long(Ext::from_f64(40.0) * rng.rnd() + Ext::from_f64(20.0));
            }
        } else if q < 4 && Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) {
            let r = rng.rnd();
            spoiled_units = to_long(cast::<Ext, _>(units) * (sf(0.01f32) * r) + cast::<Ext, _>(units) * sf(0.005f32));
        }
    }
    if 0 < spoiled_units {
        f.spoiled_entity = f.entity_id;
        units = units - spoiled_units;
        let cost_basis: Ext = crate::pb::numeric::double(val(field(&f.rec_text, 4).as_bytes())); // DOUBLE
        f.rec_text = f.commod.clone()
            + "|"
            + &str_trim(cast::<Ext, _>(f.entity_id))
            + "|"
            + &str_trim(cast::<Ext, _>(units))
            + "|"
            + &str_trim(cost_basis);
        g.phys_commod_rec[f.rec_idx] = f.rec_text.clone();
    }
}

/// Port of ChargeStorage_ApplyEntity (GOSUB 0x661d3f).
/// Deducts the monthly storage cost from cash and accrued P&L, runs the grain spoilage over the entity's
/// WHEAT/CORN records and sets the " [runtime resource wording] …" news for the current player (above
/// 5) or the active and selected entity (above 10, if no news is pending).
fn apply_entity(g: &mut Globals, rng: &mut Rng, f: &mut StorageFrame) {
    let __content = crate::content::current();
    let e = f.entity_id;
    g.cash[e] = g.cash[e] - f.storage_cost;
    g.accrued_pl[e] = g.accrued_pl[e] - f.storage_cost;
    f.spoiled_entity = 0;
    let rec_count = to_long(val(g.phys_commod_rec[0].as_bytes()));
    f.rec_idx = 1;
    while f.rec_idx <= rec_count {
        if !g.phys_commod_rec[f.rec_idx].is_empty() {
            f.rec_text = g.phys_commod_rec[f.rec_idx].clone();
            if field_val(&f.rec_text, 2) == cast::<Ext, _>(e) {
                f.commod = trim(field(&f.rec_text, 1).as_bytes());
                if f.commod == "WHEAT" || f.commod == "CORN" {
                    grain_spoilage(g, rng, f);
                }
            }
        }
        f.rec_idx += 1;
    }
    let mut show_news = 0;
    if g.cur_player == e {
        if Ext::from_f64(5.0) < f.storage_cost {
            show_news = 1;
        }
    } else if g.active_entity == e && g.sel_entity == e && g.news_msg.is_empty() && Ext::from_f64(10.0) < f.storage_cost {
        show_news = 1;
    }
    if show_news == 1 {
        g.news_msg = s(__content.string(2773))
            + &g.ent_symbol[e]
            + " are "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(f.storage_cost, b"* ########")
            + " "
            + &g.million_prefix_lc
            + "il.";
        if 70 < len(g.news_msg.as_bytes()) {
            g.news_msg = PbStr::new();
        }
    }
}

/// Port of ChargeCommodStorageCosts (0x661bd7) with its GOSUBs ChargeStorage_ApplyEntity (0x661d3f),
/// ChargeStorage_GrainSpoilage (0x6620f8) and exit stub ChargeCommodStorageCosts_Exit (0x6625aa).
/// For every active player and entity 131..1600 with physical commodities: charges a twelfth of the
/// storage cost (`CalcPhysCommodStorageCost`, if at least 1) to cash and P&L and lets stored grain
/// spoil; posts the pending news at the end.
pub fn charge_commod_storage_costs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // ON ERROR GOTO handler: ShowRuntimeError only
    if !(Ext::from_f64(1.0) <= val(g.phys_commod_rec[0].as_bytes())) {
        return;
    }
    let mut f = StorageFrame::default();
    f.entity_id = 1;
    while f.entity_id <= 1600 {
        let e = f.entity_id;
        if (e <= g.num_players || 130 < e) && g.entity_status[e] == 1 {
            f.storage_cost = Ext::from_f64(0.0);
            let cost = calc_phys_commod_storage_cost(g, e);
            if Ext::from_f64(1.0) <= cost {
                f.storage_cost = cost / Ext::from_f64(12.0);
                apply_entity(g, rng, &mut f);
            }
        }
        f.entity_id += 1;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
}

/// Port of DeliverPhysCommodForShortFutures (0x664ce0).
/// Sells available physical units as delivery against an expiring short futures contract. Returns 1
/// when any units were delivered; a shortfall is settled in cash by `SettleFuturesContract`.
pub fn deliver_phys_commod_for_short_futures(
    g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, futures_rec: &PbStr,
) -> i32 {
    deliver_phys_commod_for_short_futures_skipping(g, ui, rng, futures_rec, |_| false)
}

pub(crate) fn deliver_phys_commod_for_short_futures_skipping(
    g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, futures_rec: &PbStr,
    mut skips: impl FnMut(u32) -> bool,
) -> i32 {
    use crate::pb::ext80::Ext80;
    let mut result = 0;
    if val(g.phys_commod_rec[0].as_bytes()) == Ext::from_f64(0.0) {
        return result;
    }
    let fut_rec = futures_rec.clone();
    let commod_name = parse_any(fut_rec.as_bytes(), b"|", 1);
    let holder_id = to_long(val(parse_any(fut_rec.as_bytes(), b"|", 2).as_bytes()));
    if !(0 < holder_id && g.entity_status[holder_id] == 1) {
        return result;
    }
    let contracts = to_long(val(parse_any(fut_rec.as_bytes(), b"|", 3).as_bytes()).abs());
    if contracts == 0 {
        return result;
    }
    let holding_value = if skips(0x65b912) { Ext::from_f64(0.0) } else { get_phys_commod_value(g, holder_id, &commod_name) };
    if !(sf(0.5f32) <= holding_value) {
        return result;
    }
    let rec_idx = if skips(0x65be8a) { 0 } else { find_phys_commod_rec(g, holder_id, &commod_name) };
    let mut rec_text = PbStr::new();
    if 0 < rec_idx {
        rec_text = g.phys_commod_rec[rec_idx].clone();
    }
    if 0 < rec_idx {
        let units_owned = to_long(val(parse_any(rec_text.as_bytes(), b"|", 3).as_bytes()));
        if 0 < units_owned {
            let mut pct_to_sell: f32 = cast::<f32, _>(Ext::from_f32(100.0f32));
            if contracts < units_owned {
                // fild 100; fild units; fidivr contracts; fmulp → 100 * (contracts / units) in 80 bits
                let q = Ext80::from_i64(cast::<i64, _>(contracts)).fdiv(Ext80::from_i64(cast::<i64, _>(units_owned)));
                pct_to_sell = Ext80::from_i64(100).fmul(q).to_f32();
            }
            g.cur_commodity = commod_name.clone();
            let mut delivery_mode = 1;
            if units_owned < contracts {
                delivery_mode = -1;
            }
            if !skips(0x65db62) {
                sell_phys_commod(g, ui, rng, holder_id, rec_idx, 2, pct_to_sell, delivery_mode);
            }
            result = 1;
        }
    }
    result
}
