//! Tests of logic::economy::quarterly: UpdateEconomy_52978d (0x52978d) with its GOSUB
//! UpdateEconomy_IndGrowthForecasts (0x52cb42), UpdateIndustryLongTermGrowth (0x4a56ec),
//! AdjustIndLongTermGrowth (0x4f8f90).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use std::cell::RefCell;
use wsr_core::logic::economy::{
    adjust_ind_long_term_growth, update_economy_52978d, update_economy_52978d_with, update_industry_long_term_growth,
};

const RANDOM_WORLD_EVENTS: (u32, u32) = (0x52d793, 0);

/// Routines of other economy submodules, skipped in the original until their ports are merged.
const PEER_SKIPS: &[(u32, u32)] = &[];

/// The shared world plus the industry / macro fields the quarterly step reads.
fn econ_world(seed: u64) -> Globals {
    let mut g = world(seed);
    pow_safe(&mut g, seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491) ^ 0x51);
    for ind in 1..=71 {
        g.ind_volatility[ind] = cast::<i16, _>(r.range(-2, 12));
        g.ind_growth_rate_now[ind] = cast::<i16, _>(r.range(-10, 30));
        g.ind_long_term_growth[ind] = cast::<i16, _>(r.range(-8, 30));
        g.ind_growth_forecast[ind] = cast::<i16, _>(r.range(-40, 70));
        g.ind_growth_rate[ind] = cast::<i16, _>(r.range(-40, 70));
    }
    g.gdp_level = r.range(10_000, 200_000);
    g.market_valuation = cast::<f32, _>([Ext::from_f64(0.0), Ext::from_f64(0.7), Ext::from_f64(0.8), Ext::from_f64(1.0), Ext::from_f64(1.3)][cast::<usize, _>(r.range(0, 4))]);
    g.market_val_target = cast::<f64, _>([Ext::from_f64(0.0), Ext::from_f64(0.7), Ext::from_f64(0.95), Ext::from_f64(1.2)][cast::<usize, _>(r.range(0, 3))]);
    g.pandemic_state = if r.chance(Ext::from_f64(0.1)) { [-2, 1, 3][cast::<usize, _>(r.range(0, 2))] } else { cast::<i16, _>(r.range(-1, 0)) };
    g.oil_crisis_level = cast::<f32, _>(if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(1.0) } else { Ext::from_f64(0.0) });
    g.subprime_crisis_state = cast::<i16, _>(r.range(-2, 1));
    g.monetary_stance = cast::<i32, _>(r.range(-1, 1));
    g.prime_rate = cast::<f32, _>(if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(3.0) } else { Ext::from(g.prime_rate) });
    g.bubble_state = [0, 0, 50, 101, 110, 150, 205, 215][cast::<usize, _>(r.range(0, 7))];
    g.tmp_share_price = Ext::from_f64(0.0);
    g.housing_starts = cast::<i32, _>(r.range(300, 1500));
    if r.chance(Ext::from_f64(0.3)) {
        g.cur_year = g.start_year + cast::<i32, _>(r.range(0, 1));
    }
    // Few active entities: the quarterly step re-values every one (slow under emulation).
    for e in 11..=1600 {
        if g.entity_status[e] == 1 && !r.chance(Ext::from_f64(0.01)) {
            g.entity_status[e] = 0;
        }
    }
    g
}

fn opts(rng: u32, skips: Vec<(u32, u32)>) -> Opts<'static> {
    Opts { rng: Some(rng), skips, ..Opts::default() }
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn update_industry_long_term_growth_unit() {
    let mut g = Globals::default();
    g.sim_co_count = 999;
    g.ind_growth_rate_now[5] = 10;
    g.ind_long_term_growth[5] = 5;
    g.ind_growth_rate_now[6] = 0;
    g.ind_long_term_growth[6] = 5;
    g.ind_long_term_growth[15] = -4;
    let mut rng = Rng::from_state(1);
    update_industry_long_term_growth(&mut g, &mut rng);
    assert_eq!(g.ind_long_term_growth[5], 6);
    assert_eq!(g.ind_long_term_growth[6], 4);
    assert_eq!(g.ind_long_term_growth[15], 2);
    assert_eq!(rng.state(), 1); // no RND outside Q4 of old games
    let mut g2 = Globals::default();
    g2.sim_co_count = 450;
    g2.ind_growth_rate_now[5] = 10;
    let before = g2.ind_long_term_growth[5];
    update_industry_long_term_growth(&mut g2, &mut rng);
    assert_eq!(g2.ind_long_term_growth[5], before);
}

#[test]
fn adjust_ind_long_term_growth_unit() {
    let mut g = Globals::default();
    g.start_year = 2000;
    g.cur_year = 2011; // 11 MOD 5 = 1
    g.ind_long_term_growth[3] = 40; // > RND*7+8: x0.8
    g.ind_long_term_growth[4] = -10; // < -5: x0.7 + 1
    let mut rng = Rng::from_state(7);
    adjust_ind_long_term_growth(&mut g, &mut rng);
    assert_eq!(g.ind_long_term_growth[3], 32);
    assert_eq!(g.ind_long_term_growth[4], -6);
    g.cur_year = 2012;
    let s = rng.state();
    adjust_ind_long_term_growth(&mut g, &mut rng);
    assert_eq!(rng.state(), s);
}

// ------------------------------------------------------------------------------- differential tests

#[test]
fn diff_update_industry_long_term_growth() {
    diff::par_seeds(1..=8, |seed| {
        let mut g = econ_world(seed);
        g.sim_co_count = if seed % 4 == 0 { 450 } else { 999 };
        g.cur_quarter = if seed % 2 == 0 { 4 } else { 3 };
        g.start_year = g.cur_year - [3, 12, 20][cast::<usize, _>(seed % 3)];
        if seed == 5 {
            g.bubble_state = 120;
        }
        check(0x4a56ec, &[], &g, 4242 + cast::<u32, _>(seed), &|g, _ui, rng| update_industry_long_term_growth(g, rng));
    });
}

#[test]
fn diff_adjust_ind_long_term_growth() {
    diff::par_seeds(1..=8, |seed| {
        let mut g = econ_world(seed);
        g.bubble_state = if seed == 3 { 150 } else { 0 };
        g.start_year = g.cur_year - [11, 16, 21, 7, 12][cast::<usize, _>(seed % 5)];
        check(0x4f8f90, &[], &g, 777 + cast::<u32, _>(seed), &|g, _ui, rng| adjust_ind_long_term_growth(g, rng));
    });
}

/// The quarterly step with the peer routines (incl. RandomWorldEvents, i.e. event 0) skipped in the
/// original; pandemic early exit, forecasts, clamps, re-valuation, tail rules.
#[test]
fn diff_update_economy_52978d() {
    diff::par_seeds(1..=10, |seed| {
        let g = econ_world(seed);
        let rng0 = 99 + cast::<u32, _>(seed) * 7919;
        let Some(o) = run_original_with(0x52978d, &opts(rng0, PEER_SKIPS.to_vec()), &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Scripted::default();
        update_economy_52978d(&mut port, &mut ui, &mut rng, 0);
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND sequence");
    });
}

/// (world seed, forced event id) pairs whose RandomWorldEvents result covers the econ-effect cases
/// 1, 5, 6, 7, 22, 25, 26, 27, 31, 46, 59, 61, 62, 63, 64, 68, 70, 71, 73, 75, 76, 77, 78, 80, 83, 87, 88,
/// 90, 92 (found by running the original with every event id and [`force_event`]; 10, 11, 19..21 and 56
/// need rarer states).
const EVENT_RUNS: &[(u64, i32)] = &[
    (24, 24), (130, 27), (15, 15), (517, 2), (21, 21), (14, 14), (22, 22), (539, 24), (13, 13), (184, 81),
    (185, 82), (318, 9), (394, 85), (2, 2), (80, 80), (33, 33), (320, 11), (1, 1), (123, 20), (30, 30),
    (531, 16), (116, 13), (136, 33), (403, 94), (22, 32), (14, 32), (15, 83), (18, 18), (30, 32), (38, 32),
];

/// Forces RandomWorldEvents to pick `ev` (every other event used up) and adjusts the state so that
/// the event's rarer branches are taken (variant `k`).
fn force_event(g: &mut Globals, ev: i32, k: u64) {
    g.pandemic_state = 0;
    g.crypto_ban_state = 1;
    if g.cur_year == g.start_year {
        g.cur_year += 1;
    }
    for e in 1..=103 {
        g.event_state[e] = 5;
    }
    g.event_state[ev] = 0;
    match ev {
        17 => g.gdp_growth = cast::<f32, _>(-Ext::from_f64(2.0)),
        18 => {
            g.monetary_policy = 0;
            g.gdp_growth = cast::<f32, _>(Ext::from_f64(3.0));
        }
        19 => {
            g.start_year = g.cur_year - 13;
            g.bubble_state = 0;
        }
        32 => g.event_state[32] = cast::<i32, _>(k % 5),
        83 => {
            g.start_year = g.cur_year - 16;
            g.peak_oil_crisis = [0, -1][cast::<usize, _>(k % 2)];
            g.gdp_growth = cast::<f32, _>([Ext::from_f64(1.0), Ext::from_f64(2.0)][cast::<usize, _>(k / 2 % 2)]);
            g.oil_crisis_level = cast::<f32, _>([Ext::from_f64(0.0), Ext::from_f64(1.0)][cast::<usize, _>(k / 4 % 2)]);
        }
        _ => {}
    }
}

/// The event cases: the ORIGINAL RandomWorldEvents is run (alone, on the port's state just before the
/// call) to supply the event, so the SELECT CASE effects are compared. The event is forced by marking
/// every other event as used up (`EventState > 4`).
#[test]
fn diff_update_economy_52978d_events() {
    let skips: Vec<(u32, u32)> = PEER_SKIPS.iter().copied().filter(|&s| s != RANDOM_WORLD_EVENTS).collect();
    let seen = std::sync::Mutex::new(std::collections::BTreeSet::new());
    diff::par_seeds(0..cast::<u64, _>(EVENT_RUNS.len()), |i| {
        let (k, ev) = EVENT_RUNS[cast::<usize, _>(i)];
        let mut g = econ_world(k);
        force_event(&mut g, ev, k / 8);
        let rng0 = 5 + cast::<u32, _>(k);
        let Some(o) = run_original_with(0x52978d, &opts(rng0, skips.clone()), &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Scripted::default();
        let event = RefCell::new(0);
        let mut world_events = |g: &mut Globals, _ui: &mut dyn wsr_core::ui::Ui, rng: &mut Rng, _t: i32| -> i32 {
            let w = run_original_with(0x52d793, &opts(rng.state(), skips.clone()), g).unwrap();
            *g = w.globals;
            rng.set_state(w.rng);
            *event.borrow_mut() = cast::<i32, _>(w.return_value);
            cast::<i32, _>(w.return_value)
        };
        update_economy_52978d_with(&mut port, &mut ui, &mut rng, 0, &mut world_events);
        seen.lock().unwrap().insert(*event.borrow());
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND sequence (event {})", event.borrow());
    });
    eprintln!("econ effects covered: {:?}", seen.lock().unwrap());
}
