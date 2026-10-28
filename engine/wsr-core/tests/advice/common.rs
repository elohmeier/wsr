//! Shared helpers of the advice tests: an advice-menu world on top of the sim_entity world and the
//! differential check of BuildMenuAdviceText (0x557bbf) for one state.
#![allow(dead_code)]
use wsr_core::pb::real::{Ext, cast};


pub use super::diff::{Compare, Rand, par_seeds};
pub use super::sim_common::{Rec, cmp, f64r, orig_msgs, run_hooked, st, word, world};
use wsr_core::logic::advice::build_menu_advice_text_body;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

pub const BUILD_MENU_ADVICE_TEXT: u32 = 0x557bbf;
/// UpdateMarketDisplay (0x4400d7): skipped in the original, the port calls the body only.
pub const UPDATE_MARKET_DISPLAY: u32 = 0x4400d7;

/// Skipped routines of every run: (address, stdcall dword args).
pub fn skips() -> Vec<(u32, u32)> {
    vec![(UPDATE_MARKET_DISPLAY, 0)]
}

/// A plausible state for the transactions menu `mode` (1..7): the sim_entity world plus currency,
/// selected / active entity, headlines and player data.
pub fn advice_world(seed: u64, mode: i32) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ (0x5eed_0000 + cast::<u64, _>(mode)) | 1);
    g.action_menu_mode = mode;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    (g.currency_code, g.currency_symbol, g.currency_suffix) = match r.range(0, 5) {
        0 => (st("UK"), st("£"), st("")),
        1 => (st("JAP"), st("¥"), st("")),
        2 => (st("SWI"), st("SFr "), st("")),
        3 => (st("EUR"), st(" € "), st(" euros")),
        _ => (st("USA"), st("$"), st("")),
    };
    // selected entity: a player (often the current one) or a company / bank / insurer / fund
    g.sel_entity = match r.range(0, 5) {
        0 | 1 => g.cur_player,
        2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
        3 => cast::<i32, _>([r.range(11, 130), r.range(1581, 1600)][cast::<usize, _>(r.range(0, 1))]),
        _ => cast::<i32, _>(r.range(131, 1580)),
    };
    g.sel_corp_name = g.ent_name[g.sel_entity].clone();
    // active entity: usually the selected one when the player controls it
    g.active_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        g.sel_entity
    } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.cur_player
    } else {
        cast::<i32, _>(r.range(11, 1600))
    };
    if g.active_entity > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
        g.corp_controller[g.active_entity] = g.cur_player;
    }
    g.active_entity_name = g.ent_name[g.active_entity].clone();
    g.sel_industry = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        0
    } else {
        cast::<i32, _>(r.range(1, 71))
    };
    // headlines naming the selected company
    let sym = g.ent_symbol[g.sel_entity].clone();
    for i in 1..=60 {
        g.news_headline[i] = match r.range(0, 6) {
            0 => st(&format!("{__wsr_text_13546}{i} about ({}) news", sym.to_display(),__wsr_text_13546=__content.string(13546))),
            1 => st(&format!("{} reports earnings of {i}", sym.to_display())),
            2 => st(&format!("Something about {}.", sym.to_display())),
            3 => st(&format!("Story {i} {}{__wsr_text_13547}", sym.to_display(),__wsr_text_13547=__content.string(13547))),
            _ => word(&mut r, 30),
        };
    }
    g.start_net_worth = cast::<i32, _>(r.range(0, 3000));
    g.game_end_flag = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { st("Y") } else { st("N") };
    g
}

/// Run the original and the port (body only, `UpdateMarketDisplay`
/// skipped) on `g` with RND state `rng_state`; compare `HelpText`, all globals, RND state and the number
/// of message boxes. Returns the port's state.
pub fn check(seed: u64, g: &Globals, rng_state: u32) -> Option<Globals> {
    check_with(seed, g, rng_state, cmp())
}

/// [`check`] with a custom comparison.
pub fn check_with(seed: u64, g: &Globals, rng_state: u32, compare: Compare) -> Option<Globals> {
    let __content = wsr_core::content::current();
    let o = run_hooked(
        BUILD_MENU_ADVICE_TEXT,
        &[],
        g,
        rng_state,
        &[],
        &skips(),
        &[],
    )?;
    let mut p = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng_state);
    build_menu_advice_text_body(&mut p, &mut ui, &mut rng);
    assert_eq!(
        p.help_text.to_display(),
        o.globals.help_text.to_display(),
        "seed {seed}{__wsr_text_13548}{}: HelpText (port vs original)",
        g.action_menu_mode
    ,__wsr_text_13548=__content.string(13548));
    let d = compare.diffs(&p, &o.globals);
    assert!(
        d.is_empty(),
        "seed {seed}{__wsr_text_13548}{}: globals differ:\n{}",
        g.action_menu_mode,
        d.join("\n")
    ,__wsr_text_13548=__content.string(13548));
    assert_eq!(rng.state(), o.rng, "seed {seed}: RND state");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(&o),
        "seed {seed}: message boxes {:?}",
        ui.msgs
    );
    Some(p)
}
