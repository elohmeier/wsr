//! The commodity desk: `ShowCommodityDesk` (0x627a84) and its callbacks.
//!
//! Before a commodity is selected the first six buttons pick it (CRUDE OIL, GOLD, SILVER, WHEAT, CORN,
//! STOCK INDEX: `CommodSel` 1..6, `CurCommodity`); then they become BUY, SELL, COVER SHORT, SELL SHORT,
//! RE-SELECT and the futures/physical mode switch (`CommodTradeMode`). POSITIONS lists the positions of
//! `ActiveEntity` for a sale/cover, CLOSE ends the desk. The callbacks' names come from the first role
//! of their buttons (`CommodDesk_OilBuyClick` is CRUDE OIL / BUY, etc.).

// Keep the original's statement shapes.
#![allow(clippy::collapsible_if, clippy::collapsible_else_if)]
use crate::pb::real::{Ext, cast};


use super::{Button, choose_button, list_and_trade_positions, s};
use crate::logic::commod_ai::desk::{futures_buy_or_cover, futures_sell_or_short};
use crate::logic::commod_ai::physical::{buy_phys_commod, sell_phys_commod};
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::show_quick_message;
use crate::logic::reports_corp::consolidate_phys_commod_recs;
use crate::logic::restructure::dlg_util::{
    Nmhdr, adjust_dialog_frame_size, get_notify_hwnd_from, get_tooltip_ctrl_id, is_tab_sel_change,
    is_tooltip_need_text, set_tooltip_text,
};
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{CaseMode, len, mcase};
use crate::state::Globals;
use crate::ui::{DialogKind, DialogUi, Ui};

/// Control id of the "Selected:" text box.
pub const ID_SELECTED: i32 = 540;
/// Button ids in display order: CRUDE OIL/BUY, GOLD/SELL, SILVER/COVER SHORT, WHEAT/SELL SHORT,
/// CORN/RE-SELECT, STOCK INDEX/mode switch, POSITIONS, CLOSE.
pub const BUTTON_IDS: [i32; 8] = [542, 544, 546, 548, 550, 552, 554, 556];

/// The desk dialog as a UI would draw it (the original's control texts and enabled states).
#[derive(Clone, Debug, PartialEq)]
pub struct CommodDesk {
    /// Window title "Commodity [Trading ]Desk -- <ActiveEntityName>".
    pub title: PbStr,
    /// Text of the "Selected:" box (control 540).
    pub selected: PbStr,
    /// The buttons in [`BUTTON_IDS`] order.
    pub buttons: Vec<Button>,
    /// False once a callback ended the dialog (`DIALOG END`).
    pub open: bool,
}

impl CommodDesk {
    fn button(&mut self, ctrl_id: i32) -> &mut Button {
        self.buttons.iter_mut().find(|b| b.id == ctrl_id).expect("commodity desk button")
    }
    /// `CONTROL SET TEXT` of a button or the "Selected:" box.
    fn set_text(&mut self, ctrl_id: i32, text: PbStr) {
        if ctrl_id == ID_SELECTED {
            self.selected = text;
        } else {
            self.button(ctrl_id).caption = text;
        }
    }
    /// `CONTROL ENABLE` / `CONTROL DISABLE`.
    fn enable(&mut self, ctrl_id: i32, on: bool) {
        self.button(ctrl_id).enabled = on;
    }
}

/// Port of ShowCommodityDesk (0x627a84).
/// Refused during the first quarter of a new game (`StartYear = CurYear`, Q1, `SimCoCount < 503`: quick
/// message). Otherwise computes the financials of `ActiveEntity`, clears the selection (`CurCommodity`,
/// `CommodSel`), defaults `CommodTradeMode` to "FUTURES" and shows the desk (`DlgOpen_CommodDesk` = 1
/// while open). Coming from a price chart (`TmpExt1 = -999`, reset to 0) the charted commodity
/// (`ChartEntity` 6..10, or 0 for the stock index) is preselected. The dialog runs as a loop over
/// [`Ui::choose`] of the enabled buttons ([`commod_desk_click`]); `None` closes the window without a
/// callback. `DlgCommodDesk` (the window handle) is not modelled.
pub fn show_commodity_desk(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, _h_parent: i32) {
    let __content = crate::content::current();
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 503 {
        let mut m = s(__content.string(11342))
            + str_num(cast::<Ext, _>(g.cur_year), 15).as_str()
            + __content.string(11343);
        show_quick_message(g, ui, &mut m);
        return;
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    g.cur_commodity = PbStr::new();
    g.commod_sel = 0;
    if g.commod_trade_mode.is_empty() {
        g.commod_trade_mode = s("FUTURES");
    }
    let dlg_style: u32 = 0x80c0_0880;
    let mut label_text = s(__content.string(11344));
    let max_name_len = 15;
    if max_name_len < len(g.active_entity_name.as_bytes()) {
        label_text = PbStr::new();
    }
    let title = s(__content.string(11345)) + &label_text + __content.string(11346) + &g.active_entity_name;
    // DlgCommodDesk = DIALOG NEW(hParent, title, 0, 0, 203, 207, dlgStyle, 0)
    adjust_dialog_frame_size(g.dlg_commod_desk, dlg_style, 0);
    let index_label = if g.commod_trade_mode == "PHYSICAL" { "---------" } else { __content.string(11347) };
    let mut desk = CommodDesk {
        title,
        selected: s(__content.string(11348)),
        buttons: vec![
            Button::new(542, __content.string(11349)),
            Button::new(544, "&GOLD"),
            Button::new(546, "&SILVER"),
            Button::new(548, "&WHEAT"),
            Button::new(550, "CO&RN"),
            Button::new(552, index_label),
            Button::new(554, __content.string(11350)),
            Button::new(556, "CL&OSE"),
        ],
        open: true,
    };
    g.dlg_open_commod_desk = 1;
    if g.tmp_ext1 == -Ext::from_f64(999.0) {
        g.tmp_ext1 = Ext::from_f64(0.0);
        if (g.chart_entity == 0 || 5 < g.chart_entity) && g.chart_entity < 11 {
            g.commod_sel = g.chart_entity - 5;
            if g.chart_entity == 0 {
                g.commod_sel = 6;
            }
            match g.commod_sel {
                1 => g.cur_commodity = s("OIL"),
                2 => g.cur_commodity = s("GOLD"),
                3 => g.cur_commodity = s("SILVER"),
                4 => g.cur_commodity = s("WHEAT"),
                5 => g.cur_commodity = s("CORN"),
                6 => {
                    g.cur_commodity = s("STOCKS");
                    g.commod_trade_mode = s("FUTURES");
                }
                _ => {}
            }
            commod_desk_show_trade_buttons(g, &mut desk);
        }
    }
    // DIALOG SHOW MODAL DlgCommodDesk CALL DlgProcCommodDesk
    let mut modal_ui = DialogUi::new(ui, DialogKind::CommodityDesk);
    let ui = &mut modal_ui;
    while desk.open && ui.open {
        let prompt = [desk.selected.clone()];
        let title = desk.title.clone();
        match choose_button(ui, &title, &prompt, &desk.buttons) {
            Some(ctrl_id) => commod_desk_click(g, ui, rng, &mut desk, ctrl_id),
            None => desk.open = false,
        }
    }
    g.dlg_open_commod_desk = 0;
}

/// Runs the callback of desk button `ctrl_id` (a click, `BN_CLICKED`).
pub fn commod_desk_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, desk: &mut CommodDesk, ctrl_id: i32) {
    match ctrl_id {
        542 => commod_desk_oil_buy_click(g, ui, rng, desk),
        544 => commod_desk_gold_sell_click(g, ui, rng, desk),
        546 => commod_desk_silver_cover_click(g, ui, rng, desk),
        548 => commod_desk_wheat_short_click(g, ui, rng, desk),
        550 => commod_desk_corn_reselect_click(g, ui, desk),
        552 => commod_desk_index_mode_click(g, ui, desk),
        554 => commod_desk_positions_click(g, ui, rng, desk),
        556 => commod_desk_close_click(g, desk),
        _ => {}
    }
}

/// After a trade: `DIALOG SHOW STATE DlgCommodDesk, 5`; the desk closes once the player has used up the
/// turn's actions (`TurnActionCount > 4`, not in solo mode).
fn end_if_turn_used(g: &Globals, desk: &mut CommodDesk) {
    if 4 < g.turn_action_count && g.solo_human_mode == 0 {
        desk.open = false;
    }
}

/// Port of CommodDesk_OilBuyClick (0x6286e9).
/// Unselected: selects crude oil (`CommodSel` 1). Selected: BUY — physical mode buys `CurCommodity`
/// ([`buy_phys_commod`]), futures mode opens a long position ([`futures_buy_or_cover`] mode 1).
pub fn commod_desk_oil_buy_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, desk: &mut CommodDesk) {
    if g.commod_sel == 0 {
        g.commod_sel = 1;
        g.cur_commodity = s("OIL");
        commod_desk_show_trade_buttons(g, desk);
    } else {
        // (UI) DIALOG SHOW STATE DlgCommodDesk, 0
        let ae = g.active_entity;
        if g.commod_trade_mode == "PHYSICAL" {
            buy_phys_commod(g, ui, rng, ae, 0);
        } else {
            futures_buy_or_cover(g, ui, rng, 1, ae, 0);
        }
        end_if_turn_used(g, desk);
    }
}

/// Port of CommodDesk_GoldSellClick (0x628839).
/// Unselected: selects gold (`CommodSel` 2). Selected: SELL — physical mode sells the `CurCommodity`
/// record ([`sell_phys_commod`], after `ConsolidatePhysCommodRecs` for a company), futures mode sells a
/// long position ([`futures_sell_or_short`] mode 2).
pub fn commod_desk_gold_sell_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, desk: &mut CommodDesk) {
    if g.commod_sel == 0 {
        g.commod_sel = 2;
        g.cur_commodity = s("GOLD");
        commod_desk_show_trade_buttons(g, desk);
    } else {
        let ae = g.active_entity;
        if g.commod_trade_mode == "PHYSICAL" {
            if 10 < ae {
                consolidate_phys_commod_recs(g, ae);
            }
            sell_phys_commod(g, ui, rng, ae, 0, 0, 0.0f32, 0);
        } else {
            futures_sell_or_short(g, ui, rng, 2, ae, 0);
        }
        end_if_turn_used(g, desk);
    }
}

/// Port of CommodDesk_SilverCoverClick (0x6289c6).
/// Unselected: selects silver (`CommodSel` 3). Selected: COVER SHORT ([`futures_buy_or_cover`] mode 2;
/// the button is disabled in physical mode).
pub fn commod_desk_silver_cover_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, desk: &mut CommodDesk) {
    if g.commod_sel == 0 {
        g.commod_sel = 3;
        g.cur_commodity = s("SILVER");
        commod_desk_show_trade_buttons(g, desk);
    } else {
        let ae = g.active_entity;
        futures_buy_or_cover(g, ui, rng, 2, ae, 0);
        end_if_turn_used(g, desk);
    }
}

/// Port of CommodDesk_WheatShortClick (0x628a9d).
/// Unselected: selects wheat (`CommodSel` 4). Selected: SELL SHORT ([`futures_sell_or_short`] mode 1;
/// disabled in physical mode).
pub fn commod_desk_wheat_short_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, desk: &mut CommodDesk) {
    if g.commod_sel == 0 {
        g.commod_sel = 4;
        g.cur_commodity = s("WHEAT");
        commod_desk_show_trade_buttons(g, desk);
    } else {
        let ae = g.active_entity;
        futures_sell_or_short(g, ui, rng, 1, ae, 0);
        end_if_turn_used(g, desk);
    }
}

/// Port of CommodDesk_CornReselectClick (0x628b74).
/// Unselected: selects corn (`CommodSel` 5). Selected: RE-SELECT — refused for an insurance company
/// (`IndustryNum` < 3: quick message), else the selection is reset ([`commod_desk_reset_selection`]).
pub fn commod_desk_corn_reselect_click(g: &mut Globals, ui: &mut dyn Ui, desk: &mut CommodDesk) {
    let __content = crate::content::current();
    if g.commod_sel == 0 {
        g.commod_sel = 5;
        g.cur_commodity = s("CORN");
        commod_desk_show_trade_buttons(g, desk);
    } else {
        if 10 < g.active_entity && g.industry_num[g.active_entity] < 3 {
            let mut m = s(__content.string(11351))
                + "\r\n"
                + "\r\n"
                + __content.string(11352)
                + __content.string(11353)
                + __content.string(11354);
            show_quick_message(g, ui, &mut m);
            return;
        }
        commod_desk_reset_selection(g, desk);
        desk.enable(546, true);
        desk.enable(548, true);
    }
}

/// Port of CommodDesk_IndexModeClick (0x628cc0).
/// Unselected: selects the stock index (`CommodSel` 6). With a commodity selected (`CommodSel` < 6)
/// toggles `CommodTradeMode` between "FUTURES" and "PHYSICAL" (caption "--> FUTURES" / "--> PHYSICAL";
/// COVER SHORT and SELL SHORT are disabled in physical mode). The first switch each way explains the
/// mode once (quick message; `CommodModeHintMask` bit 1 physical, bit 2 futures).
pub fn commod_desk_index_mode_click(g: &mut Globals, ui: &mut dyn Ui, desk: &mut CommodDesk) {
    let __content = crate::content::current();
    if g.commod_sel == 0 {
        g.commod_sel = 6;
        g.cur_commodity = s("STOCKS");
        commod_desk_show_trade_buttons(g, desk);
    } else if g.commod_sel < 6 {
        if g.commod_trade_mode == "FUTURES" {
            g.commod_trade_mode = s("PHYSICAL");
            desk.set_text(552, s(__content.string(11355)));
            desk.enable(546, false);
            desk.enable(548, false);
            if g.commod_mode_hint_mask == 0 || g.commod_mode_hint_mask == 2 {
                let mut m = s(__content.string(11356))
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11357)
                    + __content.string(11358)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11359)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11360);
                show_quick_message(g, ui, &mut m);
                g.commod_mode_hint_mask = g.commod_mode_hint_mask + 1;
            }
        } else if g.commod_trade_mode == "PHYSICAL" {
            g.commod_trade_mode = s("FUTURES");
            desk.set_text(552, s(__content.string(11361)));
            desk.enable(546, true);
            desk.enable(548, true);
            if g.commod_mode_hint_mask == 0 || g.commod_mode_hint_mask == 1 {
                let mut m = s(__content.string(11362))
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11363)
                    + "\r\n"
                    + "\r\n"
                    + __content.string(11360);
                show_quick_message(g, ui, &mut m);
                g.commod_mode_hint_mask = g.commod_mode_hint_mask + 2;
            }
        }
    }
}

/// Port of CommodDesk_PositionsClick (0x628ff8).
/// Clears the selection (`CommodSel`, `CurCommodity`; `CommodTradeMode` defaults to "FUTURES"), ends the
/// desk and lets the player pick a position of `ActiveEntity` to sell or cover
/// ([`super::list_and_trade_positions`]).
pub fn commod_desk_positions_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, desk: &mut CommodDesk) {
    g.commod_sel = 0;
    g.cur_commodity = PbStr::new();
    if g.commod_trade_mode.is_empty() {
        g.commod_trade_mode = s("FUTURES");
    }
    desk.open = false;
    list_and_trade_positions(g, ui, rng);
}

/// Port of CommodDesk_ShowTradeButtons (0x629775).
/// After a selection: "Selected:  <commodity>" and the trade captions (BUY, SELL, COVER SHORT, SELL SHORT,
/// RE-SELECT); for a commodity the mode switch ("--> PHYSICAL" / "--> FUTURES", the short buttons
/// disabled in physical mode), for the stock index (`CommodSel` 6) futures mode and "---------".
pub fn commod_desk_show_trade_buttons(g: &mut Globals, desk: &mut CommodDesk) {
    let __content = crate::content::current();
    let mut commod_label = mcase(g.cur_commodity.as_bytes(), CaseMode::Ansi);
    if commod_label == "Oil" {
        commod_label = s(__content.string(11315));
    }
    if commod_label == "Stocks" {
        commod_label = s(__content.string(11316));
    }
    desk.set_text(ID_SELECTED, s(__content.string(5674)) + &commod_label);
    desk.set_text(542, s("&BUY"));
    desk.set_text(544, s("&SELL"));
    desk.set_text(546, s(__content.string(11364)));
    desk.set_text(548, s(__content.string(11365)));
    desk.set_text(550, s(__content.string(11366)));
    if g.commod_sel < 6 {
        if g.commod_trade_mode == "FUTURES" {
            desk.set_text(552, s(__content.string(11361)));
        } else if g.commod_trade_mode == "PHYSICAL" {
            desk.set_text(552, s(__content.string(11355)));
            desk.enable(546, false);
            desk.enable(548, false);
        }
    } else if g.commod_sel == 6 {
        g.commod_trade_mode = s("FUTURES");
        desk.set_text(552, s("---------"));
    }
    // (UI) CONTROL SET FOCUS DlgCommodDesk, 556
}

/// Port of CommodDesk_ResetSelection (0x6299f6): clears `CurCommodity` / `CommodSel` and restores the
/// selection captions (CRUDE OIL … STOCK INDEX), SILVER and WHEAT enabled.
pub fn commod_desk_reset_selection(g: &mut Globals, desk: &mut CommodDesk) {
    let __content = crate::content::current();
    g.cur_commodity = PbStr::new();
    g.commod_sel = 0;
    desk.set_text(ID_SELECTED, s(__content.string(11348)));
    desk.set_text(542, s(__content.string(11349)));
    desk.set_text(544, s("&GOLD"));
    desk.set_text(546, s("&SILVER"));
    desk.set_text(548, s("&WHEAT"));
    desk.set_text(550, s("CO&RN"));
    desk.set_text(552, s(__content.string(11347)));
    desk.enable(546, true);
    desk.enable(548, true);
}

/// Port of CommodDesk_CloseClick (0x629709): clears the selection (`CommodSel`, `CurCommodity`) and ends
/// the desk.
pub fn commod_desk_close_click(g: &mut Globals, desk: &mut CommodDesk) {
    g.commod_sel = 0;
    g.cur_commodity = PbStr::new();
    desk.open = false;
}

/// Port of DlgProcCommodDesk (0x628251).
/// Dialog procedure of the desk: the colours of the "Selected:" box 540 (`WM_CTLCOLOR*`: text colour 0 on
/// 22, returns `BrushHandles(22)`) and the button tooltips (`WM_NOTIFY`: "Control -  %FORM18_…"). Pure
/// window plumbing; `ctrl_id` is `GetDlgCtrlID(lParam)` for the colour messages, `nm` the notification.
pub fn dlg_proc_commod_desk(g: &Globals, msg_id: i32, ctrl_id: i32, nm: Option<&mut Nmhdr>) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    match msg_id {
        // WM_CTLCOLORMSGBOX..WM_CTLCOLORSTATIC (not WM_CTLCOLORDLG)
        306..=309 | 311 | 312 => {
            result = if ctrl_id == ID_SELECTED { g.brush_handles[22] } else { 0 };
        }
        // WM_NOTIFY
        78 => {
            if let Some(nm) = nm {
                if is_tooltip_need_text(nm) {
                    let tip = match get_tooltip_ctrl_id(nm, |h| h) {
                        542 => __content.string(11367),
                        544 => __content.string(11368),
                        546 => __content.string(11369),
                        548 => __content.string(11370),
                        550 => __content.string(11371),
                        552 => __content.string(11372),
                        554 => __content.string(11373),
                        556 => __content.string(11374),
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
    result
}
