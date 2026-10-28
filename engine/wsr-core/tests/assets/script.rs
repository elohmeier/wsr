//! Shared helpers of the buy/sell tests: a scripted Ui driving the port and the original alike.
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Scripted UI sharing one answer queue the way the harness does: MSGBOX answers (`IDYES`, … or a number)
/// and INPUTBOX texts are popped in call order; `pick:k` tokens are dialog actions taken only by a report,
/// which then picks its k-th (mod count) link line (`@C` or `@I`). ShowQuickMessage pop-ups (style 0, no
/// title) take nothing. Records the listbox index of every pick so the original can be driven with
/// `select:301:N`.
#[derive(Default)]
pub struct Script {
    pub q: VecDeque<String>,
    pub msgs: Vec<String>,
    pub titles: Vec<String>,
    pub inputs: usize,
    /// INPUTBOX titles.
    pub input_titles: Vec<String>,
    pub reports: Vec<String>,
    /// What each consumed special token stood for, in order.
    pub specials: Vec<String>,
    /// (token, listbox index or None when the report had no link line)
    pub picks: Vec<(String, Option<usize>)>,
}

pub fn is_pick(a: &str) -> bool {
    a.starts_with("pick:")
}

/// Tokens resolved by the port's run: `dflt` (an INPUTBOX answers with its default text) and `frac:x`
/// (x times the default's value). Taken by a MSGBOX they count as `IDOK`.
pub fn is_special(a: &str) -> bool {
    a == "dflt" || a.starts_with("frac:")
}

impl Ui for Script {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        let yes_no = matches!(style & 0xf, 3 | 4);
        if style == 0 && title.is_empty() {
            return 1;
        }
        match self.q.front() {
            Some(a) if !is_pick(a) => {
                let a = self.q.pop_front().unwrap();
                if is_special(&a) {
                    self.specials.push("IDOK".to_string());
                    return 1;
                }
                match a.as_str() {
                    "IDOK" => 1,
                    "IDCANCEL" => 2,
                    "IDYES" => 6,
                    "IDNO" => 7,
                    n => n.parse().unwrap_or(0),
                }
            }
            _ => {
                if yes_no {
                    6
                } else {
                    1
                }
            }
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr {
        self.inputs += 1;
        self.input_titles.push(title.to_display());
        if let Some(a) = self.q.pop_front_if(|a| is_special(a)) {
            let d = default.to_display();
            let text = if a == "dflt" {
                d
            } else {
                let x: Ext = a[5..].parse().unwrap();
                let num: String = d
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '$' | ' '))
                    .filter(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                let v: Ext = num.trim_end_matches('.').parse().unwrap_or(Ext::from_f64(0.0));
                format!("{}", (v * x).floor())
            };
            self.specials.push(text.clone());
            return PbStr::from(text.as_str());
        }
        match self.q.front() {
            Some(a) if !is_pick(a) => PbStr::from(self.q.pop_front().unwrap().as_str()),
            _ => PbStr::new(),
        }
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push(title.to_display());
        match self.q.front() {
            Some(a) if is_pick(a) => {
                let tok = self.q.pop_front().unwrap();
                let k: usize = tok[5..].parse().unwrap();
                let links: Vec<usize> = (0..lines.len())
                    .filter(|&i| {
                        let l = lines[i].to_display();
                        l.contains("@C") || l.contains("@I")
                    })
                    .collect();
                if links.is_empty() {
                    self.picks.push((tok, None));
                    return ReportAction::Ok;
                }
                let idx = links[k % links.len()];
                self.picks.push((tok, Some(idx)));
                let l = lines[idx].to_display();
                ReportAction::Link(PbStr::from(&l[l.len() - 6..]))
            }
            _ => ReportAction::Ok,
        }
    }
    fn news(&mut self, _headline: &PbStr) {}
}

/// The original's answer list for `answers` after the port's run: `pick:k` → `select:301:<index>`.
pub fn orig_answers(answers: &[String], ui: &Script) -> Vec<String> {
    let mut picks = ui.picks.iter();
    let mut specials = ui.specials.iter();
    let mut out = Vec::new();
    for a in answers {
        if is_special(a) {
            out.push(specials.next().cloned().unwrap_or_else(|| "0".to_string()));
        } else if is_pick(a) {
            // a pick the port's report took: select the line, then OK (so that a following dialog action
            // can't be taken by the same dialog); a pick no report took stays a blocking dialog action
            match picks.next() {
                Some((_, Some(i))) => {
                    out.push(format!("select:301:{i}"));
                    out.push("click:1".to_string());
                }
                Some((_, None)) => out.push("click:1".to_string()),
                None => out.push("select:301:0".to_string()),
            }
        } else {
            out.push(a.clone());
        }
    }
    out
}

pub fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// MSGBOX/INPUTBOX counts in the original's log.
pub fn orig_inputs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count()
}

/// Run the port, then the original with the equivalent answers; compare state, RND, UI counts. Returns
/// the port's Script and the port's state.
pub fn check(
    seed: u64,
    addr: u32,
    args: &[String],
    g: &Globals,
    answers: &[String],
    skips: &[(u32, u32)],
    port: impl Fn(&mut Globals, &mut Script, &mut Rng),
) -> Option<(Script, Globals)> {
    let rng0 = rng_of(seed);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Script { q: answers.iter().cloned().collect(), ..Script::default() };
    port(&mut pg, &mut ui, &mut rng);
    let oa = orig_answers(answers, &ui);
    let oa: Vec<&str> = oa.iter().map(|s| s.as_str()).collect();
    let opts = Opts { args: args.to_vec(), rng: Some(rng0), answers: &oa, skips: skips.to_vec(), ..Opts::default() };
    let o = run_original_with(addr, &opts, g)?;
    if std::env::var("WSR_DEBUG").is_ok() {
        eprintln!("seed {seed} port {:?}\n orig log {:?}", ui.titles, o.log);
    }
    let d = cmp().rel_tol(Ext::from_f64(1e-9)).diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.titles, o.log);
    assert_eq!(ui.inputs, orig_inputs(&o), "inputs seed {seed}");
    // an error in the routine's own frame goes to its ON ERROR handler (modelled as "Error number 9 …");
    // errors inside callees without a handler only set ERR
    let own = format!("PBERR 9 in 0x{addr:x} ");
    assert!(
        !o.log.iter().any(|l| l.starts_with(&own)) || ui.msgs.iter().any(|m| m.starts_with("Error number 9")),
        "unexpected PB error seed {seed}: {:?}",
        o.log.iter().filter(|l| l.starts_with("PBERR")).collect::<Vec<_>>()
    );
    Some((ui, pg))
}

/// `range`, or only the seed in `$WSR_SEED` (debugging).
pub fn seeds(range: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    match std::env::var("WSR_SEED").ok().and_then(|s| s.parse().ok()) {
        Some(s) => vec![s],
        None => range.collect(),
    }
}
