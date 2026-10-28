//! Differential tests of `logic::trade_screens::ir_swaps`: the interest-rate swap dialog
//! (ShowIRSwapDialog 0x63b281) driven like the original's dialog — option buttons, edit fields, OFFER
//! (IRSwap_CreateSwap with validation, position limits and the counterparty search), VIEW LIST
//! (PickSwapFromList + ShowSwapDetails mode 1) and TERMINATE (ShowSwapDetails mode 2).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Mutex;
use wsr_core::logic::trade_screens::ir_swaps::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SHOW_IR_SWAP_DIALOG: u32 = 0x63b281;

/// Scripted UI for the swap dialog. Tokens: `c:K` (choose option K of [`IrSwapControl::ALL`]), `t:TEXT`
/// (the text typed into the edit field chosen just before), `pick:K` (a report: click its K-th link line),
/// `rok` (a report: OK), MSGBOX answers `IDYES`/`IDNO`/`IDOK`/`IDCANCEL`. While running the port it
/// builds the original's answer list in consumption order: `click:ID`, `text:ID:TEXT`, `select:301:N`,
/// `click:1` (report OK), `close` and the MSGBOX answers.
#[derive(Default)]
struct SwapUi {
    q: VecDeque<String>,
    orig: Vec<String>,
    msgs: Vec<String>,
    titles: Vec<String>,
    news: Vec<String>,
    reports: usize,
    field: i32,
}

fn is_msg_answer(a: &str) -> bool {
    a.starts_with("ID")
}

impl Ui for SwapUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage (qm.exe): no answer
        }
        let yes_no = matches!(style & 0xf, 3 | 4);
        match self.q.front() {
            Some(a) if is_msg_answer(a) => {
                let a = self.q.pop_front().unwrap();
                self.orig.push(a.clone());
                match a.as_str() {
                    "IDOK" => 1,
                    "IDCANCEL" => 2,
                    "IDYES" => 6,
                    "IDNO" => 7,
                    _ => unreachable!(),
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
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        match self.q.front() {
            Some(a) if a.starts_with("t:") => {
                let a = self.q.pop_front().unwrap();
                self.orig.push(format!("text:{}:{}", self.field, &a[2..]));
                PbStr::from(&a[2..])
            }
            _ => default.clone(),
        }
    }
    fn report(&mut self, _title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports += 1;
        match self.q.front().cloned() {
            Some(a) if a.starts_with("pick:") => {
                self.q.pop_front();
                let k: usize = a[5..].parse().unwrap();
                let links: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].to_display().contains("@C")).collect();
                if links.is_empty() {
                    self.orig.push("click:1".into());
                    return ReportAction::Ok;
                }
                let idx = links[k % links.len()];
                self.orig.push(format!("select:301:{idx}"));
                self.orig.push("click:1".into());
                let l = lines[idx].to_display();
                ReportAction::Link(PbStr::from(&l[l.len() - 6..]))
            }
            Some(a) if a == "rok" => {
                self.q.pop_front();
                self.orig.push("click:1".into());
                ReportAction::Ok
            }
            _ => {
                self.orig.push("click:1".into());
                ReportAction::Ok
            }
        }
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
    fn choose(&mut self, _title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        assert_eq!(options.len(), IrSwapControl::ALL.len());
        match self.q.front() {
            Some(a) if a.starts_with("c:") => {
                let a = self.q.pop_front().unwrap();
                let k: usize = a[2..].parse().unwrap();
                let ctrl = IrSwapControl::ALL[k];
                let mut f = IrSwapForm::default();
                if ctrl.field(&mut f).is_some() {
                    self.field = ctrl.id();
                } else {
                    self.orig.push(format!("click:{}", ctrl.id()));
                }
                Some(k)
            }
            _ => {
                self.orig.push("sysclose".into());
                None
            }
        }
    }
}

fn opt(c: IrSwapControl) -> String {
    format!("c:{}", IrSwapControl::ALL.iter().position(|&x| x == c).unwrap())
}

fn rng0(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20) | 1
}

/// A random swap record `long|short|rate|type|start|end|notional`.
fn swap_rec(g: &Globals, r: &mut Rand, long: i32, short: i32) -> PbStr {
    let start = cast::<i64, _>(g.cur_year) + r.range(-2, 1);
    let sq = r.range(1, 4);
    let end = start + r.range(0, 5);
    let eq = r.range(1, 4);
    let ty = ["P", "L", "S"][cast::<usize, _>(r.range(0, 2))];
    let rate = match r.range(0, 2) {
        0 => format!("{:.2}", f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(12.0)))),
        1 => format!("{:.1}", f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(12.0)))),
        _ => format!("{}", r.range(1, 12)),
    };
    st(&format!("{long}|{short}|{rate}|{ty}|{start}.{sq}|{end}.{eq}|{}", r.range(100, 60_000)))
}

/// [`world`] with a swap market, a solvent acting entity and some swaps of it.
fn swap_world(seed: u64) -> (Globals, Rand) {
    let mut g = world(seed);
    let mut r = Rand(seed * 977 + 13);
    g.sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { 0 } else { g.active_entity };
    g.central_bank_name = st("the Fed");
    g.short_bond_desc = st(&format!("{} SHORT T-NOTE {:5.2}", g.cur_year + cast::<i32, _>(r.range(1, 6)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(9.0)))));
    g.active_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.cur_player
    } else {
        // a company the player controls: bank, insurer, brokerage or other
        let lo = [11, 86, 131, 131][cast::<usize, _>(r.range(0, 3))];
        let hi = if lo == 11 { 85 } else if lo == 86 { 130 } else { 1580 };
        let c = loop {
            let c = cast::<i32, _>(r.range(lo, hi));
            if g.entity_status[c] == 1 {
                break c;
            }
        };
        g.corp_controller[c] = g.cur_player;
        if lo == 131 && r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.industry_num[c] = 37;
        }
        c
    };
    let a = g.active_entity;
    g.active_entity_name = g.ent_name[a].clone();
    g.credit_rating_num[a] = if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { cast::<i32, _>(r.range(7, 10)) } else { cast::<i32, _>(r.range(1, 6)) };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
        g.cash[a] = f64r(&mut r, Ext::from_f64(3_000.0), Ext::from_f64(400_000.0));
    }
    for e in 11..=1600 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.credit_rating_num[e] = cast::<i32, _>(r.range(7, 10));
        }
        if 130 < e && r.chance(cast::<f64, _>(Ext::from_f64(0.04))) {
            g.industry_num[e] = 37;
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.industry_num[1595] = 71;
    }
    let n = cast::<i32, _>(r.range(0, 40));
    for i in 1..=n {
        let other = cast::<i32, _>(r.range(11, 1600));
        let (l, s) = match r.range(0, 3) {
            0 => (a, other),
            1 => (other, a),
            _ => (cast::<i32, _>(r.range(11, 1600)), other),
        };
        g.swap_contracts[i] = swap_rec(&g, &mut r, l, s);
    }
    g.swap_contracts[0] = st(&n.to_string());
    g.turn_action_count = cast::<i32, _>(r.range(0, 5));
    g.solo_human_mode = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
    (g, r)
}

/// A script for one dialog session.
fn script(g: &Globals, r: &mut Rand) -> Vec<String> {
    use IrSwapControl::*;
    let mut v: Vec<String> = Vec::new();
    let msg = |r: &mut Rand| if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { "IDYES" } else { "IDNO" }.to_string();
    match r.range(0, 9) {
        // VIEW LIST: pick swaps, analysis, then OK
        0 | 1 => {
            v.push(opt(ViewSwaps));
            for _ in 0..r.range(0, 2) {
                v.push(format!("pick:{}", r.range(0, 30)));
            }
            v.push("rok".into());
        }
        // TERMINATE
        2 | 3 => {
            v.push(opt(Terminate));
            for _ in 0..r.range(1, 2) {
                v.push(format!("pick:{}", r.range(0, 30)));
                v.push(msg(r));
                v.push(msg(r));
            }
            v.push("rok".into());
        }
        // OFFER
        _ => {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
                v.push(opt([LongBondRate, ShortBondRate, PrimeRate][cast::<usize, _>(r.range(0, 2))]));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
                v.push(opt(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Long } else { Short }));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                v.push(opt(Notional));
                let x = match r.range(0, 5) {
                    0 => "".to_string(),
                    1 => format!("{}", r.range(1, 99)),
                    2 => format!("${},{:03}", r.range(1, 900), r.range(0, 999)),
                    3 => format!("{}", r.range(100, 20_000)),
                    _ => format!("{}", r.range(100, 3_000_000)),
                };
                v.push(format!("t:{x}"));
            }
            let fields = [StartQtr, StartYear, EndQtr, EndYear];
            for f in fields {
                if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
                    v.push(opt(f));
                    let x = match f {
                        StartQtr | EndQtr => r.range(0, 5).to_string(),
                        _ => (cast::<i64, _>(g.cur_year) + r.range(-1, 7)).to_string(),
                    };
                    v.push(format!("t:{x}"));
                }
            }
            v.push(opt(Offer));
            v.push(msg(r));
            v.push(msg(r));
        }
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        v.push(opt(Info));
    }
    v.push(opt(Exit));
    v
}

#[test]
fn diff_show_ir_swap_dialog() {
    let __content = wsr_core::content::current();
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(1..=402, |seed| {
        let (mut g, mut r) = swap_world(seed);
        let mut steps = script(&g, &mut r);
        if seed > 400 {
            // Deliberate small balance sheets cover entry refusals that the asset-rich random
            // worlds do not reach once CalcEntityFinancials recomputes the credit rating.
            g = Globals::default();
            g.num_players = 2; g.cur_player = 1; g.active_entity = 1;
            g.entity_status[1] = 1; g.entity_status[11] = 1; g.lending_bank[1] = 11;
            g.ent_name[1] = st("Test Player"); g.player_name[1] = st("Test Player");
            g.active_entity_name = st("Test Player"); g.ent_name[11] = st("Test Bank");
            g.currency_code = st("USA"); g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
            g.cur_year = 2020; g.cur_quarter = 1; g.start_year = 2019; g.sim_co_count = 100;
            g.cash[1] = if seed == 401 { -Ext::from_f64(1.0e9) } else { Ext::from_f64(500.0) };
            steps = vec![opt(IrSwapControl::Offer)];
        }
        if steps.contains(&opt(IrSwapControl::Terminate)) {
            // Keep dialog termination focused on swap settlement. An unaffordable fee in this
            // dense synthetic ownership graph launches whole-company liquidation; that separate
            // subsystem has its own tests and can take billions of original instructions here.
            g.cash[g.active_entity] = Ext::from_f64(10_000_000.0);
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = SwapUi { q: steps.iter().cloned().collect(), ..SwapUi::default() };
        show_ir_swap_dialog(&mut port, &mut ui, &mut rng, 0);
        let answers: Vec<&str> = ui.orig.iter().map(|s| s.as_str()).collect();
        let what = format!("seed {seed}{__wsr_text_14017}{steps:?}\norig {answers:?}",__wsr_text_14017=__content.string(14017));
        let Some(o) = run(SHOW_IR_SWAP_DIALOG, &["v:0"], &g, rs, &answers, &[]) else {
            return;
        };
        if std::env::var("DBG").is_ok() {
            for l in o.log.iter().filter(|l| !l.starts_with("API")) {
                eprintln!("seed {seed}: {l}");
            }
            eprintln!("seed {seed}: port msgs {:?}", ui.titles);
        }
        assert!(!o.log.iter().any(|l| l.contains("FORCED END")), "{what}: dialog forced to end\n{:?}", o.log);
        let d = cmp().ignore("DlgIRSwap").rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).diffs(&port, &o.globals);
        assert!(d.is_empty(), "{what}: {} differences (port vs original):\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
        assert_eq!(rng.state(), o.rng, "{what}: RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "{what}: message boxes {:?} vs {:?}", ui.titles, o.log);
        let mut s = seen.lock().unwrap();
        for t in ui.titles.iter().chain(ui.msgs.iter()).chain(ui.news.iter()) {
            for key in COVER(&__content) {
                if t.contains(key) {
                    s.insert(key.to_string());
                }
            }
        }
        if port.swap_contracts[0] != g.swap_contracts[0] {
            s.insert("count changed".into());
        }
    });
    if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
        return;
    }
    let s = seen.into_inner().unwrap();
    eprintln!("COV {s:?}");
    for k in COVER(&__content) {
        assert!(s.contains(k), "branch not covered: {k}\n{s:?}");
    }
}

#[allow(non_snake_case)]
fn COVER(__content: &wsr_core::content::ContentPack) -> Vec<&str> { vec![
    __content.string(11440),
    __content.string(11445),
    __content.string(14207),
    __content.string(14208),
    __content.string(11493),
    __content.string(14209),
    __content.string(14210),
    __content.string(14211),
    __content.string(14212),
    __content.string(14213),
    __content.string(14214),
    __content.string(14215),
    __content.string(14216),
    __content.string(14217),
    __content.string(14218),
    __content.string(14219),
    __content.string(14220),
    __content.string(14221),
    __content.string(11213),
    __content.string(14222),
    __content.string(14223),
    __content.string(14224),
] }
