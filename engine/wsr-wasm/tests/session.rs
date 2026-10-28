//! Session binding coverage: complete load lifecycle, modal desk actions, and persisted save bytes.
mod common;

use wsr_core::pb::real::{Ext, cast};

use wsr_core::{logic::newgame::LocalTime, pb::{pbstr::PbStr, rng::Rng}, state::Globals, ui::{ReportAction, Ui}};
use wsr_wasm::session;

#[derive(Default)]
struct Host { menus: Vec<String> }
impl Ui for Host {
    fn local_time(&self) -> LocalTime { LocalTime::HARNESS }
    fn msgbox(&mut self, _: &PbStr, style: i32, _: &PbStr) -> i32 { if matches!(style & 15, 3 | 4) { 6 } else { 1 } }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, d: &PbStr) -> PbStr { d.clone() }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction { ReportAction::Ok }
    fn news(&mut self, _: &PbStr) {}
    fn choose(&mut self, title: &PbStr, _: &[PbStr], options: &[PbStr]) -> Option<usize> {
        self.menus.push(title.to_display()); Some(options.len() - 1)
    }
}
unsafe fn take(p: *mut u8) -> Vec<u8> {
    unsafe {
        let n = cast::<usize, _>(u32::from_le_bytes(*(p as *const [u8; 4])));
        let bytes = std::slice::from_raw_parts(p.add(4), n).to_vec();
        wsr_wasm::wsr_free(p, n + 4); bytes
    }
}
#[test]
fn load_act_and_save_through_session() {
    common::install_content();
    let mut ui = Host::default();
    session::put_file(b"REGINFO.DAT", b"\"Test Owner|12345\"\r\n").unwrap();
    assert_eq!(session::fresh_game(&LocalTime::HARNESS, &mut Rng::default()).licensee_name, PbStr::from("Test Owner"));
    session::put_file(b"C:\\WSR\\test.dat", b"data").unwrap();
    assert!(session::command_json(&mut ui, 0, 0, b"").contains("TEST.DAT"));
    let mut g = Globals::default();
    g.version_str = PbStr::from("9.85"); g.game_number = 7; g.name_list_dirty = 1;
    g.num_players = 2; g.cur_player = 1; g.game_in_progress = 1;
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.cur_year = 2025; g.start_year = 2020; g.game_end_year = 2030; g.cur_quarter = 3;
    g.sel_entity = 1; g.active_entity = 1; g.sim_co_count = 100;
    g.game_end_flag = PbStr::from("N"); g.ticker_stop_req = PbStr::from("Y");
    g.player_name[1] = PbStr::from("Alice"); g.ent_name[1] = PbStr::from("Alice");
    g.active_entity_name = PbStr::from("Alice"); g.sel_corp_name = PbStr::from("Alice");
    g.player_turn_order[1] = 1; g.player_turn_order[2] = 2;
    g.cash[1] = Ext::from_f64(500.0); g.cash[2] = Ext::from_f64(500.0);
    g.entity_status[1] = 1; g.entity_status[2] = 1;
    // A current-version game includes ETF records. Mark one liquidated so this deliberately tiny
    // fixture does not invoke the pre-6.30 fund migration, which needs a complete stock universe.
    g.entity_status[1581] = 2;
    let bytes = wsr_core::save::write_save_file(&mut g, &mut ui, &mut Rng::from_state(1));
    let out = session::open_game_json(&mut ui, &bytes, false, 7);
    assert!(out.starts_with("{\"ok\":true"), "{out}");
    assert!(out.contains("\"gameInProgress\":true"));
    assert!(out.contains("\"menus\":[{"));
    // Viewing another player's statement must not transfer the current turn to that player.
    let _ = wsr_wasm::report_json(8, 2);
    assert!(session::command_json(&mut ui, 0, 0, b"").contains("\"currentPlayer\":1"));
    let out = session::command_json(&mut ui, 2, 107, b"");
    assert!(out.starts_with("{\"ok\":true"), "{out}");
    assert_eq!(ui.menus.len(), 1);
    assert!(ui.menus[0].contains("Alice"));
    let out = session::command_json(&mut ui, 3, 615, b"");
    assert!(out.contains("GAME07.DAT"), "{out}");
    let name = b"GAME07.DAT";
    let bytes = unsafe { take(session::wsr_get_file(name.as_ptr(), name.len())) };
    assert!(bytes.starts_with(b"@EC9.85"));
    let changes = String::from_utf8(unsafe { take(session::wsr_file_changes()) }).unwrap();
    assert!(changes.contains("GAME07.DAT") && changes.contains("LASTGAME.DAT"), "{changes}");
    assert!(!String::from_utf8(unsafe { take(session::wsr_file_changes()) }).unwrap().contains("GAME07.DAT"));
}
