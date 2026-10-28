//! Unit tests for the report routines: ShowTextReport (0x41dfaa), ShowQuickMessage (0x46e851),
//! DescribeDefunctEntity (0x480a70), NumberToWords (0x4a18b7), ShowShareholderList (0x421544),
//! ShowInterestRates (0x45c781), ShowEconomicStats (0x47dd94).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


use wsr_core::logic::report::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

fn p(s: &str) -> PbStr {
    PbStr::from(s)
}

/// Records every report and message box.
#[derive(Default)]
struct Rec {
    reports: Vec<(String, Vec<String>)>,
    msgs: Vec<String>,
    news: Vec<String>,
    now: Option<wsr_core::logic::newgame::LocalTime>,
}

impl Ui for Rec {
    fn local_time(&self) -> wsr_core::logic::newgame::LocalTime {
        self.now.unwrap_or(wsr_core::logic::newgame::LocalTime::HARNESS)
    }
    fn msgbox(&mut self, text: &PbStr, _style: i32, _title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        1
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push((title.to_display(), lines.iter().map(|l| l.to_display()).collect()));
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

#[test]
fn number_to_words() {
    let __content = wsr_core::content::current();
    let w = |n, m, pct| wsr_core::logic::report::number_to_words(n, m, pct).to_display();
    assert_eq!(w(5, 0, 0), " five");
    assert_eq!(w(-75, 0, 0), __content.string(13970));
    assert_eq!(w(5, 0, 1), " five percent");
    assert_eq!(w(21, 0, 0), " 21");
    assert_eq!(w(21, 0, 1), " 21%");
    assert_eq!(w(-33, 0, 1), " 33%");
    assert_eq!(w(0, 0, 1), " 0 percent");
    assert_eq!(w(0, 0, 0), " 0");
    // too long: digits instead
    assert_eq!(w(75, 5, 1), " 75%");
    assert_eq!(w(75, 12, 0), __content.string(13970));
    assert_eq!(w(75, 11, 0), " 75");
    assert_eq!(w(5, 12, 1), " five percent");
}

#[test]
fn quick_message_encodes_in_place() {
    let mut g = Globals::default();
    let mut ui = Rec::default();
    let mut m = p("a\r\n\r\nb\r\nc");
    show_quick_message(&mut g, &mut ui, &mut m);
    assert_eq!(m, p("a@@b##c"));
    assert_eq!(ui.msgs, vec!["a\r\n\r\nb\r\nc".to_string()]);
    let mut m = p("x\r\n\r\n\r\ny");
    show_quick_message(&mut g, &mut ui, &mut m);
    assert_eq!(m, p("x@@##y"));
    let mut m = PbStr::new();
    show_quick_message(&mut g, &mut ui, &mut m);
    assert_eq!(ui.msgs.len(), 2, "empty text shows nothing");
    assert_eq!(g.ticker_pause_until, 36_001);
}

#[test]
fn modal_pause_uses_the_host_clock() {
    use wsr_core::logic::newgame::LocalTime;
    let mut g = Globals::default();
    let mut ui = Rec { now: Some(LocalTime { hour: 14, minute: 3, second: 5, millis: 600, ..LocalTime::HARNESS }), ..Rec::default() };
    show_quick_message(&mut g, &mut ui, &mut p("Notice"));
    assert_eq!(g.ticker_pause_until, 50_587);
    g.text_lines[1] = p("@END");
    show_text_report(&mut g, &mut ui, &mut Rng::default());
    assert_eq!(g.ticker_pause_until, 50_586);
    ui.now = Some(LocalTime::HARNESS);
    g.text_lines[1] = p("@END");
    show_text_report(&mut g, &mut ui, &mut Rng::default());
    assert_eq!(g.ticker_pause_until, 36_001);
}

#[test]
fn text_report_protocol() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.text_lines[1] = p("first");
    g.text_lines[2] = p(__content.string(14431));
    g.text_lines[3] = p("@END");
    g.text_lines[4] = p("after");
    g.text_box_title = p("=== T ===");
    g.text_box_mode = p("54");
    g.entity_pick_mode = p("GetCorp");
    g.list_item_noun = p("offer");
    g.text_has_links = 1;
    g.target_corp = 9;
    g.cur_analyst_rating = 3;
    g.dlg_open_news_report = 0;
    show_text_report(&mut g, &mut ui, &mut wsr_core::pb::rng::Rng::default());
    assert_eq!(ui.reports.len(), 1);
    assert_eq!(ui.reports[0].0, "=== T ===");
    assert_eq!(ui.reports[0].1, vec!["first", __content.string(14431)]);
    for i in 1..=1050 {
        assert!(g.text_lines[i].is_empty(), "TextLines({i}) cleared");
    }
    assert!(g.text_box_title.is_empty());
    assert!(g.text_box_mode.is_empty(), "numeric TextBoxMode is consumed");
    assert!(g.entity_pick_mode.is_empty());
    assert!(g.list_item_noun.is_empty());
    assert_eq!(g.text_has_links, 0);
    assert_eq!(g.target_corp, 0);
    assert_eq!(g.cur_analyst_rating, 0);
    assert_eq!(g.dlg_open_news_report, 0);
    assert!(g.report_print_lines.as_slice().iter().all(|l| l.is_empty()));
}

#[test]
fn text_report_defaults() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.text_lines[1] = p("only");
    g.text_box_mode = p("YESNO");
    g.entity_pick_mode = p("GetCorp");
    g.list_item_noun = p("offer");
    show_text_report(&mut g, &mut ui, &mut wsr_core::pb::rng::Rng::default());
    assert_eq!(ui.reports[0].0, __content.string(7946));
    assert_eq!(ui.reports[0].1.len(), 1050, "no @END: all 1050 lines");
    assert_eq!(g.text_box_mode, p(""), "YES on a YESNO report clears the mode (ReportDlg_OkBtn)");
    assert_eq!(g.report_dlg_yes_no, p("Y"));
    assert_eq!(g.list_item_noun, p("offer"), "no pick hint in YESNO mode");
}

fn corp(g: &mut Globals, id: i32, name: &str, sym: &str) {
    g.entity_status[id] = 1;
    g.ent_name[id] = p(name);
    g.ent_symbol[id] = p(sym);
}

fn hold(g: &mut Globals, owner: i32, slot: i32, corp_id: i32, pct: i16) {
    g.hold_stock_id[(slot, owner)] = cast::<i16, _>(corp_id);
    g.hold_stock_pct[(slot, owner)] = pct;
}

#[test]
fn describe_defunct() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.num_players = 2;
    g.ent_name[30] = p("Old Co");
    g.ent_name[40] = p("Buyer Inc");
    g.entity_status[30] = 2;
    g.ent_misc_data[30] = p("40");
    describe_defunct_entity(&mut g, &mut ui, 30);
    g.ent_misc_data[30] = p("0");
    describe_defunct_entity(&mut g, &mut ui, 30);
    g.ent_misc_data[30] = p("BKRUP");
    describe_defunct_entity(&mut g, &mut ui, 30);
    g.entity_status[2] = 2;
    g.ent_name[2] = p("Jane");
    g.ent_misc_data[2] = p("BKRUP");
    describe_defunct_entity(&mut g, &mut ui, 2);
    g.entity_status[31] = 1;
    describe_defunct_entity(&mut g, &mut ui, 31);
    assert_eq!(
        ui.msgs,
        vec![
            "Old Co has been liquidated into Buyer Inc and no longer exists.",
            "Old Co has been liquidated, and no longer exists.",
            "Old Co has gone broke, and no longer exists.",
            __content.string(14432),
        ]
    );
}

#[test]
fn shareholder_list() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    let mut rng = Rng::default();
    g.num_players = 1;
    g.cur_player = 1;
    g.entity_status[1] = 1;
    g.ent_name[1] = p("Joe Raider");
    g.ent_symbol[1] = p("JOE");
    corp(&mut g, 20, "Target Corp", "TGT");
    corp(&mut g, 21, "Holding Inc", "HLD");
    g.country_idx[20] = 3;
    g.country_name[3] = p("Canada");
    g.industry_num[20] = 5;
    g.industry_num[21] = 5;
    hold(&mut g, 1, 1, 20, 30);
    hold(&mut g, 21, 2, 20, 12);
    hold(&mut g, 21, 3, 20, -5); // a short position (only with showShorts)
    g.corp_controller[20] = 1;
    g.corp_controller[21] = 21;
    g.entity_pick_mode = p("GetCorp");
    show_shareholder_list(&mut g, &mut ui, &mut rng, 20, 0);
    assert_eq!(ui.reports.len(), 1);
    let (title, lines) = &ui.reports[0];
    assert_eq!(title, __content.string(14433));
    let link = |id: &str| format!("{}{__wsr_text_13538}{id}", " ".repeat(45),__wsr_text_13538=__content.string(13538));
    let expect: Vec<String> = vec![
        "  Incorporated in:  Canada".into(),
        format!("  {}", "-".repeat(24)),
        "".into(),
        __content.string(14434).into(),
        "-".repeat(60),
        format!("{:<37}{}{}", "Joe Raider", "  30%", link("0001")),
        format!("{:<37}{}{}", "Holding Inc (HLD)", "  12%", link("0021")),
        "".into(),
        format!("  Public owns:{} 58%", " ".repeat(24)),
        "-".repeat(60),
        "".into(),
        format!("  CONTROLLED BY:  Joe Raider{}@C0001", " ".repeat(70)),
        "".into(),
    ];
    assert_eq!(lines, &expect);
    assert_eq!(g.corp_controller[20], 1);
    assert_eq!(g.public_pct, 58);
    // EntityPickMode was kept because links were added, then cleared by ShowTextReport
    assert!(g.entity_pick_mode.is_empty());
}

#[test]
fn shareholder_list_shorts_and_quirky_link() {
    let mut g = Globals::default();
    let mut ui = Rec::default();
    let mut rng = Rng::default();
    g.num_players = 2;
    g.cur_player = 1;
    for pl in 1..=2 {
        g.entity_status[pl] = 1;
    }
    g.ent_name[2] = p("Rival");
    corp(&mut g, 20, "Target Corp", "TGT");
    hold(&mut g, 2, 1, 20, 25);
    hold(&mut g, 1, 1, 20, -4);
    g.corp_controller[20] = 2;
    g.entity_pick_mode = p("GetCorp");
    show_shareholder_list(&mut g, &mut ui, &mut rng, 20, 1);
    let lines = &ui.reports[0].1;
    // the human player's short position comes first (owner 1)
    assert_eq!(lines[5], format!("{:<37}  -4% (Short position){}@C0001", "", " ".repeat(45)));
    // another player (not the current one): the percentage text ends up as the "link code"
    assert_eq!(lines[6], format!("{:<37}  25%{}@C    25%", "Rival", " ".repeat(45)));
    // the controller line has no link for another player
    assert!(lines.iter().any(|l| l == "  CONTROLLED BY:  Rival"));
}

#[test]
fn shareholder_list_non_corporations() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    let mut rng = Rng::default();
    g.entity_status[3] = 1;
    show_shareholder_list(&mut g, &mut ui, &mut rng, 3, 0);
    assert_eq!(ui.msgs, vec![__content.string(13971)]);
    g.entity_status[50] = 2;
    g.ent_name[50] = p("Gone");
    g.ent_misc_data[50] = p("BKRUP");
    show_shareholder_list(&mut g, &mut ui, &mut rng, 50, 0);
    assert_eq!(ui.msgs[1], "Gone has gone broke, and no longer exists.");
    assert!(ui.reports.is_empty());
}

#[test]
fn economic_stats() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(2.5));
    g.prime_rate = cast::<f32, _>(Ext::from_f64(7.25));
    g.long_bond_desc = p("2054  6.50");
    g.long_bond_price = cast::<f32, _>(Ext::from_f64(97.5));
    g.short_bond_desc = p("2027 12.25");
    g.short_bond_price = cast::<f32, _>(Ext::from_f64(101.0));
    g.corp_tax_rate = 21;
    g.cap_gains_tax_rate = 15;
    g.wealth_tax_rate = 2;
    g.oil_price = cast::<f32, _>(Ext::from_f64(0.75));
    g.difficulty_level = 2;
    g.million_prefix_uc = p("M");
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.cur_year = 2025;
    g.start_year = 2025;
    show_economic_stats(&mut g, &mut ui, &mut wsr_core::pb::rng::Rng::default());
    let (title, lines) = &ui.reports[0];
    assert_eq!(title, __content.string(8004));
    assert_eq!(lines.len(), 64);
    assert_eq!(lines[4], __content.string(14435));
    assert_eq!(lines[6], "       PRICE OF 6.50% T-BOND OF 2054:   97.50 % of Par  [2]");
    assert_eq!(lines[7], "       PRICE OF 12.25% T-BOND OF 2027: 101.00 % of Par");
    assert_eq!(lines[11], __content.string(14436));
    assert_eq!(lines[14], __content.string(14437));
    assert_eq!(lines[15], __content.string(14438));
    assert_eq!(lines[20], __content.string(14439));
    assert_eq!(lines[22], format!("       NATIONAL DEBT:{}125,000,000  Mil.  [9]", " ".repeat(13)));
    // nobody holds government bonds: the whole issue minus 2 * ROUND(issue / 2) is "held"
    assert_eq!(lines[61], __content.string(14440));
    assert_eq!(lines[32], __content.string(13972));
    assert_eq!(lines[63], "-".repeat(65), "line 64 is copied before the indentation");
    assert!(g.text_box_mode.is_empty(), "\"54\" consumed by the dialog");
}

#[test]
fn economic_stats_bonds_held_note() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.million_prefix_uc = p("M");
    g.currency_name = p(__content.string(13973));
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.cur_year = 2027;
    g.start_year = 2025;
    g.long_gov_bond_face[1] = 1_000_000;
    g.short_gov_bond_face[1581] = 500_000;
    g.long_gov_bond_face[500] = 9_999_999; // not a bond holder: ignored
    show_economic_stats(&mut g, &mut ui, &mut wsr_core::pb::rng::Rng::default());
    let lines = &ui.reports[0].1;
    // ROUND(ROUND(125e6 * 1.06!) * 1.06!) with the SINGLE constant 1.06 (= 1.05999994...)
    assert_eq!(lines[22], format!("       NATIONAL DEBT:{}140,449,985  Mil.  [9]", " ".repeat(13)));
    // issue - ((half - 1e6) + (half - 0.5e6)) with half = ROUND(140449985 * 0.5) = 70224992 (half-even)
    assert_eq!(lines[61], __content.string(14454));
    assert_eq!(lines[62], __content.string(13974));
}

#[test]
fn interest_rates_layout() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.prime_rate = cast::<f32, _>(Ext::from_f64(8.5));
    g.bank_cd_rate = cast::<f32, _>(Ext::from_f64(5.0));
    g.monetary_policy = 2;
    g.central_bank_name = p(__content.string(4373));
    g.long_bond_desc = p("2054  6.50");
    show_interest_rates(&mut g, &mut ui, &mut Rng::default());
    let (title, lines) = &ui.reports[0];
    assert_eq!(title, __content.string(7968));
    assert_eq!(lines.len(), 51);
    assert_eq!(lines[3], __content.string(14441).replacen("{}", &" ".repeat(20), 1));
    assert_eq!(lines[7], __content.string(14442).replacen("{}", &" ".repeat(20), 1));
    assert_eq!(lines[14], "              FEDERAL RESERVE MONETARY POLICY: TIGHTENING");
    assert_eq!(lines[17], __content.string(7980));
    assert_eq!(lines[30], __content.string(14443));
    assert_eq!(lines[49], "");
}

// ------------------------------------------------------------------ differential tests (original code)

mod diff;

use diff::{Compare, Opts, Rand, run_original_with};

// Harness limits for this module: ShowReportDialog runs `DIALOG SHOW MODAL`, whose message loop spins on
// PeekMessageA forever under emulation, so ShowTextReport and the reports built on it (ShowShareholderList
// for an active corporation, ShowInterestRates, ShowEconomicStats) cannot be run; NumberToWords returns a
// STRING (not returned by the harness). Their texts are covered by the unit tests above; the state parts
// they share with other routines (UpdateCorpControl, PostNews) have differential tests in control.rs /
// news.rs.

#[test]
fn describe_defunct_entity_matches_original() {
    let __content = wsr_core::content::current();
    let mut r = Rand(5);
    for k in 0..8 {
        let mut g = Globals::default();
        g.num_players = 3;
        let e = [2, 30, 31, 32, 3, 40, 41, 1][k];
        g.entity_status[e] = [2, 2, 2, 1, 2, 2, 0, 2][k];
        g.ent_name[e] = p(&format!("{__wsr_text_13758}{e}",__wsr_text_13758=__content.string(13758)));
        g.ent_misc_data[e] = p(["BKRUP", "45", "5", "BKRUP", "x", "1601", "45", "BKRUP"][k]);
        g.ent_name[45] = p("Buyer");
        g.tmp_news_str = p("stale");
        let _ = r.next();
        let opts = Opts { args: vec![e.to_string()], ..Opts::default() };
        let Some(o) = run_original_with(0x480a70, &opts, &g) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Rec::default();
        describe_defunct_entity(&mut port, &mut ui, e);
        let files = o.log.iter().filter(|l| l.contains("TMP_QM.DAT")).count();
        assert_eq!(files, ui.msgs.len(), "{__wsr_text_13924}{k}",__wsr_text_13924=__content.string(13924));
        Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
    }
}

#[test]
fn shareholder_list_without_dialog_matches_original() {
    // non-corporations and defunct companies end before the report dialog
    for (corp, status) in [(3, 1), (40, 2), (41, 0), (1, 0)] {
        let mut g = Globals::default();
        g.entity_status[corp] = status;
        g.ent_name[corp] = p(if corp == 3 { "" } else { "Some Name" });
        g.ent_misc_data[corp] = p("BKRUP");
        let opts = Opts { args: vec![corp.to_string(), "1".into()], ..Opts::default() };
        let Some(o) = run_original_with(0x421544, &opts, &g) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Rec::default();
        show_shareholder_list(&mut port, &mut ui, &mut Rng::default(), corp, 1);
        assert!(ui.reports.is_empty());
        let files = o.log.iter().filter(|l| l.contains("TMP_QM.DAT")).count();
        assert_eq!(files, ui.msgs.len(), "corp {corp}");
        Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
    }
}

#[test]
fn show_quick_message_matches_original() {
    for text in ["", "plain", "a\r\n\r\nb\r\nc"] {
        let text = text.to_string();
        let mut g = Globals::default();
        let opts = Opts { args: vec![format!("s:{text}")], ..Opts::default() };
        let Some(o) = run_original_with(0x46e851, &opts, &g) else {
            return;
        };
        let mut ui = Rec::default();
        let mut m = p(&text);
        show_quick_message(&mut g, &mut ui, &mut m);
        let shells = o.log.iter().filter(|l| l.contains("qm.exe")).count();
        assert_eq!(shells, ui.msgs.len(), "{text:?}");
        Compare::default().assert(&g, &o.globals);
    }
}
