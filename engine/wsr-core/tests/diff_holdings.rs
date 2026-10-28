//! Differential tests: ClearAllHoldings (0x49261c) and PackHoldingArrays (0x49117c) of the original,
//! run under emulation (tools/emu/game.py), against the Rust port in `logic::holdings`.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Rand, assert_same, run_original, run_original_with};
use wsr_core::logic::holdings::{PackEvent, clear_all_holdings, pack_holding_arrays, post_pack_events};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::state::Globals;

/// A state with sparse holding tables (gaps, stale counts, full tables), random entity status and
/// sparse record lists. `neg_basis`: allow negative stock cost bases (excess loss accounts);
/// `max_corp`: held corp ids are 11..=max_corp (a small range makes RecaptureExcessLoss matches likely).
fn random_state(seed: u64, neg_basis: bool, max_corp: i64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.num_players = cast::<i32, _>(r.range(1, 4));
    for e in 1..=1600 {
        g.entity_status[e] = match r.range(0, 9) {
            0..=1 => 0,
            2..=7 => 1,
            8 => 2,
            _ => cast::<i16, _>(r.range(3, 5)),
        };
        let density = [Ext::from_f64(0.0), Ext::from_f64(0.2), Ext::from_f64(0.6), Ext::from_f64(1.0)][cast::<usize, _>(r.range(0, 3))];
        g.hold_stock_id[(0, e)] = cast::<i16, _>(r.range(0, 15)); // stale count
        for s in 1..=15 {
            if r.chance(density) {
                g.hold_stock_id[(s, e)] = cast::<i16, _>(r.range(11, max_corp));
                g.hold_stock_pct[(s, e)] = cast::<i16, _>(r.range(0, 100));
                g.hold_stock_basis[(s, e)] = if neg_basis && r.chance(Ext::from_f64(0.1)) {
                    -(cast::<Ext, _>(r.range(0, 5_000_000))) / Ext::from_f64(7.0)
                } else {
                    cast::<Ext, _>(r.range(0, 50_000_000)) / Ext::from_f64(3.0)
                };
            } else if r.chance(Ext::from_f64(0.3)) {
                // empty slot with leftover pct/basis (must be cleared by the pack)
                g.hold_stock_pct[(s, e)] = cast::<i16, _>(r.range(1, 50));
                g.hold_stock_basis[(s, e)] = cast::<Ext, _>(r.range(1, 1000)) + Ext::from_f64(0.25);
            }
            if r.chance(density) {
                g.hold_bond_id[(s, e)] = if r.chance(Ext::from_f64(0.05)) {
                    -(cast::<i32, _>(r.range(1, 130)))
                } else {
                    cast::<i32, _>(r.range(1, 1600))
                };
                g.hold_bond_face[(s, e)] = r.range(1, 1_000_000_000);
                g.bond_hold_cost[(s, e)] = cast::<Ext, _>(r.range(1, 1_000_000_000)) * Ext::from_f64(0.97);
            } else if r.chance(Ext::from_f64(0.3)) {
                g.hold_bond_face[(s, e)] = r.range(1, 100);
                g.bond_hold_cost[(s, e)] = Ext::from_f64(1.5);
            }
        }
        g.hold_bond_id[(0, e)] = cast::<i32, _>(r.range(0, 15));
        if r.chance(Ext::from_f64(0.05)) {
            g.extraordinary_pl[e] = r.range(-1_000_000, 1_000_000);
        }
    }
    let lists: [(&mut wsr_core::pb::array::Arr1<PbStr>, i32); 6] = [
        (&mut g.option_recs, 1000),
        (&mut g.futures_recs, 1000),
        (&mut g.swap_contracts, 1000),
        (&mut g.phys_commod_rec, 1000),
        (&mut g.asset_sale_offers, 100),
        (&mut g.price_alerts, 100),
    ];
    for (k, (recs, n)) in lists.into_iter().enumerate() {
        let density = [Ext::from_f64(0.0), Ext::from_f64(0.01), Ext::from_f64(0.3), Ext::from_f64(0.9), Ext::from_f64(1.0), Ext::from_f64(0.5)][(k + cast::<usize, _>(seed)) % 6];
        recs[0] = PbStr::from("17");
        for i in 1..=n {
            if r.chance(density) {
                recs[i] = PbStr::from(format!("{__wsr_text_13746}{k}-{i},{}", r.range(0, 99999),__wsr_text_13746=__content.string(13746)).as_str());
            }
        }
    }
    g
}

#[test]
fn clear_all_holdings_matches_original() {
    for (seed, mode) in [(1u64, 1), (2, 2), (3, 0), (4, 2), (5, 7)] {
        let g = random_state(seed, true, 1600);
        let Some((orig, log)) = run_original(0x49261c, &[mode], &g, &[]) else {
            return;
        };
        let mut port = g.clone();
        clear_all_holdings(&mut port, mode);
        assert!(log.is_empty(), "unexpected UI/API calls: {log:?}");
        assert!(
            !Compare::default().diffs(&g, &orig).is_empty(),
            "original changed nothing"
        );
        assert_same(&port, &orig);
    }
}

#[test]
fn pack_holding_arrays_matches_original() {
    for seed in 10u64..16 {
        let g = random_state(seed, false, 1600);
        let Some((orig, log)) = run_original(0x49117c, &[], &g, &[]) else {
            return;
        };
        let mut port = g.clone();
        let ev = pack_holding_arrays(&mut port);
        assert!(ev.is_empty());
        assert!(log.is_empty(), "unexpected UI/API calls: {log:?}");
        assert!(
            !Compare::default().diffs(&g, &orig).is_empty(),
            "original changed nothing"
        );
        assert_same(&port, &orig);
    }
}

/// Negative bases exercise RecaptureExcessLoss and both news headlines, including their complete
/// state changes and PostNews call order.
#[test]
fn pack_holding_arrays_with_excess_loss_recapture() {
    let __content = wsr_core::content::current();
    let mut total = 0;
    for seed in 20u64..26 {
        let g = random_state(seed, true, if seed % 2 == 0 { 14 } else { 40 });
        let opts = Opts {
            hooks: vec![0x41e7d5],
            ..Opts::default()
        };
        let Some(o) = run_original_with(0x49117c, &opts, &g) else {
            return;
        };
        let mut port = g.clone();
        let ev = pack_holding_arrays(&mut port);
        total += ev.len();
        // PostNews(BYREF ownerId, BYREF corpId), twice per recapture of more than 1
        let posts: Vec<(i32, i32)> = o
            .log
            .iter()
            .filter_map(|l| l.strip_prefix("CALL PostNews("))
            .map(|l| {
                let v: Vec<i32> = l
                    .split(", ")
                    .take(2)
                    .map(|a| a.split("->").nth(1).unwrap().parse().unwrap())
                    .collect();
                (v[0], v[1])
            })
            .collect();
        let expect: Vec<(i32, i32)> = ev
            .iter()
            .flat_map(|e| match *e {
                PackEvent::ExcessLossRecaptured {
                    owner_id, corp_id, ..
                } => [(owner_id, corp_id), (owner_id, corp_id)],
            })
            .collect();
        eprintln!(
            "seed {seed}: {}{__wsr_text_13747}{} PostNews calls, {:.2}{__wsr_text_13748}",
            ev.len(),
            posts.len(),
            o.elapsed
        ,__wsr_text_13747=__content.string(13747), __wsr_text_13748=__content.string(13748));
        assert_eq!(expect, posts, "PostNews calls");
        post_pack_events(&mut port, &mut wsr_core::ui::Scripted::default(), ev);
        Compare::default().assert(&port, &o.globals);
    }
    assert!(total > 0, "no recapture exercised");
}
