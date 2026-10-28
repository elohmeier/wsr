//! Differential tests of `logic::assets::startup` (StartupCompany 0x5af020).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use wsr_core::logic::assets::startup::startup_company_skipping;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::string::right;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const STARTUP: u32 = 0x5af020;
/// Not ported (other owners): BorrowToCoverPurchase (3 args), ChangeCompanyName (no args).
const SKIPS: [(u32, u32); 0] = [];

/// Scripted UI sharing the original harness's answer list: MSGBOX answers (`IDYES`, …), INPUTBOX texts
/// (`cap:<f>` = `f` times the default, i.e. of the available funds), and `pick:<code>` for a report
/// (clicks the line whose link code is `<code>`). Every consumed answer is recorded in `used` the way the
/// original's harness must get it (`select:301:<list index>` for a pick, the entered text for `cap:`).
#[derive(Default)]
struct TUi {
    answers: VecDeque<String>,
    used: Vec<String>,
    msgs: Vec<String>,
    news: Vec<String>,
    reports: usize,
}

fn is_pick(a: &str) -> bool {
    a.starts_with("pick:")
}

impl Ui for TUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style == 0 && title.is_empty() {
            return 1;
        }
        let yn = style & 0xf == 3 || style & 0xf == 4;
        match self.answers.front().map(|s| s.as_str()) {
            Some(a) if a.starts_with("ID") => {
                let a = self.answers.pop_front().unwrap();
                self.used.push(a.clone());
                match a.as_str() {
                    "IDYES" => 6,
                    "IDNO" => 7,
                    "IDCANCEL" => 2,
                    _ => 1,
                }
            }
            _ => {
                // the original must get the same default (its next answer may be an INPUTBOX text)
                self.used.push(if yn { "IDYES" } else { "IDOK" }.into());
                if yn {
                    6
                } else {
                    1
                }
            }
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        match self.answers.front() {
            Some(a) if !is_pick(a) && !a.starts_with("ID") => {
                let a = self.answers.pop_front().unwrap();
                let text = match a.strip_prefix("cap:") {
                    Some(f) => {
                        let d: Ext = default.to_display().trim().parse().unwrap_or(Ext::from_f64(0.0));
                        format!("{:.0}", d * f.parse::<Ext>().unwrap())
                    }
                    None => a,
                };
                self.used.push(text.clone());
                PbStr::from(text.as_str())
            }
            _ => {
                // an empty INPUTBOX answer (Cancel), also for the original
                self.used.push(String::new());
                PbStr::new()
            }
        }
    }
    fn report(&mut self, _title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports += 1;
        if let Some(a) = self.answers.pop_front_if(|a| is_pick(a)) {
            let code = a[5..].to_string();
            if let Some(idx) = lines.iter().position(|l| l.to_display().ends_with(&code)) {
                self.used.push(format!("select:301:{idx}"));
                return ReportAction::Link(right(lines[idx].as_bytes(), 6));
            }
        }
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// A world where `CurPlayer` (1, human) or a company it controls starts a company.
fn startup_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed * 977 + 13);
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.cur_player_name = g.player_name[1].clone();
    g.entity_status[1] = 1;
    g.player_startup_count[1] = if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) { 2 } else { cast::<i32, _>(r.range(0, 1)) };
    let ae = match r.range(0, 9) {
        0 => cast::<i32, _>(r.range(11, 130)),
        1..=4 => {
            let c = cast::<i32, _>(r.range(131, 1580));
            g.entity_status[c] = 1;
            g.corp_controller[c] = 1;
            c
        }
        _ => 1,
    };
    g.active_entity = ae;
    g.active_entity_name = if ae == 1 { g.cur_player_name.clone() } else { g.ent_name[ae].clone() };
    g.cash[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)) } else { f64r(&mut r, Ext::from_f64(1500.0), Ext::from_f64(9000.0)) };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
        for s in 1..=15 {
            g.hold_stock_id[(s, ae)] = cast::<i16, _>(r.range(131, 1580));
            g.hold_stock_pct[(s, ae)] = 5;
        }
    } else {
        let s = cast::<i32, _>(r.range(1, 15));
        g.hold_stock_id[(s, ae)] = 0;
        g.hold_stock_pct[(s, ae)] = 0;
    }
    g.sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { 0 } else { ae };
    g.sel_industry = cast::<i32, _>(r.range(0, 69));
    g.auto_pilot[1] = cast::<i32, _>(r.range(0, 1));
    for i in 0..=55 {
        g.country_name[i] = PbStr::from(format!("{__wsr_text_13598}{:02} {}", i, word(&mut r, 6).to_display(),__wsr_text_13598=__content.string(13598)).as_str());
    }
    g
}

/// Runs the port, then the original with the answers the port consumed, and compares.
fn check(seed: u64, g: &Globals, answers: &[String]) -> Option<(Globals, TUi, Original)> {
    let rng0 = cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20);
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = TUi { answers: answers.iter().cloned().collect(), ..TUi::default() };
    startup_company_skipping(&mut port, &mut ui, &mut rng, 0, &[]);
    let refs: Vec<&str> = ui.used.iter().map(|s| s.as_str()).collect();
    let opts = Opts { rng: Some(rng0), answers: &refs, skips: SKIPS.to_vec(), ..Opts::default() };
    let o = run_original_with(STARTUP, &opts, g)?;
    let dlgs: usize = o.log.iter().filter(|l| l.starts_with("DIALOG NEW")).count();
    assert_eq!(ui.reports, dlgs, "reports seed {seed}");
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:#?}", ui.msgs, o.log);
    // every MSGBOX of the original shows a text of the port (quick messages go to qm.exe)
    for l in o.log.iter().filter(|l| l.starts_with("MSGBOX")) {
        assert!(ui.msgs.iter().any(|m| l.contains(&m.replace('\r', "\\r"))), "msg text seed {seed}: {l}\nport: {:?}", ui.msgs);
    }
    Some((port, ui, o))
}

/// Branches reached by the port (seed loop coverage).
static HITS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn hit(what: &str) {
    HITS.lock().unwrap().push(what.to_owned());
}

#[test]
fn diff_startup_company() {
    let __content = wsr_core::content::current();
    par_seeds(1..=320, |seed| {
        let mut g = startup_world(seed);
        let mut r = Rand(seed * 31 + 5);
        let ind = match r.range(0, 9) {
            0 => 1,
            1 => 2,
            2 | 3 => 70,
            4 => 0,
            _ => r.range(3, 69),
        };
        if ind == 1 && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            for b in 61..=85 {
                g.entity_status[b] = 1;
            }
        }
        if ind == 70 && r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            for b in 1471..=1580 {
                g.entity_status[b] = 1;
            }
        }
        let mut a: Vec<String> = vec![if r.chance(Ext::from_f64(0.92)) { "IDYES" } else { "IDNO" }.into()];
        if ind > 0 {
            a.push(format!("pick:@I{ind:04}"));
        }
        a.push(match r.range(0, 9) {
            0 => "cap:2".into(),
            1 => "0".into(),
            2 => "cap:0.01".into(),
            _ => format!("cap:{}", f64r(&mut r, Ext::from(Ext::from_f64(0.3)), Ext::from(Ext::from_f64(0.98)))),
        });
        if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
            // " [runtime resource wording] ": ChangeCompanyName (new name and/or symbol)
            a.push("IDYES".into());
            a.push(["IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 2))].into());
            a.push(format!("NEWCO {seed}{__wsr_text_13599}",__wsr_text_13599=__content.string(13599)));
            a.push(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "IDYES" } else { "IDNO" }.into());
            a.push(["NWC", "X", "NEWCOX", "A"][cast::<usize, _>(r.range(0, 3))].into());
        } else {
            a.push("IDNO".into());
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            a.push("IDYES".into());
            if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
                a.push(format!("pick:@I{:04}", r.range(1, 54)));
            }
        } else {
            a.push("IDNO".into());
        }
        let Some((port, ui, o)) = check(seed, &g, &a) else { return };
        let ae = g.active_entity;
        if port.player_startup_count[1] > g.player_startup_count[1] {
            hit(match ind {
                1 => "bank",
                2 => "insurer",
                70 => "shell",
                _ => "corp",
            });
            if ui.used.iter().filter(|u| u.starts_with("select")).count() == 2 {
                hit("country");
            }
            if ae > 10 {
                hit("corp parent");
            }
        }
        if ui.msgs.iter().any(|m| m.contains("Error number 9")) {
            hit("error9");
            let qm = o.files.get("TMP_QM.DAT").cloned().flatten().unwrap_or_default();
            let qm = String::from_utf8_lossy(&qm).to_string();
            let port_msg = ui.msgs.iter().find(|m| m.contains("Error number 9")).unwrap();
            assert_eq!(qm.trim_end(), port_msg, "runtime error text seed {seed}");
        }
        if port.ent_name.as_slice() != o.globals.ent_name.as_slice() || ui.used.iter().any(|u| u.starts_with("NEWCO")) {
            hit("renamed");
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(13600))) {
            hit(__content.string(13601));
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(13602))) {
            hit("count");
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(13603))) {
            hit("funds");
        }
        if ui.msgs.iter().any(|m| m.contains("maximum portfolio")) {
            hit("slots");
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(13604))) {
            hit("fin parent");
        }
    });
    let hits = HITS.lock().unwrap();
    for w in ["bank", "insurer", "shell", "corp", "country", "renamed", "corp parent", "error9", __content.string(13601), "count", "funds", "slots", "fin parent"] {
        assert!(hits.iter().any(|value| value == w), "branch {w} not reached: {hits:?}");
    }
}
