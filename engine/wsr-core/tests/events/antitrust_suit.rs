//! Differential tests of `logic::events::antitrust_suit`: AntitrustCaseRuling (0x541713),
//! AntitrustSuitTrial (0x540077) and AntitrustSuitTick (0x540401) against the original.
//!
//! `TIME$` is fixed by the harness (10:00:00), so the port gets `time_secs = 0`.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Mutex;
use wsr_core::logic::events::antitrust_suit::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::string::instr;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const CASE_RULING: u32 = 0x541713;
const SUIT_TRIAL: u32 = 0x540077;
const SUIT_TICK: u32 = 0x540401;

/// Records message boxes and news; answers message boxes *and* INPUTBOX$ from one queue (like the
/// harness: every MessageBoxA and every INPUTBOX$ pops the next answer).
#[derive(Default)]
struct AUi {
    msgs: Vec<String>,
    news: Vec<String>,
    answers: VecDeque<String>,
}

impl Ui for AUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if !(style == 0 && title.is_empty())
            && let Some(a) = self.answers.pop_front()
        {
            return match a.as_str() {
                "IDYES" => 6,
                "IDNO" => 7,
                "IDOK" => 1,
                n => n.parse().unwrap_or(1),
            };
        }
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        PbStr::from(self.answers.pop_front().unwrap_or_default().as_str())
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

#[track_caller]
fn check_a(what: &str, port: &Globals, port_rng: &Rng, ui: &AUi, o: &Original, allow_err9: bool) {
    let d = cmp().diffs(port, &o.globals);
    let log: Vec<&String> = o.log.iter().filter(|l| !l.starts_with("API")).take(30).collect();
    assert!(
        d.is_empty(),
        "{what}: {} differences:\n{}\nlog: {log:?}",
        d.len(),
        d.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(port_rng.state(), o.rng, "{what}: RND state");
    let errs: Vec<&String> = o
        .log
        .iter()
        .filter(|l| l.starts_with("PBERR") && !(allow_err9 && l.starts_with("PBERR 9 ")))
        .collect();
    assert!(errs.is_empty(), "{what}: runtime errors in the original: {errs:?}");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs log {log:?}", ui.msgs);
}

fn active_op_corp(g: &Globals, r: &mut Rand, not: i32) -> i32 {
    for _ in 0..200 {
        let c = cast::<i32, _>(r.range(131, 1580));
        if c != not && g.entity_status[c] == 1 && g.industry_num[c] < 70 && g.industry_num[c] > 2 {
            return c;
        }
    }
    panic!("no active corp")
}

/// Scenario of a pending suit. `kind`: 0 = the current (human) player controls the defendant,
/// 1 = the current player controls the plaintiff, 2 = another human player controls the defendant,
/// 3 = both have the same controller, 4 = the plaintiff filed recently, else corporate/computer
/// controllers.
fn suit_world(seed: u64, kind: u64) -> (Globals, Rand) {
    let mut r = Rand(seed * 7919 + 17);
    let mut g = world(seed + 3100);
    let d = active_op_corp(&g, &mut r, 0);
    let p = active_op_corp(&g, &mut r, d);
    g.antitrust_defendant = d;
    g.antitrust_plaintiff = p;
    g.antitrust_market_share = [12, 20, 25, 31, 36, 42, 47, 49, 51, 53, 55, 57, 59, 61, 64, 66, 70, 74, 78, 85]
        [cast::<usize, _>(r.range(0, 19))];
    g.antitrust_damages = if r.chance(Ext::from_f64(0.4)) { r.range(50, 400_000) } else { 0 };
    g.antitrust_settle_amt = if r.chance(Ext::from_f64(0.4)) { r.range(1, 3000) } else { 0 };
    g.antitrust_memo_shown = cast::<i32, _>(r.range(0, 2));
    g.private_antitrust_off = if r.chance(Ext::from_f64(0.03)) { 1 } else { 0 };
    g.exchange_rate = cast::<f64, _>(if r.chance(Ext::from_f64(0.6)) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.001), Ext::from_f64(0.02)) });
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    if g.num_players < 2 && (kind == 2 || r.chance(Ext::from_f64(0.3))) {
        g.num_players = 2 + cast::<i32, _>(r.range(0, 1));
    }
    if kind == 2 {
        g.is_computer_player[2] = 0;
    }
    for pl in 1..=5 {
        g.player_law_firm[pl] = cast::<i32, _>(r.range(0, 3));
        g.player_antitrust_settles_paid[pl] = [0, 1, 3, 4, 5, 8, 12][cast::<usize, _>(r.range(0, 6))];
        g.player_antitrust_settles_refused[pl] = [0, 1, 2, 4, 6, 9, 30][cast::<usize, _>(r.range(0, 6))];
    }
    let other = |r: &mut Rand| cast::<i32, _>(r.range(11, 1600));
    g.corp_controller[d] = other(&mut r);
    g.corp_controller[p] = other(&mut r);
    match kind {
        0 => g.corp_controller[d] = 1,
        1 => g.corp_controller[p] = 1,
        2 => g.corp_controller[d] = 2,
        3 => g.corp_controller[p] = g.corp_controller[d],
        _ => {
            if r.chance(Ext::from_f64(0.3)) && g.num_players > 1 {
                g.corp_controller[p] = cast::<i32, _>(r.range(2, cast::<i64, _>(g.num_players)));
            }
        }
    }
    for s in 1..=10 {
        g.recent_antitrust_filers[s] = if r.chance(Ext::from_f64(0.5)) { other(&mut r) } else { 0 };
    }
    if kind == 4 {
        g.recent_antitrust_filers[cast::<i32, _>(r.range(1, 10))] = p;
    }
    // plaintiff funds: rich, poor, or poor with a lending bank
    match r.range(0, 4) {
        3 => {
            // broke: no credit line, negative net worth
            g.cash[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0));
            g.t_bills[p] = Ext::from_f64(0.0);
            g.bank_debt[p] = Ext::from_f64(1.0e6);
        }
        0 => {
            g.cash[p] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(20.0));
            g.t_bills[p] = Ext::from_f64(0.0);
        }
        1 => g.cash[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200_000.0)),
        _ => {}
    }
    g.credit_rating_num[p] = cast::<i32, _>(r.range(0, 9));
    if r.chance(Ext::from_f64(0.3)) {
        let l = g.lending_bank[p];
        g.corp_controller[cast::<i32, _>(l)] = g.corp_controller[d];
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.contingency_reserve[d] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
    }
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    if r.chance(Ext::from_f64(0.3)) {
        g.news_msg = st("Earlier news item");
    }
    (g, r)
}

fn answers_for(kind: u64, r: &mut Rand, g: &mut Globals) -> Vec<String> {
    let memo = g.antitrust_memo_shown;
    match kind {
        0 => {
            let mut a = vec!["IDOK".to_string()];
            if memo == 0 {
                a.push("IDOK".into());
            }
            a.push(if r.chance(Ext::from_f64(0.5)) { "IDYES".into() } else { "IDNO".into() });
            a
        }
        1 => {
            let amt = ["0", "$5", "40", "250,000", "1e3"][cast::<usize, _>(r.range(0, 4))];
            if !amt.bytes().all(|b| b.is_ascii_digit()) {
                // a text answer must reach the INPUTBOX (the harness parses MSGBOX answers as numbers)
                g.antitrust_settle_amt = 0;
            }
            let mut a = vec!["IDOK".to_string(), amt.to_string()];
            if amt == "0" {
                a.push("120".into());
            }
            a
        }
        _ => vec![],
    }
}

fn outcome(ui: &AUi, res: &PbStr) -> String {
    let __content = wsr_core::content::current();
    if *res == "Delay" {
        return "delay".into();
    }
    for (k, tag) in [
        (__content.string(13817), "settled"),
        (" wins ", "plaintiff wins"),
        (__content.string(13818), __content.string(13818)),
        (__content.string(13819), "defendant wins"),
        (__content.string(13820), "drops"),
        (__content.string(13821), __content.string(13822)),
    ] {
        if ui.news.iter().any(|n| n.contains(k)) {
            return tag.into();
        }
    }
    "other".into()
}

#[test]
fn diff_antitrust_case_ruling() {
    let __content = wsr_core::content::current();
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(1..=96, |seed| {
        let kind = seed % 7;
        let (mut g, mut r) = suit_world(seed, kind);
        g.private_antitrust_off = 0;
        let mode = match kind {
            1 => [0, 0, 2][cast::<usize, _>(r.range(0, 2))],
            2 => cast::<i32, _>(r.range(1, 2)),
            _ => cast::<i32, _>(r.range(0, 2)),
        };
        let answers = answers_for(kind, &mut r, &mut g);
        let ans: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(CASE_RULING, &[&mode.to_string()], &g, rs, &ans, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = AUi { answers: answers.into(), ..AUi::default() };
        let res = antitrust_case_ruling(&mut port, &mut ui, &mut rng, mode);
        let what = format!("AntitrustCaseRuling seed {seed}{__wsr_text_13623}{kind}{__wsr_text_13548}{mode}",__wsr_text_13623=__content.string(13623), __wsr_text_13548=__content.string(13548));
        check_a(&what, &port, &rng, &ui, &o, false);
        assert_eq!(Some(res.clone()), o.return_str.clone().or(Some(PbStr::new())), "{what}: result");
        seen.lock().unwrap().insert(format!("{kind}:{}", outcome(&ui, &res)));
        if instr(1, port.news_msg.as_bytes(), b"drops") > 0 {
            seen.lock().unwrap().insert("drops".to_string());
        }
        let pl = g.antitrust_plaintiff;
        if port.bank_debt[pl] > g.bank_debt[pl] + Ext::from_f64(0.001) && g.antitrust_market_share > 71 && mode > 0 {
            seen.lock().unwrap().insert("loan".to_string());
        }
        seen.lock().unwrap().insert(outcome(&ui, &res));
    });
    let seen = seen.into_inner().unwrap();
    eprintln!("{__wsr_text_13663}{seen:?}",__wsr_text_13663=__content.string(13663));
    if unavailable().is_none() {
        for k in ["settled", "plaintiff wins", __content.string(13818), "defendant wins", "delay", "drops", "loan", "0:settled", "1:settled"] {
            assert!(seen.contains(k), "branch {k} not hit: {seen:?}");
        }
    }
}

#[test]
fn diff_antitrust_suit_trial() {
    let __content = wsr_core::content::current();
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(1..=40, |seed| {
        let kind = seed % 6;
        let (mut g, mut r) = suit_world(seed + 500, kind);
        if seed % 10 == 9 {
            g.entity_status[g.antitrust_plaintiff] = 2;
        }
        let mode = cast::<i32, _>(r.range(0, 2));
        g.tmp_ext1 = cast::<Ext, _>(mode);
        let answers = answers_for(kind, &mut r, &mut g);
        let ans: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(SUIT_TRIAL, &[], &g, rs, &ans, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = AUi { answers: answers.into(), ..AUi::default() };
        antitrust_suit_trial(&mut port, &mut ui, &mut rng, 0);
        let what = format!("AntitrustSuitTrial seed {seed}{__wsr_text_13623}{kind}{__wsr_text_13548}{mode}",__wsr_text_13623=__content.string(13623), __wsr_text_13548=__content.string(13548));
        check_a(&what, &port, &rng, &ui, &o, false);
        let tag = if g.private_antitrust_off == 1 {
            "off".to_string()
        } else if port.antitrust_defendant != 0 && g.antitrust_defendant == port.antitrust_defendant {
            "pending".to_string()
        } else {
            outcome(&ui, &PbStr::new())
        };
        seen.lock().unwrap().insert(tag);
    });
    let seen = seen.into_inner().unwrap();
    eprintln!("{__wsr_text_13663}{seen:?}",__wsr_text_13663=__content.string(13663));
    if unavailable().is_none() {
        for k in ["settled", "pending"] {
            assert!(seen.contains(k), "branch {k} not hit: {seen:?}");
        }
    }
}

#[test]
fn diff_antitrust_suit_tick() {
    let __content = wsr_core::content::current();
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(1..=40, |seed| {
        let kind = [0, 2, 5, 6, 6, 1][cast::<usize, _>(seed % 6)];
        let (mut g, mut r) = suit_world(seed + 900, kind);
        if seed % 9 == 4 {
            g.antitrust_market_share = 0;
        }
        if seed % 11 == 5 {
            g.entity_status[g.antitrust_defendant] = 0;
        }
        if seed % 4 == 1 {
            g.antitrust_market_share = [12, 25, 40, 44, 47][cast::<usize, _>(r.range(0, 4))];
        }
        if r.chance(Ext::from_f64(0.4)) {
            // a computer player controls the plaintiff
            let cp = (2..=5).find(|&p| g.is_computer_player[p] == 1);
            if let Some(cp) = cp {
                g.corp_controller[g.antitrust_plaintiff] = cp;
            }
        }
        if r.chance(Ext::from_f64(0.3)) {
            for s in 1..=10 {
                g.recent_antitrust_filers[s] = cast::<i32, _>(r.range(11, 1600));
            }
        }
        let answers = answers_for(kind, &mut r, &mut g);
        let ans: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(SUIT_TICK, &[], &g, rs, &ans, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = AUi { answers: answers.into(), ..AUi::default() };
        antitrust_suit_tick(&mut port, &mut ui, &mut rng, 0);
        let what = format!("AntitrustSuitTick seed {seed}{__wsr_text_13623}{kind}",__wsr_text_13623=__content.string(13623));
        check_a(&what, &port, &rng, &ui, &o, true);
        let tag = if ui.news.iter().any(|n| n.contains(__content.string(13821))) {
            __content.string(13822).to_string()
        } else if port.antitrust_defendant == 0 && port.antitrust_plaintiff == 0 && ui.news.is_empty() {
            "cleared".to_string()
        } else if port.antitrust_defendant == g.antitrust_defendant && ui.news.is_empty() {
            "waiting".to_string()
        } else {
            "trial".to_string()
        };
        seen.lock().unwrap().insert(tag);
    });
    let seen = seen.into_inner().unwrap();
    eprintln!("{__wsr_text_13663}{seen:?}",__wsr_text_13663=__content.string(13663));
    if unavailable().is_none() {
        for k in [__content.string(13822), "cleared", "trial"] {
            assert!(seen.contains(k), "branch {k} not hit: {seen:?}");
        }
    }
}
