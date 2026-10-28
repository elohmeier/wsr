//! Differential coverage for action-menu command dispatch.
use wsr_core::pb::real::cast;


use super::common::{cmp, run, world};
use super::support::assert_same_skips;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::{menu::execute_menu_command, Env};
use wsr_core::pb::rng::Rng;
use wsr_core::ui::Scripted;

#[test]
fn miscellaneous_price_alert_dispatch_matches_original() {
    let skips = [(0x49117c, 0), (0x6828ca, 0)];
    let skip_addresses: Vec<_> = skips.iter().map(|entry| entry.0).collect();
    for seed in 0..8 {
        let mut g = world(seed);
        g.action_menu_mode = 3;
        g.active_entity = g.cur_player;
        g.turn_action_count = 0;
        let mut port = g.clone();
        let mut ui = Scripted::default();
        let mut rng = Rng::from_state(cast::<u32, _>(seed) + 1);
        let env = Env::skipping(&LocalTime::HARNESS, &skip_addresses);
        execute_menu_command(&mut port, &mut ui, &mut rng, &env, 7);
        if let Some(original) = run(0x553664, &["7"], &g, cast::<u32, _>(seed) + 1, &[], &skips) {
            let diffs = cmp().diffs(&port, &original.globals);
            assert!(diffs.is_empty(), "seed {seed}: {diffs:?}");
            assert_eq!(rng.state(), original.rng, "seed {seed}");
            assert_same_skips(seed, &env.skipped(), &original);
        }
    }
}
