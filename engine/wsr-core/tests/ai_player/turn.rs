//! Differential tests of AiComputerPlayerTurn (0x5c3172): the dispatcher alone (all steps skipped) and
//! whole turns (all steps).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;

/// The dispatcher with every step skipped: entry checks, auto settings, news flush, hot stock.
#[test]
fn diff_turn_dispatcher() {
    let ran = std::sync::atomic::AtomicU32::new(0);
    par_seeds(1..=16, |seed| {
        let mut g = ai_world(seed + 50);
        let mut r = Rand(seed * 13 + 1);
        match seed % 8 {
            0 => g.is_computer_player[g.cur_player] = 0,
            1 => {
                g.start_year = g.cur_year;
                g.cur_quarter = 1;
                g.sim_co_count = cast::<i32, _>(r.range(1, 600));
            }
            2 => g.entity_status[g.cur_player] = 2,
            3 => {
                let t = cast::<i32, _>(g.hold_stock_id[(1, g.cur_player)]);
                if t > 10 {
                    g.market_cap[t] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(199.0));
                    g.cur_year = g.game_end_year - 1;
                }
            }
            _ => {}
        }
        let Some(t) = run_turn(&g, cast::<u32, _>(r.next()), Some(&[]), &[], &[]) else { return };
        if check_turn(&format!("seed {seed}"), &t, Ext::from_f64(1e-12)) && t.changed_field("ActionCode") {
            ran.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    let _ = ran;
}

/// Whole turns of computer players (all steps). `TURN_SEEDS=n` runs n seeds (default 48).
#[test]
fn diff_turn_whole() {
    let __content = wsr_core::content::current();
    use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
    let n = std::env::var("TURN_SEEDS").ok().and_then(|s| s.parse().ok()).unwrap_or(48u64);
    let (compared, acted, changes) = (AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0));
    par_seeds(1..=n, |seed| {
        let mut g = ai_world(seed + 7000);
        // half of the turns near the end of the game (end-game steps), some without a main holding
        if seed % 2 == 0 {
            g.cur_year = g.game_end_year - cast::<i32, _>(seed % 3);
        }
        let rng = (cast::<u32, _>(seed)).wrapping_mul(2654435761);
        let Some(t) = run_turn(&g, rng, None, &[], &[]) else { return };
        if check_turn(&format!("seed {seed}"), &t, Ext::from_f64(1e-9)) {
            compared.fetch_add(1, Relaxed);
            changes.fetch_add(cast::<u32, _>(t.changed().len()), Relaxed);
            if t.orig.globals.turn_action_count > 0 {
                acted.fetch_add(1, Relaxed);
            }
        }
    });
    let (c, a) = (compared.load(Relaxed), acted.load(Relaxed));
    eprintln!("whole turns: {c}{__wsr_text_13578}{n}{__wsr_text_13579}{a} with actions, {} changed fields", changes.load(Relaxed),__wsr_text_13578=__content.string(13578), __wsr_text_13579=__content.string(13579));
    if super::diff::unavailable().is_none() {
        assert!(cast::<u64, _>(c) * 4 >= n * 3, "too many turns not comparable");
        assert!(a * 3 >= c, "too few turns with actions");
    }
}
