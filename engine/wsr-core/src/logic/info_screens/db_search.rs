//! The DATABASE SEARCH SCREEN: `ShowDbSearchDialog` (0x5472ee), its dialog callbacks (`DbSearch_On*`,
//! `DlgProcDbSearch`), the search itself (`RunDbSearch` 0x54b072 with its GOSUBs TestCriteria /
//! ListBonds / ShowNoResults / ExitGosub), `DbSearch_ReadParams`, `DbSearch_SyncCheckboxes` and
//! `SelectIndustryFromList`.
//!
//! The original is a modeless-looking modal DDT dialog: checkboxes, value text boxes, "Change" buttons
//! cycling a rating, and Display / Clear / Help / Close buttons. Every click updates the search globals
//! (`SrchChk*`, `Scr*`, `Search*`) and re-runs the search in counting mode (the number of matches is
//! shown in text box 389); "Display Results" shows the matching companies (or bonds) as a pick list.
//!
//! The port keeps the dialog's controls in a [`DbSearchDlg`] (the texts of the text boxes and captions,
//! the check marks the routines set) and drives it with [`DbSearchAction`]s: [`db_search_dispatch`]
//! runs the callback of a control exactly like a click (or an `EN_CHANGE` of a value box) in the
//! original. [`show_db_search_dialog_with`] runs the modal loop with any driver (a web UI);
//! [`show_db_search_dialog`] drives it through [`Ui::choose`] / [`Ui::inputbox`].

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init
)]
use crate::pb::real::{Ext, cast};


use crate::logic::cashflow::show_cash_flow_projection;
use crate::logic::control::update_corp_control;
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::{show_quick_message, show_text_report};
use crate::logic::reports_lists::{fmt, link_id, pb_array_sort, s, str15, str_trim};
use crate::logic::trading::select::select_industry_combo_item;
use crate::logic::valuation::{
    calc_analyst_rating, calc_dividend_yield, calc_industry_outlook, count_active_corps, get_mgmt_margin_vs_industry,
    price_trend_streak,
};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{compare, instr, ltrim, mid, parse_any, remove_any, replace, right, rtrim, string_fill};
use crate::pb::numeric::{to_long, to_quad, Cur};
use crate::state::Globals;
use crate::ui::Ui;

/// SINGLE constant of the original's constant pool (`fmul dword [...]`).
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// Control ids of the DATABASE SEARCH SCREEN.
pub mod ctl {

    pub const CHK_HIGH_ROE: i32 = 284;
    pub const TXT_MIN_ROE: i32 = 285;
    pub const CHK_LOW_PE: i32 = 286;
    pub const TXT_MAX_PE: i32 = 288;
    pub const CHK_PRICE_VS_NW: i32 = 290;
    pub const TXT_PRICE_PCT_NW: i32 = 292;
    pub const CHK_DIV_YIELD: i32 = 294;
    pub const TXT_DIV_YIELD: i32 = 296;
    pub const CHK_MARKET_CAP: i32 = 370;
    pub const TXT_MIN_MKT_CAP: i32 = 372;
    pub const TXT_MAX_MKT_CAP: i32 = 371;
    pub const CHK_EARN_UP_NEXT_QTR: i32 = 298;
    pub const TXT_EARN_GROWTH: i32 = 363;
    pub const CHK_EARN_UP_YEARS: i32 = 364;
    pub const BTN_EARN_YEARS: i32 = 366;
    pub const TXT_EARN_YEARS: i32 = 368;
    pub const CHK_MGMT_RATING: i32 = 373;
    pub const BTN_MGMT_RATING: i32 = 375;
    pub const TXT_MGMT_RATING: i32 = 376;
    pub const CHK_CREDIT_RATING: i32 = 377;
    pub const BTN_CREDIT_RATING: i32 = 378;
    pub const TXT_CREDIT_RATING: i32 = 380;
    pub const CHK_ANALYST_RATING: i32 = 391;
    pub const BTN_ANALYST_RATING: i32 = 392;
    pub const TXT_ANALYST_RATING: i32 = 393;
    pub const CHK_EXCL_FINANCIALS: i32 = 383;
    pub const CHK_HAS_BONDS: i32 = 385;
    pub const CHK_CONVERTIBLE_ONLY: i32 = 379;
    pub const CHK_IND_SUPPLY_DEMAND: i32 = 381;
    pub const CHK_ONE_INDUSTRY: i32 = 384;
    pub const CHK_POS_CASH_FLOW: i32 = 358;
    pub const CHK_CASH_FLOW_PRE_DEBT: i32 = 360;
    pub const CHK_SHORT_CANDIDATES: i32 = 361;
    pub const CHK_EXCL_BANKRUPTS: i32 = 362;
    pub const TXT_COUNT: i32 = 389;
    pub const BTN_DISPLAY: i32 = 387;
    pub const BTN_CLEAR: i32 = 386;
    pub const BTN_HELP: i32 = 390;
    pub const BTN_CLOSE: i32 = 388;
}

/// The controls of the dialog that hold state: control texts (value boxes, rating displays, the match
/// count, checkbox captions) and the check marks set by `CONTROL SET CHECK`. `open` is false once the
/// dialog was ended (`DIALOG END`).
#[derive(Clone, Debug, Default)]
pub struct DbSearchDlg {
    pub open: bool,
    pub texts: Vec<(i32, PbStr)>,
    pub checks: Vec<(i32, i32)>,
}

impl DbSearchDlg {
    /// `CONTROL GET TEXT` ("" for a control that does not exist).
    pub fn text(&self, id: i32) -> PbStr {
        self.texts.iter().find(|(i, _)| *i == id).map(|(_, t)| t.clone()).unwrap_or_default()
    }
    /// `CONTROL SET TEXT` (also the user typing into a value box).
    pub fn set_text(&mut self, id: i32, t: PbStr) {
        match self.texts.iter_mut().find(|(i, _)| *i == id) {
            Some(e) => e.1 = t,
            None => self.texts.push((id, t)),
        }
    }
    /// The check mark of checkbox `id`.
    pub fn check(&self, id: i32) -> i32 {
        self.checks.iter().find(|(i, _)| *i == id).map(|(_, c)| *c).unwrap_or(0)
    }
    /// `CONTROL SET CHECK`.
    pub fn set_check(&mut self, id: i32, v: i32) {
        match self.checks.iter_mut().find(|(i, _)| *i == id) {
            Some(e) => e.1 = v,
            None => self.checks.push((id, v)),
        }
    }
}

/// What the user does in the dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum DbSearchAction {
    /// Click on a checkbox or button (`BN_CLICKED`): runs the control's callback.
    Click(i32),
    /// Type `text` into value box `id` (no notification).
    SetText(i32, PbStr),
    /// `EN_CHANGE` of value box `id`: runs its callback.
    EditChanged(i32),
    /// Typing `text` into value box `id`: [`DbSearchAction::SetText`] followed by its `EN_CHANGE`.
    Edit(i32, PbStr),
    /// Close box / ESC (`WM_CLOSE`).
    Close,
}

/// The checkboxes of the dialog: (control id, caption). The caption of 379/384 changes (see
/// [`DbSearchDlg::text`] of these ids).
#[allow(non_snake_case)]
pub fn CHECKBOXES(__content: &crate::content::ContentPack) -> [(i32, &str); 19] { [
    (ctl::CHK_HIGH_ROE, __content.string(6041)),
    (ctl::CHK_LOW_PE, __content.string(6042)),
    (ctl::CHK_PRICE_VS_NW, __content.string(6043)),
    (ctl::CHK_DIV_YIELD, __content.string(6044)),
    (ctl::CHK_MARKET_CAP, __content.string(6045)),
    (ctl::CHK_EARN_UP_NEXT_QTR, __content.string(6046)),
    (ctl::CHK_EARN_UP_YEARS, __content.string(6047)),
    (ctl::CHK_MGMT_RATING, __content.string(6048)),
    (ctl::CHK_CREDIT_RATING, __content.string(6049)),
    (ctl::CHK_ANALYST_RATING, __content.string(6050)),
    (ctl::CHK_EXCL_FINANCIALS, __content.string(6051)),
    (ctl::CHK_HAS_BONDS, __content.string(6052)),
    (ctl::CHK_CONVERTIBLE_ONLY, __content.string(6053)),
    (ctl::CHK_IND_SUPPLY_DEMAND, __content.string(6054)),
    (ctl::CHK_ONE_INDUSTRY, __content.string(6055)),
    (ctl::CHK_POS_CASH_FLOW, __content.string(6056)),
    (ctl::CHK_CASH_FLOW_PRE_DEBT, __content.string(6057)),
    (ctl::CHK_SHORT_CANDIDATES, __content.string(6058)),
    (ctl::CHK_EXCL_BANKRUPTS, __content.string(6059)),
] }

/// The value boxes the user can type into: (control id, label).
#[allow(non_snake_case)]
pub fn VALUE_BOXES(__content: &crate::content::ContentPack) -> [(i32, &str); 7] { [
    (ctl::TXT_MIN_ROE, "Minimum return on equity %"),
    (ctl::TXT_MAX_PE, __content.string(6060)),
    (ctl::TXT_PRICE_PCT_NW, "Maximum stock price as % of net worth"),
    (ctl::TXT_DIV_YIELD, __content.string(6061)),
    (ctl::TXT_MIN_MKT_CAP, __content.string(6062)),
    (ctl::TXT_MAX_MKT_CAP, __content.string(6063)),
    (ctl::TXT_EARN_GROWTH, __content.string(6064)),
] }

#[allow(non_snake_case)]
fn NO_INDUSTRY(__content: &crate::content::ContentPack) -> &str { __content.string(6065) }

/// `IndName(industry)` with `" & "` doubled for a DDT label.
fn industry_label(g: &Globals, industry: i32) -> PbStr {
    let mut name = g.ind_name[industry].clone();
    if 0 < instr(1, name.as_bytes(), b" & ") {
        name = replace(name.as_bytes(), b" & ", b" && ");
    }
    name
}

/// Port of ShowDbSearchDialog (0x5472ee), driven through [`Ui::choose`] (see [`show_db_search_dialog_with`]).
pub fn show_db_search_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    show_db_search_dialog_with(g, ui, rng, &mut choose_action);
}

/// The [`Ui`] driver of the dialog: one [`Ui::choose`] per action listing the checkboxes (with their
/// check marks), the value boxes, the rating "Change" buttons and Display / Clear / Help / Close; a
/// value box asks for the new value with [`Ui::inputbox`] (then `EN_CHANGE`). Cancel closes.
pub fn choose_action(g: &mut Globals, ui: &mut dyn Ui, dlg: &DbSearchDlg) -> DbSearchAction {
    let __content = crate::content::current();
    let _ = g;
    let mut ids: Vec<(i32, bool)> = Vec::new();
    let mut options: Vec<PbStr> = Vec::new();
    for (id, caption) in CHECKBOXES(&__content).iter() {
        let cap = match *id {
            ctl::CHK_CONVERTIBLE_ONLY | ctl::CHK_ONE_INDUSTRY => dlg.text(*id),
            _ => s(caption),
        };
        let mark = if dlg.check(*id) != 0 { "[x] " } else { "[ ] " };
        options.push(s(mark) + &cap);
        ids.push((*id, false));
    }
    for (id, label) in VALUE_BOXES(&__content).iter() {
        options.push(s(label) + ": " + &dlg.text(*id));
        ids.push((*id, true));
    }
    for (id, label) in [
        (ctl::BTN_EARN_YEARS, "Change earnings-up years"),
        (ctl::BTN_MGMT_RATING, "Change management rating"),
        (ctl::BTN_CREDIT_RATING, "Change credit rating"),
        (ctl::BTN_ANALYST_RATING, "Change analyst rating"),
        (ctl::BTN_DISPLAY, __content.string(6066)),
        (ctl::BTN_CLEAR, __content.string(6067)),
        (ctl::BTN_HELP, "Help"),
        (ctl::BTN_CLOSE, "Close"),
    ] {
        options.push(s(label));
        ids.push((id, false));
    }
    let prompt = [
        s(__content.string(6068)) + &dlg.text(ctl::TXT_COUNT),
        s("Earnings up years: ") + &dlg.text(ctl::TXT_EARN_YEARS) + __content.string(6069) + &dlg.text(ctl::TXT_MGMT_RATING)
            + "   Credit rating: " + &dlg.text(ctl::TXT_CREDIT_RATING) + "   Analyst rating: "
            + &dlg.text(ctl::TXT_ANALYST_RATING),
        s(__content.string(6070)),
    ];
    match ui.choose(&s(__content.string(6071)), &prompt, &options) {
        Some(k) if ids[k].1 => {
            let id = ids[k].0;
            let t = ui.inputbox(&options[k], &s(__content.string(6071)), &dlg.text(id));
            DbSearchAction::Edit(id, t)
        }
        Some(k) => DbSearchAction::Click(ids[k].0),
        None => DbSearchAction::Close,
    }
}

/// Port of ShowDbSearchDialog (0x5472ee).
/// Ends an open search dialog, builds the dialog (initial texts: the defaults "20%", "10", "95%", "5%",
/// "200", "1000", "10%", "1" the first time, else the current `Srch*` values; `ScrMgmtRating` "Good",
/// `ScrCreditRatingStr` "BBB", `ScrAnalystRatingStr` "Buy" and `ScrIndustryName` are set), initializes
/// the search settings once (`ScrInitialized`: analyst rating "Buy" checked, P/E 10, price 95% of net
/// worth, ROE 20%, earnings growth 10%, yield 5%, 1 year, credit rating BBB), syncs the checkboxes,
/// counts the matches (`RunDbSearch` mode 1) and runs the dialog modally (`DlgOpen_DbSearch` = 1 while
/// open). `driver` supplies the user's actions until the dialog is ended.
pub fn show_db_search_dialog_with(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    driver: &mut dyn FnMut(&mut Globals, &mut dyn Ui, &DbSearchDlg) -> DbSearchAction,
) {
    let __content = crate::content::current();
    // DIALOG END hDlgDbSearch, 0 (a previous instance; see DbSearch_OnClear)
    g.dlg_open_db_search = 0;
    let mut dlg = DbSearchDlg { open: true, ..DbSearchDlg::default() };
    let init = g.scr_initialized != 0;
    let field = |init: bool, dflt: &str, v: PbStr| if !init { s(dflt) } else { v };
    dlg.set_text(ctl::TXT_MIN_ROE, field(init, "20%", str_trim(cast::<Ext, _>(g.srch_min_roe))));
    dlg.set_text(ctl::TXT_MAX_PE, field(init, "10", str_trim(cast::<Ext, _>(g.srch_max_pe))));
    dlg.set_text(ctl::TXT_PRICE_PCT_NW, field(init, "95%", str_trim(cast::<Ext, _>(g.srch_max_price_pct_nw)) + "%"));
    dlg.set_text(ctl::TXT_DIV_YIELD, field(init, "5%", str_trim(cast::<Ext, _>(g.srch_min_div_yield)) + "%"));
    dlg.set_text(ctl::TXT_MIN_MKT_CAP, field(init, "200", str_trim(cast::<Ext, _>(g.srch_min_mkt_cap))));
    dlg.set_text(ctl::TXT_MAX_MKT_CAP, field(init, "1000", str_trim(cast::<Ext, _>(g.srch_max_mkt_cap))));
    dlg.set_text(ctl::TXT_EARN_GROWTH, field(init, "10%", str_trim(cast::<Ext, _>(g.srch_min_earn_growth)) + "%"));
    dlg.set_text(ctl::TXT_EARN_YEARS, field(init, "1", str_trim(cast::<Ext, _>(g.scr_earnings_up_years))));
    if !init {
        g.scr_mgmt_rating = s("Good");
    }
    dlg.set_text(ctl::TXT_MGMT_RATING, g.scr_mgmt_rating.clone());
    if !init {
        g.scr_credit_rating_str = s("BBB");
    }
    dlg.set_text(ctl::TXT_CREDIT_RATING, g.scr_credit_rating_str.clone());
    if !init {
        g.scr_analyst_rating_str = s("Buy");
    }
    dlg.set_text(ctl::TXT_ANALYST_RATING, g.scr_analyst_rating_str.clone());
    if g.scr_conv_premium_pct == 0 {
        dlg.set_text(ctl::CHK_CONVERTIBLE_ONLY, s(__content.string(6053)));
    } else {
        let t = str_trim(cast::<Ext, _>(g.scr_conv_premium_pct)) + "%)";
        dlg.set_text(ctl::CHK_CONVERTIBLE_ONLY, s(__content.string(6072)) + &t);
    }
    if g.search_industry < 1 || 70 < g.search_industry {
        g.scr_industry_name = s(NO_INDUSTRY(&__content));
    } else {
        g.scr_industry_name = industry_label(g, g.search_industry);
    }
    dlg.set_text(ctl::CHK_ONE_INDUSTRY, s(__content.string(6055)) + &g.scr_industry_name);
    dlg.set_text(ctl::TXT_COUNT, s("0"));
    if g.scr_initialized == 0 {
        g.scr_initialized = 1;
        g.srch_chk_analyst_rating = 1;
        g.scr_analyst_rating = 2;
        g.srch_max_pe = 10;
        g.srch_max_price_pct_nw = 95;
        g.srch_min_roe = 20;
        g.srch_min_earn_growth = 10;
        g.srch_min_div_yield = 5;
        g.scr_earnings_up_years = 1;
        g.scr_credit_rating_rank = 7;
    }
    db_search_sync_checkboxes(g, &mut dlg);
    let mut mode = 1;
    run_db_search(g, ui, rng, &mut dlg, &mut mode);
    g.dlg_open_db_search = 1;
    // DIALOG SHOW MODAL hDlgDbSearch CALL DlgProcDbSearch
    while dlg.open {
        let act = driver(g, ui, &dlg);
        db_search_dispatch(g, ui, rng, &mut dlg, act, driver);
    }
    g.dlg_open_db_search = 0;
}

/// Runs the callback of the control an action addresses (the PB DDT dispatch of `WM_COMMAND` to
/// `CONTROL ADD ... CALL` callbacks; `DlgProcDbSearch` itself only colours controls). `driver` is
/// needed by "Clear Checkboxes", which re-opens the dialog.
pub fn db_search_dispatch(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    dlg: &mut DbSearchDlg,
    action: DbSearchAction,
    driver: &mut dyn FnMut(&mut Globals, &mut dyn Ui, &DbSearchDlg) -> DbSearchAction,
) {
    match action {
        DbSearchAction::SetText(id, t) => dlg.set_text(id, t),
        DbSearchAction::Edit(id, t) => {
            dlg.set_text(id, t);
            db_search_dispatch(g, ui, rng, dlg, DbSearchAction::EditChanged(id), driver);
        }
        DbSearchAction::Close => dlg.open = false,
        DbSearchAction::EditChanged(id) => match id {
            ctl::TXT_MIN_ROE => db_search_on_edit_high_roe(g, ui, rng, dlg),
            ctl::TXT_MAX_PE => db_search_on_edit_low_pe(g, ui, rng, dlg),
            ctl::TXT_PRICE_PCT_NW => db_search_on_edit_price_vs_nw(g, ui, rng, dlg),
            ctl::TXT_DIV_YIELD => db_search_on_edit_div_yield(g, ui, rng, dlg),
            ctl::TXT_EARN_GROWTH => db_search_on_edit_earn_up_next_qtr(g, ui, rng, dlg),
            ctl::TXT_MAX_MKT_CAP => db_search_on_edit_max_mkt_cap(g, ui, rng, dlg),
            ctl::TXT_MIN_MKT_CAP => db_search_on_edit_min_mkt_cap(g, ui, rng, dlg),
            _ => {}
        },
        DbSearchAction::Click(id) => match id {
            ctl::CHK_HIGH_ROE => db_search_on_chk_high_roe(g, ui, rng, dlg),
            ctl::CHK_LOW_PE => db_search_on_chk_low_pe(g, ui, rng, dlg),
            ctl::CHK_PRICE_VS_NW => db_search_on_chk_price_vs_nw(g, ui, rng, dlg),
            ctl::CHK_DIV_YIELD => db_search_on_chk_div_yield(g, ui, rng, dlg),
            ctl::CHK_MARKET_CAP => db_search_on_chk_market_cap(g, ui, rng, dlg),
            ctl::CHK_EARN_UP_NEXT_QTR => db_search_on_chk_earn_up_next_qtr(g, ui, rng, dlg),
            ctl::CHK_EARN_UP_YEARS => db_search_on_chk_earn_up_years(g, ui, rng, dlg),
            ctl::BTN_EARN_YEARS => db_search_on_cycle_earn_up_years(g, ui, rng, dlg),
            ctl::CHK_MGMT_RATING => db_search_on_chk_mgmt_rating(g, ui, rng, dlg),
            ctl::BTN_MGMT_RATING => db_search_on_cycle_mgmt_rating(g, ui, rng, dlg),
            ctl::CHK_CREDIT_RATING => db_search_on_chk_credit_rating(g, ui, rng, dlg),
            ctl::BTN_CREDIT_RATING => db_search_on_cycle_credit_rating(g, ui, rng, dlg),
            ctl::CHK_ANALYST_RATING => db_search_on_chk_analyst_rating(g, ui, rng, dlg),
            ctl::BTN_ANALYST_RATING => db_search_on_cycle_analyst_rating(g, ui, rng, dlg),
            ctl::CHK_EXCL_FINANCIALS => db_search_on_chk_excl_financials(g, ui, rng, dlg),
            ctl::CHK_HAS_BONDS => db_search_on_chk_has_bonds(g, ui, rng, dlg),
            ctl::CHK_CONVERTIBLE_ONLY => db_search_on_chk_convertible_only(g, ui, rng, dlg),
            ctl::CHK_IND_SUPPLY_DEMAND => db_search_on_chk_ind_supply_demand(g, ui, rng, dlg),
            ctl::CHK_ONE_INDUSTRY => db_search_on_chk_one_industry(g, ui, rng, dlg),
            ctl::CHK_POS_CASH_FLOW => db_search_on_chk_pos_cash_flow(g, ui, rng, dlg),
            ctl::CHK_CASH_FLOW_PRE_DEBT => db_search_on_chk_cash_flow_pre_debt(g, ui, rng, dlg),
            ctl::CHK_SHORT_CANDIDATES => db_search_on_chk_short_candidates(g, ui, rng, dlg),
            ctl::CHK_EXCL_BANKRUPTS => db_search_on_chk_excl_bankrupts(g, ui, rng, dlg),
            ctl::BTN_DISPLAY => db_search_on_display(g, ui, rng, dlg),
            ctl::BTN_CLEAR => db_search_on_clear(g, ui, rng, dlg, driver),
            ctl::BTN_HELP => db_search_on_help(),
            ctl::BTN_CLOSE => db_search_on_close(dlg),
            _ => {}
        },
    }
}

/// Port of DlgProcDbSearch (0x548a44): pure window plumbing (control colours `WM_CTLCOLOR*`, tooltips,
/// `WM_HELP` running `WSRHELP.EXE |DB|`); no state effects. Returns the message result 0.
pub fn dlg_proc_db_search() -> i32 {
    0
}

/// `searchMode = 1: RunDbSearch searchMode` after a callback.
fn recount(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let mut mode = 1;
    run_db_search(g, ui, rng, dlg, &mut mode);
}

/// Body shared by the plain checkbox callbacks: turning the criterion on clears the short-sale search,
/// then the flag is toggled, the checkboxes synced and the matches counted.
fn toggle_criterion(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    dlg: &mut DbSearchDlg,
    flag: fn(&mut Globals) -> &mut i32,
) {
    if *flag(g) == 0 {
        g.search_short_candidates = 0;
        g.search_exclude_bankrupts = 0;
    }
    let f = flag(g);
    *f = cast::<i32, _>(*f == 0);
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Body shared by the value-box callbacks (`EN_CHANGE`): checks the criterion, clears the short-sale
/// search, syncs and counts.
fn edit_criterion(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    dlg: &mut DbSearchDlg,
    flag: fn(&mut Globals) -> &mut i32,
) {
    *flag(g) = 1;
    g.search_short_candidates = 0;
    g.search_exclude_bankrupts = 0;
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkHighROE (0x549184): toggles `SrchChkHighROE` (see `toggle_criterion`).
pub fn db_search_on_chk_high_roe(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_high_roe);
}

/// Port of DbSearch_OnEditHighROE (0x549217): `EN_CHANGE` of the ROE box checks `SrchChkHighROE`.
pub fn db_search_on_edit_high_roe(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_high_roe);
}

/// Port of DbSearch_OnChkLowPE (0x549291): toggles `SrchChkLowPE`.
pub fn db_search_on_chk_low_pe(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_low_pe);
}

/// Port of DbSearch_OnEditLowPE (0x549324): `EN_CHANGE` of the P/E box checks `SrchChkLowPE`.
pub fn db_search_on_edit_low_pe(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_low_pe);
}

/// Port of DbSearch_OnChkPriceVsNW (0x54939e): toggles `SrchChkPriceVsNetWorth`.
pub fn db_search_on_chk_price_vs_nw(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_price_vs_net_worth);
}

/// Port of DbSearch_OnEditPriceVsNW (0x549431): `EN_CHANGE` checks `SrchChkPriceVsNetWorth`.
pub fn db_search_on_edit_price_vs_nw(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_price_vs_net_worth);
}

/// Port of DbSearch_OnChkDivYield (0x5494ab): toggles `SrchChkDivYield`.
pub fn db_search_on_chk_div_yield(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_div_yield);
}

/// Port of DbSearch_OnEditDivYield (0x54953e): `EN_CHANGE` checks `SrchChkDivYield`.
pub fn db_search_on_edit_div_yield(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_div_yield);
}

/// Port of DbSearch_OnChkEarnUpNextQtr (0x5495b8): toggles `SrchChkEarnUpNextQtr`.
pub fn db_search_on_chk_earn_up_next_qtr(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_earn_up_next_qtr);
}

/// Port of DbSearch_OnEditEarnUpNextQtr (0x54964b): `EN_CHANGE` checks `SrchChkEarnUpNextQtr`.
pub fn db_search_on_edit_earn_up_next_qtr(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_earn_up_next_qtr);
}

/// Port of DbSearch_OnChkEarnUpYears (0x5496c5): toggles `SrchChkEarnUpYears`.
pub fn db_search_on_chk_earn_up_years(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_earn_up_years);
}

/// Port of DbSearch_OnCycleEarnUpYears (0x549758): "Change" cycles `ScrEarningsUpYears` 1 → 2 → 3 → 1,
/// shows it in box 368 and checks the criterion.
pub fn db_search_on_cycle_earn_up_years(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let years = g.scr_earnings_up_years;
    if years == 1 {
        g.scr_earnings_up_years = 2;
    } else if years == 2 {
        g.scr_earnings_up_years = 3;
    } else {
        g.scr_earnings_up_years = 1;
    }
    dlg.set_text(ctl::TXT_EARN_YEARS, str_trim(cast::<Ext, _>(g.scr_earnings_up_years)));
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_earn_up_years);
}

/// Port of DbSearch_OnChkMarketCap (0x54984e): toggles `SrchChkMarketCap` (no short-sale reset).
pub fn db_search_on_chk_market_cap(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    g.srch_chk_market_cap = cast::<i32, _>(g.srch_chk_market_cap == 0);
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnEditMaxMktCap (0x5498cd): `EN_CHANGE` checks `SrchChkMarketCap`.
pub fn db_search_on_edit_max_mkt_cap(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    g.srch_chk_market_cap = 1;
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnEditMinMktCap (0x549933): `EN_CHANGE` checks `SrchChkMarketCap`.
pub fn db_search_on_edit_min_mkt_cap(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    g.srch_chk_market_cap = 1;
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkMgmtRating (0x549999): toggles `SrchChkMgmtRating`.
pub fn db_search_on_chk_mgmt_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_mgmt_rating);
}

/// Port of DbSearch_OnCycleMgmtRating (0x549a30): cycles `ScrMgmtRating` Good → Average → Poor → Good
/// (anything else → Good) and checks the criterion.
pub fn db_search_on_cycle_mgmt_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let cur_rating = g.scr_mgmt_rating.clone();
    if cur_rating == "Good" {
        g.scr_mgmt_rating = s("Average");
    } else if cur_rating == "Average" {
        g.scr_mgmt_rating = s("Poor");
    } else {
        g.scr_mgmt_rating = s("Good");
    }
    dlg.set_text(ctl::TXT_MGMT_RATING, g.scr_mgmt_rating.clone());
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_mgmt_rating);
}

/// Port of DbSearch_OnChkCreditRating (0x549b69): toggles `ScrCreditRatingOn`.
pub fn db_search_on_chk_credit_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.scr_credit_rating_on);
}

/// Port of DbSearch_OnCycleCreditRating (0x549c00): steps the minimum credit rating up (`ScrCreditRatingStr`
/// / `ScrCreditRatingRank`: D 0 → C 2 → CC 3 → CCC 4 → B 5 → BB 6 → BBB 7 → A 8 → AA 9 → AAA 10 → D);
/// an empty rating counts as BBB (rank 7 is set), an unknown one stays. Checks the criterion.
pub fn db_search_on_cycle_credit_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let mut cur_rating = g.scr_credit_rating_str.clone();
    if cur_rating.is_empty() {
        cur_rating = s("BBB");
        g.scr_credit_rating_rank = 7;
    }
    let next: Option<(&str, i32)> = match cur_rating.to_display().as_str() {
        "AAA" => Some(("D", 0)),
        "AA" => Some(("AAA", 10)),
        "A" => Some(("AA", 9)),
        "BBB" => Some(("A", 8)),
        "BB" => Some(("BBB", 7)),
        "B" => Some(("BB", 6)),
        "CCC" => Some(("B", 5)),
        "CC" => Some(("CCC", 4)),
        "C" => Some(("CC", 3)),
        "D" => Some(("C", 2)),
        _ => None,
    };
    if let Some((r, rank)) = next {
        g.scr_credit_rating_str = s(r);
        g.scr_credit_rating_rank = rank;
    }
    dlg.set_text(ctl::TXT_CREDIT_RATING, g.scr_credit_rating_str.clone());
    edit_criterion(g, ui, rng, dlg, |g| &mut g.scr_credit_rating_on);
}

/// Port of DbSearch_OnChkAnalystRating (0x549fd6): toggles `SrchChkAnalystRating`.
pub fn db_search_on_chk_analyst_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_analyst_rating);
}

/// Port of DbSearch_OnCycleAnalystRating (0x54a06d): cycles the wanted analyst rating (`ScrAnalystRatingStr`
/// / `ScrAnalystRating`) Strong Buy 1 → Buy 2 → Hold 3 → Sell 4 → Strong Sell 5 → Strong Buy; an empty
/// rating or rating number 0 counts as Strong Buy. Checks the criterion.
pub fn db_search_on_cycle_analyst_rating(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let __content = crate::content::current();
    let mut cur_rating = g.scr_analyst_rating_str.clone();
    if cur_rating.is_empty() || g.scr_analyst_rating == 0 {
        cur_rating = s(__content.string(1074));
        g.scr_analyst_rating = 1;
    }
    let next: Option<(&str, i32)> = match cur_rating.to_display().as_str() {
        value if value == __content.string(1074) => Some(("Buy", 2)),
        "Buy" => Some(("Hold", 3)),
        "Hold" => Some(("Sell", 4)),
        "Sell" => Some((__content.string(1075), 5)),
        value if value == __content.string(1075) => Some((__content.string(1074), 1)),
        _ => None,
    };
    if let Some((r, n)) = next {
        g.scr_analyst_rating_str = s(r);
        g.scr_analyst_rating = n;
    }
    dlg.set_text(ctl::TXT_ANALYST_RATING, g.scr_analyst_rating_str.clone());
    edit_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_analyst_rating);
}

/// Port of DbSearch_OnChkIndSupplyDemand (0x54a2e4): toggles `SrchChkIndSupplyDemand`.
pub fn db_search_on_chk_ind_supply_demand(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_ind_supply_demand);
}

/// Port of DbSearch_OnChkOneIndustry (0x54a377): turning "Search One Industry" on lets the player pick the
/// industry ([`select_industry_from_list`]; `SelIndustry` is restored when none is picked), off clears
/// `SearchIndustry`. Updates `ScrIndustryName` and the checkbox caption, syncs and counts.
pub fn db_search_on_chk_one_industry(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let __content = crate::content::current();
    let saved_sel_industry = g.sel_industry;
    if g.search_industry == 0 {
        g.search_industry = 71;
    } else {
        g.search_industry = 0;
    }
    if g.search_industry == 71 {
        g.search_industry = select_industry_from_list(g, ui, rng);
        if g.search_industry == 0 {
            g.sel_industry = saved_sel_industry;
        }
        select_industry_combo_item(g);
        if 0 < g.search_industry && g.search_industry < 71 {
            g.scr_industry_name = industry_label(g, g.search_industry);
        }
    }
    if g.search_industry == 0 {
        g.scr_industry_name = s(NO_INDUSTRY(&__content));
        dlg.set_check(ctl::CHK_ONE_INDUSTRY, 0);
    }
    dlg.set_text(ctl::CHK_ONE_INDUSTRY, s(__content.string(6055)) + &g.scr_industry_name);
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkExclFinancials (0x54a52d): toggles `SrchChkExclFinancials` (no short-sale reset).
pub fn db_search_on_chk_excl_financials(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    g.srch_chk_excl_financials = cast::<i32, _>(g.srch_chk_excl_financials == 0);
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkHasBonds (0x54a5ac): toggles `ScrHasBonds` (turning it on clears the convertible
/// and short-sale searches).
pub fn db_search_on_chk_has_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    if g.scr_has_bonds == 0 {
        g.scr_convertible_only = 0;
        g.search_short_candidates = 0;
        g.search_exclude_bankrupts = 0;
    }
    g.scr_has_bonds = cast::<i32, _>(g.scr_has_bonds == 0);
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkConvertibleOnly (0x54a655): turning "convertible bonds only" on (clears "has
/// bonds" and the short-sale search) explains the conversion-premium setting and offers to change it
/// (YES/NO; then INPUTBOX for the maximum premium %, 0 = ignore, capped at 10000, negative input
/// ignored); updates the caption. Turning it off just clears `ScrConvertibleOnly`.
pub fn db_search_on_chk_convertible_only(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let __content = crate::content::current();
    if g.scr_convertible_only == 0 {
        g.scr_convertible_only = 1;
        g.scr_has_bonds = 0;
        g.search_short_candidates = 0;
        g.search_exclude_bankrupts = 0;
        let mut prompt_text = s(__content.string(6073));
        if g.scr_conv_premium_pct < 1 {
            prompt_text = prompt_text
                + __content.string(6074)
                + __content.string(6075)
                + __content.string(6076);
        } else {
            prompt_text = prompt_text
                + __content.string(6077)
                + &str15(cast::<Ext, _>(g.scr_conv_premium_pct))
                + __content.string(6078)
                + __content.string(6079);
        }
        prompt_text = prompt_text + "\r" + "\r" + __content.string(6080);
        if 0 < g.scr_conv_premium_pct {
            prompt_text = prompt_text
                + "\r"
                + "\r"
                + __content.string(6081);
        }
        let mut premium_text = str_trim(cast::<Ext, _>(g.scr_conv_premium_pct)) + "%";
        let answer = ui.msgbox(&prompt_text, 8484, &s(__content.string(6082)));
        if answer == 6 {
            if g.scr_conv_premium_pct == 0 {
                premium_text = PbStr::new();
            }
            loop {
                let mut input_text = ui.inputbox(
                    &(s(__content.string(6083))
                        + __content.string(6084)),
                    &s(__content.string(6085)),
                    &premium_text,
                );
                input_text = remove_any(input_text.as_bytes(), b"$ ,%");
                input_text = trim(&input_text);
                if !input_text.is_empty() {
                    let premium_val = to_quad(val(input_text.as_bytes()));
                    if 10000 < premium_val {
                        g.scr_conv_premium_pct = 10000;
                        break;
                    }
                    if 0 <= premium_val {
                        g.scr_conv_premium_pct = to_long(cast::<Ext, _>(premium_val));
                    }
                }
                if !(g.scr_conv_premium_pct < 0) {
                    break;
                }
            }
        }
        if g.scr_conv_premium_pct < 1 {
            dlg.set_text(ctl::CHK_CONVERTIBLE_ONLY, s(__content.string(6053)));
        } else {
            let t = str_trim(cast::<Ext, _>(g.scr_conv_premium_pct)) + "%)";
            dlg.set_text(ctl::CHK_CONVERTIBLE_ONLY, s(__content.string(6086)) + &t);
        }
    } else {
        g.scr_convertible_only = 0;
    }
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkPosCashFlow (0x54aa72): toggles `SrchChkPosCashFlow` (turning it off also clears
/// "before debt payment", on clears the short-sale search).
pub fn db_search_on_chk_pos_cash_flow(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    if g.srch_chk_pos_cash_flow != 0 {
        g.srch_chk_cash_flow_pre_debt = 0;
    } else {
        g.search_short_candidates = 0;
        g.search_exclude_bankrupts = 0;
    }
    g.srch_chk_pos_cash_flow = cast::<i32, _>(g.srch_chk_pos_cash_flow == 0);
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkCashFlowPreDebt (0x54ab0f): only with "positive cash flow" checked (else the
/// check mark is removed): toggles `SrchChkCashFlowPreDebt`.
pub fn db_search_on_chk_cash_flow_pre_debt(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    if g.srch_chk_pos_cash_flow == 0 {
        dlg.set_check(ctl::CHK_CASH_FLOW_PRE_DEBT, 0);
    } else {
        toggle_criterion(g, ui, rng, dlg, |g| &mut g.srch_chk_cash_flow_pre_debt);
    }
}

/// Port of DbSearch_OnChkShortCandidates (0x54abc8): turning the short-sale search on clears every
/// conflicting criterion (with a quick message); off also clears "exclude bankrupts".
pub fn db_search_on_chk_short_candidates(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let __content = crate::content::current();
    if g.search_short_candidates == 0 {
        g.search_short_candidates = 1;
        let mut tmp_arg = s(__content.string(6087));
        show_quick_message(g, ui, &mut tmp_arg);
        g.srch_chk_high_roe = 0;
        g.srch_chk_low_pe = 0;
        g.srch_chk_price_vs_net_worth = 0;
        g.srch_chk_div_yield = 0;
        g.srch_chk_earn_up_next_qtr = 0;
        g.srch_chk_earn_up_years = 0;
        g.srch_chk_mgmt_rating = 0;
        g.scr_credit_rating_on = 0;
        g.srch_chk_analyst_rating = 0;
        g.srch_chk_ind_supply_demand = 0;
        g.scr_convertible_only = 0;
        g.scr_has_bonds = 0;
        g.srch_chk_pos_cash_flow = 0;
        g.srch_chk_cash_flow_pre_debt = 0;
    } else {
        g.search_short_candidates = 0;
        g.search_exclude_bankrupts = 0;
    }
    db_search_sync_checkboxes(g, dlg);
    recount(g, ui, rng, dlg);
}

/// Port of DbSearch_OnChkExclBankrupts (0x54ad02): only with the short-sale search on (else the check
/// mark is removed): toggles `SearchExcludeBankrupts`.
pub fn db_search_on_chk_excl_bankrupts(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    if g.search_short_candidates == 0 {
        dlg.set_check(ctl::CHK_EXCL_BANKRUPTS, 0);
    } else {
        g.search_exclude_bankrupts = cast::<i32, _>(g.search_exclude_bankrupts == 0);
        db_search_sync_checkboxes(g, dlg);
        recount(g, ui, rng, dlg);
    }
}

/// Port of DbSearch_OnClear (0x54ada7): "Clear Checkboxes": tells the number of active corporations
/// (`CountActiveCorps`, quick message), clears every criterion and `SearchIndustry`, counts again and
/// re-opens the dialog ([`show_db_search_dialog_with`], which ends this one).
pub fn db_search_on_clear(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    dlg: &mut DbSearchDlg,
    driver: &mut dyn FnMut(&mut Globals, &mut dyn Ui, &DbSearchDlg) -> DbSearchAction,
) {
    let __content = crate::content::current();
    count_active_corps(g);
    let mut tmp_arg = s(__content.string(6088))
        + &str15(cast::<Ext, _>(g.active_corp_count))
        + __content.string(6089);
    show_quick_message(g, ui, &mut tmp_arg);
    g.srch_chk_high_roe = 0;
    g.srch_chk_low_pe = 0;
    g.srch_chk_price_vs_net_worth = 0;
    g.srch_chk_div_yield = 0;
    g.srch_chk_earn_up_next_qtr = 0;
    g.srch_chk_earn_up_years = 0;
    g.srch_chk_market_cap = 0;
    g.srch_chk_mgmt_rating = 0;
    g.scr_credit_rating_on = 0;
    g.srch_chk_analyst_rating = 0;
    g.srch_chk_ind_supply_demand = 0;
    g.scr_convertible_only = 0;
    g.scr_has_bonds = 0;
    g.srch_chk_excl_financials = 0;
    g.search_industry = 0;
    g.srch_chk_pos_cash_flow = 0;
    g.srch_chk_cash_flow_pre_debt = 0;
    g.search_short_candidates = 0;
    g.search_exclude_bankrupts = 0;
    db_search_sync_checkboxes(g, dlg);
    dlg.set_text(ctl::TXT_COUNT, s("0"));
    recount(g, ui, rng, dlg);
    // ShowDbSearchDialog(0): DIALOG END hDlgDbSearch (this dialog), then a new one runs modally
    dlg.open = false;
    show_db_search_dialog_with(g, ui, rng, driver);
}

/// Port of DbSearch_OnDisplay (0x54af2d): "Display Results" (`RunDbSearch` mode 2).
pub fn db_search_on_display(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg) {
    let mut mode = 2;
    run_db_search(g, ui, rng, dlg, &mut mode);
}

/// Port of DbSearch_OnHelp (0x54af81): runs `WSRHELP.EXE |DB|` (the help viewer; no state effects).
pub fn db_search_on_help() {}

/// Port of DbSearch_OnClose (0x54b00c): ends the dialog.
pub fn db_search_on_close(dlg: &mut DbSearchDlg) {
    dlg.open = false;
}

/// Port of DbSearch_SyncCheckboxes (0x54e118): sets every checkbox from its search global (384 from
/// `SearchIndustry > 0`; a `SearchIndustry < 1` also resets `ScrIndustryName`).
pub fn db_search_sync_checkboxes(g: &mut Globals, dlg: &mut DbSearchDlg) {
    let __content = crate::content::current();
    dlg.set_check(ctl::CHK_HIGH_ROE, g.srch_chk_high_roe);
    dlg.set_check(ctl::CHK_LOW_PE, g.srch_chk_low_pe);
    dlg.set_check(ctl::CHK_PRICE_VS_NW, g.srch_chk_price_vs_net_worth);
    dlg.set_check(ctl::CHK_DIV_YIELD, g.srch_chk_div_yield);
    dlg.set_check(ctl::CHK_EARN_UP_NEXT_QTR, g.srch_chk_earn_up_next_qtr);
    dlg.set_check(ctl::CHK_EARN_UP_YEARS, g.srch_chk_earn_up_years);
    dlg.set_check(ctl::CHK_MARKET_CAP, g.srch_chk_market_cap);
    dlg.set_check(ctl::CHK_MGMT_RATING, g.srch_chk_mgmt_rating);
    dlg.set_check(ctl::CHK_CREDIT_RATING, g.scr_credit_rating_on);
    dlg.set_check(ctl::CHK_ANALYST_RATING, g.srch_chk_analyst_rating);
    dlg.set_check(ctl::CHK_IND_SUPPLY_DEMAND, g.srch_chk_ind_supply_demand);
    let none = g.search_industry < 1;
    if none {
        g.scr_industry_name = s(NO_INDUSTRY(&__content));
    }
    dlg.set_check(ctl::CHK_ONE_INDUSTRY, cast::<i32, _>(!none));
    dlg.set_check(ctl::CHK_EXCL_FINANCIALS, g.srch_chk_excl_financials);
    dlg.set_check(ctl::CHK_HAS_BONDS, g.scr_has_bonds);
    dlg.set_check(ctl::CHK_CONVERTIBLE_ONLY, g.scr_convertible_only);
    dlg.set_check(ctl::CHK_POS_CASH_FLOW, g.srch_chk_pos_cash_flow);
    dlg.set_check(ctl::CHK_CASH_FLOW_PRE_DEBT, g.srch_chk_cash_flow_pre_debt);
    dlg.set_check(ctl::CHK_SHORT_CANDIDATES, g.search_short_candidates);
    dlg.set_check(ctl::CHK_EXCL_BANKRUPTS, g.search_exclude_bankrupts);
}

/// `ROUND(VAL(CONTROL GET TEXT id))` into a QUAD (`fistp qword`).
fn read_quad(dlg: &DbSearchDlg, id: i32) -> i64 {
    to_quad(val(dlg.text(id).as_bytes()))
}

/// Port of DbSearch_ReadParams (0x54d9fe).
/// Reads the value boxes into the search globals: ROE % (> 1000 → 1000, <= -100 unchecks), P/E (> 1000 →
/// 1000, <= 0 unchecks), price % of net worth (> 5000 → 5000, <= 0 unchecks), dividend yield % (> 100 →
/// 100, <= 0 unchecks), earnings growth % (> 1000 → 1000, <= 0 unchecks); a market cap bound < 1 is
/// reset to 1000 / 200 and unchecks the market-cap criterion. Clamped values are written back.
///
/// BUG (original, #390): the market-cap resets clear the check mark of the value boxes 371/372 instead
/// of checkbox 370, which keeps showing a check mark while `SrchChkMarketCap = 0`.
pub fn db_search_read_params(g: &mut Globals, dlg: &mut DbSearchDlg) {
    let v = read_quad(dlg, ctl::TXT_MIN_ROE);
    g.srch_min_roe = to_long(cast::<Ext, _>(v));
    if v > 1000 {
        dlg.set_text(ctl::TXT_MIN_ROE, s("1000%"));
        g.srch_min_roe = 1000;
    } else if v <= -100 {
        g.srch_chk_high_roe = 0;
        dlg.set_check(ctl::CHK_HIGH_ROE, 0);
    }
    let v = read_quad(dlg, ctl::TXT_MAX_PE);
    g.srch_max_pe = to_long(cast::<Ext, _>(v));
    if v > 1000 {
        dlg.set_text(ctl::TXT_MAX_PE, s("1000"));
        g.srch_max_pe = 1000;
    } else if v <= 0 {
        g.srch_chk_low_pe = 0;
        dlg.set_check(ctl::CHK_LOW_PE, 0);
    }
    let v = read_quad(dlg, ctl::TXT_PRICE_PCT_NW);
    g.srch_max_price_pct_nw = to_long(cast::<Ext, _>(v));
    if v > 5000 {
        dlg.set_text(ctl::TXT_PRICE_PCT_NW, s("5000%"));
        g.srch_max_price_pct_nw = 5000;
    } else if v <= 0 {
        g.srch_chk_price_vs_net_worth = 0;
        dlg.set_check(ctl::CHK_PRICE_VS_NW, 0);
    }
    let v = read_quad(dlg, ctl::TXT_DIV_YIELD);
    g.srch_min_div_yield = to_long(cast::<Ext, _>(v));
    if v > 100 {
        dlg.set_text(ctl::TXT_DIV_YIELD, s("100%"));
        g.srch_min_div_yield = 100;
    } else if v <= 0 {
        g.srch_chk_div_yield = 0;
        dlg.set_check(ctl::CHK_DIV_YIELD, 0);
    }
    let v = read_quad(dlg, ctl::TXT_EARN_GROWTH);
    g.srch_min_earn_growth = to_long(cast::<Ext, _>(v));
    if v > 1000 {
        dlg.set_text(ctl::TXT_EARN_GROWTH, s("1000%"));
        g.srch_min_earn_growth = 1000;
    } else if v <= 0 {
        g.srch_chk_earn_up_next_qtr = 0;
        dlg.set_check(ctl::CHK_EARN_UP_NEXT_QTR, 0);
    }
    let v = read_quad(dlg, ctl::TXT_MAX_MKT_CAP);
    g.srch_max_mkt_cap = v;
    if v < 1 {
        g.srch_max_mkt_cap = 1000;
        dlg.set_text(ctl::TXT_MAX_MKT_CAP, s("1000"));
        g.srch_chk_market_cap = 0;
        dlg.set_check(ctl::TXT_MAX_MKT_CAP, 0);
    }
    let v = read_quad(dlg, ctl::TXT_MIN_MKT_CAP);
    g.srch_min_mkt_cap = v;
    if v < 1 {
        g.srch_min_mkt_cap = 200;
        dlg.set_text(ctl::TXT_MIN_MKT_CAP, s("200"));
        g.srch_chk_market_cap = 0;
        dlg.set_check(ctl::TXT_MIN_MKT_CAP, 0);
    }
}

/// Port of SelectIndustryFromList (0x54e33f).
/// Lets the player pick one of the industries 1..70 from a sorted pick list (report in `"GetInd"` mode,
/// `SelIndustry` = 71 while it is open) and returns it as `SearchIndustry`; without a pick `SelIndustry`
/// is restored, `SearchIndustry` = 0 and a quick message says that all industries will be searched.
///
/// The list's title line still reads " [runtime resource wording] " (copied from the
/// start-up dialog; cosmetic).
pub fn select_industry_from_list(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> i32 {
    let __content = crate::content::current();
    let mut list_lines: Vec<PbStr> = vec![PbStr::new(); 91];
    list_lines[3] = s(__content.string(2288));
    list_lines[4] = string_fill(70, b"-");
    let mut line_idx: usize = 4;
    for industry in 1..=70 {
        line_idx += 1;
        let ind_label = g.ind_name[industry].clone() + &string_fill(75, b" ");
        list_lines[line_idx] = ind_label + "@I" + &link_id(industry);
    }
    // ARRAY SORT listLines(5) FOR 70
    pb_array_sort::<PbStr, ()>(&mut list_lines[5..75], None, false, &|a, b| {
        compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less
    });
    list_lines[line_idx + 3] = s("@END");
    for k in 1..=90i32 {
        g.text_lines[k] = list_lines[cast::<usize, _>(k)].clone();
    }
    g.entity_pick_mode = s("GetInd");
    g.text_box_title = s(__content.string(6090));
    let saved_sel_ind = g.sel_industry;
    g.sel_industry = 71;
    show_text_report(g, ui, rng);
    if g.sel_industry < 1 || 70 < g.sel_industry {
        g.sel_industry = saved_sel_ind;
        g.search_industry = 0;
        let mut msg =
            s(__content.string(6091));
        show_quick_message(g, ui, &mut msg);
    } else {
        g.search_industry = g.sel_industry;
    }
    g.search_industry
}

/// The locals of RunDbSearch shared with its GOSUBs (zeroed at entry).
#[derive(Default)]
struct Frame {
    search_mode: i32,
    params_ok: i32,
    corp_id: i32,
    rejected: i32,
    match_count: i32,
    bond_count: i32,
    has_convertible: i32,
    cap_fix_iter: i32,
    tag_text: PbStr,
    /// `DIM resultLines(0 TO 1600)` (RunDbSearch) / `(1 TO 1600)` (ListBonds); index = subscript.
    result_lines: Vec<PbStr>,
    rating_keys: Vec<i32>,
    maturity_keys: Vec<i32>,
}

/// Port of RunDbSearch (0x54b072).
/// `search_mode` (BYREF): 1 = count the matches into box 389, 2 = also show them, 3 = show the results
/// of the last search again ("recall": no parameters read; without a previous search a quick message
/// and result 0; the mode becomes 2). Modes 1/2 read the value boxes first (`DbSearch_ReadParams`,
/// `DbSearchParamsSet` = 1). Returns `paramsOk` (1 after a successful recall, else 0).
///
/// Mode 1 also props up market caps: for every active corp, while `MarketCap * 100` is below its net
/// worth times `0.67 + (CurQuarter + DifficultyLevel) / 100` (and the game is not over), the market cap
/// is raised by 10%.
/// BUG (original, #391): the counter limiting these raises to 2000 is not reset per corp, so the raises
/// of all corps together are limited to 2001 per search.
///
/// With "has bonds" / "convertible only" the bond list is built instead ([`RunDbSearch_ListBonds`]).
/// Otherwise each active corp passing the criteria gets a line (name, symbol, credit rating or industry,
/// `@C` link); mode 2 shows them (sorted by name, or by credit rating when a minimum below AAA is set)
/// as a pick list in `"ChangeCorp"` mode (none / more than 1500 → message box instead) and ends the
/// dialog when a company was picked; the first time without the credit-rating sort a hint is shown
/// (`SrchSortHintShown`).
///
/// BUG (original, #392): the credit-rating sort covers one element too many (subscripts 1 TO
/// matchCount + 1, the last one an empty line with key 0); with "D" as minimum rating, a D-rated
/// company can be sorted behind it and is then missing from the list (an empty line instead).
pub fn run_db_search(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg, search_mode: &mut i32) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (0x46ea48): error path not ported.
    let mut f = Frame { search_mode: *search_mode, ..Frame::default() };
    let r = run_db_search_body(g, ui, rng, dlg, &mut f);
    *search_mode = f.search_mode;
    r
}

fn run_db_search_body(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg, f: &mut Frame) -> i32 {
    let __content = crate::content::current();
    if f.search_mode < 3 {
        db_search_read_params(g, dlg);
        g.db_search_params_set = 1;
    }
    if f.search_mode == 3 {
        f.search_mode = 2;
        if g.db_search_params_set == 0 {
            let mut msg = s(__content.string(6092));
            show_quick_message(g, ui, &mut msg);
            f.params_ok = 0;
            return f.params_ok;
        }
        f.params_ok = 1;
    }
    if f.search_mode == 1 || f.search_mode == 3 {
        f.corp_id = 11;
        while f.corp_id <= 1600 {
            let corp_id = f.corp_id;
            if g.entity_status[corp_id] == 1 {
                calc_entity_financials(g, ui, rng, corp_id);
                // DOUBLE nwFactor = (CurQuarter + DifficultyLevel) * 0.01! + 0.67!
                let nw_factor = Ext80::from_i64(cast::<i64, _>(g.cur_quarter) + cast::<i64, _>(g.difficulty_level))
                    .fmul(Ext80::from_f32(0.01f32))
                    .fadd(Ext80::from_f32(0.67f32))
                    .into_ext();
                while f.cap_fix_iter <= 2000 {
                    if !(Ext::from_f64(0.0) < g.market_cap[corp_id]) {
                        break;
                    }
                    if !(g.market_cap[corp_id] * Ext::from_f64(100.0) < g.net_worth * nw_factor) {
                        break;
                    }
                    if g.game_end_flag == "Y" {
                        break;
                    }
                    g.market_cap[corp_id] = sf(1.1f32) * g.market_cap[corp_id];
                    f.cap_fix_iter += 1;
                }
            }
            f.corp_id += 1;
        }
    }
    if g.scr_has_bonds == 1 || g.scr_convertible_only == 1 {
        run_db_search_list_bonds(g, ui, rng, dlg, f);
        return f.params_ok;
    }
    f.result_lines = vec![PbStr::new(); 1601];
    f.rating_keys = vec![0; 1601];
    f.corp_id = 11;
    while f.corp_id <= 1600 {
        let corp_id = f.corp_id;
        if g.entity_status[corp_id] == 1 {
            f.rejected = 0;
            run_db_search_test_criteria(g, ui, rng, f);
            if f.rejected != -1 {
                f.match_count += 1;
                let n = cast::<usize, _>(f.match_count);
                f.tag_text = link_id(corp_id);
                f.result_lines[n] = g.ent_name[corp_id].clone() + " (" + &g.ent_symbol[corp_id] + ")";
                if g.scr_credit_rating_on == 1 {
                    f.rating_keys[n] = g.credit_rating_num[corp_id];
                    f.result_lines[n] = f.result_lines[n].clone() + &string_fill(34, b" ");
                    f.result_lines[n] =
                        mid(f.result_lines[n].as_bytes(), 1, 34) + "Rated: " + &g.credit_rating_str + "      ";
                } else {
                    f.result_lines[n] = f.result_lines[n].clone() + &string_fill(34, b" ");
                    let ind = cast::<i32, _>(g.industry_num[corp_id]);
                    f.result_lines[n] = mid(f.result_lines[n].as_bytes(), 1, 34) + "Ind: " + &g.ind_name[ind];
                }
                f.result_lines[n] = f.result_lines[n].clone() + &string_fill(50, b" ") + "@C" + &f.tag_text;
            }
        }
        f.corp_id += 1;
    }
    if f.search_mode == 1 || f.search_mode == 2 {
        dlg.set_text(ctl::TXT_COUNT, str_trim(cast::<Ext, _>(f.match_count)));
    }
    if f.search_mode == 1 {
        return f.params_ok;
    }
    if run_db_search_show_no_results(ui, f) {
        return f.params_ok;
    }
    let n = cast::<usize, _>(f.match_count + 1);
    if g.scr_credit_rating_on == 1 && g.scr_credit_rating_rank < 10 {
        // ARRAY SORT ratingKeys(1) FOR matchCount + 1, TAGARRAY resultLines(), DESCEND
        pb_array_sort(&mut f.rating_keys[1..1 + n], Some(&mut f.result_lines[1..1 + n]), true, &|a, b| a < b);
    } else {
        // ARRAY SORT resultLines() FOR matchCount + 1 (from LBOUND 0)
        pb_array_sort::<PbStr, ()>(&mut f.result_lines[0..n], None, false, &|a, b| {
            compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less
        });
    }
    let mc = cast::<usize, _>(f.match_count);
    f.result_lines[mc + 4] = s("@END");
    g.entity_pick_mode = s(__content.string(6093));
    for k in 1..=mc {
        g.text_lines[cast::<i32, _>(k)] = f.result_lines[k].clone();
    }
    g.text_lines[cast::<i32, _>(mc + 4)] = s("@END");
    g.text_box_title = s(__content.string(6094));
    if g.scr_credit_rating_on == 1 {
        g.text_box_title = replace(g.text_box_title.as_bytes(), b" ===", __content.bytes(6095));
    }
    f.corp_id = g.sel_entity;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    if g.sel_entity != f.corp_id {
        dlg.open = false;
    }
    if g.srch_sort_hint_shown == 0 && g.scr_credit_rating_on == 0 {
        let mut msg = s(__content.string(6096))
            + __content.string(6097);
        show_quick_message(g, ui, &mut msg);
        g.srch_sort_hint_shown = 1;
    }
    f.params_ok
}

/// Port of RunDbSearch_ShowNoResults (0x54ba0f), GOSUB of RunDbSearch / ListBonds: no match or more than
/// 1500 → message box, then the routine ends (`RunDbSearch_ExitGosub`): returns true.
fn run_db_search_show_no_results(ui: &mut dyn Ui, f: &mut Frame) -> bool {
    let __content = crate::content::current();
    if f.match_count < 1501 && f.match_count != 0 {
        return false;
    }
    f.tag_text = s(__content.string(6098))
        + __content.string(6099);
    if f.match_count == 0 {
        f.tag_text = s(__content.string(6100));
    }
    ui.msgbox(&f.tag_text, 8208, &s(__content.string(6101)));
    // RunDbSearch_ExitGosub (0x54d9d8): the SUB's exit, returning paramsOk.
    true
}

/// `PARSE$(EntMiscData(corp), ANY "|", 3)`.
fn misc_field3(g: &Globals, corp_id: i32) -> PbStr {
    parse_any(g.ent_misc_data[corp_id].as_bytes(), b"|", 3)
}

/// Port of RunDbSearch_TestCriteria (0x54ba9e), GOSUB of RunDbSearch / ListBonds: sets `rejected = -1`
/// when `corpId` fails one of the checked criteria (funds always fail). In this order: bonds issued /
/// convertible (with the conversion premium: `TmpSharePrice` is set), market cap, financials, management
/// rating, [runtime resource wording] growth, earnings up 1..3 years, P/E, (analyst rating computed),
/// business assets >= 20% of `TotalAssets` (financials excluded), credit rating, analyst rating, price vs.
/// net worth, ROE, dividend yield, industry supply/demand (`TmpExt1..3` cleared), public float (and not
/// controlled by the player, financials excluded; `UpdateCorpControl`), one industry, cash flow (`TmpExt1`),
/// short-sale score.
///
/// `NetWorth` and `TotalAssets` are the globals of the last `CalcEntityFinancials` call: in mode 2 that
/// is whatever the analyst rating computed for this corp (mode 1: the last market cap check).
fn run_db_search_test_criteria(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let corp_id = f.corp_id;
    if g.industry_num[corp_id] == 71 {
        f.rejected = -1;
        return;
    }
    if g.scr_has_bonds == 1 || g.scr_convertible_only == 1 {
        if g.bonds_issued[corp_id] <= 0 {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.scr_convertible_only == 1 {
        if instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") == 0 {
            f.rejected = -1;
        }
        if 0 < g.scr_conv_premium_pct {
            let conv_price = val(parse_any(g.bond_issue_info[corp_id].as_bytes(), b"|", 2).as_bytes())
                / g.shares_outstanding[corp_id];
            let shares_per_bond = Cur::from_f64(Ext::from_f64(1000.0) / conv_price);
            g.tmp_share_price = g.market_cap[corp_id] / g.shares_outstanding[corp_id];
            let conv_value = g.tmp_share_price * shares_per_bond.get();
            let bond_price_amt = Cur::from_f64(g.bond_price[corp_id].get() * Ext::from_f64(10.0));
            let conv_premium = ((bond_price_amt.get() - conv_value) / conv_value) * Ext::from_f64(100.0);
            if (cast::<Ext, _>(g.scr_conv_premium_pct)) < conv_premium {
                f.rejected = -1;
            }
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_market_cap == 1 {
        let threshold = g.market_cap[corp_id] * Ext::from_f64(100.0);
        if (cast::<Ext, _>(g.srch_max_mkt_cap)) < threshold || threshold < cast::<Ext, _>(g.srch_min_mkt_cap) {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_excl_financials == 1 {
        let ind = g.industry_num[corp_id];
        if ind < 3 || ind == 70 {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_mgmt_rating == 1 {
        if g.industry_num[corp_id] < 70 {
            let rating_val = get_mgmt_margin_vs_industry(g, corp_id, 0);
            if g.scr_mgmt_rating == "Good" && rating_val < 9 {
                f.rejected = -1;
            }
            if g.scr_mgmt_rating == "Average" && rating_val < -6 {
                f.rejected = -1;
            }
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_earn_up_next_qtr == 1 {
        if g.ent_misc_data[corp_id].len() < 20 {
            f.rejected = -1;
            return;
        }
        f.tag_text = misc_field3(g, corp_id);
        let mut threshold = val(f.tag_text.as_bytes());
        let metric = g.proj_qtr_earnings[corp_id];
        if !(Ext::from_f64(0.0) < metric) {
            f.rejected = -1;
        }
        threshold = threshold * (cast::<Ext, _>(g.srch_min_earn_growth) * sf(0.01f32)) + threshold;
        if metric < threshold {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_earn_up_years == 1 {
        if g.ent_misc_data[corp_id].len() > 19 {
            f.tag_text = misc_field3(g, corp_id);
            let metric = val(f.tag_text.as_bytes());
            let threshold = g.earnings_y1[corp_id] * sf(0.25f32);
            if metric < threshold {
                f.rejected = -1;
            }
        }
        if !(g.earnings_yr2_ago[corp_id] < g.earnings_y1[corp_id]) {
            f.rejected = -1;
        }
        if 1 < g.scr_earnings_up_years {
            if !(g.earnings_yr3_ago[corp_id] < g.earnings_yr2_ago[corp_id]) {
                f.rejected = -1;
            }
        }
        if g.scr_earnings_up_years == 3 {
            if !(g.earnings_yr4_ago[corp_id] < g.earnings_yr3_ago[corp_id]) {
                f.rejected = -1;
            }
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_low_pe == 1 {
        let mut annual_earnings = g.earnings_ytd[corp_id];
        let mut qtr = g.cur_quarter;
        if g.sim_co_count < corp_id {
            qtr -= 1;
        }
        if qtr == 1 {
            annual_earnings = g.earnings_y1[corp_id] * sf(0.75f32) + annual_earnings;
        }
        if qtr == 2 {
            annual_earnings = g.earnings_y1[corp_id] * sf(0.5f32) + annual_earnings;
        }
        if qtr == 3 {
            annual_earnings = g.earnings_y1[corp_id] * sf(0.25f32) + annual_earnings;
        }
        if qtr == 4 || qtr == 0 {
            annual_earnings = g.earnings_y1[corp_id];
        }
        if !(Ext::from_f64(0.0) < annual_earnings) {
            f.rejected = -1;
            return;
        }
        let threshold = annual_earnings * cast::<Ext, _>(g.srch_max_pe);
        let metric = g.market_cap[corp_id] * Ext::from_f64(100.0);
        if threshold < metric {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    let analyst_rating = calc_analyst_rating(g, ui, rng, corp_id);
    if g.srch_chk_excl_financials == 1 {
        if g.business_assets[corp_id] < g.total_assets * sf(0.2f32) {
            f.rejected = -1;
            return;
        }
    }
    if g.scr_credit_rating_on == 1 {
        if g.credit_rating_num[corp_id] < g.scr_credit_rating_rank {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_analyst_rating == 1 {
        if g.scr_analyst_rating == 0 {
            g.scr_analyst_rating = 1;
        }
        if analyst_rating != g.scr_analyst_rating {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_price_vs_net_worth == 1 {
        if !(Ext::from_f64(0.0) < g.net_worth) {
            f.rejected = -1;
            return;
        }
        let threshold = g.net_worth * (cast::<Ext, _>(g.srch_max_price_pct_nw) * sf(0.01f32));
        let metric = g.market_cap[corp_id] * Ext::from_f64(100.0);
        if threshold < metric {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_high_roe == 1 {
        if !(Ext::from_f64(0.0) < g.corp_prior_net_worth[corp_id]) || !(Ext::from_f64(0.0) < g.earnings_y1[corp_id]) {
            f.rejected = -1;
            return;
        }
        let threshold = g.corp_prior_net_worth[corp_id] * (cast::<Ext, _>(g.srch_min_roe) * sf(0.01f32));
        if g.earnings_y1[corp_id] < threshold {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_div_yield == 1 {
        if !(Ext::from_f64(0.0) < g.dividend_per_share[corp_id]) {
            f.rejected = -1;
            return;
        }
        let threshold = calc_dividend_yield(g, corp_id);
        if threshold < cast::<Ext, _>(g.srch_min_div_yield) {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.srch_chk_ind_supply_demand == 1 {
        let ind = g.industry_num[corp_id];
        if 2 < ind && ind < 70 {
            calc_industry_outlook(g, rng, ind);
            let threshold = g.tmp_ext1;
            let metric = g.tmp_ext2;
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            if !(threshold < metric) {
                f.rejected = -1;
            }
        }
    }
    if f.rejected == -1 {
        return;
    }
    if g.scr_has_bonds == 0 && g.scr_convertible_only == 0 {
        update_corp_control(g, ui, rng, corp_id);
        if g.public_pct == 0 {
            f.rejected = -1;
        }
        if g.srch_chk_excl_financials == 1 {
            if g.cur_player == g.corp_controller[corp_id] {
                f.rejected = -1;
            }
        }
    }
    if f.rejected == -1 {
        return;
    }
    if 0 < g.search_industry && g.search_industry < 71 {
        if cast::<i32, _>(g.industry_num[corp_id]) != g.search_industry {
            f.rejected = -1;
        }
    }
    if f.rejected == -1 {
        return;
    }
    if 0 < g.srch_chk_pos_cash_flow || 0 < g.srch_chk_cash_flow_pre_debt {
        if corp_id < 86 {
            f.rejected = -1;
        } else {
            g.tmp_ext1 = Ext::from_f64(0.0);
            let cash_flow_flag = to_long(show_cash_flow_projection(g, ui, rng, corp_id, 1));
            if 1 < cash_flow_flag && (g.srch_chk_cash_flow_pre_debt == 0 || g.tmp_ext1 != -Ext::from_f64(99.0)) {
                f.rejected = -1;
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
        }
    }
    if f.rejected == -1 {
        return;
    }
    if 0 < g.search_short_candidates {
        if g.ent_misc_data[corp_id].len() >= 20 && g.earnings_y1[corp_id] != Ext::from_f64(0.0) {
            if g.credit_rating_num[corp_id] == 0 {
                f.rejected = -1;
            } else if g.credit_rating_num[corp_id] == 1 && g.search_exclude_bankrupts == 1 {
                f.rejected = -1;
            } else {
                f.tag_text = misc_field3(g, corp_id);
                let threshold = val(f.tag_text.as_bytes());
                let metric = g.proj_qtr_earnings[corp_id];
                let mut short_score = 0;
                if metric < Ext::from_f64(0.0) {
                    short_score = 1;
                    if metric < threshold {
                        short_score = 2;
                    }
                } else if Ext::from_f64(0.0) < threshold && metric < threshold * sf(0.8f32) {
                    short_score = 2;
                } else if Ext::from_f64(0.0) < g.earnings_y1[corp_id] * sf(0.25f32) && metric < g.earnings_y1[corp_id] * sf(0.2f32) {
                    short_score = 1;
                }
                if short_score == 0 {
                    f.rejected = -1;
                } else {
                    if price_trend_streak(g, corp_id) < -3 {
                        short_score += 1;
                    } else if 2 < price_trend_streak(g, corp_id) {
                        short_score -= 1;
                    }
                    if short_score == 0 {
                        f.rejected = -1;
                    } else {
                        let rating_val = calc_analyst_rating(g, ui, rng, corp_id);
                        if 3 < rating_val {
                            short_score += 1;
                        } else if rating_val < 3 {
                            short_score -= 1;
                        }
                        if g.market_cap[corp_id] * Ext::from_f64(100.0) < g.net_worth * sf(1.25f32) {
                            short_score -= 1;
                        }
                        if g.net_worth * Ext::from_f64(5.0) < g.market_cap[corp_id] * Ext::from_f64(100.0) {
                            short_score += 1;
                        }
                        if 85 < corp_id {
                            let cf = show_cash_flow_projection(g, ui, rng, corp_id, 1);
                            if Ext::from_f64(5.0) < cf && g.tmp_ext1 != -Ext::from_f64(99.0) {
                                short_score += 1;
                            }
                        }
                        g.tmp_ext1 = Ext::from_f64(0.0);
                        if short_score < 4 {
                            f.rejected = -1;
                        }
                    }
                }
            }
        } else {
            f.rejected = -1;
        }
    }
}

/// Port of RunDbSearch_ListBonds (0x54cd15), GOSUB of RunDbSearch: the bond research list. Every active
/// corp with bonds issued that passes the criteria and has bonds in public hands (`CalcEntityFinancials`
/// → `BondsPublicHeld`) gets a line (name, coupon, maturity, yield `BondYtm` or " Nil " for D, rating with
/// `*` for convertibles, price, amount issued, publicly held, `@C` link); a footnote follows when a
/// convertible was listed. Box 389 gets the bond count (modes 1/2); mode 2 shows the list sorted by credit
/// rating (with a minimum rating checked) or maturity year, under a 4-line header, and ends the dialog
/// when a company was picked; the first time without the rating sort a hint is shown.
fn run_db_search_list_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut DbSearchDlg, f: &mut Frame) {
    let __content = crate::content::current();
    // DIM resultLines(1 TO 1600), ratingKeys(1 TO 1600), maturityKeys(1 TO 1600)
    f.result_lines = vec![PbStr::new(); 1601];
    f.rating_keys = vec![0; 1601];
    f.maturity_keys = vec![0; 1601];
    f.match_count = 0;
    f.corp_id = 11;
    while f.corp_id <= 1600 {
        let corp_id = f.corp_id;
        if g.entity_status[corp_id] == 1 {
            f.rejected = 0;
            if 0 < g.bonds_issued[corp_id] {
                run_db_search_test_criteria(g, ui, rng, f);
                if f.rejected != -1 {
                    calc_entity_financials(g, ui, rng, corp_id);
                    if 0 < g.bonds_public_held {
                        list_bond_line(g, f, corp_id);
                    }
                }
            }
        }
        f.corp_id += 1;
    }
    if f.has_convertible == 1 {
        f.match_count += 1;
        f.result_lines[cast::<usize, _>(f.match_count)] = string_fill(79, b"-");
        f.match_count += 1;
        f.result_lines[cast::<usize, _>(f.match_count)] = s(__content.string(6102));
    }
    if f.search_mode == 1 || f.search_mode == 2 {
        dlg.set_text(ctl::TXT_COUNT, str_trim(cast::<Ext, _>(f.bond_count)));
    }
    if f.search_mode == 1 {
        return;
    }
    if run_db_search_show_no_results(ui, f) {
        return;
    }
    let n = cast::<usize, _>(f.bond_count.max(0));
    if g.scr_credit_rating_on == 1 {
        // ARRAY SORT ratingKeys() FOR bondCount, TAGARRAY resultLines(), DESCEND
        pb_array_sort(&mut f.rating_keys[1..1 + n], Some(&mut f.result_lines[1..1 + n]), true, &|a, b| a < b);
    } else {
        // ARRAY SORT maturityKeys() FOR bondCount, TAGARRAY resultLines()
        pb_array_sort(&mut f.maturity_keys[1..1 + n], Some(&mut f.result_lines[1..1 + n]), false, &|a, b| a < b);
    }
    f.result_lines[cast::<usize, _>(f.match_count + 4)] = s("@END");
    f.match_count = 5;
    while f.match_count <= 1050 {
        g.text_lines[f.match_count] = f.result_lines[cast::<usize, _>(f.match_count - 4)].clone();
        f.match_count += 1;
    }
    f.result_lines[1] = s(__content.string(6103));
    f.result_lines[2] = s(__content.string(6104));
    f.result_lines[3] = s(__content.string(6105));
    f.result_lines[4] = s("--------------- ------ ---- ------ ------ ------ --------------- ----------------");
    for k in 1..=4i32 {
        g.text_lines[k] = f.result_lines[cast::<usize, _>(k)].clone();
    }
    f.corp_id = 5;
    g.text_box_title = s(__content.string(6106));
    if g.scr_credit_rating_on == 0 {
        g.text_box_title = replace(g.text_box_title.as_bytes(), __content.bytes(6107), __content.bytes(6108));
    }
    f.corp_id = g.sel_entity;
    g.text_box_mode = s("64");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    if g.sel_entity != f.corp_id {
        dlg.open = false;
    }
    if g.srch_sort_hint_shown == 0 && g.scr_credit_rating_on == 0 {
        let mut msg = s(__content.string(6109))
            + __content.string(6110);
        show_quick_message(g, ui, &mut msg);
        g.srch_sort_hint_shown = 1;
    }
}

/// The line of one bond issue in [`run_db_search_list_bonds`].
fn list_bond_line(g: &mut Globals, f: &mut Frame, corp_id: i32) {
    let __content = crate::content::current();
    f.match_count += 1;
    f.bond_count += 1;
    let n = cast::<usize, _>(f.match_count);
    let info = g.bond_issue_info[corp_id].clone();
    let mut line_text = g.ent_name[corp_id].clone() + &string_fill(15, b" ");
    line_text = mid(line_text.as_bytes(), 1, 15) + " ";
    let mut field_text = right(info.as_bytes(), 5);
    if val(field_text.as_bytes()) < Ext::from_f64(10.0) {
        field_text = s(" ") + &right(field_text.as_bytes(), 4);
    }
    line_text = line_text + &field_text + "% " + &mid(info.as_bytes(), 1, 4) + " ";
    if g.scr_credit_rating_on == 1 {
        f.rating_keys[n] = g.credit_rating_num[corp_id];
    } else {
        let sort_key = to_quad(val(mid(info.as_bytes(), 1, 4).as_bytes()));
        let sort_key = to_quad(cast::<Ext, _>(corp_id) + cast::<Ext, _>(sort_key) * Ext::from_f64(2000.0));
        f.maturity_keys[n] = cast::<i32, _>(sort_key);
    }
    field_text = s("      ") + &fmt(cast::<Ext, _>(g.bond_ytm), "###.00") + "%";
    field_text = right(field_text.as_bytes(), 6);
    if g.credit_rating_str == "D" {
        field_text = s(" Nil  ");
    }
    line_text = line_text + &field_text;
    let mut rating_text = g.credit_rating_str.clone();
    if rating_text.len() < 3 {
        rating_text = s(" ") + &rating_text + " ";
    }
    rating_text = mid(rating_text.as_bytes(), 1, 3);
    field_text = s("  ") + &rating_text;
    if instr(1, info.as_bytes(), b"cv") < 1 {
        field_text = field_text + " ";
    } else {
        field_text = field_text + "*";
        f.has_convertible = 1;
    }
    line_text = line_text + &field_text;
    field_text = fmt(g.bond_price[corp_id].get(), "* ###.00");
    line_text = line_text + &field_text;
    field_text = string_fill(13, b" ") + &trim(&fmt(cast::<Ext, _>(g.bonds_issued[corp_id]), "* ###,"));
    line_text = line_text + &right(field_text.as_bytes(), 13);
    field_text = string_fill(17, b" ") + &trim(&fmt(cast::<Ext, _>(g.bonds_public_held), "* ###,"));
    line_text = line_text + &right(field_text.as_bytes(), 17);
    g.entity_pick_mode = s(__content.string(6093));
    field_text = link_id(corp_id);
    f.result_lines[n] = line_text + &string_fill(20, b" ") + "@C" + &field_text;
}
