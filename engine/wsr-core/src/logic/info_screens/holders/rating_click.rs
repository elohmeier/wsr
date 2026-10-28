//! The research report's analyst-rating label: a click offers to buy (rating 1/2) or sell (4/5) the stock.
use crate::pb::real::cast;


use crate::logic::control::update_corp_control;
use crate::logic::economy::close_all_dialogs;
use crate::logic::holdings::find_stock_holding;
use crate::logic::market::select_target_corp;
use crate::logic::newgame::LocalTime;
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::s;
use crate::logic::shell::{Env, menu::do_transaction_menu_action};
use crate::logic::trading::select_acting_entity;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, replace};
use crate::state::Globals;
use crate::ui::Ui;

/// Port of ReportDlg_RatingClick (0x4188bf), the BN_CLICKED callback of the report dialog's rating label.
/// Only acts while a research report is shown (`TextBoxTitle` contains `"=== RESEARCH REPORT"`); takes
/// (and clears) `ResearchReportCorp` and clears `LargestHolder`. For a "buy" rating (`CurAnalystRating`
/// 1/2) it offers to buy the stock, for a "sell" rating (4/5) to sell it — or, when the acting entity
/// owns none, to let a major shareholder the player controls (`LargestHolder` of
/// [`update_corp_control`]) sell; without one it shows "... Cancelling...." and ends. A confirmed offer
/// ([`report_dlg_rating_click_confirm_trade`]) selects the acting entity / the shareholder, then the
/// transaction menu opens (`ActionMenuMode = 4`, `SkipMarginPrompt = "Y"` for a player already holding
/// the stock long, [`close_all_dialogs`], `DoTransactionMenuAction(1)`).
///
/// Returns `true` when the report dialog is ended (`DIALOG END hDlg`: declined offer, or "Cancelling").
/// The caller (the report dialog) invokes it while the research report is open, i.e. before
/// [`crate::logic::report::show_text_report`] resets the report globals. `now`: local time for
/// [`select_target_corp`] (TIME$ seconds) and [`select_acting_entity`].
pub fn report_dlg_rating_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) -> bool {
    report_dlg_rating_click_with_env(g, ui, rng, &Env::new(now))
}

/// Variant using the shell environment, including its differential-test skip list.
pub fn report_dlg_rating_click_with_env(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) -> bool {
    let __content = crate::content::current();
    let now = env.now;
    let corp_id = g.research_report_corp;
    g.research_report_corp = 0;
    g.largest_holder = 0;
    if !(0 < instr(1, g.text_box_title.as_bytes(), __content.bytes(6189))) {
        return report_dlg_rating_click_exit(false);
    }
    let mut msg_text;
    if g.cur_analyst_rating == 1 || g.cur_analyst_rating == 2 {
        if g.active_entity == g.sel_entity && 10 < g.active_entity && g.sel_entity == corp_id {
            msg_text = s(__content.string(6190)) + &g.sel_corp_name + "?";
            if !report_dlg_rating_click_confirm_trade(g, ui, rng, now, corp_id, &msg_text) {
                return report_dlg_rating_click_exit(true);
            }
        } else if g.active_entity == g.sel_entity && corp_id != g.sel_entity {
            msg_text = s(__content.string(6191)) + &g.ent_name[corp_id] + "?";
            if !report_dlg_rating_click_confirm_trade(g, ui, rng, now, corp_id, &msg_text) {
                return report_dlg_rating_click_exit(true);
            }
        }
        g.action_menu_mode = 4;
        if g.active_entity < 11 && 0 <= find_stock_holding(g, g.active_entity, corp_id) {
            g.skip_margin_prompt = s("Y");
        }
        open_transaction_menu(g, ui, rng, env);
    } else if g.cur_analyst_rating == 4 || g.cur_analyst_rating == 5 {
        if g.active_entity == g.sel_entity && corp_id != g.sel_entity {
            let player_name = g.cur_player_name.clone();
            if 0 < find_stock_holding(g, g.cur_player, corp_id) {
                msg_text = s("You (") + &player_name + __content.string(6192) + &g.ent_name[corp_id] + ".";
            } else {
                msg_text = s("You (") + &player_name + __content.string(6193) + &g.ent_name[corp_id];
                if g.cur_player == g.corp_controller[corp_id] {
                    msg_text = msg_text
                        + __content.string(6194)
                        + &g.ent_name[corp_id]
                        + ", "
                        + __content.string(6195);
                }
                update_corp_control(g, ui, rng, corp_id);
                if cancel_no_holder(g) {
                    show_quick_message(g, ui, &mut (msg_text + "\r\n" + "\r\n" + __content.string(6196)));
                    return report_dlg_rating_click_exit(true);
                }
                let lh = cast::<i32, _>(g.largest_holder);
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(2961)
                    + &g.ent_name[lh]
                    + __content.string(6197)
                    + __content.string(6198)
                    + &g.ent_name[corp_id]
                    + ".";
            }
            msg_text =
                msg_text + "\r" + "\r" + __content.string(6199) + &g.ent_name[corp_id] + "?";
            if 10 < g.largest_holder {
                let with = g.ent_name[cast::<i32, _>(g.largest_holder)].clone() + __content.string(6200);
                msg_text = replace(msg_text.as_bytes(), __content.bytes(6201), with.as_bytes());
            }
            if !report_dlg_rating_click_confirm_trade(g, ui, rng, now, corp_id, &msg_text) {
                return report_dlg_rating_click_exit(true);
            }
        } else if g.active_entity == g.sel_entity && corp_id == g.sel_entity {
            if 0 < find_stock_holding(g, g.cur_player, g.sel_entity) {
                msg_text = s("You, (")
                    + &g.cur_player_name
                    + __content.string(6202)
                    + &g.sel_corp_name
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(6199)
                    + &g.sel_corp_name
                    + "?";
                if !report_dlg_rating_click_confirm_trade(g, ui, rng, now, corp_id, &msg_text) {
                    return report_dlg_rating_click_exit(true);
                }
            } else if g.cur_player == g.corp_controller[g.sel_entity] {
                msg_text = s("You, ")
                    + &g.cur_player_name
                    + __content.string(6203)
                    + &g.sel_corp_name
                    + " "
                    + __content.string(6204)
                    + &g.ent_name[corp_id]
                    + ", "
                    + __content.string(6195);
                update_corp_control(g, ui, rng, corp_id);
                if cancel_no_holder(g) {
                    show_quick_message(g, ui, &mut (msg_text + "\r\n" + "\r\n" + __content.string(6205)));
                    return report_dlg_rating_click_exit(true);
                }
                msg_text = msg_text + &however_sell(g, corp_id);
                if !report_dlg_rating_click_confirm_trade(g, ui, rng, now, corp_id, &msg_text) {
                    return report_dlg_rating_click_exit(true);
                }
            }
        } else if find_stock_holding(g, g.active_entity, g.sel_entity) == 0 {
            msg_text = g.active_entity_name.clone() + __content.string(6206) + &g.sel_corp_name + ".";
            update_corp_control(g, ui, rng, corp_id);
            if cancel_no_holder(g) {
                show_quick_message(g, ui, &mut (msg_text + "\r\n" + "\r\n" + __content.string(6196)));
                return report_dlg_rating_click_exit(true);
            }
            msg_text = msg_text + &however_sell(g, corp_id);
            if !report_dlg_rating_click_confirm_trade(g, ui, rng, now, corp_id, &msg_text) {
                return report_dlg_rating_click_exit(true);
            }
        }
        g.action_menu_mode = 4;
        if g.active_entity < 11 && 0 < find_stock_holding(g, g.active_entity, corp_id) {
            g.skip_margin_prompt = s("Y");
        }
        open_transaction_menu(g, ui, rng, env);
    }
    report_dlg_rating_click_exit(false)
}

/// `LargestHolder < 11 OR CurPlayer <> CorpController(LargestHolder)`: no major shareholder the player controls.
fn cancel_no_holder(g: &Globals) -> bool {
    g.largest_holder < 11 || g.cur_player != g.corp_controller[cast::<i32, _>(g.largest_holder)]
}

/// `"\r\rHowever, <LargestHolder> [runtime resource wording] major shareholder of <corp>.\r\rDo you
/// want <LargestHolder> [runtime resource wording] its stock of <corp>?"`.
fn however_sell(g: &Globals, corp_id: i32) -> PbStr {
    let __content = crate::content::current();
    let lh = cast::<i32, _>(g.largest_holder);
    s("\r")
        + "\r"
        + __content.string(2961)
        + &g.ent_name[lh]
        + __content.string(6207)
        + __content.string(6208)
        + &g.ent_name[corp_id]
        + "."
        + "\r"
        + "\r"
        + __content.string(1936)
        + &g.ent_name[lh]
        + __content.string(6209)
        + __content.string(6210)
        + &g.ent_name[corp_id]
        + "?"
}

/// `CloseAllDialogs 1: DIALOG SHOW STATE hDlgMain, 0: DoTransactionMenuAction 1: DIALOG SHOW STATE hDlgMain, 5`.
fn open_transaction_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    close_all_dialogs(g, ui, rng, 1);
    // (UI) DIALOG SHOW STATE hDlgMain, 0 (SW_HIDE)
    if !env.skips(0x552f0b) {
        do_transaction_menu_action(g, ui, rng, env, 1);
    }
    // (UI) DIALOG SHOW STATE hDlgMain, 5 (SW_SHOW)
}

/// Port of ReportDlg_RatingClick_ConfirmTrade (0x41954b), GOSUB of [`report_dlg_rating_click`]: asks
/// `msg_text` (YES/NO/CANCEL, title "<corp> -- POTENTIAL STOCK PURCHASE/SALE"). Anything but YES ends
/// the dialog and the routine: returns `false`. YES selects the acting entity (`SelectActingEntity 1`)
/// or, with a major shareholder (`LargestHolder > 10`), makes it the target corp ([`select_target_corp`]).
fn report_dlg_rating_click_confirm_trade(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    now: &LocalTime,
    corp_id: i32,
    msg_text: &PbStr,
) -> bool {
    let __content = crate::content::current();
    let mut trade_kind = s("SALE");
    if 0 < instr(1, msg_text.as_bytes(), b"buy") {
        trade_kind = s("PURCHASE");
    }
    let title = g.ent_name[corp_id].clone() + __content.string(6211) + &trade_kind;
    let answer = ui.msgbox(msg_text, 8483, &title);
    if answer != 6 {
        // (UI) DIALOG END hDlg, 0
        return false;
    }
    if g.largest_holder < 11 {
        select_acting_entity(g, ui, rng, 1, now);
    } else {
        g.target_corp = cast::<i32, _>(g.largest_holder);
        select_target_corp(g, ui, rng, now.time_secs());
    }
    true
}

/// Port of ReportDlg_RatingClick_Exit (0x41964e): the routine's exit label; the callback's result is
/// always 0. Passes the port's "dialog ended" flag through.
fn report_dlg_rating_click_exit(dialog_ended: bool) -> bool {
    dialog_ended
}
