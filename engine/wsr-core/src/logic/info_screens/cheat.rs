//! The cheat menu: `CheatMenu` (0x67383a) with its dialog `DlgCheatMenu` (0x674441) and the GO button
//! `DlgCheatMenu_Go` (0x67515d), the add/reduce cash cheat, the insider merger tip
//! `CheatInsiderMergerTip` (0x677230) and the antitrust cheat `CheatDisableAntitrust` (0x680954).
//!
//! The dialog is a DDT dialog with four option buttons and one button (CLOSE, renamed GO once an option
//! is picked). The port shows it through [`Ui::select`]: the four option labels in screen order
//! (add/subtract cash, merger tip, earnings tip, antitrust); picking one is the option callback
//! (`TmpExt1` = 1..4) followed by the button, cancelling is the button without an option (CLOSE).

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::collapsible_if, clippy::collapsible_else_if, clippy::if_same_then_else, clippy::neg_cmp_op_on_partial_ord)]
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use crate::logic::sim_entity::prelude::*;
use crate::logic::control::update_corp_control;
use crate::logic::economy::close_all_dialogs;
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::info_screens::insider::cheat_insider_earnings_tip;
use crate::logic::market::{select_target_corp, update_market_display};
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::quarter_start::clear_insider_tip;
use crate::logic::valuation::{
    calc_analyst_rating, calc_stock_price, price_pct_change_months, price_trend_direction, price_trend_streak,
};
use crate::pb::string::remove_any;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `ShowQuickMessage(text)` on a temporary.
fn quick(g: &mut Globals, ui: &mut dyn Ui, text: PbStr) {
    let mut t = text;
    show_quick_message(g, ui, &mut t);
}

/// `t = REPLACE$(t, a, b)`.
fn rep(t: &mut PbStr, a: &str, b: &[u8]) {
    *t = replace(t.as_bytes(), a.as_bytes(), b);
}

/// MSGBOX style `MB_OK | MB_ICONINFORMATION | MB_TASKMODAL`.
const MB_INFO: i32 = 8256;
/// MSGBOX style `MB_YESNOCANCEL | MB_ICONQUESTION | MB_TASKMODAL`.
const MB_YNC: i32 = 8227;

// ------------------------------------------------------------------------------------------- CheatMenu

/// Port of CheatMenu (0x67383a).
/// The cheat menu of the current player (only with Cheat Mode on, a game in progress that has not
/// ended, and an active current player; otherwise a pop-up says why it is not available). Runs the
/// dialog ([`dlg_cheat_menu`]); its GO button runs the insider tips itself and leaves `TmpExt1` = 1 for
/// the cash cheat or 4 for the antitrust cheat ([`cheat_disable_antitrust`]), which run here.
///
/// Cash cheat: with `PlayerCheatCode < 6` (not used yet) a rules box and an INPUTBOX for an amount up
/// to `ExchangeRate * 1,000,000` (more re-asks; 0 cancels): a positive amount needs a YES to the
/// disqualification warning and sets `PlayerCheatCode = 7 + RandSeed MOD 5`; a negative one may not
/// make the cash negative. Afterwards (code >= 6) only reductions (up to the cash) are offered. The
/// amount is added to `Cash(CurPlayer)` (|cash| < 0.505 becomes 0), then `UpdateMarketDisplay`. In long
/// games (`LongGameMarker > 5`, less than 36 years) `PlayerCheatCode` is always set first, so only
/// reductions are possible.
pub fn cheat_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut msg_text = PbStr::new();
    let mut cash_delta: i64;
    if !(g.cheat_mode == "N") && !(g.game_end_flag == "Y") && g.game_in_progress != 0 {
        let cp = g.cur_player;
        if cp == 0 || g.entity_status[cp] != 1 {
            return;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        dlg_cheat_menu(g, ui, rng, time_secs, 0);
        if g.tmp_ext1 == Ext::from_f64(4.0) {
            g.tmp_ext1 = Ext::from_f64(0.0);
            cheat_disable_antitrust(g, ui);
            return;
        } else if g.tmp_ext1 != Ext::from_f64(1.0) {
            g.tmp_ext1 = Ext::from_f64(0.0);
            return;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        if 5 < g.long_game_marker && g.game_end_year - g.start_year < 36 {
            g.player_cheat_code[cp] = g.rand_seed % 5 + 7;
        }
        let limit_text: PbStr;
        if g.player_cheat_code[cp] < 6 {
            limit_text = fmt_trim(g.exchange_rate * Ext::from_f64(1000000.0), b"* ###,") + " ";
            msg_text = s(__content.string(5728))
                + "\r"
                + "\r"
                + __content.string(5729)
                + __content.string(5730)
                + &trim(g.currency_symbol.as_bytes())
                + &limit_text
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + __content.string(5731)
                + "\r"
                + "\r"
                + __content.string(5732)
                + __content.string(5733)
                + "\r"
                + "\r"
                + __content.string(5734)
                + __content.string(5735)
                + "\r"
                + "\r"
                + __content.string(5736);
            ui.msgbox(&msg_text, MB_INFO, &s(__content.string(5737)));
            msg_text = s(__content.string(5738)) + &g.cur_player_name + ". ";
            let corp_possessive = g.sel_corp_name.clone() + "'s";
            if g.cur_player != g.sel_entity {
                msg_text = msg_text + __content.string(5739) + &corp_possessive + ". ";
            }
            msg_text = msg_text + __content.string(5740);
            loop {
                let default = limit_text.clone() + &g.million_prefix_lc + "illion" + &g.currency_suffix;
                let input_text = ui.inputbox(&msg_text, &s(__content.string(5741)), &default);
                cash_delta = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
                if cash_delta == 0 {
                    return;
                }
                if cash_delta <= 0 {
                    if g.cash[cp] + cast::<Ext, _>(cash_delta) + Ext::from_f64(1.0) < Ext::from_f64(0.0) {
                        quick(g, ui, s(__content.string(5742)));
                        return;
                    }
                } else if cast::<Ext, _>(cash_delta) - Ext::from_f64(0.5) <= g.exchange_rate * Ext::from_f64(1000000.0) {
                    let confirm_msg = s(__content.string(5743))
                        + __content.string(5744)
                        + "\r"
                        + "\r"
                        + __content.string(5745);
                    let answer = ui.msgbox(&confirm_msg, MB_YNC, &s(__content.string(5746)));
                    if answer == 2 || answer == 7 {
                        return;
                    }
                    g.player_cheat_code[cp] = g.rand_seed % 5 + 7;
                } else {
                    cash_delta = 0;
                }
                if cash_delta != 0 {
                    break;
                }
            }
            msg_text = s(__content.string(5747));
            if cash_delta < 0 {
                msg_text = PbStr::new();
            }
        } else {
            if Ext::from_f64(0.5) <= g.cash[cp] {
                msg_text = s(__content.string(5748))
                    + "\r"
                    + "\r"
                    + __content.string(5749)
                    + &g.cur_player_name
                    + "\r"
                    + "\r"
                    + __content.string(5750)
                    + __content.string(5751)
                    + "\r"
                    + "\r"
                    + __content.string(5752)
                    + __content.string(5753)
                    + "\r"
                    + "\r"
                    + __content.string(5754);
                let answer = ui.msgbox(&msg_text, MB_YNC, &s(__content.string(5755)));
                if answer == 2 || answer == 7 {
                    return;
                }
                loop {
                    let lt = trim(g.currency_symbol.as_bytes())
                        + &fmt_trim(g.cash[cp], b"* ###,")
                        + " "
                        + &g.million_prefix_lc
                        + "il."
                        + &g.currency_suffix;
                    msg_text = s(__content.string(5756))
                        + &g.cur_player_name
                        + __content.string(5757)
                        + &lt
                        + ")";
                    let input_text = ui.inputbox(&msg_text, &s(__content.string(5758)), &lt);
                    cash_delta = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
                    if cash_delta == 0 {
                        return;
                    }
                    if (cast::<Ext, _>(cash_delta)).abs() <= g.cash[cp] + Ext::from_f64(1.0) {
                        if 0 < cash_delta {
                            cash_delta = to_quad(cast::<Ext, _>(cash_delta) * -Ext::from_f64(1.0));
                        }
                        msg_text = PbStr::new();
                        break;
                    }
                    quick(g, ui, s(__content.string(5759)) + &lt + __content.string(5760));
                }
            } else {
                msg_text = s(__content.string(5761)) + "\r\n" + "\r\n" + __content.string(5762);
                if sf(0.005f32) < g.cash[cp] {
                    rep(&mut msg_text, "no cash", __content.bytes(5763));
                }
                show_quick_message(g, ui, &mut msg_text);
                return;
            }
        }
        // LAB_00674358
        if cash_delta != 0 {
            g.cash[cp] = g.cash[cp] + cast::<Ext, _>(cash_delta);
            if g.cash[cp].abs() < sf(0.505f32) {
                g.cash[cp] = Ext::from_f64(0.0);
            }
            update_market_display(g, ui, rng, time_secs);
            quick(g, ui, s(__content.string(3443)) + &msg_text);
        }
        return;
    }
    if g.cheat_mode == "N" {
        msg_text = s(__content.string(5764))
            + __content.string(5765);
    }
    if g.game_end_flag == "Y" {
        msg_text = s(__content.string(5766));
    }
    if g.game_in_progress == 0 {
        msg_text = s(__content.string(5767));
    }
    quick(g, ui, s(__content.string(5768)) + &msg_text);
}

// ------------------------------------------------------------------------------------------ the dialog

/// The four option labels of the cheat dialog in screen order (add/subtract cash, merger tip, earnings
/// tip, antitrust), as [`dlg_cheat_menu`] builds them (`PlayerCheatCode` as it is after the dialog's
/// long-game draw). Pure; for UIs that want to show the menu themselves.
pub fn cheat_menu_options(g: &Globals) -> [PbStr; 4] {
    let __content = crate::content::current();
    let cp = g.cur_player;
    let mut opt4 = s(__content.string(5769));
    if g.private_antitrust_off == 1 && g.govt_antitrust_off == 0 {
        opt4 = s(__content.string(5770));
    }
    if g.private_antitrust_off == 0 && g.govt_antitrust_off == 1 {
        opt4 = s(__content.string(5771));
    }
    opt4 = s(__content.string(5772)) + &opt4;
    let mut opt2 = s(__content.string(5773));
    if cast::<i32, _>(g.insider_tippee) == cp && 10 < g.merger_tip_corp {
        opt2 = s(__content.string(5774)) + &g.ent_symbol[cast::<i32, _>(g.merger_tip_corp)] + " Merger";
    } else if 0 < g.insider_tippee {
        if g.tipster_state[cp] != -1 {
            opt2 = s(__content.string(5775));
        }
    }
    let mut opt3 = s(__content.string(5776));
    if cast::<i32, _>(g.insider_tippee) != cp || (cast::<i32, _>(g.insider_earn_tip_corp)).abs() <= 10 {
        if 0 < g.insider_tippee {
            if g.tipster_state[cp] != -1 {
                opt3 = s(__content.string(5777));
            }
        }
    } else {
        let tip_corp_id = (cast::<i32, _>(g.insider_earn_tip_corp)).abs();
        opt3 = s(__content.string(5778)) + &g.ent_symbol[tip_corp_id];
    }
    let mut opt1 = s(__content.string(5779));
    if 5 < g.player_cheat_code[cp] {
        opt1 = s(__content.string(5780));
    }
    [opt1 + __content.string(5781), opt2, opt3, opt4]
}

/// Port of DlgCheatMenu (0x674441).
/// Builds and shows the modal cheat dialog "CHEAT MENU (For <player>)" (`CloseAllDialogs 1` first):
/// option labels depend on the antitrust cheats, the current insider tip and `PlayerCheatCode`, which
/// in long games (`LongGameMarker > 5`, less than 36 years) is set to `ROUND(RND * 5 + 7)` here.
/// `TmpExt1 = 0`, `DlgOpen_CheatMenu = 1`; the choice goes through [`Ui::select`]: a picked option is
/// its callback ([`dlg_cheat_menu_opt`]) followed by the button ([`dlg_cheat_menu_go`]); cancel is the
/// button without an option.
///
/// Port of DlgProcCheatMenuColors (0x674a8c): the dialog procedure only sets colours and the tooltip
/// of the button (UI, no state effects).
pub fn dlg_cheat_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32, _h_parent: i32) {
    let __content = crate::content::current();
    close_all_dialogs(g, ui, rng, 1);
    let title = s(__content.string(5782)) + &g.cur_player_name + ")";
    // the option labels before the draw only differ in the add-cash line, built after it
    if 5 < g.long_game_marker && g.game_end_year - g.start_year < 36 {
        let cp = g.cur_player;
        g.player_cheat_code[cp] = cast::<i32, _>(to_quad(rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(7.0)));
    }
    let options = cheat_menu_options(g);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.dlg_open_cheat_menu = 1;
    // DIALOG SHOW MODAL hCheatDlg CALL DlgProcCheatMenuColors
    if let Some(k) = ui.select(&title, &[s(__content.string(5783))], &options, None, &s("Go")) {
        dlg_cheat_menu_opt(g, cast::<i32, _>(k) + 1);
    }
    dlg_cheat_menu_go(g, ui, rng, time_secs);
}

/// Port of DlgCheatMenu_Opt1 (0x674fa9): the add/subtract cash option (`TmpExt1 = 1`).
/// Port of DlgCheatMenu_Opt2 (0x675010): the merger tip option (`TmpExt1 = 2`).
/// Port of DlgCheatMenu_Opt3 (0x67507b): the earnings tip option (`TmpExt1 = 3`).
/// Port of DlgCheatMenu_Opt4 (0x6750e6): the antitrust option (`TmpExt1 = 4`).
/// Each also renames the button from CLOSE to GO (UI).
pub fn dlg_cheat_menu_opt(g: &mut Globals, option: i32) {
    g.tmp_ext1 = cast::<Ext, _>(option);
}

/// Port of DlgCheatMenu_Go (0x67515d).
/// The dialog's button: ends the dialog (`DlgOpen_CheatMenu = 0`), recomputes the current player's
/// financials and handles the insider tip options (`TmpExt1` 2 = merger, 3 = earnings; 1 and 4 are
/// left in `TmpExt1` for [`cheat_menu`]). Tips need a net worth of at least 50 (million, times the
/// exchange rate) or an established tipster, and are not given in the game's last two years. A new
/// tip goes to [`cheat_insider_merger_tip`] / `CheatInsiderEarningsTip`; while a tip is running the
/// tipster's stage (`TipsterState` 1..3) picks an update message, and stage 4 / -1 (arrested tipster)
/// a warning that may start or advance an insider trading probe (`InsiderProbePlayer`, headlines,
/// `TipsterState = -1`). An earnings tip whose company was liquidated or taken over by someone else
/// ends the caper (`ClearInsiderTip`).
pub fn dlg_cheat_menu_go(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let mut choice = 0;
    if Ext::from_f64(0.0) < g.tmp_ext1 {
        choice = to_long(g.tmp_ext1);
    }
    g.dlg_open_cheat_menu = 0;
    // DIALOG END hDlg, 0
    let cp = g.cur_player;
    calc_entity_financials(g, ui, rng, cp);
    let mut msg_text: PbStr;
    if g.net_worth < g.exchange_rate * Ext::from_f64(50.0)
        && g.exchange_rate * Ext::from_f64(1.0) < g.net_worth
        && g.tipster_state[cp] == 0
        && 1 < choice
        && choice < 4
    {
        msg_text = s(__content.string(5784))
            + __content.string(5785)
            + __content.string(5786)
            + __content.string(5787)
            + __content.string(5788)
            + "\r"
            + "\r"
            + __content.string(5789)
            + __content.string(5790)
            + __content.string(5791);
        if choice == 2 {
            rep(&mut msg_text, __content.string(5792), __content.bytes(5793));
        }
        ui.msgbox(&msg_text, MB_INFO, &s(__content.string(5794)));
        return;
    }
    if 1 < choice && g.game_end_year - 2 <= g.cur_year {
        if g.tipster_state[cp] == 0 {
            msg_text = s(__content.string(5795));
        } else {
            msg_text = s(__content.string(5796))
                + __content.string(5797)
                + __content.string(5798)
                + __content.string(5799);
        }
        ui.msgbox(&msg_text, MB_INFO, &s(__content.string(5800)));
        g.tmp_ext1 = Ext::from_f64(0.0);
        return;
    }
    let tippee = cast::<i32, _>(g.insider_tippee);
    let corp_id: i32;
    let tip_stage: i32;
    if choice == 2 {
        corp_id = cast::<i32, _>(g.merger_tip_corp);
        if 0 < tippee
            && (tippee != cp || 10 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs())
            && g.tipster_state[cp] != -1
        {
            msg_text = s(__content.string(5801));
            if tippee == cp && 10 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs() {
                let tip_corp_id = (cast::<i32, _>(g.insider_earn_tip_corp)).abs();
                msg_text = msg_text
                    + __content.string(5802)
                    + __content.string(5803)
                    + &g.ent_name[tip_corp_id]
                    + __content.string(5804)
                    + __content.string(5805);
                if g.insider_earn_tip_corp < -10 {
                    rep(&mut msg_text, __content.string(5806), __content.bytes(2549));
                }
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    rep(&mut msg_text, __content.string(5807), __content.bytes(5808));
                }
            }
            ui.msgbox(&msg_text, MB_INFO, &s(__content.string(5809)));
            g.tmp_ext1 = Ext::from_f64(0.0);
            return;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        if g.tipster_state[cp] < 4 && g.tipster_state[cp] != -1 {
            if tippee != cp || g.tipster_state[cp] == 0 {
                cheat_insider_merger_tip(g, ui, rng, time_secs);
                return;
            }
            tip_stage = cast::<i32, _>(g.tipster_state[cp]);
        } else {
            tip_stage = 4;
        }
    } else if choice == 3 {
        if 0 < tippee && (tippee != cp || 10 < g.merger_tip_corp) && g.tipster_state[cp] != -1 {
            msg_text = s(__content.string(5810));
            if tippee == cp {
                msg_text = msg_text
                    + __content.string(5811)
                    + __content.string(5812)
                    + &g.ent_name[cast::<i32, _>(g.merger_tip_corp)]
                    + __content.string(5813);
            }
            ui.msgbox(&msg_text, MB_INFO, &s(__content.string(5814)));
            g.tmp_ext1 = Ext::from_f64(0.0);
            return;
        }
        corp_id = (cast::<i32, _>(g.insider_earn_tip_corp)).abs();
        if 10 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs() {
            let mut deal_active = 1;
            let mut fate = PbStr::new();
            update_corp_control(g, ui, rng, corp_id);
            if g.entity_status[corp_id] == 1 && g.public_pct == 0 {
                if cast::<i32, _>(g.insider_tippee) != g.corp_controller[corp_id] && 0 < g.wholly_owned_by {
                    if 0 < get_controller_type(g, corp_id) {
                        deal_active = 0;
                    }
                }
                fate = s(__content.string(5815)) + &g.ent_name[cast::<i32, _>(g.wholly_owned_by)] + __content.string(5816);
            } else if g.entity_status[corp_id] != 1 {
                deal_active = 0;
                fate = s(__content.string(5817));
            }
            if deal_active == 0 {
                let aux_text = g.ent_name[corp_id].clone()
                    + " ("
                    + &g.ent_symbol[corp_id]
                    + ") "
                    + &fate
                    + __content.string(5818);
                ui.msgbox(&aux_text, MB_INFO, &s(__content.string(5819)));
                clear_insider_tip(g);
                g.tmp_ext1 = Ext::from_f64(0.0);
                return;
            }
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        if g.tipster_state[cp] < 4 && g.tipster_state[cp] != -1 {
            if cast::<i32, _>(g.insider_tippee) != cp || g.tipster_state[cp] == 0 {
                cheat_insider_earnings_tip(g, ui, rng, time_secs);
                return;
            }
            tip_stage = cast::<i32, _>(g.tipster_state[cp]);
        } else {
            tip_stage = 4;
        }
    } else {
        // 1 (cash) and 4 (antitrust) are run by CheatMenu; 0: CLOSE without an option
        return;
    }

    let mut has_stake = 0;
    if 0 < calc_group_stake_pct(g, cp, corp_id) || Ext::from_f64(0.0) < g.tmp_ext1 {
        has_stake = 1;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    let mut msg_title = s(__content.string(5820));
    let iet = cast::<i32, _>(g.insider_earn_tip_corp);
    let mtc = cast::<i32, _>(g.merger_tip_corp);
    msg_text = PbStr::new();
    match tip_stage {
        4 => {
            msg_text = s(__content.string(5821))
                + __content.string(5822)
                + __content.string(5823)
                + __content.string(5824)
                + __content.string(5825)
                + __content.string(5826)
                + "\r"
                + "\r";
            if g.tipster_state[cp] == 4 && cp != g.insider_probe_player {
                msg_text = s(__content.string(5827))
                    + __content.string(5828)
                    + __content.string(5829)
                    + __content.string(5830)
                    + "\r"
                    + "\r";
            }
            if cp == g.insider_probe_player && g.tipster_state[cp] == -1 {
                let mut aux_text = PbStr::new();
                let cc = g.currency_code.clone();
                if cc == "USA" || cc == "UK" {
                    aux_text = s("the ") + &cc;
                }
                for (code, country) in [
                    ("AUS", __content.string(4564)),
                    ("CAN", "Canada"),
                    ("NZ", __content.string(5831)),
                    ("SAF", __content.string(5832)),
                    ("DAN", "Denmark"),
                    ("SWE", "Sweden"),
                    ("SWI", __content.string(5833)),
                    ("NOR", "Norway"),
                    ("HK", __content.string(1294)),
                    ("SNG", __content.string(5834)),
                ] {
                    if cc == code {
                        aux_text = s(country);
                    }
                }
                if !(aux_text == "") {
                    aux_text = s(" with ") + &aux_text;
                }
                if (g.cur_year + g.cur_quarter) % 2 == 1 {
                    msg_text = msg_text
                        + __content.string(5835)
                        + __content.string(5836)
                        + __content.string(5837);
                } else {
                    aux_text = s(" Feds");
                    if !(g.currency_code == "USA") {
                        aux_text = s(__content.string(5838));
                    }
                    msg_text = msg_text
                        + __content.string(5839)
                        + __content.string(5840)
                        + __content.string(5841);
                }
                msg_text = msg_text + &aux_text + ".)";
            } else if g.tipster_state[cp] == -1 {
                msg_text = msg_text + __content.string(5842);
            }
            if cast::<i32, _>(g.insider_tippee) == cp || g.tipster_state[cp] != 4 {
                if cast::<i32, _>(g.insider_tippee) == cp && g.tipster_state[cp] == 4 {
                    let mut aux_text = s(__content.string(5843));
                    if g.currency_code == "USA" {
                        aux_text = s(__content.string(5844));
                    }
                    msg_text = s(__content.string(5845))
                        + __content.string(5846)
                        + "\r"
                        + "\r";
                    if 10 < iet.abs() {
                        rep(&mut msg_text, __content.string(5847), __content.bytes(5848));
                    }
                    if iet < -10 {
                        rep(&mut msg_text, "good", b"bad");
                    }
                    msg_text = msg_text
                        + __content.string(5849)
                        + &g.ent_name[corp_id]
                        + __content.string(5850)
                        + __content.string(5851)
                        + &aux_text
                        + __content.string(5852)
                        + __content.string(5853)
                        + __content.string(5854);
                }
            } else {
                let mut aux_text = s(__content.string(1550));
                if g.currency_code == "USA" {
                    aux_text = s(__content.string(5855));
                }
                msg_text = msg_text
                    + __content.string(5856)
                    + __content.string(5857)
                    + &aux_text
                    + __content.string(5858)
                    + __content.string(5859)
                    + __content.string(5860);
                if rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0) || g.insider_probe_player != 0 {
                    if cp == g.insider_probe_player || g.insider_probe_player == 0 {
                        if g.insider_probe_player == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                            g.news_msg = g.cur_player_name.clone() + __content.string(5861);
                            quick(g, ui,
                                s(__content.string(5862))
                                    + &g.news_msg
                                    + __content.string(5863)
                                    + &g.cur_player_name
                                    + __content.string(5864)
                                    + __content.string(5865),
                            );
                            post_news(g, ui, 0, 0);
                            g.tipster_state[cp] = -1;
                            return;
                        }
                        g.news_msg =
                            s(__content.string(5866)) + &g.cur_player_name + __content.string(5867);
                        if cp == g.insider_probe_player {
                            g.tipster_state[cp] = -1;
                        }
                    } else if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
                        g.news_msg = s(__content.string(5868)) + &g.cur_player_name + __content.string(5869);
                    }
                } else {
                    g.insider_probe_player = cp;
                    g.news_msg = s(__content.string(5870)) + &g.cur_player_name + __content.string(5871);
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        let mut n = g.news_msg.clone();
                        rep(&mut n, __content.string(5872), __content.bytes(5873));
                        g.news_msg = n;
                    }
                }
                if cp == g.insider_probe_player || g.insider_probe_player == 0 {
                    g.tipster_state[cp] = -1;
                }
            }
            msg_title = s(__content.string(5874));
        }
        3 => {
            let mut aux_text = s(__content.string(5875));
            if 2 < g.cur_quarter {
                aux_text = s(__content.string(5876));
            }
            msg_text = s(__content.string(5877))
                + "the "
                + &g.ent_name[corp_id]
                + __content.string(5878)
                + __content.string(5879);
            if 10 < mtc && mtc != g.merger_target {
                msg_text = msg_text + __content.string(5880);
            }
            msg_text = msg_text + ". ";
            if (g.cur_year + g.cur_quarter) % 2 == 1 {
                rep(&mut msg_text, __content.string(5881), __content.bytes(5882));
            }
            if 10 < iet.abs() {
                rep(&mut msg_text, __content.string(5883), __content.bytes(5884));
            }
            if mtc < 11 || mtc != g.merger_target || g.merger_acquirer < 11 {
                if 10 < iet.abs() {
                    rep(&mut msg_text, __content.string(5885), __content.bytes(5886));
                }
            } else {
                let with = s(__content.string(5887)) + &g.ent_name[g.merger_acquirer] + __content.string(5888);
                rep(&mut msg_text, __content.string(5885), with.as_bytes());
            }
            if iet < -10 {
                rep(&mut msg_text, "roof.", b"floor.");
                rep(&mut msg_text, __content.string(5889), __content.bytes(5890));
            }
            msg_text = msg_text
                + __content.string(5891)
                + __content.string(5892)
                + &aux_text;
        }
        2 => {
            msg_text = s(__content.string(5893))
                + __content.string(5894)
                + &g.ent_name[corp_id]
                + ". "
                + __content.string(5895);
            if iet < 11 && mtc < 11 {
                if iet < -10 {
                    rep(&mut msg_text, "buying", __content.bytes(5896));
                    if calc_group_stake_pct(g, cp, -corp_id) < 0 || g.tmp_ext1 < Ext::from_f64(0.0) {
                        msg_text = msg_text + __content.string(5897);
                    }
                    g.tmp_ext1 = Ext::from_f64(0.0);
                }
            } else if has_stake < 1 {
                if 10 < price_pct_change_months(g, corp_id, 3) {
                    if price_trend_direction(g, corp_id, 3, 1) == 1 {
                        msg_text = msg_text
                            + __content.string(5898)
                            + __content.string(5899)
                            + __content.string(5900);
                    }
                }
            } else {
                rep(&mut msg_text, "buying", b"holding");
            }
            if mtc == g.merger_target && 10 < mtc {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(5901)
                    + __content.string(5902)
                    + &g.ent_name[corp_id]
                    + __content.string(5903)
                    + __content.string(5904);
            }
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(5905)
                + __content.string(5906)
                + __content.string(5907);
        }
        1 => {
            msg_text = s(__content.string(5908))
                + __content.string(5909)
                + &g.ent_name[corp_id]
                + __content.string(5910)
                + __content.string(5911);
            if iet.abs() <= 10 {
                if 10 < mtc {
                    if has_stake == 0 {
                        rep(&mut msg_text, __content.string(5912), __content.bytes(5913));
                    }
                    if mtc == g.merger_target && 10 < g.merger_acquirer {
                        msg_text = msg_text
                            + __content.string(5914)
                            + &g.ent_symbol[corp_id]
                            + __content.string(5915)
                            + __content.string(5916)
                            + &g.ent_name[g.merger_acquirer]
                            + ". ";
                    }
                }
            } else {
                if 10 < iet && has_stake == 0 {
                    rep(&mut msg_text, __content.string(5917), b"acquire");
                    if 12 < price_pct_change_months(g, corp_id, 3) {
                        if price_trend_direction(g, corp_id, 3, 1) == 1 {
                            msg_text = msg_text
                                + __content.string(5918);
                        }
                    }
                }
                rep(&mut msg_text, __content.string(5919), __content.bytes(5920));
                if 10 < iet.abs() && cast::<i32, _>(g.insider_tip_event_tick) == g.cur_quarter {
                    let pqe = g.proj_qtr_earnings[corp_id];
                    if pqe < Ext::from_f64(0.0) || pqe < g.earnings_y1[corp_id] * sf(0.25f32) {
                        msg_text = msg_text
                            + __content.string(5921)
                            + __content.string(5922);
                        if iet < 1 {
                            msg_text = msg_text + __content.string(5923);
                        } else {
                            msg_text = msg_text + __content.string(5924);
                        }
                        msg_text = msg_text + __content.string(5925);
                    }
                }
            }
            if iet < -10 {
                rep(&mut msg_text, __content.string(5917), __content.bytes(5926));
                if calc_group_stake_pct(g, cp, -corp_id) == 0 && g.tmp_ext1 == Ext::from_f64(0.0) {
                    rep(&mut msg_text, __content.string(5926), __content.bytes(5927));
                }
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
        }
        _ => {}
    }
    if !(msg_text == "") {
        ui.msgbox(&msg_text, MB_INFO, &msg_title);
    }
    if !(g.news_msg == "") {
        post_news(g, ui, cp, 0);
    }
}

// ------------------------------------------------------------------------------- CheatInsiderMergerTip

/// Port of CheatInsiderMergerTip (0x677230).
/// A tipster offers the name of an upcoming merger target for `TipsterState + 1` (million): up to 1000
/// random tries for an active non-ETF company (no profit margin below -3 for industries < 70) with a
/// market cap between `minCap` (0.5% of the player's net worth, clamped to 50..500 times the exchange
/// rate) and 15 times that (doubled from try 500), analyst rating <= 4, at least 95% public and a voting
/// controller. On YES: the fee is paid, a stock priced below 15 gets its share count halved (reverse
/// split, `SharesOutstanding * 0.5`), the company becomes `MergerTipCorp` with
/// `InsiderTippee`, margin +5 / no discretionary spending, [runtime resource wording] is released, cap
/// -3%, notoriety +1, the event tick is scheduled, a denial headline is posted, the tipster's stage
/// advances and an insider probe may start. Then the target's name and price are revealed and it
/// becomes the selected company. With a tip already running, a pending merger, or no candidate: a
/// "nothing for you now" message.
pub fn cheat_insider_merger_tip(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    let cp = g.cur_player;
    calc_entity_financials(g, ui, rng, cp);
    let mut min_cap = g.net_worth * sf(0.005f32);
    if min_cap < g.exchange_rate * Ext::from_f64(50.0) {
        min_cap = g.exchange_rate * Ext::from_f64(50.0);
    }
    if g.exchange_rate * Ext::from_f64(500.0) < min_cap {
        min_cap = g.exchange_rate * Ext::from_f64(500.0);
    }
    let mut max_cap = min_cap * Ext::from_f64(15.0);
    if 0 < g.merger_tip_corp && g.insider_tippee == 0 {
        clear_insider_tip(g);
    }
    if 0 < g.insider_tippee && g.merger_tip_corp == 0 && g.insider_earn_tip_corp == 0 {
        clear_insider_tip(g);
    }
    let mut target_id = 0;
    let mut msg_text: PbStr;
    let mut tries = 1;
    'tries: loop {
        if 10 < g.merger_tip_corp || 10 < (cast::<i32, _>(g.insider_earn_tip_corp)).abs() || 10 < g.merger_target {
            break;
        }
        if tries == 500 {
            max_cap = max_cap * Ext::from_f64(2.0);
        }
        let cand_id = rng.rnd_range(1600, 11);
        'cand: {
            if g.entity_status[cand_id] != 1 || g.industry_num[cand_id] == 71 {
                break 'cand;
            }
            if g.industry_num[cand_id] < 70 && g.profit_margin[cand_id] < -3 {
                break 'cand;
            }
            if g.market_cap[cand_id] < min_cap || max_cap < g.market_cap[cand_id] {
                break 'cand;
            }
            if 4 < calc_analyst_rating(g, ui, rng, cand_id) {
                break 'cand;
            }
            update_corp_control(g, ui, rng, cand_id);
            if g.public_pct < 95 || g.voting_controller < 11 {
                break 'cand;
            }
            let ts = g.tipster_state[cp];
            let mut tipster_desc = s(__content.string(5928));
            if ts == 1 {
                tipster_desc = s(__content.string(5929));
            }
            if ts == 2 {
                tipster_desc = s(__content.string(5930));
            }
            if ts == 3 {
                tipster_desc = s(__content.string(5931));
            }
            msg_text = s(__content.string(5932))
                + &tipster_desc
                + __content.string(5933)
                + __content.string(5934)
                + __content.string(5935)
                + __content.string(5936);
            if ts == 0 {
                rep(&mut msg_text, "spy,", __content.bytes(5937));
            }
            if 0 < ts {
                msg_text = msg_text
                    + __content.string(5938)
                    + __content.string(5939)
                    + __content.string(5940)
                    + "\r"
                    + "\r";
            }
            if 1 < ts {
                rep(&mut msg_text, "inflate", b"pump up");
            }
            if ts == 0 {
                msg_text = msg_text
                    + __content.string(5941)
                    + __content.string(5942)
                    + __content.string(5943)
                    + __content.string(5944)
                    + __content.string(5945)
                    + "\r"
                    + "\r";
            } else if ts == 1 {
                rep(&mut msg_text, "spy,", __content.bytes(5946));
                msg_text = msg_text
                    + __content.string(5947)
                    + __content.string(5948)
                    + __content.string(5949)
                    + __content.string(5950)
                    + "\r"
                    + "\r";
            } else if ts == 2 {
                msg_text = msg_text
                    + __content.string(5951)
                    + __content.string(5952)
                    + __content.string(5953)
                    + __content.string(5954)
                    + __content.string(5955)
                    + "\r"
                    + "\r";
                if g.currency_code == "UK" {
                    rep(&mut msg_text, "lawyer", __content.bytes(5956));
                }
            } else if ts == 3 {
                msg_text = msg_text
                    + __content.string(5957)
                    + __content.string(5958)
                    + "\r"
                    + "\r";
            }
            if ts == 1 {
                rep(&mut msg_text, __content.string(5959), __content.bytes(5960));
            }
            if ts < 2 {
                msg_text = msg_text
                    + __content.string(5961)
                    + __content.string(5962);
                if ts == 1 {
                    msg_text = msg_text
                        + __content.string(5963)
                        + __content.string(5964)
                        + __content.string(5965);
                } else {
                    msg_text = msg_text + __content.string(5966);
                }
                msg_text = msg_text + "\r" + "\r";
            }
            if ts == 0 {
                rep(&mut msg_text, __content.string(5967), __content.bytes(5968));
            }
            msg_text = msg_text
                + __content.string(5969)
                + "\r"
                + "\r"
                + __content.string(5970)
                + &trim(g.currency_symbol.as_bytes())
                + &trim(str_s(cast::<Ext, _>(ts) + Ext::from_f64(1.0)).as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + __content.string(5971);
            if 1 < ts {
                rep(&mut msg_text, __content.string(5972), b"WANT");
            }
            if 2 < ts {
                rep(&mut msg_text, "wants", __content.bytes(5973));
            }
            let answer = ui.msgbox(&msg_text, 8483, &s(__content.string(5974)));
            if answer != 6 {
                return;
            }
            g.cash[cp] = g.cash[cp] - (cast::<Ext, _>(g.tipster_state[cp]) + Ext::from_f64(1.0));
            target_id = cand_id;
            if Ext::from_f64(0.0) < g.shares_outstanding[target_id] {
                if g.market_cap[target_id] / g.shares_outstanding[target_id] < Ext::from_f64(15.0) {
                    g.shares_outstanding[target_id] = g.shares_outstanding[target_id] * Ext::from_f64(0.5);
                }
            }
            g.merger_tip_corp = cast::<i16, _>(cand_id);
            g.insider_tippee = cast::<i16, _>(cp);
            let m = cast::<i32, _>(g.merger_tip_corp);
            if g.industry_num[m] < 70 {
                g.profit_margin[m] = g.profit_margin[m].wrapping_add(5);
                g.disc_spend_pct[m] = 0;
            }
            if 85 < m && Ext::from_f64(0.0) < g.contingency_reserve[m] {
                g.reserve_adj_pending[m] = g.contingency_reserve[m] * -Ext::from_f64(1.0);
            }
            g.market_cap[m] = g.market_cap[m] * sf(0.97f32);
            g.player_notoriety[cp] = g.player_notoriety[cp].wrapping_add(1);
            if g.sim_co_count < 800 {
                g.insider_tip_event_tick = cast::<i16, _>(rng.rnd_range(1250, 500) + g.sim_co_count);
                if 1500 < g.insider_tip_event_tick {
                    g.insider_tip_event_tick = 1400;
                }
            } else {
                g.insider_tip_event_tick = cast::<i16, _>(g.sim_co_count - rng.rnd_range(1250, 10));
                if g.insider_tip_event_tick < 50 {
                    g.insider_tip_event_tick = cast::<i16, _>(rng.rnd_range(150, 50));
                }
            }
            g.insider_tip_event_qtr = 0;
            g.news_msg = g.ent_symbol[m].clone() + __content.string(5975);
            if g.rand_seed % 2 == 1 {
                g.news_msg = g.ent_symbol[m].clone() + __content.string(5976);
            }
            post_news(g, ui, 0, 0);
            g.tipster_state[cp] = g.tipster_state[cp].wrapping_add(1);
            if (cast::<Ext, _>(8 - cast::<i32, _>(g.tipster_state[cp]))) < rng.rnd() * Ext::from_f64(10.0) {
                if g.tipster_state[cp] == 1 {
                    if g.insider_probe_player == 0 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        g.insider_probe_player = cp;
                    }
                    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                        g.tipster_state[cp] = 4;
                    }
                } else if g.insider_probe_player == 0 && Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(5.0) {
                    g.insider_probe_player = cp;
                    if g.tipster_state[cp] == 4 {
                        g.tipster_state[cp] = -1;
                    }
                }
            }
            break 'tries;
        }
        // LAB_0067826d
        tries += 1;
        if 1000 < tries {
            break;
        }
    }
    // LAB_00678283
    if target_id < 11 {
        let mut tipster_desc = s(__content.string(5977));
        if 0 < g.tipster_state[cp] {
            tipster_desc = s("law");
        }
        msg_text = s(__content.string(5978))
            + &tipster_desc
            + __content.string(5979)
            + __content.string(5980)
            + __content.string(5981);
        if g.exchange_rate * Ext::from_f64(35000.0) < g.net_worth {
            msg_text = msg_text
                + __content.string(5982)
                + __content.string(5983)
                + __content.string(5984)
                + __content.string(5985)
                + __content.string(5986)
                + "\r"
                + "\r";
        }
        if g.tipster_state[cp] < 1 {
            rep(&mut msg_text, "source", __content.bytes(5987));
            msg_text = msg_text
                + __content.string(5988)
                + __content.string(5989)
                + __content.string(5990)
                + "\r"
                + "\r"
                + __content.string(5991);
        } else {
            rep(&mut msg_text, " she", b" he");
            rep(&mut msg_text, " her", b" him");
            rep(&mut msg_text, __content.string(5992), __content.bytes(5993));
        }
        ui.msgbox(&msg_text, MB_INFO, &s(__content.string(5994)));
    } else {
        g.valuation_mode = 0;
        calc_stock_price(g, ui, rng, target_id, time_secs);
        msg_text = s(__content.string(5995))
            + &g.ent_name[target_id]
            + "\r"
            + "\r"
            + __content.string(5996)
            + &g.ent_symbol[target_id]
            + __content.string(5997)
            + &g.stock_price_str
            + ". ";
        let streak = price_trend_streak(g, target_id);
        let pct_change = price_pct_change_months(g, target_id, 6);
        if streak < -3 || pct_change < -20 {
            msg_text = msg_text + "\r" + "\r" + __content.string(5998);
            if streak < -3 {
                msg_text = msg_text
                    + __content.string(5999)
                    + &number_to_words(streak, 0, 0)
                    + __content.string(1095);
            } else {
                msg_text = msg_text
                    + __content.string(6000)
                    + &number_to_words(pct_change, 0, 1)
                    + ". ";
            }
        } else if 13 < price_pct_change_months(g, target_id, 3) {
            if price_trend_direction(g, target_id, 3, 1) == 1 {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + __content.string(6001);
            }
        }
        ui.msgbox(&msg_text, MB_INFO, &s(__content.string(6002)));
        msg_text = s(__content.string(6003));
        if 1 < g.tipster_state[cp] {
            msg_text = s(__content.string(6004))
                + __content.string(6005)
                + "\""
                + __content.string(6006)
                + "\""
                + __content.string(6007);
        }
        show_quick_message(g, ui, &mut msg_text);
        g.target_corp = target_id;
        select_target_corp(g, ui, rng, time_secs);
        // (UI) CONTROL SET TEXT hDlgMain, 202, EntSymbol(SelEntity)
    }
}

// ------------------------------------------------------------------------------- CheatDisableAntitrust

/// Port of CheatDisableAntitrust (0x680954).
/// The antitrust cheat: asks (YES/NO/CANCEL) whether to [runtime resource wording]
/// (`PrivateAntitrustOff = 1`, `AntitrustCheatUsed = 1`, pending suits cleared) and then government
/// antitrust rules (`GovtAntitrustOff = 1`, `AntitrustCheatUsed = 1`); CANCEL ends it. If both are
/// already off only an error box is shown. The exit GOSUB CheatDisableAntitrust_Exit (0x680fa5) is empty.
pub fn cheat_disable_antitrust(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let mut msg_text: PbStr;
    if g.private_antitrust_off == 1 && g.govt_antitrust_off == 1 {
        msg_text = s(__content.string(6008))
            + __content.string(6009)
            + __content.string(6010)
            + "\r"
            + "\r"
            + __content.string(6011)
            + "\r"
            + "\r"
            + __content.string(6012);
        ui.msgbox(&msg_text, 8208, &s(__content.string(6013)));
        return;
    }
    let mut closing_line = s("\r\n") + "\r\n" + __content.string(6014);
    if g.private_antitrust_off == 0 {
        if g.game_end_year - g.start_year < 36 && g.govt_antitrust_off == 0 {
            msg_text = s(__content.string(6015))
                + __content.string(6016)
                + __content.string(6017)
                + __content.string(6018)
                + "\r"
                + "\r";
            if g.player_cheat_code[g.cur_player] < 6 && g.game_end_year - g.start_year < 36 {
                msg_text = msg_text
                    + __content.string(6019)
                    + __content.string(6020)
                    + "\r"
                    + "\r";
            }
            msg_text = msg_text + __content.string(6021);
            ui.msgbox(&msg_text, MB_INFO, &s(__content.string(6022)));
        }
        msg_text = s(__content.string(6023));
        if g.govt_antitrust_off == 1 {
            rep(&mut msg_text, __content.string(6024), b"Do");
        }
        let answer = ui.msgbox(&msg_text, MB_YNC | 0x100, &s(__content.string(6025)));
        if answer == 2 {
            return;
        }
        if answer == 6 {
            msg_text = s(__content.string(6026))
                + __content.string(6027);
            if g.govt_antitrust_off == 1 {
                msg_text = msg_text + &closing_line;
            } else {
                msg_text = msg_text
                    + __content.string(6028)
                    + __content.string(6029);
            }
            show_quick_message(g, ui, &mut msg_text);
            g.private_antitrust_off = 1;
            g.antitrust_cheat_used = 1;
            cheat_disable_antitrust_clear_suits(g);
        }
    }
    if g.govt_antitrust_off == 1 {
        quick(g, ui, s(__content.string(6030)));
        return;
    }
    closing_line = s("\r\n") + "\r\n" + __content.string(6031);
    if g.govt_antitrust_off == 0 {
        msg_text = s(__content.string(6032))
            + __content.string(6033)
            + __content.string(6034);
        let answer = ui.msgbox(&msg_text, MB_YNC | 0x100, &s(__content.string(6035)));
        if answer == 2 {
            return;
        }
        if answer == 6 {
            msg_text = s(__content.string(6036))
                + __content.string(6037)
                + __content.string(6038);
            if g.private_antitrust_off == 1 {
                msg_text = msg_text + &closing_line;
            } else {
                msg_text = msg_text
                    + __content.string(6039)
                    + __content.string(6040);
            }
            show_quick_message(g, ui, &mut msg_text);
            g.govt_antitrust_off = 1;
            g.antitrust_cheat_used = 1;
        }
    }
}

/// Port of CheatDisableAntitrust_ClearSuits (0x680ebb), GOSUB of CheatDisableAntitrust.
/// Deletes every pending lawsuit record except harassing suits (type `"2"`) and clears the current
/// antitrust case (`AntitrustDefendant` / `Plaintiff` / `MarketShare` / `Damages` / `SettleAmt`).
fn cheat_disable_antitrust_clear_suits(g: &mut Globals) {
    for suit_slot in 1..=10 {
        let rec = g.lawsuit_rec[suit_slot].clone();
        if !(rec == "") && !(mid(rec.as_bytes(), 1, 1) == "2") {
            g.lawsuit_rec[suit_slot] = PbStr::new();
        }
    }
    g.antitrust_defendant = 0;
    g.antitrust_plaintiff = 0;
    g.antitrust_market_share = 0;
    g.antitrust_damages = 0;
    g.antitrust_settle_amt = 0;
}
