//! Native test of the binding: write a save with the engine, load it back through the JSON entry point.
mod common;
use wsr_core::pb::real::Ext;


use wsr_core::pb::pbstr::PbStr;
use wsr_core::save;
use wsr_core::state::Globals;

#[test]
#[allow(clippy::field_reassign_with_default)]
fn load_save_json_reports_state() {
    common::install_content();
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.version_str = PbStr::from("9.85");
    g.num_players = 2;
    g.cur_player = 1;
    g.entity_status[1] = 1;
    g.entity_status[2] = 1;
    g.cur_year = 2025;
    g.cur_quarter = 3;
    g.player_name[1] = PbStr::from(__content.string(6719));
    g.player_name[2] = PbStr::from(__content.string(6811));
    g.ent_name[1] = g.player_name[1].clone();
    g.ent_name[2] = g.player_name[2].clone();
    g.cash[1] = Ext::from_f64(500.0);
    g.entity_status[1581] = 2;
    g.entity_status[131] = 1;
    g.ent_name[131] = PbStr::from("Acme Widgets Inc.");
    g.ent_symbol[131] = PbStr::from("ACME");
    g.market_cap[131] = Ext::from_f64(12.5);
    g.shares_outstanding[131] = Ext::from_f64(0.5);
    g.name_list_dirty = 1;
    let bytes = save::write_save_file(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    let json = wsr_wasm::load_save_json(&bytes, false);
    assert!(json.starts_with("{\"ok\":true"), "{json}");
    assert!(json.contains(&g.player_name[1].to_display()), "{json}");
    assert!(json.contains("\"ACME\""), "{json}");
    assert!(json.contains("\"year\":2025"), "{json}");
    std::fs::write(concat!(env!("CARGO_TARGET_TMPDIR"), "/synthetic-GAME01.DAT"), &bytes).unwrap();
}
