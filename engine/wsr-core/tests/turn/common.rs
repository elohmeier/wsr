//! Shared helpers of the turn tests: a UI that records message boxes / news and answers reports.
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


use super::diff::{Compare, Opts, Original, run_original_with};
use std::collections::VecDeque;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Records message boxes, reports and news; reports are answered from `report_answers` (default OK/YES).
#[derive(Default)]
pub struct TUi {
    pub msgs: Vec<String>,
    pub titles: Vec<String>,
    pub reports: Vec<(PbStr, Vec<PbStr>, bool)>,
    pub news: Vec<String>,
    pub report_answers: VecDeque<ReportAction>,
}

impl Ui for TUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        // like the harness without answers: an INPUTBOX$ returns ""; the symbol dialog (InBox) takes its
        // default action, OK with the prefilled text
        if prompt.to_display().ends_with(__content.string(6717)) { default.clone() } else { PbStr::new() }
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction {
        self.reports.push((title.clone(), lines.to_vec(), yes_no));
        // (the harness's "click:2" on a report without a NO button does nothing; OK closes it)
        match self.report_answers.pop_front() {
            Some(ReportAction::No) if !yes_no => ReportAction::Ok,
            Some(a) => a,
            None => ReportAction::Ok,
        }
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// MSGBOX calls and ShowQuickMessage pop-ups (qm.exe) in the original's log.
pub fn orig_msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

/// The harness's fixed clock.
pub const NOW: LocalTime = LocalTime::HARNESS;

pub fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// UI-only state the ports don't model.
pub fn cmp() -> Compare {
    Compare::default().ignore("TickerPauseUntil").ignore("hReportDlg")
}

/// Run the original at `addr` with harness arg specs `args`, RND state `rng`, answers and skips.
pub fn run(addr: u32, args: &[&str], g: &Globals, rng: u32, answers: &[&str], skips: &[(u32, u32)]) -> Option<Original> {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng),
        answers,
        skips: skips.to_vec(),
        hooks: std::env::var("TURN_HOOKS")
            .map(|h| h.split(',').filter_map(|x| u32::from_str_radix(x.trim(), 16).ok()).collect())
            .unwrap_or_default(),
        ..Opts::default()
    };
    run_original_with(addr, &opts, g)
}

/// Loads `SCENWIN1..4.WSR` and `MSCNEWS.DAT` from extracted/app into the port (once).
pub fn load_scenwin() {
    let __content = wsr_core::content::current();
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/app");
        for n in 1..=4 {
            let data = std::fs::read(dir.join(format!("Scenwin{n}{__wsr_text_14154}",__wsr_text_14154=__content.string(14154)))).ok();
            wsr_core::logic::turn::set_scenwin_file(n, data);
        }
        wsr_core::logic::economy::set_mscnews_dat(std::fs::read(dir.join("Mscnews.dat")).ok());
    });
}

/// `SEEDS=a,b,..` overrides the default seeds.
pub fn seeds(default: impl IntoIterator<Item = u64>) -> Vec<u64> {
    match std::env::var("SEEDS") {
        Ok(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
        Err(_) => default.into_iter().collect(),
    }
}

/// Runtime errors of the original whose ON ERROR handler showed `ShowRuntimeError` (a `qm.exe` pop-up
/// after the `PBERR` line, before anything but API calls); the ports leave those routines without the pop-up.
pub fn runtime_error_popups(o: &Original) -> usize {
    let mut n = 0;
    for (i, l) in o.log.iter().enumerate() {
        // (GetControllerType 0x4a09b9 has no error pop-up: its errors are resumed silently)
        if l.starts_with("PBERR") && !l.contains("in 0x4a09b9") {
            let next = o.log[i + 1..].iter().find(|l| !l.starts_with("API") || l.contains("qm.exe"));
            if next.is_some_and(|l| l.contains("CreateProcessA(0x0, 'qm.exe")) {
                n += 1;
            }
        }
    }
    n
}

/// Option records hold STR$ of EXT values (strike, premium) computed in 80-bit precision; the port works
/// in f64, so the 15th digit of such a field can differ. Takes the original's record where only numeric
/// fields differ, by at most 1e-12 relative.
pub fn align_option_recs(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (port.option_recs[i].to_display(), orig.option_recs[i].to_display());
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        if pa.len() != pb.len() || pa.len() < 2 {
            continue;
        }
        let close = pa.iter().zip(&pb).all(|(x, y)| {
            x == y
                || match (x.trim().parse::<Ext>(), y.trim().parse::<Ext>()) {
                    (Ok(x), Ok(y)) => (x - y).abs() <= Ext::from_f64(1e-12) * y.abs().max(Ext::from_f64(1e-300)),
                    _ => false,
                }
        });
        if close {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

/// Compares port and original: globals (relative tolerance 1e-9; option records aligned within 1e-12),
/// RND state, message box count (plus the original's ShowRuntimeError pop-ups).
#[track_caller]
pub fn check(what: &str, port: &Globals, ui: &TUi, rng_port: u32, o: &Original) {
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    let c = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9));
    let mut port = port.clone();
    align_option_recs(&mut port, &o.globals);
    let port = &port;
    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.assert(port, &o.globals))) {
        if std::env::var_os("TURN_DBG").is_some() {
            for l in &o.log {
                eprintln!("  {l}");
            }
        }
        panic!("{what}: {} (PBERR {errs:?})", e.downcast_ref::<String>().cloned().unwrap_or_default());
    }
    assert_eq!(rng_port, o.rng, "{what}: RND state");
    let rt_errs = runtime_error_popups(o);
    if ui.msgs.len() + rt_errs != orig_msgs(o) && std::env::var_os("TURN_DBG").is_some() {
        for l in o.log.iter().filter(|l| !l.starts_with("API") || l.contains("qm.exe")) {
            eprintln!("  {l}");
        }
    }
    assert_eq!(ui.msgs.len() + rt_errs, orig_msgs(o), "{what}: message boxes {:?} vs {:?}", ui.msgs,
        o.log.iter().filter(|l| l.starts_with("MSGBOX")).collect::<Vec<_>>());
}

/// PORT-TODO routines of other modules that are not ported (skipped in the original; the port skips
/// them unconditionally): (address, stdcall dword args).
pub const GLOBAL_TODO: &[(u32, u32)] = &[
    (0x40c01b, 1), // SaveGamePrompt
];

/// The callees SimScheduledEvents leaves as PORT-TODO.
pub const SCHEDULED_TODO: &[(u32, u32)] = &[];

/// EndTurn's PORT-TODO callees.
pub const END_TURN_TODO: &[(u32, u32)] = &[];

/// `GLOBAL_TODO` + `SCHEDULED_TODO` + `END_TURN_TODO` + `extra`.
pub fn all_skips(extra: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut v: Vec<(u32, u32)> = GLOBAL_TODO.iter().chain(SCHEDULED_TODO).chain(END_TURN_TODO).copied().collect();
    for e in extra {
        if !v.iter().any(|x| x.0 == e.0) {
            v.push(*e);
        }
    }
    v
}

/// `super::world::world` plus the state the turn routines read: lender, bond infos in the game's format,
/// player settings, watch lists, ticker, scenario variables.
pub fn turn_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    use super::diff::Rand;
    use super::world::{f64r, world};
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    for e in 11..=1600 {
        if g.bonds_issued[e] == 0 {
            continue;
        }
        let mat = g.cur_year + cast::<i32, _>(r.range(0, 14));
        let coupon = cast::<Ext, _>(r.range(40, 300)) / Ext::from_f64(20.0);
        g.bond_issue_info[e] = st(&format!("{mat}@{coupon:05.2}"));
    }
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    for p in 1..=5 {
        g.player_turn_order[p] = p;
        g.player_suppress_popups[p] = cast::<i32, _>(r.range(0, 1));
        g.player_suppress_earnings[p] = cast::<i32, _>(r.range(0, 1));
        g.player_suppress_cash_warn[p] = cast::<i32, _>(r.range(0, 1));
        g.player_auto_save[p] = cast::<i32, _>(r.range(0, 1));
        g.player_exercise_options[p] = cast::<i32, _>(r.range(0, 1));
        g.player_make_delivery[p] = cast::<i32, _>(r.range(0, 1));
        g.player_take_delivery[p] = cast::<i32, _>(r.range(0, 1));
        g.player_auto_add_stream[p] = cast::<i32, _>(r.range(0, 1));
        g.sweep_cash_to_loan[p] = cast::<i32, _>(r.range(0, 1));
        g.player_last_stock[p] = cast::<i32, _>(r.range(0, 1600));
        g.player_select_last_corp[p] = cast::<i32, _>(r.range(0, 1600));
        g.player_last_active_entity[p] = cast::<i32, _>(r.range(0, 1600));
        let n = r.range(0, 15);
        for k in 1..=cast::<i32, _>(n) {
            g.watch_list[(k, p)] = cast::<i32, _>(r.range(11, 1600));
        }
        g.watch_list[(0, p)] = cast::<i32, _>(n);
    }
    g.ticker_speed = cast::<i32, _>(r.range(0, 100));
    g.ticker_text = st(&"Some ticker text ... ".repeat(cast::<usize, _>(r.range(0, 5))));
    g.app_title = st(__content.string(13661));
    g.has_cpu_players = st(if g.is_computer_player.as_slice().contains(&1) { "Y" } else { "N" });
    g.ent_name[1] = g.player_name[1].clone();
    for p in 1..=5 {
        g.ent_name[p] = g.player_name[p].clone();
    }
    g.cur_player_name = g.player_name[g.cur_player].clone();
    g.fund_fee_pct.as_mut_slice().iter_mut().for_each(|x| *x = cast::<f32, _>(f64r(&mut r, Ext::from_f64(0.1), Ext::from_f64(1.2))));
    g
}
