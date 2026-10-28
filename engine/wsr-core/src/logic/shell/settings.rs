//! The trading desk's **File / Game Options / Settings** menu entries (DlgProcTradingDesk's
//! `WM_COMMAND` targets): the settings toggles, ticker speed, growth throttle, currency, law firm, chart
//! options, new game / exit, and the help / web-page launches.
//!
//! Every entry is a `pub fn` taking `(g, ui, rng, env)`. Settings that the original stores in `cfig.wsr`
//! (`SaveConfig`) write `CFIG.WSR` into [`Env::files`]; web pages, the help program and the external
//! utilities are [`HostRequest`]s; the menu captions the original updates with `MENU SET TEXT` are
//! available through [`settings_menu_label`] and [`update_currency_menu_text`].
//!
//! The original extension calls use the portable `extensions` implementations of LAWDLL and VARICHEK.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::newgame_helpers::save_config;

/// Menu command ids of the trading desk's File, Game Options and Settings menus (`WM_COMMAND` ids of
/// DlgProcTradingDesk).
pub mod menu_id {

    pub const NEW_GAME: i32 = 605;
    pub const OPEN_SAVED_GAME: i32 = 610;
    pub const SAVE_GAME: i32 = 615;
    pub const SAVE_GAME_AS: i32 = 620;
    pub const EXIT: i32 = 630;
    pub const TICKER_SPEED: i32 = 650;
    pub const SELECT_CURRENCY: i32 = 651;
    pub const CHEAT_MODE: i32 = 652;
    pub const LAW_FIRM: i32 = 653;
    pub const HIGH_SCORE: i32 = 654;
    pub const CUSTOMIZER: i32 = 655;
    pub const ORDER_MANUAL: i32 = 656;
    pub const SPECULATOR: i32 = 657;
    pub const STRATEGY_MANUAL: i32 = 658;
    pub const UPDATES: i32 = 659;
    pub const UPGRADES: i32 = 660;
    pub const FORUM: i32 = 661;
    pub const TUTORIAL: i32 = 662;
    pub const SUPPRESS_CASH_WARN: i32 = 664;
    pub const SUPPRESS_POPUPS: i32 = 665;
    pub const SUPPRESS_EARNINGS: i32 = 666;
    pub const AUTO_SAVE: i32 = 667;
    pub const SWEEP_CASH: i32 = 668;
    pub const EXERCISE_OPTIONS: i32 = 669;
    pub const MAKE_DELIVERY: i32 = 670;
    pub const TAKE_DELIVERY: i32 = 671;
    pub const CHART_SIZE: i32 = 672;
    pub const CLEAR_CHART_HISTORY: i32 = 673;
    pub const AUTO_ADD_STREAM: i32 = 674;
    pub const AUTOPILOT_GLOBAL: i32 = 675;
    pub const GROWTH_THROTTLE: i32 = 676;
    pub const HELP: i32 = 678;
    pub const ABOUT: i32 = 679;
}

const SAVE_CONFIG: u32 = 0x46cdb0;
const SHOW_QUICK_MESSAGE: u32 = 0x46e851;
const TICKER_SIM_STEPS: u32 = 0x41ef3f;
const FILE_EXISTS: u32 = 0x46bf86;
const SHELL_OPEN_URL: u32 = 0x5c301a;

/// `ShowQuickMessage(text)` through the skip mechanism.
fn quick(g: &mut Globals, ui: &mut dyn Ui, env: &Env, text: PbStr) {
    if !env.skips(SHOW_QUICK_MESSAGE) {
        let mut t = text;
        show_quick_message(g, ui, &mut t);
    }
}

/// `SaveConfig(PrefCurrency)`: writes `cfig.wsr`.
fn save_cfg(g: &mut Globals, env: &Env) {
    if !env.skips(SAVE_CONFIG) {
        let cfg = save_config(g);
        let path = g.data_path.clone() + "cfig.wsr";
        env.files.borrow_mut().write(path.as_bytes(), cfg);
    }
}

/// `FileExists(path)` (0x46bf86) on [`Env::files`].
fn exists(env: &Env, path: &PbStr) -> bool {
    if env.skips(FILE_EXISTS) {
        return false;
    }
    env.files.borrow().exists(path.as_bytes())
}

/// `ShellOpenUrl(url)` through the skip mechanism.
fn open_url(env: &Env, url: PbStr) {
    if !env.skips(SHELL_OPEN_URL) {
        shell_open_url(env, &url);
    }
}

/// " [runtime resource wording] "
#[allow(non_snake_case)]
fn OVERRIDE_NOTE(__content: &crate::content::ContentPack) -> &str { __content.string(10184) }
/// The variant with "the setting … in the saved game file" (delivery toggles).
#[allow(non_snake_case)]
fn OVERRIDE_NOTE_THE(__content: &crate::content::ContentPack) -> &str { __content.string(3643) }

/// The common tail of the simple toggles: store the setting for the current player (or add the note
/// when no game is running), `SaveConfig`, `ConfigLoaded = "Y"`, and the message box.
fn toggle_tail(
    g: &mut Globals,
    ui: &mut dyn Ui,
    env: &Env,
    mut msg_text: PbStr,
    note: &str,
    store: fn(&mut Globals, i32),
    title: &str,
) {
    if g.cur_player < 1 {
        msg_text = msg_text + "\r" + "\r" + note;
    } else {
        let p = g.cur_player;
        store(g, p);
    }
    save_cfg(g, env);
    set(&mut g.config_loaded, "Y");
    ui.msgbox(&msg_text, 8192, &s(title));
}

/// The caption a settings menu item shows after its toggle routine ran (`MENU SET TEXT MenuSettings, id,
/// …`), for the current values; `None` for ids without a state-dependent caption. (The captions the
/// trading desk is created with differ slightly, see CreateTradingDeskDialog.)
pub fn settings_menu_label(g: &Globals, id: i32) -> Option<PbStr> {
    let __content = crate::content::current();
    let on_off = |x: i32| if x == 1 { "On" } else { "Off" };
    let yes_no = |x: i32| if x == 1 { "Yes" } else { "No" };
    Some(match id {
        menu_id::TICKER_SPEED => s(__content.string(10055)) + &str_s(cast::<Ext, _>(g.ticker_speed)),
        menu_id::SELECT_CURRENCY => s(__content.string(10056)) + &g.currency_code + ")",
        menu_id::CHEAT_MODE => s(__content.string(10057)) + if g.cheat_mode == "Y" { "On" } else { "Off" },
        menu_id::SUPPRESS_POPUPS => s(__content.string(10059)) + on_off(g.suppress_popups),
        menu_id::SUPPRESS_EARNINGS => s(__content.string(10060)) + on_off(g.cur_suppress_earnings),
        menu_id::SUPPRESS_CASH_WARN => s(__content.string(10061)) + on_off(g.cur_suppress_cash_warn),
        menu_id::AUTO_SAVE => s(__content.string(10185)) + on_off(g.cur_auto_save),
        menu_id::SWEEP_CASH => s(__content.string(10066)) + yes_no(g.cur_sweep_cash),
        menu_id::EXERCISE_OPTIONS => s(__content.string(10063)) + yes_no(g.cur_exercise_options),
        menu_id::MAKE_DELIVERY => s(__content.string(10064)) + yes_no(g.cur_make_delivery),
        menu_id::TAKE_DELIVERY => s(__content.string(10065)) + yes_no(g.cur_take_delivery),
        menu_id::CHART_SIZE => s(__content.string(10067)) + if g.chart_size == 1 { "Large" } else { "Small" },
        menu_id::AUTO_ADD_STREAM => s(__content.string(10069)) + on_off(g.auto_add_stream_list),
        menu_id::AUTOPILOT_GLOBAL if 0 < g.cur_player => {
            s(__content.string(10070)) + if g.auto_pilot[g.cur_player] == 0 { "Off" } else { "On" }
        }
        menu_id::GROWTH_THROTTLE if 0 < g.cur_player && g.cur_player <= 5 => {
            s(__content.string(10071))
                + &str_trim(cast::<Ext, _>(g.player_growth_throttle[g.cur_player]))
                + "%"
        }
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------- the toggles

/// Port of MenuToggleSuppressPopups (0x40d305).
/// Settings > Suppress Popups: toggles `SuppressPopups` (stored as the current player's
/// `PlayerSuppressPopups`), saves `cfig.wsr` and explains the setting in a box.
pub fn menu_toggle_suppress_popups(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let msg_text;
    if g.suppress_popups == 0 {
        msg_text = s(__content.string(10186))
            + __content.string(10187)
            + __content.string(10188)
            + __content.string(10189)
            + "\r\n"
            + "\r\n"
            + __content.string(10190)
            + __content.string(10191);
        g.suppress_popups = 1; // (UI) MENU SET TEXT 665
    } else {
        msg_text = s(__content.string(10192));
        g.suppress_popups = 0;
    }
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE(&__content), |g, p| g.player_suppress_popups[p] = g.suppress_popups, __content.string(10193));
}

/// Port of MenuToggleSuppressCashWarn (0x40d157).
/// Settings > [runtime resource wording] : toggles `CurSuppressCashWarn` (→ `PlayerSuppressCashWarn`).
pub fn menu_toggle_suppress_cash_warn(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let msg_text;
    if g.cur_suppress_cash_warn == 0 {
        msg_text = s(__content.string(10194))
            + __content.string(10195)
            + __content.string(10196);
        g.cur_suppress_cash_warn = 1;
    } else {
        msg_text = s(__content.string(10197))
            + __content.string(10198)
            + __content.string(10199);
        g.cur_suppress_cash_warn = 0;
    }
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE(&__content), |g, p| g.player_suppress_cash_warn[p] = g.cur_suppress_cash_warn, __content.string(10200));
}

/// Port of MenuToggleSuppressEarnings (0x40d4e0).
/// Settings >  [runtime resource wording] : toggles `CurSuppressEarnings` (→ `PlayerSuppressEarnings`).
pub fn menu_toggle_suppress_earnings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let msg_text;
    if g.cur_suppress_earnings == 0 {
        msg_text = s(__content.string(10201))
            + __content.string(10202);
        g.cur_suppress_earnings = 1;
    } else {
        msg_text = s(__content.string(10203))
            + __content.string(10204);
        g.cur_suppress_earnings = 0;
    }
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE(&__content), |g, p| g.player_suppress_earnings[p] = g.cur_suppress_earnings, __content.string(10205));
}

/// Port of MenuToggleAutoSave (0x40d670).
/// Settings > AutoSave: toggles `CurAutoSave` (→ `PlayerAutoSave`).
pub fn menu_toggle_auto_save(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let msg_text;
    if g.cur_auto_save == 0 {
        g.cur_auto_save = 1;
        msg_text = s(__content.string(10206))
            + __content.string(10207);
    } else {
        g.cur_auto_save = 0;
        msg_text = s(__content.string(10208));
    }
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE(&__content), |g, p| g.player_auto_save[p] = g.cur_auto_save, __content.string(10209));
}

/// Port of MenuToggleExerciseOptions (0x40d7f1).
/// Settings > Exercise Options?: toggles `CurExerciseOptions` (→ `PlayerExerciseOptions`).
pub fn menu_toggle_exercise_options(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let mut msg_text;
    if g.cur_exercise_options == 0 {
        g.cur_exercise_options = 1;
        msg_text = s(__content.string(10210))
            + __content.string(10211);
    } else {
        g.cur_exercise_options = 0;
        msg_text = s(__content.string(10212))
            + __content.string(10213);
    }
    if g.cur_exercise_options == 1 {
        msg_text = msg_text
            + __content.string(10214)
            + __content.string(10215)
            + __content.string(10216);
    }
    toggle_tail(
        g,
        ui,
        env,
        msg_text,
        __content.string(10217),
        |g, p| g.player_exercise_options[p] = g.cur_exercise_options,
        __content.string(10218),
    );
}

/// Port of MenuToggleMakeDelivery (0x40dc69).
/// Settings > Make Physical Delivery?: toggles `CurMakeDelivery` (→ `PlayerMakeDelivery`).
pub fn menu_toggle_make_delivery(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let mut msg_text = s(__content.string(10219))
        + "\r"
        + "\r"
        + __content.string(10220)
        + __content.string(10221)
        + __content.string(2644);
    let detail_text;
    if g.cur_make_delivery == 0 {
        g.cur_make_delivery = 1;
        detail_text = s(__content.string(10222))
            + __content.string(10223)
            + __content.string(10224)
            + "\r"
            + "\r"
            + __content.string(10225);
    } else {
        g.cur_make_delivery = 0;
        msg_text = replace(msg_text.as_bytes(), __content.bytes(10226), __content.bytes(10227));
        detail_text = s(__content.string(10228))
            + __content.string(10229);
    }
    msg_text = msg_text + &detail_text;
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE_THE(&__content), |g, p| g.player_make_delivery[p] = g.cur_make_delivery, __content.string(10230));
}

/// Port of MenuToggleTakeDelivery (0x40def1).
/// Settings > Take Physical Delivery?: toggles `CurTakeDelivery` (→ `PlayerTakeDelivery`).
pub fn menu_toggle_take_delivery(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let mut index_note = PbStr::new();
    if g.cur_take_delivery == 0 {
        index_note = s(__content.string(10231));
    }
    let mut msg_text = s(__content.string(10232))
        + "\r"
        + "\r"
        + __content.string(10233)
        + &index_note
        + ", "
        + __content.string(10234);
    let detail_text;
    if g.cur_take_delivery == 0 {
        g.cur_take_delivery = 1;
        detail_text = s(__content.string(10235))
            + __content.string(10236)
            + __content.string(10237)
            + __content.string(10238)
            + "\r"
            + "\r"
            + __content.string(10239)
            + "\r"
            + "\r"
            + __content.string(10240)
            + __content.string(10241);
    } else {
        g.cur_take_delivery = 0;
        msg_text = replace(msg_text.as_bytes(), __content.bytes(10226), __content.bytes(10227));
        msg_text = replace(msg_text.as_bytes(), __content.bytes(10242), __content.bytes(10243));
        detail_text = s(__content.string(10244))
            + __content.string(10245);
    }
    msg_text = msg_text + &detail_text;
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE_THE(&__content), |g, p| g.player_take_delivery[p] = g.cur_take_delivery, __content.string(10246));
}

/// Port of MenuToggleAutoAddStream (0x40e591).
/// Settings > AutoAdd To StreamList: toggles `AutoAddStreamList` (→ `PlayerAutoAddStream`).
pub fn menu_toggle_auto_add_stream(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let msg_text;
    if g.auto_add_stream_list == 0 {
        g.auto_add_stream_list = 1;
        msg_text = s(__content.string(10247))
            + __content.string(10248)
            + __content.string(10249);
    } else {
        g.auto_add_stream_list = 0;
        msg_text = s(__content.string(10250))
            + __content.string(10251);
    }
    toggle_tail(g, ui, env, msg_text, OVERRIDE_NOTE(&__content), |g, p| g.player_auto_add_stream[p] = g.auto_add_stream_list, __content.string(10252));
}

/// Port of MenuToggleChartSize (0x40e21c).
/// Settings > Stock Chart Size: toggles `ChartSize` between 1 (large) and 2 (small) (anything else
/// counts as large), saves `cfig.wsr`, says so in a quick message and, when switching to small, reopens
/// the price chart (`ShowPriceChart(0)`).
pub fn menu_toggle_chart_size(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    if g.chart_size < 1 || 2 < g.chart_size {
        g.chart_size = 2;
    }
    let size_text;
    if g.chart_size == 1 {
        g.chart_size = 2;
        size_text = "Small";
    } else {
        g.chart_size = 1;
        size_text = "Large";
    }
    let msg_text = s(__content.string(10253))
        + size_text
        + __content.string(10254);
    save_cfg(g, env);
    set(&mut g.config_loaded, "Y");
    quick(g, ui, env, msg_text);
    // (UI) MENU SET TEXT MenuSettings, 672, "Stock Chart Si&ze: " + sizeText
    if size_text != "Large" {
        if !env.skips(0x6141b1) { crate::logic::info_screens::chart::show_price_chart(g, ui, rng, env.now); }
    }
}

/// Port of MenuToggleCheatMode (0x40cb57).
/// Settings > Cheat Mode: `CheatMode` "N" → "Y"; "Y" → "N" unless a cheat scenario is pending
/// (`ScenarioNum`, `MergerTipCorp`, `InsiderEarnTipCorp`) or no game is running (error box, no change;
/// `ScenarioTag` is cleared when turned off); any other value → "N". Then closes the dialogs and
/// explains the setting in a quick message.
pub fn menu_toggle_cheat_mode(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    if g.cheat_mode == "N" {
        set(&mut g.cheat_mode, "Y");
    } else if g.cheat_mode == "Y" {
        if g.scenario_num != 0 || g.merger_tip_corp != 0 || g.insider_earn_tip_corp != 0 || g.game_in_progress != 1 {
            let mut msg_text = PbStr::new();
            if g.game_in_progress == 1 {
                msg_text = s(__content.string(10255));
            }
            let t = s(__content.string(10256)) + "\r" + "\r" + &msg_text;
            let title = g.app_title.clone();
            ui.msgbox(&t, 8208, &title);
            return;
        }
        set(&mut g.cheat_mode, "N");
        g.scenario_tag = PbStr::new();
    } else {
        // (UI) MENU SET TEXT 652, "&Cheat Mode is: On" (sic: the caption says On, the mode becomes "N")
        set(&mut g.cheat_mode, "N");
    }
    let state = if g.cheat_mode == "Y" { "ON" } else { "OFF" };
    let mut msg_text = s(__content.string(10257)) + state;
    if g.cheat_mode == "Y" {
        msg_text = msg_text
            + __content.string(10258)
            + "\r\n"
            + "\r\n"
            + __content.string(10259)
            + __content.string(10260)
            + __content.string(10261)
            + __content.string(10262)
            + "\r\n"
            + "\r\n"
            + __content.string(10263)
            + __content.string(10264)
            + __content.string(10265);
    } else {
        msg_text = msg_text + __content.string(10266);
    }
    if !env.skips(0x493e2b) {
        crate::logic::economy::close_all_dialogs_skipping(g, ui, rng, 1, env);
    }
    quick(g, ui, env, msg_text);
}

// ------------------------------------------------------------------------------ AutoPilot / growth

/// Port of MenuToggleAutoPilotGlobal (0x40e734).
/// Settings > AutoPilot (Global Setting): during a game, after a YES/NO confirmation, flips the
/// current player's `AutoPilot` and copies it to every active company the player controls (except the
/// company the player is CEO of, which is set to 0). Outside a game (or after its end) an error box.
pub fn menu_toggle_auto_pilot_global(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    if g.cur_player != 0 && g.game_in_progress != 0 && g.game_end_flag != "Y" {
        let cp = g.cur_player;
        let status_text = if g.auto_pilot[cp] == 0 { "OFF" } else { "ON" };
        let mut msg_text = s(__content.string(10267))
            + __content.string(10268)
            + status_text
            + "."
            + "\r"
            + "\r"
            + __content.string(10269);
        let answer = ui.msgbox(&msg_text, 8228, &s(__content.string(10270)));
        if answer != 6 {
            return;
        }
        if g.auto_pilot[cp] == 0 {
            g.auto_pilot[cp] = 1;
            msg_text = s(__content.string(10271))
                + "\r"
                + "\r"
                + __content.string(10272)
                + __content.string(10273)
                + __content.string(10274)
                + "\r"
                + "\r";
            msg_text = msg_text
                + __content.string(10275)
                + __content.string(10276)
                + __content.string(10277)
                + __content.string(10278)
                + "\r"
                + "\r"
                + __content.string(10279)
                + __content.string(10280)
                + "\r"
                + "\r";
            let corp_id = g.player_ceo_of[cp];
            if 10 < corp_id {
                if !env.skips(0x41f71b) {
                    crate::logic::control::update_corp_control(g, ui, rng, corp_id);
                }
                if g.cur_player == g.corp_controller[corp_id] {
                    msg_text = msg_text
                        + __content.string(10281)
                        + &g.ent_name[corp_id]
                        + __content.string(10282)
                        + __content.string(10283);
                }
            }
        } else {
            g.auto_pilot[cp] = 0;
            msg_text = s(__content.string(10284))
                + "\r"
                + "\r"
                + __content.string(10285)
                + __content.string(10286)
                + __content.string(10287)
                + __content.string(10288)
                + __content.string(10289)
                + __content.string(10290);
        }
        msg_text = msg_text
            + "\r"
            + "\r"
            + __content.string(10291)
            + __content.string(10292)
            + __content.string(10293);
        ui.msgbox(&msg_text, 8192, &s(__content.string(10294)));
        let cp = g.cur_player;
        let v = g.auto_pilot[cp];
        for corp_id in 11..=1600 {
            if g.entity_status[corp_id] == 1 && g.cur_player == g.corp_controller[corp_id] {
                g.auto_pilot[corp_id] = v;
            }
        }
        let corp_id = g.player_ceo_of[cp];
        if 10 < corp_id {
            g.auto_pilot[corp_id] = 0;
        }
        return;
    }
    let mut status_text = s(__content.string(10295))
        + __content.string(10296);
    if g.game_end_flag == "Y" {
        status_text = s(__content.string(10297));
    }
    let msg_text = s(__content.string(10298)) + &status_text;
    ui.msgbox(&msg_text, 8208, &s(__content.string(10299)));
}

/// Port of MenuSetGrowthThrottle (0x40edbc).
/// Settings > [runtime resource wording] asks whether to change the current player's
/// `PlayerGrowthThrottle`, then for a rate of -10..60 (repeating on invalid input, cancel = empty
/// answer), explains it and optionally caps the `GrowthRate` of every active non-financial... (sic:
/// industry < 70) company on AutoPilot the player controls at once.
///
/// The original indexes `PlayerGrowthThrottle(CurPlayer)` (1 TO 5) also when no game is running
/// (`CurPlayer = 0`: runtime error 9, bug 434); the port returns without doing anything then.
pub fn menu_set_growth_throttle(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    if g.game_end_flag == "Y" {
        return;
    }
    let cp = g.cur_player;
    if cp < 1 || 5 < cp {
        return; // runtime error 9 in the original (bug 434)
    }
    let q = s("\"");
    let mut msg_text = s(__content.string(10300))
        + &q
        + __content.string(10301)
        + &q
        + __content.string(10302)
        + __content.string(10303)
        + &q
        + __content.string(10304)
        + &q
        + "\r"
        + "\r"
        + __content.string(10305)
        + &str_s(cast::<Ext, _>(g.player_growth_throttle[cp]))
        + __content.string(10306)
        + __content.string(10307)
        + __content.string(10308)
        + "\r"
        + "\r"
        + __content.string(6080);
    let answer = ui.msgbox(&msg_text, 8228, &s(__content.string(10309)));
    if answer == 7 {
        return;
    }
    msg_text = s(__content.string(10310))
        + __content.string(10311);
    loop {
        let default = str_trim(cast::<Ext, _>(g.player_growth_throttle[cp]));
        let input_text = ui.inputbox(&msg_text, &s(__content.string(10312)), &default);
        if input_text.is_empty() {
            break;
        }
        let answer = to_long(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
        if -11 < answer && answer < 61 {
            g.player_growth_throttle[cp] = cast::<i16, _>(answer);
            // (UI) MENU SET TEXT MenuSettings, 676, " [runtime resource wording] " + n + "%"
            if answer < 0 {
                msg_text = s(__content.string(10313))
                    + __content.string(10314)
                    + &str_trim(cast::<Ext, _>(1 + answer))
                    + __content.string(10315)
                    + __content.string(10316)
                    + &str_trim(cast::<Ext, _>(answer))
                    + "%. ";
                if answer != -10 {
                    msg_text = msg_text
                        + __content.string(10317)
                        + &str_trim(cast::<Ext, _>(answer))
                        + __content.string(10318)
                        + __content.string(10319)
                        + __content.string(10320);
                }
            } else {
                msg_text = s(__content.string(10321))
                    + __content.string(10322)
                    + &str_s(cast::<Ext, _>(answer))
                    + "%.";
            }
            msg_text = msg_text
                + "\r"
                + "\r"
                + __content.string(10323)
                + &q
                + __content.string(10301)
                + &q
                + __content.string(10324)
                + __content.string(10325)
                + __content.string(10326)
                + __content.string(10327)
                + __content.string(10328)
                + __content.string(10329);
            let title = s(__content.string(10330)) + &str_s(cast::<Ext, _>(answer)) + "%";
            ui.msgbox(&msg_text, 8256, &title);
            msg_text = s(__content.string(10331))
                + __content.string(10332);
            let answer = ui.msgbox(&msg_text, 8228, &s(__content.string(10333)));
            if answer != 7 {
                for corp_id in 131..=1600 {
                    if g.entity_status[corp_id] == 1
                        && g.industry_num[corp_id] < 70
                        && g.cur_player == g.corp_controller[corp_id]
                        && g.auto_pilot[corp_id] != 0
                    {
                        let t = g.player_growth_throttle[g.cur_player];
                        if t < g.growth_rate[corp_id] {
                            g.growth_rate[corp_id] = t;
                        }
                    }
                }
            }
            break;
        }
        quick(g, ui, env, s(__content.string(10334)));
    }
}

// --------------------------------------------------------------------------- ticker, currency, law

/// Port of MenuTickerSpeed (0x40c88a).
/// Settings > Ticker Speed: asks for 1..100 (repeating with an error message on other numbers; an
/// empty / zero answer cancels), stores `TickerSpeed` and saves `cfig.wsr`. A zero `TickerSpeed` is
/// first set to 50 (also when cancelled).
pub fn menu_ticker_speed(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let mut speed = 0;
    if g.ticker_speed == 0 {
        g.ticker_speed = 50;
    }
    while speed < 1 || 100 < speed {
        let prompt = s(__content.string(10335))
            + __content.string(10336)
            + "\r\n"
            + "\r\n"
            + __content.string(10337)
            + &str_s(cast::<Ext, _>(g.ticker_speed));
        let input_text = ui.inputbox(&prompt, &s(__content.string(10338)), &PbStr::new());
        speed = to_long(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
        if speed == 0 {
            return;
        }
        if speed < 1 || 100 < speed {
            quick(g, ui, env, s(__content.string(10339)));
        }
    }
    g.ticker_speed = speed;
    save_cfg(g, env);
    set(&mut g.config_loaded, "Y");
    // (UI) MENU SET TEXT MenuSettings, 650, "Ticker &Speed:" + STR$(TickerSpeed)
}

/// Port of MenuSelectCurrency (0x40ca39).
/// Settings > Reselect Currency: runs the currency selector ([`run_currency_selector`]) and saves
/// `cfig.wsr`. Once a world is loaded the change only applies to the next game (a note says so);
/// before a game starts `CurrencyCode` takes the selection (`PrefCurrency`) at once.
pub fn menu_select_currency(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    if g.world_generated == 1 {
        let msg_text = s(__content.string(10340))
            + __content.string(10341)
            + __content.string(10342);
        quick(g, ui, env, msg_text);
        if !env.skips(0x41dd39) {
            run_currency_selector(g, ui, rng, env);
        }
        save_cfg(g, env);
    } else {
        if !env.skips(0x41dd39) {
            run_currency_selector(g, ui, rng, env);
        }
        if g.game_in_progress == 0 {
            g.currency_code = g.pref_currency.clone();
            // (UI) MENU SET TEXT MenuSettings, 651, "Reselect Currenc&y: (" + CurrencyCode + ")"
        }
        save_cfg(g, env);
    }
}

/// The currencies of the selector dialog (CURRENCY.EXE): (code written to `TMPDAT.WSR`, caption), in
/// the dialog's order.
#[allow(non_snake_case)]
pub fn CURRENCIES(__content: &crate::content::ContentPack) -> [(&str, &str); 15] { [
    ("USA", __content.string(10343)),
    ("AUS", __content.string(10344)),
    ("BRZ", __content.string(10345)),
    ("UK", __content.string(10346)),
    ("CAN", __content.string(10347)),
    ("CHN", __content.string(10348)),
    ("EUR", __content.string(10349)),
    ("HK", __content.string(10350)),
    ("JAP", __content.string(10351)),
    ("KOR", __content.string(10352)),
    ("MAL", __content.string(10353)),
    ("NZ", __content.string(10354)),
    ("SNG", __content.string(10355)),
    ("SAF", __content.string(10356)),
    ("SWI", __content.string(10357)),
] }

/// Port of RunCurrencySelector (0x41dd39).
/// Runs the external currency picker `CURRENCY.EXE <DataPath>|<CurrencyCode>|<CurrencyName>|` (an empty
/// `CurrencyCode` first becomes "USA" / "U.S. dollar") and reads the selected code from `TMPDAT.WSR`
/// (`INPUT #` → `PrefCurrency`, then KILLed). Before a game the start-up headlines are rebuilt with the
/// new currency (`LoadCurrencyHeadlines`).
///
/// The picker is [`Ui::choose`] over [`CURRENCIES`] (title " [runtime resource wording] "); the port
/// writes the chosen code to `TMPDAT.WSR` as CURRENCY.EXE does (cancel: the current `CurrencyCode`), and
/// the host also gets a [`HostRequest::RunProgram`] with the command line. When the file is missing
/// the original's ON ERROR handler ends the routine (the port returns).
pub fn run_currency_selector(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    if g.currency_code.is_empty() {
        set(&mut g.currency_code, "USA");
        set(&mut g.currency_name, __content.string(10358));
    }
    let shell_args = g.data_path.clone() + "|" + &g.currency_code + "|" + &g.currency_name + "|";
    env.request(HostRequest::RunProgram(g.data_path.clone() + __content.string(10359) + &shell_args));
    let options: Vec<PbStr> = CURRENCIES(&__content).iter().map(|c| s(c.1)).collect();
    let path = g.data_path.clone() + "TMPDAT.WSR";
    let code = match ui.choose(&s(__content.string(10360)), &[], &options) {
        Some(k) => s(CURRENCIES(&__content)[k].0),
        None => g.currency_code.clone(),
    };
    env.files.borrow_mut().write(path.as_bytes(), (code + "\r\n").0);
    // OPEN DataPath + "TMPDAT.WSR" FOR INPUT: INPUT #f, PrefCurrency: CLOSE: KILL
    let data = env.files.borrow().read(path.as_bytes()).cloned();
    let Some(data) = data else { return }; // ON ERROR: runtime error 53
    let mut rd = crate::pb::numeric::InputReader::new(&data);
    let Ok(v) = rd.try_input_str() else { return }; // error 62
    g.pref_currency = PbStr(v);
    env.files.borrow_mut().delete(path.as_bytes());
    if g.game_in_progress == 0 {
        if !env.skips(0x41eddc) {
            crate::logic::news::load_currency_headlines(g);
        }
        if !env.skips(0x41e749) {
            crate::logic::turn::refresh_news_listbox(g);
        }
    }
}

/// Port of UpdateCurrencyMenuText (0x47d570).
/// The Settings menu caption for the currency of a running game, "Currenc&y (This game): " + the
/// currency's name for `CurrencyCode` (`CurrencyName` for codes without a fixed name). Pure UI (menu
/// item 651): returns the caption, no game state changes.
pub fn update_currency_menu_text(g: &Globals) -> PbStr {
    let __content = crate::content::current();
    let label = match g.currency_code.as_bytes() {
        b"BRZ" => s(__content.string(10361)),
        b"CHN" => s(__content.string(10362)),
        b"JAP" => s("The Yen"),
        b"EUR" => s(__content.string(10363)),
        b"KOR" => s(__content.string(10364)),
        b"UK" => s(__content.string(10365)),
        b"MAL" => s(__content.string(10366)),
        b"SAF" => s(__content.string(10367)),
        b"SWI" => s(__content.string(10368)),
        b"USA" => s(__content.string(10369)),
        _ => g.currency_name.clone(),
    };
    s(__content.string(10370)) + &label
}

/// Port of MenuSelectLawFirm (0x40ced1).
/// Settings > Select Law Firm: `PlayerLawFirm(CurPlayer) = LawFirm(PlayerLawFirm(CurPlayer),
/// CurrencySymbol, MillionPrefixUc)`, a dialog in `lawdll.dll` (cheap / average / expensive firm,
/// 1..3).
///
/// Uses LAWDLL's selection loop and time-dependent confirmation through [`Ui::choose`].
/// Outside a game (`CurPlayer` 0) the original indexes
/// `PlayerLawFirm(0)` (runtime error 9, bug 434); the port does nothing then.
pub fn menu_select_law_firm(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let _ = (rng, env);
    let cp = g.cur_player;
    if cp < 1 || 5 < cp {
        return;
    }
    let firm = crate::logic::extensions::law_firm(ui, g.player_law_firm[cp], &g.currency_symbol, &g.million_prefix_uc);
    g.player_law_firm[cp] = firm;
}

/// Port of MenuHighScore (0x40cf78).
/// Game Options > High Score: `HiScore(-1, "2", DataPath)` in VARICHEK.DLL shows the high score;
/// a result of -1..100 means there is none: " [runtime resource wording] " (quick message).
///
/// Reads the record through `extensions::high_score`, preserving the original's missing-file error.
pub fn menu_high_score(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let data = env.files.borrow().read((g.data_path.clone() + "SCORES.WSR").as_bytes()).cloned();
    let score_result = cast::<i64, _>(crate::logic::extensions::high_score::HighScores::default()
        .call(ui, data.as_deref(), &g.data_path, &s("2"), -1).value);
    if score_result <= 100 && -1 <= score_result {
        quick(g, ui, env, s(__content.string(10371)));
    }
}

/// Port of MenuClearChartHistory (0x40e372).
/// Settings > Clear Chart History: for the selected entity (`SelEntity`), after a YES/NO question,
/// clears its price (or, for the current player, net worth) history (`ClearPriceHistory`). Without a
/// valid selection a quick message.
pub fn menu_clear_chart_history(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let entity_id = g.sel_entity;
    let mut msg_text;
    if g.cur_player == g.sel_entity {
        msg_text = s(__content.string(10372));
    } else {
        if g.sel_entity == 0 || 1600 < g.sel_entity {
            quick(g, ui, env, s(__content.string(10373)));
            return;
        }
        msg_text = s(__content.string(10374))
            + &g.ent_name[entity_id]
            + "?"
            + "\r"
            + "\r"
            + __content.string(10375)
            + __content.string(10376)
            + __content.string(10377);
    }
    let title = s(__content.string(10378)) + &g.ent_name[entity_id] + "?";
    let answer = ui.msgbox(&msg_text, 8484, &title);
    if answer != 7 {
        if !env.skips(0x4a389b) {
            crate::logic::valuation::clear_price_history(g, entity_id);
        }
        msg_text = s(__content.string(10379));
        if entity_id < 11 {
            msg_text = PbStr::new();
        }
        quick(g, ui, env, s("Done.") + &msg_text);
    }
}

// ------------------------------------------------------------------------------- new game / exit

/// Port of MenuNewGame (0x40a74a).
/// File > New Game. Deletes `GTIME.DAT` on Windows XP. During a game the program restarts itself: it
/// sets `GameEndFlag = "Y"`, closes the dialogs, writes `RESTART.DAT` and launches `<ExeName> 1`
/// ([`HostRequest::RunProgram`]) and ends ([`HostRequest::Exit`]) — except under a non-XP `INSTALL` run.
/// Otherwise it picks the first free game number 1..50 (`GAMExx.DAT` missing or a `@DUMMY` placeholder,
/// which is deleted on XP / INSTALL runs; 50 at most), sets `StockIndexBase = 5000`, runs
/// `NewGameSetup` and, if a game started, copies the current settings to the human players (fixed ones
/// for computer players), revises the earnings estimates, refreshes the market and starts the ticker
/// (`TickerOn`, `ClockState = "RUN"`, `RunTickerLoop`).
pub fn menu_new_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let mut file_path = g.data_path.clone() + "GTIME.DAT";
    if exists(env, &file_path) && g.win_version == "XP" {
        env.files.borrow_mut().delete(file_path.as_bytes());
    }
    if g.game_in_progress == 1 {
        set(&mut g.game_end_flag, "Y");
        if !env.skips(0x493e2b) {
            crate::logic::economy::close_all_dialogs_skipping(g, ui, rng, 1, env);
        }
        if g.win_version != "XP" && g.cmd_line_mode == "INSTALL" {
            return;
        }
        let p = g.data_path.clone() + "RESTART.DAT";
        env.files.borrow_mut().write(p.as_bytes(), b"\"xxx\"\r\n".to_vec());
        // (UI) DIALOG SHOW STATE hDlgMain, 0
        env.request(HostRequest::RunProgram(g.data_path.clone() + &g.exe_name + " 1"));
        // DIALOG END hDlgMain: the program ends
        env.request(HostRequest::Exit);
        return;
    }
    let mut game_num = 0;
    loop {
        game_num += 1;
        let mut game_num_str = str_trim(cast::<Ext, _>(game_num));
        if game_num_str.len() == 1 {
            game_num_str = s("0") + &game_num_str;
        }
        file_path = g.data_path.clone() + "GAME" + &game_num_str + ".DAT";
        let mut file_found = exists(env, &file_path);
        if file_found {
            let data = env.files.borrow().read(file_path.as_bytes()).cloned().unwrap_or_default();
            let first_line = crate::pb::numeric::InputReader::new(&data).try_line_input().unwrap_or_default();
            if 0 < instr(1, &first_line, b"@DUMMY") {
                file_found = false;
                if g.win_version == "XP" || g.cmd_line_mode == "INSTALL" {
                    env.files.borrow_mut().delete(file_path.as_bytes());
                }
            }
        }
        if game_num == 50 {
            file_found = false;
        }
        if !file_found {
            break;
        }
    }
    g.game_number = game_num;
    g.stock_index_base = 5000;
    if !env.skips(0x473e5b) {
        let snapshot = env.files.borrow().clone();
        let files = |n: &str| snapshot.read(n.as_bytes()).cloned();
        let written = crate::logic::newgame::new_game_setup_with_options(g, ui, rng, &files, env.now,
            crate::logic::newgame::NewGameOptions { repair_initial_shares: env.repair_initial_shares });
        for (name, data) in written {
            env.files.borrow_mut().write(name.as_bytes(), data);
        }
    }
    if g.game_in_progress != 0 {
        let n = g.num_players;
        for p in 1..=n {
            if g.is_computer_player[p] == 1 {
                g.player_suppress_popups[p] = 1;
                g.player_suppress_earnings[p] = 1;
                g.player_suppress_cash_warn[p] = 1;
                g.player_auto_save[p] = 0;
                g.player_auto_add_stream[p] = 0;
                g.player_exercise_options[p] = 1;
                g.player_make_delivery[p] = 1;
                g.player_take_delivery[p] = 0;
                g.sweep_cash_to_loan[p] = 0;
            } else {
                g.player_suppress_popups[p] = g.suppress_popups;
                g.player_suppress_earnings[p] = g.cur_suppress_earnings;
                g.player_suppress_cash_warn[p] = g.cur_suppress_cash_warn;
                g.player_auto_save[p] = g.cur_auto_save;
                g.player_exercise_options[p] = g.cur_exercise_options;
                g.player_make_delivery[p] = g.cur_make_delivery;
                g.player_take_delivery[p] = g.cur_take_delivery;
                g.sweep_cash_to_loan[p] = g.cur_sweep_cash;
                g.player_auto_add_stream[p] = g.auto_add_stream_list;
                g.player_growth_throttle[p] = 60;
            }
        }
        if !env.skips(0x473796) {
            crate::logic::turn::update_settings_menu_text(g);
        }
        if !env.skips(0x4b8397) {
            crate::logic::corp_ai::capital::revise_earnings_estimates(g, ui, rng);
        }
        // the chart hint text built here (" [runtime resource wording] …") is not shown
        if !env.skips(0x4400d7) {
            crate::logic::market::update_market_display(g, ui, rng, env.now.time_secs());
        }
        // (UI) CONTROL SET TEXT 249, "TICKER ON/OFF"
        g.ticker_on = 1;
        set(&mut g.clock_state, "RUN");
        if !env.skips(0x41d9c2) {
            super::app::run_ticker_loop(g, ui, rng, env);
        }
        if !env.skips(0x424754) {
            crate::logic::turn::enable_main_controls(g);
        }
    }
}

/// Port of MenuExitGame (0x40c6c2).
/// File > Exit: asks " [runtime resource wording] " (not when the game is over). YES closes every
/// dialog, sets `ChartEntity = 0` and ends the program ([`HostRequest::Exit`]); NO stops a running
/// ticker, resets `GameEndFlag`/`TickerStopReq` to "N" and runs a ticker batch (`TickerSimSteps(0)`).
pub fn menu_exit_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let answer;
    if g.game_end_flag == "Y" {
        answer = 6;
    } else {
        set(&mut g.game_end_flag, "Y");
        set(&mut g.ticker_stop_req, "Y");
        let title = g.app_title.clone();
        answer = ui.msgbox(&s(__content.string(10129)), 8484, &title);
    }
    if answer == 6 {
        // (UI) DIALOG END of every dialog
        g.chart_entity = 0;
        env.request(HostRequest::Exit); // DIALOG END hDlgMain
    } else {
        if g.clock_state == "RUN" {
            set(&mut g.clock_state, "STOP");
            g.ticker_on = 0;
        }
        set(&mut g.game_end_flag, "N");
        set(&mut g.ticker_stop_req, "N");
        if !env.skips(TICKER_SIM_STEPS) {
            crate::logic::turn::ticker_sim_steps_skipping(g, ui, rng, 0, env.now, env.skip);
        }
    }
}

/// Port of MenuSaveGameAs (0x40c685): File > Save Game As = `SaveGamePrompt(1)`.
pub fn menu_save_game_as(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if !env.skips(0x40c01b) { super::app::save_game_mode(g, ui, rng, env, 1); }
}

// ------------------------------------------------------------------------ help, about, web pages

/// Port of ShowHelp (0x40f506): starts the help program `<DataPath>WSRHELP.EXE <DataPath>`
/// ([`HostRequest::RunProgram`]).
pub fn show_help(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = (ui, rng);
    env.request(HostRequest::RunProgram(g.data_path.clone() + __content.string(10380) + &g.data_path));
}

/// Port of ShowAboutBox (0x40f571): the copyright / version / licensee box.
pub fn show_about_box(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let year = right(date_str(env.now).as_bytes(), 4);
    let mut msg_text = s(__content.string(10381))
        + &PbStr(vec![169])
        + " 1986-"
        + &year
        + "\r"
        + "\r"
        + __content.string(10382)
        + "\r"
        + __content.string(10383)
        + "\r"
        + __content.string(10384)
        + "\r"
        + "\r"
        + __content.string(10385)
        + "\r"
        + "\r"
        + __content.string(10386)
        + &g.version_str;
    if !g.licensee_name.is_empty() {
        msg_text = msg_text
            + "\r"
            + "\r"
            + __content.string(10387)
            + "\r"
            + "        "
            + &g.licensee_name
            + "\r"
            + "\r"
            + __content.string(10388)
            + &g.registration_num;
    }
    let title = g.app_title.clone();
    ui.msgbox(&msg_text, 8192, &title);
}

/// Port of PromoSpeculator (0x40f8be).
/// Game Options > Speculator: advertises the sister game and, on YES, opens its order page (odd
/// "month") or its download page (even). The "month" is `VAL(MID$(DATE$, 4, 2))`, which is the day of
/// the month (bug 433).
pub fn promo_speculator(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = (g, rng);
    let month_num = to_long(val(mid(date_str(env.now).as_bytes(), 4, 2).as_bytes()));
    let mut msg_text = s(__content.string(10389))
        + __content.string(10390)
        + __content.string(10391)
        + __content.string(10392)
        + __content.string(10393)
        + __content.string(10394)
        + "\r"
        + "\r"
        + __content.string(10395)
        + __content.string(10396)
        + __content.string(10397)
        + __content.string(10398)
        + __content.string(10399)
        + "\r"
        + "\r"
        + __content.string(10400)
        + __content.string(10401)
        + __content.string(10402)
        + "\r"
        + "\r";
    msg_text = msg_text + __content.string(10403);
    if month_num % 2 == 1 {
        msg_text = msg_text + __content.string(10404);
    } else {
        msg_text = msg_text + __content.string(10405);
    }
    let answer = ui.msgbox(&msg_text, 8227, &s(__content.string(10406)));
    if answer == 6 {
        if month_num % 2 == 1 {
            open_url(env, s(__content.string(14458)));
        } else {
            open_url(env, s(__content.string(14463)));
        }
    }
}

/// Stops a running ticker before leaving for a web page (`TickerOn = 1` → `ClockState = "STOP"`,
/// `TickerOn = 0`; the ticker dialog is ended).
fn stop_ticker(g: &mut Globals) {
    if g.ticker_on == 1 {
        set(&mut g.clock_state, "STOP");
        // (UI) DIALOG END RunLoopDlg
        g.ticker_on = 0;
    }
}

/// Port of OpenStrategyManual (0x40fb76).
/// Opens the strategy manual `WSRBOOK.DOC` (or `WSR_PDF.PDF`) from the data directory, with a quick
/// message first. When neither exists the original opens the bare `DataPath` (sic).
pub fn open_strategy_manual(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    let mut file_name = PbStr::new();
    let mut file_desc = PbStr::new();
    if !exists(env, &(g.data_path.clone() + "WSRBOOK.DOC")) {
        if exists(env, &(g.data_path.clone() + "WSR_PDF.PDF")) {
            file_name = s("WSR_PDF.PDF");
            file_desc = s(__content.string(10407));
        }
    } else {
        file_name = s("WSRBOOK.DOC");
        file_desc = s(__content.string(10408));
    }
    quick(g, ui, env, s(__content.string(10409)) + &file_desc);
    open_url(env, g.data_path.clone() + &file_name);
}

/// Port of OpenTutorialPage (0x4103cc): Game Options > Online Tutorial (stops the ticker, quick
/// message, opens ` [runtime resource wording] `).
pub fn open_tutorial_page(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    stop_ticker(g);
    let msg_text = s(__content.string(10410))
        + __content.string(10411)
        + "\r\n"
        + "\r\n"
        + __content.string(10412);
    quick(g, ui, env, msg_text);
    open_url(env, s(__content.string(14462)));
}

/// Port of OpenOrderManualPage (0x40f7ba): Game Options > View / Buy W$R Manual (stops the ticker;
/// opens `WSRBOOK.HTM` from the data directory if present, else the order page).
pub fn open_order_manual_page(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = (ui, rng);
    let file_path = g.data_path.clone() + "WSRBOOK.HTM";
    let found = exists(env, &file_path);
    stop_ticker(g);
    if !found {
        open_url(env, s(__content.string(14461)));
    } else {
        open_url(env, file_path);
    }
}

/// Port of OpenForumPage (0x4102f0): Game Options > W$R Forum (stops the ticker, quick message, opens
/// ` [runtime resource wording] `).
pub fn open_forum_page(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    stop_ticker(g);
    let msg_text = s(__content.string(10413))
        + "\r\n"
        + "\r\n"
        + __content.string(10412);
    quick(g, ui, env, msg_text);
    open_url(env, s(__content.string(14460)));
}

/// Port of MenuUpgrades (0x4102b3): Game Options > Upgrades = `OpenUpdatesPage(2)`.
pub fn menu_upgrades(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if !env.skips(0x410144) {
        open_updates_page(g, ui, rng, env, 2);
    }
}

/// Port of OpenUpdatesPage (0x410144).
/// Stops the ticker and opens the updates page (`page_mode` 1: updates.htm, else updates2.htm); in mode
/// 2 (Upgrades) a registered copy also shows its registration.
pub fn open_updates_page(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, page_mode: i32) {
    let __content = crate::content::current();
    let _ = rng;
    stop_ticker(g);
    if page_mode == 1 {
        open_url(env, s(__content.string(14459)));
    } else {
        open_url(env, s(__content.string(14458)));
    }
    if page_mode == 2 && !g.licensee_name.is_empty() {
        let t = s(__content.string(10414))
            + &g.licensee_name
            + "\r"
            + "\r"
            + __content.string(10415)
            + &g.registration_num;
        ui.msgbox(&t, 4096, &s(__content.string(10416)));
    }
}

/// Port of MenuCustomizer (0x40d06c).
/// Game Options > Customizer Utility: if `CUSTOMIZ.EXE` exists, runs it ([`HostRequest::RunProgram`]
/// `<DataPath>CUSTOMIZ.EXE <DataPath>`), pauses the ticker and, before a game, reloads the company
/// names (`LoadCorpNames` from `CORPNAME.DAT`).
pub fn menu_customizer(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let _ = rng;
    if !exists(env, &(g.data_path.clone() + "CUSTOMIZ.EXE")) {
        return;
    }
    env.request(HostRequest::RunProgram(g.data_path.clone() + __content.string(10417) + &g.data_path));
    if !env.skips(0x46e001) {
        crate::logic::turn::pause_ticker(g, &ui.local_time(), 1.0f32);
    }
    if g.game_in_progress == 0 && !env.skips(0x42230e) {
        let p = g.data_path.clone() + "CORPNAME.DAT";
        let data = env.files.borrow().read(p.as_bytes()).cloned();
        if let Some(d) = data {
            let _ = crate::save::load_corp_names(g, &d);
        }
    }
}

/// Port of ShellOpenUrl (0x5c301a): `ShellExecute(0, "Open", url, "", 0, 5)` →
/// [`HostRequest::OpenUrl`].
pub fn shell_open_url(env: &Env, url: &PbStr) {
    env.request(HostRequest::OpenUrl(url.clone()));
}

/// Port of OpenOrderWebPage (0x5c30e5): opens ` [runtime resource wording] `.
pub fn open_order_web_page(env: &Env) {
    let __content = crate::content::current();
    open_url(env, s(__content.string(14457)));
}

/// Port of OpenKedianomicsSite (0x4104d2): button callback opening ` [runtime resource wording] `.
pub fn open_kedianomics_site(env: &Env) {
    let __content = crate::content::current();
    open_url(env, s(__content.string(14456)));
}
