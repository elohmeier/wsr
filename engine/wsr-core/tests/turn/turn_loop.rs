//! Differential tests of TickerSimSteps (0x41ef3f), EndTurn (0x424fd6) and CbEndTurn (0x411de0) on
//! random worlds. PORT-TODO callees are skipped in the original (`all_skips`).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::turn::{end_turn_skipping, ticker_sim_steps_skipping};
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

fn loop_world(seed: u64) -> (Globals, u32) {
    let mut g = turn_world(seed);
    let mut r = Rand(seed * 2_654_435 + 9);
    g.sim_co_count = match r.range(0, 4) {
        0 => cast::<i32, _>(r.range(1, 10)),
        1 => cast::<i32, _>(r.range(1580, 1600)),
        _ => cast::<i32, _>(r.range(11, 1579)),
    };
    g.game_in_progress = 1;
    g.ticker_stop_req = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { "Y" } else { "N" });
    g.clock_state = st(["", "RUN", "STOP", ""][cast::<usize, _>(r.range(0, 3))]);
    g.action_code = st(["", "SUPP", "AUTO", "SUPP"][cast::<usize, _>(r.range(0, 3))]);
    g.turn_action_count = cast::<i32, _>(r.range(0, 7));
    g.econ_cycle_counter = cast::<i16, _>(r.range(0, 25));
    g.index_recalc_tick = cast::<i32, _>(r.range(0, 40));
    g.ticker_on = cast::<i32, _>(r.range(0, 1));
    g.solo_human_mode = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { 1 } else { 0 };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) {
        g.game_end_year = g.cur_year;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.player_name[p] = st("...OUT...");
    }
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.sel_entity = cast::<i32, _>(r.range(0, 1600));
    g.last_sel_entity = cast::<i32, _>(r.range(0, 1600));
    g.active_entity = cast::<i32, _>(r.range(0, 1600));
    g.ticker_dirty = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "Y" } else { "" });
    g.cheat_mode = st("N");
    (g, cast::<u32, _>(r.next()))
}

/// Branch coverage: [game ended, turn passed to another player, a computer player's turn, ticks run].
static HITS: [std::sync::atomic::AtomicUsize; 4] = [
    std::sync::atomic::AtomicUsize::new(0),
    std::sync::atomic::AtomicUsize::new(0),
    std::sync::atomic::AtomicUsize::new(0),
    std::sync::atomic::AtomicUsize::new(0),
];

fn hit(k: usize) {
    HITS[k].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn assert_hits(ks: &[usize]) {
    if unavailable().is_none() && std::env::var_os("SEEDS").is_none() {
        for &k in ks {
            assert!(0 < HITS[k].load(std::sync::atomic::Ordering::Relaxed), "branch {k} not covered");
        }
    }
}

fn run_loop(addr: u32, seed: u64) {
    let __content = wsr_core::content::current();
    load_scenwin();
    let (g, rng) = loop_world(seed);
    // mostly short batches (the emulated ticks are slow); every 6th seed a random 10..20 (x1.5, x2.25)
    let step_count = if seed.is_multiple_of(6) { 0 } else { 1 + cast::<i32, _>(seed % 3) };
    let what = format!("{addr:x} seed {seed} tick {} clock {:?}{__wsr_text_14363}{:?}", g.sim_co_count, g.clock_state.to_display(), g.action_code.to_display(),__wsr_text_14363=__content.string(14363));
    let skips = all_skips(&[]);
    let args: Vec<String> = if addr == 0x41ef3f { vec![step_count.to_string()] } else { vec![] };
    let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let Some(o) = run(addr, &args, &g, rng, &[], &skips) else { return };
    let mut port = g.clone();
    let mut ui = TUi::default();
    let mut rr = Rng::from_state(rng);
    let skip: Vec<u32> = skips.iter().map(|x| x.0).collect();
    if addr == 0x41ef3f {
        ticker_sim_steps_skipping(&mut port, &mut ui, &mut rr, step_count, &NOW, &skip);
    } else {
        end_turn_skipping(&mut port, &mut ui, &mut rr, &NOW, &skip);
    }
    check(&what, &port, &ui, rr.state(), &o);
    if port.game_end_flag == "Y" && g.game_end_flag != "Y" {
        hit(0);
    }
    if port.cur_player != g.cur_player {
        hit(1);
    }
    if ui.news.iter().any(|n| n.contains(__content.string(14364))) {
        hit(2);
    }
    if port.sim_co_count != g.sim_co_count {
        hit(3);
    }
}

#[test]
fn diff_ticker_sim_steps() {
    par_seeds(seeds(1..=24), |seed| run_loop(0x41ef3f, seed));
    assert_hits(&[3]);
}

#[test]
fn diff_end_turn() {
    par_seeds(seeds(1..=24), |seed| run_loop(0x424fd6, seed));
    assert_hits(&[0, 1, 2]);
}
