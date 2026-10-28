//! The transaction / research "action menu": `ShowActionMenu` (0x4131ea), its dialog procedure
//! `DlgProcActionMenu` (0x41628e), the button callbacks `ActionMenu_Btn1..15`, `ActionMenu_Close`
//! (0x417061), `ActionMenu_SelectCorpBtn` (0x4171d7), `ActionMenu_PlayerBtn` (0x41715d) and
//! `DisableActionMenuControls` (0x4245e3).
//!
//! # The menu as the host sees it
//!
//! `ShowActionMenu` builds a modal dialog with up to 15 command buttons for the menu `ActionMenuMode`
//! (taken from `TmpExt1`; see [`modes`]), an advice box (`HelpText`, filled by `BuildMenuAdviceText`),
//! and the buttons "Player", "Select Corp." / "My Corps." and "CLOSE". The port shows that dialog with
//! one [`Ui::choose`] call per round of the modal loop:
//!
//! * `title` = the dialog title, `prompt` = the advice label followed by the lines of `HelpText`,
//! * `options` = the command captions (`&` marks the accelerator, as in the original), then
//!   [`ActionMenu::player_caption`], [`ActionMenu::corp_caption`] and [`ActionMenu::close_caption`].
//!
//! The answer maps onto the original's callbacks: an index below the caption count presses command
//! button `index + 1` ([`action_menu_btn`] → `DoTransactionMenuAction`), the next three indices are
//! [`action_menu_player_btn`], [`action_menu_select_corp_btn`] and [`action_menu_close`], and `None`
//! closes the window with its close box ([`dlg_proc_action_menu_close`]). The loop ends when the dialog
//! is gone ([`dialog_end_trans_menu`] cleared `hDlgTransMenu`, e.g. because a command ended it or a
//! nested `ShowActionMenu` replaced it). [`build_action_menu`] alone yields the content (title,
//! captions) for a host that wants to render the menu itself.
//!
//! # The dialog handle
//!
//! `hDlgTransMenu` is the original's window handle. The port keeps a non-zero token there while the
//! menu is open (a fresh value per dialog) and 0 once it was ended, so that every `DIALOG END
//! hDlgTransMenu` of the original (this module, `DoTransactionMenuAction`, `ExecuteMenuCommand`,
//! `CloseAllDialogs(1)`) can be ported as [`dialog_end_trans_menu`], which runs the dialog's
//! `WM_DESTROY` handler like Windows does. The value itself is meaningless (the tests ignore it).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;

/// `ActionMenuMode` values (`TmpExt1` when calling [`show_action_menu`]): which menu the dialog shows.
pub mod modes {

    /// Entity research (reports on `SelEntity`).
    pub const ENTITY_RESEARCH: i32 = 1;
    /// General research (market, industries, news, database search).
    pub const GENERAL_RESEARCH: i32 = 2;
    /// Miscellaneous functions (loans, splits, alerts, T-bills).
    pub const MISC: i32 = 3;
    /// Buy / sell transactions (stocks, bonds, options, futures, crypto).
    pub const BUY_SELL: i32 = 4;
    /// Financing transactions (start-ups, offerings, bonds, dividends, liquidation).
    pub const FINANCING: i32 = 5;
    /// Management transactions (CEO, managers, policies, autopilot).
    pub const MANAGEMENT: i32 = 6;
    /// Other transactions (lawsuits, rumors, exercising options, banks, swaps).
    pub const OTHER: i32 = 7;
}

/// Control ids [runtime resource wording] 1..15 (`hDlgTransMenu`; 463 is not used).
pub const BUTTON_IDS: [i32; 15] = [461, 462, 464, 465, 466, 467, 468, 469, 470, 471, 472, 473, 474, 475, 476];
/// Control id of the "Player" button.
pub const PLAYER_BUTTON_ID: i32 = 477;
/// Control id of the "Select Corp." / "My Corps." button.
pub const CORP_BUTTON_ID: i32 = 478;
/// Control id of the "CLOSE" button (IDOK).
pub const CLOSE_BUTTON_ID: i32 = 1;

/// What the action menu dialog shows (built by [`build_action_menu`]).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionMenu {
    /// `ActionMenuMode`.
    pub mode: i32,
    /// Dialog title.
    pub title: PbStr,
    /// The command buttons 1..n (`&` marks the accelerator key).
    pub captions: Vec<PbStr>,
    /// Caption of the advice box (label 479).
    pub advice_label: PbStr,
    /// "&Player".
    pub player_caption: PbStr,
    /// "Select &Corp." (modes 1, 2) or "My &Corps." (modes 3..7).
    pub corp_caption: PbStr,
    /// "CL&OSE".
    pub close_caption: PbStr,
}

impl ActionMenu {
    /// The options of the [`Ui::choose`] call: captions, then Player, Select Corp., CLOSE.
    pub fn options(&self) -> Vec<PbStr> {
        let mut o = self.captions.clone();
        o.push(self.player_caption.clone());
        o.push(self.corp_caption.clone());
        o.push(self.close_caption.clone());
        o
    }
}

/// `DIALOG END hDlgTransMenu`: if the action menu is open, ends it and runs its `WM_DESTROY` handler
/// ([`dlg_proc_action_menu_destroy`]); `hDlgTransMenu` becomes 0. Callers of the original's `DIALOG END
/// hDlgTransMenu` use this so that the port's modal loop in [`show_action_menu`] ends like the dialog.
pub fn dialog_end_trans_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if g.h_dlg_trans_menu != 0 {
        // DestroyWindow keeps the handle valid during WM_DESTROY. A news story reached by the
        // callback can end this menu again and run another pair of ticks before this call returns.
        dlg_proc_action_menu_destroy(g, ui, rng, env);
        g.h_dlg_trans_menu = 0;
    }
}

/// `TickerPauseUntil = 0: ClockState = "": TickerOn = 0: ActionCode = "SUPP"` (+ `TickerSimSteps(n)`
/// when `run`), then `ActionCode = ""`: the suppressed ticker steps after a menu closes.
fn suppressed_ticker(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, steps: i32, run: bool) {
    g.ticker_pause_until = 0;
    set(&mut g.clock_state, "");
    g.ticker_on = 0;
    set(&mut g.action_code, "SUPP");
    if run && !env.skips(0x41ef3f) {
        crate::logic::turn::ticker_sim_steps_skipping(g, ui, rng, steps, env.now, env.skip);
    }
    set(&mut g.action_code, "");
}

/// The `SELECT CASE ActionMenuMode` part of ShowActionMenu (0x4131ea): the dialog's title and command
/// captions for the current state (`ActionMenuMode`, `SelEntity`, `ActiveEntity`, ...). Calls
/// `FindShortPositions` (mode 4, players), `GetSelectedPlayerEtf` (mode 5) and `GetFundsAdvisedBy`
/// (mode 6) like the original. `None` for a mode outside 1..7 (no dialog).
pub fn build_action_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) -> Option<ActionMenu> {
    let __content = crate::content::current();
    let mut cap: Vec<PbStr> = vec![PbStr::new(); 16]; // DIM btnCaption(15)
    let btn_count: usize;
    let title: PbStr;
    let ae = g.active_entity;
    let se = g.sel_entity;
    let ind = |g: &Globals, e: i32| cast::<i32, _>(g.industry_num[e]);
    match g.action_menu_mode {
        1 => {
            let mut n = 14;
            title = s(__content.string(9905)) + &g.sel_corp_name + ")";
            for (k, t) in [
                __content.string(9906),
                __content.string(9907),
                __content.string(9908),
                __content.string(9909),
                __content.string(9910),
                __content.string(9911),
                __content.string(9912),
                __content.string(9913),
                __content.string(9914),
                __content.string(9915),
                __content.string(9916),
                __content.string(9917),
                __content.string(9918),
                __content.string(9919),
            ]
            .iter()
            .enumerate()
            {
                cap[k + 1] = s(t);
            }
            if 10 < se && ind(g, se) == 1 {
                cap[9] = s(__content.string(9920));
                cap[12] = cap[14].clone();
                n = 12;
            }
            if se < 86 {
                if se < 11 {
                    n = 11;
                    cap[1] = cap[2].clone();
                    cap[2] = cap[5].clone();
                    cap[3] = cap[6].clone();
                    cap[4] = cap[7].clone();
                    cap[5] = cap[10].clone();
                    cap[6] = s(__content.string(9921));
                    cap[7] = s(__content.string(9916));
                    cap[8] = cap[12].clone();
                    cap[9] = s(__content.string(9918));
                    cap[10] = cap[14].clone();
                    cap[11] = s(__content.string(9922));
                }
            } else if ind(g, se) == 2 || 69 < ind(g, se) {
                cap[9] = cap[10].clone();
                if ind(g, se) == 2 || ind(g, se) == 70 {
                    cap[10] = cap[11].clone();
                    cap[11] = cap[12].clone();
                }
                if ind(g, se) == 2 {
                    n = 12;
                    cap[12] = cap[14].clone();
                } else if ind(g, se) == 70 {
                    n = 13;
                    cap[12] = cap[13].clone();
                    cap[13] = cap[14].clone();
                } else if ind(g, se) == 71 {
                    if se == 1600 {
                        n = 13;
                        cap[10] = cap[11].clone();
                        cap[11] = cap[12].clone();
                        cap[12] = cap[13].clone();
                        cap[13] = cap[14].clone();
                    } else if se == 1584 || se == 1585 {
                        n = 11;
                        cap[10] = cap[12].clone();
                        cap[11] = cap[14].clone();
                    } else if se < 1581 || 1583 < se {
                        n = 11;
                        cap[10] = cap[11].clone();
                        cap[11] = cap[14].clone();
                    } else {
                        n = 10;
                        cap[10] = cap[14].clone();
                    }
                }
            }
            btn_count = n;
        }
        2 => {
            let mut suffix = PbStr::new();
            if 0 < g.sel_industry {
                suffix = s(" -- ") + &g.ind_name[g.sel_industry];
            }
            title = s(__content.string(9923)) + &suffix;
            for (k, t) in [
                __content.string(9924),
                __content.string(9925),
                __content.string(9914),
                __content.string(9913),
                __content.string(9926),
                __content.string(9927),
                __content.string(9928),
                __content.string(9929),
                __content.string(9930),
                __content.string(9931),
                __content.string(9932),
                __content.string(9933),
                __content.string(9934),
            ]
            .iter()
            .enumerate()
            {
                cap[k + 1] = s(t);
            }
            if g.sel_industry == 71 {
                cap[3] = s(__content.string(9935));
            }
            btn_count = 13;
        }
        3 => {
            let mut n = 9;
            title = s(__content.string(9936)) + &g.active_entity_name + ")";
            for (k, t) in [
                __content.string(9937),
                __content.string(9938),
                __content.string(9939),
                __content.string(9940),
                __content.string(9941),
                __content.string(9942),
                __content.string(9943),
                __content.string(9944),
                __content.string(9945),
            ]
            .iter()
            .enumerate()
            {
                cap[k + 1] = s(t);
            }
            if ae < 11 {
                n = 8;
                cap[3] = s(__content.string(9946));
                cap[4] = s(__content.string(9947));
                cap[5] = s(__content.string(9948));
                cap[7] = cap[8].clone();
                cap[8] = cap[9].clone();
            } else if ae < 131 || 69 < ind(g, ae) {
                n = 8;
                cap[7] = cap[8].clone();
                cap[8] = cap[9].clone();
            }
            btn_count = n;
        }
        4 => {
            title = s(__content.string(9949)) + &g.active_entity_name + ")";
            let mut n = 15;
            cap[15] = s(__content.string(9950));
            if ae < 11 {
                n = 13;
                cap[11] = s(__content.string(9951));
                cap[12] = s(__content.string(9952));
                cap[13] = cap[15].clone();
            } else if ind(g, ae) == 70 {
                n = 14;
                cap[12] = s(__content.string(9951));
                cap[13] = s(__content.string(9952));
                cap[14] = cap[15].clone();
            } else if ind(g, ae) == 1 {
                n = 12;
            } else if ind(g, ae) == 2 {
                n = 13;
                cap[13] = s(__content.string(9951));
            }
            cap[1] = s(__content.string(9953));
            if ae < 11 {
                let cp = g.cur_player;
                let shorts = if env.skips(0x5df7c4) {
                    0
                } else {
                    crate::logic::shorts::find_short_positions(g, ui, rng, 0, cp)
                };
                if shorts == 3 {
                    cap[1] = s(__content.string(9954));
                }
            }
            cap[2] = s(__content.string(9955));
            let mut off: usize = 0;
            // (the industry tests below read ActiveEntity again: FindShortPositions does not change it)
            let ae = g.active_entity;
            if ae < 11 {
                cap[2] = s(__content.string(9956));
                cap[3] = s(__content.string(9957));
                cap[4] = s(__content.string(9958));
                cap[5] = s(__content.string(9959));
            } else {
                cap[3] = s("&Merger");
                cap[4] = s(__content.string(9960));
                cap[5] = s(__content.string(9961));
                off = 2;
                if ind(g, ae) < 3 {
                    cap[6] = s(__content.string(9957));
                    cap[7] = s(__content.string(9958));
                    cap[8] = s(__content.string(9959));
                } else {
                    cap[6] = s(__content.string(9962));
                    if ind(g, ae) != 70 {
                        cap[7] = s(__content.string(9963));
                        cap[13] = s(__content.string(9951));
                        cap[14] = s(__content.string(9952));
                    }
                }
                if ind(g, ae) == 70 {
                    off = 1;
                } else if ind(g, ae) < 3 {
                    off = 3;
                }
            }
            cap[off + 6] = s(__content.string(9964));
            cap[off + 7] = s(__content.string(9965));
            cap[off + 8] = s(__content.string(9966));
            cap[off + 9] = s(__content.string(9967));
            if ae < 11 || 130 < ae {
                cap[off + 10] = s(__content.string(9968));
            }
            btn_count = n;
        }
        5 => {
            let mut n = 10;
            title = s(__content.string(9969)) + &g.active_entity_name + ")";
            for (k, t) in [
                __content.string(9970),
                __content.string(9971),
                __content.string(9972),
                __content.string(9973),
                __content.string(9974),
                __content.string(9975),
                __content.string(9976),
                __content.string(9977),
                __content.string(9978),
                __content.string(9979),
            ]
            .iter()
            .enumerate()
            {
                cap[k + 1] = s(t);
            }
            if ae < 11 {
                n = 2;
            } else if ind(g, ae) < 3 {
                n = 9;
                cap[9] = cap[10].clone();
            }
            let etf = if env.skips(0x652b17) {
                0
            } else {
                crate::logic::trading::get_selected_player_etf(g, ui, rng)
            };
            if 1580 < etf && g.active_entity < 11 {
                n = 7;
            }
            btn_count = n;
        }
        6 => {
            let mut n = 9;
            title = s(__content.string(9980)) + &g.active_entity_name + ")";
            let cp = g.cur_player;
            let mut suffix = PbStr::new();
            if 10 < ae {
                suffix = s(" of ") + &g.ent_name[ae];
                if 7 < suffix.len() {
                    suffix = s(" of ") + &g.ent_symbol[ae];
                }
                if ae == g.player_ceo_of[cp] {
                    suffix = PbStr::new();
                }
            }
            cap[1] = s(__content.string(9981)) + &suffix;
            suffix = PbStr::new();
            if 10 < g.player_ceo_of[cp] {
                suffix = s(" of ") + &g.ent_symbol[g.player_ceo_of[cp]];
            }
            cap[2] = s(__content.string(9982)) + &suffix;
            cap[3] = s(__content.string(9983));
            cap[4] = s(__content.string(9984));
            cap[5] = s(__content.string(9985));
            cap[6] = s(__content.string(9986));
            cap[7] = s(__content.string(9987));
            cap[8] = s(__content.string(9988));
            cap[9] = s(__content.string(9989));
            if 10 < ae && g.auto_pilot[ae] == 0 {
                cap[9] = replace(cap[9].as_bytes(), b"OFF", b"ON");
            }
            cap[10] = s(__content.string(9990));
            if 10 < ae && (ind(g, ae) == 2 || ind(g, ae) == 37) {
                let funds = if env.skips(0x64d4fe) { 0 } else { crate::logic::earnings::get_funds_advised_by(g, ae) };
                if funds == 0 {
                    cap[10] = s(__content.string(9991));
                }
            }
            if ae < 11 {
                n = 2;
            } else if ind(g, ae) < 3 {
                n = 8;
                if ind(g, ae) == 1 {
                    cap[7] = s(__content.string(9992));
                } else {
                    cap[7] = cap[10].clone();
                }
                cap[8] = cap[9].clone();
            } else if ind(g, ae) == 70 {
                n = 5;
                cap[5] = cap[9].clone();
            } else if ind(g, ae) == 37 {
                n = 10;
            }
            btn_count = n;
        }
        7 => {
            let mut n = 5;
            title = s(__content.string(9993)) + &g.active_entity_name + ")";
            cap[1] = s(__content.string(9994));
            cap[2] = s(__content.string(9995));
            cap[3] = s(__content.string(9996));
            cap[4] = s(__content.string(9997));
            cap[5] = s(__content.string(9998));
            if 10 < ae {
                if ind(g, ae) == 1 {
                    n = 8;
                    cap[5] = s(__content.string(9999));
                    cap[6] = s(__content.string(10000));
                    cap[7] = s(__content.string(10001));
                    cap[8] = s(__content.string(10002)) + &g.ent_symbol[ae] + __content.string(10003);
                } else {
                    cap[6] = s(__content.string(10002)) + &g.ent_symbol[ae] + __content.string(10003);
                    cap[7] = s(__content.string(10004)) + &g.ent_symbol[ae] + __content.string(10003);
                    n = 7;
                }
            }
            n += 1;
            cap[n] = s(__content.string(10005));
            n += 1;
            cap[n] = s(__content.string(10006));
            if 85 < ae && ae < 131 {
                n += 1;
                cap[n] = s(__content.string(10007));
                n += 1;
                cap[n] = s(__content.string(10008));
            }
            btn_count = n;
        }
        _ => return None,
    }
    let mode = g.action_menu_mode;
    let mut advice_label = s(__content.string(10009));
    if mode == 1 {
        advice_label = if g.cur_player == g.sel_entity {
            s(__content.string(10010))
        } else {
            s(__content.string(10011))
        };
    }
    if mode == 2 {
        advice_label = s(__content.string(10012));
    }
    Some(ActionMenu {
        mode,
        title,
        captions: cap[1..=btn_count].to_vec(),
        advice_label,
        player_caption: s("&Player"),
        corp_caption: if 2 < mode { s(__content.string(10013)) } else { s(__content.string(10014)) },
        close_caption: s("CL&OSE"),
    })
}

/// Port of ShowActionMenu (0x4131ea).
///
/// Opens the action menu `ActionMenuMode = ROUND(TmpExt1)` (`TmpExt1` is reset to 0): when the player has
/// used up the turn (`TurnActionCount > 4`, not `ActionCode = "AUTO"`, game not over) it only refreshes
/// the market and runs `TickerSimSteps(4)`. Otherwise it makes `SelEntity` the acting entity if the
/// current player controls it (after `UpdateCorpControl`), ends other menus, sets `ActionMenuOpen = 1`, lets
/// a bank `ActiveEntity` repay its own bank debt from cash (the repayment is credited to entity
/// `ActiveEntity - 1`, see BUGS.md #425), builds the menu ([`build_action_menu`]), fills `HelpText`
/// (`BuildMenuAdviceText`) and runs the modal dialog loop through [`Ui::choose`] (see the
/// module docs). Afterwards: `PauseTicker(27.5)` while the ticker runs, `ActionMenuOpen = 0`. A mode
/// outside 1..7 returns right after the repayment (no dialog, `ActionMenuOpen` stays 1).
pub fn show_action_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if 4 < g.turn_action_count && g.action_code != "AUTO" && g.game_end_flag != "Y" {
        if !env.skips(0x4400d7) {
            crate::logic::market::update_market_display(g, ui, rng, env.now.time_secs());
        }
        if !env.skips(0x41ef3f) {
            crate::logic::turn::ticker_sim_steps_skipping(g, ui, rng, 4, env.now, env.skip);
        }
        return;
    }
    let entity_id = g.sel_entity;
    if !env.skips(0x41f71b) {
        crate::logic::control::update_corp_control(g, ui, rng, entity_id);
    }
    if g.cur_player == g.corp_controller[entity_id] {
        g.active_entity = entity_id;
        g.active_entity_name = g.ent_name[entity_id].clone();
    }
    g.action_menu_mode = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    // (UI) DIALOG END DlgWhoOwnsWhat; DIALOG END hCheatDlg
    dialog_end_trans_menu(g, ui, rng, env);
    g.action_menu_open = 1;
    let ae = g.active_entity;
    if 10 < ae && g.industry_num[ae] == 1 && Ext::from_f64(10.0) < g.cash[ae] && Ext::from_f64(10.0) < g.bank_debt[ae] {
        let repay = if g.bank_debt[ae] <= g.cash[ae] { g.bank_debt[ae] } else { g.cash[ae] };
        g.cash[ae] = g.cash[ae] - repay;
        g.bank_debt[ae] = g.bank_debt[ae] - repay;
        if 11 < ae {
            g.cash[ae - 1] = g.cash[ae - 1] + repay;
        }
    }
    let Some(menu) = build_action_menu(g, ui, rng, env) else {
        return;
    };
    // DIALOG NEW (UI: layout, fonts); the token stands for the window handle (see the module docs)
    let handle = g.h_dlg_trans_menu.wrapping_add(1).max(1);
    g.h_dlg_trans_menu = handle;
    if !env.skips(0x557bbf) { crate::logic::advice::build_menu_advice_text(g, ui, rng, env.now.second); }
    // (UI) labels, advice box (HelpText), buttons, CONTROL SET FOCUS; DIALOG SHOW MODAL ... CALL DlgProcActionMenu
    run_modal(g, ui, rng, env, &menu, handle);
    if g.clock_state == "RUN" {
        crate::logic::turn::pause_ticker(g, &ui.local_time(), 27.5f32);
    }
    g.action_menu_open = 0;
}

/// The modal loop of the action menu dialog `handle`: one [`Ui::choose`] per round until the dialog is
/// gone.
fn run_modal(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, menu: &ActionMenu, handle: i32) {
    let n = menu.captions.len();
    while g.h_dlg_trans_menu == handle {
        let mut prompt = vec![menu.advice_label.clone()];
        prompt.extend(help_lines(&g.help_text));
        match ui.choose(&menu.title, &prompt, &menu.options()) {
            Some(k) if k < n => action_menu_btn(g, ui, rng, env, cast::<i32, _>(k) + 1),
            Some(k) if k == n => action_menu_player_btn(g, ui, rng, env),
            Some(k) if k == n + 1 => action_menu_select_corp_btn(g, ui, rng, env),
            Some(_) => action_menu_close(g, ui, rng, env),
            None => {
                dlg_proc_action_menu_close(g, ui, rng, env);
                // The modal dialog's default close action then invokes the CLOSE button.
                action_menu_close(g, ui, rng, env);
            }
        }
    }
}

/// The lines of the advice box text (`HelpText`, `$CRLF`-separated).
fn help_lines(t: &PbStr) -> Vec<PbStr> {
    if t.is_empty() {
        return Vec::new();
    }
    let b = t.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' || b[i] == b'\n' {
            out.push(PbStr(b[start..i].to_vec()));
            if b[i] == b'\r' && i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    out.push(PbStr(b[start..].to_vec()));
    out
}

/// The command buttons of the action menu:
///
/// * Port of ActionMenu_Btn1 (0x416b75).
/// * Port of ActionMenu_Btn2 (0x416bc9).
/// * Port of ActionMenu_Btn3 (0x416c1d).
/// * Port of ActionMenu_Btn4 (0x416c71).
/// * Port of ActionMenu_Btn5 (0x416cc5).
/// * Port of ActionMenu_Btn6 (0x416d19).
/// * Port of ActionMenu_Btn7 (0x416d6d).
/// * Port of ActionMenu_Btn8 (0x416dc1).
/// * Port of ActionMenu_Btn9 (0x416e15).
/// * Port of ActionMenu_Btn10 (0x416e69).
/// * Port of ActionMenu_Btn11 (0x416ebd).
/// * Port of ActionMenu_Btn12 (0x416f11).
/// * Port of ActionMenu_Btn13 (0x416f65).
/// * Port of ActionMenu_Btn14 (0x416fb9).
/// * Port of ActionMenu_Btn15 (0x41700d).
///
/// Each runs `DoTransactionMenuAction(n)` on `BN_CLICKED` (`btn_num` = 1..15).
pub fn action_menu_btn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, btn_num: i32) {
    if !env.skips(0x552f0b) {
        super::menu::do_transaction_menu_action(g, ui, rng, env, btn_num);
    }
}

/// Port of ActionMenu_Close (0x417061): the CLOSE button. `ActionMenuOpen = 0`, ends the dialog; unless
/// the ticker runs, `SimCoCount < 1590` gives 4 suppressed ticker steps; then `CloseAllDialogs(2)`.
pub fn action_menu_close(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    g.action_menu_open = 0;
    dialog_end_trans_menu(g, ui, rng, env);
    if g.clock_state != "RUN" && g.sim_co_count < 1590 {
        suppressed_ticker(g, ui, rng, env, 4, true);
    }
    if !env.skips(0x493e2b) {
        crate::logic::economy::close_all_dialogs_skipping(g, ui, rng, 2, env);
    }
}

/// Port of ActionMenu_PlayerBtn (0x41715d): the "Player" button. Unless the player is already selected,
/// selects the player (`SelectActingEntity(1)`) and reopens the menu in the same mode.
pub fn action_menu_player_btn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if g.cur_player != g.sel_entity {
        if !env.skips(0x4721dc) {
            crate::logic::trading::select_acting_entity(g, ui, rng, 1, env.now);
        }
        g.tmp_ext1 = cast::<Ext, _>(g.action_menu_mode);
        if !env.skips(0x4131ea) {
            show_action_menu(g, ui, rng, env);
        }
    }
}

/// Port of ActionMenu_SelectCorpBtn (0x4171d7): "Select Corp." (research menus: the symbol entry,
/// `SelectActingEntity(3)`) or "My Corps." (transaction menus: the pick list of the player's companies,
/// `SelectActingEntity(2)`), then reopens the menu in the same mode.
pub fn action_menu_select_corp_btn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if g.action_menu_mode < 3 {
        if !env.skips(0x4721dc) {
            crate::logic::trading::select_acting_entity(g, ui, rng, 3, env.now);
        }
    } else {
        if !env.skips(0x4242c6) {
            crate::logic::restructure::merge::disable_main_controls(g);
        }
        if !env.skips(0x4245e3) {
            disable_action_menu_controls(g);
        }
        if !env.skips(0x4721dc) {
            crate::logic::trading::select_acting_entity(g, ui, rng, 2, env.now);
        }
        if !env.skips(0x424754) {
            crate::logic::turn::enable_main_controls(g);
        }
    }
    g.tmp_ext1 = cast::<Ext, _>(g.action_menu_mode);
    if !env.skips(0x4131ea) {
        show_action_menu(g, ui, rng, env);
    }
}

/// Port of DlgProcActionMenu (0x41628e), `WM_DESTROY` (2): when the menu window goes away and the ticker
/// does not run, `TickerStopReq = "N"` and `SimCoCount < 1590`: suppressed ticker state and, while the
/// turn is not used up (`TurnActionCount < 5`), `TickerSimSteps(2)`.
///
/// The other messages: `WM_CLOSE` is [`dlg_proc_action_menu_close`], `WM_HELP` (F1) is
/// [`dlg_proc_action_menu_help`]; colours (`WM_CTLCOLOR*`, by `ActionMenuMode`), tooltips and the rest
/// are UI only.
pub fn dlg_proc_action_menu_destroy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if g.clock_state != "RUN" && g.ticker_stop_req == "N" && g.sim_co_count < 1590 {
        let run = g.turn_action_count < 5;
        suppressed_ticker(g, ui, rng, env, 2, run);
    }
}

/// DlgProcActionMenu (0x41628e), `WM_CLOSE` (16): `CloseAllDialogs(2)` (the window is then destroyed).
pub fn dlg_proc_action_menu_close(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if !env.skips(0x493e2b) {
        crate::logic::economy::close_all_dialogs_skipping(g, ui, rng, 2, env);
    }
}

/// DlgProcActionMenu (0x41628e), `WM_HELP` (83): `ShowHelp`.
pub fn dlg_proc_action_menu_help(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if !env.skips(0x40f506) {
        super::settings::show_help(g, ui, rng, env);
    }
}

/// Port of DisableActionMenuControls (0x4245e3): disables all buttons of the action menu while a command
/// runs (UI only; no game state).
pub fn disable_action_menu_controls(g: &mut Globals) {
    let _ = g;
}
