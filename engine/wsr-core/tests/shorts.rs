//! Tests of `logic::shorts`: hand-checked unit tests plus differential tests against the original
//! routines run under emulation (tools/emu/game.py, see tests/diff/mod.rs).

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, par_seeds, run_original_with};
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::shorts::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

/// ForceCoverShorts (0x5dfe3a), called by FindShortPositions in mode 0.
const FORCE_COVER: u32 = 0x5dfe3a;

/// Players 1..NumPlayers with long and short stock slots, market caps, margin balances; other entities
/// with long holdings (to shape the float). `heavy`: big short percentages (squeezes, float limits).
fn world(seed: u64, heavy: bool) -> Globals {
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    let f64r = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.num_players = cast::<i32, _>(r.range(1, 5));
    let corps: Vec<i16> = (0..6).map(|_| cast::<i16, _>(r.range(11, 1600))).collect();
    for p in 1..=5 {
        g.player_name[p] = PbStr::from(if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { "...OUT..." } else { "Player" });
        g.player_short_margin_bal[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0)) } else { Ext::from_f64(0.0) };
    }
    for e in 1..=1600 {
        g.entity_status[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.9))) { 1 } else { 0 };
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        if e > 10 {
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
        }
        let n = if e <= 5 { 15 } else if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) { 3 } else { 0 };
        for slot in 1..=n {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                let short = e <= 5 && r.chance(cast::<f64, _>(Ext::from_f64(0.5)));
                g.hold_stock_id[(slot, e)] = corps[cast::<usize, _>(r.range(0, 5))];
                g.hold_stock_pct[(slot, e)] = if short {
                    -(cast::<i16, _>(r.range(1, if heavy { 40 } else { 15 })))
                } else {
                    cast::<i16, _>(r.range(0, if heavy { 30 } else { 5 }))
                };
            }
        }
    }
    g
}

fn run(addr: u32, args: &[String], g: &Globals, hooks: Vec<u32>, rng: u32) -> Option<Original> {
    let opts = Opts { args: args.to_vec(), hooks, rng: Some(rng), ..Opts::default() };
    run_original_with(addr, &opts, g)
}

fn cmp() -> Compare {
    // TickerPauseUntil: PauseTicker (UI timing, not ported) in ForceCoverShorts' news
    Compare::default().rel_tol(Ext::from_f64(1e-12)).ignore("TickerPauseUntil")
}

#[test]
fn short_value_and_margin() {
    let mut g = Globals::default();
    g.num_players = 2;
    g.player_name[1] = PbStr::from("A");
    g.player_name[2] = PbStr::from("...OUT...");
    g.market_cap[20] = Ext::from_f64(10.0);
    g.market_cap[30] = Ext::from_f64(2.5);
    g.hold_stock_id[(1, 1)] = 20;
    g.hold_stock_pct[(1, 1)] = -3;
    g.hold_stock_id[(2, 1)] = 30;
    g.hold_stock_pct[(2, 1)] = -4;
    g.hold_stock_id[(3, 1)] = 30;
    g.hold_stock_pct[(3, 1)] = 50; // long: not counted
    g.hold_stock_id[(1, 2)] = 20;
    g.hold_stock_pct[(1, 2)] = -1;
    assert_eq!(calc_short_position_value(&g, 1), Ext::from_f64(40.0));
    g.player_short_margin_bal[1] = Ext::from_f64(15.0);
    g.cash[1] = Ext::from_f64(100.0);
    recalc_short_margin_balances(&mut g);
    assert_eq!(g.player_short_margin_bal[1], Ext::from_f64(40.0));
    assert_eq!(g.cash[1], Ext::from_f64(75.0));
    assert_eq!(g.player_short_margin_bal[2], Ext::from_f64(0.0), "players out of the game are skipped");
    let (mut ui, mut rng) = (Scripted::default(), Rng::from_state(1));
    assert_eq!(find_short_positions(&mut g, &mut ui, &mut rng, 20, 0), 2);
    assert_eq!(find_short_positions(&mut g, &mut ui, &mut rng, 21, 0), -2);
    assert_eq!(find_short_positions(&mut g, &mut ui, &mut rng, 0, 2), 3);
    assert_eq!(find_short_positions(&mut g, &mut ui, &mut rng, 30, 2), -4);
    assert_eq!(find_short_positions(&mut g, &mut ui, &mut rng, 0, 0), 1);
    assert_eq!(g.short_pos_flag, 1);
}

#[test]
fn calc_short_position_value_matches_original() {
    let __content = wsr_core::content::current();
    for seed in 0..6u64 {
        let g = world(seed, seed % 2 == 1);
        for p in 1..=g.num_players {
            let Some(o) = run(0x5e0cc9, &[format!("v:{p}")], &g, vec![], 1) else {
                return;
            };
            let v = calc_short_position_value(&g, p);
            let w = o.return_float.unwrap();
            assert!((v - w).abs() <= Ext::from_f64(1e-12) * v.abs().max(w.abs()), "{__wsr_text_13997}{p}: {v} vs {w}",__wsr_text_13997=__content.string(13997));
        }
    }
}

#[test]
fn recalc_short_margin_balances_matches_original() {
    for seed in 10..18u64 {
        let mut g = world(seed, seed % 2 == 1);
        let g0 = g.clone();
        let Some(o) = run(0x5e0ade, &[], &g, vec![], 1) else {
            return;
        };
        recalc_short_margin_balances(&mut g);
        assert!(!Compare::default().diffs(&g0, &o.globals).is_empty(), "original changed nothing");
        cmp().assert(&g, &o.globals);
    }
}

#[test]
fn find_short_positions_matches_original() {
    let __content = wsr_core::content::current();
    let covers = AtomicUsize::new(0);
    par_seeds(20..32, |seed| {
        let g0 = world(seed, seed % 2 == 0);
        let corp = (1..=15).map(|s| cast::<i32, _>(g0.hold_stock_id[(s, 1)])).find(|&c| c > 10).unwrap_or(500);
        for (c, p) in [(0, 0), (corp, 0), (0, 1), (corp, 1), (777, 2), (0, g0.num_players)] {
            let mut g = g0.clone();
            let rs = (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b9) ^ (cast::<u32, _>(c)) << 8 ^ cast::<u32, _>(p);
            let Some(o) = run(0x5df7c4, &[format!("v:{c}"), format!("v:{p}")], &g, vec![FORCE_COVER], rs) else {
                return;
            };
            let (mut ui, mut rng) = (Scripted::default(), Rng::from_state(rs));
            let r = find_short_positions(&mut g, &mut ui, &mut rng, c, p);
            assert_eq!(r, cast::<i32, _>(o.return_value), "corp {c} player {p}");
            assert_eq!(rng.state(), o.rng, "seed {seed} corp {c}{__wsr_text_13565}{p}: RND state",__wsr_text_13565=__content.string(13565));
            covers.fetch_add(o.log.iter().filter(|l| l.starts_with("CALL")).count(), Ordering::Relaxed);
            cmp().assert(&g, &o.globals);
        }
    });
    assert!(covers.into_inner() > 0, "no ForceCoverShorts path exercised");
}
