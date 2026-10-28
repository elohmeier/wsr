//! Shared helpers of the commod_ai tests: a plausible world with commodity markets, futures and physical
//! records (on top of the sim_entity test world), and the comparison against the original.
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


#[allow(unused_imports)]
pub use super::diff::{Original, Rand, par_seeds};
#[allow(unused_imports)]
pub use super::sim_common::{Rec, cmp, f32r, f64r, orig_msgs, run, run_hooked, st};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

pub const COMMODS: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];
pub const PHYS: [&str; 7] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "BITCOIN", "ETHEREUM"];

/// RND start state for a seed.
pub fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x1b87_3593
}

/// Market data of the commodities: prices, targets, ranges, bases.
pub fn set_markets(g: &mut Globals, r: &mut Rand) {
    g.oil_price = cast::<f32, _>(f32r(r, Ext::from_f64(0.4), Ext::from_f64(2.2)));
    g.oil_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(0.4), Ext::from_f64(2.2)));
    g.metals_price_base = if r.chance(Ext::from_f64(0.5)) { 0 } else { cast::<i32, _>(r.range(0, 900)) };
    g.gold_price = cast::<f32, _>(f32r(r, Ext::from_f64(600.0), Ext::from_f64(3000.0)) + cast::<f32, _>(g.metals_price_base));
    g.gold_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(600.0), Ext::from_f64(3000.0)) + cast::<f32, _>(g.metals_price_base));
    g.silver_price = cast::<f32, _>(f32r(r, Ext::from_f64(5.0), Ext::from_f64(40.0)));
    g.silver_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(5.0), Ext::from_f64(40.0)));
    g.wheat_price = cast::<f32, _>(f32r(r, Ext::from_f64(1.5), Ext::from_f64(12.0)));
    g.wheat_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(1.5), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(r, Ext::from_f64(1.5), Ext::from_f64(10.0)));
    g.corn_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(1.5), Ext::from_f64(10.0)));
    g.bitcoin_price = cast::<i32, _>(r.range(5000, 260000));
    g.bitcoin_target_price = cast::<i32, _>(r.range(5000, 260000));
    g.ethereum_price = cast::<i32, _>(r.range(300, 8000));
    g.ethereum_target_price = cast::<i32, _>(r.range(300, 8000));
    g.stock_index = f64r(r, Ext::from_f64(3000.0), Ext::from_f64(30000.0));
    for k in 1..=10 {
        g.futures_basis[k] = cast::<i16, _>(r.range(-60, 260));
    }
    g.peak_oil_crisis = [0, 0, 0, 1, -1][cast::<usize, _>(r.range(0, 4))];
    g.oil_crisis_level = cast::<f32, _>(if r.chance(Ext::from_f64(0.3)) { f32r(r, Ext::from_f64(0.0), Ext::from_f64(3.0)) } else { 0.0f32 });
    g.pandemic_state = 0;
    g.ticker_speed = 60;
    g.credit_rating_str = st(["AAA", "AA", "A", "BBB", "BB"][cast::<usize, _>(r.range(0, 4))]);
    g.currency_code = st(if r.chance(Ext::from_f64(0.8)) { "USA" } else { "GBR" });
    g.currency_suffix = PbStr::new();
    g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(4.0), Ext::from_f64(7.0)));
    g.gdp_growth_target = cast::<f32, _>(f32r(r, -Ext::from_f64(4.0), Ext::from_f64(7.0)));
    g.base_interest_rate = cast::<f32, _>(f32r(r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
}

/// A random futures record of `owner`.
pub fn futures_rec(g: &Globals, r: &mut Rand, c: &str, owner: i32) -> PbStr {
    let qty = r.range(1, 80) * if r.chance(Ext::from_f64(0.4)) { -1 } else { 1 };
    let year = g.cur_year + cast::<i32, _>(r.range(0, 3));
    let cost = f64r(r, Ext::from_f64(1.0), Ext::from_f64(3000.0));
    st(&format!("{c}|{owner}|{qty}|{}|{year}|{cost:.4}|{:.2}", r.range(1, 12), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(999.0)))))
}

/// The commodity world: the sim_entity world plus markets, controlled players, futures and physical
/// records on a few `owners`.
pub fn cworld(seed: u64) -> (Globals, Vec<i32>) {
    let mut g = super::sim_common::world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    set_markets(&mut g, &mut r);
    g.cur_commodity = st(COMMODS[cast::<usize, _>(r.range(0, 7))]);
    for p in 1..=g.num_players {
        g.corp_controller[p] = p;
        g.entity_status[p] = 1;
    }
    for e in 1..=1600 {
        g.commod_margin_bal[e] = if r.chance(Ext::from_f64(0.1)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
        g.credit_rating_num[e] = cast::<i32, _>(r.range(4, 10));
        g.ctrl_since_year[e] = g.cur_year - cast::<i32, _>(r.range(0, 10));
    }
    let mut owners: Vec<i32> = Vec::new();
    for k in 0..10 {
        let o = match k % 5 {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 | 2 => cast::<i32, _>(r.range(131, 1580)),
            3 => cast::<i32, _>(r.range(11, 130)),
            _ => {
                // a corp controlled by player 1
                let c = cast::<i32, _>(r.range(131, 1580));
                g.corp_controller[c] = 1;
                c
            }
        };
        g.entity_status[o] = 1;
        owners.push(o);
    }
    let n = cast::<i32, _>(r.range(0, 40));
    for i in 1..=n {
        let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
        let o = owners[cast::<usize, _>(r.range(0, 9))];
        g.futures_recs[i] = futures_rec(&g, &mut r, c, o);
    }
    for i in n + 1..=1000 {
        g.futures_recs[i] = PbStr::new();
    }
    g.futures_recs[0] = st(&n.to_string());
    let n = cast::<i32, _>(r.range(0, 25));
    let mut k = 0;
    let mut used = std::collections::HashSet::new();
    for _ in 0..n {
        let c = PHYS[cast::<usize, _>(r.range(0, 6))];
        let o = owners[cast::<usize, _>(r.range(0, 9))];
        if !used.insert((c, o)) {
            continue; // one record per entity and commodity
        }
        k += 1;
        g.phys_commod_rec[k] = st(&format!("{c}|{o}|{}|{:.2}", r.range(1, 3000), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    for i in k + 1..=1000 {
        g.phys_commod_rec[i] = PbStr::new();
    }
    g.phys_commod_rec[0] = st(&k.to_string());
    (g, owners)
}

/// Compares state, RND state and the number of message boxes / pop-ups of a port run with the original's.
#[track_caller]
pub fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs {:?}", ui.msgs, o.log);
}

/// Counts of booleans (branch coverage asserts).
#[derive(Default)]
pub struct Hits(pub std::sync::Mutex<std::collections::BTreeMap<String, usize>>);

impl Hits {
    pub fn hit(&self, k: &str) {
        *self.0.lock().unwrap().entry(k.to_string()).or_default() += 1;
    }
    pub fn get(&self, k: &str) -> usize {
        self.0.lock().unwrap().get(k).copied().unwrap_or(0)
    }
    #[track_caller]
    pub fn require(&self, keys: &[&str]) {
        let m = self.0.lock().unwrap();
        for k in keys {
            assert!(m.get(*k).copied().unwrap_or(0) > 0, "branch {k} not hit: {m:?}");
        }
    }
}

/// Like [`Rec`] but with one script of answers shared by MSGBOX and INPUTBOX$ in call order, as the
/// harness does (`IDYES`/`IDNO`/`IDOK`/`IDCANCEL` or a number for a MSGBOX, the text for an INPUTBOX$;
/// ShowQuickMessage pop-ups take none).
#[derive(Default)]
pub struct Io {
    pub rec: Rec,
    pub answers: std::collections::VecDeque<String>,
}

impl Io {
    pub fn new(answers: &[&str]) -> Self {
        Io { rec: Rec::default(), answers: answers.iter().map(|a| a.to_string()).collect() }
    }
}

impl wsr_core::ui::Ui for Io {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        let quick = style == 0 && title.is_empty();
        let rv = self.rec.msgbox(text, style, title);
        if quick {
            return rv;
        }
        match self.answers.pop_front() {
            Some(a) => match a.as_str() {
                "IDOK" => 1,
                "IDCANCEL" => 2,
                "IDYES" => 6,
                "IDNO" => 7,
                n => n.parse().unwrap(),
            },
            None => rv,
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        st(&self.answers.pop_front().unwrap_or_default())
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> wsr_core::ui::ReportAction {
        self.rec.report(title, lines, yes_no)
    }
    fn news(&mut self, headline: &PbStr) {
        self.rec.news(headline)
    }
}
