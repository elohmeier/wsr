//! Tests of logic::economy::crises2: SubprimeCrisisUpdate (0x53b4c7), PandemicUpdate (0x53ca7b).
//!
//! The harness fixes `DATE$` = "01-03-2000" and `TIME$` = "10:00:00" (seconds 0).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{pandemic_update, subprime_crisis_update};

/// Peer routines (other economy submodules) the original skips until they are merged:
/// ShockCommodityPrice (0x635b16, 2 args).
const PEER_SKIPS: &[(u32, u32)] = &[];

const DATE: &str = "01-03-2000";

/// Like `check` in tests/economy.rs, with the peer skips.
#[track_caller]
fn check_skips(addr: u32, g: &Globals, rng0: u32, port_fn: &dyn Fn(&mut Globals, &mut Scripted, &mut Rng)) -> Option<Globals> {
    let opts = Opts { rng: Some(rng0), skips: PEER_SKIPS.to_vec(), ..Opts::default() };
    let o = run_original_with(addr, &opts, g)?;
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    port_fn(&mut port, &mut ui, &mut rng);
    assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
    let port_reports = ui.log.iter().filter(|l| l.starts_with("REPORT")).count();
    assert_eq!(port_reports, o.dialogs.len(), "reports: port {:?}\norig {:?}", ui.log, o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
    Some(o.globals)
}

/// World with macro state for the crises.
fn crisis_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491) ^ 0x5bd1);
    g.central_bank_name = s([__content.string(13428), __content.string(6645), "European Central Bank (ECB)", "BoJ"][cast::<usize, _>(r.range(0, 3))]);
    g.market_val_target = cast::<f64, _>(f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.6)));
    g.market_valuation = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.4), Ext::from_f64(1.8)));
    g.metals_price_base = cast::<i32, _>(r.range(0, 3000));
    g.econ_cycle_counter = cast::<i16, _>(r.range(0, 30));
    g.gdp_level = r.range(1_000_000, 40_000_000_000_000);
    g.monetary_stance = cast::<i32, _>(r.range(0, 2));
    g.gold_price_max = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(9000.0)));
    g.silver_price_max = cast::<f32, _>(f32r(&mut r, Ext::from_f64(20.0), Ext::from_f64(90.0)));
    g.silver_price_min = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(20.0)));
    g.tmp_news_str = s("joke");
    g.news_msg = if r.chance(Ext::from_f64(0.2)) { s(__content.string(13571)) } else { PbStr::new() };
    for e in 11..=85 {
        g.bank_c_ds[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
    }
    g
}

#[test]
fn subprime_start_and_stages_unit() {
    let __content = wsr_core::content::current();
    // start: armed, tight policy, after the first year
    let mut g = Globals::default();
    g.subprime_crisis_state = -2;
    g.monetary_policy = 2;
    g.start_year = 2000;
    g.cur_year = 2002;
    g.base_interest_rate = cast::<f32, _>(Ext::from_f64(5.0));
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(3.0));
    g.ind_growth_rate[5] = 10;
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(77);
    subprime_crisis_update(&mut g, &mut ui, &mut rng, &s(DATE));
    assert!((5..=10).contains(&g.subprime_crisis_state));
    assert_eq!(g.base_interest_rate, Ext::from_f64(6.0));
    assert_eq!(g.econ_cycle_counter, 5);
    assert_eq!(g.ind_growth_forecast[5], -7);
    assert_eq!(g.ind_growth_rate[5], 4);
    assert!(g.news_headline[60].to_display().contains(__content.string(13764)));

    // bubble blocks the start; state -1 does nothing
    let mut g2 = Globals::default();
    g2.subprime_crisis_state = -2;
    g2.monetary_policy = 2;
    g2.cur_year = 1;
    g2.bubble_state = 3;
    let before = g2.clone();
    subprime_crisis_update(&mut g2, &mut ui, &mut rng, &s(DATE));
    assert_eq!(g2.subprime_crisis_state, before.subprime_crisis_state);

    // stage 4 (state 5 -> 4): headline with the central bank's name, silver target x1.25
    let mut g3 = Globals::default();
    g3.subprime_crisis_state = 5;
    g3.central_bank_name = s("Bank of X");
    g3.silver_target_price = 20.0f32;
    g3.market_val_target = cast::<f64, _>(Ext::from_f64(1.0));
    g3.rand_seed = 2;
    subprime_crisis_update(&mut g3, &mut ui, &mut rng, &s(DATE));
    assert_eq!(g3.silver_target_price, Ext::from_f64(25.0));
    assert!(g3.news_headline[60].to_display().starts_with("BANK OF X SAYS SUBPRIME MORTGAGE CRISIS ALMOST OVER"));
}

#[test]
fn pandemic_time_paths_unit() {
    let __content = wsr_core::content::current();
    // contained after 30 seconds
    let mut g = Globals::default();
    g.pandemic_state = -3;
    g.gold_target_price = cast::<f32, _>(Ext::from_f64(5000.0));
    g.entity_status[200] = 1;
    g.industry_num[200] = 5;
    g.market_cap[200] = Ext::from_f64(100.0);
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(5);
    pandemic_update(&mut g, &mut ui, &mut rng, 45);
    assert_eq!(g.pandemic_state, 0);
    assert_eq!(g.gold_target_price, Ext::from_f64(900.0));
    assert!((g.market_cap[200] - Ext::from_f64(105.0)).abs() < Ext::from_f64(1e-5));
    assert!(g.news_headline[60].to_display().starts_with(__content.string(13765)));

    // Hainan virus: contained before 20 seconds, else the pandemic starts
    let mut g = Globals::default();
    g.pandemic_state = 6;
    pandemic_update(&mut g, &mut ui, &mut rng, 10);
    assert_eq!((g.pandemic_state, g.event_state[76]), (0, 9));
    let mut g = Globals::default();
    g.pandemic_state = 6;
    pandemic_update(&mut g, &mut ui, &mut rng, 25);
    assert_eq!(g.pandemic_state, 5);
}

/// SubprimeCrisisUpdate over all stages (-2 start / tightening, 0, -1, 1..11) and macro conditions.
#[test]
fn diff_subprime_crisis_update() {
    init_files();
    diff::par_seeds(1..=60, |seed| {
        let mut g = crisis_world(seed);
        let mut r = Rand(seed * 1009 + 17);
        const STATES: [i16; 15] = [-2, -2, 0, -1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        g.subprime_crisis_state = STATES[cast::<usize, _>(seed % 15)];
        g.bubble_state = if r.chance(Ext::from_f64(0.2)) { 3 } else { 0 };
        g.monetary_policy = cast::<i16, _>(r.range(0, 2));
        g.rand_seed = cast::<i32, _>(r.range(0, 30000));
        g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(8.0)));
        if r.chance(Ext::from_f64(0.5)) {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(3.0), Ext::from_f64(2.0)));
        }
        if g.subprime_crisis_state == -2 && seed % 15 == 0 {
            g.monetary_policy = 2;
            g.start_year = g.cur_year - 1;
        } else if g.subprime_crisis_state == -2 {
            g.monetary_policy = 0;
            g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(13.0)));
        }
        let rng0 = 1000 + cast::<u32, _>(seed) * 7919;
        check_skips(0x53b4c7, &g, rng0, &|p, ui, rng| subprime_crisis_update(p, ui, rng, &s(DATE)));
    });
}

/// PandemicUpdate over states -6..6 (TIME$ seconds = 0 in the harness).
#[test]
fn diff_pandemic_update() {
    init_files();
    diff::par_seeds(1..=30, |seed| {
        let mut g = crisis_world(seed + 100);
        let mut r = Rand(seed * 3301 + 5);
        g.pandemic_state = [-6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 5, -5][cast::<usize, _>(seed % 15)];
        g.rand_seed = cast::<i32, _>(r.range(-5, 30000));
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(25.0), Ext::from_f64(8.0)));
        g.ind_growth_rate[50] = cast::<i16, _>(r.range(10, 40));
        if r.chance(Ext::from_f64(0.5)) {
            g.start_year = g.cur_year;
            g.cur_quarter = 1;
        }
        let rng0 = 4242 + cast::<u32, _>(seed) * 104_729;
        check_skips(0x53ca7b, &g, rng0, &|p, ui, rng| pandemic_update(p, ui, rng, 0));
    });
}
