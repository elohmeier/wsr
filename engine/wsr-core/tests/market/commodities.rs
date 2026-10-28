//! UpdateCommodityPrices (0x62a6a2): unit checks and differential tests against the original.
use wsr_core::pb::real::{Ext, cast};


use crate::diff::{Compare, Opts, Rand, run_original_with};
use wsr_core::logic::market::commodities::update_commodity_prices;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

const NAMES: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];

/// A plausible commodity-market state; `regime` spreads the runs over the special cases.
fn world(seed: u64) -> Globals {
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    let f = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000)) / Ext::from_f64(1e6);
    g.num_players = 2;
    g.cur_player = 1;
    g.start_year = cast::<i32, _>(r.range(1990, 2010));
    g.cur_year = g.start_year + cast::<i32, _>(r.range(0, 40));
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = [6, 300, 1531, 1600][cast::<usize, _>(r.range(0, 3))];
    g.rand_seed = cast::<i32, _>(r.range(-100_000, 100_000));
    g.metals_price_base = if r.chance(Ext::from_f64(0.2)) { 0 } else { cast::<i32, _>(r.range(0, 9000)) };
    let base = cast::<Ext, _>(g.metals_price_base);
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(14.0))));
    g.interest_rate_target = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(14.0))));
    g.prime_rate = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(15.0))));
    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(f(&mut r, -Ext::from_f64(12.0), Ext::from_f64(9.0))));
    g.peak_oil_crisis = cast::<i16, _>(r.range(-1, 1));
    g.oil_crisis_level = cast::<f32, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { Ext::from(cast::<f32, _>(f(&mut r, -Ext::from_f64(1.0), Ext::from_f64(3.0)))) });
    g.pandemic_state = cast::<i16, _>(r.range(-1, 1));
    let sign = |r: &mut Rand| if r.chance(Ext::from_f64(0.08)) { -Ext::from_f64(1.0) } else { Ext::from_f64(1.0) };
    g.oil_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.05), Ext::from_f64(3.5)) * sign(&mut r)));
    g.oil_target_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.05), Ext::from_f64(3.5))));
    g.gold_price = cast::<f32, _>(cast::<f32, _>((base + f(&mut r, Ext::from_f64(500.0), Ext::from_f64(25000.0))) * sign(&mut r)));
    if r.chance(Ext::from_f64(0.2)) {
        g.gold_price = cast::<f32, _>(cast::<f32, _>(base + f(&mut r, Ext::from_f64(40000.0), Ext::from_f64(70000.0)))); // UpdateMetalsPriceBase / 0.2 > 8000
    }
    g.gold_target_price = cast::<f32, _>(if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(0.0) } else { Ext::from(cast::<f32, _>(base + f(&mut r, Ext::from_f64(500.0), Ext::from_f64(25000.0)))) });
    g.gold_price_min = cast::<f32, _>(cast::<f32, _>(base + f(&mut r, Ext::from_f64(1000.0), Ext::from_f64(6000.0))));
    g.gold_price_max = cast::<f32, _>(cast::<f32, _>(base + f(&mut r, Ext::from_f64(4000.0), Ext::from_f64(26000.0))));
    g.silver_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(2.0), Ext::from_f64(260.0)) * sign(&mut r)));
    g.silver_target_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(2.0), Ext::from_f64(260.0))));
    g.silver_price_min = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(1.0), Ext::from_f64(60.0))));
    g.silver_price_max = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(30.0), Ext::from_f64(300.0))));
    g.wheat_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(20.0)) * sign(&mut r)));
    g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(20.0))));
    g.wheat_price_min = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(4.0))));
    g.wheat_price_max = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(3.0), Ext::from_f64(22.0))));
    g.corn_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(19.0)) * sign(&mut r)));
    g.corn_target_price = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(19.0))));
    g.corn_price_min = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(0.5), Ext::from_f64(4.0))));
    g.corn_price_max = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(3.0), Ext::from_f64(22.0))));
    g.bitcoin_price = cast::<i32, _>(r.range(-50_000, 90_000));
    g.ethereum_price = cast::<i32, _>(r.range(-500, 5000));
    g.stock_index = f(&mut r, Ext::from_f64(500.0), Ext::from_f64(90_000.0));
    g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0))));
    g.short_bond_yield = cast::<f32, _>(cast::<f32, _>(f(&mut r, Ext::from_f64(1.0), Ext::from_f64(12.0))));
    for k in 1..=10 {
        g.futures_basis[k] = cast::<i16, _>(r.range(-130, 330));
        g.commodity_name[k] = PbStr::from(NAMES.get(cast::<usize, _>(k) - 1).copied().unwrap_or(""));
    }
    if r.chance(Ext::from_f64(0.2)) {
        g.news_msg = PbStr::from("Something happened");
    }
    // some price-history slots already in use
    for series in [0, 6, 7, 8, 9, 10, 1601, 1602, 1603, 1604, 1605, 1606] {
        if r.chance(Ext::from_f64(0.7)) {
            let slot = cast::<Ext, _>(r.range(1, 12));
            g.cap_hist_close[(0, series)] = slot;
            g.cap_hist_close[(cast::<i32, _>(slot), series)] = f(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
            g.price_hist_high[(cast::<i32, _>(slot), series)] = f(&mut r, Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.price_hist_low[(cast::<i32, _>(slot), series)] = f(&mut r, Ext::from_f64(0.5), Ext::from_f64(50.0));
        }
    }
    g
}

fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

fn port(g: &mut Globals, rng0: u32) -> (Rng, Scripted) {
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(rng0);
    update_commodity_prices(g, &mut ui, &mut rng);
    (rng, ui)
}

#[test]
fn update_commodity_prices_invariants() {
    for seed in (0..200u64).chain(1000..1060) {
        let mut g = world(seed);
        port(&mut g, cast::<u32, _>(seed) * 7 + 1);
        for k in 1..=6 {
            let b = g.futures_basis[k];
            assert!((-100..=300).contains(&b), "seed {seed}: basis {k} = {b}");
        }
        assert!(g.futures_basis[2] >= 2 && g.futures_basis[3] >= 2, "seed {seed}");
        assert!(g.futures_basis[6] >= -30, "seed {seed}");
        assert!(g.bitcoin_price >= 0 && g.ethereum_price >= 0);
        assert!(g.oil_price >= Ext::from_f64(0.15), "seed {seed}: oil {}", g.oil_price);
        assert!(g.news_msg.is_empty() || g.news_msg != "Something happened");
    }
}

#[test]
fn update_commodity_prices_hand_checked() {
    // Negative prices are made positive first; with all bases far above 300 they end clamped.
    let mut g = world(5);
    g.bitcoin_price = -1234;
    g.ethereum_price = -77;
    port(&mut g, 99);
    assert_eq!(g.bitcoin_price, 1234);
    assert_eq!(g.ethereum_price, 77);
    // GoldTargetPrice = 0 triggers SetCommodityPriceRanges, which sets a target.
    let mut g = world(6);
    g.gold_target_price = cast::<f32, _>(Ext::from_f64(0.0));
    port(&mut g, 3);
    assert!(g.gold_target_price > Ext::from_f64(0.0));
}

#[test]
fn update_commodity_prices_matches_original() {
    let mut ran = 0;
    let mut news = 0;
    for seed in 0..60u64 {
        let mut g = world(1000 + seed);
        let rng0 = 31 * cast::<u32, _>(seed) + 5;
        let opts = Opts { rng: Some(rng0), ..Opts::default() };
        let Some(o) = run_original_with(0x62a6a2, &opts, &g) else {
            return;
        };
        let (rng, ui) = port(&mut g, rng0);
        assert_eq!(rng.state(), o.rng, "seed {seed}: RND state");
        cmp().assert(&g, &o.globals);
        assert!(!o.log.iter().any(|l| l.starts_with("PBERR")), "seed {seed}: {:?}", o.log);
        news += ui.log.len();
        ran += 1;
    }
    assert!(ran == 60);
    eprintln!("ui log entries: {news}");
}

/// Rare branches, found by instrumenting the port: the silver shock (RND * 195 > 194, BEEP), gold -35
/// (UpdateMetalsPriceBase > 2700 * RND + 8800) and gold -50 (> 2000 * RND + 8000 with rising rates).
#[test]
fn update_commodity_prices_rare_branches_match_original() {
    for (seed, k, rates) in [(1000u64, 299u32, false), (1001, 24, false), (1003, 100, false), (1019, 50, false), (1019, 2, true), (1019, 15, true)] {
        let mut g = world(seed);
        if rates {
            g.base_interest_rate = cast::<f32, _>(Ext::from_f64(8.0));
            g.interest_rate_target = cast::<f32, _>(Ext::from_f64(10.0));
            g.gdp_growth = cast::<f32, _>(Ext::from_f64(2.0));
        }
        let rng0 = k * 7919 + 13;
        let opts = Opts { rng: Some(rng0), ..Opts::default() };
        let Some(o) = run_original_with(0x62a6a2, &opts, &g) else {
            return;
        };
        let (rng, _) = port(&mut g, rng0);
        assert_eq!(rng.state(), o.rng, "seed {seed} k {k}");
        cmp().assert(&g, &o.globals);
    }
}
