//! Tests of `logic::futures`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::logic::futures::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

const COMMODS: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];

// ------------------------------------------------------------------------------------------- states

/// A plausible world with commodity prices, futures bases, futures records and physical holdings.
fn world(seed: u64) -> Globals {
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    let f32r = |r: &mut Rand, lo: Ext, hi: Ext| cast::<f32, _>(lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000)) / Ext::from_f64(1e6));
    let f64r = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.game_end_flag = s(if r.chance(Ext::from_f64(0.1)) { "Y" } else { "N" });
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.start_year = cast::<i32, _>(r.range(1990, 2010));
    g.cur_year = g.start_year + cast::<i32, _>(r.range(0, 40));
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = [0, 3, 200, 300, 600, 800, 1100, 1300, 1600][cast::<usize, _>(r.range(0, 8))];
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(18.0)));
    g.exchange_rate = cast::<f64, _>(if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.ticker_speed = [10, 60, 61, 100][cast::<usize, _>(r.range(0, 3))];
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)) * if r.chance(Ext::from_f64(0.1)) { -Ext::from_f64(1.0) } else { Ext::from_f64(1.0) });
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(25000.0)) * if r.chance(Ext::from_f64(0.1)) { -Ext::from_f64(1.0) } else { Ext::from_f64(1.0) });
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(400.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(19.0)));
    g.bitcoin_price = cast::<i32, _>(r.range(-100, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(10, 5000));
    g.stock_index = if r.chance(Ext::from_f64(0.1)) { -Ext::from_f64(5.0) } else { f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(160000.0)) };
    g.metals_price_base = cast::<i32, _>(r.range(0, 9000));
    g.pandemic_state = cast::<i16, _>(r.range(-1, 1));
    g.rand_seed = cast::<i32, _>(r.range(-100000, 100000));
    for k in 1..=10 {
        g.futures_basis[k] = cast::<i16, _>(r.range(-120, 320));
    }
    g.gold_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(25000.0)));
    g.silver_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(400.0)));
    g.wheat_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0)));
    g.corn_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(19.0)));
    g.cur_commodity = s(COMMODS[cast::<usize, _>(r.range(0, 7))]);
    for p in 1..=5 {
        g.is_computer_player[p] = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
    }
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(Ext::from_f64(0.93)) };
        g.entity_status[e] = if active { 1 } else { 0 };
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.commod_margin_bal[e] = if r.chance(Ext::from_f64(0.1)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1585 => 71,
                _ => [7, 8, 11, 15, 16, 23, 25, 28, 37, 50, 52, 69, 3, 40][cast::<usize, _>(r.range(0, 13))],
            };
        }
    }
    // futures records on a few owners (so per-entity routines find several)
    let owners: Vec<i64> = (0..8)
        .map(|k| match k % 4 {
            0 => r.range(1, cast::<i64, _>(g.num_players)),
            1 => r.range(131, 1580),
            2 => [1584, 1585][cast::<usize, _>(r.range(0, 1))],
            _ => r.range(11, 130),
        })
        .collect();
    let n = cast::<i32, _>(r.range(0, 40));
    for i in 1..=n {
        let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
        let owner = owners[cast::<usize, _>(r.range(0, 7))];
        let qty = r.range(1, 60) * if r.chance(Ext::from_f64(0.4)) { -1 } else { 1 };
        let year = g.cur_year + cast::<i32, _>(r.range(-1, 3));
        let cost = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(2000.0));
        g.futures_recs[i] = s(&format!("{c}|{owner}|{qty}|{}|{year}|{cost:.4}|{:.2}", r.range(1, 12), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(999.0)))));
    }
    g.futures_recs[0] = s(&n.to_string());
    let n = cast::<i32, _>(r.range(0, 30));
    for i in 1..=n {
        let c = ["GOLD", "SILVER", "OIL", "WHEAT", "CORN", "BITCOIN", "ETHEREUM", "XYZ"][cast::<usize, _>(r.range(0, 7))];
        let owner = owners[cast::<usize, _>(r.range(0, 7))];
        g.phys_commod_rec[i] = s(&format!("{c}|{owner}|{}|{:.2}", r.range(1, 50000), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    g.phys_commod_rec[0] = s(&n.to_string());
    g
}

/// Owner ids appearing in the futures records (field 2), plus a player and an unrelated corp.
fn owners(g: &Globals) -> Vec<i32> {
    let mut v: Vec<i32> = (1..=1000)
        .filter(|&i| !g.futures_recs[i].is_empty())
        .map(|i| g.futures_recs[i].to_display().split('|').nth(1).unwrap().parse().unwrap())
        .collect();
    v.sort();
    v.dedup();
    v.push(1);
    v.push(777);
    v
}

fn run(addr: u32, args: &[String], g: &Globals, rng: u32, skips: Vec<(u32, u32)>) -> Option<Original> {
    let opts = Opts { args: args.to_vec(), rng: Some(rng), skips, ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn msgs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe")).count()
}

fn port_msgs(ui: &Scripted) -> usize {
    ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count()
}

/// EXT is f64 in the port (80-bit in the original); the main window labels are UI.
fn cmp() -> Compare {
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

#[track_caller]
fn close(a: Ext, b: Ext, what: &str) {
    let d = (a - b).abs();
    assert!(d <= Ext::from_f64(1e-12) * a.abs().max(b.abs()) || d < Ext::from_f64(1e-12), "{what}: port {a} vs original {b}");
}

// ------------------------------------------------------------------------------------- unit tests

#[test]
fn commod_data_and_formatting() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    assert_eq!(get_commod_data(&mut g, &mut ui, &s(" GOLD "), 1), __content.string(5687));
    assert_eq!(get_commod_data(&mut g, &mut ui, &s("STOCKS"), 1), __content.string(5691));
    assert!(ui.log.is_empty());
    assert_eq!(get_commod_data(&mut g, &mut ui, &s("XX"), 7), "ERROR");
    assert_eq!(ui.log.len(), 1);
    assert!(ui.log[0].contains("T$ = XX  Z$ = XX Commodity$ =  X&= 7 COCNT = 0"), "{}", ui.log[0]);
    g.oil_price = cast::<f32, _>(Ext::from_f64(1.25));
    g.gold_price = cast::<f32, _>(Ext::from_f64(12345.5));
    g.bitcoin_price = 45678;
    assert_eq!(format_commod_price(&g, &s("OIL")), "125.00");
    assert_eq!(format_commod_price(&g, &s("GOLD")), "12,345.50");
    assert_eq!(format_commod_price(&g, &s("BITCOIN")), "45,678");
    assert_eq!(format_commod_price(&g, &s("STOCKS")), "");
}

#[test]
fn futures_price_carry() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    g.game_end_flag = s("N");
    g.cur_year = 2000;
    g.cur_quarter = 1;
    g.num_players = 1;
    g.sim_co_count = 1100; // month 3 (beyond the quarter's 500 steps)
    g.base_interest_rate = cast::<f32, _>(Ext::from_f64(6.0));
    g.futures_basis[4] = 100;
    g.wheat_price = cast::<f32, _>(Ext::from_f64(5.0));
    // 3 months out: 5 * (1 + 100 * 0.000065 * 6 / 12)^3
    let p = get_futures_price(&mut g, &mut ui, &mut rng, &s("WHEAT"), 6, 2000, 0);
    let rate = (cast::<Ext, _>(6.5e-05f32)) * Ext::from_f64(100.0) * Ext::from_f64(6.0) / Ext::from_f64(12.0);
    let expect = (cast::<Ext, _>(0.01f32)) * (Ext::from_f64(100.0) * (Ext::from_f64(1.0) + rate).powi(3)) * Ext::from_f64(5.0);
    assert_eq!(p.to_display(), format!("{expect:.2}"));
    // a past year counts as next year; small positive bases become 3
    g.futures_basis[4] = 2;
    let _ = get_futures_price(&mut g, &mut ui, &mut rng, &s("WHEAT"), 6, 1990, 0);
    assert_eq!(g.futures_basis[4], 3);
    // STOCKS outside the index range give ""
    g.stock_index = Ext::from_f64(200000.0);
    assert_eq!(get_futures_price(&mut g, &mut ui, &mut rng, &s("STOCKS"), 6, 2000, 0), "");
}

#[test]
fn add_remove_and_count() {
    let mut g = Globals::default();
    g.futures_recs[0] = s("0");
    add_or_remove_futures_rec(&mut g, &s("GOLD|1|5|3|2001|100|1"), 0);
    add_or_remove_futures_rec(&mut g, &s("OIL|1|-2|3|2001|100|1"), 0);
    add_or_remove_futures_rec(&mut g, &s("GOLD|2|1|3|2001|100|1"), 0);
    assert_eq!(g.futures_recs[0], "3");
    assert_eq!(count_futures_positions(&g, 1, 0), 2);
    g.cur_commodity = s("OIL");
    assert_eq!(count_futures_positions(&g, 1, 1), -1);
    g.cur_commodity = s("GOLD");
    assert_eq!(count_futures_positions(&g, 1, 1), 1);
    add_or_remove_futures_rec(&mut g, &s(""), 1);
    assert_eq!(g.futures_recs[0], "2");
    assert_eq!(g.futures_recs[1], "GOLD|2|1|3|2001|100|1", "last record moves into the gap");
    assert!(g.futures_recs[3].is_empty());
}

// ------------------------------------------------------------------------------- differential tests

#[test]
fn get_commod_data_matches_original() {
    let __content = wsr_core::content::current();
    for (k, name) in COMMODS.iter().chain(["  CORN ", "XYZ", ""].iter()).enumerate() {
        let mut g = world(cast::<u64, _>(k));
        let Some(o) = run(0x61c5aa, &[format!("{__wsr_text_13622}{name}",__wsr_text_13622=__content.string(13622)), (cast::<i32, _>(k)).to_string()], &g, 1, vec![]) else {
            return;
        };
        let mut ui = Scripted::default();
        let r = get_commod_data(&mut g, &mut ui, &s(name), cast::<i32, _>(k));
        assert_eq!(Some(r), o.return_str, "{name}");
        assert_eq!(port_msgs(&ui), msgs(&o), "{name}");
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn format_commod_price_matches_original() {
    let __content = wsr_core::content::current();
    for (k, name) in COMMODS.iter().chain(["XYZ"].iter()).enumerate() {
        let g = world(100 + cast::<u64, _>(k));
        let Some(o) = run(0x6657eb, &[format!("{__wsr_text_13622}{name}",__wsr_text_13622=__content.string(13622))], &g, 1, vec![]) else {
            return;
        };
        assert_eq!(Some(format_commod_price(&g, &s(name))), o.return_str.clone().or(Some(PbStr::new())), "{name}");
    }
}

#[test]
fn get_futures_price_matches_original() {
    let __content = wsr_core::content::current();
    for seed in 0..24u64 {
        let mut g = world(200 + seed);
        let mut r = Rand(seed + 7);
        let name = if seed == 23 { "XX" } else { COMMODS[cast::<usize, _>(seed % 8)] };
        let (m, y) = (cast::<i32, _>(r.range(1, 12)), g.cur_year + cast::<i32, _>(r.range(-1, 4)));
        let rng0 = cast::<u32, _>(seed * 7919 + 13);
        let args = [format!("{__wsr_text_13622}{name}",__wsr_text_13622=__content.string(13622)), m.to_string(), y.to_string(), "5".into()];
        let Some(o) = run(0x61b5b4, &args, &g, rng0, vec![]) else {
            return;
        };
        let mut ui = Scripted::default();
        let mut rng = Rng::from_state(rng0);
        let p = get_futures_price(&mut g, &mut ui, &mut rng, &s(name), m, y, 5);
        eprintln!("{name} {m}/{y}: {p}");
        assert_eq!(Some(p), o.return_str.clone().or(Some(PbStr::new())), "seed {seed} {name}");
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(port_msgs(&ui), msgs(&o));
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn calc_futures_contract_value_matches_original() {
    let mut n = 0;
    for seed in 0..10u64 {
        let g0 = world(300 + seed);
        for i in 1..=1000 {
            if g0.futures_recs[i].is_empty() || i % 3 != 0 {
                continue;
            }
            let mut g = g0.clone();
            let rec = g.futures_recs[i].clone();
            let rng0 = 4242 + cast::<u32, _>(i);
            let Some(o) = run(0x61b3c3, &[format!("s:{}", rec.to_display())], &g, rng0, vec![]) else {
                return;
            };
            let mut ui = Scripted::default();
            let mut rng = Rng::from_state(rng0);
            let v = calc_futures_contract_value(&mut g, &mut ui, &mut rng, &rec);
            close(v, o.return_float.unwrap(), &rec.to_display());
            assert_eq!(rng.state(), o.rng);
            cmp().assert(&g, &o.globals);
            n += 1;
        }
    }
    assert!(n > 10);
}

#[test]
fn calc_entity_futures_net_value_matches_original() {
    let __content = wsr_core::content::current();
    for seed in 0..6u64 {
        let g0 = world(400 + seed);
        for e in owners(&g0).into_iter().take(4) {
            let mut g = g0.clone();
            let rng0 = 99 + cast::<u32, _>(e);
            let Some(o) = run(0x61c244, &[e.to_string()], &g, rng0, vec![]) else {
                return;
            };
            let mut ui = Scripted::default();
            let mut rng = Rng::from_state(rng0);
            let v = calc_entity_futures_net_value(&mut g, &mut ui, &mut rng, e);
            close(v, o.return_float.unwrap_or(Ext::from_f64(0.0)), &format!("{__wsr_text_13847}{e}",__wsr_text_13847=__content.string(13847)));
            assert_eq!(rng.state(), o.rng);
            cmp().assert(&g, &o.globals);
        }
    }
}

#[test]
fn recalc_commod_margin_balances_matches_original() {
    let mut changed = 0;
    for seed in 0..8u64 {
        let g0 = world(500 + seed);
        let mut ents = owners(&g0);
        ents.truncate(2);
        ents.push(0);
        ents.push(-1);
        for e in ents {
            let mut g = g0.clone();
            let rng0 = 5 + cast::<u32, _>(seed);
            let Some(o) = run(0x61c8fc, &[e.to_string()], &g, rng0, vec![]) else {
                return;
            };
            let mut ui = Scripted::default();
            let mut rng = Rng::from_state(rng0);
            recalc_commod_margin_balances(&mut g, &mut ui, &mut rng, e);
            assert_eq!(rng.state(), o.rng);
            cmp().assert(&g, &o.globals);
            changed += cast::<i32, _>(!Compare::default().diffs(&g0, &o.globals).is_empty());
        }
    }
    assert!(changed > 10, "{changed}");
}

#[test]
fn count_futures_positions_matches_original() {
    for seed in 0..4u64 {
        let g = world(600 + seed);
        for e in owners(&g).into_iter().take(3) {
            for mode in [0, 1, 2] {
                let Some(o) = run(0x633d45, &[e.to_string(), mode.to_string()], &g, 1, vec![]) else {
                    return;
                };
                assert_eq!(count_futures_positions(&g, e, mode), cast::<i32, _>(o.return_value), "entity {e} mode {mode}");
            }
        }
    }
}

#[test]
fn sort_futures_recs_matches_original() {
    for seed in 0..6u64 {
        let mut g = world(700 + seed);
        if seed == 5 {
            g.futures_recs[3] = PbStr::new(); // a gap: PackHoldingArrays closes it first
        }
        let Some(o) = run(0x634002, &[], &g, 1, vec![]) else {
            return;
        };
        let g0 = g.clone();
        let ev = sort_futures_recs(&mut g);
        assert!(!Compare::default().diffs(&g0, &o.globals).is_empty(), "nothing sorted");
        assert!(ev.is_empty());
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn add_or_remove_futures_rec_matches_original() {
    for seed in 0..8u64 {
        let mut g = world(800 + seed);
        let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
        let idx = match seed % 4 {
            0 => 0,
            1 => n.max(1),
            2 => (n / 2).max(1),
            _ => -3,
        };
        let rec = format!("GOLD|{}|7|5|2001|123.5|9", seed + 1);
        let Some(o) = run(0x62a37e, &[format!("s:{rec}"), idx.to_string()], &g, 1, vec![]) else {
            return;
        };
        add_or_remove_futures_rec(&mut g, &s(&rec), idx);
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn sum_commodity_holdings_matches_original() {
    for seed in 0..6u64 {
        let mut g = world(900 + seed);
        for (k, e) in owners(&g).into_iter().take(4).enumerate() {
            g.cur_commodity = s(["STOCKS", "GOLD", "OIL", "CORN", "SILVER", "WHEAT"][(k + cast::<usize, _>(seed)) % 6]);
            for (fm, pm) in [(0, 0), (0, 2), (1, 0), (2, 0), (2, 2)] {
                let args = [e.to_string(), fm.to_string(), pm.to_string()];
                let Some(o) = run(0x6395e2, &args, &g, 1, vec![]) else {
                    return;
                };
                let v = sum_commodity_holdings(&g, e, fm, pm);
                assert_eq!(cast::<Ext, _>(v), o.return_float.unwrap(), "entity {e} modes {fm}/{pm}");
            }
        }
    }
}

#[test]
fn calc_phys_commod_storage_cost_matches_original() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    for seed in 0..6u64 {
        let g0 = world(1000 + seed);
        let mut ents: Vec<i32> = (1..=1000)
            .filter(|&i| !g0.phys_commod_rec[i].is_empty())
            .map(|i| g0.phys_commod_rec[i].to_display().split('|').nth(1).unwrap().parse().unwrap())
            .collect();
        ents.sort();
        ents.dedup();
        ents.push(999);
        for e in ents.into_iter().take(4) {
            let mut g = g0.clone();
            let Some(o) = run(0x6616d9, &[e.to_string()], &g, 1, vec![]) else {
                return;
            };
            let v = calc_phys_commod_storage_cost(&mut g, e);
            close(v, o.return_float.unwrap_or(Ext::from_f64(0.0)), &format!("{__wsr_text_13847}{e}",__wsr_text_13847=__content.string(13847)));
            cmp().assert(&g, &o.globals);
            n += cast::<i32, _>(v != Ext::from_f64(0.0));
        }
    }
    assert!(n > 3);
}

#[test]
fn estimate_corp_hedge_gains_matches_original() {
    let mut n = 0;
    for seed in 0..10u64 {
        let g0 = world(1100 + seed);
        for e in owners(&g0).into_iter().filter(|&e| e > 10).take(3) {
            let mut g = g0.clone();
            if seed % 2 == 0 {
                g.industry_num[e] = [7, 15, 37, 50, 28][cast::<usize, _>(seed / 2) % 5];
            }
            let rng0 = 31 * cast::<u32, _>(seed) + 1;
            let Some(o) = run(0x6330c1, &[e.to_string()], &g, rng0, vec![]) else {
                return;
            };
            let mut ui = Scripted::default();
            let mut rng = Rng::from_state(rng0);
            let v = estimate_corp_hedge_gains(&mut g, &mut ui, &mut rng, e);
            close(v, o.return_float.unwrap_or(Ext::from_f64(0.0)), &format!("corp {e}"));
            assert_eq!(rng.state(), o.rng);
            cmp().assert(&g, &o.globals);
            n += cast::<i32, _>(v != Ext::from_f64(0.0));
        }
    }
    assert!(n > 2, "too few nonzero gains ({n})");
}

#[test]
fn set_commodity_price_ranges_matches_original() {
    for seed in 0..8u64 {
        let mut g = world(1200 + seed);
        if seed % 3 == 0 {
            g.metals_price_base = 0;
            g.cur_year = g.start_year + 5;
        }
        let rng0 = 77 * cast::<u32, _>(seed) + 3;
        let Some(o) = run(0x629f0f, &[], &g, rng0, vec![]) else {
            return;
        };
        let mut ui = Scripted::default();
        let mut rng = Rng::from_state(rng0);
        set_commodity_price_ranges(&mut g, &mut ui, &mut rng);
        assert_eq!(rng.state(), o.rng);
        assert_eq!(port_msgs(&ui), msgs(&o));
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn init_commodity_markets_matches_original() {
    for seed in 0..6u64 {
        let mut g = world(1300 + seed);
        if seed != 5 {
            g.wheat_price = cast::<f32, _>(Ext::from_f64(0.0));
        }
        let rng0 = 1234 + cast::<u32, _>(seed);
        let Some(o) = run(0x629b50, &[], &g, rng0, vec![]) else {
            return;
        };
        let mut ui = Scripted::default();
        let mut rng = Rng::from_state(rng0);
        init_commodity_markets(&mut g, &mut ui, &mut rng);
        assert_eq!(rng.state(), o.rng);
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn update_metals_price_base_matches_original() {
    let mut news = 0;
    for seed in 0..10u64 {
        let mut g = world(1400 + seed);
        match seed % 4 {
            0 => {
                g.cur_year = g.start_year;
                g.cur_quarter = 1;
            }
            1 | 2 => {
                g.metals_price_base = 0; // old game: the base is added to the gold price
                g.cur_year = g.start_year + 10 + cast::<i32, _>(seed);
                g.gold_price = cast::<f32, _>(Ext::from_f64(1500.0) + cast::<f32, _>(seed) * Ext::from_f64(100.0));
            }
            _ => {}
        }
        let Some(o) = run(0x63a94e, &[], &g, 1, vec![]) else {
            return;
        };
        let mut ui = Scripted::default();
        let v = update_metals_price_base(&mut g, &mut ui);
        close(v, o.return_float.unwrap(), "result");
        assert_eq!(port_msgs(&ui), msgs(&o));
        news += port_msgs(&ui);
        cmp().assert(&g, &o.globals);
    }
    assert!(news > 0, "GOLD SOARS not exercised");
}

#[test]
fn calc_futures_basis_matches_original() {
    let __content = wsr_core::content::current();
    let mut seen = std::collections::HashSet::new();
    for seed in 0..40u64 {
        let mut g = world(1500 + seed);
        let mut r = Rand(seed + 3);
        let c = cast::<i32, _>(seed % 8); // 0 and 7: out of range
        // spread the prices over all bands
        g.oil_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(r.range(30, 180)) / Ext::from_f64(100.0)));
        g.gold_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) + cast::<Ext, _>(r.range(300, 2600))));
        g.silver_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(g.metals_price_base) * Ext::from_f64(0.01) + cast::<Ext, _>(r.range(500, 3000)) / Ext::from_f64(100.0)));
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(r.range(150, 1200)) / Ext::from_f64(100.0)));
        g.corn_price = cast::<f32, _>(cast::<f32, _>(cast::<Ext, _>(r.range(150, 1200)) / Ext::from_f64(100.0)));
        g.oil_crisis_level = cast::<f32, _>(if r.chance(Ext::from_f64(0.2)) { Ext::from_f64(1.0) } else { Ext::from_f64(0.0) });
        g.sel_entity = cast::<i32, _>(r.range(0, 1600));
        let rng0 = 17 + cast::<u32, _>(seed);
        let Some(o) = run(0x635fd1, &[c.to_string()], &g, rng0, vec![]) else {
            return;
        };
        let mut rng = Rng::from_state(rng0);
        let v = calc_futures_basis(&mut g, &mut rng, c);
        close(v, o.return_float.unwrap(), &format!("seed {seed}{__wsr_text_13858}{c}",__wsr_text_13858=__content.string(13858)));
        assert_eq!(rng.state(), o.rng);
        cmp().assert(&g, &o.globals);
        seen.insert(if v == Ext::from_f64(300.0) { 300 } else if v == -Ext::from_f64(100.0) { -100 } else if (Ext::from_f64(90.0)..Ext::from_f64(100.0)).contains(&v) { 90 } else { cast::<i32, _>(v.signum()) });
    }
    assert!(seen.len() >= 4, "bands exercised: {seen:?}");
}
