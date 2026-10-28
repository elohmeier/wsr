//! The main window, the "Trading Desk": `CreateTradingDeskDialog` (0x402844), its dialog procedure
//! `DlgProcTradingDesk` (0x405a7c, the menu bar and window close), the main-window button callbacks
//! (`Cb*`, see [`callbacks`]).
//!
//! # Host API
//!
//! * [`create_trading_desk_dialog`]: the state effects of building the main window (call once when
//!   the main window is shown, after a new game / load). It does not run the original's modal loop:
//!   the host runs its own event loop and calls the functions below.
//! * [`desk_menu_bar`]: the menu bar (popup titles, item ids, captions, grayed state) as the original
//!   builds it, computed from the current state; [`desk_controls`]: the main window's controls with
//!   their ids, kinds, initial captions and the callback a click runs.
//! * [`desk_command`]`(g, ui, rng, env, id)`: a menu item was chosen (ids in [`cmd`]).
//! * [`desk_close`]: the window's close box / Alt+F4 (" [runtime resource wording] ").
//! * [`desk_click`]`(g, ui, rng, env, ctl, notify)`: a main-window control was clicked (ids in [`ctl`];
//!   `notify` is the Win32 notification code, 0 = click, 1 = double click for the logo). The
//!   individual callbacks are public in [`callbacks`] as well.
//! * [`control_colors`]: the (text, background) palette indices of a main-window control.
use crate::pb::real::{Ext, cast};



pub mod callbacks;
pub use callbacks::*;

#[allow(unused_imports)]
use super::prelude::*;


use super::app::{file_exists, update_file_menu_state};
use super::settings::*;

/// Menu item ids of the main window's menu bar (`WM_COMMAND` ids of [`desk_command`]).
pub mod cmd {

    /// File > New Game (`MenuNewGame`).
    pub const NEW_GAME: i32 = 605;
    /// File > Open Saved Game (`MenuOpenSavedGame`); the host supplies its file picker.
    pub const OPEN_SAVED_GAME: i32 = 610;
    /// File > Save Game (`SaveGamePrompt(0)` through `app::save_game`).
    pub const SAVE_GAME: i32 = 615;
    /// File > Save Game As (`MenuSaveGameAs`).
    pub const SAVE_GAME_AS: i32 = 620;
    /// File menu separator.
    pub const FILE_SEPARATOR: i32 = 625;
    /// File > Exit (`MenuExitGame`).
    pub const EXIT: i32 = 630;
    /// Settings > Ticker Speed (`MenuTickerSpeed`).
    pub const TICKER_SPEED: i32 = 650;
    /// Settings > Reselect Currency (`MenuSelectCurrency`).
    pub const SELECT_CURRENCY: i32 = 651;
    /// Settings > Cheat Mode (`MenuToggleCheatMode`).
    pub const CHEAT_MODE: i32 = 652;
    /// Settings > Select Law Firm (`MenuSelectLawFirm`).
    pub const LAW_FIRM: i32 = 653;
    /// Game Options > High Score (`MenuHighScore`).
    pub const HIGH_SCORE: i32 = 654;
    /// Game Options > Customizer Utility (`MenuCustomizer`).
    pub const CUSTOMIZER: i32 = 655;
    /// Game Options > View/Buy W$R Manual (`OpenOrderManualPage`).
    pub const MANUAL: i32 = 656;
    /// Game Options >  [runtime resource wording]  (`PromoSpeculator`).
    pub const SPECULATOR: i32 = 657;
    /// Strategy manual (`OpenStrategyManual`; dispatched, but not on the menu).
    pub const STRATEGY_MANUAL: i32 = 658;
    /// Game Options > Updates (`OpenUpdatesPage(1)`).
    pub const UPDATES: i32 = 659;
    /// Game Options > Upgrades (`MenuUpgrades`).
    pub const UPGRADES: i32 = 660;
    /// Game Options > W$R Forum (`OpenForumPage`).
    pub const FORUM: i32 = 661;
    /// Game Options > Online Tutorial (`OpenTutorialPage`).
    pub const TUTORIAL: i32 = 662;
    /// Settings menu separator.
    pub const SETTINGS_SEPARATOR: i32 = 663;
    /// Settings > [runtime resource wording] (`MenuToggleSuppressCashWarn`).
    pub const SUPPRESS_CASH_WARN: i32 = 664;
    /// Settings > Suppress Popups (`MenuToggleSuppressPopups`).
    pub const SUPPRESS_POPUPS: i32 = 665;
    /// Settings >  [runtime resource wording] (`MenuToggleSuppressEarnings`).
    pub const SUPPRESS_EARNINGS: i32 = 666;
    /// Settings > AutoSave (`MenuToggleAutoSave`).
    pub const AUTO_SAVE: i32 = 667;
    /// Settings > [runtime resource wording]  (`MenuToggleSweepCash`).
    pub const SWEEP_CASH: i32 = 668;
    /// Settings > Exercise Options (`MenuToggleExerciseOptions`).
    pub const EXERCISE_OPTIONS: i32 = 669;
    /// Settings > [runtime resource wording] (`MenuToggleMakeDelivery`).
    pub const MAKE_DELIVERY: i32 = 670;
    /// Settings > [runtime resource wording] (`MenuToggleTakeDelivery`).
    pub const TAKE_DELIVERY: i32 = 671;
    /// Settings > Stock Chart Size (`MenuToggleChartSize`).
    pub const CHART_SIZE: i32 = 672;
    /// Settings > Clear Chart History (`MenuClearChartHistory`).
    pub const CLEAR_CHART_HISTORY: i32 = 673;
    /// Settings > AutoAdd To StreamList (`MenuToggleAutoAddStream`).
    pub const AUTO_ADD_STREAM: i32 = 674;
    /// Settings > AutoPilot (Global Setting) (`MenuToggleAutoPilotGlobal`).
    pub const AUTOPILOT: i32 = 675;
    /// Settings > [runtime resource wording] (`MenuSetGrowthThrottle`).
    pub const GROWTH_THROTTLE: i32 = 676;
    /// Help > [runtime resource wording] (`ShowHelp`).
    pub const HELP: i32 = 678;
    /// Help > Registration Info (`ShowAboutBox(2)`).
    pub const REGISTRATION: i32 = 679;
    /// Help > About (`ShowAboutBox(1)`).
    pub const ABOUT: i32 = 680;
}

/// Control ids of the main window (see [`desk_controls`]).
pub mod ctl {

    /// "Go" (symbol box entry).
    pub const GO: i32 = 1;
    pub const ENTITY_INFO: i32 = 102;
    pub const GENERAL: i32 = 104;
    pub const BUY_STOCK: i32 = 105;
    pub const SELL_STOCK: i32 = 106;
    pub const BUY_SELL_MENU: i32 = 107;
    pub const FINANCING_MENU: i32 = 108;
    pub const MANAGEMENT_MENU: i32 = 109;
    pub const OTHER_TRANS_MENU: i32 = 110;
    /// Next earnings date label.
    pub const NEXT_EARNINGS: i32 = 120;
    pub const LIST_FUTURES: i32 = 125;
    pub const RECALL_DB_SEARCH: i32 = 126;
    pub const EARNINGS_REPORT: i32 = 127;
    pub const LIST_PORTFOLIO: i32 = 128;
    pub const LIST_SHAREHOLDERS: i32 = 129;
    pub const RESEARCH_REPORT: i32 = 130;
    pub const FINANCIAL_PROFILE: i32 = 131;
    pub const LIST_OPTIONS: i32 = 132;
    pub const MY_CORPORATIONS: i32 = 134;
    pub const PLAYER_LABEL: i32 = 140;
    pub const DATE_LABEL: i32 = 141;
    pub const TRANSACTIONS_LEFT: i32 = 112;
    pub const CHART_STOCK_INDEX: i32 = 114;
    pub const CHART_PRIME_RATE: i32 = 152;
    pub const CHART_GDP_GROWTH: i32 = 153;
    pub const CHART_CRUDE: i32 = 154;
    pub const CHART_LONG_BOND: i32 = 157;
    pub const CHART_SHORT_BOND: i32 = 158;
    pub const CHART_GOLD: i32 = 159;
    pub const CHART_SILVER: i32 = 161;
    pub const CHART_WHEAT: i32 = 163;
    pub const CHART_CORN: i32 = 165;
    pub const SELECT_CORP: i32 = 168;
    pub const SELECTED_NAME: i32 = 169;
    pub const SELECT_PLAYER: i32 = 170;
    pub const SELECT_LAST: i32 = 173;
    pub const MY_NEWS: i32 = 175;
    pub const CONTROLLER_LABEL: i32 = 181;
    pub const ACTING_LABEL: i32 = 190;
    pub const SYMBOL_BOX: i32 = 202;
    pub const NEWS_LIST: i32 = 205;
    pub const NET_WORTH: i32 = 218;
    pub const INDUSTRY_COMBO: i32 = 234;
    pub const END_TURN: i32 = 244;
    pub const MISC_MENU: i32 = 246;
    pub const CHART: i32 = 247;
    pub const CHEAT: i32 = 248;
    pub const TICKER_ON_OFF: i32 = 249;
    pub const TICKER_TEXT: i32 = 123;
    /// The logo (double click opens the cheat menu).
    pub const LOGO: i32 = 119;
    pub const STREAM_ADD_DELETE: i32 = 951;
    pub const STREAM_CLEAR: i32 = 953;
    pub const STREAM_FILL: i32 = 954;
    /// First streaming-quote line (801..815, slot = id - 800).
    pub const STREAM_QUOTE_FIRST: i32 = 801;
    pub const STREAM_QUOTE_LAST: i32 = 815;
}

/// Enabled state of EnableMainControls (0x424754). These constraints originally lived in
/// Windows controls; browser hosts must apply them before dispatching an action too.
pub fn control_enabled(g: &Globals, control: i32) -> bool {
    if g.game_in_progress == 0 { return false; }
    if control == ctl::END_TURN && g.solo_human_mode != 0 { return false; }
    if g.game_end_flag == "Y" || g.action_code == "AUTO" {
        if matches!(control, 244 | 249 | 246 | 105..=110 | 951 | 953 | 954) {
            return false;
        }
    }
    if g.action_code == "AUTO" && matches!(control,
        170 | 168 | 104 | 102 | 173 | 175 | 1 | 202 | 234 | 247 | 248 |
        125..=132 | 134) {
        return false;
    }
    true
}

/// Browser controls, including text/select fields and clickable labels.
pub fn disabled_controls(g: &Globals) -> Vec<i32> {
    [1, 102, 104, 105, 106, 107, 108, 109, 110, 114, 125, 126, 127, 128, 129,
        130, 131, 132, 134, 152, 153, 154, 157, 158, 159, 161, 163, 165, 168,
        170, 173, 175, 181, 190, 202, 218, 234, 244, 246, 247, 248, 249, 951, 953, 954]
        .into_iter().filter(|&id| !control_enabled(g, id)).collect()
}

/// One menu item of the main window's menu bar.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
    /// The popup menu it belongs to ("&File", "&Game Options", "&Settings", "&Help").
    pub popup: String,
    /// The command id ([`cmd`]).
    pub id: i32,
    /// The caption (`"-"` = separator; `&` marks the access key).
    pub text: PbStr,
    /// Shown grayed (disabled).
    pub grayed: bool,
}

/// `"On"`/`"Off"` of a 1/other flag.
fn on_off(x: i32) -> &'static str {
    if x == 1 { "On" } else { "Off" }
}

/// The current player's Yes/No setting as CreateTradingDeskDialog shows it: the player's own setting
/// when `CurPlayer > 0`, else the current setting.
fn yes_no(cur: i32, player: Option<i32>) -> &'static str {
    match player {
        Some(p) => if p == 1 { "Yes" } else { "No" },
        None => if cur == 1 { "Yes" } else { "No" },
    }
}

/// The menu bar of the main window as `CreateTradingDeskDialog` (0x402844) builds it from the current
/// state, including the enabled state updated after a game starts or ends.
pub fn desk_menu_bar(g: &Globals, env: &Env) -> Vec<MenuItem> {
    let __content = crate::content::current();
    let mut m = Vec::new();
    let mut add = |popup: &str, id: i32, text: PbStr, grayed: bool| m.push(MenuItem { popup: popup.to_owned(), id, text, grayed });
    let f = "&File";
    add(f, cmd::NEW_GAME, s(__content.string(10040)), false);
    add(f, cmd::OPEN_SAVED_GAME, s(__content.string(10041)), false);
    add(f, cmd::SAVE_GAME, s(__content.string(10042)), false);
    add(f, cmd::SAVE_GAME_AS, s(__content.string(10043)), false);
    add(f, cmd::FILE_SEPARATOR, s("-"), false);
    add(f, cmd::EXIT, s(__content.string(10044)), false);
    let o = __content.string(10045);
    add(o, cmd::HIGH_SCORE, s(__content.string(10046)), false);
    let has_customizer = env.files.borrow().exists((g.data_path.clone() + "CUSTOMIZ.EXE").as_bytes());
    add(o, cmd::CUSTOMIZER, s(__content.string(10047)), !has_customizer);
    if val(g.updates_flag_str.as_bytes()) == Ext::from_f64(0.0) {
        add(o, cmd::UPDATES, s("Updates"), false);
    }
    add(o, cmd::UPGRADES, s(__content.string(10048)), false);
    add(o, cmd::FORUM, s(__content.string(10049)), false);
    add(o, cmd::TUTORIAL, s(__content.string(10050)), false);
    let has_book = env.files.borrow().exists((g.data_path.clone() + "WSRBOOK.HTM").as_bytes());
    add(o, cmd::MANUAL, s(if has_book { __content.string(10051) } else { __content.string(10052) }), false);
    add(o, cmd::SPECULATOR, s(__content.string(10053)), false);
    let st = __content.string(10054);
    add(st, cmd::TICKER_SPEED, s(__content.string(10055)) + &str_s(cast::<Ext, _>(g.ticker_speed)), false);
    add(st, cmd::SELECT_CURRENCY, s(__content.string(10056)) + &g.currency_code + ")", false);
    add(st, cmd::CHEAT_MODE, s(__content.string(10057)) + if g.cheat_mode == "N" { "Off" } else { "On" }, false);
    add(st, cmd::LAW_FIRM, s(__content.string(10058)), false);
    add(st, cmd::SETTINGS_SEPARATOR, s("-"), false);
    add(st, cmd::SUPPRESS_POPUPS, s(__content.string(10059)) + on_off(g.suppress_popups), false);
    add(st, cmd::SUPPRESS_EARNINGS, s(__content.string(10060)) + on_off(g.cur_suppress_earnings), false);
    add(st, cmd::SUPPRESS_CASH_WARN, s(__content.string(10061)) + on_off(g.cur_suppress_cash_warn), false);
    add(st, cmd::AUTO_SAVE, s(__content.string(10062)) + on_off(g.cur_auto_save), false);
    let p = g.cur_player;
    let pl = |a: &crate::pb::array::Arr1<i32>| if 0 < p { Some(a[p]) } else { None };
    add(st, cmd::EXERCISE_OPTIONS, s(__content.string(10063)) + yes_no(g.cur_exercise_options, pl(&g.player_exercise_options)), false);
    add(st, cmd::MAKE_DELIVERY, s(__content.string(10064)) + yes_no(g.cur_make_delivery, pl(&g.player_make_delivery)), false);
    add(st, cmd::TAKE_DELIVERY, s(__content.string(10065)) + yes_no(g.cur_take_delivery, pl(&g.player_take_delivery)), false);
    add(st, cmd::SWEEP_CASH, s(__content.string(10066)) + yes_no(g.cur_sweep_cash, pl(&g.sweep_cash_to_loan)), false);
    // ChartSize 0 is turned into 2 ("Small") first
    add(st, cmd::CHART_SIZE, s(__content.string(10067)) + if g.chart_size == 1 { "Large" } else { "Small" }, false);
    let sym = if 0 < g.sel_entity && g.sel_entity < 1601 { g.ent_symbol[g.sel_entity].clone() } else { s(__content.string(1604)) };
    add(st, cmd::CLEAR_CHART_HISTORY, s(__content.string(10068)) + &sym, false);
    add(st, cmd::AUTO_ADD_STREAM, s(__content.string(10069)) + on_off(g.auto_add_stream_list), false);
    let ap = if 0 < p && g.auto_pilot[p] == 1 { "On" } else { "Off" };
    add(st, cmd::AUTOPILOT, s(__content.string(10070)) + ap, false);
    let thr = if g.game_in_progress == 1 { str_trim(cast::<Ext, _>(g.player_growth_throttle[p])) + "%" } else { s("60%") };
    add(st, cmd::GROWTH_THROTTLE, s(__content.string(10071)) + &thr, false);
    let h = "&Help";
    add(h, cmd::HELP, s(__content.string(10072)), false);
    add(h, cmd::REGISTRATION, s(__content.string(10073)), false);
    add(h, cmd::ABOUT, s("&About"), false);
    let in_game = g.game_in_progress == 1;
    let ended = in_game && g.game_end_flag == "Y";
    for item in &mut m {
        match item.id {
            cmd::NEW_GAME if in_game => item.text = s(__content.string(10074)),
            cmd::OPEN_SAVED_GAME => item.grayed = in_game,
            cmd::SAVE_GAME | cmd::SAVE_GAME_AS => item.grayed = !in_game || ended,
            cmd::CHEAT_MODE | cmd::LAW_FIRM | cmd::SWEEP_CASH | cmd::CLEAR_CHART_HISTORY => item.grayed = !in_game,
            cmd::GROWTH_THROTTLE => item.grayed = !in_game || ended,
            cmd::AUTO_SAVE if ended => item.grayed = true,
            _ => {}
        }
    }
    m
}

/// Kind of a main-window control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Image,
    Label,
    Frame,
    Button,
    TextBox,
    ListBox,
}

/// One control of the main window with its initial caption.
#[derive(Clone, Debug, PartialEq)]
pub struct DeskControl {
    pub id: i32,
    pub kind: ControlKind,
    /// Caption as created (later updated by the market display, labels are UI in the port).
    pub text: PbStr,
    /// The original's callback (`Cb*`) a click runs, if any ([`desk_click`]).
    pub callback: Option<&'static str>,
}

/// The main window's controls with the captions `CreateTradingDeskDialog` (0x402844) gives them,
/// in creation order (pure; `now` gives `DATE$` for the date label). The industry combo (234) and the
/// streaming quote lines (801..815, 901..915) are created by other routines and not listed.
pub fn desk_controls(g: &Globals, now: &LocalTime) -> Vec<DeskControl> {
    let __content = crate::content::current();
    use ControlKind::*;
    let mut v = Vec::new();
    let mut c = |id: i32, kind: ControlKind, text: PbStr, cb: Option<&'static str>| {
        v.push(DeskControl { id, kind, text, callback: cb })
    };
    c(119, Image, s(""), Some("CbLogoDblClick"));
    c(191, Image, s(""), None);
    c(120, Label, s(""), None);
    let pname = if g.cur_player_name.is_empty() { s("????") } else { g.cur_player_name.clone() };
    c(140, Label, s(__content.string(10075)) + &pname, None);
    let mut year = str_s(val(right(date_str(now).as_bytes(), 4).as_bytes()) + Ext::from_f64(1.0));
    if 2013 < g.cur_year {
        year = str_s(cast::<Ext, _>(g.cur_year));
    }
    c(141, Label, s(__content.string(10076)) + &year, None);
    c(112, Label, s(__content.string(10077)), None);
    c(100, Frame, s("Other:"), None);
    c(244, Label, s(__content.string(10078)), Some("CbEndTurn"));
    c(246, Label, s(__content.string(10079)), Some("CbMiscMenuBtn"));
    c(247, Label, s("Chart"), Some("CbChartBtn"));
    c(248, Label, s("Cheat"), Some("CbCheatLabel"));
    c(101, Frame, s(__content.string(10080)), None);
    c(105, Label, s(__content.string(10081)), Some("CbBuyStockBtn"));
    c(106, Label, s(__content.string(10082)), Some("CbSellStockBtn"));
    c(107, Label, s(__content.string(10083)), Some("CbBuySellMenu"));
    c(108, Label, s(__content.string(10084)), Some("CbFinancingMenu"));
    c(109, Label, s(__content.string(5317)), Some("CbManagementMenu"));
    c(110, Label, s(__content.string(10085)), Some("CbOtherTransMenu"));
    c(103, Frame, s(__content.string(10086)), None);
    c(170, Button, s(__content.string(10087)), Some("CbSelectPlayerBtn"));
    c(168, Button, s(__content.string(10014)), Some("CbSelectCorpBtn"));
    c(104, Button, s(__content.string(10088)), Some("CbGeneralBtn"));
    c(102, Button, s(__content.string(10089)), Some("CbEntityInfoBtn"));
    let last = if 10 < g.last_sel_entity { g.ent_symbol[g.last_sel_entity].clone() } else { s("Last") };
    c(173, Button, s(__content.string(10090)) + &last, Some("CbSelectLastBtn"));
    c(175, Button, s(__content.string(10091)), Some("CbMyNewsBtn"));
    c(122, Frame, s(__content.string(10092)), None);
    c(139, Label, s(__content.string(10093)), None);
    c(138, Label, s(__content.string(10094)), None);
    c(137, Label, s(__content.string(10095)), None);
    c(136, Label, s(__content.string(10096)), None);
    c(135, Label, s(__content.string(10097)), None);
    c(208, Label, s("0.00"), None);
    c(211, Label, s("0.00"), None);
    c(214, Label, s("0.00"), None);
    c(217, Label, s("-0.00"), None);
    c(218, Label, s("0.00"), Some("CbChartNetWorth"));
    c(111, Frame, s(__content.string(10098)), None);
    c(142, Label, s(__content.string(10099)), None);
    c(151, Label, s(__content.string(10100)), None);
    c(148, Label, s(__content.string(10101)), None);
    c(145, Label, s(__content.string(10102)), None);
    let sel = if g.sel_corp_name.is_empty() { s(__content.string(10103)) } else { g.sel_corp_name.clone() };
    c(169, Label, sel, None);
    c(176, Label, s(__content.string(10104)), None);
    c(181, Label, s(if g.sel_entity == 0 { __content.string(10104) } else { "No One" }), Some("CbControllerLabelClick"));
    c(190, Label, s(if g.active_entity == 0 { __content.string(10104) } else { "No One" }), Some("CbActiveEntityLabelClick"));
    c(113, Frame, s(__content.string(10105)), None);
    c(117, Label, s(__content.string(10106)), None);
    c(114, Label, s(__content.string(10107)), Some("CbChartStockIndex"));
    c(116, Label, s(__content.string(10108)), None);
    c(152, Label, s(" 7.25%"), Some("CbChartPrimeRate"));
    c(115, Label, s(__content.string(10109)), None);
    c(153, Label, s("     2.5%"), Some("CbChartGdpGrowth"));
    c(118, Label, s(__content.string(10110)), None);
    c(154, Label, s("  $100.00"), Some("CbChartCrude"));
    c(160, Label, s(__content.string(10111)), None);
    c(159, Label, s("  $1,200.00"), Some("CbChartGold"));
    c(162, Label, s("Silver:"), None);
    c(161, Label, s("$20.00"), Some("CbChartSilver"));
    c(164, Label, s(__content.string(10112)), None);
    c(163, Label, s("$5.00"), Some("CbChartWheat"));
    c(166, Label, s(__content.string(10113)), None);
    c(165, Label, s("$5.00"), Some("CbChartCorn"));
    c(155, Label, s(__content.string(10114)), None);
    c(157, Label, s(" 7.25%"), Some("CbChartLongBondRate"));
    c(156, Label, s(__content.string(10115)), None);
    c(158, Label, s(" 6.50%"), Some("CbChartShortBondRate"));
    c(124, Frame, s(__content.string(10116)), None);
    c(125, Label, s(__content.string(10117)), Some("CbListFutures"));
    c(126, Label, s(__content.string(10118)), Some("CbRecallDbSearch"));
    c(127, Label, s(__content.string(10119)), Some("CbEarningsReport"));
    c(128, Label, s(__content.string(10120)), Some("CbListPortfolio"));
    c(130, Label, s(__content.string(10121)), Some("CbResearchReport"));
    c(129, Label, s(__content.string(10122)), Some("CbListShareholders"));
    c(131, Label, s(__content.string(10123)), Some("CbFinancialProfile"));
    c(132, Label, s(__content.string(10124)), Some("CbListOptions"));
    c(134, Label, s(__content.string(10125)), Some("CbMyCorporations"));
    c(950, Frame, s(__content.string(10126)), None);
    c(951, Label, s(__content.string(10127)), Some("CbStreamAddDelete"));
    c(953, Label, s("Clear"), Some("CbStreamClear"));
    c(954, Label, s("Fill"), Some("CbStreamFill"));
    let sym = if 10 < g.sel_entity { g.ent_symbol[g.sel_entity].clone() } else { s("N/A") };
    c(202, TextBox, sym, Some("CbSymbolBoxFocus"));
    c(1, Button, s("Go"), Some("CbGoButton"));
    c(205, ListBox, s(""), None);
    c(249, Label, s(__content.string(10128)), Some("CbTickerOnOff"));
    c(123, Label, mid(g.ticker_text.as_bytes(), 1, 60), None);
    v
}

/// Port of CreateTradingDeskDialog (0x402844): builds the main window. State effects only: deletes
/// `GTIME.DAT` under Windows XP, syncs the current settings with the current player's (`CurExerciseOptions`,
/// `CurMakeDelivery`, `CurTakeDelivery`, `CurSweepCash`), `ChartSize` 0 → 2, `CheatMode` "" → "Y", and runs
/// `ShowNextEarningsDate`, `UpdateDateDisplay`, `UpdateControllerLabel`, `RebuildStreamingQuotesPanel(2)`,
/// `CreateIndustryCombo`, `RefreshNewsListbox`, `EnableMainControls`, `UpdateFileMenuState` and
/// `UpdateMarketDisplay` like the original. The window/menu handles (`hDlgMain`, `hMenuBar`, …) are not
/// touched; the menu and the controls are described by [`desk_menu_bar`] / [`desk_controls`]. The
/// original's modal message loop (`DIALOG SHOW MODAL … CALL DlgProcTradingDesk`) is the host's event loop.
pub fn create_trading_desk_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let gtime = g.data_path.clone() + "GTIME.DAT";
    if file_exists(env, &gtime) != 0 && g.win_version == "XP" {
        env.files.borrow_mut().delete(gtime.as_bytes());
    }
    // (UI) DIALOG NEW hDlgMain, AdjustDialogFrameSize, hMainDlgAlias, menus (desk_menu_bar)
    let p = g.cur_player;
    if 0 < p {
        g.cur_exercise_options = g.player_exercise_options[p];
        g.cur_make_delivery = g.player_make_delivery[p];
        g.cur_take_delivery = g.player_take_delivery[p];
        g.cur_sweep_cash = g.sweep_cash_to_loan[p];
    }
    if g.chart_size == 0 {
        g.chart_size = 2;
    }
    // (UI) logo, headline image, label 120
    if !env.skips(0x4a070a) {
        crate::logic::market::show_next_earnings_date(g);
    }
    // (UI) labels 140, 141
    if g.game_in_progress == 1 && !env.skips(0x4a054a) {
        crate::logic::turn::update_date_display(g);
    }
    // (UI) buttons, frames, labels
    if g.cheat_mode.is_empty() {
        set(&mut g.cheat_mode, "Y");
    }
    if g.sel_entity == 1 && !env.skips(0x46d8d5) {
        crate::logic::market::update_controller_label(g);
    }
    if g.active_entity == 1 && !env.skips(0x46d8d5) {
        crate::logic::market::update_controller_label(g);
    }
    // (UI) price labels, quick search labels, streaming list frame and buttons
    if g.game_in_progress == 0 || g.game_end_flag == "Y" {
        // (UI) CONTROL DISABLE 951, 953, 954
    } else if !env.skips(0x5dcee5) {
        crate::logic::market::rebuild_streaming_quotes_panel(g, ui, 2);
    }
    if 1 < len(g.ind_name[70].as_bytes()) && !env.skips(0x46dc2b) {
        crate::logic::newgame::create_industry_combo(g);
    }
    // (UI) symbol box 202, Go button, news listbox 205
    if !env.skips(0x41e749) {
        crate::logic::turn::refresh_news_listbox(g);
    }
    // (UI) ticker button 249, ticker text label 123
    if !env.skips(0x424754) {
        crate::logic::turn::enable_main_controls(g);
    }
    if !env.skips(0x46e2a0) {
        update_file_menu_state(g, env);
    }
    if g.game_in_progress == 1 && !env.skips(0x4400d7) {
        crate::logic::market::update_market_display(g, ui, rng, env.now.second);
    }
    // DIALOG SHOW MODAL hDlgMain CALL DlgProcTradingDesk: the host's event loop
}

/// Port of DlgProcTradingDesk (0x405a7c), the main window's dialog procedure: `WM_COMMAND` (273) runs
/// [`desk_command`] with `LOWORD(wParam)`, `WM_CLOSE` (16) runs [`desk_close`], `WM_HELP` (83) shows the
/// help. Everything else is window plumbing (colours: [`control_colors`], tooltips, the icon) and returns 0.
pub fn dlg_proc_trading_desk(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, msg: i32, wparam: u32) -> i32 {
    match msg {
        83 => {
            if !env.skips(0x40f506) {
                show_help(g, ui, rng, env);
            }
        }
        16 => desk_close(g, ui, rng, env),
        273 => desk_command(g, ui, rng, env, cast::<i32, _>(wparam & 0xffff)),
        _ => {}
    }
    0
}

/// The `WM_COMMAND` menu dispatch of DlgProcTradingDesk (0x405a7c): runs the routine of menu item `id`
/// ([`cmd`]). Unknown ids and the separators do nothing.
pub fn desk_command(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, id: i32) {
    type F = fn(&mut Globals, &mut dyn Ui, &mut Rng, &Env);
    let simple: Option<(u32, F)> = match id {
        cmd::NEW_GAME => Some((0x40a74a, menu_new_game)),
        cmd::SAVE_GAME_AS => Some((0x40c685, menu_save_game_as)),
        cmd::AUTO_SAVE => Some((0x40d670, menu_toggle_auto_save)),
        cmd::EXERCISE_OPTIONS => Some((0x40d7f1, menu_toggle_exercise_options)),
        cmd::MAKE_DELIVERY => Some((0x40dc69, menu_toggle_make_delivery)),
        cmd::TAKE_DELIVERY => Some((0x40def1, menu_toggle_take_delivery)),
        cmd::CHART_SIZE => Some((0x40e21c, menu_toggle_chart_size)),
        cmd::CLEAR_CHART_HISTORY => Some((0x40e372, menu_clear_chart_history)),
        cmd::AUTO_ADD_STREAM => Some((0x40e591, menu_toggle_auto_add_stream)),
        cmd::AUTOPILOT => Some((0x40e734, menu_toggle_auto_pilot_global)),
        cmd::GROWTH_THROTTLE => Some((0x40edbc, menu_set_growth_throttle)),
        cmd::EXIT => Some((0x40c6c2, menu_exit_game)),
        cmd::TICKER_SPEED => Some((0x40c88a, menu_ticker_speed)),
        cmd::SELECT_CURRENCY => Some((0x40ca39, menu_select_currency)),
        cmd::CHEAT_MODE => Some((0x40cb57, menu_toggle_cheat_mode)),
        cmd::LAW_FIRM => Some((0x40ced1, menu_select_law_firm)),
        cmd::HIGH_SCORE => Some((0x40cf78, menu_high_score)),
        cmd::CUSTOMIZER => Some((0x40d06c, menu_customizer)),
        cmd::MANUAL => Some((0x40f7ba, open_order_manual_page)),
        cmd::SPECULATOR => Some((0x40f8be, promo_speculator)),
        cmd::STRATEGY_MANUAL => Some((0x40fb76, open_strategy_manual)),
        cmd::UPGRADES => Some((0x4102b3, menu_upgrades)),
        cmd::FORUM => Some((0x4102f0, open_forum_page)),
        cmd::TUTORIAL => Some((0x4103cc, open_tutorial_page)),
        cmd::SUPPRESS_CASH_WARN => Some((0x40d157, menu_toggle_suppress_cash_warn)),
        cmd::SUPPRESS_POPUPS => Some((0x40d305, menu_toggle_suppress_popups)),
        cmd::SUPPRESS_EARNINGS => Some((0x40d4e0, menu_toggle_suppress_earnings)),
        cmd::HELP => Some((0x40f506, show_help)),
        // ShowAboutBox(2) / ShowAboutBox(1): the argument is unused
        cmd::REGISTRATION | cmd::ABOUT => Some((0x40f571, show_about_box)),
        _ => None,
    };
    if let Some((addr, f)) = simple {
        if !env.skips(addr) {
            f(g, ui, rng, env);
        }
        return;
    }
    match id {
        cmd::OPEN_SAVED_GAME => {
            if !env.skips(0x40ae7f) { super::app::menu_open_saved_game(g, ui, rng, env); }
        }
        cmd::SAVE_GAME => {
            if !env.skips(0x40c01b) {
                super::app::save_game(g, ui, rng, env);
            }
        }
        cmd::SWEEP_CASH => {
            if !env.skips(0x40d9d5) {
                let cfg = crate::logic::corp_finance::loans::menu_toggle_sweep_cash(g, ui, rng, env.now.second);
                env.files.borrow_mut().write(b"cfig.wsr", cfg);
            }
        }
        cmd::UPDATES => {
            if !env.skips(0x410144) {
                open_updates_page(g, ui, rng, env, 1);
            }
        }
        _ => {} // 625, 663 (separators), unknown ids
    }
}

/// The `WM_CLOSE` branch of DlgProcTradingDesk (0x405a7c): under Windows XP or an installer run
/// deletes `GTIME.DAT`; unless the game is over, stops a running ticker and asks "Are you sure you want
/// to quit?" (YES: `TickerStopReq = "Y"` with `GameEndFlag = "Y"`; NO: both back to "N"); closes the
/// dialogs and clears `ChartEntity`. The program ends when the host sees `GameEndFlag = "Y"` /
/// `TickerStopReq = "Y"` (the window itself is destroyed by DDT).
pub fn desk_close(g: &mut Globals, ui: &mut dyn Ui, _rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    if g.win_version == "XP" || g.cmd_line_mode == "INSTALL" {
        let gtime = g.data_path.clone() + "GTIME.DAT";
        if file_exists(env, &gtime) != 0 {
            env.files.borrow_mut().delete(gtime.as_bytes());
        }
    }
    if g.game_end_flag != "Y" {
        if g.ticker_on == 1 {
            set(&mut g.clock_state, "STOP");
            g.ticker_on = 0;
            // (UI) DIALOG END RunLoopDlg
        }
        set(&mut g.game_end_flag, "Y");
        let app_title = g.app_title.clone();
        if ui.msgbox(&s(__content.string(10129)), 8484, &app_title) == id::YES {
            set(&mut g.ticker_stop_req, "Y");
        } else {
            set(&mut g.game_end_flag, "N");
            set(&mut g.ticker_stop_req, "N");
        }
    }
    // (UI) DIALOG END of the transaction menu, report, symbol entry, ticker loop, DB search, ownership,
    // chart, commodity desk, IR swap and who-owns-what dialogs
    g.chart_entity = 0;
}

/// Text/background palette indices of main-window control `id` (the `WM_CTLCOLOR*` branch of
/// DlgProcTradingDesk 0x405a7c): `None` = system default. Streaming quote lines (801..815 symbol,
/// 901..915 price) are green/red/yellow by `StreamPriceDir(slot, CurPlayer)` (text colour index
/// `11 - dir`).
pub fn control_colors(g: &Globals, id: i32) -> Option<(i32, i32)> {
    Some(match id {
        249 | 105 => (1, 10),
        100 | 244 | 246 | 247 | 248 | 101 | 107 | 109 | 108 | 110 | 103 => (14, 0),
        106 => (15, 4),
        111 | 113 => (0, 14),
        117 | 155 | 156 | 160 | 162 | 164 | 166 => (15, 0),
        114 | 157 | 158 | 159 | 161 | 163 => (0, 22),
        801..=815 | 901..=915 => {
            let slot = if id > 900 { id - 900 } else { id - 800 };
            let dir = g.stream_price_dir[(slot, g.cur_player.clamp(0, 5))];
            return Some((11 - dir, 0)).filter(|_| (1..=15).contains(&slot));
        }
        _ => return None,
    })
}
