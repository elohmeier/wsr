//! Tests of logic::economy::crises: MonetaryPolicyScenario (0x537b20), PeakOilCrisisUpdate (0x53a308),
//! CountryDisasterShock (0x539f98), TechBubbleCycle (0x482549) with Bubble_StageNews (0x4831b8).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{
    country_disaster_shock, monetary_policy_scenario, peak_oil_crisis_update, tech_bubble_cycle,
};

/// Peer routines (other submodules) the original skips while they are stubs in this worktree:
/// ShockCommodityPrice (commodities.rs).
const PEER_SKIPS: &[(u32, u32)] = &[];

#[track_caller]
fn check_sk(addr: u32, args: &[i32], g: &Globals, rng0: u32, port_fn: &dyn Fn(&mut Globals, &mut Scripted, &mut Rng)) {
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
    // PBERR: runtime errors that go to the ON ERROR handler (ShowRuntimeError only)
    let unexp: Vec<_> = unexpected(&o.log).into_iter().filter(|l| !l.starts_with("PBERR 9")).collect();
    assert!(unexp.is_empty(), "{unexp:?}");
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    let err_boxes = o.log.iter().filter(|l| l.starts_with("PBERR")).count(); // ShowRuntimeError popups
    assert_eq!(port_msgs + err_boxes, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
    let port_reports = ui.log.iter().filter(|l| l.starts_with("REPORT")).count();
    assert_eq!(port_reports, o.dialogs.len(), "reports: port {:?}", ui.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
}

/// Economic globals the scenarios read, randomized on top of [`world`].
fn macro_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x51_7cc1_b727_220a) | 1);
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.next_market_cycle_year = g.cur_year + cast::<i32, _>(r.range(-1, 1));
    g.metals_price_base = cast::<i32, _>(r.range(0, 2000));
    g.cap_gains_tax_rate = cast::<i32, _>(r.range(10, 35));
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.central_bank_name = s([__content.string(4373), __content.string(6647), __content.string(6645), ""][cast::<usize, _>(r.range(0, 3))]);
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)));
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(8.0)));
    g.market_val_target = cast::<f64, _>(f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.5)));
    g.market_valuation = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.5)));
    g.suppress_popups = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
    g.oil_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)));
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(15.0)));
    g.interest_rate_target = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(20.0)));
    g.econ_scenario_id = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(1, 100)) } else { 0 };
    for k in 0..=100 {
        if r.chance(Ext::from_f64(0.6)) {
            g.macro_event_used[k] = 1;
        }
    }
    for p in 1..=71 {
        g.ind_long_term_growth[p] = cast::<i16, _>(r.range(-60, 60));
    }
    g.news_msg = if r.chance(Ext::from_f64(0.3)) { s("STALE HEADLINE LEFT BY THE CALLER") } else { PbStr::new() };
    g
}

#[test]
fn country_disaster_shock_unit() {
    let mut g = Globals::default();
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.entity_status[200] = 1;
    g.country_idx[200] = 3;
    g.industry_num[200] = 10;
    g.profit_margin[200] = 20;
    g.market_cap[200] = Ext::from_f64(10.0);
    g.cash[200] = Ext::from_f64(100.0);
    g.business_assets[200] = Ext::from_f64(50.0); // below 100 * ExchangeRate: no asset loss
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    country_disaster_shock(&mut g, &mut ui, &mut rng, 3);
    assert_eq!(g.profit_margin[200], 8);
    assert_eq!(g.cash[200], Ext::from_f64(70.0));
    assert_eq!(g.extraordinary_pl[200], -30);
    assert!((g.market_cap[200] - Ext::from_f64(10.0) * cast::<Ext, _>(0.85f32)).abs() < Ext::from_f64(1e-12));
    let before = g.clone();
    country_disaster_shock(&mut g, &mut ui, &mut rng, 53); // out of range: nothing
    assert_eq!(g.cash[200], before.cash[200]);
}

#[test]
fn diff_country_disaster_shock() {
    diff::par_seeds(1..=8, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 13 + 1);
        g.exchange_rate = cast::<f64, _>([Ext::from_f64(1.0), Ext::from_f64(0.8), Ext::from_f64(1.5)][cast::<usize, _>(r.range(0, 2))]);
        let c = match seed {
            7 => -1,
            8 => 53,
            _ => cast::<i32, _>(r.range(0, 25)),
        };
        for e in 11..=1600 {
            g.extraordinary_pl[e] = r.range(-1000, 1000);
        }
        check_sk(0x539f98, &[c], &g, cast::<u32, _>(seed) * 7919, &|g, ui, rng| country_disaster_shock(g, ui, rng, c));
    });
}

/// Peak-oil stages (start, middle, late, aftermath levels) with the various headline repeats.
#[test]
fn diff_peak_oil_crisis_update() {
    let __content = wsr_core::content::current();
    diff::par_seeds(1..=24, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 101 + 5);
        match seed % 3 {
            0 => {
                g.peak_oil_crisis = -1;
                g.oil_crisis_level = cast::<f32, _>([Ext::from_f64(0.0), Ext::from_f64(0.5), Ext::from_f64(1.5), Ext::from_f64(2.5), Ext::from_f64(4.0), Ext::from_f64(6.0), Ext::from_f64(8.0)][cast::<usize, _>(r.range(0, 6))]);
                g.econ_cycle_counter = cast::<i16, _>(r.range(0, 3));
            }
            _ => {
                g.peak_oil_crisis = 1;
                g.oil_crisis_stage = [0, 1, 3, 6, 10, 12, 15, 17, 20][cast::<usize, _>(r.range(0, 8))];
                g.oil_crisis_level = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(7.0)));
            }
        }
        if seed == 3 {
            g.peak_oil_crisis = 0;
        }
        g.last_oil_crisis_headline = s([
            "",
            "EXPERTS WARN: X",
            __content.string(13760),
            __content.string(13761),
            __content.string(13762),
            __content.string(13763),
        ][cast::<usize, _>(r.range(0, 5))]);
        g.rand_seed = if r.chance(Ext::from_f64(0.1)) { cast::<i32, _>(r.range(0, 10)) } else { cast::<i32, _>(r.range(11, 32000)) };
        for ind in 1..=71 {
            g.ind_demand[ind] = r.range(1000, 300_000);
        }
        check_sk(0x53a308, &[], &g, cast::<u32, _>(seed) * 7717 + 3, &|g, ui, rng| peak_oil_crisis_update(g, ui, rng));
    });
}

/// Tech (100..120) and biotech (200..220) bubble stages, small and large games.
#[test]
fn diff_tech_bubble_cycle() {
    init_files();
    diff::par_seeds(1..=32, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 977 + 11);
        let stage = 100 + cast::<i32, _>((seed - 1) % 21);
        g.bubble_state = if seed % 2 == 0 { stage + 100 } else { stage };
        if seed == 32 {
            g.bubble_state = 50;
        }
        g.sim_co_count = if r.chance(Ext::from_f64(0.35)) { 300 } else { 1450 };
        if seed == 25 {
            g.sim_co_count = 300;
            g.bubble_state = 213;
        }
        g.bubble_start_fired = if r.chance(Ext::from_f64(0.5)) { 0 } else { 1 };
        g.last_bubble_news_stage = if r.chance(Ext::from_f64(0.3)) { stage } else { 0 };
        g.econ_cycle_counter = cast::<i16, _>(r.range(-3, 6));
        for ind in 1..=71 {
            g.ind_demand[ind] = r.range(1000, 300_000);
            g.ind_growth_rate[ind] = cast::<i16, _>(r.range(-10, 40));
        }
        check_sk(0x482549, &[], &g, cast::<u32, _>(seed) * 31337 + 1, &|g, ui, rng| tech_bubble_cycle(g, ui, rng));
    });
}

/// Every scenario range and every id with its own branch forced (plus out-of-range ids), and random
/// picks (scenario 0) with or without a running scenario.
#[test]
fn diff_monetary_policy_scenario() {
    const IDS: &[i32] = &[
        1, 2, 5, 3, 6, 8, 9, 11, 12, 13, 16, 18, 19, 20, 21, 22, 25, 23, 26, 27, 28, 31, 33, 36, 41, 46, 51, 52,
        56, 58, 59, 62, 65, 68, 70, 71, 74, 76, 78, 79, 80, 81, 83, 84, 85, 87, 88, 90, 93, 99, 100, 101, -3,
    ];
    diff::par_seeds(1..=(cast::<u64, _>(IDS.len()) + 8), |seed| {
        let g = macro_world(seed);
        let sc = IDS.get(cast::<usize, _>(seed) - 1).copied().unwrap_or(0);
        check_sk(0x537b20, &[sc], &g, cast::<u32, _>(seed) * 2654435 + 7, &|g, ui, rng| {
            monetary_policy_scenario(g, ui, rng, sc)
        });
    });
}
