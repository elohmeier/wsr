//! Differential tests of BuildMenuAdviceText (0x557bbf): menus section.
use wsr_core::pb::real::cast;


#[allow(unused_imports)]
use super::common::*;

#[test]
fn mode3_matches_original() {
    par_seeds(1..=30, |seed| {
        let mut g = advice_world(seed, 3);
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.active_entity].clone();
        g.cur_player = if g.sel_entity < 11 {
            g.sel_entity
        } else {
            g.cur_player
        };
        check(seed, &g, cast::<u32, _>(seed));
    });
}

#[test]
fn mode5_matches_original() {
    par_seeds(1..=30, |seed| {
        let mut g = advice_world(seed, 5);
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.active_entity].clone();
        g.cur_player = if g.sel_entity < 11 {
            g.sel_entity
        } else {
            g.cur_player
        };
        check(seed, &g, cast::<u32, _>(seed));
    });
}

#[test]
fn mode7_matches_original() {
    par_seeds(1..=30, |seed| {
        let mut g = advice_world(seed, 7);
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.active_entity].clone();
        g.cur_player = if g.sel_entity < 11 {
            g.sel_entity
        } else {
            g.cur_player
        };
        check(seed, &g, cast::<u32, _>(seed));
    });
}

#[test]
fn mode4_matches_original() {
    par_seeds(1..=30, |seed| {
        let mut g = advice_world(seed, 4);
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.active_entity].clone();
        g.cur_player = if g.sel_entity < 11 {
            g.sel_entity
        } else {
            g.cur_player
        };
        check(seed, &g, cast::<u32, _>(seed));
    });
}

#[test]
fn mode6_matches_original() {
    par_seeds(1..=30, |seed| {
        let mut g = advice_world(seed, 6);
        g.active_entity = g.sel_entity;
        g.active_entity_name = g.ent_name[g.active_entity].clone();
        g.cur_player = if g.sel_entity < 11 {
            g.sel_entity
        } else {
            g.cur_player
        };
        check(seed, &g, cast::<u32, _>(seed));
    });
}
