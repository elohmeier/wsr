mod common;

use wsr_core::{
    content::{CONTENT_FILE, ContentPack, TextId},
    logic::newgame::LocalTime,
    pb::rng::Rng,
};
use wsr_wasm::session;

fn stored_content() -> Vec<u8> {
    unsafe {
        let name = CONTENT_FILE.as_bytes();
        let p = session::wsr_get_file(name.as_ptr(), name.len());
        let n = u32::from_le_bytes(std::slice::from_raw_parts(p, 4).try_into().unwrap()) as usize;
        let bytes = std::slice::from_raw_parts(p.add(4), n).to_vec();
        wsr_wasm::wsr_free(p, n + 4);
        bytes
    }
}

#[test]
fn import_is_atomic_and_catalogs_are_local_to_the_session() {
    // A new host session has no implicit original-content fallback.
    let mut ui = wsr_core::ui::Scripted::default();
    assert!(
        session::fresh_game(&LocalTime::HARNESS, &mut Rng::default())
            .ticker_text
            .is_empty()
    );
    assert!(session::command_json(&mut ui, 1, 0, b"").contains("runtime game content"));
    assert!(
        session::open_game_json(&mut ui, b"invalid", false, 1).contains("runtime game content")
    );

    common::install_content();
    let before = stored_content();
    assert!(session::put_file(CONTENT_FILE.as_bytes(), b"invalid replacement").is_err());
    assert_eq!(stored_content(), before);
    let fresh = session::fresh_game(&LocalTime::HARNESS, &mut Rng::default());
    assert_eq!(
        &fresh.ticker_text,
        common::synthetic_content().text(TextId::STARTUP_TICKER)
    );

    std::thread::spawn(|| {
        let fresh = session::fresh_game(&LocalTime::HARNESS, &mut Rng::default());
        assert!(
            fresh.ticker_text.is_empty(),
            "Catalog leaked into another session"
        );
        let bytes = common::synthetic_content().to_bytes();
        session::put_file(CONTENT_FILE.as_bytes(), &bytes).unwrap();
        assert_eq!(stored_content(), bytes);
    })
    .join()
    .unwrap();
    assert_eq!(
        ContentPack::parse(&stored_content()).unwrap().to_bytes(),
        before
    );
}
