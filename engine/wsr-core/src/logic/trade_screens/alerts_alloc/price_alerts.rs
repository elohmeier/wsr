//! Setting price alerts: `SetPriceAlert` (0x6828ca), the alert-type dialog `DlgPriceAlertType`
//! (0x68498c) with its dialog procedure and control callbacks, and `SetCommodityOrRateAlert` (0x68600e).
//!
//! `PriceAlerts(1..100)` records are `"entity|player|UP/DN|target|quarter|year|tick"` (element 0 = count),
//! see [`crate::logic::reports_lists::check_price_alerts`]. Alert types: 3001 = the stock of the active
//! company (stored with the corp id and the target market cap), 3002..3013 = [runtime resource wording]
//! corn, bitcoin, ethereum, the stock index, the prime / long bond / short bond rate and GDP growth
//! (stored with the type and the target price or rate).
use crate::pb::real::{Ext, cast};


use super::s;
use crate::logic::newgame::LocalTime;
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::{check_price_alerts, expire_price_alerts};
use crate::logic::restructure::dlg_util::{
    adjust_dialog_frame_size, get_notify_hwnd_from, get_tooltip_ctrl_id, is_tab_sel_change, is_tooltip_need_text,
    set_tooltip_text, Nmhdr,
};
use crate::logic::trading::select::select_acting_entity;
use crate::logic::valuation::calc_stock_price;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::{format, format_ext};
use crate::pb::numeric::{str_num, str_num_ext, val, val_ext};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{ltrim, remove_any, replace, right, rtrim, ucase, CaseMode};
use crate::pb::numeric::to_long;
use crate::state::Globals;
use crate::ui::Ui;

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &[u8]) -> PbStr {
    rtrim(ltrim(x).as_bytes())
}

/// `RTRIM$(LTRIM$(STR$(x)))` of a LONG (15 digits).
fn str_l(x: i32) -> PbStr {
    trim(str_num(cast::<Ext, _>(x), 15).as_bytes())
}

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))` of an EXT.
fn fmt_trim_ext(x: Ext80, mask: &str) -> PbStr {
    trim(&format_ext(x, mask.as_bytes()))
}

/// Title of the alert-type dialog.
#[allow(non_snake_case)]
pub fn PRICE_ALERT_DLG_TITLE(__content: &crate::content::ContentPack) -> &str { __content.string(11281) }

/// Control ids of the alert-type option buttons (alert type `3001 + index`).
pub const PRICE_ALERT_OPTION_IDS: [i32; 13] = [1203, 1206, 1209, 1212, 1215, 1218, 1221, 1224, 1227, 1230, 1233, 1236, 1239];
/// Control id of the APPLY button of the alert-type dialog (CANCEL is 1).
pub const PRICE_ALERT_APPLY_ID: i32 = 1242;

/// The texts of the alert-type dialog's 13 option buttons (alert type `3001 + index`); the stock option
/// names the active company (`SelCorpName`, else `EntName(LastSelEntity)`, else "Active Entity").
pub fn price_alert_type_options(g: &Globals) -> Vec<PbStr> {
    let __content = crate::content::current();
    let entity_label = if g.sel_entity < 11 {
        if 10 < g.last_sel_entity { g.ent_name[g.last_sel_entity].clone() } else { s(__content.string(1604)) }
    } else {
        g.sel_corp_name.clone()
    };
    let mut v = vec![s(__content.string(11282)) + &entity_label];
    for t in [
        __content.string(11283),
        __content.string(11284),
        __content.string(11285),
        __content.string(11286),
        __content.string(11287),
        __content.string(11288),
        __content.string(11289),
        __content.string(11290),
        __content.string(11291),
        __content.string(11292),
        __content.string(11293),
        __content.string(11294),
    ] {
        v.push(s(t));
    }
    v
}

/// Port of SetPriceAlert (0x6828ca).
/// The "set an alert" menu: expires old alerts (`ExpirePriceAlerts`), then repeatedly asks for the alert
/// type ([`dlg_price_alert_type`]) until cancelled. Each round runs `CheckPriceAlerts(0)`, finds a free
/// slot (none → " [runtime resource wording] " and return), and for a stock alert asks for a target
/// price of the active company (`SelEntity`, else `LastSelEntity`, else the symbol dialog
/// `SelectActingEntity(3)`): it must differ from the current price (`StockPriceStr` after
/// `CalcStockPrice` with `ValuationMode = 0`); the alert stores the target market cap (price × shares)
/// and expires after a year. Other types go to [`set_commodity_or_rate_alert`]. An empty / non-positive
/// entry ends the menu.
///
/// Bug (BUGS.md #384): the free slot is only searched, never reset, so from the second round on a full
/// list reuses the slot found before (overwriting the alert just set, and the count still grows).
pub fn set_price_alert(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    let __content = crate::content::current();
    expire_price_alerts(g, ui, rng);
    let mut free_slot = 0;
    let mut corp_id;
    let mut dir_code = PbStr::new();
    let mut dir_word = PbStr::new();
    loop {
        dlg_price_alert_type(g, ui, 0);
        g.alert_dlg_open = 0;
        let mut alert_type = to_long(g.tmp_ext1);
        g.tmp_ext1 = Ext::from_f64(0.0);
        if alert_type == 0 {
            return;
        }
        check_price_alerts(g, ui, rng, 0);
        if let Some(slot) = (1..=100).find(|&k| g.price_alerts[k].is_empty()) {
            free_slot = slot;
        }
        if free_slot == 0 {
            let msg_text = s(__content.string(11295))
                + "\r"
                + "\r"
                + __content.string(11296)
                + __content.string(11297);
            ui.msgbox(&msg_text, 8208, &s(__content.string(11298)));
            return;
        }
        if alert_type < 11 || 3013 < alert_type || (1600 < alert_type && alert_type < 3001) {
            return;
        }
        if 3002 <= alert_type {
            set_commodity_or_rate_alert(g, ui, &mut alert_type);
            continue;
        }
        if g.sel_entity < 11 {
            if g.last_sel_entity < 11 {
                let mut m = s(__content.string(11299))
                    + __content.string(11300);
                show_quick_message(g, ui, &mut m);
                select_acting_entity(g, ui, rng, 3, now);
                if g.sel_entity < 11 || 1600 < g.sel_entity {
                    return;
                }
                corp_id = g.sel_entity;
            } else {
                corp_id = g.last_sel_entity;
            }
        } else {
            corp_id = g.sel_entity;
        }
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, corp_id, now.time_secs());
        let msg_text = s(__content.string(11301))
            + &g.ent_name[corp_id]
            + __content.string(11302)
            + __content.string(11303)
            + &g.stock_price_str
            + __content.string(1071);
        let title = s(__content.string(11304)) + &g.ent_symbol[corp_id];
        let (target_value, price_text) = loop {
            let mut valid_input = 1;
            let input_text = ui.inputbox(&msg_text, &title, &PbStr::new());
            let target = val_ext(remove_any(input_text.as_bytes(), b",").as_bytes());
            if target.cmp_val(Ext80::from_ext(Ext::from_f64(0.0))).is_le() {
                return;
            }
            let price_text = fmt_trim_ext(target, "* ###.00");
            let cur = val(g.stock_price_str.as_bytes());
            let p = val(price_text.as_bytes());
            if cur < p {
                dir_code = s("UP");
                dir_word = s(" above");
            }
            if p < cur {
                dir_code = s("DN");
                dir_word = s(" below");
            }
            if p == cur {
                valid_input = 0;
            }
            if valid_input == 0 {
                let mut m = s(__content.string(11305));
                show_quick_message(g, ui, &mut m);
            }
            if valid_input == 1 {
                break (target, price_text);
            }
        };
        let msg_text = s(__content.string(11306))
            + &g.ent_name[corp_id]
            + ":"
            + "\r"
            + "\r"
            + __content.string(11307)
            + &g.ent_name[corp_id]
            + " ("
            + &g.ent_symbol[corp_id]
            + ") "
            + "goes"
            + &dir_word
            + " "
            + &price_text
            + &g.currency_suffix
            + __content.string(1071)
            + "\r"
            + "\r"
            + __content.string(11308)
            + __content.string(11309);
        ui.msgbox(&msg_text, 8256, &(s(__content.string(11310)) + &g.ent_name[corp_id]));
        let target_value = target_value * Ext80::from_ext(g.shares_outstanding[corp_id]);
        g.price_alerts[free_slot] = alert_record(g, corp_id, &dir_code, target_value);
        let alert_count = to_long(val(g.price_alerts[0].as_bytes()) + Ext::from_f64(1.0));
        g.price_alerts[0] = str_l(alert_count);
    }
}

/// `entity|CurPlayer|dir|STR$(target)|CurQuarter|CurYear + 1|SimCoCount`.
fn alert_record(g: &Globals, entity: i32, dir_code: &PbStr, target_value: Ext80) -> PbStr {
    str_l(entity)
        + "|"
        + &str_l(g.cur_player)
        + "|"
        + dir_code
        + "|"
        + &trim(str_num_ext(target_value, 15).as_bytes())
        + "|"
        + &str_l(g.cur_quarter)
        + "|"
        + &str_l(1 + g.cur_year)
        + "|"
        + &str_l(g.sim_co_count)
}

/// Port of DlgPriceAlertType (0x68498c).
/// The modal " [runtime resource wording] " dialog: 13 option buttons for the alert type
/// ([`price_alert_type_options`]), APPLY and CANCEL, shown through [`Ui::select`] (a choice = its option
/// button, then APPLY; `None` = CANCEL). The chosen type comes back in `TmpExt1` (3001..3013, 0 =
/// cancelled); `AlertDlgOpen` is 1 while it is open. The dialog handle `hPriceAlertDlg` is not modelled.
pub fn dlg_price_alert_type(g: &mut Globals, ui: &mut dyn Ui, _h_parent: i32) {
    let __content = crate::content::current();
    adjust_dialog_frame_size(g.h_price_alert_dlg, 0x80ca_0880, 0);
    let options = price_alert_type_options(g);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.alert_dlg_open = 1;
    let prompt = [s(__content.string(11311))];
    match ui.select(&s(PRICE_ALERT_DLG_TITLE(&__content)), &prompt, &options, None, &s("Apply")) {
        Some(k) => {
            dlg_price_alert_opt(g, 3001 + cast::<i32, _>(k));
            dlg_price_alert_ok(g, ui);
        }
        None => dlg_price_alert_cancel(g),
    }
    g.alert_dlg_open = 0;
}

/// Port of the option-button callbacks DlgPriceAlert_OptStock (0x685afd), DlgPriceAlert_OptOil
/// (0x685b49), DlgPriceAlert_OptGold (0x685b95), DlgPriceAlert_OptSilver (0x685be1),
/// DlgPriceAlert_OptWheat (0x685c2d), DlgPriceAlert_OptCorn (0x685c79), DlgPriceAlert_OptBitcoin
/// (0x685cc5), DlgPriceAlert_OptEthereum (0x685d11), DlgPriceAlert_OptStockIndex (0x685d5d),
/// DlgPriceAlert_OptPrimeRate (0x685da9), DlgPriceAlert_OptLongBond (0x685df5),
/// DlgPriceAlert_OptShortBond (0x685e41) and DlgPriceAlert_OptGdpGrowth (0x685e8d): each sets
/// `TmpExt1` to its alert type (3001 .. 3013 in that order).
pub fn dlg_price_alert_opt(g: &mut Globals, alert_type: i32) {
    g.tmp_ext1 = cast::<Ext, _>(alert_type);
}

/// Port of DlgPriceAlert_Ok (0x685ed9): APPLY keeps `TmpExt1` if it is an alert type 3001..3013, else
/// sets it to 0 with the message " [runtime resource wording] "; ends the dialog.
pub fn dlg_price_alert_ok(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let mut alert_type = to_long(g.tmp_ext1);
    if !(3001..=3013).contains(&alert_type) {
        alert_type = 0;
    }
    g.tmp_ext1 = cast::<Ext, _>(alert_type);
    if alert_type == 0 {
        let mut m = s(__content.string(11312));
        show_quick_message(g, ui, &mut m);
    }
}

/// Port of DlgPriceAlert_Cancel (0x685f90): `TmpExt1 = 0` and ends the dialog.
pub fn dlg_price_alert_cancel(g: &mut Globals) {
    g.tmp_ext1 = Ext::from_f64(0.0);
}

/// Port of DlgProcPriceAlertColors (0x68518b).
/// Dialog procedure of the alert-type dialog: control colours (`WM_CTLCOLOR*`: the dialog and the option
/// buttons on colour 30, the frame 1200 on 15 — returns the background brush `BrushHandles(n)`) and the
/// tooltips of APPLY / CANCEL (`WM_NOTIFY`). Pure window plumbing; `ctrl_id` is `GetDlgCtrlID(lParam)`
/// for the colour messages, `nm` the notification.
pub fn dlg_proc_price_alert_colors(g: &Globals, msg_id: i32, is_dialog: bool, ctrl_id: i32, nm: Option<&mut Nmhdr>) -> i32 {
    let __content = crate::content::current();
    let mut brush = 0;
    match msg_id {
        310 => {
            if is_dialog {
                brush = g.brush_handles[30];
            }
        }
        306..=309 | 311 | 312 => {
            brush = match ctrl_id {
                1200 => g.brush_handles[15],
                1203 | 1206 | 1209 | 1212 | 1215 | 1218 | 1221 | 1224 | 1227 | 1230 | 1233 | 1236 | 1239 => {
                    g.brush_handles[30]
                }
                _ => 0,
            };
        }
        78 => {
            if let Some(nm) = nm {
                if is_tooltip_need_text(nm) {
                    match get_tooltip_ctrl_id(nm, |h| h) {
                        1242 => set_tooltip_text(nm, &s(__content.string(11313))),
                        1 => set_tooltip_text(nm, &s(__content.string(11314))),
                        _ => {}
                    }
                }
                if is_tab_sel_change(nm) {
                    get_notify_hwnd_from(nm);
                }
            }
        }
        _ => {}
    }
    brush
}

/// Port of SetCommodityOrRateAlert (0x68600e).
/// Sets an alert of type `alert_type` 3002..3013 (commodity, crypto, stock index, interest rate or GDP
/// growth): asks for a target (INPUTBOX; "" or a non-positive value other than for GDP growth cancels)
/// that must differ from the current value (else an input error and again), confirms it and stores
/// `type|player|UP/DN|target|quarter|year+1|tick` in the first free slot, incrementing the count.
///
/// The free slot is searched here again; with none free (see BUGS.md #384) the record goes to
/// `PriceAlerts(0)`, whose count then becomes `VAL(record) + 1`.
pub fn set_commodity_or_rate_alert(g: &mut Globals, ui: &mut dyn Ui, alert_type: &mut i32) {
    let __content = crate::content::current();
    let free_slot = (1..=100).find(|&k| g.price_alerts[k].is_empty()).unwrap_or(0);
    let item_code = *alert_type - 3000;
    if !(1 < item_code && item_code < 14) {
        return;
    }
    let mut gdp_text = PbStr::new();
    let (item_name, cur_value): (&str, Ext) = match item_code {
        2 => (__content.string(11315), cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0)),
        3 => ("Gold", cast::<Ext, _>(g.gold_price)),
        4 => ("Silver", cast::<Ext, _>(g.silver_price)),
        5 => ("Wheat", cast::<Ext, _>(g.wheat_price)),
        6 => ("Corn", cast::<Ext, _>(g.corn_price)),
        7 => ("BitCoin", cast::<Ext, _>(g.bitcoin_price)),
        8 => (__content.string(1272), cast::<Ext, _>(g.ethereum_price)),
        9 => (__content.string(11316), g.stock_index),
        10 => (__content.string(11317), cast::<Ext, _>(g.prime_rate)),
        11 => (__content.string(11318), cast::<Ext, _>(g.long_bond_yield)),
        12 => (__content.string(11319), cast::<Ext, _>(g.short_bond_yield)),
        _ => {
            let gdp_tenths = to_long(cast::<Ext, _>(g.gdp_growth) * Ext::from_f64(10.0));
            // FLD SINGLE 0.1 / FIMUL gdpTenths / FSTP DOUBLE (exact in f64)
            let v = cast::<Ext, _>(0.1f32) * cast::<Ext, _>(gdp_tenths);
            gdp_text = PbStr(format(v, b"* #########.0"));
            // (UI) CONTROL SET TEXT hDlgMain, 153, RIGHT$(gdpText, 9)
            let _ = right(gdp_text.as_bytes(), 9);
            (__content.string(11320), v)
        }
    };
    let item_label = s(item_name);
    let mut cur_text = trim(&format(cur_value, b"* ###.00"));
    if item_code == 13 {
        cur_text = gdp_text.clone();
    }
    if 9 < item_code {
        cur_text = cur_text + "%";
    }
    let mut msg_text = s(__content.string(11321))
        + &item_label
        + __content.string(11302)
        + __content.string(11303)
        + &cur_text
        + ".";
    if (10..=12).contains(&item_code) {
        msg_text = replace(msg_text.as_bytes(), __content.bytes(11322), __content.bytes(11323));
    } else if item_code == 13 {
        msg_text = replace(msg_text.as_bytes(), __content.bytes(11324), __content.bytes(11325));
    }
    if 9 < item_code {
        msg_text = replace(msg_text.as_bytes(), b"price", b"rate");
    }
    if g.currency_code != "USA" && item_code < 10 {
        msg_text = msg_text + __content.string(11326);
    }
    let title = s(__content.string(11327)) + &item_label;
    let mut dir_code = PbStr::new();
    let mut dir_word = PbStr::new();
    let (target_value, target_text) = loop {
        let mut valid_input = 1;
        let input_text = ui.inputbox(&msg_text, &title, &PbStr::new());
        if input_text.is_empty() {
            return;
        }
        let target = val_ext(remove_any(input_text.as_bytes(), b", %").as_bytes());
        if target.cmp_val(Ext80::from_ext(Ext::from_f64(0.0))).is_le() && item_code != 13 {
            return;
        }
        let target_text = fmt_trim_ext(target, "* ###.00");
        let cur = val(cur_text.as_bytes());
        let t = val(target_text.as_bytes());
        if cur < t {
            dir_code = s("UP");
            dir_word = s(" above");
        }
        if t < cur {
            dir_code = s("DN");
            dir_word = s(" below");
        }
        if t == cur {
            valid_input = 0;
        }
        if valid_input == 0 {
            let mut err_text = s(__content.string(11328));
            if 9 < item_code {
                err_text = replace(err_text.as_bytes(), b"price", b"rate");
            }
            show_quick_message(g, ui, &mut err_text);
        }
        if valid_input == 1 {
            break (target, target_text);
        }
    };
    let mut msg_text = s(__content.string(11329))
        + &ucase(item_label.as_bytes(), CaseMode::Ansi)
        + ":"
        + "\r"
        + "\r"
        + __content.string(11330);
    if item_code < 10 {
        msg_text = msg_text + __content.string(3292) + &item_label + " goes" + &dir_word + " U.S. $" + &target_text + "." + "\r" + "\r";
    } else {
        msg_text = msg_text + &item_label + " goes" + &dir_word + " " + &target_text + "%." + "\r" + "\r";
        msg_text = replace(msg_text.as_bytes(), b"PRICE", __content.bytes(11331));
    }
    msg_text = msg_text
        + __content.string(11332)
        + __content.string(11333);
    let mut msg_title = s(__content.string(11310)) + &item_label;
    if 8 < item_code {
        msg_title = replace(msg_title.as_bytes(), b"Price", b"Rate");
    }
    ui.msgbox(&msg_text, 8256, &msg_title);
    g.price_alerts[free_slot] = alert_record(g, *alert_type, &dir_code, target_value);
    let alert_count = to_long(val(g.price_alerts[0].as_bytes()) + Ext::from_f64(1.0));
    g.price_alerts[0] = str_l(alert_count);
}
