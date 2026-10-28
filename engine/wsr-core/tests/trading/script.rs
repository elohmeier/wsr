//! Shared helpers of the trading tests: a scripted Ui driving the port and the original alike, and a
//! runner comparing both.
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Scripted UI sharing one answer queue the way the harness does: MSGBOX answers (`IDYES`, `IDNO`,
/// `IDCANCEL`, `IDOK` or a number) and INPUTBOX texts are popped in call order; `pick:k` tokens are
/// dialog actions taken only by a report, which then picks its k-th (mod count) `@C`/`@I` link line.
/// `sym:TEXT` tokens are the stock-symbol entry dialog (InBox mode 3, a DDT dialog in the original:
/// entry box 310 (308 is the prompt), OK 1, Cancel 2): the port's `inputbox` returns TEXT (`sym:` alone = Cancel); message
/// boxes never take them.
/// ShowQuickMessage pop-ups (style 0, no title) take nothing. Records message boxes, news and the listbox
/// index of every pick so the original can be driven with `select:301:N`.
#[derive(Default)]
pub struct Script {
    pub q: VecDeque<String>,
    pub msgs: Vec<String>,
    pub titles: Vec<String>,
    pub news: Vec<String>,
    pub inputs: usize,
    pub reports: Vec<String>,
    /// (token, listbox index or None when the report had no link line)
    pub picks: Vec<(String, Option<usize>)>,
    /// Actual host replies, in consumption order, translated to original dialog actions.
    pub replies: Vec<String>,
}

pub fn is_pick(a: &str) -> bool {
    a.starts_with("pick:")
}

pub fn is_sym(a: &str) -> bool {
    a.starts_with("sym:")
}

impl Ui for Script {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        let yes_no = matches!(style & 0xf, 3 | 4);
        if style == 0 && title.is_empty() {
            return 1;
        }
        let result = match self.q.front() {
            Some(a) if !is_pick(a) && !is_sym(a) => {
                let a = self.q.pop_front().unwrap();
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
        };
        self.replies.push(result.to_string());
        result
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        match self.q.front() {
            // the symbol dialog is no INPUTBOX$ in the original: not counted in `inputs`
            Some(a) if is_sym(a) => {
                let a = self.q.pop_front().unwrap();
                if a.len() == 4 { self.replies.push("click:2".into()); }
                else { self.replies.extend([format!("text:310:{}", &a[4..]), "click:1".into()]); }
                return PbStr::from(&a[4..]);
            }
            // the symbol dialog without a `sym:` answer: its default action (OK, prefilled text)
            _ if prompt.to_display().ends_with(__content.string(6717)) => {
                self.replies.extend([format!("text:310:{}", default.to_display()), "click:1".into()]);
                return default.clone();
            }
            _ => self.inputs += 1,
        }
        let result = match self.q.front() {
            Some(a) if !is_pick(a) => PbStr::from(self.q.pop_front().unwrap().as_str()),
            _ => PbStr::new(),
        };
        self.replies.push(result.to_display());
        result
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
                    self.replies.push("click:1".into());
                    return ReportAction::Ok;
                }
                let idx = links[k % links.len()];
                self.picks.push((tok, Some(idx)));
                self.replies.extend([format!("select:301:{idx}"), "click:1".into()]);
                let l = lines[idx].to_display();
                ReportAction::Link(PbStr::from(&l[l.len() - 6..]))
            }
            _ => { self.replies.push("click:1".into()); ReportAction::Ok },
        }
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

/// The replies actually consumed by the port, including report confirmation and symbol-dialog actions.
/// Unused planned input must not become an answer to an earlier original message box.
pub fn orig_answers(_answers: &[String], ui: &Script) -> Vec<String> {
    ui.replies.clone()
}

pub fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// INPUTBOX calls in the original's log.
pub fn orig_inputs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count()
}

/// Run the port, then the original with the equivalent answers; compare state, RND and the UI counts.
/// Returns the port's Script, state and the original's run.
#[allow(clippy::too_many_arguments)]
pub fn check(
    seed: u64,
    addr: u32,
    args: &[String],
    g: &Globals,
    answers: &[String],
    skips: &[(u32, u32)],
    compare: Compare,
    port: impl Fn(&mut Globals, &mut Script, &mut Rng),
) -> Option<(Script, Globals, Original)> {
    let __content = wsr_core::content::current();
    let rng0 = rng_of(seed);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Script { q: answers.iter().cloned().collect(), ..Script::default() };
    port(&mut pg, &mut ui, &mut rng);
    if std::env::var("WSR_DEBUG").is_ok() {
        eprintln!("seed {seed}{__wsr_text_13576}{answers:?}\nseed {seed} port titles {:?}{__wsr_text_14320}{}\nseed {seed} port news {:?}", ui.titles, ui.inputs, ui.news,__wsr_text_13576=__content.string(13576), __wsr_text_14320=__content.string(14320));
    }
    let oa = orig_answers(answers, &ui);
    let oa: Vec<&str> = oa.iter().map(|s| s.as_str()).collect();
    // `WSR_HOOKS=0x528736,...`: log calls of these routines in the original (debugging)
    let hooks = std::env::var("WSR_HOOKS")
        .map(|v| v.split(',').filter_map(|a| u32::from_str_radix(a.trim_start_matches("0x"), 16).ok()).collect())
        .unwrap_or_default();
    let opts =
        Opts { args: args.to_vec(), rng: Some(rng0), answers: &oa, skips: skips.to_vec(), hooks, ..Opts::default() };
    // a panic inside the harness (e.g. a MSGBOX answer the original reads as a number) names the seed
    struct SeedGuard(u64);
    impl Drop for SeedGuard {
        fn drop(&mut self) {
            if std::thread::panicking() {
                eprintln!("(while running the original for seed {})", self.0);
            }
        }
    }
    let guard = SeedGuard(seed);
    let o = run_original_with(addr, &opts, g)?;
    std::mem::forget(guard);
    if std::env::var("WSR_DEBUG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("seed {seed} orig: {l}");
        }
    }
    // the symbol entry dialog's window handle (UI only; the original keeps it after DIALOG END)
    let d = compare.ignore("DlgSymbolEntry").diffs(&pg, &o.globals);
    assert!(
        d.is_empty(),
        "seed {seed}: {} differences:\n{}",
        d.len(),
        d.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.titles, o.log);
    assert_eq!(ui.inputs, orig_inputs(&o), "inputs seed {seed}");
    Some((ui, pg, o))
}

/// `range`, or only the seed in `$WSR_SEED` (debugging).
pub fn seeds(range: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    match std::env::var("WSR_SEED").ok().and_then(|s| s.parse().ok()) {
        Some(s) => vec![s],
        None => range.collect(),
    }
}

/// Branch-coverage bookkeeping: every fragment of `want` must occur in a recorded message box title or
/// text or a news headline of some seed.
#[derive(Default)]
pub struct Hits(pub Mutex<Vec<String>>);

impl Hits {
    pub fn record(&self, ui: &Script) {
        let mut h = self.0.lock().unwrap();
        h.extend(ui.titles.iter().cloned());
        h.extend(ui.msgs.iter().cloned());
        h.extend(ui.news.iter().cloned());
    }
    pub fn add(&self, s: &str) {
        self.0.lock().unwrap().push(s.to_string());
    }
    pub fn assert(&self, want: &[&str]) {
        if unavailable().is_some() || std::env::var("WSR_SEED").is_ok() || std::env::var("WSR_ONLY_SEED").is_ok() {
            return;
        }
        let h = self.0.lock().unwrap();
        for w in want {
            assert!(h.iter().any(|x| x.contains(w)), "branch not hit: {w:?}");
        }
    }
}

/// Removes every holding of `corp` (all holders) and returns how many there were.
pub fn clear_holders(g: &mut Globals, corp: i32) -> usize {
    let mut n = 0;
    for e in 1..=1600 {
        for s in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(s, e)]) == corp {
                g.hold_stock_id[(s, e)] = 0;
                g.hold_stock_pct[(s, e)] = 0;
                g.hold_stock_basis[(s, e)] = Ext::from_f64(0.0);
                n += 1;
            }
        }
        g.hold_stock_id[(0, e)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, e)] != 0).count());
    }
    n
}

/// Puts a holding of `pct` percent of `corp` into a free slot of `holder` (returns the slot, 0 if full).
pub fn give_holding(g: &mut Globals, holder: i32, corp: i32, pct: i16, basis: Ext) -> i32 {
    let Some(s) = (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0) else {
        return 0;
    };
    g.hold_stock_id[(s, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(s, holder)] = pct;
    g.hold_stock_basis[(s, holder)] = basis;
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&k| g.hold_stock_id[(k, holder)] != 0).count());
    s
}

/// A random active company id in `lo..=hi` other than `not`.
pub fn pick_active(r: &mut Rand, g: &Globals, lo: i64, hi: i64, not: &[i32]) -> i32 {
    loop {
        let c = cast::<i32, _>(r.range(lo, hi));
        if !not.contains(&c) && g.entity_status[c] == 1 {
            return c;
        }
    }
}
