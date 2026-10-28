//! Differential tests of the scenario story: MaybeStartScenario (0x490982), AdvanceScenarioStory
//! (0x48fe45), RunScenarioEvent (0x48ec04).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::turn::{advance_scenario_story, maybe_start_scenario, run_scenario_event};
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::ReportAction;

const MAYBE_START: u32 = 0x490982;
const ADVANCE: u32 = 0x48fe45;
const RUN_EVENT: u32 = 0x48ec04;

/// A world with a scenario in some state for a company of player 1.
fn story_world(seed: u64) -> (Globals, u32, bool) {
    let mut g = turn_world(seed);
    let mut r = Rand(seed * 104_729 + 3);
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.cheat_mode = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { "Y" } else { "N" });
    let corp = cast::<i32, _>(r.range(11, 1580));
    g.entity_status[corp] = 1;
    g.corp_controller[corp] = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { 1 } else { cast::<i32, _>(r.range(11, 1600)) };
    g.sim_co_count = corp;
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.earnings_yr4_ago[corp] = Ext::from_f64(0.0);
        g.earnings_yr3_ago[corp] = Ext::from_f64(0.0);
    }
    g.adventure_cooldown = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { 0 } else { cast::<i16, _>(r.range(-1, 3)) };
    for e in 1..=200 {
        g.event_status[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { 0 } else { cast::<i16, _>(r.range(1, 2)) };
    }
    match r.range(0, 3) {
        0 => {
            g.scenario_num = 0;
            g.scenario_corp = 0;
            g.event_player = 0;
        }
        1 => {
            g.scenario_num = cast::<i16, _>(r.range(1, 70));
            g.scenario_corp = cast::<i16, _>(corp);
            g.event_player = 1;
        }
        2 => {
            g.scenario_num = cast::<i16, _>(r.range(111, 120));
            g.scenario_corp = cast::<i16, _>(r.range(11, 1600));
            g.event_player = cast::<i16, _>(r.range(0, 2));
        }
        _ => {
            g.scenario_num = 104;
            g.cheat_japan_target = -1;
            g.scenario_corp = cast::<i16, _>(corp);
        }
    }
    g.scenario_stage = cast::<i16, _>(r.range(-3, 12));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.scenario_headline = st("Something happens at the company now");
    }
    g.story_events111to120_done = cast::<i16, _>(r.range(0, 1));
    g.player_ceo_of[1] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { corp } else { 0 };
    g.currency_code = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { "USA" } else { "UK" });
    g.million_prefix_uc = st("Mil");
    g.sel_entity = 0;
    (g, cast::<u32, _>(r.next()), r.chance(cast::<f64, _>(Ext::from_f64(0.7))))
}

/// Runs that showed a scenario memo / news update (branch coverage).
static SHOWN: [std::sync::atomic::AtomicUsize; 3] =
    [std::sync::atomic::AtomicUsize::new(0), std::sync::atomic::AtomicUsize::new(0), std::sync::atomic::AtomicUsize::new(0)];

fn slot(addr: u32) -> usize {
    match addr {
        MAYBE_START => 0,
        ADVANCE => 1,
        _ => 2,
    }
}

fn run_one(addr: u32, seed: u64, f: fn(&mut Globals, &mut TUi, &mut Rng)) {
    load_scenwin();
    let (mut g, rng, yes) = story_world(seed);
    if addr == ADVANCE {
        g.sim_co_count = 213 * (1 + cast::<i32, _>(seed % 7)) + 1;
        if seed.is_multiple_of(5) {
            g.sim_co_count += 1;
        }
    }
    let answers: &[&str] = if yes { &[] } else { &["click:2"] };
    let what = format!("{addr:x} seed {seed}");
    let skips = all_skips(&[]);
    let Some(o) = run(addr, &[], &g, rng, answers, &skips) else { return };
    let mut port = g.clone();
    let mut ui = TUi::default();
    if !yes {
        ui.report_answers.push_back(ReportAction::No);
    }
    let mut rr = Rng::from_state(rng);
    f(&mut port, &mut ui, &mut rr);
    check(&what, &port, &ui, rr.state(), &o);
    if !ui.reports.is_empty() || !ui.news.is_empty() || !ui.msgs.is_empty() {
        SHOWN[slot(addr)].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn assert_coverage(addr: u32) {
    if unavailable().is_none() && std::env::var_os("SEEDS").is_none() {
        assert!(0 < SHOWN[slot(addr)].load(std::sync::atomic::Ordering::Relaxed), "{addr:x}: no scenario event shown");
    }
}

#[test]
fn diff_maybe_start_scenario() {
    par_seeds(seeds(1..=60), |seed| {
        run_one(MAYBE_START, seed, |g, ui, r| maybe_start_scenario(g, ui, r, &NOW));
    });
    assert_coverage(MAYBE_START);
}

#[test]
fn diff_advance_scenario_story() {
    par_seeds(seeds(1..=60), |seed| {
        run_one(ADVANCE, seed, |g, ui, r| advance_scenario_story(g, ui, r, &NOW));
    });
    assert_coverage(ADVANCE);
}

#[test]
fn diff_run_scenario_event() {
    par_seeds(seeds(1..=60), |seed| {
        run_one(RUN_EVENT, seed, |g, ui, r| run_scenario_event(g, ui, r, &NOW));
    });
    assert_coverage(RUN_EVENT);
}
