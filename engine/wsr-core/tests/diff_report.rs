//! Differential tests of the report ports (logic::report) against the original, run under emulation with
//! the harness's window manager: the lines the original adds to the report listbox (control 301 of the
//! dialog ShowReportDialog 0x41727f builds) vs the lines the port passes to `Ui::report`, plus the state.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::logic::report;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SHOW_TEXT_REPORT: u32 = 0x41dfaa;
const SHOW_SHAREHOLDER_LIST: u32 = 0x421544;
const SHOW_INTEREST_RATES: u32 = 0x45c781;
const SHOW_ECONOMIC_STATS: u32 = 0x47dd94;

/// Records every report, message box and news headline.
#[derive(Default)]
struct Rec {
    reports: Vec<(PbStr, Vec<PbStr>)>,
    msgs: Vec<String>,
    news: Vec<String>,
    actions: Vec<ReportAction>,
}

impl Ui for Rec {
    fn msgbox(&mut self, text: &PbStr, style: i32, _title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push((title.clone(), lines.to_vec()));
        self.actions.pop().unwrap_or(ReportAction::Ok)
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

fn word(r: &mut Rand, n: i64) -> PbStr {
    PbStr((0..r.range(1, n)).map(|_| b'A' + cast::<u8, _>(r.range(0, 25))).collect())
}

/// Report dialogs of the original as (title, listbox 301 items).
fn orig_reports(o: &Original) -> Vec<(PbStr, Vec<PbStr>)> {
    o.dialogs
        .iter()
        .filter_map(|d| d.control(301).map(|c| (d.title.clone(), c.items.clone())))
        .collect()
}

/// The port's reports as the original's listbox shows them: ShowReportDialog adds the `@END` line as " ".
fn port_reports(ui: &Rec, end_found: &[bool]) -> Vec<(PbStr, Vec<PbStr>)> {
    ui.reports
        .iter()
        .zip(end_found.iter().chain(std::iter::repeat(&true)))
        .map(|((t, l), &e)| {
            let mut l = l.clone();
            if e {
                l.push(st(" "));
            }
            (t.clone(), l)
        })
        .collect()
}

#[track_caller]
fn assert_reports(port: &[(PbStr, Vec<PbStr>)], orig: &[(PbStr, Vec<PbStr>)]) {
    let __content = wsr_core::content::current();
    assert_eq!(port.len(), orig.len(), "number of reports (port vs original)");
    for ((pt, pl), (ot, ol)) in port.iter().zip(orig) {
        assert_eq!(pt, ot, "report title");
        for (k, (a, b)) in pl.iter().zip(ol).enumerate() {
            assert_eq!(a, b, "{__wsr_text_13605}{:?}{__wsr_text_13606}{}", pt.to_display(), k + 1,__wsr_text_13605=__content.string(13605), __wsr_text_13606=__content.string(13606));
        }
        assert_eq!(pl.len(), ol.len(), "{__wsr_text_13605}{:?}: number of lines", pt.to_display(),__wsr_text_13605=__content.string(13605));
    }
}

/// Number of messages the original showed: MSGBOXes and ShowQuickMessage popups (which run `qm.exe` on
/// TMP_QM.DAT; the port shows those through `Ui::msgbox` too).
fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

/// The native window handle is the only report state the host does not model.
fn cmp() -> Compare {
    Compare::default().ignore("hReportDlg")
}

fn names(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    for i in 0..=55 {
        g.country_name[i] = word(r, 12);
    }
    for i in 1..=1600 {
        g.ent_name[i] = PbStr::from(format!("{} {}{__wsr_text_13676}", word(r, 8), i,__wsr_text_13676=__content.string(13676)).as_str());
        g.ent_symbol[i] = word(r, 5);
        g.country_idx[i] = cast::<i32, _>(r.range(0, 55));
    }
}

fn rates(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    let f = |r: &mut Rand, lo: i64, hi: i64| cast::<f32, _>(r.range(lo * 100, hi * 100)) / Ext::from_f64(100.0);
    g.prime_rate = cast::<f32, _>(f(r, 1, 20));
    g.long_bond_yield = cast::<f32, _>(f(r, 1, 15));
    g.short_bond_yield = cast::<f32, _>(f(r, 0, 12));
    g.long_bond_price = cast::<f32, _>(f(r, 50, 150));
    g.short_bond_price = cast::<f32, _>(f(r, 80, 110));
    g.t_bill_rate = cast::<f32, _>(f(r, 0, 10));
    g.bank_cd_rate = cast::<f32, _>(f(r, 0, 10));
    g.sofr_rate = cast::<f32, _>(f(r, 0, 10));
    g.base_interest_rate = cast::<f32, _>(f(r, 1, 12));
    g.gdp_growth = cast::<f32, _>(f(r, -5, 8));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.subprime_crisis_state = r.range(0, 2) as _;
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = 1600;
    g.central_bank_name = st([__content.string(4373), __content.string(6649), "ECB"][cast::<usize, _>(r.range(0, 2))]);
    g.long_bond_desc = PbStr::from(format!("{} 30-Yr Bonds", r.range(2030, 2070)).as_str());
    g.short_bond_desc = PbStr::from(format!("{} 2-Yr Notes", r.range(2025, 2040)).as_str());
    g.housing_starts = cast::<i32, _>(r.range(200_000, 3_000_000));
    g.oil_price = cast::<f32, _>(f(r, 10, 200));
    g.stock_index = cast::<Ext, _>(r.range(100, 50_000_000)) / Ext::from_f64(1000.0);
    g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(110.0), Ext::from_f64(0.85)][cast::<usize, _>(r.range(0, 2))]);
    g.start_year = cast::<i32, _>(r.range(1990, 2030));
    g.cur_year = g.start_year + cast::<i32, _>(r.range(0, 60));
    g.corp_tax_rate = cast::<i32, _>(r.range(0, 50));
    g.cap_gains_tax_rate = cast::<i32, _>(r.range(0, 40));
    g.wealth_tax_rate = cast::<i32, _>(r.range(0, 5));
    g.carbon_tax_rate = cast::<i32, _>(r.range(0, 30));
    g.health_care_tax_rate = cast::<i32, _>(r.range(0, 10));
    g.holding_co_tax_rate = cast::<i32, _>(r.range(0, 10));
    g.oil_windfall_tax_rate = cast::<i32, _>(r.range(0, 60));
    g.corp_shares_tax_country = cast::<i32, _>(r.range(0, 55));
    g.difficulty_level = cast::<i32, _>(r.range(1, 5));
    g.currency_name = st(["Dollars", "Yen", "Euros"][cast::<usize, _>(r.range(0, 2))]);
    g.million_prefix_uc = st("M");
    for i in 1..=1583 {
        if r.chance(Ext::from_f64(0.2)) {
            g.long_gov_bond_face[i] = r.range(0, 1_000_000_000);
            g.short_gov_bond_face[i] = r.range(0, 1_000_000_000);
        }
    }
}

fn run(addr: u32, args: &[&str], g: &Globals, rng: u32) -> Option<Original> {
    run_answers(addr, args, g, rng, &[])
}

fn run_answers(addr: u32, args: &[&str], g: &Globals, rng: u32, answers: &[&str]) -> Option<Original> {
    let opts = Opts { args: args.iter().map(|a| a.to_string()).collect(), rng: Some(rng), answers, ..Opts::default() };
    run_original_with(addr, &opts, g)
}

#[test]
fn show_text_report_matches_original() {
    let __content = wsr_core::content::current();
    for seed in 1u64..=6 {
        let mut r = Rand(seed * 7919);
        let mut g = Globals::default();
        let n = cast::<i32, _>(r.range(0, 60));
        for i in 1..=n {
            let mut l = PbStr::from(format!("{__wsr_text_13749}{i}: {} {}", word(&mut r, 10).to_display(), r.range(0, 999),__wsr_text_13749=__content.string(13749)).as_str());
            if r.chance(Ext::from_f64(0.2)) {
                l = l + " ".repeat(cast::<usize, _>(r.range(0, 20))).as_str() + "@C0" + format!("{:03}", r.range(11, 999)).as_str();
            }
            g.text_lines[i] = l;
        }
        let end = seed % 3 != 0;
        if end {
            g.text_lines[n + 1] = st("@END");
        }
        g.text_box_title = if seed % 2 == 0 { PbStr::new() } else { PbStr::from(format!("{__wsr_text_13750}{seed}",__wsr_text_13750=__content.string(13750)).as_str()) };
        g.text_has_links = cast::<i32, _>(seed % 2);
        g.text_box_mode = st(["", "54", "YESNO", "30"][cast::<usize, _>(seed % 4)]);
        g.entity_pick_mode = st(["", "GetCorp", "", __content.string(6093)][cast::<usize, _>(seed % 4)]);
        // YESNO reports: the default action clicks YES (id 1); seed 6 clicks NO (id 2)
        let answers: &[&str] = if seed == 6 { &["click:2"] } else { &[] };
        let Some(o) = run_answers(SHOW_TEXT_REPORT, &[], &g, 1, answers) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        if seed == 6 {
            ui.actions.push(ReportAction::No);
        }
        report::show_text_report(&mut port, &mut ui, &mut Rng::from_state(1));
        assert_reports(&port_reports(&ui, &[end]), &orig_reports(&o));
        let d = &o.dialogs[0];
        if g.text_box_mode == "YESNO" {
            // ReportDlg_OkBtn / ReportDlg_NoBtn set ReportDlgYesNo = "Y" / "N" and clear TextBoxMode.
            let yes = seed != 6;
            assert_eq!(d.closed_by, if yes { "default click:1" } else { "click:2" });
            assert_eq!(o.globals.report_dlg_yes_no, if yes { "Y" } else { "N" });
            assert_eq!(o.globals.text_box_mode, "");
            assert!(d.control(2).is_some_and(|c| c.text == "&NO"));
            cmp().assert(&port, &o.globals);
        } else {
            assert_eq!(d.closed_by, "default click:1");
            cmp().assert(&port, &o.globals);
        }
    }
}

#[test]
fn show_interest_rates_matches_original() {
    for seed in 1u64..=5 {
        let mut r = Rand(seed * 104729);
        let mut g = Globals::default();
        names(&mut g, &mut r);
        rates(&mut g, &mut r);
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(SHOW_INTEREST_RATES, &[], &g, rng0) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        report::show_interest_rates(&mut port, &mut ui, &mut rng);
        assert_reports(&port_reports(&ui, &[]), &orig_reports(&o));
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o));
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn show_economic_stats_matches_original() {
    for seed in 1u64..=5 {
        let mut r = Rand(seed * 1299709);
        let mut g = Globals::default();
        names(&mut g, &mut r);
        rates(&mut g, &mut r);
        let Some(o) = run(SHOW_ECONOMIC_STATS, &[], &g, 1) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        report::show_economic_stats(&mut port, &mut ui, &mut Rng::from_state(1));
        assert_reports(&port_reports(&ui, &[]), &orig_reports(&o));
        assert_eq!(ui.msgs.len(), orig_msgs(&o));
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn show_shareholder_list_matches_original() {
    let __content = wsr_core::content::current();
    let mut reports = 0;
    for seed in 1u64..=10 {
        let mut r = Rand(seed * 15485863);
        let mut g = Globals::default();
        names(&mut g, &mut r);
        g.num_players = cast::<i32, _>(r.range(1, 4));
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.game_end_flag = st(if r.chance(Ext::from_f64(0.2)) { "Y" } else { "N" });
        g.entity_pick_mode = st(["", "GetCorp", __content.string(6093)][cast::<usize, _>(r.range(0, 2))]);
        for i in 1..=1600 {
            g.entity_status[i] = if i <= g.num_players || r.chance(Ext::from_f64(0.8)) { 1 } else { cast::<i16, _>(r.range(0, 2)) };
            g.industry_num[i] = cast::<i16, _>(r.range(1, 70));
            g.corp_controller[i] = if r.chance(Ext::from_f64(0.5)) { i } else { cast::<i32, _>(r.range(0, 1600)) };
            g.shares_outstanding[i] = cast::<Ext, _>(r.range(1_000, 100_000_000));
        }
        // the corp reported on and a few others get holders
        let corp = cast::<i32, _>(r.range(11, 1600));
        if seed == 3 {
            g.entity_status[corp] = 2; // liquidated: DescribeDefunctEntity popup instead
        }
        let targets = [corp, cast::<i32, _>(r.range(11, 1600)), cast::<i32, _>(r.range(11, 1600))];
        for (k, &t) in targets.iter().enumerate() {
            let n = if k == 0 { r.range(0, 14) } else { r.range(0, 4) };
            for _ in 0..n {
                let owner = cast::<i32, _>(if r.chance(Ext::from_f64(0.3)) { r.range(1, cast::<i64, _>(g.num_players)) } else { r.range(11, 1600) });
                let slot = cast::<i32, _>(r.range(1, 15));
                g.hold_stock_id[(slot, owner)] = cast::<i16, _>(t);
                g.hold_stock_pct[(slot, owner)] = if r.chance(Ext::from_f64(0.15)) { -(cast::<i16, _>(r.range(1, 20))) } else { cast::<i16, _>(r.range(1, 60)) };
                g.hold_stock_basis[(slot, owner)] = cast::<Ext, _>(r.range(0, 1_000_000));
                g.hold_stock_id[(0, owner)] = 15;
            }
        }
        let show_shorts = cast::<i32, _>(seed % 2);
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(SHOW_SHAREHOLDER_LIST, &[&corp.to_string(), &show_shorts.to_string()], &g, rng0) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        report::show_shareholder_list(&mut port, &mut ui, &mut rng, corp, show_shorts);
        let orig = orig_reports(&o);
        reports += orig.len();
        eprintln!("seed {seed}: corp {corp}, {}{__wsr_text_13751}{} msgboxes, {:.2}{__wsr_text_13748}", orig.len(), orig_msgs(&o), o.elapsed,__wsr_text_13751=__content.string(13751), __wsr_text_13748=__content.string(13748));
        assert_reports(&port_reports(&ui, &[]), &orig);
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: port {:?}, original log {:?}", ui.msgs, o.log);
        if let Some(Some(qm)) = o.files.get("TMP_QM.DAT") {
            // ShowQuickMessage's popup text file: CRLF CRLF -> "@@", CRLF -> "##"
            let want = ui.msgs.last().unwrap().replace("\r\n\r\n", "@@").replace("\r\n", "##");
            let got = String::from_utf8_lossy(qm);
            assert_eq!(got.trim_end_matches("\r\n"), want, "TMP_QM.DAT");
        }
        cmp().assert(&port, &o.globals);
    }
    assert!(reports > 0);
}
