//! A complete multi-year session with AI turns, disk-compatible saves and reload.
//! Uses the owner's extracted assets, like the original-executable differential tests.
mod common;

use wsr_core::pb::real::cast;

use std::{cell::Cell, path::PathBuf};
use wsr_core::{
    logic::newgame::LocalTime,
    pb::{pbstr::PbStr, rng::Rng},
    save,
    state::Globals,
    ui::{ReportAction, Ui},
};
use wsr_wasm::session;

#[derive(Default)]
struct Player {
    seconds: Cell<i32>,
    messages: usize,
    headlines: usize,
}
impl Ui for Player {
    fn local_time(&self) -> LocalTime {
        let seconds = self.seconds.get() + 2;
        self.seconds.set(seconds);
        LocalTime {
            hour: 10 + seconds / 3600,
            minute: seconds / 60 % 60,
            second: seconds % 60,
            ..LocalTime::HARNESS
        }
    }
    fn msgbox(&mut self, _: &PbStr, style: i32, title: &PbStr) -> i32 {
    let __content = wsr_core::content::current();
        self.messages += 1;
        if title.to_display().ends_with(__content.string(13911)) {
            6
        } else if matches!(style & 15, 3 | 4) {
            7
        } else {
            1
        }
    }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        let p = prompt.to_display();
        if *title == __content.string(6635) {
            return PbStr::from("2");
        }
        if p.starts_with(__content.string(13759)) {
            return PbStr::from(__content.string(14385));
        }
        if p.starts_with(__content.string(14386)) {
            return PbStr::from("Campaign Player");
        }
        if p.starts_with(__content.string(13913)) {
            return PbStr::from("3");
        }
        default.clone()
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {
        self.headlines += 1;
    }
    fn choose(&mut self, _: &PbStr, _: &[PbStr], _: &[PbStr]) -> Option<usize> {
        None
    }
}
unsafe fn file(name: &[u8]) -> Vec<u8> {
    unsafe {
        let p = session::wsr_get_file(name.as_ptr(), name.len());
        let n = cast::<usize, _>(u32::from_le_bytes(*(p as *const [u8; 4])));
        let out = std::slice::from_raw_parts(p.add(4), n).to_vec();
        wsr_wasm::wsr_free(p, n + 4);
        out
    }
}
fn number(json: &str, key: &str) -> i32 {
    json.split(&format!("\"{key}\":"))
        .nth(1)
        .unwrap()
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
fn assert_finite_financials(g: &Globals) {
    let __content = wsr_core::content::current();
    for e in 1..=1600 {
        assert!(g.cash[e].is_finite(), "{__wsr_text_14387}{e}",__wsr_text_14387=__content.string(14387));
        assert!(g.bank_debt[e].is_finite(), "{__wsr_text_14388}{e}",__wsr_text_14388=__content.string(14388));
        if e >= 11 {
            assert!(g.dividend_per_share[e].is_finite(), "{__wsr_text_10795}{e}",__wsr_text_10795=__content.string(10795));
            assert!(g.market_cap[e].is_finite(), "{__wsr_text_14389}{e}",__wsr_text_14389=__content.string(14389));
            assert!(g.shares_outstanding[e].is_finite(), "{__wsr_text_14390}{e}",__wsr_text_14390=__content.string(14390));
        }
    }
}
fn import_assets() -> bool {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/app");
    if !directory.join("wsr101.dat").exists() {
        assert!(
            std::env::var_os("WSR_REQUIRE_ORIGINAL").is_none(),
            "Required original campaign assets are missing"
        );
        eprintln!("Campaign skipped: extract the owner's wsr985.exe into extracted/app.");
        return false;
    }
    common::install_original_content(&directory);
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_uppercase();
        if (name.ends_with(".DAT") || name.ends_with(".WSR")) && !name.starts_with("GAME") {
            session::put_file(name.as_bytes(), &std::fs::read(&path).unwrap()).unwrap();
        }
    }
    true
}

#[test]
fn customized_countries_cannot_hang_startup_or_replace_the_current_game() {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(|| {
            if !import_assets() {
                return;
            }
            let mut ui = Player::default();
            let state = session::command_json(&mut ui, 1, 0, b"");
            assert!(state.starts_with("{\"ok\":true"), "{state}");
            let state = session::command_json(&mut ui, 3, 615, b"");
            assert!(state.starts_with("{\"ok\":true"), "{state}");
            let original = unsafe { file(b"CORPNAME.DAT") };
            let mut custom = original.clone();
            // The second | field is the Customizer's two-digit country code.
            for line in custom.split_mut(|&c| c == b'\n') {
                if let Some(country) = line.split_mut(|&c| c == b'|').nth(1) {
                    if country.len() == 2 {
                        country.copy_from_slice(b"00");
                    }
                }
            }
            assert_ne!(custom, original);
            session::put_file(b"CORPNAME.DAT", &custom).unwrap();
            let before = session::command_json(&mut ui, 0, 0, b"");
            let saved = unsafe { file(b"GAME01.DAT") };
            let last = unsafe { file(b"LASTGAME.DAT") };
            let rejected = session::command_json(&mut ui, 1, 0, b"");
            assert!(
                rejected.contains("Cannot initialize ETF 1593"),
                "{rejected}"
            );
            assert!(rejected.contains("only 0 are available"), "{rejected}");
            assert_eq!(session::command_json(&mut ui, 0, 0, b""), before);
            assert_eq!(unsafe { file(b"GAME01.DAT") }, saved);
            assert_eq!(unsafe { file(b"LASTGAME.DAT") }, last);
            assert_eq!(unsafe { file(b"CORPNAME.DAT") }, custom);
            session::put_file(b"CORPNAME.DAT", &original).unwrap();
            let recovered = session::command_json(&mut ui, 1, 0, b"");
            assert!(recovered.starts_with("{\"ok\":true"), "{recovered}");
        });
        let _ = send.send(result);
    });
    if let Err(panic) = receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("customized startup exceeded its bound")
    {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn three_year_game_with_ai_save_and_reload() {
    let __content = wsr_core::content::current();
    if !import_assets() {
        return;
    }
    let mut ui = Player::default();
    let mut state = session::command_json(&mut ui, 1, 0, b"");
    assert!(state.starts_with("{\"ok\":true"), "{state}");
    let start = number(&state, "year");
    assert!(state.contains("\"computer\":true"));
    let _ = session::command_json(&mut ui, 3, 615, b"");
    let initial_bytes = unsafe { file(b"GAME01.DAT") };
    let mut initial = Globals::default();
    save::read_save_file(
        &mut initial,
        &mut ui,
        &mut Rng::default(),
        &initial_bytes,
        false,
    )
    .unwrap();
    assert_finite_financials(&initial);
    let mut saved = false;
    for turn in 0..25000 {
        // End-turn alone leaves the solo human's clock stopped; time advances on ticker steps.
        if !state.contains("\"clock\":\"RUN\"") {
            let _ = session::command_json(&mut ui, 2, 249, b"");
        }
        state = session::command_json(&mut ui, 5, 0, b"");
        assert!(state.starts_with("{\"ok\":true"), "{state}");
        if !saved && number(&state, "year") > start {
            let _ = session::command_json(&mut ui, 3, 615, b"");
            let bytes = unsafe { file(b"GAME01.DAT") };
            assert!(bytes.starts_with(b"@EC9.85"));
            let mut snapshot = Globals::default();
            save::read_save_file(&mut snapshot, &mut ui, &mut Rng::default(), &bytes, false)
                .unwrap();
            assert!(snapshot.stock_index.is_finite() && snapshot.stock_index > 0.0);
            assert_finite_financials(&snapshot);
            for player in 1..=snapshot.num_players {
                assert!(snapshot.cash[player].is_finite());
            }
            state = session::open_game_json(&mut ui, &bytes, false, 1);
            assert!(state.starts_with("{\"ok\":true"), "{state}");
            saved = true;
        }
        if state.contains("\"gameOver\":true") {
            assert!(saved && number(&state, "year") >= start + 3);
            assert!(ui.headlines > 100);
            eprintln!(
                "Campaign completed after {} ticker steps, {} notices, {}{__wsr_text_13766}",
                turn + 1,
                ui.messages,
                ui.headlines
            ,__wsr_text_13766=__content.string(13766));
            return;
        }
    }
    panic!("Campaign did not finish: {state}");
}
