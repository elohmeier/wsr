//! Differential tests of `logic::events::random_news`: RandomCorpNewsEvent (0x4a5a08) and
//! AdjustProfitMarginWithMisc (0x4aee22) against the original routines.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::BTreeSet;
use std::sync::Mutex;
use wsr_core::logic::events::random_news::*;
use wsr_core::logic::finance::calc_entity_financials;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::to_long;
use wsr_core::state::Globals;

const RANDOM_CORP_NEWS_EVENT: u32 = 0x4a5a08;
const ADJUST_PROFIT_MARGIN: u32 = 0x4aee22;

/// Differences of the port vs. the original (globals, RND state, runtime errors, message boxes).
fn mismatch(port: &Globals, port_rng: &Rng, ui: &Rec, o: &Original) -> Option<String> {
    let d = cmp().diffs(port, &o.globals);
    let log: Vec<&String> = o.log.iter().filter(|l| !l.starts_with("API")).take(20).collect();
    if !d.is_empty() {
        return Some(format!(
            "{} differences:\n{}\nlog: {log:?}",
            d.len(),
            d.iter().take(25).cloned().collect::<Vec<_>>().join("\n")
        ));
    }
    if port_rng.state() != o.rng {
        return Some(format!("RND state {} vs {}", port_rng.state(), o.rng));
    }
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    if !errs.is_empty() {
        return Some(format!("runtime errors in the original: {errs:?}"));
    }
    if ui.msgs.len() != orig_msgs(o) {
        return Some(format!("message boxes {:?} vs log {log:?}", ui.msgs));
    }
    None
}

// ------------------------------------------------------------------------ AdjustProfitMarginWithMisc

#[test]
fn diff_adjust_profit_margin_with_misc() {
    let __content = wsr_core::content::current();
    let fails = Mutex::new(Vec::new());
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 7919 + 3);
        let mut g = world(seed + 2000);
        // corp ids <= 10 and a zero delta crash the original (bare RETURN, see BUGS.md #130)
        let corp = match seed % 6 {
            0 | 1 => cast::<i32, _>(r.range(11, 130)),
            _ => cast::<i32, _>(r.range(131, 1600)),
        };
        let delta = [3i16, 7, -7, 11, -10, 9, 5, -9][cast::<usize, _>(r.range(0, 7))];
        if seed % 5 == 4 {
            // record without the trailing '|' / with fewer fields
            let n = if seed % 2 == 0 { 31 } else { 25 };
            let f: Vec<String> = (1..=n).map(|k| format!("{}", cast::<Ext, _>(k) * Ext::from_f64(1.5))).collect();
            g.ent_misc_data[corp] = st(&f.join("|"));
        }
        let args = [corp.to_string(), format!("{__wsr_text_13653}{delta}",__wsr_text_13653=__content.string(13653))];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(ADJUST_PROFIT_MARGIN, &args, &g, 1, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        adjust_profit_margin_with_misc(&mut port, corp, delta);
        let rng = Rng::from_state(1);
        if let Some(m) = mismatch(&port, &rng, &Rec::default(), &o) {
            fails.lock().unwrap().push(format!("seed {seed} corp {corp} delta {delta}: {m}"));
        }
    });
    let f = fails.into_inner().unwrap();
    assert!(f.is_empty(), "{}{__wsr_text_13838}{}", f.len(), f.join("\n\n"),__wsr_text_13838=__content.string(13838));
}

// ------------------------------------------------------------------------------- RandomCorpNewsEvent

/// What a seed should exercise: an event-number range and, for the industry events, an industry.
#[derive(Clone, Copy, Debug)]
struct Target {
    lo: i32,
    hi: i32,
    industry: Option<i16>,
}

/// The event number (`RND * 2000`) and corp the routine would draw from RND state `state`, if it gets
/// that far (cooldown 0).
fn pre_roll(g: &Globals, state: u32) -> Option<(i32, i32)> {
    let mut r = Rng::from_state(state);
    if r.rnd() * (cast::<Ext, _>(g.difficulty_level)) < Ext::from_f64(0.5) {
        return None;
    }
    let corp = r.rnd_range(11, 1600);
    if g.entity_status[corp] != 1 || g.industry_num[corp] == 71 {
        return None;
    }
    let mut r2 = r;
    Some((corp, to_long(r2.rnd() * Ext::from_f64(2000.0))))
}

/// Exact event number (runs CalcEntityFinancials on a copy, which may draw RND).
fn exact_roll(g: &Globals, state: u32, corp: i32) -> i32 {
    let mut r = Rng::from_state(state);
    r.rnd();
    r.rnd_range(11, 1600);
    let mut c = g.clone();
    calc_entity_financials(&mut c, &mut Rec::default(), &mut r, corp);
    to_long(r.rnd() * Ext::from_f64(2000.0))
}

/// Finds an RND state (and adjusts the corp's industry) so that the event of `t` happens.
fn find_state(g: &mut Globals, r: &mut Rand, t: Target) -> Option<(u32, i32, i32)> {
    for _ in 0..400_000 {
        let s = cast::<u32, _>(r.next());
        let Some((corp, roll0)) = pre_roll(g, s) else { continue };
        if roll0 < t.lo - 1 || roll0 > t.hi + 1 {
            continue;
        }
        if let Some(ind) = t.industry {
            match ind {
                1 if !(11..=85).contains(&corp) => continue,
                2 if !(86..=130).contains(&corp) => continue,
                1 | 2 => {}
                _ => {
                    if !(131..=1580).contains(&corp) {
                        continue;
                    }
                    g.industry_num[corp] = ind;
                }
            }
        }
        let roll = exact_roll(g, s, corp);
        if roll >= t.lo && roll <= t.hi {
            return Some((s, corp, roll));
        }
    }
    None
}

/// Branch-favouring adjustments of the world (random per seed).
fn tweak(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    g.rand_corp_news_cooldown = if r.chance(Ext::from_f64(0.1)) { 1 } else { 0 };
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.antitrust_defendant = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(r.range(131, 1580)) } else { 0 };
    g.sec_suit_count = cast::<i32, _>(r.range(0, 2));
    g.consumer_suit_count = cast::<i32, _>(r.range(0, 3));
    g.price_fix_suit_count = cast::<i32, _>(r.range(0, 2));
    g.bitcoin_ban_news_done = if r.chance(Ext::from_f64(0.2)) { 1 } else { 0 };
    g.bitcoin_price = cast::<i32, _>(r.range(1000, 90000));
    g.bitcoin_target_price = cast::<i32, _>(r.range(1000, 90000));
    g.layoff_news_count = cast::<i32, _>(r.range(0, 4));
    g.euro_crisis_news_cooldown = cast::<i32, _>(r.range(0, 2));
    g.plant_disaster_count = cast::<i32, _>(r.range(0, 4));
    g.nigeria_oil_news_cooldown = [0, 0, 1, 2, 3, 49, -1][cast::<usize, _>(r.range(0, 6))];
    g.peak_oil_crisis = if r.chance(Ext::from_f64(0.2)) { 1 } else { 0 };
    g.oil_price = cast::<f32, _>(f32r(r, Ext::from_f64(0.5), Ext::from_f64(2.0)));
    g.econ_news_quarter = if r.chance(Ext::from_f64(0.2)) { g.cur_quarter } else { 0 };
    g.crop_news_quarter = if r.chance(Ext::from_f64(0.2)) { g.cur_quarter } else { 0 };
    g.corn_price = cast::<f32, _>([f32r(r, Ext::from_f64(2.0), Ext::from_f64(2.8)), f32r(r, Ext::from_f64(3.0), Ext::from_f64(3.4)), f32r(r, Ext::from_f64(3.6), Ext::from_f64(9.0)), f32r(r, Ext::from_f64(10.3), Ext::from_f64(12.0))][cast::<usize, _>(r.range(0, 3))]);
    g.wheat_price = cast::<f32, _>([f32r(r, Ext::from_f64(2.0), Ext::from_f64(3.2)), f32r(r, Ext::from_f64(3.4), Ext::from_f64(3.9)), f32r(r, Ext::from_f64(4.1), Ext::from_f64(12.0))][cast::<usize, _>(r.range(0, 2))]);
    g.corn_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.wheat_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.oil_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(0.5), Ext::from_f64(2.0)));
    g.gold_target_price = cast::<f32, _>(f32r(r, Ext::from_f64(300.0), Ext::from_f64(2500.0)));
    g.currency_code = st(if r.chance(Ext::from_f64(0.5)) { "USA" } else { "GBR" });
    g.currency_suffix = st(if r.chance(Ext::from_f64(0.3)) { " Euro" } else { "" });
    g.gold_strike_stage = cast::<i32, _>(r.range(0, 3));
    g.ind_growth_rate[50] = cast::<i16, _>(r.range(0, 20));
    g.stabilize_news_count = cast::<i32, _>(r.range(0, 2));
    g.deposit_seize_news_count = cast::<i32, _>(r.range(0, 2));
    g.superfund_corp = if r.chance(Ext::from_f64(0.6)) { 0 } else { cast::<i16, _>(r.range(131, 1580)) };
    g.strike_corp_code = if r.chance(Ext::from_f64(0.6)) { 0 } else { 20500 };
    g.last_fraud_corp = 0;
    g.insider_earn_tip_corp = 0;
    g.new_mgmt_list = st(["", "0123/", "0123/0456/"][cast::<usize, _>(r.range(0, 2))]);
    if r.chance(Ext::from_f64(0.3)) {
        g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(12.0), -Ext::from_f64(6.5)));
    }
    if r.chance(Ext::from_f64(0.3)) {
        g.news_msg = st(__content.string(13571));
    }
    for e in 131..=1580 {
        if r.chance(Ext::from_f64(0.5)) {
            g.profit_margin[e] = cast::<i16, _>(r.range(-8, 5));
            g.earnings_y1[e] = -f64r(r, Ext::from_f64(1.0), Ext::from_f64(50.0));
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.corp_controller[e] = cast::<i32, _>(r.range(11, 1600));
        }
        if r.chance(Ext::from_f64(0.2)) {
            g.country_idx[e] = [0, 2, 6, 8, 38, 42][cast::<usize, _>(r.range(0, 5))];
        }
        if r.chance(Ext::from_f64(0.1)) {
            g.ent_name[e] = st(&format!("{} TANKER COPPER {e}", word(r, 4).to_display()));
        }
    }
    // trends of the price series used (bond yields, GDP, commodities, stock index)
    for series in [0i32, 6, 7, 8, 9, 10, 1602, 1603, 1604] {
        if r.chance(Ext::from_f64(0.6)) {
            let up = r.chance(Ext::from_f64(0.5));
            let cur = cast::<i32, _>(g.cap_hist_close[(0, series)]);
            let mut v = g.cap_hist_close[(cur, series)];
            for k in 0..14 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                g.cap_hist_close[(slot, series)] = v;
                v *= if up { Ext::from_f64(0.9) } else { Ext::from_f64(1.1) };
            }
        }
    }
}

/// The targets: every event-number range, and each industry for the industry events.
fn targets() -> Vec<Target> {
    let mut t = Vec::new();
    let ranges = [
        (0, 4),
        (5, 9),
        (5, 9),
        (10, 10),
        (11, 14),
        (11, 14),
        (15, 15),
        (60, 61),
        (60, 61),
        (62, 63),
        (62, 63),
        (64, 67),
        (64, 67),
        (68, 74),
        (75, 79),
        (75, 79),
        (80, 81),
        (82, 83),
        (84, 85),
        (86, 87),
        (88, 89),
        (90, 99),
        (90, 99),
        (90, 99),
        (100, 104),
        (105, 107),
        (108, 109),
        (110, 114),
        (115, 129),
        (115, 129),
        (130, 1999),
    ];
    for (lo, hi) in ranges {
        t.push(Target { lo, hi, industry: None });
    }
    for ind in (1..=70).chain([9, 50, 50, 7, 7, 4, 12, 58, 59, 23, 26, 52]) {
        t.push(Target { lo: 16, hi: 49, industry: Some(ind) });
    }
    for ind in [5, 7, 50, 8, 10, 14, 30, 61, 62, 63, 64, 4, 20, 21, 35, 51, 3, 40] {
        t.push(Target { lo: 50, hi: 59, industry: Some(ind) });
    }
    for ind in [4, 26, 27, 29, 10, 3] {
        t.push(Target { lo: 100, hi: 104, industry: Some(ind) });
    }
    t
}

/// Runs one seed per target (`tg[seed % len]`); returns the event ranges (`lo`) whose run posted news.
fn run_news_event_seeds(tg: &[Target], seeds: std::ops::RangeInclusive<u64>, base: u64) -> BTreeSet<i32> {
    let __content = wsr_core::content::current();
    let fails = Mutex::new(Vec::new());
    let hit = Mutex::new(BTreeSet::new());
    let posted = Mutex::new(BTreeSet::new());
    par_seeds(seeds, |seed| {
        let t = tg[(cast::<usize, _>(seed)) % tg.len()];
        let mut r = Rand(seed * 104_729 + base);
        let mut g = world(seed + base);
        tweak(&mut g, &mut r);
        let Some((rs, corp, roll)) = find_state(&mut g, &mut r, t) else {
            fails.lock().unwrap().push(format!("seed {seed}: no RND state for {t:?}"));
            return;
        };
        let Some(o) = run(RANDOM_CORP_NEWS_EVENT, &[], &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        random_corp_news_event(&mut port, &mut ui, &mut rng, 0);
        let what = format!(
            "seed {seed}{__wsr_text_13552}{t:?} roll {roll} corp {corp} ind {}{__wsr_text_13611}{:?}",
            g.industry_num[corp], ui.news
        ,__wsr_text_13552=__content.string(13552), __wsr_text_13611=__content.string(13611));
        if let Some(m) = mismatch(&port, &rng, &ui, &o) {
            fails.lock().unwrap().push(format!("{what}: {m}"));
        } else {
            hit.lock().unwrap().insert((t.lo, t.industry));
            if !ui.news.is_empty() {
                posted.lock().unwrap().insert(t.lo);
            }
        }
    });
    let f = fails.into_inner().unwrap();
    assert!(f.is_empty(), "{}{__wsr_text_13838}{}", f.len(), f.join("\n\n"),__wsr_text_13838=__content.string(13838));
    let hit = hit.into_inner().unwrap();
    if unavailable().is_none() {
        for t in tg {
            assert!(hit.contains(&(t.lo, t.industry)), "{__wsr_text_13839}{t:?} never exercised",__wsr_text_13839=__content.string(13839));
        }
    }
    posted.into_inner().unwrap()
}

#[test]
fn diff_random_corp_news_event() {
    let tg = targets();
    run_news_event_seeds(&tg, 1..=cast::<u64, _>(tg.len()), 0);
}

#[test]
fn diff_random_corp_news_event_more() {
    let tg = targets();
    let n = cast::<u64, _>(tg.len());
    run_news_event_seeds(&tg, n + 1..=2 * n, 77_000);
}

/// Only the event-number ranges, 6 seeds each; every range that can post news must have done so.
#[test]
fn diff_random_corp_news_event_ranges() {
    let tg: Vec<Target> = targets().into_iter().filter(|t| t.industry.is_none()).collect();
    let posted = run_news_event_seeds(&tg, 1..=6 * cast::<u64, _>(tg.len()), 31_000);
    if unavailable().is_none() {
        for lo in [0, 5, 10, 11, 15, 60, 62, 64, 68, 75, 80, 82, 84, 86, 88, 90, 100, 105, 108, 110, 115] {
            assert!(posted.contains(&lo), "event range from {lo} never posted news");
        }
    }
}
