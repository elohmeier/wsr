//! Text reports and popups: `ShowTextReport` (0x41dfaa) with `ShowReportDialog` (0x41727f),
//! `ShowQuickMessage` (0x46e851), `DescribeDefunctEntity` (0x480a70), `NumberToWords` (0x4a18b7) and the
//! reports `ShowShareholderList` (0x421544), `ShowInterestRates` (0x45c781), `ShowEconomicStats`
//! (0x47dd94).
//!
//! Report protocol of the original: the caller fills `TextLines(1..)` (terminated by a line `"@END"`),
//! sets `TextBoxTitle`, optionally `TextHasLinks = 1` (lines may end in a 7-character link code `@C<id>`
//! / `@I<id>`, i.e. `"@C0123"` padded to the right edge), `TextBoxMode` (`"YESNO"`, a width number such
//! as `"54"`, …) and `EntityPickMode`, then calls `ShowTextReport`.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::assign_op_pattern, clippy::manual_range_contains)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::finance::{calc_gov_bond_yields, get_market_interest_rate};
use crate::logic::market::update_market_display;
use crate::pb::numeric::format;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, mid, replace, right, rtrim, space, string_fill, ucase, CaseMode};
use crate::pb::numeric::to_long;
use crate::state::Globals;
use crate::ui::{DialogKind, DialogUi, ReportAction, ReportOptions, Ui};

/// Controls created by ShowReportDialog (0x41727f), excluding print (handled by the host).
pub fn report_options(g: &Globals) -> ReportOptions {
    let __content = crate::content::current();
    let research = instr(1, g.text_box_title.as_bytes(), __content.bytes(6189)) > 0;
    ReportOptions {
        analyst_rating: research.then_some(g.cur_analyst_rating),
        rating_trade: research && matches!(g.cur_analyst_rating, 1 | 2 | 4 | 5)
            && crate::logic::shell::desk::control_enabled(g, crate::logic::shell::desk::ctl::BUY_STOCK),
        stock_chart: instr(1, g.text_box_title.as_bytes(), __content.bytes(7945)) > 0
            && g.chart_entity > 10 && g.sim_co_count == g.chart_entity && g.sel_entity != g.chart_entity,
    }
}

/// `RTRIM$(LTRIM$(STR$(n)))` for a LONG.
fn str_trim(n: i32) -> PbStr {
    let s = str_num(cast::<Ext, _>(n), 15);
    rtrim(ltrim(s.as_bytes()).as_bytes())
}

/// `FORMAT$(x, mask)` as a [`PbStr`].
fn fmt(x: Ext, mask: &str) -> PbStr {
    PbStr(format(x, mask.as_bytes()))
}

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A local `DIM lines(1 TO hi) AS STRING` of a report routine. Writing past `hi` is error 9 in the
/// original, which jumps to the routine's ON ERROR handler (`ShowRuntimeError`, then exit); [`Lines::set`]
/// returns `Err` so the port can leave the routine at the same point.
struct Lines {
    v: Vec<PbStr>,
}

struct OutOfRange;

impl Lines {
    fn new(hi: i32) -> Self {
        Lines { v: vec![PbStr::new(); hi as usize + 1] }
    }
    fn hi(&self) -> i32 {
        cast::<i32, _>(self.v.len()) - 1
    }
    fn get(&self, i: i32) -> Result<PbStr, OutOfRange> {
        if i < 1 || i > self.hi() { Err(OutOfRange) } else { Ok(self.v[cast::<usize, _>(i)].clone()) }
    }
    fn set(&mut self, i: i32, x: PbStr) -> Result<(), OutOfRange> {
        if i < 1 || i > self.hi() {
            return Err(OutOfRange);
        }
        self.v[cast::<usize, _>(i)] = x;
        Ok(())
    }
    /// `FOR lineNo = 1 TO 90: TextLines(lineNo) = lines(lineNo): NEXT`.
    fn copy_to_text_lines(&self, g: &mut Globals) {
        for line_no in 1..=90 {
            g.text_lines[line_no] = self.v[cast::<usize, _>(line_no)].clone();
        }
    }
}

/// Port of ShowQuickMessage (0x46e851).
/// Shows `msg_text` in a small popup. **BYREF:** like the original, `msg_text` is modified: every
/// `CRLF CRLF` becomes `@@` and every remaining `CRLF` `##` (the encoding of `TMP_QM.DAT`).
///
/// The original writes the encoded text to `DataPath + "TMP_QM.DAT"` and runs `qm.exe` (which shows it
/// as a timed popup and decodes the line breaks); the port shows the text as it was passed in (with its
/// line breaks) through `ui.msgbox(text, 0, "")`. The ticker pauses around it use the host's local clock.
pub fn show_quick_message(g: &mut Globals, ui: &mut dyn Ui, msg_text: &mut PbStr) {
    if msg_text.is_empty() {
        return;
    }
    crate::logic::turn::pause_ticker(g, &ui.local_time(), 2.0f32);
    let shown = msg_text.clone();
    while 0 < instr(1, msg_text.as_bytes(), b"\r\n") {
        *msg_text = replace(msg_text.as_bytes(), b"\r\n\r\n", b"@@");
        *msg_text = replace(msg_text.as_bytes(), b"\r\n", b"##");
    }
    // OPEN DataPath + "TMP_QM.DAT" FOR OUTPUT: PRINT #f, msgText: CLOSE; SHELL DataPath + "qm.exe " + DataPath
    ui.msgbox(&shown, 0, &PbStr::new());
    crate::logic::turn::pause_ticker(g, &ui.local_time(), 1.0f32);
}

/// `ShowQuickMessage(NewsMsg BYREF)`, including its CRLF substitutions and ticker pauses.
pub fn show_quick_news(g: &mut Globals, ui: &mut dyn Ui) {
    let mut message = std::mem::take(&mut g.news_msg);
    show_quick_message(g, ui, &mut message);
    g.news_msg = message;
}

/// Port of ShowTextReport (0x41dfaa).
/// Shows `TextLines()` (up to the first line containing `"@END"`) under `TextBoxTitle` through
/// [`Ui::report`], link codes included, then resets the report globals (`TextBoxTitle`,
/// `EntityPickMode`, `TextHasLinks`, `CurAnalystRating`, `ReportPrintLines`, and — in ShowReportDialog —
/// `TextLines(1..1050)`).
pub fn show_text_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if 0 < g.dlg_open_news_report {
        ui.end_dialog(DialogKind::Report);
        // (UI) an open report dialog is closed first (CONTROL SET TEXT 304, DIALOG END hReportDlg); its
        // TextBoxMode is saved and restored around the DIALOG END, i.e. left unchanged.
    }
    g.dlg_open_news_report = g.dlg_open_news_report + 1;
    if g.text_has_links == 1 {
        // The printable copy (link codes cut off) used by the dialog's PRINT button (PrintReportText).
        g.report_print_lines[0] = PbStr::new();
        for line_no in 1..=1050 {
            if 0 < instr(1, g.text_lines[line_no].as_bytes(), b"@END") {
                g.report_print_lines[0] = PbStr::from(str_num(cast::<Ext, _>(line_no - 1), 15).as_str());
                break;
            }
            let mut line_text = g.text_lines[line_no].clone();
            if 0 < instr(1, line_text.as_bytes(), b"@C") || 0 < instr(1, line_text.as_bytes(), b"@I") {
                let n = line_text.len();
                line_text = mid(line_text.as_bytes(), 1, n - 7);
            }
            g.report_print_lines[line_no] = rtrim(line_text.as_bytes());
        }
    }
    show_report_dialog(g, ui, rng);
    g.cur_analyst_rating = 0;
    g.text_box_title = PbStr::new();
    g.entity_pick_mode = PbStr::new();
    g.dlg_open_news_report = 0;
    g.report_print_lines.reset();
    g.text_has_links = 0;
    // ShowTextReport uses the SINGLE 0.8 constant at the relocated address 0x75e450. The dialog's
    // buttons use zero (PauseTicker's 0.4 default), but the enclosing routine has a longer pause.
    crate::logic::turn::pause_ticker(g, &ui.local_time(), 0.8f32);
}

/// Port of ShowReportDialog (0x41727f), state effects only.
/// Builds the report dialog from `TextLines(1..n)` (n = the first line equal to `"@END"`, which is shown
/// as `" "`, else 1050) and runs it modally; the port hands lines 1..n-1 (all 1050 without `@END`) to
/// [`Ui::report`]. Clears `TargetCorp`, `ReportLinkCode`, `ReportDlgYesNo`, a numeric `TextBoxMode`
/// (dialog width), `ListItemNoun` (when a pick hint is shown) and `TextLines(1..1050)`; an empty title
/// becomes "Financial News Report".
///
/// The dialog's buttons come back as a [`ReportAction`] and are applied like ReportDlg_OkBtn (0x418678),
/// ReportDlg_NoBtn (0x41878a) and ReportDlg_ListClick (0x4184db).
///
/// Runs `UpdateMarketDisplay` ([`update_market_display`]: market tick, >1,700 `RND` draws) before the
/// dialog when `SelEntity = 1` and after it when `SelEntity > 0`.
pub fn show_report_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    if g.sel_entity == 1 {
        let seconds = ui.local_time().time_secs();
        update_market_display(g, ui, rng, seconds);
    }
    g.target_corp = 0;
    g.report_link_code = PbStr::new();
    g.report_dlg_yes_no = PbStr::new();
    if Ext::from_f64(0.0) < val(g.text_box_mode.as_bytes()) {
        // extraWidth = VAL(TextBoxMode) (dialog geometry)
        g.text_box_mode = PbStr::new();
    }
    if g.text_box_title.is_empty() {
        g.text_box_title = s(__content.string(7946));
    }
    let options = report_options(g);
    let mut line_count = 1050;
    let mut end_found = false;
    for line_idx in 1..=1050 {
        if g.text_lines[line_idx] == "@END" {
            line_count = line_idx;
            end_found = true;
            break;
        }
    }
    let shown_count = if end_found { line_count - 1 } else { line_count };
    let lines: Vec<PbStr> = (1..=shown_count).map(|i| g.text_lines[i].clone()).collect();
    if end_found {
        g.text_lines[line_count] = s(" ");
    }
    for line_idx in 1..=1050 {
        g.text_lines[line_idx] = PbStr::new();
    }
    // (UI) OK / YES-NO buttons
    if !g.entity_pick_mode.is_empty() && !(g.text_box_mode == "YESNO") && !(g.text_box_mode == "CHEATRESULT") {
        // (UI) "(Click on <noun> to select)" hint label
        g.list_item_noun = PbStr::new();
    }
    let title = g.text_box_title.clone();
    let yes_no = g.text_box_mode == "YESNO";
    let mut report_ui = DialogUi::new(ui, DialogKind::Report);
    loop {
        match report_ui.report_with_actions(&title, &lines, yes_no, &options) {
            ReportAction::Ok => { report_ok_button(g, &mut report_ui, rng); break; },
            ReportAction::No => {
                // ReportDlg_NoBtn (0x41878a)
                g.report_dlg_yes_no = s("N");
                g.text_box_mode = PbStr::new();
                g.report_link_code = PbStr::new();
                crate::logic::turn::pause_ticker(g, &report_ui.local_time(), 0.0f32);
                break;
            }
            ReportAction::Link(code) => {
                // ReportDlg_ListClick (0x4184db) stores the clicked line's link code, then OK follows it.
                g.report_link_code = code;
                report_ok_button(g, &mut report_ui, rng);
                break;
            }
            ReportAction::Closed => break,
            ReportAction::Chart if options.stock_chart => {
                let now = report_ui.local_time();
                let entity = g.chart_entity;
                crate::logic::shell::dialogs::show_stock_chart(g, &mut report_ui, rng,
                    &crate::logic::shell::Env::new(&now), entity);
                if !report_ui.open { break; }
                g.text_box_title = title.clone();
            }
            ReportAction::Rating if options.rating_trade => {
                let now = report_ui.local_time();
                let ended = crate::logic::info_screens::holders::report_dlg_rating_click(g, &mut report_ui, rng, &now);
                if ended || !report_ui.open { break; }
            }
            _ => {} // A host cannot activate controls absent from this report.
        }
    }
    // DlgProcReportDialog (0x417e54), WM_CLOSE: sent by DIALOG END (every button) and the close box.
    g.report_link_code = PbStr::new();
    if g.text_box_mode == "YESNO" && g.report_dlg_yes_no.is_empty() {
        g.report_dlg_yes_no = s("N");
    }
    g.text_box_mode = PbStr::new();
    if 0 < g.sel_entity {
        let seconds = ui.local_time().time_secs();
        update_market_display(g, ui, rng, seconds);
    }
}

/// Port of ReportDlg_OkBtn (0x418678): YES on a YES/NO report, else follow a clicked link.
fn report_ok_button(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    if g.text_box_mode == "YESNO" {
        g.report_dlg_yes_no = s("Y");
        g.text_box_mode = PbStr::new();
    } else if !(g.text_box_mode == "CHEATRESULT") && !g.report_link_code.is_empty() {
        // FollowReportLink(ReportLinkCode): selects the linked corp/industry (result unused).
        let link = g.report_link_code.clone();
        crate::logic::trading::follow_report_link(g, ui, rng, &link);
    }
    g.report_link_code = PbStr::new();
    crate::logic::turn::pause_ticker(g, &ui.local_time(), 0.0f32);
}

/// Port of GetRandomRemark (0x480da1).
/// `TmpNewsStr` = the remark returned by `RemarksData(1)` in WSRDLL.DLL.
///
/// The host keeps the DLL-private remark pool across games (see `extensions::remarks`).
fn get_random_remark(g: &mut Globals, ui: &mut dyn Ui) {
    g.tmp_news_str = ui.random_remark();
}

/// Port of DescribeDefunctEntity (0x480a70).
/// For a liquidated entity (`EntityStatus = 2`) pops up "<name> has been liquidated[ into <buyer>] and no
/// longer exists." or, if `EntMiscData = "BKRUP"`, "<name> has gone broke, and no longer exists." (for a
/// player: "… and <random remark /  [runtime resource wording] >.").
pub fn describe_defunct_entity(g: &mut Globals, ui: &mut dyn Ui, entity_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.entity_status[entity_id] != 2 {
        return;
    }
    let mut fate_text = s(__content.string(7947));
    let buyer = to_long(val(g.ent_misc_data[entity_id].as_bytes()));
    if buyer < 11 || 1600 < buyer {
        fate_text = fate_text + ", and ";
    } else {
        fate_text = fate_text + " into " + &g.ent_name[buyer] + " and ";
    }
    let mut status_text = s(__content.string(7948));
    if g.ent_misc_data[entity_id] == "BKRUP" {
        fate_text = s(__content.string(7949));
        if entity_id <= g.num_players {
            get_random_remark(g, ui);
            status_text = g.tmp_news_str.clone();
            if status_text.is_empty() {
                status_text = s(__content.string(7950));
            }
        }
    }
    let mut msg_text = g.ent_name[entity_id].clone() + &fate_text + &status_text + ".";
    show_quick_message(g, ui, &mut msg_text);
}

/// Port of NumberToWords (0x4a18b7).
/// `" " + words`: |number| as an English word for 1..20, 25, 30, 40, …, 90 (75 = "seventy-five"), else the
/// digits; with `as_percent > 0` a word gets " percent" and digits get "%". If `max_len > 0` and the
/// result (without the leading blank) is longer, the digits (+ "%") are used instead.
pub fn number_to_words(number: i32, max_len: i32, as_percent: i32) -> PbStr {
    let __content = crate::content::current();
    let abs_value = to_long((cast::<Ext, _>(number)).abs()); // fild / fabs / fistp: ABS(-2^31) -> 0x80000000
    let mut words = match abs_value {
        1 => s("one"),
        2 => s("two"),
        3 => s("three"),
        4 => s("four"),
        5 => s("five"),
        6 => s("six"),
        7 => s("seven"),
        8 => s("eight"),
        9 => s("nine"),
        10 => s("ten"),
        11 => s("eleven"),
        12 => s("twelve"),
        13 => s(__content.string(7951)),
        14 => s(__content.string(7952)),
        15 => s("fifteen"),
        16 => s("sixteen"),
        17 => s(__content.string(7953)),
        18 => s(__content.string(7954)),
        19 => s(__content.string(7955)),
        20 => s("twenty"),
        25 => s(__content.string(7956)),
        30 => s("thirty"),
        40 => s("forty"),
        50 => s("fifty"),
        60 => s("sixty"),
        70 => s("seventy"),
        75 => s(__content.string(7957)),
        80 => s("eighty"),
        90 => s("ninety"),
        _ => str_trim(abs_value),
    };
    if 0 < as_percent {
        if val(words.as_bytes()) <= Ext::from_f64(0.0) {
            words = words + __content.string(7958);
        } else {
            words = words + "%";
        }
    }
    if 0 < max_len && max_len < words.len() {
        words = str_trim(abs_value);
        if 0 < as_percent {
            words = words + "%";
        }
    }
    s(" ") + &words
}

/// `RIGHT$("000" + RTRIM$(LTRIM$(STR$(id))), 4)`: the 4-digit id of a `@C` link code.
fn link_code(id: i32) -> PbStr {
    let t = s("000") + &str_trim(id);
    right(t.as_bytes(), 4)
}

/// Port of ShowShareholderList (0x421544).
/// Report " [runtime resource wording] ": country of incorporation, every holder of `corp_ref` with its
/// percentage (short positions only with `show_shorts != 0`), the public's share and the controller.
/// Calls `UpdateCorpControl` for every holder listed and for `corp_ref` (state changes, news). With
/// `EntityPickMode = "GetCorp"/"ChangeCorp"` holder lines get `@C<id>` link codes.
///
/// Error 9 (more than 90 report lines, or a controller id 0) ends the routine like its ON ERROR handler
/// (which only shows `ShowRuntimeError`): no report is shown.
pub fn show_shareholder_list(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, corp_ref: i32, show_shorts: i32) {
    let _ = shareholder_list(g, ui, rng, corp_ref, show_shorts);
}

fn shareholder_list(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_ref: i32,
    show_shorts: i32,
) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
    let mut lines = Lines::new(90);
    // (UI) DIALOG END DlgWhoOwnsWhat
    let mut has_links = 0;
    if corp_ref < 1 || 1600 < corp_ref {
        return Err(OutOfRange); // EntityStatus(corpRef): error 9
    }
    if g.entity_status[corp_ref] != 1 {
        if g.entity_status[corp_ref] == 2 {
            describe_defunct_entity(g, ui, corp_ref);
        }
        return Ok(());
    }
    if corp_ref < 11 {
        let mut line_text = g.ent_name[corp_ref].clone();
        if line_text.is_empty() {
            line_text = s(__content.string(7959));
        }
        let mut msg = line_text + __content.string(7960);
        show_quick_message(g, ui, &mut msg);
        return Ok(());
    }
    let mut line_no;
    // (EntityStatus(corpRef) = 1 again; the " [runtime resource wording] " branch is unreachable)
    {
        lines.set(1, s(__content.string(7961)) + &g.country_name[g.country_idx[corp_ref]])?;
        let n = lines.get(1)?.len() - 2;
        lines.set(2, s("  ") + &string_fill(n, b"-"))?;
        lines.set(
            4,
            s(__content.string(7962)) + &g.ent_name[corp_ref] + " (" + &g.ent_symbol[corp_ref] + "):",
        )?;
        lines.set(5, string_fill(60, b"-"))?;
        line_no = 5;
        for owner_id in 1..=1600 {
            if g.entity_status[owner_id] != 1 {
                continue;
            }
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, owner_id)]) != corp_ref {
                    continue;
                }
                if g.hold_stock_pct[(slot, owner_id)] < 0 && show_shorts == 0 {
                    continue;
                }
                line_no = line_no + 1;
                let holder_id = owner_id;
                update_corp_control(g, ui, rng, holder_id);
                let hold_pct = g.hold_stock_pct[(slot, owner_id)];
                let mut suffix_text = s(" (") + &g.ent_symbol[holder_id] + ")";
                if holder_id < 11 {
                    suffix_text = PbStr::new();
                }
                let mut line_text = g.ent_name[holder_id].clone() + &suffix_text + &space(37);
                line_text = mid(line_text.as_bytes(), 1, 37);
                suffix_text = fmt(cast::<Ext, _>(hold_pct), "* ####") + "%";
                lines.set(line_no, line_text + &right(suffix_text.as_bytes(), 5))?;
                if hold_pct < 0 {
                    lines.set(line_no, lines.get(line_no)? + __content.string(7963))?;
                }
                if !(g.entity_pick_mode == "GetCorp") && !(g.entity_pick_mode == __content.string(6093)) {
                    continue;
                }
                if holder_id < 11 && g.cur_player != holder_id && !(g.game_end_flag == "Y") {
                    if g.entity_pick_mode == __content.string(6093) {
                        suffix_text = s("0000");
                    }
                    // Quirk: otherwise suffixText still holds the percentage text ("   25%"), which
                    // becomes the "link code".
                } else {
                    has_links = 1;
                    suffix_text = link_code(holder_id);
                }
                lines.set(line_no, lines.get(line_no)? + &space(45) + "@C" + &suffix_text)?;
            }
        }
        update_corp_control(g, ui, rng, corp_ref);
        line_no = line_no + 2;
        let line_text = right(fmt(cast::<Ext, _>(g.public_pct), "* ###").as_bytes(), 3);
        lines.set(line_no, s(__content.string(7964)) + &line_text + "%")?;
        line_no = line_no + 1;
        lines.set(line_no, string_fill(60, b"-"))?;
        line_no = line_no + 2;
        let holder_id = g.corp_controller[corp_ref];
        if holder_id < 1 || 1600 < holder_id {
            return Err(OutOfRange); // EntSymbol(holderId): error 9
        }
        let mut suffix_text = s(" (") + &g.ent_symbol[holder_id] + ")";
        if holder_id < 11 {
            suffix_text = PbStr::new();
        }
        if holder_id < 11 && g.cur_player != holder_id {
            suffix_text = PbStr::new();
        } else {
            let code = link_code(holder_id);
            suffix_text = suffix_text + &space(70) + "@C" + &code;
        }
        let mut line_text = g.ent_name[holder_id].clone() + &suffix_text;
        if corp_ref == holder_id || holder_id == 0 {
            line_text = s(__content.string(7965));
        }
        lines.set(line_no, s(__content.string(7966)) + &line_text)?;
    }
    lines.set(line_no + 2, s("@END"))?;
    lines.copy_to_text_lines(g);
    g.text_box_title = s(__content.string(7967))
        + &g.ent_name[corp_ref]
        + " ("
        + &g.ent_symbol[corp_ref]
        + ") ===";
    if has_links == 0 {
        g.entity_pick_mode = PbStr::new();
    }
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    Ok(())
}

/// Port of ShowInterestRates (0x45c781).
/// Report "CURRENT INTEREST RATES": prime rate, government bond yields, T-bill, CD, SOFR, consumer /
/// mortgage / subprime rates, the central bank's policy, and explanatory notes.
pub fn show_interest_rates(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = Lines::new(100);
    let rate_fmt = "* ##.00";
    // GetMarketInterestRate returns EXT; stored in DOUBLE locals
    let consumer_rate: Ext = get_market_interest_rate(g, rng, 1);
    let mortgage_rate: Ext = get_market_interest_rate(g, rng, 2);
    let subprime_rate: Ext = get_market_interest_rate(g, rng, 3);
    calc_gov_bond_yields(g);
    let _ = interest_rate_lines(g, &mut lines, rate_fmt, consumer_rate, mortgage_rate, subprime_rate);
    lines.copy_to_text_lines(g);
    g.text_box_title = s(__content.string(7968));
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

fn interest_rate_lines(
    g: &Globals,
    lines: &mut Lines,
    rate_fmt: &str,
    consumer_rate: Ext,
    mortgage_rate: Ext,
    subprime_rate: Ext,
) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
    const CD_FACTOR: f64 = 0.98f32 as f64; // fld dword [0x75e4e0]
    lines.set(2, s(__content.string(7969)))?;
    lines.set(3, s("======================================"))?;
    lines.set(4, s(__content.string(7970)) + &fmt(cast::<Ext, _>(g.prime_rate), rate_fmt) + "%")?;
    lines.set(5, s(__content.string(7971)) + &fmt(cast::<Ext, _>(g.long_bond_yield), rate_fmt) + "%")?;
    lines.set(6, s(__content.string(7972)) + &fmt(cast::<Ext, _>(g.short_bond_yield), rate_fmt) + "%")?;
    lines.set(7, s(__content.string(7973)) + &fmt(cast::<Ext, _>(g.t_bill_rate), rate_fmt) + "%")?;
    lines.set(8, s(__content.string(7974)) + &fmt(CD_FACTOR * cast::<Ext, _>(g.bank_cd_rate), rate_fmt) + "%")?;
    lines.set(9, s(__content.string(7975)) + &fmt(cast::<Ext, _>(g.sofr_rate), rate_fmt) + "%")?;
    lines.set(10, s(__content.string(7976)) + &fmt(consumer_rate, rate_fmt) + "%")?;
    lines.set(11, s(__content.string(7977)) + &fmt(mortgage_rate, rate_fmt) + "%")?;
    lines.set(12, s(__content.string(7978)) + &fmt(subprime_rate, rate_fmt) + "%")?;
    lines.set(13, lines.get(3)?)?;
    let mut policy_text = s("NEUTRAL");
    if g.monetary_policy == 0 {
        policy_text = s("EASING");
    }
    if g.monetary_policy == 2 {
        policy_text = s("TIGHTENING");
    }
    lines.set(
        15,
        s("              ") + &ucase(g.central_bank_name.as_bytes(), CaseMode::Ansi) + __content.string(7979) + &policy_text,
    )?;
    let mut line_no = 2;
    while line_no <= 13 {
        lines.set(line_no, space(20) + &lines.get(line_no)?)?;
        line_no += 1;
    }
    // line_no = 14 after the FOR loop
    lines.set(line_no + 4, s(__content.string(7980)))?;
    lines.set(line_no + 5, s(__content.string(7981)))?;
    lines.set(line_no + 6, s(__content.string(7982)))?;
    line_no = line_no + 1;
    lines.set(line_no + 7, s(__content.string(7983)))?;
    lines.set(line_no + 8, s(__content.string(7984)))?;
    lines.set(line_no + 9, s(__content.string(7985)))?;
    lines.set(line_no + 10, s(__content.string(7986)))?;
    lines.set(line_no + 11, s(__content.string(7987)))?;
    lines.set(line_no + 12, s(__content.string(7988)))?;
    lines.set(line_no + 13, s(__content.string(7989)))?;
    lines.set(line_no + 14, s(__content.string(7990)))?;
    lines.set(line_no + 15, s(__content.string(7991)))?;
    lines.set(
        line_no + 16,
        s(__content.string(7992)) + &mid(g.long_bond_desc.as_bytes(), 1, 4) + ".",
    )?;
    line_no = line_no + 1;
    lines.set(line_no + 17, s(__content.string(7993)))?;
    lines.set(line_no + 18, s(__content.string(7994)))?;
    lines.set(line_no + 19, s(__content.string(7995)))?;
    lines.set(line_no + 20, s(__content.string(7996)))?;
    lines.set(line_no + 21, s(__content.string(7997)))?;
    lines.set(line_no + 22, s(__content.string(7998)))?;
    lines.set(line_no + 23, s(__content.string(7999)))?;
    line_no = line_no + 1;
    lines.set(line_no + 24, s(__content.string(8000)))?;
    lines.set(line_no + 25, s(__content.string(8001)))?;
    lines.set(line_no + 26, s(__content.string(8002)))?;
    lines.set(line_no + 27, s(__content.string(8003)))?;
    line_no = line_no + 33;
    lines.set(line_no + 2, s("@END"))?;
    Ok(())
}

/// The action 7/8 queries of CheckGovtBondLargeTrade (0x5bbcc5): 7 = total government debt outstanding
/// (`ROUND(ExchangeRate * 125000000)` grown 6% a year since `StartYear`, at most 50 years), 8 = the part of
/// it held by players, banks and funds (entities 1..130 and 1581..1583: long + short government bond
/// face).
///
/// Only these two branches are ported here (the other actions are the large-trade check with MSGBOX).
fn govt_debt_query(g: &Globals, action: i32) -> Ext {
    const GROWTH: f64 = 1.06f32 as f64; // fld dword [0x75ecb0]
    const HALF: f64 = 0.5f32 as f64; // fmul dword [0x75e384]
    let mut total_issue = crate::pb::numeric::to_quad(g.exchange_rate * Ext::from_f64(125_000_000.0));
    let mut years = g.cur_year - g.start_year;
    if 0 < years {
        if 50 < years {
            years = 50;
        }
        for _ in 1..=years {
            total_issue = crate::pb::numeric::to_quad(GROWTH * cast::<Ext, _>(total_issue));
        }
    }
    let mut float_a = crate::pb::numeric::to_quad(cast::<Ext, _>(total_issue) * HALF);
    let mut float_b = float_a;
    for i in 1..=1583 {
        if i <= 130 || 1581 <= i {
            float_a = float_a.wrapping_sub(g.long_gov_bond_face[i]);
            float_b = float_b.wrapping_sub(g.short_gov_bond_face[i]);
        }
    }
    match action {
        7 => cast::<Ext, _>(total_issue),
        8 => cast::<Ext, _>(total_issue) - (cast::<Ext, _>(float_b) + cast::<Ext, _>(float_a)),
        _ => Ext::from_f64(0.0),
    }
}

/// Port of ShowEconomicStats (0x47dd94).
/// Report "ECONOMIC DATA": GDP growth, prime rate, government bond prices, housing starts, tax rates, oil
/// price, the stock index, the national debt, and notes [1]..[9].
pub fn show_economic_stats(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = Lines::new(120);
    let _ = economic_stats_lines(g, &mut lines);
    lines.copy_to_text_lines(g);
    g.text_box_title = s(__content.string(8004));
    g.entity_pick_mode = PbStr::new();
    g.text_has_links = 1;
    g.text_box_mode = s("54");
    show_text_report(g, ui, rng);
}

fn economic_stats_lines(g: &Globals, lines: &mut Lines) -> Result<(), OutOfRange> {
    let __content = crate::content::current();
    lines.set(2, s(__content.string(8005)))?;
    lines.set(3, s("                 -----------------------------"))?;
    lines.set(5, s(__content.string(8006)) + &fmt(cast::<Ext, _>(g.gdp_growth), "* ##.0") + " %  [1]")?;
    lines.set(6, s(__content.string(8007)) + &fmt(cast::<Ext, _>(g.prime_rate), "* ##.00") + " %")?;

    let mut pad_text = PbStr::new();
    let mut value_text = right(g.long_bond_desc.as_bytes(), 5);
    if val(value_text.as_bytes()) < Ext::from_f64(10.0) {
        value_text = right(value_text.as_bytes(), 4);
        pad_text = s(" ");
    }
    let mut num_text = fmt(cast::<Ext, _>(g.long_bond_price), "* ###.00");
    num_text = right(num_text.as_bytes(), 6);
    lines.set(
        7,
        s(__content.string(8008))
            + &value_text
            + __content.string(8009)
            + &mid(g.long_bond_desc.as_bytes(), 1, 4)
            + ": "
            + &pad_text
            + &num_text
            + __content.string(8010),
    )?;
    value_text = right(g.short_bond_desc.as_bytes(), 5);
    pad_text = PbStr::new();
    if val(value_text.as_bytes()) < Ext::from_f64(10.0) {
        value_text = right(value_text.as_bytes(), 4);
        pad_text = s(" ");
    }
    num_text = fmt(cast::<Ext, _>(g.short_bond_price), "* ###.00");
    num_text = right(num_text.as_bytes(), 6);
    lines.set(
        8,
        s(__content.string(8008))
            + &value_text
            + __content.string(8009)
            + &mid(g.short_bond_desc.as_bytes(), 1, 4)
            + ": "
            + &pad_text
            + &num_text
            + __content.string(8011),
    )?;
    lines.set(9, s(__content.string(8012)) + &fmt(cast::<Ext, _>(g.housing_starts), "* ####") + __content.string(8013))?;
    lines.set(10, s(__content.string(8014)))?;
    lines.set(11, s(__content.string(8015)) + &fmt(cast::<Ext, _>(g.corp_tax_rate), "* ##") + " %")?;
    lines.set(12, s(__content.string(8016)) + &fmt(cast::<Ext, _>(5 + g.corp_tax_rate), "* ##") + " %")?;
    lines.set(13, s(__content.string(8017)) + &fmt(cast::<Ext, _>(g.cap_gains_tax_rate), "* ##") + " %")?;
    lines.set(14, s(__content.string(8018)))?;
    pad_text = s(__content.string(8019));
    value_text = fmt(cast::<Ext, _>(g.wealth_tax_rate), "* ##") + " %  ";
    if g.wealth_tax_rate < 1 {
        value_text = pad_text.clone();
    }
    lines.set(15, s(__content.string(8020)) + &value_text + "[3]")?;
    value_text = s("   2 %  ");
    if g.corp_shares_tax_country < 1 {
        value_text = pad_text.clone();
    }
    lines.set(16, s(__content.string(8021)) + &value_text + "[4]")?;
    value_text = fmt(cast::<Ext, _>(g.carbon_tax_rate), "* ##") + " %  ";
    if g.carbon_tax_rate < 1 {
        value_text = pad_text.clone();
    }
    lines.set(17, s(__content.string(8022)) + &value_text + "[5]")?;
    value_text = fmt(cast::<Ext, _>(g.oil_windfall_tax_rate), "* ##") + " %  ";
    if g.oil_windfall_tax_rate < 1 {
        value_text = pad_text.clone();
    }
    lines.set(18, s(__content.string(8023)) + &value_text + "[6]")?;
    value_text = fmt(cast::<Ext, _>(g.holding_co_tax_rate), "* ##") + " %  ";
    if g.holding_co_tax_rate < 1 {
        value_text = pad_text.clone();
    }
    lines.set(19, s(__content.string(8024)) + &value_text + "[7]")?;
    value_text = fmt(cast::<Ext, _>(g.health_care_tax_rate), "* ##") + " %  ";
    if g.health_care_tax_rate < 1 {
        value_text = pad_text.clone();
    }
    lines.set(20, s(__content.string(8025)) + &value_text + "[8]")?;
    // fild word 100; fmul dword OilPrice
    lines.set(
        21,
        s(__content.string(8026)) + &fmt(Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price), "* ###.00") + __content.string(8027),
    )?;
    num_text = fmt(g.stock_index, "* ######.00");
    lines.set(22, s(__content.string(8028)) + &right(num_text.as_bytes(), 9))?;
    let national_debt = govt_debt_query(g, 7);
    num_text = fmt(national_debt, "* ##################,");
    lines.set(23, s(__content.string(8029)) + &right(num_text.as_bytes(), 15) + "  " + &g.million_prefix_uc + "il.")?;
    lines.set(24, s("-----------------------------------------------------------------"))?;
    lines.set(25, s(__content.string(8030)))?;
    lines.set(26, s(__content.string(8031)))?;
    lines.set(27, s(__content.string(8032)))?;
    lines.set(29, s(__content.string(8033)))?;
    lines.set(30, s(__content.string(8034)))?;
    lines.set(
        31,
        s(__content.string(8035))
            + &mid(g.long_bond_desc.as_bytes(), 1, 4)
            + __content.string(8036)
            + &mid(g.short_bond_desc.as_bytes(), 1, 4)
            + ".",
    )?;
    lines.set(33, s(__content.string(8037)))?;
    lines.set(34, s(__content.string(8038)))?;
    if g.difficulty_level < 3 {
        lines.set(33, s(__content.string(8039)))?;
        lines.set(34, s(__content.string(8040)))?;
    }
    lines.set(36, s(__content.string(8041)))?;
    lines.set(37, s(__content.string(8042)))?;
    if g.corp_shares_tax_country < 1 {
        value_text = s(__content.string(8043));
    } else {
        value_text = g.country_name[g.corp_shares_tax_country].clone() + ").";
    }
    lines.set(38, s(__content.string(8044)) + &value_text + __content.string(8045))?;
    lines.set(40, s(__content.string(8046)))?;
    lines.set(41, s(__content.string(8047)))?;
    lines.set(42, s(__content.string(8048)))?;
    lines.set(43, s(__content.string(8049)))?;
    lines.set(44, s(__content.string(8050)))?;
    lines.set(46, s(__content.string(8051)))?;
    lines.set(47, s(__content.string(8052)))?;
    lines.set(48, s(__content.string(8053)))?;
    lines.set(49, s(__content.string(8054)))?;
    lines.set(51, s(__content.string(8055)))?;
    lines.set(52, s(__content.string(8056)))?;
    lines.set(53, s(__content.string(8057)))?;
    lines.set(54, s(__content.string(8058)))?;
    lines.set(55, s(__content.string(8059)))?;
    lines.set(57, s(__content.string(8060)))?;
    lines.set(58, s(__content.string(8061)))?;
    lines.set(59, s(__content.string(8062)))?;
    lines.set(60, s(__content.string(8063)))?;
    let gov_bonds_held = govt_debt_query(g, 8);
    if gov_bonds_held < national_debt {
        lines.set(23, lines.get(23)? + "  [9]")?;
        lines.set(
            62,
            s(__content.string(8064))
                + &rtrim(ltrim(&format(gov_bonds_held, b"##############,")).as_bytes())
                + " "
                + &g.million_prefix_uc
                + ".",
        )?;
        lines.set(63, s(__content.string(8065)) + &g.currency_name + ")")?;
    }
    lines.set(64, lines.get(24)?)?;
    for line_no in 2..=63 {
        lines.set(line_no, s("       ") + &lines.get(line_no)?)?;
    }
    lines.set(65, s("@END"))?;
    Ok(())
}
