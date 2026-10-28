//! Price alerts: `CheckPriceAlerts` (0x6830fc) with its GOSUB `CheckPriceAlerts_Trigger` (0x6838bb),
//! `ExpirePriceAlerts` (0x684123), and the streaming-quote refresh `UpdateStreamingQuotes` (0x5dc70b) the
//! trigger runs for stock alerts.
//!
//! `PriceAlerts(1..100)` records are `"entity|player|UP/DN|target|quarter|year|tick"` (element 0 = count):
//! entity 11..1600 = a stock (target = market cap), 3002..3013 = a commodity, crypto, the stock index, an
//! interest rate or GDP growth (target = the price / rate).
use crate::pb::real::{Ext, cast};


use super::{fmt_trim, s};
use crate::logic::holdings::pack_holding_arrays_with_news as pack_holding_arrays;
use crate::logic::report::show_quick_message;
use crate::logic::valuation::{calc_stock_price, update_main_commodity_display};
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{lcase, ltrim, mcase, parse_any, rtrim, CaseMode};
use crate::pb::numeric::to_long;
use crate::state::Globals;
use crate::ui::Ui;

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `PriceAlerts(slot)`; slots outside `0 TO 100` read as "" (error 9 without an ON ERROR handler: the
/// shared dummy element).
fn alert(g: &Globals, slot: i32) -> PbStr {
    if (0..=100).contains(&slot) { g.price_alerts[slot].clone() } else { PbStr::new() }
}

fn clear_alert(g: &mut Globals, slot: i32) {
    if (0..=100).contains(&slot) {
        g.price_alerts[slot] = PbStr::new();
    }
}

/// Locals of CheckPriceAlerts the GOSUB `CheckPriceAlerts_Trigger` works on.
#[derive(Default)]
struct Frame {
    slot: i32,
    entity_id: i32,
    remaining: i32,
    dir_code: PbStr,
    target_value: Ext,
    cur_value: Ext,
    item_name: PbStr,
    unit_name: PbStr,
    value_text: PbStr,
}

/// Port of CheckPriceAlerts (0x6830fc).
/// Checks the current player's price alerts (all, or only those on `filter_entity` if it is 11..1600):
/// an alert whose stock market cap / commodity price / rate has reached (`UP`) or fallen to (`DN`) its
/// target pops up a message ([`Ui::msgbox`], after refreshing the streaming quotes around the stock) and
/// is removed. Alerts on entities < 11 or on stocks no longer active are removed silently. If anything was
/// removed and `filter_entity = 0`, the lists are packed (`PackHoldingArrays`).
pub fn check_price_alerts(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, filter_entity: i32) {
    let filter_id = filter_entity;
    let alert_count = to_long(val(alert(g, 0).as_bytes()));
    if alert_count == 0 {
        return;
    }
    let mut f = Frame { remaining: alert_count, ..Frame::default() };
    f.slot = 1;
    while f.slot <= alert_count {
        let alert_rec = alert(g, f.slot);
        if !alert_rec.is_empty() {
            check_one(g, ui, rng, &mut f, &alert_rec, filter_id);
        }
        f.slot = f.slot + 1;
    }
    if alert_count != f.remaining && filter_id == 0 {
        pack_holding_arrays(g, ui);
    }
}

fn check_one(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame, alert_rec: &PbStr, filter_id: i32) {
    let __content = crate::content::current();
    f.entity_id = to_long(val(field(alert_rec, 1).as_bytes()));
    let e = f.entity_id;
    if !(e < 11 || 1600 < e || g.entity_status[e] == 1) {
        clear_alert(g, f.slot);
        f.remaining = f.remaining - 1;
        return;
    }
    if !(filter_id < 11 || 1600 < filter_id || filter_id == e) {
        return;
    }
    if e < 11 {
        clear_alert(g, f.slot);
        f.remaining = f.remaining - 1;
        return;
    }
    let owner_id = to_long(val(field(alert_rec, 2).as_bytes()));
    if g.cur_player != owner_id {
        return;
    }
    f.dir_code = rtrim(ltrim(field(alert_rec, 3).as_bytes()).as_bytes());
    f.target_value = val(field(alert_rec, 4).as_bytes());
    if 10 < e && e < 1601 {
        let cap = g.market_cap[e];
        if f.dir_code == "UP" {
            if cap >= f.target_value {
                trigger(g, ui, rng, f);
            }
        } else if f.dir_code == "DN" && !(cap > f.target_value) {
            trigger(g, ui, rng, f);
        }
        return;
    }
    if !(3001 < e && e < 3014) {
        return;
    }
    // SELECT CASE itemCode (an EXT copy of entityId)
    match e {
        3002 => {
            f.item_name = s("OIL");
            f.cur_value = cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0);
            f.unit_name = s("barrel");
        }
        3003 => {
            f.item_name = s("GOLD");
            f.cur_value = cast::<Ext, _>(g.gold_price);
            f.unit_name = s("ounce");
        }
        3004 => {
            f.item_name = s("SILVER");
            f.cur_value = cast::<Ext, _>(g.silver_price);
            f.unit_name = s("ounce");
        }
        3005 => {
            f.item_name = s("WHEAT");
            f.cur_value = cast::<Ext, _>(g.wheat_price);
            f.unit_name = s("bushel");
        }
        3006 => {
            f.item_name = s("CORN");
            f.cur_value = cast::<Ext, _>(g.corn_price);
            f.unit_name = s("bushel");
        }
        3007 => {
            f.item_name = s("BITCOIN");
            f.cur_value = cast::<Ext, _>(g.bitcoin_price);
            f.unit_name = s(__content.string(8504));
        }
        3008 => {
            f.item_name = s("ETHEREUM");
            f.cur_value = cast::<Ext, _>(g.ethereum_price);
            f.unit_name = s(__content.string(8504));
        }
        3009 => {
            f.item_name = s(__content.string(2499));
            f.cur_value = g.stock_index;
            f.unit_name = s("unit");
        }
        3010 => {
            f.item_name = s(__content.string(6152));
            f.cur_value = cast::<Ext, _>(g.prime_rate);
        }
        3011 => {
            f.item_name = s(__content.string(8505));
            f.cur_value = cast::<Ext, _>(g.long_bond_yield);
        }
        3012 => {
            f.item_name = s(__content.string(8506));
            f.cur_value = cast::<Ext, _>(g.short_bond_yield);
        }
        3013 => {
            f.item_name = s(__content.string(8507));
            let gdp_tenths = to_long(Ext::from_f64(10.0) * cast::<Ext, _>(g.gdp_growth));
            f.cur_value = cast::<Ext, _>(0.1f32) * cast::<Ext, _>(gdp_tenths); // fld dword 0.1 / fimul
            // (UI) CONTROL SET TEXT 153, RIGHT$(FORMAT$(curValue, "* #########.0"), 9)
        }
        _ => {}
    }
    if f.dir_code == "UP" {
        if f.cur_value >= f.target_value {
            trigger(g, ui, rng, f);
        }
    } else if f.dir_code == "DN" && !(f.cur_value > f.target_value) {
        trigger(g, ui, rng, f);
    }
}

/// Port of CheckPriceAlerts_Trigger (0x6838bb), GOSUB of CheckPriceAlerts: shows the alert message for
/// the alert in `f.slot` and removes it.
fn trigger(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.entity_id;
    let header = s(__content.string(8508)) + &g.cur_player_name + "\r" + "\r";
    let is_stock = 10 < e && e < 1601;
    if is_stock && Ext::from_f64(0.0) < g.shares_outstanding[e] {
        f.target_value = f.target_value / g.shares_outstanding[e];
        f.cur_value = g.market_cap[e] / g.shares_outstanding[e];
        f.value_text = fmt_trim(f.cur_value, "* ###,.00");
        // (UI) IF SelEntity = entityId THEN CONTROL SET TEXT 176, valueText
    } else if is_stock {
        clear_alert(g, f.slot);
        return;
    } else if 3001 < e && e < 3014 {
        update_main_commodity_display(g);
        let mut verb_phrase = s(__content.string(8509));
        if f.dir_code == "DN" {
            verb_phrase = s(__content.string(8510));
        }
        let mut category = s("COMMODITY");
        if 3006 < e {
            category = s("CRYPTOCURRENCY");
        }
        if e == 3009 {
            category = s("INDEX");
        }
        f.value_text = fmt_trim(f.cur_value, "* ###,.00");
        let msg_text;
        if e < 3010 {
            msg_text = header.clone()
                + &category
                + ":  "
                + &f.item_name
                + "\r"
                + "\r"
                + __content.string(8511)
                + &lcase(category.as_bytes(), CaseMode::Ansi)
                + __content.string(8512)
                + &verb_phrase
                + __content.string(8513)
                + __content.string(8514)
                + &fmt_trim(f.target_value, "* ###,.00")
                + __content.string(8515)
                + __content.string(8516)
                + &f.value_text
                + " per "
                + &f.unit_name
                + ".";
        } else {
            category = s(__content.string(8517));
            if e == 3013 {
                category = s(__content.string(8518));
            }
            msg_text = header.clone()
                + &category
                + ":  "
                + &f.item_name
                + "\r"
                + "\r"
                + __content.string(8519)
                + &lcase(category.as_bytes(), CaseMode::Ansi)
                + __content.string(8512)
                + &verb_phrase
                + __content.string(8520)
                + __content.string(8521)
                + &fmt_trim(f.target_value, "* ###,.00")
                + __content.string(8522)
                + __content.string(8523)
                + &f.value_text
                + "%.";
        }
        let mut msg_title = mcase(category.as_bytes(), CaseMode::Ansi) + __content.string(8524) + &f.item_name;
        if 3009 < e {
            msg_title = s(__content.string(8525)) + &mcase(f.item_name.as_bytes(), CaseMode::Ansi);
        }
        if e == 3013 {
            msg_title = s(__content.string(8526)) + &f.item_name;
        }
        ui.msgbox(&msg_text, 8256, &msg_title);
    }
    // LAB_00683eac
    if is_stock {
        update_streaming_quotes(g, ui, rng, e - 1, e + 1);
        let mut verb_phrase = s(__content.string(8509));
        if f.dir_code == "DN" {
            verb_phrase = s(__content.string(8510));
        }
        let msg_text = header
            + __content.string(8527)
            + &g.ent_name[e]
            + " ("
            + &g.ent_symbol[e]
            + ")"
            + "\r"
            + "\r"
            + __content.string(8528)
            + &verb_phrase
            + __content.string(8513)
            + __content.string(8521)
            + &fmt_trim(f.target_value, "* ###,.00")
            + &g.currency_suffix
            + __content.string(8515)
            + __content.string(8529)
            + &f.value_text
            + __content.string(1071);
        let title = s(__content.string(8530)) + &g.ent_name[e];
        ui.msgbox(&msg_text, 8256, &title);
    }
    f.remaining = f.remaining - 1;
    clear_alert(g, f.slot);
}

/// Port of ExpirePriceAlerts (0x684123).
/// Removes every alert whose quarter, year and tick fields are all reached (`VAL(year) <= CurYear`,
/// `VAL(quarter) <= CurQuarter`, `tick <= SimCoCount`). For the current player's alerts on active stocks
/// it first runs `CheckPriceAlerts(corp)` if the target is reached (then no expiry message), else pops up
/// " [runtime resource wording] … [runtime resource wording] ". Packs the lists
/// (`PackHoldingArrays`) if anything expired.
pub fn expire_price_alerts(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut changed = 0;
    let mut slot = 1;
    loop {
        let mut alert_rec = alert(g, slot);
        'body: {
            if alert_rec.is_empty() {
                break 'body;
            }
            if !(val(field(&alert_rec, 6).as_bytes()) <= cast::<Ext, _>(g.cur_year)) {
                break 'body;
            }
            if !(val(field(&alert_rec, 5).as_bytes()) <= cast::<Ext, _>(g.cur_quarter)) {
                break 'body;
            }
            let expire_tick = to_long(val(field(&alert_rec, 7).as_bytes()));
            if expire_tick > g.sim_co_count {
                break 'body;
            }
            clear_alert(g, slot);
            changed = 1;
            let owner_id = to_long(val(field(&alert_rec, 2).as_bytes()));
            if g.cur_player != owner_id {
                break 'body;
            }
            let corp_id = to_long(val(field(&alert_rec, 1).as_bytes()));
            if !(10 < corp_id && corp_id < 1601 && (corp_id < 11 || g.entity_status[corp_id] == 1)) {
                break 'body;
            }
            let dir_code = rtrim(ltrim(field(&alert_rec, 3).as_bytes()).as_bytes());
            let mut target_value = val(field(&alert_rec, 4).as_bytes());
            if dir_code == "UP" {
                if g.market_cap[corp_id] >= target_value {
                    check_price_alerts(g, ui, rng, corp_id);
                    slot = slot - 1;
                    alert_rec = PbStr::new();
                }
            } else if dir_code == "DN" && !(g.market_cap[corp_id] > target_value) {
                check_price_alerts(g, ui, rng, corp_id);
                slot = slot - 1;
                alert_rec = PbStr::new();
            }
            if g.shares_outstanding[corp_id] <= Ext::from_f64(0.0) {
                alert_rec = PbStr::new();
            } else {
                target_value = target_value / g.shares_outstanding[corp_id];
            }
            if !alert_rec.is_empty() {
                let mut msg = s(__content.string(8531))
                    + &g.ent_name[corp_id]
                    + __content.string(8532)
                    + "of $"
                    + &fmt_trim(target_value, "* ###.00")
                    + __content.string(8533);
                show_quick_message(g, ui, &mut msg);
            }
        }
        slot = slot + 1;
        if slot >= 101 {
            break;
        }
    }
    if changed == 1 {
        pack_holding_arrays(g, ui);
    }
}

/// Port of UpdateStreamingQuotes (0x5dc70b), state effects only.
/// Refreshes the current (human, not "...OUT...") player's streaming quote list `WatchList(1..15)`:
/// slots without an active corp are cleared; corps in `from_id..to_id` (clamped to 11..1600) are re-priced
/// (`ValuationMode = 0`; an unchanged price is nudged by RND*3% - 1.5% with probability 3/5 unless the
/// game has ended) and their `StreamPrevPriceStr` / `StreamCurPrice` / `StreamPriceDir` updated. Finally
/// the selected corp is re-priced. The quote labels (CONTROL ADD/KILL) are UI.
pub fn update_streaming_quotes(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, from_id: i32, to_id: i32) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if 0 < g.cur_player {
        if g.cur_player > 5 {
            return; // IsComputerPlayer(CurPlayer): error 9 -> handler
        }
        if g.is_computer_player[g.cur_player] == 1 {
            return;
        }
    }
    if g.cur_player < 1 || 5 < g.cur_player {
        return; // PlayerName(CurPlayer): error 9 -> handler
    }
    if g.player_name[g.cur_player] == "...OUT..." {
        return;
    }
    let mut min_id = from_id;
    if min_id < 11 {
        min_id = 11;
    }
    let mut max_id = to_id;
    if 1600 < max_id {
        max_id = 1600;
    }
    let player_id = g.cur_player;
    if player_id < 1 || g.num_players < player_id {
        return;
    }
    // (the IsComputerPlayer / "...OUT..." tests are repeated)
    let mut price_dir = 0;
    for slot in 1..=15 {
        let corp_id = g.watch_list[(slot, player_id)];
        if 1600 < corp_id {
            return; // EntityStatus(corpId): error 9 -> handler
        }
        if corp_id < 11 || g.entity_status[corp_id] != 1 {
            // GOSUB UpdateStreamingQuotes_KillSlotCtrls: (UI) CONTROL KILL slot + 800, slot + 900
            // GOSUB UpdateStreamingQuotes_ClearSlot (0x5dcd6c)
            g.watch_list[(slot, player_id)] = 0;
            g.stream_prev_price_str[(slot, player_id)] = PbStr::new();
            g.stream_cur_price[(slot, player_id)] = PbStr::new();
            g.stream_price_dir[(slot, player_id)] = 0;
            continue;
        }
        if !(min_id <= corp_id && corp_id <= max_id) {
            continue;
        }
        g.stream_prev_price_str[(slot, player_id)] = g.stream_cur_price[(slot, player_id)].clone();
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, corp_id, 0); // mode 0 does not use TIME$
        if g.stock_price_str == g.stream_prev_price_str[(slot, player_id)] && !(g.game_end_flag == "Y") {
            let r = rng.rnd();
            if Ext::from_f64(5.0) * r > Ext::from_f64(2.0) {
                let r = rng.rnd();
                let factor = cast::<Ext, _>(0.03f32) * r + cast::<Ext, _>(0.985f32);
                g.market_cap[corp_id] = factor * g.market_cap[corp_id];
                g.valuation_mode = 0;
                calc_stock_price(g, ui, rng, corp_id, 0);
            }
        }
        g.stream_cur_price[(slot, player_id)] = g.stock_price_str.clone();
        let cur = val(g.stream_cur_price[(slot, player_id)].as_bytes());
        let prev = val(g.stream_prev_price_str[(slot, player_id)].as_bytes());
        if cur == prev {
            let r = rng.rnd();
            if Ext::from_f64(25.0) * r > Ext::from_f64(24.0) {
                price_dir = 0;
            }
        }
        if cur > prev {
            price_dir = 1;
        }
        if prev > cur {
            price_dir = -1;
        }
        if g.stream_price_dir[(slot, player_id)] == price_dir && cur == prev {
            continue;
        }
        g.stream_price_dir[(slot, player_id)] = price_dir;
        // (UI) IF CurPlayer = playerId: re-create the quote label slot + 900
    }
    if 10 < g.sel_entity {
        g.valuation_mode = 0;
        let sel = g.sel_entity;
        calc_stock_price(g, ui, rng, sel, 0);
        // (UI) CONTROL SET TEXT 176, StockPriceStr
    }
}
