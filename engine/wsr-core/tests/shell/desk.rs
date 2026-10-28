//! Tests of logic::shell::desk.

#[allow(unused_imports)]
use super::common::*;

#[test]
fn file_menu_tracks_game_lifecycle() {
    use wsr_core::logic::newgame::LocalTime;
    use wsr_core::logic::shell::{desk::{cmd, desk_menu_bar}, Env};
    use wsr_core::pb::pbstr::PbStr;
    use wsr_core::state::Globals;

    let env = Env::new(&LocalTime::HARNESS);
    let mut g = Globals::default();
    let enabled = |items: &[wsr_core::logic::shell::desk::MenuItem], id| !items.iter().find(|item| item.id == id).unwrap().grayed;
    let items = desk_menu_bar(&g, &env);
    assert!(enabled(&items, cmd::OPEN_SAVED_GAME));
    assert!(!enabled(&items, cmd::SAVE_GAME));
    g.game_in_progress = 1;
    let items = desk_menu_bar(&g, &env);
    assert!(!enabled(&items, cmd::OPEN_SAVED_GAME));
    assert!(enabled(&items, cmd::SAVE_GAME));
    g.game_end_flag = PbStr::from("Y");
    let items = desk_menu_bar(&g, &env);
    assert!(!enabled(&items, cmd::SAVE_GAME));
    assert!(!enabled(&items, cmd::AUTO_SAVE));
}
