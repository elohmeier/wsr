//! Tests of logic::economy::world_events: RandomWorldEvents (0x52d793), EventConfiscateGold (0x6630ab),
//! AddRemovePhysCommodRec (0x65f1bf).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{add_remove_phys_commod_rec, event_confiscate_gold, random_world_events};

/// Routines of other economy submodules the original must skip until their ports are merged
/// (`(address, stdcall dword args)`): CryptoBanRumorEvent, ShockCommodityPrice, CountryDisasterShock.
const PEER_SKIPS: &[(u32, u32)] = &[];

/// Physical-commodity records (gold for players, companies of various kinds, other commodities).
fn phys_recs(g: &mut Globals, r: &mut Rand) {
    let mut n = 0;
    for i in 1..=cast::<i32, _>(r.range(0, 30)) {
        let code = ["GOLD", "GOLD", " GOLD", "OIL", "SILVER", "WHEAT"][cast::<usize, _>(r.range(0, 5))];
        let owner = match r.range(0, 3) {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => loop {
                let e = cast::<i32, _>(r.range(131, 1580));
                if g.entity_status[e] == 1 {
                    break e;
                }
            },
        };
        g.phys_commod_rec[i] =
            s(&format!("{code}|{owner}|{}|{:.2}", r.range(1, 5000), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
        n = i;
    }
    g.phys_commod_rec[0] = s(&n.to_string());
}

/// A world for RandomWorldEvents: the shared world plus the macro state the events read, steered to
/// event `target` (every other event blocked with `EventState = 5`).
fn rwe_world(seed: u64, target: i32) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    pow_safe(&mut g, seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x5bd1_e995);
    g.cur_quarter = [1, 3, 4][cast::<usize, _>(r.range(0, 2))];
    g.start_year = g.cur_year - cast::<i32, _>(r.range(0, 20));
    if g.cur_quarter == 4 {
        // keep the PB_Pow-safe quarters for CalcEntityFinancials (tests/finance.rs)
        g.cur_quarter = 3;
    }
    for i in 1..=200 {
        g.event_state[i] = 5;
    }
    g.event_state[target] = if r.chance(Ext::from_f64(0.7)) { 0 } else { cast::<i32, _>(r.range(1, 4)) };
    if target == 32 || (33..=37).contains(&target) {
        g.event_state[32] = [0, 0, 9, 1, 2, 3, 4, 5, 6, 7][cast::<usize, _>(r.range(0, 9))];
        if target == 37 && r.chance(Ext::from_f64(0.5)) {
            g.event_state[32] = 7;
        }
    }
    if target == 86 || target == 94 {
        g.event_state[target - 1] = if r.chance(Ext::from_f64(0.8)) { 9 } else { 0 };
    }
    if target == 20 {
        g.event_state[19] = if r.chance(Ext::from_f64(0.8)) { 9 } else { 0 };
    }
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 60));
    g.cap_gains_tax_rate = cast::<i32, _>(r.range(5, 40));
    g.gdp_growth = cast::<f32, _>([f32r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(8.0)), f32r(&mut r, -Ext::from_f64(2.0), Ext::from_f64(6.0)), f32r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4.0))][cast::<usize, _>(r.range(0, 2))]);
    g.gdp_growth_target = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(6.0)));
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(14.0)));
    g.monetary_policy = cast::<i16, _>(r.range(-1, 2));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.housing_starts = cast::<i32, _>(r.range(100, 3000));
    g.econ_cycle_counter = cast::<i16, _>(r.range(-5, 20));
    let crisis = r.range(0, 5);
    g.peak_oil_crisis = if crisis == 1 { [1, -1][cast::<usize, _>(r.range(0, 1))] } else { 0 };
    g.subprime_crisis_state = if crisis == 2 { cast::<i16, _>(r.range(-2, 5)) } else { 0 };
    g.pandemic_state = if crisis == 3 { cast::<i16, _>(r.range(-3, 6)) } else { 0 };
    g.oil_crisis_stage = if r.chance(Ext::from_f64(0.2)) { 20 } else { cast::<i32, _>(r.range(0, 19)) };
    g.oil_crisis_level = cast::<f32, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { Ext::from(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(3.5))) });
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.4), Ext::from_f64(2.2)));
    g.oil_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.6), Ext::from_f64(2.0)));
    g.wheat_price = cast::<f32, _>([f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(6.4)), f32r(&mut r, Ext::from_f64(6.5), Ext::from_f64(8.2)), f32r(&mut r, Ext::from_f64(8.3), Ext::from_f64(12.0))][cast::<usize, _>(r.range(0, 2))]);
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(9.0)));
    g.wheat_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.bubble_state = [0, -1, 0, 50, 101, 105, 150, 200, 205, 208, 210][cast::<usize, _>(r.range(0, 10))];
    g.crypto_ban_state = [0, 0, -1, 1, 2][cast::<usize, _>(r.range(0, 4))];
    g.bitcoin_price = if r.chance(Ext::from_f64(0.5)) { cast::<i32, _>(r.range(100_000, 200_000)) } else { cast::<i32, _>(r.range(1000, 90_000)) };
    g.corp_shares_tax_country = if target == 98 { cast::<i32, _>(r.range(-1, 52)) } else { -1 };
    g.world_event_state = [0, 0, 0, 1, -1][cast::<usize, _>(r.range(0, 4))];
    g.metals_price_base = cast::<i32, _>(r.range(50, 3000));
    g.drug_cancel_news_done = cast::<i32, _>(r.range(0, 1));
    g.last_rand_event_ind = cast::<i32, _>(r.range(0, 69));
    g.currency_code = s(["USA", "UK", "JAP", "SWE", "ICE", "SWI", "EUR"][cast::<usize, _>(r.range(0, 6))]);
    g.currency_symbol = s(["$", " EUR ", "£"][cast::<usize, _>(r.range(0, 2))]);
    g.currency_name = s(["dollar", "European euro", "yen"][cast::<usize, _>(r.range(0, 2))]);
    g.currency_suffix = s(["", " Euro"][cast::<usize, _>(r.range(0, 1))]);
    g.million_prefix_lc = s("m");
    g.central_bank_name = s([__content.string(13428), __content.string(6645)][cast::<usize, _>(r.range(0, 1))]);
    g.news_msg = if r.chance(Ext::from_f64(0.2)) { s(__content.string(13571)) } else { PbStr::new() };
    for c in 0..=55 {
        g.country_name[c] = s(&format!("COUNTRY NO {c}"));
        g.country_display_name[c] = s(&format!("{__wsr_text_13770}{c}",__wsr_text_13770=__content.string(13770)));
    }
    for i in 0..=71 {
        g.ind_name[i] = s(&format!("{__wsr_text_13675}{i}",__wsr_text_13675=__content.string(13675)));
    }
    for e in 11..=1600 {
        if r.chance(Ext::from_f64(0.03)) {
            g.industry_num[e] = [56, 4, 21, 3, 16, 17, 20, 57, 69, 50][cast::<usize, _>(r.range(0, 9))];
        }
        g.ent_name[e] = s(["Acme", "A Very Long Company Name Inc", "Big Pharma Corp"][cast::<usize, _>(r.range(0, 2))]);
    }
    for id in 1..=1600 {
        if r.chance(Ext::from_f64(0.3)) {
            g.long_gov_bond_face[id] = r.range(0, 500);
            g.lt_gov_bond_cost[id] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
            g.short_gov_bond_face[id] = r.range(0, 500);
            g.st_gov_bond_cost[id] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        }
    }
    for k in 1..=60 {
        g.price_hist_low[(k, 1604)] = if r.chance(Ext::from_f64(0.003)) { -Ext::from_f64(3.0) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)) };
    }
    phys_recs(&mut g, &mut r);
    steer(&mut g, &mut r, target);
    g
}

/// Per-event state that makes the event's conditions hold more often.
fn steer(g: &mut Globals, r: &mut Rand, target: i32) {
    let __content = wsr_core::content::current();
    if target % 2 == 0 && r.chance(Ext::from_f64(0.15)) {
        return;
    }
    match target {
        11 => g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(3.0), Ext::from_f64(0.9))),
        12 | 21 => g.gdp_growth = cast::<f32, _>(f32r(r, Ext::from_f64(3.6), Ext::from_f64(6.0))),
        17 => {
            g.peak_oil_crisis = 0;
            g.pandemic_state = 0;
            g.monetary_policy = 2;
            g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(10.0), -Ext::from_f64(1.0)));
            g.gold_price = cast::<f32, _>(f32r(r, Ext::from_f64(500.0), Ext::from_f64(3000.0)));
            // a steadily falling series 7
            let cur = cast::<i32, _>(g.cap_hist_close[(0, 7)]);
            let cur = if cur < 1 { 1 } else { cur };
            g.cap_hist_close[(0, 7)] = cast::<Ext, _>(cur);
            for k in 0..60 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                let v = Ext::from_f64(100.0) * Ext::from_f64(1.06f64).powi(k);
                g.cap_hist_close[(slot, 7)] = v;
                g.price_hist_high[(slot, 7)] = v * Ext::from_f64(1.01);
                g.price_hist_low[(slot, 7)] = v * Ext::from_f64(0.99);
            }
        }
        16 | 18 => {
            g.monetary_policy = 0;
            g.gdp_growth = cast::<f32, _>(f32r(r, Ext::from_f64(1.5), Ext::from_f64(5.0)));
        }
        28 | 29 | 101 => {
            g.gold_price = cast::<f32, _>(f32r(r, Ext::from_f64(20000.0), Ext::from_f64(45000.0)));
            g.gdp_growth = cast::<f32, _>(if target == 101 { f32r(r, -Ext::from_f64(5.0), Ext::from_f64(0.5)) } else { f32r(r, Ext::from_f64(2.0), Ext::from_f64(6.0)) });
        }
        80 => {
            g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(5.0), Ext::from_f64(2.0)));
            g.event_state[76] = 5;
        }
        81 | 82 | 95 | 96 | 97 => {
            g.peak_oil_crisis = if target == 96 || target == 97 { 1 } else { 0 };
            g.subprime_crisis_state = 0;
            g.pandemic_state = 0;
            g.start_year = g.cur_year - cast::<i32, _>(r.range(5, 8));
            g.oil_price = cast::<f32, _>(f32r(r, Ext::from_f64(0.85), Ext::from_f64(2.0)));
        }
        84 => {
            for e in 144..=183 {
                if r.chance(Ext::from_f64(0.6)) {
                    g.entity_status[e] = 1;
                    g.industry_num[e] = 4;
                    g.business_assets[e] = f64r(r, Ext::from_f64(100.0), Ext::from_f64(5000.0));
                }
            }
        }
        85 | 86 => {
            g.peak_oil_crisis = 0;
            g.start_year = g.cur_year - cast::<i32, _>(r.range(8, 13));
            g.cur_quarter = 3;
            g.sim_co_count = 450;
        }
        87 => {
            g.base_interest_rate = cast::<f32, _>(f32r(r, Ext::from_f64(11.5), Ext::from_f64(16.0)));
            g.corp_tax_rate = cast::<i32, _>(r.range(40, 60));
            g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(14.0), Ext::from_f64(1.0)));
        }
        88..=90 => {
            g.drug_cancel_news_done = 0;
            g.last_rand_event_ind = 0;
        }
        91 => {
            g.base_interest_rate = cast::<f32, _>(f32r(r, Ext::from_f64(1.0), Ext::from_f64(4.75)));
            g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(1.0), Ext::from_f64(1.3)));
            g.gdp_growth_target = cast::<f32, _>(g.gdp_growth - Ext::from_f64(1.0));
            g.monetary_policy = 0;
            g.central_bank_name = s(__content.string(13428));
        }
        98 => g.corp_shares_tax_country = cast::<i32, _>(r.range(0, 52)),
        26 | 27 => {
            g.base_interest_rate = cast::<f32, _>(f32r(r, Ext::from_f64(7.0), Ext::from_f64(14.0)));
            // a steadily rising series 1602
            let cur = (cast::<i32, _>(g.cap_hist_close[(0, 1602)])).max(1);
            g.cap_hist_close[(0, 1602)] = cast::<Ext, _>(cur);
            for k in 0..60 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                let v = Ext::from_f64(100.0) * Ext::from_f64(0.95f64).powi(k);
                g.cap_hist_close[(slot, 1602)] = v;
                g.price_hist_high[(slot, 1602)] = v * Ext::from_f64(1.01);
                g.price_hist_low[(slot, 1602)] = v * Ext::from_f64(0.99);
            }
        }
        77 => {
            g.start_year = g.cur_year - cast::<i32, _>(r.range(3, 15));
            g.gdp_growth = cast::<f32, _>(f32r(r, Ext::from_f64(1.5), Ext::from_f64(5.0)));
            g.oil_crisis_level = cast::<f32, _>(Ext::from_f64(0.0));
            g.pandemic_state = 0;
            g.subprime_crisis_state = 0;
        }
        83 => {
            g.start_year = g.cur_year - cast::<i32, _>(r.range(15, 25));
            g.peak_oil_crisis = [0, -1][cast::<usize, _>(r.range(0, 1))];
            g.oil_crisis_level = cast::<f32, _>(f32r(r, Ext::from_f64(0.0), Ext::from_f64(3.5)));
            g.gdp_growth = cast::<f32, _>(f32r(r, -Ext::from_f64(5.0), Ext::from_f64(1.8)));
        }
        93 | 94 => {
            g.start_year = g.cur_year - cast::<i32, _>(if target == 93 { r.range(0, 9) } else { r.range(10, 15) });
        }
        100 => {
            if r.chance(Ext::from_f64(0.5)) {
                g.oil_price = cast::<f32, _>(f32r(r, Ext::from_f64(1.16), Ext::from_f64(2.0)));
                g.corn_price = cast::<f32, _>(f32r(r, Ext::from_f64(2.0), Ext::from_f64(5.9)));
            } else {
                g.oil_price = cast::<f32, _>(f32r(r, Ext::from_f64(0.4), Ext::from_f64(0.79)));
                g.corn_price = cast::<f32, _>(f32r(r, Ext::from_f64(6.6), Ext::from_f64(9.0)));
            }
        }
        102 => {
            g.crypto_ban_state = 0;
            g.bitcoin_price = cast::<i32, _>(r.range(130_000, 400_000));
        }
        73 => {
            g.world_event_state = 0;
            g.start_year = g.cur_year - cast::<i32, _>(r.range(6, 9));
        }
        74 => {
            g.world_event_state = 0;
            g.cur_year = 2042 + cast::<i32, _>(r.range(0, 1));
        }
        41..=76 => g.world_event_state = 0,
        32..=36 => g.gdp_growth = cast::<f32, _>(f32r(r, Ext::from_f64(2.0), Ext::from_f64(5.2))),
        _ => {}
    }
}

/// An RND seed (tried with the port) for which the event does something: a return value, a headline or a
/// story. Falls back to `base`.
fn interesting_rng(g: &Globals, base: u32) -> u32 {
    for k in 0..48u32 {
        let rng0 = base.wrapping_add(k.wrapping_mul(0x9e37_79b9));
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Scripted::default();
        let ret = random_world_events(&mut port, &mut ui, &mut rng, 0);
        if ret != 0 || !ui.log.is_empty() {
            return rng0;
        }
    }
    base
}

fn run_rwe(g: &Globals, rng0: u32) -> Option<()> {
    let rng0 = interesting_rng(g, rng0);
    let opts = Opts { rng: Some(rng0), skips: PEER_SKIPS.to_vec(), ..Opts::default() };
    let o = run_original_with(0x52d793, &opts, g)?;
    let mut port = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Scripted::default();
    let ret = random_world_events(&mut port, &mut ui, &mut rng, 0);
    assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
    let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
    assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
    cmp().assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND sequence");
    assert_eq!(ret, cast::<i32, _>(o.return_value), "econ effect");
    Some(())
}

/// Every event number (1..103, CASE ELSE included; some twice) on varied states, steered to the event.
#[test]
fn diff_random_world_events() {
    init_files();
    diff::par_seeds(1..=124, |seed| {
        let target = cast::<i32, _>((seed - 1) % 103 + 1);
        let g = rwe_world(seed, target);
        run_rwe(&g, (cast::<u32, _>(seed)).wrapping_mul(2654435761));
    });
}

/// Unsteered: the event state as a game leaves it (most events unused).
#[test]
fn diff_random_world_events_unsteered() {
    init_files();
    diff::par_seeds(1..=12, |seed| {
        let mut g = rwe_world(seed + 1000, 1);
        let mut r = Rand(seed * 7 + 1);
        for i in 1..=103 {
            g.event_state[i] = if r.chance(Ext::from_f64(0.8)) { 0 } else { cast::<i32, _>(r.range(1, 9)) };
        }
        run_rwe(&g, (cast::<u32, _>(seed)).wrapping_mul(40503) ^ 0x1234);
    });
}

#[test]
fn diff_event_confiscate_gold() {
    diff::par_seeds(1..=12, |seed| {
        let g = rwe_world(seed + 5000, 1);
        let pct = 40 + (cast::<i32, _>(seed) * 7) % 21;
        let opts = Opts { args: vec![pct.to_string()], rng: Some(99), ..Opts::default() };
        let Some(o) = run_original_with(0x6630ab, &opts, &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(99);
        let mut ui = Scripted::default();
        let ret = event_confiscate_gold(&mut port, &mut ui, &mut rng, pct);
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng);
        assert_eq!(ret, cast::<i32, _>(o.return_value));
    });
}

#[test]
fn diff_add_remove_phys_commod_rec() {
    diff::par_seeds(1..=8, |seed| {
        let mut g = Globals::default();
        let mut r = Rand(seed * 3 + 11);
        let n = cast::<i32, _>(r.range(0, 6));
        for i in 1..=n {
            g.phys_commod_rec[i] = s(&format!("GOLD|{i}|1|2"));
        }
        g.phys_commod_rec[0] = s(&n.to_string());
        let idx = cast::<i32, _>(r.range(0, cast::<i64, _>(n.max(1))));
        let opts = Opts { args: vec!["s:SILVER|3|4|5".to_string(), idx.to_string()], ..Opts::default() };
        let Some(o) = run_original_with(0x65f1bf, &opts, &g) else { return };
        let mut port = g.clone();
        assert!(add_remove_phys_commod_rec(&mut port, b"SILVER|3|4|5", idx));
        cmp().assert(&port, &o.globals);
    });
}

#[test]
fn add_remove_phys_commod_rec_unit() {
    let mut g = Globals::default();
    g.phys_commod_rec[0] = s("3");
    g.phys_commod_rec[1] = s("A");
    g.phys_commod_rec[2] = s("B");
    g.phys_commod_rec[3] = s("C");
    assert!(add_remove_phys_commod_rec(&mut g, b"", 1));
    assert_eq!(g.phys_commod_rec[1], "C");
    assert_eq!(g.phys_commod_rec[3], "");
    assert_eq!(g.phys_commod_rec[0], "2");
    assert!(add_remove_phys_commod_rec(&mut g, b"D", 0));
    assert_eq!(g.phys_commod_rec[3], "D");
    assert_eq!(g.phys_commod_rec[0], "3");
}

#[test]
fn random_world_events_first_quarter_is_quiet() {
    let mut g = Globals::default();
    g.start_year = 2020;
    g.cur_year = 2020;
    g.cur_quarter = 1;
    g.sim_co_count = 450;
    let before = g.clone();
    let mut rng = Rng::from_state(5);
    let mut ui = Scripted::default();
    assert_eq!(random_world_events(&mut g, &mut ui, &mut rng, 0), 0);
    assert_eq!(rng.state(), 5);
    Compare::default().assert(&g, &before);
}
