//! Host-facing ticker and file behavior.
use wsr_core::pb::real::Ext;


use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::{Env, Files};
use wsr_core::logic::shell::app::{file_exists, ticker_step, update_file_menu_state};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Scripted;

#[test]
fn virtual_file_lookup_uses_original_boolean_and_case_insensitive_basename() {
    let mut files = Files::default();
    files.write(b"C:\\WSR\\Game01.dat", b"save".to_vec());
    let env = Env::new(&LocalTime::HARNESS).with_files(files);
    assert_eq!(file_exists(&env, &PbStr::from("game01.DAT")), -1);
    assert_eq!(file_exists(&env, &PbStr::from("missing.dat")), 0);
}

#[test]
fn stopped_ticker_does_not_run_and_game_end_disables_streaming() {
    let env = Env::new(&LocalTime::HARNESS);
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    g.clock_state = PbStr::from("RUN");
    g.ticker_on = 1;
    g.game_in_progress = 1;
    g.game_end_flag = PbStr::from("Y");
    g.auto_add_stream_list = 1;
    ticker_step(&mut g, &mut ui, &mut rng, &env);
    update_file_menu_state(&mut g, &env);
    assert!(g.clock_state.is_empty());
    assert_eq!(g.ticker_on, 1); // Stop-request/game-end takes the original early exit.
    assert_eq!(g.auto_add_stream_list, 0);
}

#[test]
fn save_as_cancel_does_not_write_a_file() {
    let env = Env::new(&LocalTime::HARNESS);
    let mut g = Globals::default();
    g.game_number = 1;
    let mut ui = Scripted::default();
    ui.answers.push_back(PbStr::from("0"));
    let mut rng = Rng::from_state(1);
    assert!(wsr_core::logic::shell::app::save_game_mode(&mut g, &mut ui, &mut rng, &env, 1).is_none());
    assert!(env.files.borrow().written.is_empty());
    assert_eq!(g.game_number, 1);
}

#[test]
fn opening_missing_game_reports_error() {
    let __content = wsr_core::content::current();
    let env = Env::new(&LocalTime::HARNESS);
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    ui.answers.push_back(PbStr::from("7"));
    let mut rng = Rng::from_state(1);
    wsr_core::logic::shell::app::menu_open_saved_game(&mut g, &mut ui, &mut rng, &env);
    assert_eq!(g.game_number, 7);
    assert!(ui.log.iter().any(|line| line.contains("GAME07.DAT") && line.contains(__content.string(14130))));
}

#[test]
fn save_records_encoded_game_and_last_game_number() {
    let __content = wsr_core::content::current();
    let skips = [0x41ef3f];
    let env = Env::skipping(&LocalTime::HARNESS, &skips);
    let mut g = super::common::world(77);
    g.game_number = 7;
    g.version_str = PbStr::from("9.85");
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    let name = wsr_core::logic::shell::app::save_game(&mut g, &mut ui, &mut rng, &env).unwrap();
    assert_eq!(name, PbStr::from("GAME07.DAT"));
    assert_eq!(g.ticker_pause_until, 36_001); // SaveGamePrompt pauses for one second.
    let files = env.files.borrow();
    assert!(files.read(b"GAME07.DAT").unwrap().starts_with(b"@EC9.85"));
    assert_eq!(files.read(b"LASTGAME.DAT").unwrap(), &wsr_core::save::lastgame_dat(&g));
    assert!(ui.log.iter().any(|line| line.contains(__content.string(14465))));
}

#[test]
fn quarter_end_autosave_uses_the_original_notice() {
    let __content = wsr_core::content::current();
    let env = Env::skipping(&LocalTime::HARNESS, &[0x41ef3f]);
    let mut g = super::common::world(77);
    g.game_number = 7;
    g.sim_co_count = 1600;
    g.cur_auto_save = 1;
    let mut ui = Scripted::default();
    wsr_core::logic::shell::app::save_game_mode(&mut g, &mut ui, &mut Rng::from_state(1), &env, 2).unwrap();
    assert!(ui.log.iter().any(|line| line.contains(__content.string(10019))));
    assert!(!ui.log.iter().any(|line| line.contains(__content.string(14131))));
}

#[test]
fn legacy_dividend_recovery_reports_repairs_and_preserves_valid_payouts() {
    let mut g = Globals::default();
    g.dividend_per_share[11] = Ext::NAN;
    g.dividend_per_share[12] = Ext::INFINITY;
    g.dividend_per_share[13] = Ext::from_f64(1.25);
    let mut ui = Scripted::default();
    assert_eq!(wsr_core::logic::shell::app::repair_legacy_dividends(&mut g, &mut ui), 2);
    assert_eq!(g.dividend_per_share[11], Ext::from_f64(0.0));
    assert_eq!(g.dividend_per_share[12], Ext::from_f64(0.0));
    assert_eq!(g.dividend_per_share[13], Ext::from_f64(1.25));
    assert!(ui.log.iter().any(|line| line.contains("invalid dividends for 2 companies")));
    let before = ui.log.len();
    assert_eq!(wsr_core::logic::shell::app::repair_legacy_dividends(&mut g, &mut ui), 0);
    assert_eq!(ui.log.len(), before);
}

#[test]
fn running_ticker_waits_for_pause_then_requests_a_tick() {
    let skips = [0x41ef3f];
    let now = LocalTime::HARNESS;
    let env = Env::skipping(&now, &skips);
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    g.clock_state = PbStr::from("RUN");
    g.ticker_pause_until = 36_001;
    ticker_step(&mut g, &mut ui, &mut rng, &env);
    assert!(env.skipped().is_empty());
    assert_eq!(g.run_loop_ticker_refreshes, 1);
    g.ticker_pause_until = 35_999;
    ticker_step(&mut g, &mut ui, &mut rng, &env);
    assert_eq!(g.ticker_pause_until, 0);
    assert_eq!(env.skipped(), vec![0x41ef3f]);
}

#[test]
fn ticker_cleans_up_a_stopped_clock_and_refreshes_once() {
    let env = Env::skipping(&LocalTime::HARNESS, &[0x41e749]);
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    g.clock_state = PbStr::from("STOP");
    g.ticker_on = 1;
    ticker_step(&mut g, &mut ui, &mut rng, &env);
    assert!(g.clock_state.is_empty());
    assert_eq!(g.ticker_on, 0);
    assert_eq!(env.skipped(), vec![0x41e749]);
}

#[test]
fn successful_load_updates_last_game_and_runs_post_load_actions_in_order() {
    let mut source = super::common::world(77);
    source.game_number = 7;
    source.version_str = PbStr::from("9.85");
    source.game_end_flag = PbStr::from("N");
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    let (_, bytes) = wsr_core::save::save_game_prompt(&mut source, &mut ui, &mut rng, 0, &PbStr::from("10:00:00"), &PbStr::from("01-01-2020")).unwrap();
    let skips = [0x5e0ade, 0x6b68ef, 0x629b50, 0x649fc9, 0x424754, 0x46e2a0, 0x4a054a, 0x4a070a, 0x41e7d5, 0x41e749, 0x46dc2b, 0x4400d7, 0x5dcee5, 0x473796, 0x41ef3f];
    let env = Env::skipping(&LocalTime::HARNESS, &skips);
    let mut loaded = Globals::default();
    wsr_core::logic::shell::app::load_game(&mut loaded, &mut ui, &mut rng, &env, &bytes, true).unwrap();
    assert_eq!(loaded.game_in_progress, 1);
    assert_eq!(env.files.borrow().read(b"LASTGAME.DAT").unwrap(), &wsr_core::save::lastgame_dat(&loaded));
    assert_eq!(env.skipped(), skips);
}
