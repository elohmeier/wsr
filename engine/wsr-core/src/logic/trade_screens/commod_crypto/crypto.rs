//! The crypto-currency trading platform: `ShowDlgCrypto` (0x6b4357) and its callbacks.
//!
//! Two columns (Bitcoin, Ethereum) of: spot price, buy, sell (physical coins via `BuyPhysCommod` /
//! `SellPhysCommod`), buy/cover futures, sell/short futures, chart; plus LIST (positions) and CLOSE.

// Keep the original's statement shapes.
#![allow(clippy::collapsible_if, clippy::collapsible_else_if)]
use crate::pb::real::{Ext, cast};


use super::{Button, list_and_trade_positions, plain_caption, s};
use crate::logic::commod_ai::desk::{futures_buy_or_cover, futures_sell_or_short};
use crate::logic::commod_ai::physical::{buy_phys_commod, sell_phys_commod};
use crate::logic::economy::close_all_dialogs;
use crate::logic::futures::count_futures_positions;
use crate::logic::info_screens::chart::show_price_chart;
use crate::logic::newgame::LocalTime;
use crate::logic::report::show_quick_message;
use crate::logic::restructure::dlg_util::{
    Nmhdr, adjust_dialog_frame_size, get_notify_hwnd_from, get_tooltip_ctrl_id, is_tab_sel_change,
    is_tooltip_need_text, set_tooltip_text,
};
use crate::pb::numeric::format;
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, ltrim, mcase, right, rtrim};
use crate::state::Globals;
use crate::ui::{Ui, id, mb};

/// Button ids in display order: Buy Bitcoin, Buy Ethereum, Sell Bitcoin, Sell Ethereum, Bitcoin
/// Buy/Cover Futures, Ethereum Buy/Cover Futures, Bitcoin Sell/Short Futures, Ethereum Sell/Short
/// Futures, Bitcoin chart, Ethereum chart, LIST, CLOSE (id 1, the default button).
pub const BUTTON_IDS: [i32; 12] = [1142, 1145, 1148, 1151, 1154, 1157, 1160, 1163, 1166, 1169, 1170, 1];

/// The crypto dialog as a UI would draw it.
#[derive(Clone, Debug, PartialEq)]
pub struct CryptoDlg {
    /// Spot price texts of the two price boxes (1172 Bitcoin, 1175 Ethereum).
    pub btc_price_text: PbStr,
    pub eth_price_text: PbStr,
    /// The buttons in [`BUTTON_IDS`] order.
    pub buttons: Vec<Button>,
    /// False once a callback ended the dialog.
    pub open: bool,
}

/// The option text [`show_dlg_crypto`] shows for a button: the caption, prefixed with its column for
/// the coin-specific buttons whose captions are the same in both columns.
pub fn crypto_option_label(b: &Button) -> PbStr {
    let __content = crate::content::current();
    let cap = plain_caption(&b.caption);
    match b.id {
        1154 | 1160 | 1166 => s(__content.string(11375)) + &cap,
        1157 | 1163 | 1169 => s(__content.string(11376)) + &cap,
        _ => cap,
    }
}

/// `RIGHT$(FORMAT$(price, "###########,"), 9)` or the placeholder for a zero price.
fn price_box_text(price: i32, zero_text: &str) -> PbStr {
    if price == 0 {
        return s(zero_text);
    }
    let t = PbStr(format(cast::<Ext, _>(price), b"###########,"));
    right(t.as_bytes(), 9)
}

/// Port of ShowDlgCrypto (0x6b4357).
/// Refused during the first quarter of a new game (`StartYear = CurYear`, Q1, `SimCoCount < 503`: quick
/// message; the original has already created the dialog, it is never shown). Otherwise shows the
/// platform (`DlgOpen_Crypto` = 1 while open) as a loop over [`Ui::choose`] of its buttons
/// ([`dlg_crypto_click`]; `None` closes it), then `CloseAllDialogs(2)`. The window handle `DlgCrypto`
/// and showing/hiding the main and transaction windows are not modelled.
pub fn show_dlg_crypto(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, _h_parent: i32) {
    show_dlg_crypto_skipping_chart(g, ui, rng, now, false);
}

/// Variant for differential tests that intercept the original chart call.
pub fn show_dlg_crypto_skipping_chart(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, skip_chart: bool) {
    let __content = crate::content::current();
    let dlg_style: u32 = 0x80ca_0880;
    // DlgCrypto = DIALOG NEW(hParent, " [runtime resource wording] ", 0, 0, 260, 256, dlgStyle, 0)
    adjust_dialog_frame_size(g.dlg_crypto, dlg_style, 0);
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 503 {
        let mut m = s(__content.string(11342))
            + str_num(cast::<Ext, _>(g.cur_year), 15).as_str()
            + __content.string(11343);
        show_quick_message(g, ui, &mut m);
        return;
    }
    // The original's local `futuresMode` is never set: its "/Short" caption suffix is dead code.
    let mut dlg = CryptoDlg {
        btc_price_text: price_box_text(g.bitcoin_price, "50,000"),
        eth_price_text: price_box_text(g.ethereum_price, "2,000"),
        buttons: vec![
            Button::new(1142, __content.string(11377)),
            Button::new(1145, __content.string(11378)),
            Button::new(1148, __content.string(11379)),
            Button::new(1151, __content.string(11380)),
            Button::new(1154, __content.string(11381)),
            Button::new(1157, __content.string(11382)),
            Button::new(1160, __content.string(11383)),
            Button::new(1163, __content.string(11384)),
            Button::new(1166, __content.string(11385)),
            Button::new(1169, __content.string(11386)),
            Button::new(1170, "LIST"),
            Button::new(1, "CL&OSE"),
        ],
        open: true,
    };
    // (UI) DIALOG SHOW STATE hDlgMain, 5: DIALOG SHOW STATE hDlgTransMenu, 0
    g.dlg_open_crypto = 1;
    let title = s(__content.string(11387));
    while dlg.open {
        let prompt = [
            s("BITCOIN TRADING:  Spot: ") + &dlg.btc_price_text,
            s("ETHEREUM TRADING:  Spot: ") + &dlg.eth_price_text,
        ];
        let shown = dlg.buttons.clone();
        let options: Vec<PbStr> = shown.iter().map(crypto_option_label).collect();
        let enabled: Vec<_> = shown.iter().map(|b| b.enabled).collect();
        match ui.choose_enabled(&title, &prompt, &options, &enabled) {
            Some(k) => dlg_crypto_click_inner(g, ui, rng, now, &mut dlg, shown[k].id, skip_chart),
            None => dlg.open = false,
        }
    }
    g.dlg_open_crypto = 0;
    // (UI) DIALOG SHOW STATE hDlgTransMenu, 5
    close_all_dialogs(g, ui, rng, 2);
}

/// Runs the callback of crypto button `ctrl_id`.
pub fn dlg_crypto_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, dlg: &mut CryptoDlg, ctrl_id: i32) {
    dlg_crypto_click_inner(g, ui, rng, now, dlg, ctrl_id, false);
}

fn dlg_crypto_click_inner(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, dlg: &mut CryptoDlg, ctrl_id: i32, skip_chart: bool) {
    match ctrl_id {
        1142 => dlg_crypto_buy_bitcoin(g, ui, rng, dlg),
        1145 => dlg_crypto_buy_ethereum(g, ui, rng, dlg),
        1148 => dlg_crypto_sell_bitcoin(g, ui, rng, dlg),
        1151 => dlg_crypto_sell_ethereum(g, ui, rng, dlg),
        1154 => dlg_crypto_buy_cover_btc_futures(g, ui, rng, dlg),
        1157 => dlg_crypto_buy_cover_eth_futures(g, ui, rng, dlg),
        1160 => dlg_crypto_sell_short_btc_futures(g, ui, rng, dlg),
        1163 => dlg_crypto_sell_short_eth_futures(g, ui, rng, dlg),
        1166 if skip_chart => g.chart_entity = 1605,
        1169 if skip_chart => g.chart_entity = 1606,
        1166 => dlg_crypto_chart_bitcoin(g, ui, rng, now),
        1169 => dlg_crypto_chart_ethereum(g, ui, rng, now),
        1170 => dlg_crypto_list_positions(g, ui, rng),
        1 => dlg_crypto_close(dlg),
        _ => {}
    }
}

/// After a trade (`DIALOG SHOW STATE DlgCrypto, 5`): the dialog closes once the turn's actions are used up
/// (`TurnActionCount > 4`, not in solo mode).
fn end_if_turn_used(g: &Globals, dlg: &mut CryptoDlg) {
    if 4 < g.turn_action_count && g.solo_human_mode == 0 {
        dlg.open = false;
    }
}

fn buy_coin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg, coin: &str) {
    // (UI) DIALOG SHOW STATE DlgCrypto, 0
    g.cur_commodity = s(coin);
    let ae = g.active_entity;
    buy_phys_commod(g, ui, rng, ae, 0);
    end_if_turn_used(g, dlg);
}

fn sell_coin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg, coin: &str) {
    g.cur_commodity = s(coin);
    let ae = g.active_entity;
    sell_phys_commod(g, ui, rng, ae, 0, 0, 0.0f32, 0);
    end_if_turn_used(g, dlg);
}

/// Port of DlgCrypto_BuyBitcoin (0x6b551b): buys bitcoin for `ActiveEntity` ([`buy_phys_commod`]).
pub fn dlg_crypto_buy_bitcoin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    buy_coin(g, ui, rng, dlg, "BITCOIN");
}

/// Port of DlgCrypto_BuyEthereum (0x6b55bf): buys ether for `ActiveEntity` ([`buy_phys_commod`]).
pub fn dlg_crypto_buy_ethereum(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    buy_coin(g, ui, rng, dlg, "ETHEREUM");
}

/// Port of DlgCrypto_SellBitcoin (0x6b5663): sells the bitcoin of `ActiveEntity` ([`sell_phys_commod`]).
pub fn dlg_crypto_sell_bitcoin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    sell_coin(g, ui, rng, dlg, "BITCOIN");
}

/// Port of DlgCrypto_SellEthereum (0x6b572e): sells the ether of `ActiveEntity` ([`sell_phys_commod`]).
pub fn dlg_crypto_sell_ethereum(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    sell_coin(g, ui, rng, dlg, "ETHEREUM");
}

/// BUY/COVER futures of `coin`: with no or only long positions in `CurCommodity`
/// (`CountFuturesPositions(ActiveEntity, 1)` 0 or 1) a new long position ([`futures_buy_or_cover`] mode
/// 1), with short ones (-1, or 2 = both) the cover-or-buy question ([`prompt_close_crypto_futures`] 1).
fn buy_cover_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg, coin: &str) {
    g.cur_commodity = s(coin);
    let ae = g.active_entity;
    let pos_state = count_futures_positions(g, ae, 1);
    if pos_state == 0 || pos_state == 1 {
        futures_buy_or_cover(g, ui, rng, 1, ae, 0);
    } else if pos_state == -1 || pos_state == 2 {
        prompt_close_crypto_futures(g, ui, rng, 1);
    }
    end_if_turn_used(g, dlg);
}

/// SELL/SHORT futures of `coin`: with no or only short positions (0 / -1) a new short position
/// ([`futures_sell_or_short`] mode 1), with long ones (1 / 2) the sell-or-short question
/// ([`prompt_close_crypto_futures`] 2).
fn sell_short_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg, coin: &str) {
    g.cur_commodity = s(coin);
    let ae = g.active_entity;
    let pos_state = count_futures_positions(g, ae, 1);
    if pos_state == 0 || pos_state == -1 {
        futures_sell_or_short(g, ui, rng, 1, ae, 0);
    } else if pos_state == 1 || pos_state == 2 {
        prompt_close_crypto_futures(g, ui, rng, 2);
    }
    end_if_turn_used(g, dlg);
}

/// Port of DlgCrypto_BuyCoverBtcFutures (0x6b57f9): buy or cover bitcoin futures.
pub fn dlg_crypto_buy_cover_btc_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    buy_cover_futures(g, ui, rng, dlg, "BITCOIN");
}

/// Port of DlgCrypto_BuyCoverEthFutures (0x6b5915): buy or cover ethereum futures.
pub fn dlg_crypto_buy_cover_eth_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    buy_cover_futures(g, ui, rng, dlg, "ETHEREUM");
}

/// Port of DlgCrypto_SellShortBtcFutures (0x6b5a31): sell or short bitcoin futures.
pub fn dlg_crypto_sell_short_btc_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    sell_short_futures(g, ui, rng, dlg, "BITCOIN");
}

/// Port of DlgCrypto_SellShortEthFutures (0x6b5b4d): sell or short ethereum futures.
pub fn dlg_crypto_sell_short_eth_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut CryptoDlg) {
    sell_short_futures(g, ui, rng, dlg, "ETHEREUM");
}

/// Port of DlgCrypto_ChartBitcoin (0x6b5c69): the bitcoin price chart (`ChartEntity` 1605).
pub fn dlg_crypto_chart_bitcoin(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    g.chart_entity = 1605;
    show_price_chart(g, ui, rng, now);
}

/// Port of DlgCrypto_ChartEthereum (0x6b5cba): the ethereum price chart (`ChartEntity` 1606).
pub fn dlg_crypto_chart_ethereum(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    g.chart_entity = 1606;
    show_price_chart(g, ui, rng, now);
}

/// Port of DlgCrypto_Close (0x6b5d0b): ends the dialog.
pub fn dlg_crypto_close(dlg: &mut CryptoDlg) {
    dlg.open = false;
}

/// Port of DlgCrypto_ListPositions (0x6b5d64).
/// Clears `CurCommodity` (`CommodTradeMode` defaults to "FUTURES") and lets the player pick a futures or
/// physical/crypto position of `ActiveEntity` to sell or cover ([`super::list_and_trade_positions`]; the
/// same code as the commodity desk's POSITIONS, but the dialog stays open).
pub fn dlg_crypto_list_positions(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    g.cur_commodity = PbStr::new();
    if g.commod_trade_mode.is_empty() {
        g.commod_trade_mode = s("FUTURES");
    }
    list_and_trade_positions(g, ui, rng);
}

/// Port of PromptCloseCryptoFutures (0x6b6471).
/// `direction` 1 (the entity is short `CurCommodity` futures): YES/NO "cover (buy back) such positions?"
/// — YES covers ([`futures_buy_or_cover`] mode 2), NO buys a new long position (mode 1). Otherwise (long
/// positions): "sell off such positions?" — YES sells ([`futures_sell_or_short`] mode 2), NO sells short
/// (mode 1). The text names the coin's spot price.
pub fn prompt_close_crypto_futures(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, direction: i32) {
    let __content = crate::content::current();
    let coin_name = mcase(g.cur_commodity.as_bytes(), CaseMode::Ansi);
    let mut msg_text = s(__content.string(4950));
    if 10 < g.active_entity {
        msg_text = g.active_entity_name.clone() + " has ";
    }
    let cur_sym = trim(g.currency_symbol.as_bytes());
    let mut price_text = cur_sym.clone() + &trim(&format(cast::<Ext, _>(g.bitcoin_price), b"* ###,"));
    if g.cur_commodity == "ETHEREUM" {
        price_text = cur_sym + &trim(&format(cast::<Ext, _>(g.ethereum_price), b"* ###,"));
    }
    let price_note = s(__content.string(11388)) + &coin_name + " is " + &price_text + __content.string(11389);
    let title = s("Close ") + &coin_name + __content.string(11390);
    let style = mb::YESNO | mb::ICONQUESTION | mb::TASKMODAL;
    let ae = g.active_entity;
    if direction == 1 {
        msg_text = msg_text
            + __content.string(11391)
            + &coin_name
            + "."
            + &price_note
            + "\r"
            + "\r"
            + __content.string(11392)
            + "\r"
            + "\r"
            + __content.string(11393);
        let answer = ui.msgbox(&msg_text, style, &title);
        if answer == id::YES {
            futures_buy_or_cover(g, ui, rng, 2, ae, 0);
        } else {
            futures_buy_or_cover(g, ui, rng, 1, ae, 0);
        }
    } else {
        msg_text = msg_text
            + __content.string(11394)
            + &coin_name
            + "."
            + &price_note
            + "\r"
            + "\r"
            + __content.string(11395)
            + "\r"
            + "\r"
            + __content.string(11396)
            + &coin_name
            + __content.string(11397);
        let answer = ui.msgbox(&msg_text, style, &title);
        if answer == id::YES {
            futures_sell_or_short(g, ui, rng, 2, ae, 0);
        } else {
            futures_sell_or_short(g, ui, rng, 1, ae, 0);
        }
    }
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &[u8]) -> PbStr {
    rtrim(ltrim(x).as_bytes())
}

/// Port of DlgProcCryptoColors (0x6b4da7).
/// Dialog procedure of the crypto platform: colours (`WM_CTLCOLORDLG` for the dialog itself: 0 on 0,
/// `BrushHandles(0)`; the labels 1130/1136 in colour 11, 1133/1139 in 14, on 0) and the button tooltips
/// ("Control -  %FORM23_…"). Pure window plumbing; `is_dialog`: the colour message is for the dialog
/// itself (`hDlg = lParam`), `ctrl_id` = `GetDlgCtrlID(lParam)`, `nm` the notification.
pub fn dlg_proc_crypto_colors(g: &Globals, msg_id: i32, is_dialog: bool, ctrl_id: i32, nm: Option<&mut Nmhdr>) -> i32 {
    let __content = crate::content::current();
    let mut ret_val = 0;
    match msg_id {
        // WM_CTLCOLORDLG
        310 => {
            if is_dialog {
                ret_val = g.brush_handles[0];
            }
        }
        // WM_CTLCOLORMSGBOX..WM_CTLCOLORSTATIC
        306..=309 | 311 | 312 => {
            ret_val = match ctrl_id {
                1130 | 1133 | 1136 | 1139 => g.brush_handles[0],
                _ => 0,
            };
        }
        // WM_NOTIFY
        78 => {
            if let Some(nm) = nm {
                if is_tooltip_need_text(nm) {
                    let tip = match get_tooltip_ctrl_id(nm, |h| h) {
                        1142 => __content.string(11398),
                        1145 => __content.string(11399),
                        1148 => __content.string(11400),
                        1151 => __content.string(11401),
                        1154 => __content.string(11402),
                        1157 => __content.string(11403),
                        1160 => __content.string(11404),
                        1163 => __content.string(11405),
                        1166 => __content.string(11406),
                        1169 => __content.string(11407),
                        1 => __content.string(11408),
                        1170 => __content.string(11409),
                        _ => "",
                    };
                    if !tip.is_empty() {
                        set_tooltip_text(nm, &s(tip));
                    }
                }
                if is_tab_sel_change(nm) {
                    get_notify_hwnd_from(nm);
                }
            }
        }
        _ => {}
    }
    ret_val
}
