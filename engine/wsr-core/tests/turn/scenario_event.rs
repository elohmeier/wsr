//! Differential tests of RunScenarioEvent_689361 (0x689361): every scenario number as a proposal
//! (EventStatus 0, answered YES or NO) and as an outcome (EventStatus 1), on random worlds.
//!
//! `SCEN_CASES=a,b,..` restricts the scenario numbers, `SEEDS=..` the seeds; `TURN_DBG=1` prints the
//! original's log on a mismatch.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::world::world;
use wsr_core::logic::turn::run_scenario_event_689361;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::ReportAction;

const EVENT: u32 = 0x689361;

/// The scenario numbers the routine knows (others take CASE ELSE).
pub fn cases() -> Vec<i32> {
    match std::env::var("SCEN_CASES") {
        Ok(s) => s.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
        Err(_) => (1..=72).chain(100..=122).collect(),
    }
}

/// Tags that select alternative outcome texts (ScenarioTag).
#[allow(non_snake_case)]
fn TAGS(__content: &wsr_core::content::ContentPack) -> Vec<&str> { vec![
    "", __content.string(13130), __content.string(13279), "ALZHEIMER", "E.R.", "BRONTO", "TLAXACOATL", "SEXBOT", "KILLERBOT",
    "SAIPAN", "ZOMBEX-II", __content.string(4564), "TEAMSTERS", "SONDER", "NUKE", "SAWDUST", "THC", "ZING!",
    "CHINESETOYS", "HYPO", "PYRO", "Hanford", "BRE-X", "Disney", "CYANIDE", __content.string(14361), "corn futures",
    __content.string(14362), "Oregon", "ARIZONA",
] }

/// A world with scenario `n` for a company (of industry `n` for 3..69, else random) of player 1.
pub fn scen_world(seed: u64, n: i32, status: i16) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x51_7cc1_b727_220a) ^ (cast::<u64, _>(n)) << 20 | 1);
    let mut corp = cast::<i32, _>(r.range(131, 1579));
    // (companies 131..1580: banks and insurers never get these stories, and BusinessAssets is
    // DIM (131 TO 1600), error 9 in the original)
    if (1..=71).contains(&n) && r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
        g.industry_num[corp] = cast::<i16, _>(n);
    } else if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
        corp = 1580;
    }
    g.entity_status[corp] = 1;
    g.scenario_corp = cast::<i16, _>(corp);
    g.scenario_num = cast::<i16, _>(n);
    g.event_status[n] = status;
    // RunScenarioEvent sets EventPlayer = CurPlayer first (0 would be error 9 in PlayerCeoOf)
    g.event_player = 1;
    g.cur_player = 1;
    g.player_notoriety[1] = cast::<i16, _>(r.range(0, 6));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.player_ceo_of[1] = corp;
    }
    g.cheat_mode = st("Y");
    g.scenario_tag = st(TAGS(&__content)[cast::<usize, _>(r.range(0, cast::<i64, _>(TAGS(&__content).len()) - 1))]);
    g.scenario_stage = cast::<i16, _>(r.range(-2, 12));
    g.currency_code = st(["USA", "UK", "AUS", "NZ", "EUR"][cast::<usize, _>(r.range(0, 4))]);
    g.currency_symbol = st(["$", "£", "€"][cast::<usize, _>(r.range(0, 2))]);
    g.million_prefix_uc = st("Mil");
    g.halt_comment_count2 = cast::<i32, _>(r.range(0, 2));
    g.pandemic_state = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { 0 } else { 1 };
    g.oil_crisis_level = cast::<f32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { Ext::from_f64(0.0) } else { Ext::from_f64(1.0) });
    g.subprime_crisis_state = cast::<i16, _>(r.range(-3, 1));
    g.long_gov_bond_face[corp] = r.range(0, 500);
    g.short_gov_bond_face[corp] = r.range(0, 500);
    g.extraordinary_pl[corp] = r.range(-50, 50);
    g.sel_entity = 0;
    g
}

/// Runs case `n` (status, answer) on `seed` in the port and the original and compares.
pub fn run_case(seed: u64, n: i32, status: i16, yes: bool) {
    let __content = wsr_core::content::current();
    load_scenwin();
    let g = scen_world(seed, n, status);
    let mut r = Rand(seed * 31 + cast::<u64, _>(n));
    let rng = cast::<u32, _>(r.next());
    let price_pct = cast::<i32, _>(r.range(-30, 30));
    let answers: &[&str] = if yes { &[] } else { &["click:2"] };
    let what = format!("seed {seed}{__wsr_text_13612}{n}{__wsr_text_14162}{status} yes {yes}",__wsr_text_13612=__content.string(13612), __wsr_text_14162=__content.string(14162));
    let Some(o) = run(EVENT, &[&price_pct.to_string()], &g, rng, answers, &[]) else { return };
    let mut port = g.clone();
    let mut ui = TUi::default();
    if !yes {
        ui.report_answers.push_back(ReportAction::No);
    }
    let mut rr = Rng::from_state(rng);
    run_scenario_event_689361(&mut port, &mut ui, &mut rr, &NOW, price_pct);
    check(&what, &port, &ui, rr.state(), &o);
}

#[test]
fn diff_scenario_event_proposals() {
    let cs = cases();
    let jobs: Vec<u64> = seeds(1..=2).into_iter().flat_map(|s| cs.iter().map(move |&n| s * 1000 + cast::<u64, _>(n))).collect();
    par_seeds(jobs, |j| {
        let (seed, n) = (j / 1000, cast::<i32, _>(j % 1000));
        run_case(seed, n, 0, seed % 2 == 1);
        run_case(seed + 100, n, 0, seed % 2 == 0);
    });
}

#[test]
fn diff_scenario_event_outcomes() {
    let cs = cases();
    let jobs: Vec<u64> = seeds(1..=3).into_iter().flat_map(|s| cs.iter().map(move |&n| s * 1000 + cast::<u64, _>(n))).collect();
    par_seeds(jobs, |j| {
        let (seed, n) = (j / 1000, cast::<i32, _>(j % 1000));
        run_case(seed, n, 1, true);
    });
}
