//! Differential tests of `logic::restructure::tender`: LboTenderOffer (LBO tender offers and greenmail
//! buy-backs, computer and player modes) and NegotiatePremiumBuyout against the original.
#![allow(clippy::manual_is_multiple_of)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::restructure::tender::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

pub const LBO_TENDER_OFFER: u32 = 0x51f21c;
pub const NEGOTIATE_PREMIUM_BUYOUT: u32 = 0x528aef;

/// Scripted UI: message-box answers (`"IDYES"`/`"IDNO"`/... as for the harness) and INPUTBOX texts share
/// one queue, as in the harness (each kind takes the next answer).
#[derive(Default)]
pub struct Scr {
    pub answers: VecDeque<String>,
    pub msgs: Vec<String>,
    pub titles: Vec<String>,
    pub inputs: usize,
    pub news: Vec<String>,
    pub reports: usize,
}

impl Scr {
    pub fn new(answers: &[&str]) -> Self {
        Scr { answers: answers.iter().map(|a| a.to_string()).collect(), ..Default::default() }
    }
    pub fn any_msg(&self, part: &str) -> bool {
        self.msgs.iter().any(|m| m.contains(part))
    }
}

impl Ui for Scr {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        let yes_no = style & 0xf == 3 || style & 0xf == 4;
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage: no answer taken
        }
        match self.answers.pop_front().as_deref() {
            Some("IDNO") => 7,
            Some("IDCANCEL") => 2,
            Some("IDYES") => 6,
            Some("IDOK") => 1,
            Some(_) | None => {
                if yes_no {
                    6
                } else {
                    1
                }
            }
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.inputs += 1;
        PbStr::from(self.answers.pop_front().unwrap_or_default().as_str())
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports += 1;
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

pub fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x5bd1_e995
}

/// Message boxes and quick messages of the original, without ShowRuntimeError boxes.
pub fn orig_boxes(o: &Original) -> usize {
    o.log
        .iter()
        .filter(|l| (l.starts_with("MSGBOX") && !l.to_lowercase().contains("runtime error")) || l.contains("CreateProcessA(0x0, 'qm.exe"))
        .count()
}

/// Compares state, RND state and the number of message boxes of a port run with the original's.
#[track_caller]
pub fn check(port: &Globals, rng: &Rng, ui: &Scr, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| std::env::var("DBGALL").is_ok() || !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    for l in o.log.iter().filter(|l| l.starts_with("PBERR")) {
        eprintln!("{what}: {l}");
    }
    // DlgSymbolEntry: the window handle of the symbol-entry dialog (UI only)
    cmp().ignore("DlgSymbolEntry").rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_boxes(o), "{what}: message boxes {:?} vs {:?}", ui.msgs, o.log);
}

/// Counts outcomes (for the coverage assertions).
pub struct Hits(pub Mutex<BTreeMap<String, usize>>);

impl Hits {
    pub fn new() -> Self {
        Hits(Mutex::new(BTreeMap::new()))
    }
    pub fn hit(&self, k: &str) {
        *self.0.lock().unwrap().entry(k.to_string()).or_default() += 1;
    }
    #[track_caller]
    pub fn require(&self, keys: &[&str]) {
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

/// A random active operating corp (or, now and then, a bank or insurer).
pub fn pick_corp(g: &mut Globals, r: &mut Rand) -> i32 {
    let c = cast::<i32, _>(match r.range(0, 9) {
        0 => r.range(11, 85),
        1 => r.range(86, 130),
        _ => r.range(131, 1580),
    });
    g.entity_status[c] = 1;
    c
}

/// Gives `holder` a stake of `pct` in `corp` (in a free or random slot).
pub fn give_stake(g: &mut Globals, r: &mut Rand, holder: i32, corp: i32, pct: i16) -> i32 {
    for slot in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot, holder)]) == corp {
            g.hold_stock_pct[(slot, holder)] = pct;
            return slot;
        }
    }
    let slot = (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0).unwrap_or(cast::<i32, _>(r.range(1, 15)));
    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(slot, holder)] = pct;
    g.hold_stock_basis[(slot, holder)] = f64r(r, -Ext::from_f64(20.0), Ext::from_f64(500.0));
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, holder)] != 0).count());
    slot
}

/// A world for a buy-back of corp `c`: modest market cap (so that some buy-backs are affordable),
/// varied cash, debt and credit, sometimes a pending merger on it.
fn lbo_world(seed: u64) -> (Globals, i32, Rand) {
    let mut g = world(seed + 4000);
    let mut r = Rand(seed * 97 + 11);
    let c = pick_corp(&mut g, &mut r);
    g.market_cap[c] = match r.range(0, 3) {
        0 => f64r(&mut r, Ext::from_f64(0.01), Ext::from_f64(0.5)),
        1 => f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5.0)),
        2 => f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(40.0)),
        _ => f64r(&mut r, Ext::from_f64(40.0), Ext::from_f64(3000.0)),
    };
    g.shares_outstanding[c] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(50.0));
    g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
    g.t_bills[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) } else { Ext::from_f64(0.0) };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.35))) {
        // a poor company: no stock portfolio, little cash, debt
        for slot in 0..=15 {
            g.hold_stock_id[(slot, c)] = 0;
            g.hold_stock_pct[(slot, c)] = 0;
        }
        g.cash[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0));
        g.t_bills[c] = Ext::from_f64(0.0);
        g.business_assets[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
        g.bank_debt[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(400.0));
        g.market_cap[c] = f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(60.0));
        for h in 1..=1600 {
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == c {
                    g.hold_stock_id[(slot, h)] = 0;
                    g.hold_stock_pct[(slot, h)] = 0;
                }
            }
        }
    }
    g.bank_debt[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
    if c > 130 {
        g.business_assets[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(8000.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.merger_target = c;
        g.merger_acquirer = cast::<i32, _>(r.range(131, 1580));
        g.entity_status[g.merger_acquirer] = 1;
    }
    // a few holders of the stock
    for _ in 0..r.range(0, 4) {
        let h = match r.range(0, 3) {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 => cast::<i32, _>(r.range(1581, 1583)),
            _ => cast::<i32, _>(r.range(11, 1580)),
        };
        if h != c {
            let p = cast::<i16, _>(r.range(1, 40));
            give_stake(&mut g, &mut r, h, c, p);
        }
    }
    g.active_entity = c;
    (g, c, r)
}

/// LboTenderOffer in computer modes 1 and 2 (no dialogs): LBO tender offers and greenmail buy-backs.
#[test]
fn diff_lbo_tender_offer_ai() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=160, |seed| {
        let (mut g, c, mut r) = lbo_world(seed);
        let ai_mode = 1 + cast::<i32, _>(seed % 2);
        let seller = if seed % 3 == 0 {
            0
        } else {
            let s = match r.range(0, 4) {
                0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
                1 => cast::<i32, _>(r.range(1581, 1583)),
                _ => cast::<i32, _>(r.range(11, 1580)),
            };
            if s != c && !(seed % 17 == 1) {
                let p = [3, 10, 30, 60, 100][cast::<usize, _>(r.range(0, 4))];
                give_stake(&mut g, &mut r, s, c, p);
            }
            s
        };
        g.tmp_ext1 = cast::<Ext, _>(ai_mode);
        g.tmp_ext2 = cast::<Ext, _>([1, 3, 5, 10, 20, 40, 100][cast::<usize, _>(r.range(0, 6))]);
        let rng_start = rng0(seed);
        let Some(o) = run(LBO_TENDER_OFFER, &[&c.to_string(), &seller.to_string()], &g, rng_start, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scr::new(&[]);
        let mut rng = Rng::from_state(rng_start);
        lbo_tender_offer(&mut port, &mut ui, &mut rng, c, seller, &LocalTime::HARNESS);
        check(&port, &rng, &ui, &o, &format!("seed {seed} LBO({c}, {seller}) mode {ai_mode}"));
        let done = port.news_headline.as_slice().iter().any(|h| h.to_display().contains(__content.string(9865)));
        let kind = if seller > 0 { "gm" } else { "lbo" };
        if done {
            hits.hit(&format!("{kind}-done-{ai_mode}"));
        } else if port.accrued_pl[c] != g.accrued_pl[c] {
            hits.hit(&format!("{kind}-refused"));
        } else {
            hits.hit(&format!("{kind}-early"));
        }
        if port.bank_debt[c] > g.bank_debt[c] {
            hits.hit(__content.string(13624));
        }
    });
    hits.require(&["lbo-done-1", "lbo-done-2", "gm-done-1", "gm-done-2", "lbo-refused", "lbo-early", "gm-early", __content.string(13624)]);
}

/// LboTenderOffer for the player (mode 0) with its dialogs: the LBO decision, the premium negotiation
/// with a greenmail seller (MSGBOX + INPUTBOX), the pending-merger warning and the confirmations.
#[test]
fn diff_lbo_tender_offer_player() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=200, |seed| {
        let (mut g, c, mut r) = lbo_world(seed + 1000);
        let seller = if seed % 2 == 0 {
            0
        } else {
            let s = match r.range(0, 4) {
                0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
                1 => cast::<i32, _>(r.range(1581, 1583)),
                _ => cast::<i32, _>(r.range(11, 1580)),
            };
            if s != c {
                let p = [3, 10, 30, 60, 85, 100][cast::<usize, _>(r.range(0, 5))];
                give_stake(&mut g, &mut r, s, c, p);
            }
            s
        };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.last_premium_bidder = seller;
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        g.tmp_ext2 = cast::<Ext, _>([1, 5, 10, 20, 40, 100][cast::<usize, _>(r.range(0, 5))]);
        let first = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { "IDNO" } else { "IDYES" };
        let premium = ["0", "5%", "15", "25 %", "$40", "70", "99"][cast::<usize, _>(r.range(0, 6))];
        let warn = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "IDNO" } else { "IDYES" };
        let answers: Vec<&str> = if seller > 0 { vec![first, premium, warn, "IDOK"] } else { vec![first, warn, "IDOK"] };
        let rng_start = rng0(seed + 77);
        let Some(o) = run(LBO_TENDER_OFFER, &[&c.to_string(), &seller.to_string()], &g, rng_start, &answers, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scr::new(&answers);
        let mut rng = Rng::from_state(rng_start);
        lbo_tender_offer(&mut port, &mut ui, &mut rng, c, seller, &LocalTime::HARNESS);
        check(&port, &rng, &ui, &o, &format!("seed {seed} LBO({c}, {seller}{__wsr_text_14102}{answers:?}",__wsr_text_14102=__content.string(14102)));
        let kind = if seller > 0 { "gm" } else { "lbo" };
        for (key, part) in [
            ("min5", __content.string(14103)),
            (__content.string(1057), __content.string(14104)),
            (__content.string(2826), __content.string(14105)),
            ("net-worth", __content.string(14106)),
            ("credit", __content.string(14107)),
            ("merger-warning", __content.string(14108)),
            ("no-funds", __content.string(14109)),
            ("confirm", __content.string(14110)),
            ("merger-canceled", __content.string(14111)),
            (__content.string(14112), __content.string(14113)),
            (__content.string(10641), __content.string(14114)),
            ("not-serious", __content.string(14115)),
            (__content.string(14116), __content.string(14116)),
        ] {
            if ui.any_msg(part) {
                hits.hit(&format!("{kind}-{key}"));
            }
        }
    });
    hits.require(&[
        "lbo-min5",
        "lbo-declined",
        "lbo-regulators",
        "lbo-net-worth",
        "lbo-merger-warning",
        "lbo-confirm",
        "gm-min5",
        "gm-rejected",
        "gm-accepted",
        "gm-not-serious",
        "gm-confirm",
    ]);
}

/// NegotiatePremiumBuyout on its own: all premium bands, sellers (players, computer players, corps,
/// funds), repeated bidders, player/corporate buyers.
#[test]
fn diff_negotiate_premium_buyout() {
    let __content = wsr_core::content::current();
    let hits = Hits::new();
    par_seeds(1..=150, |seed| {
        let mut g = world(seed + 7000);
        let mut r = Rand(seed * 13 + 5);
        let c = pick_corp(&mut g, &mut r);
        let seller = match r.range(0, 4) {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 => cast::<i32, _>(r.range(1581, 1583)),
            _ => cast::<i32, _>(r.range(11, 1580)),
        };
        let seller_pct = [2, 10, 25, 55, 85, 100][cast::<usize, _>(r.range(0, 5))];
        if seller != c {
            give_stake(&mut g, &mut r, seller, c, seller_pct);
        }
        let buyer = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { c } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { cast::<i32, _>(r.range(11, 1580)) };
        let pct = cast::<i32, _>([1, 5, cast::<i64, _>(seller_pct), 30][cast::<usize, _>(r.range(0, 3))]);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.last_premium_bidder = seller;
        }
        g.active_entity = buyer.max(1);
        let premium = ["", "0", "3", "8%", "12", "18", "25", "33", "45", "65", "90"][cast::<usize, _>(r.range(0, 10))];
        let first = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { "IDNO" } else { "IDYES" };
        let answers = [first, premium];
        let rng_start = rng0(seed + 3);
        let args = [c.to_string(), seller.to_string(), buyer.to_string(), pct.to_string()];
        let args: Vec<&str> = args.iter().map(|a| a.as_str()).collect();
        let Some(o) = run(NEGOTIATE_PREMIUM_BUYOUT, &args, &g, rng_start, &answers, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Scr::new(&answers);
        let mut rng = Rng::from_state(rng_start);
        let v = negotiate_premium_buyout(&mut port, &mut ui, &mut rng, c, seller, buyer, pct, &LocalTime::HARNESS);
        let what = format!("seed {seed} Negotiate({c}, {seller}, {buyer}, {pct}) {answers:?}");
        check(&port, &rng, &ui, &o, &what);
        assert_eq!(v, cast::<i32, _>(o.return_value), "{what}: result");
        hits.hit(match v {
            0 => "none",
            5 => __content.string(14112),
            _ => __content.string(10641),
        });
        if ui.any_msg(__content.string(14116)) {
            hits.hit(__content.string(14116));
        }
        if ui.any_msg(__content.string(14115)) {
            hits.hit("not-serious");
        }
    });
    hits.require(&["none", __content.string(14112), __content.string(10641), __content.string(14116), "not-serious"]);
}
