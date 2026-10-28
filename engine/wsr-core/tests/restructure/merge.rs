//! Differential tests of `logic::restructure::merge`: MergeCompanies (player mode with its dialogs,
//! computer modes 1 and 2) and AdjustOptionsForMerger against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::restructure::merge::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const MERGE_COMPANIES: u32 = 0x509293;
const ADJUST_OPTIONS_FOR_MERGER: u32 = 0x5f861f;
/// LiquidateSubIntoParent / SpinOffSubsidiaryMenu (2 BYREF args each), left out in both runs.
const SKIP_LIQUIDATE: (u32, u32) = (LIQUIDATE_SUB_INTO_PARENT, 2);
const SKIP_SPIN_OFF: (u32, u32) = (SPIN_OFF_SUBSIDIARY_MENU, 2);

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_246_822_519) ^ 0x2545_f491
}

/// Python `repr` of a latin-1 string (as the harness logs MSGBOX texts).
fn py_repr(s: &str) -> String {
    let __content = wsr_core::content::current();
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (cast::<u32, _>(c)) < 0x20 || (0x7f..0xa1).contains(&(cast::<u32, _>(c))) || cast::<u32, _>(c) == 0xad => {
                out.push_str(&format!("{__wsr_text_13660}{:02x}", cast::<u32, _>(c),__wsr_text_13660=__content.string(13660)))
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// Records message boxes (text, style, title) and answers them from a queue (default YES / OK).
#[derive(Default)]
struct MRec {
    msgs: Vec<String>,
    boxes: Vec<(String, i32, String)>,
    answers: std::collections::VecDeque<i32>,
}

impl Ui for MRec {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.boxes.push((text.to_display(), style, title.to_display()));
        // ShowQuickMessage pop-ups (style 0, no title) run qm.exe in the original and take no answer
        if !(style == 0 && title.is_empty())
            && let Some(a) = self.answers.pop_front()
        {
            return a;
        }
        if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _headline: &PbStr) {}
}

/// Compares state, RND state, the number of message boxes / quick messages and the MSGBOX texts.
#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &MRec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}", ui.msgs);
    }
    for l in o.log.iter().filter(|l| l.starts_with("PBERR")) {
        eprintln!("{what}: {l}");
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?} vs {:?}",
        ui.msgs,
        o.log
    );
    // the MSGBOX texts (quick messages go through qm.exe and are not in the log)
    let orig: Vec<&String> = o.log.iter().filter(|l| l.starts_with("MSGBOX")).collect();
    let port_boxes: Vec<&(String, i32, String)> = ui
        .boxes
        .iter()
        .filter(|(_, style, t)| !(*style == 0 && t.is_empty()))
        .collect();
    assert_eq!(port_boxes.len(), orig.len(), "{what}: MSGBOX count");
    for ((text, style, title), line) in port_boxes.iter().map(|b| (&b.0, b.1, &b.2)).zip(orig) {
        let caption = if title.is_empty() { __content.string(14074) } else { title.as_str() };
        let want = format!("MSGBOX {} {} type=0x{style:x} ", py_repr(caption), py_repr(text));
        assert!(
            line.starts_with(&want),
            "{what}: MSGBOX text\n port: {want}\n orig: {line}"
        );
    }
}

/// Counts outcomes (for the coverage assertions).
struct Hits(Mutex<BTreeMap<String, usize>>);

impl Hits {
    fn new() -> Self {
        Hits(Mutex::new(BTreeMap::new()))
    }
    fn hit(&self, k: &str) {
        *self.0.lock().unwrap().entry(k.to_string()).or_default() += 1;
    }
    #[track_caller]
    fn require(&self, keys: &[&str]) {
    let __content = wsr_core::content::current();
        let m = self.0.lock().unwrap();
        eprintln!("{__wsr_text_13561}{m:?}",__wsr_text_13561=__content.string(13561));
        if m.is_empty() {
            return; // harness unavailable
        }
        for k in keys {
            assert!(m.contains_key(*k), "branch not covered: {k} ({m:?})");
        }
    }
}

/// Puts a holding of `corp` (pct, basis) into a free slot of `holder` (or slot 15).
fn give(g: &mut Globals, holder: i32, corp: i32, pct: i16, basis: Ext) -> i32 {
    let slot = (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0).unwrap_or(15);
    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(slot, holder)] = pct;
    g.hold_stock_basis[(slot, holder)] = basis;
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, holder)] != 0).count());
    slot
}

/// A random holder: a player, a fund or a company (never `a` or `b`).
fn holder(g: &Globals, r: &mut Rand, a: i32, b: i32) -> i32 {
    loop {
        let h = match r.range(0, 9) {
            0 | 1 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            2 => cast::<i32, _>(r.range(1581, 1583)),
            _ => cast::<i32, _>(r.range(11, 1580)),
        };
        if h != a && h != b {
            return h;
        }
    }
}

/// A world for a merger of `acq` and `tgt`: the random holdings of both stocks are replaced by a few
/// stakes (players, funds, companies, cross holdings), totals mostly below 100% (now and then 100%,
/// so that rounding can push the merged holdings over 100%).
fn merge_world(seed: u64) -> (Globals, i32, i32, Rand) {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 9000);
    let mut r = Rand(seed * 131 + 7);
    let banks = r.chance(cast::<f64, _>(Ext::from_f64(0.12)));
    let (acq, tgt) = loop {
        let (a, t) = if banks {
            (cast::<i32, _>(r.range(11, 85)), cast::<i32, _>(r.range(11, 85)))
        } else {
            (cast::<i32, _>(r.range(131, 1580)), cast::<i32, _>(r.range(131, 1580)))
        };
        if a != t {
            break (a, t);
        }
    };
    for c in [acq, tgt] {
        g.entity_status[c] = 1;
    }
    match r.range(0, 5) {
        0 => g.industry_num[tgt] = g.industry_num[acq],
        1 if !banks => g.industry_num[acq] = 70,
        2 if !banks => g.industry_num[tgt] = 70,
        _ => {}
    }
    // sizes: acquirer larger / smaller / similar, sometimes tiny
    g.market_cap[acq] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(4000.0));
    g.market_cap[tgt] = match r.range(0, 3) {
        0 => g.market_cap[acq] * f64r(&mut r, Ext::from_f64(0.02), Ext::from_f64(0.4)),
        1 => g.market_cap[acq] * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1)),
        2 => g.market_cap[acq] * f64r(&mut r, Ext::from_f64(1.5), Ext::from_f64(8.0)),
        _ => f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(60.0)),
    };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.cash[acq] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(40000.0));
        g.t_bills[acq] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.tax_loss_carry[tgt] = f64r(&mut r, -Ext::from_f64(3000.0), -Ext::from_f64(11.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        g.tax_loss_carry[acq] = f64r(&mut r, -Ext::from_f64(1500.0), -Ext::from_f64(301.0));
    }
    // clear all holdings of the two stocks
    for h in 1..=1600 {
        for slot in 1..=15 {
            let id = cast::<i32, _>(g.hold_stock_id[(slot, h)]);
            if id == acq || id == tgt {
                g.hold_stock_id[(slot, h)] = 0;
                g.hold_stock_pct[(slot, h)] = 0;
                g.hold_stock_basis[(slot, h)] = Ext::from_f64(0.0);
            }
        }
        g.hold_stock_id[(0, h)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, h)] != 0).count());
    }
    // stakes in the target (the acquirer's own stake first), and in the acquirer
    for (corp, other) in [(tgt, acq), (acq, tgt)] {
        let mut left: i32 = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { 100 } else { cast::<i32, _>(r.range(20, 95)) };
        if corp == tgt && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            let p = if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) {
                100
            } else {
                cast::<i32, _>(r.range(1, cast::<i64, _>(left) - 1))
            };
            let b = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(800.0));
            give(&mut g, acq, tgt, cast::<i16, _>(p), b);
            left -= p;
        }
        if corp == acq && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            let p = cast::<i32, _>(r.range(1, cast::<i64, _>(left.max(2)) - 1));
            let b = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(800.0));
            give(&mut g, tgt, acq, cast::<i16, _>(p), b);
            left -= p;
        }
        for _ in 0..r.range(0, 6) {
            if left < 2 {
                break;
            }
            let h = holder(&g, &mut r, corp, other);
            g.entity_status[h] = 1;
            let p = cast::<i32, _>(r.range(1, (cast::<i64, _>(left) - 1).min(60)));
            let basis = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
                -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0))
            } else {
                f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(900.0))
            };
            give(&mut g, h, corp, cast::<i16, _>(p), basis);
            left -= p;
            // some holders of both stocks
            if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) && left >= 2 {
                let q = cast::<i32, _>(r.range(1, (cast::<i64, _>(left) - 1).min(30)));
                let b = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(400.0));
                give(&mut g, h, other, cast::<i16, _>(q), b);
            }
        }
    }
    // a full portfolio of the acquirer (no slot for the target)
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        for slot in 1..=15 {
            if g.hold_stock_id[(slot, acq)] == 0 {
                let c = loop {
                    let c = cast::<i32, _>(r.range(131, 1580));
                    if c != acq && c != tgt {
                        break c;
                    }
                };
                g.entity_status[c] = 1;
                g.hold_stock_id[(slot, acq)] = cast::<i16, _>(c);
                g.hold_stock_pct[(slot, acq)] = cast::<i16, _>(r.range(1, 90));
                g.hold_stock_basis[(slot, acq)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(900.0));
            }
        }
        g.hold_stock_id[(0, acq)] = 15;
    }
    // control: self-dealing, the acquirer's group, others
    match r.range(0, 4) {
        0 => {
            g.corp_controller[acq] = g.cur_player;
            g.corp_controller[tgt] = g.cur_player;
        }
        1 => g.corp_controller[tgt] = g.corp_controller[acq],
        _ => {}
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
        g.merger_tip_corp = cast::<i16, _>(tgt);
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) {
        g.asbestos_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { acq } else { -acq };
    } else if r.chance(cast::<f64, _>(Ext::from_f64(0.08))) {
        g.superfund_corp = cast::<i16, _>(acq);
    }
    if banks && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.bank_merger_player = g.cur_player;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.news_headline[60] = st(__content.string(14075));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.country_idx[tgt] = [0, 2, 6, 12, 16, 31, 35, 46, 50][cast::<usize, _>(r.range(0, 8))];
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.country_idx[acq] = g.country_idx[tgt];
    }
    (g, acq, tgt, r)
}

fn premium_for(r: &mut Rand) -> i16 {
    [0, 3, 8, 15, 22, 30, 38, 45, 70, 120, -5][cast::<usize, _>(r.range(0, 10))]
}

/// Outcome keys of a merger run (from the port's messages and headlines).
fn outcome(port: &Globals, ui: &MRec, before: &Globals, acq: i32, tgt: i32) -> Vec<&'static str> {
    let __content = wsr_core::content::current();
    let mut k = vec![];
    let news: Vec<String> = port.news_headline.as_slice().iter().map(|h| h.to_display()).collect();
    let any_news = |p: &str| news.iter().any(|h| h.contains(p));
    let any_msg = |p: &str| ui.msgs.iter().any(|m| m.contains(p));
    let merged = (1..=15).any(|s| cast::<i32, _>(port.hold_stock_id[(s, acq)]) == tgt && port.hold_stock_pct[(s, acq)] == 100)
        || (1..=15).any(|s| cast::<i32, _>(port.hold_stock_id[(s, tgt)]) == acq && port.hold_stock_pct[(s, tgt)] == 100);
    if merged && before.shares_outstanding[acq] != port.shares_outstanding[acq]
        || before.shares_outstanding[tgt] != port.shares_outstanding[tgt]
    {
        k.push("merged");
    }
    for (key, p) in [
        ("voted-down", "reject"),
        ("approved-vote", __content.string(14076)),
        ("bank-halt", __content.string(14077)),
        ("cashed-out", __content.string(14078)),
        ("receives-cash", __content.string(14079)),
        ("receives-stock", __content.string(12699)),
        ("costs", __content.string(14080)),
        ("excess-loss", __content.string(14081)),
        ("divest-killed", __content.string(14082)),
        ("divest-required", __content.string(14083)),
        ("options-adjusted", __content.string(14085)),
    ] {
        if any_news(p) {
            k.push(key);
        }
    }
    for (key, p) in [
        (
            "error9",
            __content.string(14447),
        ),
        ("vote-dialog", __content.string(14086)),
        ("bank-denied", __content.string(14087)),
        ("reverse-asked", __content.string(14088)),
        ("already-100", __content.string(14089)),
        ("already-merged", __content.string(14090)),
        ("govt-ok", __content.string(14091)),
        ("govt-intervention", __content.string(14092)),
        ("divest-asked", __content.string(14093)),
        ("spin-off-asked", __content.string(14094)),
        ("final-obstacle", __content.string(14095)),
        ("options-warning", __content.string(14096)),
        ("vote-failed-msg", __content.string(14097)),
        ("unanimous", __content.string(14098)),
        ("lowball", __content.string(14099)),
    ] {
        if any_msg(p) {
            k.push(key);
        }
    }
    k
}

/// Runs MergeCompanies in the original and the port (both without LiquidateSubIntoParent) and compares
/// them.
#[allow(clippy::too_many_arguments)]
fn run_merge(
    g: &Globals,
    acq: i32,
    tgt: i32,
    mode: i32,
    premium: i16,
    answers: &[&str],
    rng_start: u32,
    what: &str,
) -> Option<(Globals, MRec)> {
    run_merge_with(g, acq, tgt, mode, premium, answers, rng_start, what, false)
}

/// [`run_merge`], optionally with LiquidateSubIntoParent.
#[allow(clippy::too_many_arguments)]
fn run_merge_with(
    g: &Globals,
    acq: i32,
    tgt: i32,
    mode: i32,
    premium: i16,
    answers: &[&str],
    rng_start: u32,
    what: &str,
    liquidate: bool,
) -> Option<(Globals, MRec)> {
    let __content = wsr_core::content::current();
    let skips: &[(u32, u32)] = if liquidate {
        &[SKIP_SPIN_OFF]
    } else {
        &[SKIP_LIQUIDATE, SKIP_SPIN_OFF]
    };
    let o = run(
        MERGE_COMPANIES,
        &[
            &acq.to_string(),
            &tgt.to_string(),
            &mode.to_string(),
            &format!("{__wsr_text_13653}{premium}",__wsr_text_13653=__content.string(13653)),
        ],
        g,
        rng_start,
        answers,
        skips,
    )?;
    let mut port = g.clone();
    let mut ui = MRec::default();
    ui.answers = answers
        .iter()
        .map(|a| match *a {
            "IDNO" => 7,
            "IDCANCEL" => 2,
            "IDOK" => 1,
            _ => 6,
        })
        .collect();
    let mut rng = Rng::from_state(rng_start);
    merge_companies_skipping(
        &mut port,
        &mut ui,
        &mut rng,
        acq,
        tgt,
        mode,
        premium,
        LocalTime::HARNESS.time_secs(),
        if liquidate {
            &[SPIN_OFF_SUBSIDIARY_MENU]
        } else {
            &[LIQUIDATE_SUB_INTO_PARENT, SPIN_OFF_SUBSIDIARY_MENU]
        },
    );
    check(&port, &rng, &ui, &o, what);
    Some((port, ui))
}

/// MergeCompanies in the computer modes 1 (a computer player's buy-out) and 2 (corporate merger).
#[test]
fn diff_merge_companies_ai() {
    let hits = Hits::new();
    par_seeds(1..=220, |seed| {
        let (g, acq, tgt, mut r) = merge_world(seed);
        let mode = 1 + cast::<i32, _>(seed % 2);
        let premium = premium_for(&mut r);
        let what = format!("seed {seed} MergeCompanies({acq}, {tgt}, {mode}, {premium})");
        let Some((port, ui)) = run_merge(&g, acq, tgt, mode, premium, &[], rng0(seed), &what) else {
            return;
        };
        for k in outcome(&port, &ui, &g, acq, tgt) {
            hits.hit(&format!("{k}-{mode}"));
        }
    });
    hits.require(&[
        "merged-1",
        "merged-2",
        "voted-down-2",
        "approved-vote-2",
        "costs-1",
        "costs-2",
        "receives-stock-2",
        "excess-loss-2",
    ]);
}

/// MergeCompanies for the player (mode 0) with its dialogs; random YES/NO answers.
#[test]
fn diff_merge_companies_player() {
    let hits = Hits::new();
    par_seeds(
        std::env::var("SEED")
            .ok()
            .map(|s| s.parse::<u64>().unwrap())
            .map_or(1..=260, |s| s..=s),
        |seed| {
            let (g, acq, tgt, mut r) = merge_world(seed + 5000);
            let premium = premium_for(&mut r);
            let answers: Vec<&str> = (0..10).map(|_| if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) { "IDNO" } else { "IDYES" }).collect();
            let what = format!("seed {seed} MergeCompanies({acq}, {tgt}, 0, {premium}) {answers:?}");
            let Some((port, ui)) = run_merge(&g, acq, tgt, 0, premium, &answers, rng0(seed + 5000), &what) else {
                return;
            };
            for k in outcome(&port, &ui, &g, acq, tgt) {
                hits.hit(k);
            }
        },
    );
    hits.require(&[
        "merged",
        "vote-failed-msg",
        "govt-ok",
        "costs",
        "reverse-asked",
        "divest-asked",
        "lowball",
    ]);
}

/// A merger in which the target's stockholders are asked in mode 2 (the player controls a holder of the
/// target): the vote dialog with its texts.
#[test]
fn diff_merge_companies_vote_dialog() {
    let hits = Hits::new();
    par_seeds(1..=120, |seed| {
        let (mut g, acq, tgt, mut r) = merge_world(seed + 12000);
        // the player (or a company the player controls) holds the target
        let h = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.cur_player
        } else {
            let c = holder(&g, &mut r, acq, tgt).max(131);
            g.corp_controller[c] = g.cur_player;
            g.entity_status[c] = 1;
            c
        };
        let (p, b) = (cast::<i16, _>(r.range(1, 40)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)));
        give(&mut g, h, tgt, p, b);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.tax_loss_carry[acq] = -f64r(&mut r, Ext::from_f64(301.0), Ext::from_f64(2000.0));
            g.tax_loss_carry[tgt] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(99.0));
        }
        let premium = premium_for(&mut r);
        let answers: Vec<&str> = (0..4).map(|_| if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { "IDNO" } else { "IDYES" }).collect();
        let what = format!("seed {seed} MergeCompanies({acq}, {tgt}, 2, {premium}) holder {h} {answers:?}");
        let Some((port, ui)) = run_merge(&g, acq, tgt, 2, premium, &answers, rng0(seed + 12000), &what) else {
            return;
        };
        for k in outcome(&port, &ui, &g, acq, tgt) {
            hits.hit(k);
        }
    });
    hits.require(&["vote-dialog", "merged", "voted-down"]);
}

/// AdjustOptionsForMerger on its own: option records of the stock and of others, ratios below/above 1,
/// quantities rounding to 0, and the zero ratio (error 151, silent).
#[test]
fn diff_adjust_options_for_merger() {
    let __content = wsr_core::content::current();
    par_seeds(1..=80, |seed| {
        let mut g = world(seed + 15000);
        let mut r = Rand(seed * 7 + 3);
        let corp = cast::<i32, _>(r.range(131, 1580));
        g.entity_status[corp] = 1;
        for i in 1..=cast::<i32, _>(r.range(0, 12)) {
            let c = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { corp } else { cast::<i32, _>(r.range(131, 1580)) };
            let holder = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            let qty = [-30, -1, 1, 2, 5, 100][cast::<usize, _>(r.range(0, 5))];
            let strike = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            let kind = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "C" } else { "P" };
            g.option_recs[i] = st(&format!(
                "{c}| {holder}|{kind}|{qty}|{strike:.4}|{}| {} |{:.2}",
                g.cur_year + 1,
                r.range(1, 4),
                f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(30.0)))
            ));
        }
        let ratio = [Ext::from_f64(0.0), Ext::from_f64(0.013), Ext::from_f64(0.37), Ext::from_f64(0.5), Ext::from_f64(0.999), Ext::from_f64(1.0), Ext::from_f64(1.7)][cast::<usize, _>(r.range(0, 6))];
        let rng_start = rng0(seed);
        let Some(o) = run(
            ADJUST_OPTIONS_FOR_MERGER,
            &[&corp.to_string(), &format!("{__wsr_text_13665}{ratio}",__wsr_text_13665=__content.string(13665))],
            &g,
            rng_start,
            &[],
            &[],
        ) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = MRec::default();
        let rng = Rng::from_state(rng_start);
        adjust_options_for_merger(&mut port, &mut ui, corp, ratio);
        check(
            &port,
            &rng,
            &ui,
            &o,
            &format!("seed {seed} AdjustOptionsForMerger({corp}, {ratio})"),
        );
    });
}

/// BUGS.md #220: Merger_TrimHoldingsTo100Pct takes one point from the largest holding until the merged
/// company's holdings add up to at most 100%; a holding reaches 0 only when all are at 1%, i.e. with
/// more than 100 holdings of 1%. Funds (not part of the vote) hold 1% stakes in the target in many
/// slots; the clearing of the basis then fails with error 9 and the ON ERROR pop-up.
#[test]
fn diff_merge_companies_trim_error9() {
    let hits = Hits::new();
    par_seeds(1..=24, |seed| {
        let (mut g, acq, tgt, mut r) = merge_world(seed + 20000);
        if acq < 131 || tgt < 131 {
            return; // operating companies only
        }
        for c in [acq, tgt] {
            g.industry_num[c] = 3 + cast::<i16, _>(seed % 60);
        }
        g.market_cap[tgt] = f64r(&mut r, Ext::from_f64(200.0), Ext::from_f64(900.0));
        g.market_cap[acq] = g.market_cap[tgt] * f64r(&mut r, Ext::from_f64(0.05), Ext::from_f64(0.3));
        g.cash[acq] = Ext::from_f64(50_000.0);
        g.bank_merger_player = 0;
        // 105..120 holdings of 1% of the target by funds 1581..1590
        let n = cast::<i32, _>(r.range(105, 120));
        let mut k = 0;
        'outer: for fund in 1581..=1590 {
            g.entity_status[fund] = 1;
            g.industry_num[fund] = 71;
            for slot in 1..=15 {
                if k == n {
                    break 'outer;
                }
                g.hold_stock_id[(slot, fund)] = cast::<i16, _>(tgt);
                g.hold_stock_pct[(slot, fund)] = 1;
                g.hold_stock_basis[(slot, fund)] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0));
                k += 1;
            }
            g.hold_stock_id[(0, fund)] = 15;
        }
        let mode = 1 + cast::<i32, _>(seed % 2);
        let what = format!("seed {seed} MergeCompanies({acq}, {tgt}, {mode}, 40) with {n} fund stakes");
        let Some((port, ui)) = run_merge(&g, acq, tgt, mode, 40, &[], rng0(seed + 20000), &what) else {
            return;
        };
        for k in outcome(&port, &ui, &g, acq, tgt) {
            hits.hit(k);
        }
    });
    hits.require(&["error9"]);
}

/// The player's rarer dialogs: options on the target (held by the acquirer / the player), the final
/// divestiture of a 20..50% holder of the acquirer in the same group and industry, the acquirer already
/// owning 100% of the target, the target owning 100% of the acquirer.
#[test]
fn diff_merge_companies_player_edges() {
    let hits = Hits::new();
    par_seeds(1..=160, |seed| {
        let (mut g, acq, tgt, mut r) = merge_world(seed + 30000);
        match seed % 4 {
            0 => {
                // options on the target
                let holder = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { acq } else { g.cur_player };
                for i in 1..=cast::<i32, _>(r.range(1, 3)) {
                    let qty = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { -5 } else { 10 };
                    g.option_recs[i] = st(&format!(
                        "{tgt}|{}|{holder}|{qty}|{:.2}|{}|1|0",
                        r.range(1, 2),
                        f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(90.0))),
                        g.cur_year + 1
                    ));
                }
                g.option_recs[0] = st(" 3");
            }
            1 => {
                // a blocking holder of the acquirer: (tgt + year + acq) MOD 10 = 1
                if acq < 131 || tgt < 131 {
                    return; // an operating company (131..1580) in a bank's industry would be inconsistent
                }
                g.cur_year += (1 - (tgt + g.cur_year + acq) % 10 + 10) % 10;
                let b = loop {
                    let b = cast::<i32, _>(r.range(131, 1580));
                    if b != acq && b != tgt {
                        break b;
                    }
                };
                g.entity_status[b] = 1;
                g.corp_controller[b] = g.corp_controller[acq];
                g.industry_num[b] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.industry_num[tgt]
                } else {
                    g.industry_num[acq]
                };
                for slot in 1..=15 {
                    if cast::<i32, _>(g.hold_stock_id[(slot, b)]) == acq {
                        g.hold_stock_id[(slot, b)] = 0;
                    }
                }
                let (p, basis) = (cast::<i16, _>(r.range(20, 50)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(600.0)));
                give(&mut g, b, acq, p, basis);
            }
            2 => {
                // the acquirer already owns 100% of the target
                for h in 1..=1600 {
                    for slot in 1..=15 {
                        if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == tgt {
                            g.hold_stock_id[(slot, h)] = 0;
                            g.hold_stock_pct[(slot, h)] = 0;
                        }
                    }
                }
                give(&mut g, acq, tgt, 100, Ext::from_f64(500.0));
            }
            _ => {
                // the target owns 100% of the acquirer
                for h in 1..=1600 {
                    for slot in 1..=15 {
                        if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == acq {
                            g.hold_stock_id[(slot, h)] = 0;
                            g.hold_stock_pct[(slot, h)] = 0;
                        }
                    }
                }
                give(&mut g, tgt, acq, 100, Ext::from_f64(500.0));
            }
        }
        let premium = [20, 35, 50][cast::<usize, _>(r.range(0, 2))];
        let answers: Vec<&str> = (0..10).map(|_| if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { "IDNO" } else { "IDYES" }).collect();
        let what = format!("seed {seed} MergeCompanies({acq}, {tgt}, 0, {premium}) edges {answers:?}");
        let Some((port, ui)) = run_merge(&g, acq, tgt, 0, premium, &answers, rng0(seed + 30000), &what) else {
            return;
        };
        for k in outcome(&port, &ui, &g, acq, tgt) {
            hits.hit(k);
        }
    });
    hits.require(&[
        "options-warning",
        "final-obstacle",
        "divest-required",
        "already-100",
        "already-merged",
    ]);
}

/// MergeCompanies followed by the liquidation of the target into the acquirer (LiquidateSubIntoParent,
/// operating companies of one industry), in all three modes.
#[test]
fn diff_merge_companies_with_liquidation() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=90, |seed| {
        let (mut g, acq, tgt, mut r) = merge_world(seed + 40000);
        if acq < 131 || tgt < 131 {
            return; // operating companies only
        }
        g.industry_num[acq] = 3 + cast::<i16, _>(seed % 60);
        g.industry_num[tgt] = g.industry_num[acq];
        g.cash[acq] = f64r(&mut r, Ext::from_f64(3000.0), Ext::from_f64(30000.0));
        let mode = cast::<i32, _>(seed % 3);
        let premium = [10, 20, 35, 45][cast::<usize, _>(r.range(0, 3))];
        let answers: Vec<&str> = (0..12).map(|_| if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { "IDNO" } else { "IDYES" }).collect();
        let what = format!("seed {seed} MergeCompanies({acq}, {tgt}, {mode}, {premium}{__wsr_text_14100}{answers:?}",__wsr_text_14100=__content.string(14100));
        let Some((port, ui)) = run_merge_with(&g, acq, tgt, mode, premium, &answers, rng0(seed + 40000), &what, true)
        else {
            return;
        };
        if port.entity_status[tgt] != g.entity_status[tgt] {
            hits.hit(&format!("{__wsr_text_14101}{mode}",__wsr_text_14101=__content.string(14101)));
        }
        for k in outcome(&port, &ui, &g, acq, tgt) {
            hits.hit(&format!("{k}-{mode}"));
        }
    });
    hits.require(&["liquidated-0", "liquidated-1", "liquidated-2"]);
}
