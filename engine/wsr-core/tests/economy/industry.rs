//! Tests of logic::economy::industry: UpdateIndustryOutlook (0x6570e9), CalcIndustryDemandSwing
//! (0x65b298), CommodityDemandEffects (0x4bce6a), IndustryFadNews (0x4bc887).
//!
//! The harness fixes `TIME$` at 10:00:00, so IndustryFadNews runs with `time_secs = 0`.
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{
    calc_industry_demand_swing, commodity_demand_effects, industry_fad_news, update_industry_outlook,
};

/// Peer routines (other economy submodules) the original must skip until they are merged: none are
/// called by these routines.
const PEER_SKIPS: &[(u32, u32)] = &[];

/// `check` with the original skipping `PEER_SKIPS`.
#[track_caller]
fn check_ind(addr: u32, args: &[i32], g: &Globals, rng0: u32, port_fn: &dyn Fn(&mut Globals, &mut Scripted, &mut Rng)) {
    let opts = Opts {
        args: args.iter().map(|a| a.to_string()).collect(),
        rng: Some(rng0),
        skips: PEER_SKIPS.to_vec(),
        ..Opts::default()
    };
    let Some(o) = run_original_with(addr, &opts, g) else { return };
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    port_fn(&mut port, &mut ui, &mut rng);
    assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
}

/// The shared world plus the industry tables and macro variables these routines read.
fn ind_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491) ^ 0x5555);
    for ind in 0..=71 {
        g.ind_name[ind] = s(["Gold Mining", "Steel", "Oil Refining & Marketing Companies", "Banks"][cast::<usize, _>(r.range(0, 3))]);
    }
    for ind in 1..=71 {
        g.ind_volatility[ind] = cast::<i16, _>(r.range(0, 8));
        g.ind_long_term_growth[ind] = cast::<i16, _>(r.range(-5, 25));
        g.ind_growth_forecast[ind] = cast::<i16, _>(r.range(-25, 45));
        g.ind_growth_rate[ind] = cast::<i16, _>(r.range(-15, 35));
        g.ind_demand[ind] = r.range(1000, 5_000_000);
    }
    for ind in 3..=69 {
        g.ind_demand_gap[ind] = cast::<f64, _>(if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(45.0)) });
    }
    g.gdp_growth = cast::<f32, _>([f32r(&mut r, -Ext::from_f64(6.0), Ext::from_f64(8.0)), f32r(&mut r, -Ext::from_f64(2.0), Ext::from_f64(1.0)), f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(6.0)), f32r(&mut r, -Ext::from_f64(25.0), Ext::from_f64(11.0))]
        [cast::<usize, _>(r.range(0, 3))]);
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(25.0), Ext::from_f64(9.0)));
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(13.0)));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.4), Ext::from_f64(1.8)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.5), Ext::from_f64(9.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(10.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(60.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(9000.0)));
    g.metals_price_base = cast::<i32, _>(r.range(0, 5000));
    g.market_valuation = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.5)));
    g.market_val_target = cast::<f64, _>(f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.5)));
    g.ind_demand_cycle_idx = if r.chance(Ext::from_f64(0.2)) { 0 } else { cast::<i32, _>(r.range(3, 69)) };
    g.sim_co_count = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(1401, 1600)) } else { cast::<i32, _>(r.range(11, 1400)) };
    g.sel_industry = cast::<i32, _>(r.range(1, 30));
    if r.chance(Ext::from_f64(0.3)) {
        g.sel_industry = [9, 11, 12, 23, 25][cast::<usize, _>(r.range(0, 4))];
    }
    g
}

#[test]
fn calc_industry_demand_swing_unit() {
    let mut g = Globals::default();
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(6.0));
    g.ind_demand_gap[5] = cast::<f64, _>(Ext::from_f64(1.0));
    g.ind_demand[5] = 1_000_000;
    // boom: RND - 1.5 >= 6 fails, RND + 3.2 <= 6 holds; gap 1 below 8..12; change (6-3)*5*1*0.01 = 0.15 -> 0.08
    let mut rng = Rng::from_state(12345);
    calc_industry_demand_swing(&mut g, &mut rng, 5);
    assert_eq!(g.ind_demand[5], 1_080_000);
    // gold (50) is never swung
    g.ind_demand_gap[50] = cast::<f64, _>(Ext::from_f64(1.0));
    g.ind_demand[50] = 777;
    calc_industry_demand_swing(&mut g, &mut rng, 50);
    assert_eq!(g.ind_demand[50], 777);
}

#[test]
fn industry_fad_news_unit() {
    let mut g = Globals::default();
    g.fad_industry = 10;
    g.fad_valuation_pct = 200;
    g.ind_fad_peak = 220;
    g.ind_demand[10] = 1000;
    g.ind_name[10] = s("Steel");
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    industry_fad_news(&mut g, &mut ui, &mut rng, 37);
    assert_eq!(g.news_headline[60], "Analysts say steel stocks are undervalued.");
    assert_eq!(g.ind_demand[10], 1050);
    assert_eq!(g.fad_valuation_pct, 202);
    // above the peak: deflate by 3, demand x0.95
    g.ind_fad_peak = 160;
    industry_fad_news(&mut g, &mut ui, &mut rng, 0);
    assert_eq!(g.fad_valuation_pct, 199);
    assert_eq!(g.ind_demand[10], 997); // 1050 * 0.95f32 = 997.4999…
    // no fad during a bubble (IndAnalystPicked is still cleared)
    g.bubble_state = 1;
    g.ind_analyst_picked[3] = 1;
    industry_fad_news(&mut g, &mut ui, &mut rng, 0);
    assert_eq!(g.fad_valuation_pct, 199);
    assert_eq!(g.ind_analyst_picked[3], 0);
}

#[test]
fn diff_calc_industry_demand_swing() {
    diff::par_seeds(1..=16, |seed| {
        let g = ind_world(seed);
        let mut r = Rand(seed * 7 + 1);
        for _ in 0..3 {
            let ind = cast::<i32, _>(r.range(3, 69));
            let rng0 = cast::<u32, _>(r.range(1, 1 << 30));
            check_ind(0x65b298, &[ind], &g, rng0, &|g, _ui, rng| calc_industry_demand_swing(g, rng, ind));
        }
    });
}

static NEWS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[test]
fn diff_update_industry_outlook() {
    let __content = wsr_core::content::current();
    diff::par_seeds(1..=24, |seed| {
        let mut g = ind_world(seed);
        g.cur_quarter = cast::<i32, _>(seed % 4) + 1;
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) | 1;
        let mut ui = Scripted::default();
        let mut port = g.clone();
        update_industry_outlook(&mut port, &mut ui, &mut Rng::from_state(rng0));
        NEWS.fetch_add(ui.log.iter().filter(|l| l.starts_with("NEWS")).count(), std::sync::atomic::Ordering::Relaxed);
        check_ind(0x6570e9, &[], &g, rng0, &|g, ui, rng| update_industry_outlook(g, ui, rng));
    });
    eprintln!("UpdateIndustryOutlook: {}{__wsr_text_13766}", NEWS.load(std::sync::atomic::Ordering::Relaxed),__wsr_text_13766=__content.string(13766));
}

static HITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[test]
fn diff_commodity_demand_effects() {
    diff::par_seeds(1..=18, |seed| {
        let mut g = ind_world(seed);
        let mut r = Rand(seed * 13 + 5);
        let inds = [3, 23, 25, 53, 67, 68, 5, 6, 7, 8, 15, 16, 52, 50, 69, 10, 44, 1];
        let ind = inds[(cast::<usize, _>(seed)) % inds.len()];
        g.sim_co_count = 70 * cast::<i32, _>(r.range(2, 22)) + ind;
        g.entity_status[g.sim_co_count] = 1;
        if (3..=69).contains(&ind) {
            g.ind_demand_gap[ind] = cast::<f64, _>(if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(45.0)) });
        }
        // RND states for which the routine acts (RND * 5 <= 1), plus one where it does not
        let mut states = Vec::new();
        let mut x = (cast::<u32, _>(seed)).wrapping_mul(747_796_405) | 1;
        while states.len() < 2 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            if Rng::from_state(x).rnd() * Ext::from_f64(5.0) <= Ext::from_f64(1.0) {
                states.push(x);
            }
        }
        states.push(x ^ 0x5a5a_5a5a);
        for rng0 in states {
            let mut port = g.clone();
            commodity_demand_effects(&mut port, &mut Scripted::default(), &mut Rng::from_state(rng0));
            if port.ind_growth_forecast.as_slice() != g.ind_growth_forecast.as_slice() {
                HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            check_ind(0x4bce6a, &[], &g, rng0, &|g, ui, rng| commodity_demand_effects(g, ui, rng));
        }
    });
    eprintln!("CommodityDemandEffects: {} runs changed forecasts", HITS.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn diff_industry_fad_news() {
    diff::par_seeds(1..=24, |seed| {
        let mut g = ind_world(seed);
        let mut r = Rand(seed * 17 + 3);
        g.bubble_state = if r.chance(Ext::from_f64(0.15)) { 1 } else { 0 };
        g.fad_industry = if r.chance(Ext::from_f64(0.3)) { 0 } else { cast::<i32, _>(r.range(1, 69)) };
        g.fad_valuation_pct = [0, 75, 80, 104, 105, 139, 140, 141, 199, 200, 201, 230][cast::<usize, _>(r.range(0, 11))];
        g.ind_fad_peak = if r.chance(Ext::from_f64(0.2)) { 0 } else { cast::<i16, _>(r.range(150, 262)) };
        g.rand_seed = if r.chance(Ext::from_f64(0.2)) { 97 + 100 * cast::<i32, _>(r.range(0, 300)) } else { cast::<i32, _>(r.range(0, 30000)) };
        let rng0 = cast::<u32, _>(r.range(1, 1 << 30));
        check_ind(0x4bc887, &[], &g, rng0, &|g, ui, rng| industry_fad_news(g, ui, rng, 0));
    });
}
