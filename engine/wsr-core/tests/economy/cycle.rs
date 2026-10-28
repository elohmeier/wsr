//! Tests of logic::economy::cycle: PlanMarketCycle (0x4bfbd3), GenMarketCyclePlan (0x4c4b18),
//! MacroOutlookNews (0x4c1bc7), MarketReversalEvent (0x4c3454), MarketAftershock (0x4c3d74),
//! GovFiscalPolicyNews (0x4c5d0b).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{
    gen_market_cycle_plan, gov_fiscal_policy_news, macro_outlook_news, market_aftershock, market_reversal_event,
    plan_market_cycle,
};

/// Peer routines of other economy submodules called by these routines: none.
const PEER_SKIPS: &[(u32, u32)] = &[];
/// Unported callees outside the economy module: none.
const OTHER_SKIPS: &[(u32, u32)] = &[];

/// Runs the original with skips and compares like `check`.
#[track_caller]
fn check_skips(
    addr: u32,
    args: &[i32],
    g: &Globals,
    rng0: u32,
    port_fn: &dyn Fn(&mut Globals, &mut Scripted, &mut Rng),
) -> Option<Original> {
    let skips: Vec<(u32, u32)> = PEER_SKIPS.iter().chain(OTHER_SKIPS).copied().collect();
    let opts = Opts { args: args.iter().map(|a| a.to_string()).collect(), rng: Some(rng0), skips, ..Opts::default() };
    let o = run_original_with(addr, &opts, g)?;
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    port_fn(&mut port, &mut ui, &mut rng);
    assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
    Some(o)
}

/// `world(seed)` plus the macro state these routines read: rate / commodity / index histories with a
/// bias per seed (so the "long period of high/low/mid" branches trigger), crisis flags mostly off, plan
/// strings, futures records.
fn macro_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x5851_f42d_4c95_7f2d) ^ 0x1234_5678);
    g.bubble_state = if r.chance(Ext::from_f64(0.1)) { 50 } else { 0 };
    g.oil_crisis_level = cast::<f32, _>(if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(1.0) } else { Ext::from_f64(0.0) });
    g.pandemic_state = if r.chance(Ext::from_f64(0.05)) { 1 } else { 0 };
    g.peak_oil_crisis = [0, 0, 0, 1, -1][cast::<usize, _>(r.range(0, 4))];
    g.subprime_crisis_state = if r.chance(Ext::from_f64(0.05)) { 1 } else { 0 };
    g.monetary_stance = cast::<i32, _>(r.range(-1, 1));
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.market_val_target = cast::<f64, _>(f64r(&mut r, Ext::from_f64(0.4), Ext::from_f64(1.6)));
    g.market_valuation = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.4), Ext::from_f64(1.6)));
    g.econ_cycle_counter = cast::<i16, _>(r.range(-2, 25));
    g.central_bank_name = s([__content.string(4373), __content.string(6645), "ECB"][cast::<usize, _>(r.range(0, 2))]);
    g.currency_code = s(["USA", "GBP", "EUR"][cast::<usize, _>(r.range(0, 2))]);
    g.metals_price_base = [0, 0, 500, 1000][cast::<usize, _>(r.range(0, 3))];
    g.game_end_year = g.start_year + cast::<i32, _>(r.range(5, 60));
    g.next_market_cycle_year = if r.chance(Ext::from_f64(0.3)) { 0 } else { g.cur_year + cast::<i32, _>(r.range(-2, 6)) };
    g.first_year_mkt_counter = [0, 0, 5, 9, 10, 18, 19][cast::<usize, _>(r.range(0, 6))];
    g.stock_index_base = [1000, 5000, 10000][cast::<usize, _>(r.range(0, 2))];
    g.stock_index = cast::<Ext, _>(g.stock_index_base) * f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(4.0));
    g.rate_target_band_str = if r.chance(Ext::from_f64(0.5)) { PbStr::new() } else { s(" 2.5| 4.75") };
    g.market_cycle_plan = match r.range(0, 3) {
        0 => PbStr::new(),
        1 => s(&format!("{}|{}|{}|{}", g.cur_year, g.cur_year + 2, g.cur_year + 6, g.cur_year + 8)),
        2 => s(&format!("{}|{}|{}|{}", g.cur_year - 6, g.cur_year - 4, g.cur_year - 1, g.cur_year + 1)),
        _ => s(&format!("{}|{}|{}|{}", g.cur_year + 3, g.cur_year + 5, g.cur_year - 9, g.cur_year - 7)),
    };
    g.oil_outlook_news_done = cast::<i32, _>(r.range(0, 1));
    g.gold_outlook_news_done = cast::<i32, _>(r.range(0, 1));
    g.silver_outlook_news_done = cast::<i32, _>(r.range(0, 1));
    g.wheat_outlook_news_done = cast::<i32, _>(r.range(0, 1));
    g.suppress_popups = if r.chance(Ext::from_f64(0.2)) { 1 } else { 0 };
    g.news_msg = PbStr::new();
    g.silver_price_min = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(20.0)));
    g.silver_price_max = cast::<f32, _>(f32r(&mut r, Ext::from_f64(20.0), Ext::from_f64(90.0)));
    // futures records (STOCKS for computer players and others)
    let n = cast::<i32, _>(r.range(0, 8));
    g.futures_recs[0] = s(&n.to_string());
    for k in 1..=n {
        let holder = r.range(1, 6);
        let kind = if r.chance(Ext::from_f64(0.7)) { "STOCKS" } else { "GOLD" };
        g.futures_recs[k] = if r.chance(Ext::from_f64(0.1)) { PbStr::new() } else { s(&format!("{kind}|{holder}|100|6|2030|1|2")) };
    }
    for p in 1..=5 {
        g.is_computer_player[p] = cast::<i32, _>(r.range(0, 1));
    }
    // histories: per series a level band chosen per seed
    let fill = |g: &mut Globals, series: i32, lo: Ext, hi: Ext, r: &mut Rand| {
        for m in 1..=60 {
            let v = f64r(r, lo, hi);
            g.cap_hist_close[(m, series)] = v;
            g.price_hist_high[(m, series)] = v * f64r(r, Ext::from_f64(1.0), Ext::from_f64(1.1));
            g.price_hist_low[(m, series)] = if r.chance(Ext::from_f64(0.03)) { Ext::from_f64(0.0) } else { v * f64r(r, Ext::from_f64(0.9), Ext::from_f64(1.0)) };
        }
    };
    let rates = [(Ext::from_f64(1.0), Ext::from_f64(3.5)), (Ext::from_f64(6.5), Ext::from_f64(12.0)), (Ext::from_f64(4.0), Ext::from_f64(6.0)), (Ext::from_f64(1.0), Ext::from_f64(9.0))][cast::<usize, _>(r.range(0, 3))];
    fill(&mut g, 1601, rates.0, rates.1, &mut r);
    let oil = [(Ext::from_f64(130.0), Ext::from_f64(200.0)), (Ext::from_f64(20.0), Ext::from_f64(75.0)), (Ext::from_f64(40.0), Ext::from_f64(160.0))][cast::<usize, _>(r.range(0, 2))];
    fill(&mut g, 6, oil.0, oil.1, &mut r);
    let mpb = cast::<Ext, _>(g.metals_price_base);
    let gold = [(mpb + Ext::from_f64(2400.0), mpb + Ext::from_f64(4000.0)), (mpb + Ext::from_f64(300.0), mpb + Ext::from_f64(1100.0)), (mpb + Ext::from_f64(800.0), mpb + Ext::from_f64(3000.0))][cast::<usize, _>(r.range(0, 2))];
    fill(&mut g, 7, gold.0, gold.1, &mut r);
    let silver = [(mpb * Ext::from_f64(0.01) + Ext::from_f64(29.0), mpb * Ext::from_f64(0.01) + Ext::from_f64(50.0)), (mpb * Ext::from_f64(0.01) + Ext::from_f64(5.0), mpb * Ext::from_f64(0.01) + Ext::from_f64(14.0)), (Ext::from_f64(5.0), Ext::from_f64(40.0))][cast::<usize, _>(r.range(0, 2))];
    fill(&mut g, 8, silver.0, silver.1, &mut r);
    let grain = [(Ext::from_f64(9.0), Ext::from_f64(14.0)), (Ext::from_f64(2.0), Ext::from_f64(4.9)), (Ext::from_f64(3.0), Ext::from_f64(10.0))][cast::<usize, _>(r.range(0, 2))];
    fill(&mut g, 9, grain.0, grain.1, &mut r);
    let idx = [(Ext::from_f64(2.4), Ext::from_f64(6.0)), (-Ext::from_f64(5.0), Ext::from_f64(0.3)), (-Ext::from_f64(3.0), Ext::from_f64(3.0)), (-Ext::from_f64(4.0), Ext::from_f64(5.0))][cast::<usize, _>(r.range(0, 3))];
    fill(&mut g, 1604, idx.0, idx.1, &mut r);
    if r.chance(Ext::from_f64(0.1)) {
        g.cap_hist_close[(48, 7)] = Ext::from_f64(0.0);
    }
    g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), Ext::from_f64(8.0)));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(2.5)));
    g.silver_price = cast::<f32, _>(cast::<f32, _>(mpb * Ext::from_f64(0.01) + f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(40.0))));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.gold_price = cast::<f32, _>(cast::<f32, _>(mpb + f64r(&mut r, Ext::from_f64(300.0), Ext::from_f64(3000.0))));
    g
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn gen_market_cycle_plan_short_game() {
    let mut g = Globals::default();
    g.start_year = 2000;
    g.game_end_year = 2010;
    let mut rng = Rng::from_state(5);
    assert_eq!(gen_market_cycle_plan(&mut g, &mut rng), "");
    assert_eq!(rng.state(), 5, "no RND for short games");
}

#[test]
fn gen_market_cycle_plan_shape() {
    let mut g = Globals::default();
    g.start_year = 2000;
    g.game_end_year = 2040;
    let mut found = 0;
    for st in 0..200u32 {
        let mut rng = Rng::from_state(st.wrapping_mul(7919));
        let p = gen_market_cycle_plan(&mut g, &mut rng);
        if p.is_empty() {
            continue;
        }
        found += 1;
        let v: Vec<i32> = p.to_display().split('|').map(|x| x.parse().unwrap()).collect();
        assert_eq!(v.len(), 4, "{p}");
        assert!(v[0] < v[1] && v[2] + 2 < v[3], "{p}");
        assert!(v[0] >= 2002 && v[2] >= 2002, "{p}");
    }
    assert!(found > 20);
}

#[test]
fn market_reversal_schedule_only() {
    let mut g = Globals::default();
    g.sim_co_count = 400;
    let mut rng = Rng::from_state(77);
    let mut ui = Scripted::default();
    market_reversal_event(&mut g, &mut ui, &mut rng, 0, 0);
    assert!(g.market_reversal_tick >= 600 && g.market_reversal_tick <= 950);
}

#[test]
fn gov_fiscal_policy_inflation() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(5.0));
    g.gdp_growth_target = cast::<f32, _>(Ext::from_f64(2.5));
    g.monetary_stance = 0;
    g.central_bank_name = s("ECB");
    for m in 1..=60 {
        g.cap_hist_close[(m, 1604)] = Ext::from_f64(5.0);
    }
    let mut rng = Rng::from_state(1);
    gov_fiscal_policy_news(&mut g, &mut rng);
    assert!(g.gdp_growth_target < Ext::from_f64(0.5));
    assert_eq!(g.news_msg, __content.string(14425));
}

// ------------------------------------------------------------------------------- differential tests

#[test]
fn diff_gov_fiscal_policy_news() {
    diff::par_seeds(1..=24, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 13 + 5);
        g.sim_co_count = if r.chance(Ext::from_f64(0.5)) { 610 } else { 611 };
        if r.chance(Ext::from_f64(0.5)) {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(10.0), -Ext::from_f64(2.0)));
        } else {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.5), Ext::from_f64(9.0)));
        }
        g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(6.0)));
        let fill = if r.chance(Ext::from_f64(0.5)) { -Ext::from_f64(3.0) } else { Ext::from_f64(5.0) };
        for m in 1..=60 {
            if r.chance(Ext::from_f64(0.6)) {
                g.cap_hist_close[(m, 1604)] = fill;
            }
        }
        check_skips(0x4c5d0b, &[], &g, (cast::<u32, _>(seed)).wrapping_mul(2654435761), &|g, _ui, rng| gov_fiscal_policy_news(g, rng));
    });
}

#[test]
fn diff_gen_market_cycle_plan() {
    diff::par_seeds(1..=24, |seed| {
        let mut g = macro_world(seed);
        g.game_end_year = g.start_year + [10, 15, 20, 30, 45, 60][cast::<usize, _>(seed % 6)];
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(2246822519);
        let opts = Opts { rng: Some(rng0), ..Opts::default() };
        let Some(o) = run_original_with(0x4c4b18, &opts, &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let plan = gen_market_cycle_plan(&mut port, &mut rng);
        assert_eq!(Some(plan), o.return_str.or(Some(PbStr::new())), "seed {seed}");
        assert_eq!(rng.state(), o.rng);
        cmp().assert(&port, &o.globals);
    });
}

#[test]
fn diff_market_aftershock() {
    diff::par_seeds(1..=12, |seed| {
        let mut g = macro_world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 31 + 1);
        g.market_aftershock_tick = [-700, 700, 30, -30][cast::<usize, _>(r.range(0, 3))];
        g.sim_co_count = [300, 450, 1500, 301][cast::<usize, _>(r.range(0, 3))];
        check_skips(0x4c3d74, &[], &g, (cast::<u32, _>(seed)).wrapping_mul(747796405), &|g, ui, rng| market_aftershock(g, ui, rng));
    });
}

#[test]
fn diff_market_reversal_event() {
    diff::par_seeds(1..=16, |seed| {
        let mut g = macro_world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 17 + 3);
        // a trending index so PriceTrendStreak(0) is not 0
        let cur = cast::<i32, _>(g.cap_hist_close[(0, 0)]);
        let up = r.chance(Ext::from_f64(0.5));
        let mut v = Ext::from_f64(1000.0);
        for k in 0..60 {
            let mut slot = cur - k;
            if slot < 1 {
                slot += 60;
            }
            g.cap_hist_close[(slot, 0)] = v;
            v *= if up { Ext::from_f64(0.97) } else { Ext::from_f64(1.03) };
        }
        g.rand_seed = cast::<i32, _>(r.range(0, 30000));
        let do_rev = if seed % 5 == 0 { 0 } else { 1 };
        check_skips(0x4c3454, &[do_rev], &g, (cast::<u32, _>(seed)).wrapping_mul(2891336453), &|g, ui, rng| {
            market_reversal_event(g, ui, rng, 0, do_rev)
        });
    });
}

#[test]
fn diff_macro_outlook_news() {
    diff::par_seeds(1..=48, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 7 + 11);
        // avoid the (CurYear + CurQuarter) MOD 4 = 3 skip most of the time
        if (g.cur_year + g.cur_quarter) % 4 == 3 && r.chance(Ext::from_f64(0.8)) {
            g.cur_quarter = g.cur_quarter % 4 + 1;
        }
        if r.chance(Ext::from_f64(0.8)) {
            g.pandemic_state = 0;
            g.subprime_crisis_state = 0;
            if g.peak_oil_crisis == 1 && r.chance(Ext::from_f64(0.5)) {
                g.peak_oil_crisis = 0;
            }
        }
        check_skips(0x4c1bc7, &[], &g, (cast::<u32, _>(seed)).wrapping_mul(1103515245).wrapping_add(12345), &|g, ui, rng| macro_outlook_news(g, ui, rng));
    });
}

#[test]
fn diff_plan_market_cycle() {
    diff::par_seeds(1..=64, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 101 + 7);
        if r.chance(Ext::from_f64(0.8)) {
            g.bubble_state = 0;
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f64(0.0));
            g.pandemic_state = 0;
            g.subprime_crisis_state = 0;
        }
        g.sim_co_count = [101, 301, 701, 1101, 301, 701, 1101, 401, 1501, 300][cast::<usize, _>(r.range(0, 9))];
        match seed % 4 {
            0 => {
                g.start_year = g.cur_year;
                g.cur_quarter = cast::<i32, _>(r.range(1, 4));
            }
            _ => {
                if g.start_year == g.cur_year {
                    g.start_year -= cast::<i32, _>(r.range(1, 20));
                }
            }
        }
        if r.chance(Ext::from_f64(0.6)) && g.next_market_cycle_year >= g.cur_year - 1 && g.next_market_cycle_year <= g.cur_year {
            g.next_market_cycle_year = g.cur_year + 3;
        }
        g.rand_seed = cast::<i32, _>(r.range(0, 30000));
        check_skips(0x4bfbd3, &[], &g, (cast::<u32, _>(seed)).wrapping_mul(3266489917), &|g, ui, rng| {
            let active = |g: &Globals| (131..=1600).filter(|&e| g.entity_status[e] == 1).count();
            let before = active(g);
            plan_market_cycle(g, ui, rng, 0);
            if active(g) > before {
                NEW_CORPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
    });
    let n = NEW_CORPS.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("new companies started: {n}");
    if diff::unavailable().is_none() && std::env::var_os("SEEDS").is_none() {
        assert!(n > 0, "StartNewCorpOrExpand not reached");
    }
}

/// Runs of PlanMarketCycle that started a company (StartNewCorpOrExpand).
static NEW_CORPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

