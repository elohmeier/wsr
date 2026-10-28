//! Differential tests of SimScheduledEvents (0x4a395a) on random worlds at the interesting ticks.
//! Callees that are PORT-TODO in the port are skipped in the original (`all_skips`).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::turn::sim_scheduled_events_skipping;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SCHED: u32 = 0x4a395a;

/// Ticks with scheduled events (plus random ones).
const TICKS: &[i32] = &[
    1, 2, 6, 11, 12, 19, 25, 28, 60, 73, 75, 76, 135, 145, 214, 220, 250, 260, 290, 401, 427, 483, 490, 495, 498,
    501, 502, 503, 525, 605, 610, 650, 700, 745, 805, 850, 960, 990, 1001, 1179, 1200, 1280, 1355, 1420, 1481,
    1548, 1550, 1584, 1586, 1590, 1595, 1599, 1600,
];

pub fn sched_world(seed: u64) -> (Globals, u32) {
    let mut g = turn_world(seed);
    let mut r = Rand(seed * 7919 + 17);
    let n = cast::<u64, _>(TICKS.len());
    g.sim_co_count = if seed % 4 == 3 { cast::<i32, _>(r.range(1, 1600)) } else { TICKS[cast::<usize, _>(seed / 4 % n)] };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        // the R&D / marketing push and the quote refresh
        g.rand_seed = 10 * cast::<i32, _>(r.range(0, 3000)) + 1;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.pending_forced_conv_corp = cast::<i32, _>(r.range(11, 1600));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.sel_entity = cast::<i32, _>(r.range(11, 1600));
        g.last_sel_entity = cast::<i32, _>(r.range(0, 1600));
    }
    g.auto_add_stream_list = cast::<i32, _>(r.range(0, 1));
    g.short_pos_flag = cast::<i32, _>(r.range(-1, 1));
    g.superfund_corp = cast::<i16, _>(r.range(0, 1)) * cast::<i16, _>(r.range(131, 1600));
    g.max_bond_issue = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 0 } else { r.range(1000, 200000) };
    g.max_stock_offering_amt = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 0 } else { r.range(1000, 200000) };
    g.ai_hostile_merger_qtr = cast::<i32, _>(r.range(-3, 1));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.start_year = g.cur_year;
        g.cur_quarter = 1;
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // a holding company at this tick
        let s = g.sim_co_count;
        if s > 130 {
            g.industry_num[s] = 70;
            g.entity_status[s] = 1;
            g.corp_controller[s] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { cast::<i32, _>(r.range(11, 1600)) } else { 1 };
        }
    }
    g.cheat_mode = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "Y" } else { "N" });
    (g, cast::<u32, _>(r.next()))
}

pub fn run_sched(seed: u64) {
    load_scenwin();
    let (g, rng) = sched_world(seed);
    let skips = all_skips(&[]);
    let what = format!("seed {seed} tick {}", g.sim_co_count);
    let Some(o) = run(SCHED, &[], &g, rng, &[], &skips) else { return };
    let mut port = g.clone();
    let mut ui = TUi::default();
    let mut rr = Rng::from_state(rng);
    let skip: Vec<u32> = skips.iter().map(|x| x.0).collect();
    sim_scheduled_events_skipping(&mut port, &mut ui, &mut rr, &NOW, &skip);
    check(&what, &port, &ui, rr.state(), &o);
}

#[test]
fn diff_sim_scheduled_events() {
    par_seeds(seeds(0..(4 * cast::<u64, _>(TICKS.len()))), run_sched);
}

#[test]
fn diff_scheduled_earnings_tip() {
    load_scenwin();
    par_seeds(0..8u64, |seed| {
        let mut g = turn_world(seed + 7810);
        let corp = 131;
        g.sim_co_count = corp;
        g.entity_status[corp] = 1;
        g.industry_num[corp] = 37;
        g.insider_earn_tip_corp = if seed % 2 == 0 { cast::<i16, _>(corp) } else { cast::<i16, _>(-corp) };
        g.insider_tippee = 1;
        g.tip_industry = 37;
        g.tipster_state[1] = cast::<i16, _>(seed / 2 + 1);
        g.insider_tip_event_qtr = cast::<i16, _>(g.cur_quarter % 4 + 1);
        g.insider_tip_event_tick = cast::<i16, _>(g.cur_quarter);
        let rng0 = 0x7810 + cast::<u32, _>(seed);
        let skips = all_skips(&[]);
        let Some(o) = run(SCHED, &[], &g, rng0, &[], &skips) else { return };
        let mut port = g.clone();
        let mut ui = TUi::default();
        let mut rng = Rng::from_state(rng0);
        let skip: Vec<u32> = skips.iter().map(|x| x.0).collect();
        sim_scheduled_events_skipping(&mut port, &mut ui, &mut rng, &NOW, &skip);
        check(&format!("earnings tip seed {seed}"), &port, &ui, rng.state(), &o);
        assert_eq!(port.insider_earn_tip_corp, 0, "the tipped company's story should break");
    });
}
