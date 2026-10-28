//! Host-facing trading desk notification dispatch.
use wsr_core::pb::real::{Ext, cast};

use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::{Env, desk::{ctl, desk_click, desk_industry_selection}};
use wsr_core::pb::{pbstr::PbStr, rng::Rng};
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

#[test]
fn chart_controls_select_the_original_series() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(123);
    let env = Env::skipping(&LocalTime::HARNESS, &[0x6141b1]);
    g.cur_player = 3;
    for (control, expected) in [(ctl::CHART_STOCK_INDEX, 0), (ctl::CHART_CRUDE, 6),
        (ctl::CHART_GOLD, 7), (ctl::CHART_SILVER, 8), (ctl::CHART_WHEAT, 9),
        (ctl::CHART_CORN, 10), (ctl::NET_WORTH, 3), (ctl::CHART_PRIME_RATE, 1601),
        (ctl::CHART_LONG_BOND, 1602), (ctl::CHART_SHORT_BOND, 1603), (ctl::CHART_GDP_GROWTH, 1604)] {
        desk_click(&mut g, &mut ui, &mut rng, &env, control, 0);
        assert_eq!(g.chart_entity, expected, "{__wsr_text_13740}{control}",__wsr_text_13740=__content.string(13740));
    }
    assert_eq!(env.skipped(), vec![0x6141b1; 11]);
    desk_click(&mut g, &mut ui, &mut rng, &env, ctl::CHART_CRUDE, 1);
    assert_eq!(g.chart_entity, 1604);
}

#[test]
fn ticker_and_menu_controls_apply_state_transitions() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    let env = Env::skipping(&LocalTime::HARNESS, &[0x41d9c2, 0x41e749, 0x481079, 0x4131ea]);
    g.ticker_pause_until = 10;
    desk_click(&mut g, &mut ui, &mut rng, &env, ctl::TICKER_ON_OFF, 0);
    assert_eq!((g.ticker_on, &g.clock_state, g.ticker_pause_until), (1, &PbStr::from("RUN"), 0));
    desk_click(&mut g, &mut ui, &mut rng, &env, ctl::TICKER_ON_OFF, 0);
    assert_eq!((g.ticker_on, &g.clock_state), (0, &PbStr::from("STOP")));
    desk_click(&mut g, &mut ui, &mut rng, &env, ctl::FINANCING_MENU, 0);
    assert_eq!(g.tmp_ext1, Ext::from_f64(5.0));
    assert_eq!(env.skipped(), vec![0x41d9c2, 0x41e749, 0x481079, 0x481079, 0x4131ea]);
    desk_industry_selection(&mut g, &PbStr::from("Banking 01"), 9);
    assert_eq!(g.sel_industry, 1);
}

mod diff;
#[path = "sim_entity/common.rs"]
mod common;

#[test]
fn chart_callbacks_match_original() {
    let cases = [(0x412e59, ctl::CHART_STOCK_INDEX), (0x412eaa, ctl::CHART_CRUDE),
        (0x412efb, ctl::CHART_GOLD), (0x412f4c, ctl::CHART_SILVER),
        (0x412f9d, ctl::CHART_WHEAT), (0x412fee, ctl::CHART_CORN),
        (0x41303f, ctl::NET_WORTH), (0x413092, ctl::CHART_PRIME_RATE),
        (0x4130e3, ctl::CHART_LONG_BOND), (0x413134, ctl::CHART_SHORT_BOND),
        (0x413185, ctl::CHART_GDP_GROWTH)];
    for (addr, control) in cases {
        let g = common::world(cast::<u64, _>(addr));
        let Some(o) = common::run(addr, &["v:0", "v:0", "v:0", "v:0"], &g, 17, &[], &[(0x6141b1, 1)]) else { return };
        let mut p = g.clone();
        let mut ui = common::Rec::default();
        let mut rng = Rng::from_state(17);
        let env = Env::skipping(&LocalTime::HARNESS, &[0x6141b1]);
        desk_click(&mut p, &mut ui, &mut rng, &env, control, 0);
        let differences = common::cmp().diffs(&p, &o.globals);
        assert!(differences.is_empty(), "{addr:#x}: {differences:?}");
        assert_eq!(rng.state(), o.rng, "{addr:#x}");
    }
}

#[test]
fn streaming_quote_selects_its_watch_list_entity() {
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    let env = Env::skipping(&LocalTime::HARNESS, &[0x493e2b, 0x4227d5]);
    g.cur_player = 2;
    g.sel_entity = 131;
    g.watch_list[(3, 2)] = 202;
    desk_click(&mut g, &mut ui, &mut rng, &env, 803, 0);
    assert_eq!(g.target_corp, 202);
    assert_eq!(env.skipped(), vec![0x493e2b, 0x4227d5]);
}

#[test]
fn dispatch_matches_original_for_menu_and_stream_controls() {
    let __content = wsr_core::content::current();
    let cases: &[(u32, i32, &[(u32, u32)])] = &[
        (0x4110d7, ctl::MISC_MENU, &[(0x4131ea, 1)]),
        (0x411736, ctl::BUY_SELL_MENU, &[(0x481079, 2), (0x4131ea, 1)]),
        (0x4117da, ctl::FINANCING_MENU, &[(0x481079, 2), (0x4131ea, 1)]),
        (0x41187e, ctl::MANAGEMENT_MENU, &[(0x481079, 2), (0x4131ea, 1)]),
        (0x411922, ctl::OTHER_TRANS_MENU, &[(0x481079, 2), (0x4131ea, 1)]),
        (0x411610, ctl::STREAM_ADD_DELETE, &[(0x5dda89, 2), (0x493e2b, 1)]),
        (0x411684, ctl::STREAM_CLEAR, &[(0x493e2b, 1), (0x5dea80, 0)]),
        (0x4116dd, ctl::STREAM_FILL, &[(0x493e2b, 1), (0x5dec84, 0)]),
        (0x5df722, 803, &[(0x493e2b, 1), (0x4227d5, 0)]),
    ];
    for &(addr, control, skips) in cases {
        let mut g = common::world(cast::<u64, _>(addr));
        g.cur_player = 2;
        g.watch_list[(3, 2)] = 202;
        let wparam = format!("{__wsr_text_13741}{control}",__wsr_text_13741=__content.string(13741));
        let Some(o) = common::run(addr, &["v:0", "v:0", &wparam, "v:0"], &g, 17, &[], skips) else { return };
        let mut p = g.clone();
        let mut ui = common::Rec::default();
        let mut rng = Rng::from_state(17);
        let skip_addrs: Vec<u32> = skips.iter().map(|s| s.0).collect();
        let env = Env::skipping(&LocalTime::HARNESS, &skip_addrs);
        desk_click(&mut p, &mut ui, &mut rng, &env, control, 0);
        let differences = common::cmp().diffs(&p, &o.globals);
        assert!(differences.is_empty(), "{addr:#x}: {differences:?}");
        assert_eq!(rng.state(), o.rng, "{addr:#x}");
    }
}

#[test]
fn quick_search_callbacks_match_original_with_dialogs_skipped() {
    let cases: &[(u32, i32, &[(u32, u32)])] = &[
        (0x412090, ctl::LIST_FUTURES, &[(0x633d45, 2), (0x65b912, 2), (0x62cbaf, 2)]),
        (0x4125a3, ctl::RECALL_DB_SEARCH, &[(0x493e2b, 1), (0x54b072, 1), (0x5472ee, 1), (0x41ef3f, 1)]),
        (0x4127d9, ctl::LIST_PORTFOLIO, &[(0x493e2b, 1), (0x426610, 1), (0x5f0cb9, 4), (0x4422a4, 3)]),
    ];
    for &(addr, control, skips) in cases {
        let mut g = common::world(cast::<u64, _>(addr));
        g.cur_player = 1;
        g.sel_entity = 1;
        g.clock_state = PbStr::from("");
        g.sim_co_count = 100;
        g.turn_action_count = 0;
        g.game_end_flag = PbStr::from("N");
        let Some(o) = common::run(addr, &["v:0", "v:0", "v:0", "v:0"], &g, 17, &[], skips) else { return };
        let mut p = g.clone();
        let mut ui = common::Rec::default();
        let mut rng = Rng::from_state(17);
        let skip_addrs: Vec<u32> = skips.iter().map(|s| s.0).collect();
        let env = Env::skipping(&LocalTime::HARNESS, &skip_addrs);
        desk_click(&mut p, &mut ui, &mut rng, &env, control, 0);
        let differences = common::cmp().diffs(&p, &o.globals);
        assert!(differences.is_empty(), "{addr:#x}: {differences:?}");
        assert_eq!(rng.state(), o.rng, "{addr:#x}");
    }
}
