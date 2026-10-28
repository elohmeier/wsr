#![allow(dead_code)]

use wsr_core::{
    content::{CONTENT_FILE, ContentPack, TextId},
    pb::pbstr::PbStr,
};

pub fn synthetic_content() -> ContentPack {
    ContentPack::from_entries(TextId::identifiers().enumerate().map(|(i, id)| {
        let text = if id == TextId::STARTUP_TICKER { "Synthetic startup ticker".to_owned() }
            else { format!("Synthetic resource {i:06}                         ") };
        (id, id.binary_length().map(|n| PbStr(vec![0; n])).unwrap_or_else(|| PbStr::from(text.as_str())))
    })).unwrap()
}

pub fn install_content() {
    wsr_wasm::session::put_file(CONTENT_FILE.as_bytes(), &synthetic_content().to_bytes()).unwrap();
}

pub fn install_original_content(directory: &std::path::Path) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "content-{}-{:?}.pack",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    let result = std::process::Command::new("node")
        .arg(root.join("web/extract-content.mjs"))
        .arg(directory)
        .arg(&output)
        .output()
        .expect("Node is required to extract original content");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&output).unwrap();
    std::fs::remove_file(output).unwrap();
    wsr_wasm::session::put_file(CONTENT_FILE.as_bytes(), &bytes).unwrap();
}
