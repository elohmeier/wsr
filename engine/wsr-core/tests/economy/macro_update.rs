//! Tests of logic::economy::macro_update: UpdateEconomy (0x458a7a), UpdateInterestRates (0x4af1d2),
//! UpdateLoanLimitMultiple (0x4bd563), GovtTaxPolicyEvents (0x45d2c8), SweepPlayerCashToLoan (0x508f5c).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{
    govt_tax_policy_events, sweep_player_cash_to_loan, update_economy, update_interest_rates,
    update_loan_limit_multiple,
};

/// Routines owned by other economy submodules: the original skips them (their ports are no-op stubs
/// until merged). UpdateIndustryOutlook, UpdateCryptoPrices, MacroOutlookNews.
const PEER_SKIPS: &[(u32, u32)] = &[];

/// The shared world plus the macro globals these routines read.
fn macro_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x51_7cc1_b727_220a) | 1);
    g.game_in_progress = if r.chance(Ext::from_f64(0.6)) { 0 } else { 1 };
    g.gdp_level = r.range(5_000, 200_000);
    g.housing_starts = cast::<i32, _>(r.range(100, 2500));
    g.gold_price = cast::<f32, _>(if r.chance(Ext::from_f64(0.5)) { f32r(&mut r, Ext::from_f64(6000.0), Ext::from_f64(20000.0)) } else { f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(8000.0)) });
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(80.0)));
    g.metals_price_base = cast::<i32, _>(r.range(0, 3000));
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(2.5)));
    g.oil_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(2.5)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.market_val_target = cast::<f64, _>(f64r(&mut r, Ext::from_f64(0.3), Ext::from_f64(1.8)));
    g.market_valuation = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.6)));
    g.monetary_policy = cast::<i16, _>(r.range(-1, 2));
    g.monetary_stance = cast::<i32, _>(r.range(-1, 2));
    g.central_bank_name = s("The Fed");
    g.holding_co_tax_rate = [0, 0, -1, 1, 2, 3, 4][cast::<usize, _>(r.range(0, 6))];
    g.yr_holding_co_tax_chg = if r.chance(Ext::from_f64(0.3)) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 6)) };
    g.news_msg = if r.chance(Ext::from_f64(0.3)) { s("Some pending news") } else { PbStr::new() };
    g.credit_line_news_done = cast::<i32, _>(r.range(0, 1));
    g.rand_seed = cast::<i32, _>(r.range(-50, 30000));
    if r.chance(Ext::from_f64(0.5)) {
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), -Ext::from_f64(3.0)));
        g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(20.0)));
        g.interest_rate_target = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.0), Ext::from_f64(20.0)));
    }
    for ind in 1..=71 {
        g.ind_growth_rate_now[ind] = cast::<i16, _>(r.range(-5, 30));
    }
    for p in 1..=5 {
        g.sweep_cash_to_loan[p] = cast::<i32, _>(r.range(0, 1));
        if r.chance(Ext::from_f64(0.1)) {
            g.player_name[p] = s("...OUT...");
        }
    }
    for b in 11..=85 {
        g.earnings_y1[b] = if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(0.0) } else { f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(400.0)) };
        if r.chance(Ext::from_f64(0.2)) {
            g.corp_prior_net_worth[b] = Ext::from_f64(0.0);
        }
    }
    g
}

/// Branches the differential states reached (by the original's results), per test.
static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn seen(tag: &str) {
    SEEN.lock().unwrap().push(tag.to_string());
}

#[track_caller]
fn assert_seen(prefix: &str, tags: &[&str]) {
    let __content = wsr_core::content::current();
    let v = SEEN.lock().unwrap();
    let missing: Vec<&&str> = tags.iter().filter(|t| !v.contains(&format!("{prefix}:{t}"))).collect();
    assert!(missing.is_empty(), "no differential state of {prefix}{__wsr_text_13767}{missing:?}",__wsr_text_13767=__content.string(13767));
}

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
    let port_news = ui.log.iter().filter(|l| l.starts_with("NEWS")).count();
    let _ = port_news;
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
    Some(o.globals)
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn sweep_player_cash_unit() {
    let mut g = Globals::default();
    g.num_players = 3;
    for p in 1..=3 {
        g.player_name[p] = s("P");
        g.sweep_cash_to_loan[p] = 1;
        g.lending_bank[p] = 20;
    }
    g.cash[1] = Ext::from_f64(100.0);
    g.bank_debt[1] = Ext::from_f64(40.0);
    g.cash[2] = Ext::from_f64(30.0);
    g.bank_debt[2] = Ext::from_f64(50.0);
    g.cash[3] = Ext::from_f64(10.0);
    g.t_bills[3] = Ext::from_f64(5.0);
    sweep_player_cash_to_loan(&mut g);
    assert_eq!((g.cash[1], g.bank_debt[1]), (Ext::from_f64(60.0), Ext::from_f64(0.0)));
    assert_eq!((g.cash[2], g.bank_debt[2]), (Ext::from_f64(0.0), Ext::from_f64(20.0)));
    assert_eq!((g.cash[3], g.t_bills[3]), (Ext::from_f64(0.0), Ext::from_f64(15.0)));
    assert_eq!(g.cash[20], Ext::from_f64(70.0));
}

#[test]
fn loan_limit_multiple_unit() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    g.prime_rate = cast::<f32, _>(Ext::from_f64(10.0)); // adj = -1.4 -> ROUND(23.6) = 24
    update_loan_limit_multiple(&mut g, &mut ui, &mut rng);
    assert!((g.loan_limit_multiple - Ext::from_f64(2.4)).abs() < Ext::from_f64(1e-6));
    g.prime_rate = cast::<f32, _>(Ext::from_f64(5.0)); // adj = 3.4 -> 28
    g.loan_limit_multiple = cast::<f64, _>(Ext::from_f64(0.0));
    update_loan_limit_multiple(&mut g, &mut ui, &mut rng);
    assert!((g.loan_limit_multiple - Ext::from_f64(2.8)).abs() < Ext::from_f64(1e-6));
}

// ------------------------------------------------------------------------------- differential tests

#[test]
fn diff_update_interest_rates() {
    diff::par_seeds(1..=24, |seed| {
        let mut g = macro_world(seed);
        if seed % 3 == 0 {
            g.long_bond_desc = PbStr::new();
        }
        if seed % 4 == 1 {
            g.sim_co_count = 349; // not a multiple of 35: only with SofrRate <= 0
            g.sofr_rate = cast::<f32, _>(if seed % 8 == 1 { Ext::from_f64(0.0) } else { Ext::from_f64(3.0) });
        } else {
            g.sim_co_count = 350;
        }
        if let Some(o) = check_skips(0x4af1d2, &g, cast::<u32, _>(seed) * 7919, &|g, _ui, rng| update_interest_rates(g, rng)) {
            seen(if o.sofr_rate != g.sofr_rate { "ir:ran" } else { "ir:skipped" });
            if o.prime_rate != g.prime_rate {
                seen("ir:prime");
            }
        }
    });
    if diff::unavailable().is_none() {
        assert_seen("ir", &["ran", "skipped", "prime"]);
    }
}

#[test]
fn diff_update_loan_limit_multiple() {
    diff::par_seeds(1..=16, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed + 99);
        g.prime_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(20.0)));
        g.loan_limit_multiple = cast::<f64, _>([Ext::from_f64(0.0), Ext::from_f64(2.0), Ext::from_f64(2.5), Ext::from_f64(3.0)][cast::<usize, _>(seed % 4)]);
        let o = check_skips(0x4bd563, &g, cast::<u32, _>(seed) * 31, &|g, ui, rng| update_loan_limit_multiple(g, ui, rng));
        if o.is_some_and(|o| o.credit_line_news_done != g.credit_line_news_done) {
            seen("ll:news");
        }
    });
    if diff::unavailable().is_none() {
        assert_seen("ll", &["news"]);
    }
}

#[test]
fn diff_update_economy() {
    init_files();
    diff::par_seeds(1..=40, |seed| {
        let g = macro_world(seed);
        if let Some(o) = check_skips(0x458a7a, &g, cast::<u32, _>(seed) * 104729, &|g, ui, rng| update_economy(g, ui, rng)) {
            if g.game_in_progress != 1 && (1..=71).any(|i| i != 37 && o.ind_growth_forecast[i] != g.ind_growth_forecast[i]) {
                seen("ue:forecasts");
            }
            if g.holding_co_tax_rate == 0 && o.holding_co_tax_rate > 0 {
                seen("ue:hctax");
            }
            if g.holding_co_tax_rate > 0 && o.holding_co_tax_rate != g.holding_co_tax_rate {
                seen("ue:hcraise");
            }
            if o.monetary_policy != g.monetary_policy {
                seen("ue:qe");
            }
        }
    });
    if diff::unavailable().is_none() {
        assert_seen("ue", &["forecasts", "hctax", "hcraise", "qe"]);
    }
}

#[test]
fn diff_govt_tax_policy_events() {
    init_files();
    diff::par_seeds(1..=48, |seed| {
        let mut g = macro_world(seed);
        let mut r = Rand(seed * 977 + 5);
        g.sim_co_count = [495, 990, 1485, 594, 891, 1188, 450, 600][cast::<usize, _>(seed % 8)];
        g.next_new_tax_year = if r.chance(Ext::from_f64(0.8)) { 0 } else { g.cur_year + 1 };
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(14.0), Ext::from_f64(9.0)));
        g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), Ext::from_f64(8.0)));
        g.long_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(40.0), Ext::from_f64(130.0)));
        g.econ_alert_cooldown = [0, 0, 5][cast::<usize, _>(r.range(0, 2))];
        g.oil_tax_cut_cooldown = [0, 0, 3][cast::<usize, _>(r.range(0, 2))];
        let rate = |r: &mut Rand| [0, 0, -1, 1, 2, 3, 4, 5][cast::<usize, _>(r.range(0, 7))];
        g.wealth_tax_rate = rate(&mut r);
        g.health_care_tax_rate = rate(&mut r);
        g.carbon_tax_rate = rate(&mut r);
        g.oil_windfall_tax_rate = rate(&mut r);
        for y in [&mut g.yr_wealth_tax_chg, &mut g.yr_health_tax_chg, &mut g.yr_carbon_tax_chg, &mut g.yr_oil_windfall_tax_chg] {
            *y = if r.chance(Ext::from_f64(0.3)) { 0 } else { g.cur_year - cast::<i32, _>(r.range(0, 5)) };
        }
        g.difficulty_level = cast::<i32, _>(r.range(1, 4));
        g.currency_code = s(["UK", "USA", "AUS", "CAN", "EUR"][cast::<usize, _>(r.range(0, 4))]);
        g.currency_symbol = s(" $ ");
        g.million_prefix_lc = s("million");
        g.peak_oil_crisis = cast::<i16, _>(r.range(0, 1));
        g.game_end_year = g.cur_year + cast::<i32, _>(r.range(0, 2));
        for p in 1..=5 {
            g.is_computer_player[p] = cast::<i32, _>(r.range(0, 1));
        }
        let scc = g.sim_co_count;
        if scc <= 1600 && r.chance(Ext::from_f64(0.6)) {
            g.industry_num[scc] = [7, 8, 63][cast::<usize, _>(r.range(0, 2))];
            g.ent_misc_data[scc] = s(&format!("{}{}", "|".repeat(29), r.range(0, 90)));
        }
        if r.chance(Ext::from_f64(0.5)) {
            g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.2), Ext::from_f64(1.0)));
        }
        let cp = g.cur_player;
        g.cash[cp] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        if seed % 3 == 0 {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(12.0), -Ext::from_f64(4.5)));
            g.wealth_tax_rate = 0;
            g.start_year = g.cur_year - 5;
            g.cash[cp] = Ext::from_f64(3000.0);
            g.rand_seed = 7 * cast::<i32, _>(r.range(0, 1000)) + 3;
            g.difficulty_level = 3;
        }
        if seed % 6 == 1 {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.5), Ext::from_f64(9.0)));
            g.carbon_tax_rate = 2;
            g.yr_carbon_tax_chg = g.cur_year - 3;
            g.econ_alert_cooldown = 0;
        }
        if seed % 6 == 5 {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, Ext::from_f64(5.5), Ext::from_f64(9.0)));
            g.carbon_tax_rate = 0;
            g.start_year = g.cur_year - 5;
            g.difficulty_level = 4;
            g.rand_seed = 7 * cast::<i32, _>(r.range(0, 1000)) + 4;
        }
        if seed % 6 == 3 {
            g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(14.0), -Ext::from_f64(8.5)));
            g.wealth_tax_rate = 2;
            g.yr_wealth_tax_chg = g.cur_year - 5;
            g.econ_alert_cooldown = 0;
        }
        if let Some(o) = check_skips(0x45d2c8, &g, cast::<u32, _>(seed) * 7, &|g, ui, rng| govt_tax_policy_events(g, ui, rng)) {
            for (tag, a, b) in [
                ("wealth", g.wealth_tax_rate, o.wealth_tax_rate),
                ("health", g.health_care_tax_rate, o.health_care_tax_rate),
                ("carbon", g.carbon_tax_rate, o.carbon_tax_rate),
                ("oil", g.oil_windfall_tax_rate, o.oil_windfall_tax_rate),
            ] {
                if a != b {
                    seen(&format!("gt:{tag}{}", if a == 0 { "new" } else { "chg" }));
                }
            }
            if o.gdp_growth != g.gdp_growth {
                seen("gt:gdp");
            }
            if (1..=71).any(|i| o.ind_growth_rate[i] != g.ind_growth_rate[i]) {
                seen("gt:growth");
            }
        }
    });
    if diff::unavailable().is_none() {
        assert_seen("gt", &["wealthnew", "wealthchg", "healthnew", "healthchg", "carbonnew", "carbonchg", "oilnew", "oilchg", "gdp", "growth"]);
    }
}
