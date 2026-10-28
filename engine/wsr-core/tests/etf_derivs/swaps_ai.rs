//! Differential tests of `logic::etf_derivs::swaps_ai`: CalcSwapFixedRate, AiCreateSwaps and
//! SwapRateShockEvent. `TIME$` is fixed by the harness (10:00:00), so the ports get `time_secs = 0`.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use wsr_core::logic::etf_derivs::swaps_ai::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const CALC_SWAP_FIXED_RATE: u32 = 0x6415dd;
const AI_CREATE_SWAPS: u32 = 0x646a2d;
const SWAP_RATE_SHOCK: u32 = 0x647de7;

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?}", ui.msgs);
}

/// A random swap record `long|short|rate|type|start|end|notional`.
fn swap_rec(g: &Globals, r: &mut Rand, long: i32, short: i32, notional: i64) -> PbStr {
    let start = cast::<i64, _>(g.cur_year) + r.range(0, 2);
    let sq = r.range(1, 4);
    let end = start + r.range(0, 6);
    let eq = r.range(1, 4);
    let ty = ["P", "L", "S"][cast::<usize, _>(r.range(0, 2))];
    st(&format!("{long}|{short}|{:.2}|{ty}|{start}.{sq}|{end}.{eq}|{notional}", f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(12.0)))))
}

/// [`world`] with a swap market: rates, monetary policy, rating-7+ banks/insurers and some swaps.
fn swap_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed * 977 + 13);
    g.prime_rate = cast::<f32, _>([f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)), f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(6.0)), f32r(&mut r, Ext::from_f64(9.0), Ext::from_f64(16.0))][cast::<usize, _>(r.range(0, 2))]);
    g.base_interest_rate = cast::<f32, _>(if r.chance(Ext::from_f64(0.5)) { f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(6.5)) } else { f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(18.0)) });
    g.interest_rate_target = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(1.0), Ext::from_f64(1.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(14.0)));
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(12.0)));
    g.central_bank_name = st("the Fed");
    g.active_entity = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { cast::<i32, _>(r.range(11, 1600)) };
    for e in 11..=130 {
        if r.chance(Ext::from_f64(0.6)) {
            g.credit_rating_num[e] = cast::<i32, _>(r.range(7, 10));
        }
        if r.chance(Ext::from_f64(0.5)) {
            g.corp_controller[e] = cast::<i32, _>(r.range(11, 1580));
        }
    }
    for e in 131..=1580 {
        if r.chance(Ext::from_f64(0.05)) {
            g.industry_num[e] = 37;
        }
    }
    let n = cast::<i32, _>(r.range(0, 60));
    for i in 1..=n {
        let a = cast::<i32, _>(r.range(11, 1600));
        let b = cast::<i32, _>(r.range(11, 1600));
        let notional = r.range(1000, 100_000);
        g.swap_contracts[i] = swap_rec(&g, &mut r, a, b, notional);
    }
    g.swap_contracts[0] = st(&n.to_string());
    g
}

#[test]
fn diff_calc_swap_fixed_rate() {
    let __content = wsr_core::content::current();
    let modes = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];
    let crowd = AtomicUsize::new(0);
    par_seeds(1..=120, |seed| {
        let mut g = swap_world(seed);
        let mut r = Rand(seed * 7 + 1);
        g.swap_contracts[0] = st(&r.range(0, 130).to_string());
        if r.chance(Ext::from_f64(0.3)) {
            g.active_entity = cast::<i32, _>(r.range(11, 130));
        }
        let long = if r.chance(Ext::from_f64(0.5)) { g.active_entity } else { cast::<i32, _>(r.range(11, 1600)) };
        let short = if long == g.active_entity { cast::<i32, _>(r.range(11, 130)) } else { g.active_entity };
        let notional = r.range(100, 100_000);
        let rec = swap_rec(&g, &mut r, long, short, notional);
        let entity = if r.chance(Ext::from_f64(0.8)) { g.active_entity } else { long };
        let rs = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let mut rng = Rng::from_state(rs);
        let v = calc_swap_fixed_rate(&g, &mut rng, &rec, entity);
        let Some(o) = run(CALC_SWAP_FIXED_RATE, &[&format!("s:{}", rec.to_display()), &entity.to_string()], &g, rs, &[], &[])
        else {
            return;
        };
        let what = format!("seed {seed} rec {}{__wsr_text_13806}{entity}", rec.to_display(),__wsr_text_13806=__content.string(13806));
        check(&g, &rng, &Rec::default(), &o, &what);
        assert_eq!(Some(v.clone()), o.return_str, "{what}: result");
        let side_s = short == entity;
        let p = cast::<Ext, _>(g.prime_rate);
        let m = if !side_s && p < Ext::from_f64(6.0) { 1 } else if side_s && p > Ext::from_f64(9.0) { 2 } else { 0 };
        modes[m].fetch_add(1, Relaxed);
        if rng.state() != rs {
            crowd.fetch_add(1, Relaxed);
        }
    });
    let m: Vec<usize> = modes.iter().map(|x| x.load(Relaxed)).collect();
    eprintln!("COV modes {m:?} crowd {}", crowd.load(Relaxed));
    if unavailable().is_none() {
        assert!(m.iter().all(|&x| x > 5) && crowd.load(Relaxed) > 3, "{__wsr_text_13574}{m:?}",__wsr_text_13574=__content.string(13574));
    }
}

/// An RND state whose first draw is below 0.1 (AiCreateSwaps runs in 10% of the calls).
fn lucky_state(r: &mut Rand) -> u32 {
    loop {
        let s = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        if Rng::from_state(s).rnd() * Ext::from_f64(10.0) <= Ext::from_f64(1.0) {
            return s;
        }
    }
}

#[test]
fn diff_ai_create_swaps() {
    let __content = wsr_core::content::current();
    let created = AtomicUsize::new(0);
    let shorts = AtomicUsize::new(0);
    par_seeds(1..=60, |seed| {
        let mut g = swap_world(seed + 300);
        let mut r = Rand(seed * 13 + 5);
        // a computer company without swaps, rating 7+
        let corp = loop {
            let e = cast::<i32, _>(r.range(131, 1580));
            if g.entity_status[e] == 1 && g.industry_num[e] != 37 {
                break e;
            }
        };
        g.sim_co_count = if r.chance(Ext::from_f64(0.05)) { cast::<i32, _>(r.range(1581, 1590)) } else { corp };
        if r.chance(Ext::from_f64(0.9)) {
            g.corp_controller[corp] = cast::<i32, _>(r.range(11, 1580));
            g.credit_rating_num[corp] = cast::<i32, _>(r.range(7, 10));
            g.cash[corp] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(20000.0));
        }
        if r.chance(Ext::from_f64(0.8)) {
            for i in 1..=60 {
                let rec = g.swap_contracts[i].to_display();
                if rec.starts_with(&format!("{corp}|")) || rec.contains(&format!("|{corp}|")) {
                    g.swap_contracts[i] = PbStr::new();
                }
            }
        }
        if r.chance(Ext::from_f64(0.8)) {
            g.base_interest_rate = cast::<f32, _>(if r.chance(Ext::from_f64(0.5)) { f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(6.4)) } else { f32r(&mut r, Ext::from_f64(9.1), Ext::from_f64(16.0)) });
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
        }
        let rs = if r.chance(Ext::from_f64(0.9)) { lucky_state(&mut r) } else { cast::<u32, _>(r.range(1, 0x7fff_ffff)) };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ai_create_swaps(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run(AI_CREATE_SWAPS, &[], &g, rs, &[], &[]) else {
            return;
        };
        let what = format!("seed {seed}");
        check(&port, &rng, &ui, &o, &what);
        if port.swap_contracts[0] != g.swap_contracts[0] {
            created.fetch_add(1, Relaxed);
            let rec = (1..=1000).map(|i| port.swap_contracts[i].to_display()).find(|x| x.contains(&format!("|{}|", g.sim_co_count)) && !x.starts_with(&format!("{}|", g.sim_co_count)));
            if rec.is_some() {
                shorts.fetch_add(1, Relaxed);
            }
        }
    });
    let (c, s) = (created.load(Relaxed), shorts.load(Relaxed));
    eprintln!("COV created {c}{__wsr_text_13807}{s}",__wsr_text_13807=__content.string(13807));
    if unavailable().is_none() {
        assert!(c >= 8 && s >= 2 && c - s >= 2, "{__wsr_text_13808}{c}{__wsr_text_13807}{s}",__wsr_text_13808=__content.string(13808), __wsr_text_13807=__content.string(13807));
    }
}

#[test]
fn diff_swap_rate_shock_event() {
    let __content = wsr_core::content::current();
    let scheduled = AtomicUsize::new(0);
    let fired = [AtomicUsize::new(0), AtomicUsize::new(0)];
    par_seeds(1..=60, |seed| {
        let mut g = swap_world(seed + 600);
        let mut r = Rand(seed * 17 + 3);
        g.sel_entity = 0;
        g.is_computer_player[1] = 0;
        let fire = seed % 2 == 0;
        if fire {
            g.fed_rate_shock_tick = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(1420, 1570)) } else { -1550 };
            // (CurYear + CurQuarter) MOD 4 >= 2
            while (g.cur_year + g.cur_quarter) % 4 < 2 {
                g.cur_quarter = g.cur_quarter % 4 + 1;
            }
            if r.chance(Ext::from_f64(0.2)) {
                g.cur_quarter = 4;
                while (g.cur_year + g.cur_quarter) % 4 < 2 {
                    g.cur_year += 1;
                }
            }
            g.monetary_policy = cast::<i16, _>(r.range(0, 2));
            g.qe_rumor_count = [0, 9, 3][cast::<usize, _>(r.range(0, 2))];
            g.rate_soar_news_count = [0, 7, 3][cast::<usize, _>(r.range(0, 2))];
            if r.chance(Ext::from_f64(0.3)) {
                g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), -Ext::from_f64(5.0)));
                g.interest_rate_target = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(12.0)));
            }
        } else {
            g.fed_rate_shock_tick = if r.chance(Ext::from_f64(0.8)) { 0 } else { -1550 };
            // 50+ swaps, some held by player 1 or companies he controls
            let n = cast::<i32, _>(r.range(50, 70));
            for i in 1..=n {
                let mut a = cast::<i32, _>(r.range(11, 1600));
                let mut b = cast::<i32, _>(r.range(11, 1600));
                let big = r.range(1_000_000, 30_000_000);
                if r.chance(Ext::from_f64(0.4)) {
                    if seed % 4 == 1 {
                        a = if r.chance(Ext::from_f64(0.5)) { 1 } else { a };
                        g.corp_controller[a] = 1;
                    } else {
                        b = if r.chance(Ext::from_f64(0.5)) { 1 } else { b };
                        g.corp_controller[b] = 1;
                    }
                }
                g.swap_contracts[i] = swap_rec(&g, &mut r, a, b, big);
            }
            if r.chance(Ext::from_f64(0.2)) {
                g.swap_contracts[cast::<i32, _>(r.range(1, cast::<i64, _>(n)))] = PbStr::new();
            }
            g.swap_contracts[0] = st(&n.to_string());
        }
        let rs = cast::<u32, _>(r.range(1, 0x7fff_ffff));
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        swap_rate_shock_event(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run(SWAP_RATE_SHOCK, &[], &g, rs, &[], &[]) else {
            return;
        };
        let what = format!("seed {seed}");
        check(&port, &rng, &ui, &o, &what);
        if !fire && port.fed_rate_shock_tick != g.fed_rate_shock_tick {
            scheduled.fetch_add(1, Relaxed);
        }
        if fire && port.base_interest_rate != g.base_interest_rate {
            fired[cast::<usize, _>(g.fed_rate_shock_tick > 0)].fetch_add(1, Relaxed);
        }
    });
    let (s, f0, f1) = (scheduled.load(Relaxed), fired[0].load(Relaxed), fired[1].load(Relaxed));
    eprintln!("COV scheduled {s} fired QE {f0}{__wsr_text_13809}{f1}",__wsr_text_13809=__content.string(13809));
    if unavailable().is_none() {
        assert!(s >= 3 && f0 >= 3 && f1 >= 3, "{__wsr_text_13810}{s} QE {f0}{__wsr_text_13809}{f1}",__wsr_text_13810=__content.string(13810), __wsr_text_13809=__content.string(13809));
    }
}
