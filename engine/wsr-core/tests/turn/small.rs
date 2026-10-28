//! Differential tests of the small turn helpers: ShowPendingPopup (0x4b25e8), TickerWatchListQuotes
//! (0x4bb959), UpdateSettingsMenuText (0x473796), EndTurn_RestoreActiveEntity (0x4262a6),
//! UpdateDateDisplay (0x4a054a), EnableMainControls (0x424754), RefreshNewsListbox (0x41e749).
//! (EndTurn_Stub 0x4265f6 is EndTurn's frame epilogue and cannot be called on its own.)
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::turn;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

fn small_world(seed: u64) -> (Globals, u32) {
    let mut g = turn_world(seed);
    let mut r = Rand(seed * 6151 + 11);
    // (CurPlayer 0 is error 9 in some of these; the GOSUB-like EndTurn_RestoreActiveEntity has no frame
    // for the error handler)
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.is_computer_player[g.cur_player.max(1)] = 1;
    }
    g.pending_popup_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { cast::<i32, _>(r.range(1, 1600)) } else { 0 };
    g.pending_event_amount = r.range(-500, 5000);
    g.pending_popup_pic = cast::<i32, _>(r.range(1, 35));
    g.pending_popup_text = st("some text");
    g.cur_suppress_earnings = cast::<i32, _>(r.range(0, 1));
    g.sim_co_count = cast::<i32, _>(r.range(1, 1600));
    (g, cast::<u32, _>(r.next()))
}

fn run_small(addr: u32, seed: u64, f: fn(&mut Globals, &mut TUi, &mut Rng)) {
    load_scenwin();
    let (g, rng) = small_world(seed);
    let what = format!("{addr:x} seed {seed}");
    let Some(o) = run(addr, &[], &g, rng, &[], &all_skips(&[])) else { return };
    let mut port = g.clone();
    let mut ui = TUi::default();
    let mut rr = Rng::from_state(rng);
    f(&mut port, &mut ui, &mut rr);
    check(&what, &port, &ui, rr.state(), &o);
}

#[test]
fn diff_show_pending_popup() {
    par_seeds(seeds(1..=30), |seed| run_small(0x4b25e8, seed, |g, ui, r| turn::show_pending_popup(g, ui, r)));
}

#[test]
fn diff_ticker_watch_list_quotes() {
    par_seeds(seeds(1..=30), |seed| run_small(0x4bb959, seed, |g, ui, r| turn::ticker_watch_list_quotes(g, ui, r)));
}

#[test]
fn diff_update_settings_menu_text() {
    par_seeds(seeds(1..=12), |seed| run_small(0x473796, seed, |g, _, _| turn::update_settings_menu_text(g)));
}

#[test]
fn diff_end_turn_restore_active_entity() {
    par_seeds(seeds(1..=12), |seed| {
        run_small(0x4262a6, seed, |g, _, _| turn::end_turn_restore_active_entity(g))
    });
}

#[test]
fn diff_update_date_display() {
    par_seeds(seeds(1..=12), |seed| run_small(0x4a054a, seed, |g, _, _| turn::update_date_display(g)));
}

#[test]
fn diff_ui_only_routines() {
    par_seeds(seeds(1..=4), |seed| {
        run_small(0x424754, seed, |g, _, _| turn::enable_main_controls(g));
        run_small(0x41e749, seed, |g, _, _| turn::refresh_news_listbox(g));
    });
}
