//! Differential tests of `logic::trade_screens::commod_crypto`: the commodity desk ShowCommodityDesk
//! (0x627a84) and the crypto platform ShowDlgCrypto (0x6b4357), driven button by button.
//!
//! Record and replay: the port runs first with a random (seeded) player that clicks buttons (planned ones
//! when they are shown), answers message boxes and input boxes and picks report lines; every answer is
//! recorded as the harness token the original needs at that point (`click:ID`, `close`, `IDYES`, input
//! text, `select:301:N` / `click:1` for reports). The original then runs on the same world with these
//! tokens and both end states are compared.
use wsr_core::pb::real::{Ext, cast};


use super::common as sim_common;
use super::common::{orig_msgs, st};
use super::diff;
use super::diff::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use wsr_core::logic::restructure::dlg_util::{Nmhdr, TTN_NEEDTEXT};
use wsr_core::logic::trade_screens::commod_crypto::commod_desk::{dlg_proc_commod_desk, show_commodity_desk};
use wsr_core::logic::trade_screens::commod_crypto::crypto::{
    BUTTON_IDS as CRYPTO_IDS, dlg_proc_crypto_colors, show_dlg_crypto_skipping_chart,
};
use wsr_core::logic::newgame::LocalTime;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

#[allow(unused_imports, dead_code)]
#[path = "../commod_ai/common.rs"]
mod ccommon;
use ccommon::{COMMODS, PHYS, cworld, futures_rec};

const SHOW_DESK: u32 = 0x627a84;
const SHOW_CRYPTO: u32 = 0x6b4357;
/// DisableMainControls (0x4242c6): UI only; ShowPriceChart (0x6141b1): tested separately.
const SKIPS: [(u32, u32); 2] = [(0x4242c6, 0), (0x6141b1, 1)];

/// The desk button a caption belongs to (captions are unique across all states of the desk).
fn desk_id(caption: &str) -> i32 {
    let __content = wsr_core::content::current();
    match caption {
        value if [__content.string(5722), "BUY"].contains(&value) => 542,
        "GOLD" | "SELL" => 544,
        value if ["SILVER", __content.string(14192)].contains(&value) => 546,
        "WHEAT" | "SELL SHORT" => 548,
        "CORN" | "RE-SELECT" => 550,
        value if [__content.string(2499), "---------", __content.string(11361), __content.string(11355)].contains(&value) => 552,
        "POSITIONS" => 554,
        "CLOSE" => 556,
        _ => panic!("unknown desk caption {caption:?}"),
    }
}

/// A random player recording the original's answer tokens.
struct Player {
    r: Rand,
    /// Buttons to click in this order (when shown; else a random shown one).
    plan: Vec<i32>,
    /// Clicks per dialog before the harness would destroy it.
    clicks: usize,
    tokens: Vec<String>,
    msgs: Vec<String>,
    inputs: usize,
    clicked: Vec<i32>,
    reports: Vec<String>,
}

impl Player {
    fn new(seed: u64, plan: Vec<i32>) -> Self {
        Player {
            r: Rand(seed.wrapping_mul(0x9e37_79b9) | 1),
            plan,
            clicks: 0,
            tokens: Vec::new(),
            msgs: Vec::new(),
            inputs: 0,
            clicked: Vec::new(),
            reports: Vec::new(),
        }
    }
}

impl Ui for Player {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(format!("[{title}] {text}"));
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage (qm.exe): no answer
        }
        let (tok, v) = match style & 0xf {
            3 => [("IDYES", 6), ("IDYES", 6), ("IDNO", 7), ("IDCANCEL", 2)][cast::<usize, _>(self.r.range(0, 3))],
            4 => [("IDYES", 6), ("IDYES", 6), ("IDYES", 6), ("IDNO", 7)][cast::<usize, _>(self.r.range(0, 3))],
            1 => [("IDOK", 1), ("IDOK", 1), ("IDOK", 1), ("IDCANCEL", 2)][cast::<usize, _>(self.r.range(0, 3))],
            _ => ("IDOK", 1),
        };
        self.tokens.push(tok.to_string());
        v
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        assert!(!prompt.to_display().ends_with(__content.string(6717)), "symbol dialog not scripted");
        self.inputs += 1;
        let a = match self.r.range(0, 9) {
            0 => String::new(),
            1 => "0".to_string(),
            2 => default.to_display(),
            3 => format!("{}", self.r.range(20, 500)),
            _ => format!("{}", self.r.range(1, 12)),
        };
        self.tokens.push(a.clone());
        PbStr::from(a.as_str())
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction {
        self.reports.push(title.to_display());
        let links: Vec<usize> = (0..lines.len())
            .filter(|&i| {
                let l = lines[i].to_display();
                l.contains("@C") || l.contains("@I")
            })
            .collect();
        if !links.is_empty() && self.r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
            let idx = links[cast::<usize, _>(self.r.range(0, cast::<i64, _>(links.len()) - 1))];
            self.tokens.push(format!("select:301:{idx}"));
            // LBN_SELCHANGE only stores the link; the separate OK button confirms it.
            self.tokens.push("click:1".to_string());
            let l = lines[idx].to_display();
            return ReportAction::Link(PbStr::from(&l[l.len() - 6..]));
        }
        if yes_no && self.r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            self.tokens.push("click:2".to_string());
            return ReportAction::No;
        }
        self.tokens.push("click:1".to_string());
        ReportAction::Ok
    }
    fn news(&mut self, _headline: &PbStr) {}
    fn choose(&mut self, title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
    let __content = wsr_core::content::current();
        let crypto = title.to_display() == __content.string(11387);
        self.clicks += 1;
        if self.clicks > 5 {
            self.tokens.push("end".to_string());
            return None; // the harness destroys the dialog (no callback)
        }
        let ids: Vec<i32> = if crypto {
            assert_eq!(options.len(), CRYPTO_IDS.len());
            CRYPTO_IDS.to_vec()
        } else {
            options.iter().map(|o| desk_id(&o.to_display())).collect()
        };
        let want = if self.plan.is_empty() { None } else { Some(self.plan.remove(0)) };
        let k = match want {
            Some(0) => {
                self.tokens.push("sysclose".to_string());
                return None;
            }
            Some(w) if ids.contains(&w) => ids.iter().position(|&x| x == w).unwrap(),
            _ => cast::<usize, _>(self.r.range(0, cast::<i64, _>(ids.len()) - 1)),
        };
        self.tokens.push(format!("click:{}", ids[k]));
        self.clicked.push(ids[k]);
        Some(k)
    }
}

/// Branch counters.
#[derive(Default)]
struct Hits(Mutex<BTreeMap<String, usize>>);

impl Hits {
    fn hit(&self, k: &str) {
        *self.0.lock().unwrap().entry(k.to_string()).or_default() += 1;
    }
    fn hit_texts(&self, p: &Player, keys: &[&str]) {
        for k in keys {
            if p.msgs.iter().any(|m| m.contains(k)) {
                self.hit(k);
            }
        }
    }
    #[track_caller]
    fn require(&self, keys: &[&str]) {
        if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
            return;
        }
        let m = self.0.lock().unwrap();
        for k in keys {
            assert!(m.get(*k).copied().unwrap_or(0) > 0, "branch {k:?} not hit: {m:?}");
        }
    }
}

fn compare() -> Compare {
    sim_common::cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).ignore("DlgCommodDesk").ignore("DlgCrypto")
}

/// Runs the port with `plan`, then the original with the recorded tokens; compares.
fn check(seed: u64, addr: u32, g: &Globals, plan: Vec<i32>, port: impl Fn(&mut Globals, &mut Player, &mut Rng)) -> Option<(Player, Globals)> {
    let rng0 = ccommon::rng0(seed);
    let mut pg = g.clone();
    let mut ui = Player::new(seed, plan);
    let mut rng = Rng::from_state(rng0);
    port(&mut pg, &mut ui, &mut rng);
    let toks: Vec<&str> = ui.tokens.iter().map(|s| s.as_str()).collect();
    let opts = Opts { args: vec!["v:0".to_string()], rng: Some(rng0), answers: &toks, skips: SKIPS.to_vec(), ..Opts::default() };
    let o = run_original_with(addr, &opts, g)?;
    if std::env::var("WSR_DEBUG").is_ok() {
        eprintln!("seed {seed} tokens {:?}\nport msgs {:#?}", ui.tokens, ui.msgs);
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("seed {seed} orig: {l}");
        }
    }
    let d = compare().diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences (tokens {:?}):\n{}", d.len(), ui.tokens, d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.msgs, o.log);
    let inputs = o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count();
    assert_eq!(ui.inputs, inputs, "inputs seed {seed}");
    Some((ui, pg))
}

/// A commodity world with `ActiveEntity` acting at the desk: one of the record owners (player or
/// company), with cash and good credit.
fn desk_world(seed: u64, r: &mut Rand) -> (Globals, i32) {
    let (mut g, owners) = cworld(seed);
    let e = owners[cast::<usize, _>(r.range(0, 9))];
    g.entity_status[e] = 1;
    g.active_entity = e;
    g.active_entity_name = g.ent_name[e].clone();
    if e <= 4 {
        g.cur_player = e;
    }
    g.cash[e] = ccommon::f64r(r, Ext::from_f64(200.0), Ext::from_f64(20000.0));
    g.credit_rating_num[e] = cast::<i32, _>(r.range(6, 10));
    if g.start_year == g.cur_year && g.cur_quarter == 1 && g.sim_co_count < 503 && !r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
        g.start_year -= 1; // the January suspension only sometimes
    }
    g.commod_trade_mode = st(["", "FUTURES", "PHYSICAL", "FUTURES"][cast::<usize, _>(r.range(0, 3))]);
    g.commod_mode_hint_mask = cast::<i32, _>(r.range(0, 3));
    g.turn_action_count = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(5, 8)) } else { cast::<i32, _>(r.range(0, 4)) };
    g.solo_human_mode = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
    g.commod_sel = cast::<i32, _>(r.range(0, 6));
    g.cur_commodity = st(COMMODS[cast::<usize, _>(r.range(0, 7))]);
    // some positions of the acting entity itself
    let mut n: i32 = g.futures_recs[0].to_display().trim().parse().unwrap_or(0);
    for _ in 0..r.range(0, 4) {
        if n >= 990 {
            break;
        }
        n += 1;
        let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
        g.futures_recs[n] = futures_rec(&g, r, c, e);
    }
    g.futures_recs[0] = st(&n.to_string());
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        let mut k: i32 = g.phys_commod_rec[0].to_display().trim().parse().unwrap_or(0);
        let c = PHYS[cast::<usize, _>(r.range(0, 6))];
        let have = (1..=k).any(|i| {
            let t = g.phys_commod_rec[i].to_display();
            t.starts_with(c) && t.split('|').nth(1) == Some(&e.to_string())
        });
        if !have {
            k += 1;
            g.phys_commod_rec[k] = st(&format!("{c}|{e}|{}|{:.2}", r.range(1, 3000), ccommon::f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
            g.phys_commod_rec[0] = st(&k.to_string());
        }
    }
    (g, e)
}

#[test]
fn diff_show_commodity_desk() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=161u64, |seed| {
        let mut r = Rand(seed * 31 + 7);
        let (mut g, e) = desk_world(seed + 20_000, &mut r);
        if seed % 9 == 0 {
            g.start_year = g.cur_year;
            g.cur_quarter = 1;
            g.sim_co_count = cast::<i32, _>(r.range(10, 500));
        }
        if seed % 4 == 1 {
            // coming from a price chart
            g.tmp_ext1 = -Ext::from_f64(999.0);
            g.chart_entity = [0, 6, 7, 8, 9, 10, 3, 1605][cast::<usize, _>(r.range(0, 7))];
        }
        if seed % 7 == 3 && e > 10 {
            // Keep entity kinds in their valid id ranges. Changing an operating company
            // into a bank or insurer indexes the original's bounded financial arrays out
            // of range. The world already includes real banks and insurers for this path.
            let _ = r.range(1, 2); // preserve the random script's draw
        }
        // plan: select something, maybe switch the mode, trade, maybe re-select / positions / close
        let mut plan = vec![[542, 544, 546, 548, 550, 552][cast::<usize, _>(r.range(0, 5))]];
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            plan.push(552);
        }
        plan.push([542, 544, 546, 548, 542, 544][cast::<usize, _>(r.range(0, 5))]);
        match r.range(0, 5) {
            0 => plan.push(550),
            1 => plan.push(554),
            2 => plan.push(556),
            3 => plan.push(0),
            _ => plan.push([542, 544, 546, 548, 550, 552, 554, 556][cast::<usize, _>(r.range(0, 7))]),
        }
        if seed % 6 == 0 {
            plan = vec![554]; // straight to the positions
        }
        if seed == 161 {
            // A deliberately empty book exercises the positions-menu refusal; random worlds
            // usually retain at least one futures or physical position for the acting entity.
            for k in 0..=1000 { g.futures_recs[k] = PbStr::new(); g.phys_commod_rec[k] = PbStr::new(); }
            g.futures_recs[0] = st("0"); g.phys_commod_rec[0] = st("0");
            plan = vec![554];
        }
        let Some((ui, pg)) = check(seed, SHOW_DESK, &g, plan, |g, ui, rng| show_commodity_desk(g, ui, rng, 0)) else {
            return;
        };
        for id in &ui.clicked {
            hits.hit(&format!("{__wsr_text_14193}{id}",__wsr_text_14193=__content.string(14193)));
        }
        if pg.commod_mode_hint_mask != g.commod_mode_hint_mask {
            hits.hit("hint");
        }
        if pg.futures_recs.as_slice() != g.futures_recs.as_slice() {
            hits.hit("futures traded");
        }
        if pg.phys_commod_rec.as_slice() != g.phys_commod_rec.as_slice() {
            hits.hit("physical traded");
        }
        if ui.reports.iter().any(|t| t.contains(__content.string(14194))) {
            hits.hit("phys list");
        }
        if ui.reports.iter().any(|t| t.contains(__content.string(14195))) {
            hits.hit("futures list");
        }
        hits.hit_texts(
            &ui,
            &[
                __content.string(14196),
                __content.string(14197),
                __content.string(14198),
                __content.string(14199),
                __content.string(14200),
                __content.string(14201),
                __content.string(11339),
                __content.string(14202),
            ],
        );
    });
    hits.require(&[
        "click 542",
        "click 544",
        "click 546",
        "click 548",
        "click 550",
        "click 552",
        "click 554",
        "click 556",
        "hint",
        "futures traded",
        "physical traded",
        "phys list",
        "futures list",
        __content.string(14196),
        __content.string(14197),
        __content.string(14198),
        __content.string(14199),
        __content.string(14200),
        __content.string(14201),
    ]);
}

#[test]
fn diff_show_dlg_crypto() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=120u64, |seed| {
        let mut r = Rand(seed * 37 + 11);
        let (mut g, e) = desk_world(seed + 21_000, &mut r);
        if seed % 10 == 0 {
            g.start_year = g.cur_year;
            g.cur_quarter = 1;
            g.sim_co_count = cast::<i32, _>(r.range(10, 500));
        }
        if seed % 8 == 1 {
            g.bitcoin_price = 0;
            g.ethereum_price = 0;
        }
        // crypto futures of the acting entity: long, short or both
        let mut n: i32 = g.futures_recs[0].to_display().trim().parse().unwrap_or(0);
        for _ in 0..r.range(0, 3) {
            n += 1;
            let c = ["BITCOIN", "ETHEREUM"][cast::<usize, _>(r.range(0, 1))];
            g.futures_recs[n] = futures_rec(&g, &mut r, c, e);
        }
        g.futures_recs[0] = st(&n.to_string());
        let mut plan: Vec<i32> = (0..r.range(1, 4)).map(|_| CRYPTO_IDS[cast::<usize, _>(r.range(0, 11))]).collect();
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            plan.insert(0, [1154, 1157, 1160, 1163][cast::<usize, _>(r.range(0, 3))]);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            plan.push(0);
        }
        let Some((ui, pg)) = check(seed, SHOW_CRYPTO, &g, plan, |g, ui, rng| show_dlg_crypto_skipping_chart(g, ui, rng, &LocalTime::HARNESS, true)) else {
            return;
        };
        for id in &ui.clicked {
            hits.hit(&format!("{__wsr_text_14193}{id}",__wsr_text_14193=__content.string(14193)));
        }
        if pg.futures_recs.as_slice() != g.futures_recs.as_slice() {
            hits.hit("futures traded");
        }
        if pg.phys_commod_rec.as_slice() != g.phys_commod_rec.as_slice() {
            hits.hit("physical traded");
        }
        hits.hit_texts(
            &ui,
            &[
                __content.string(14196),
                __content.string(14203),
                __content.string(14204),
                __content.string(14200),
            ],
        );
    });
    let mut want: Vec<String> = CRYPTO_IDS.iter().map(|id| format!("{__wsr_text_14193}{id}",__wsr_text_14193=__content.string(14193))).collect();
    want.extend(
        [
            "futures traded",
            "physical traded",
            __content.string(14196),
            __content.string(14203),
            __content.string(14204),
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    hits.require(&want.iter().map(|s| s.as_str()).collect::<Vec<_>>());
}

/// The dialog procedures: colours and tooltips (window plumbing).
#[test]
fn dlg_procs_colors_and_tooltips() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    for k in 0..=30 {
        g.brush_handles[k] = 1000 + cast::<i32, _>(k);
    }
    assert_eq!(dlg_proc_commod_desk(&g, 312, 540, None), 1022);
    assert_eq!(dlg_proc_commod_desk(&g, 312, 542, None), 0);
    assert_eq!(dlg_proc_commod_desk(&g, 310, 540, None), 0);
    let mut nm = Nmhdr { hwnd_from: 552, code: TTN_NEEDTEXT, flags: 0, text: PbStr::new() };
    dlg_proc_commod_desk(&g, 78, 0, Some(&mut nm));
    assert_eq!(nm.text, st(__content.string(14205)));
    assert_eq!(dlg_proc_crypto_colors(&g, 310, true, 0, None), 1000);
    assert_eq!(dlg_proc_crypto_colors(&g, 310, false, 0, None), 0);
    assert_eq!(dlg_proc_crypto_colors(&g, 309, false, 1133, None), 1000);
    let mut nm = Nmhdr { hwnd_from: 1, code: TTN_NEEDTEXT, flags: 0, text: PbStr::new() };
    dlg_proc_crypto_colors(&g, 78, false, 0, Some(&mut nm));
    assert_eq!(nm.text, st(__content.string(14206)));
}
