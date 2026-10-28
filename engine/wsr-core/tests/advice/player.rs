//! Differential tests of the player financial and tax advice.
use wsr_core::pb::real::cast;

use super::common::*;

#[test]
fn diff_player_advice() {
    par_seeds(0..40, |seed| {
        let mut g = advice_world(seed, 1);
        g.sel_entity = g.cur_player;
        g.active_entity = g.cur_player;
        g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        g.game_end_flag = st("N");
        check(seed, &g, 12345 + cast::<u32, _>(seed));
    });
}

#[test]
fn diff_player_game_end() {
    par_seeds(100..130, |seed| {
        let mut g = advice_world(seed, 1);
        g.sel_entity = g.cur_player;
        g.active_entity = g.cur_player;
        g.sel_corp_name = g.ent_name[g.sel_entity].clone();
        g.game_end_flag = st("Y");
        check(seed, &g, 12345 + cast::<u32, _>(seed));
    });
}
