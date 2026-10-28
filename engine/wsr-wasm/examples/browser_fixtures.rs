//! Create browser regression states from an owner's complete save; no game data is bundled.
use wsr_core::pb::real::Ext;

use wsr_core::{pb::rng::Rng, save, ui::Scripted};
fn main() {
    let content = wsr_core::content::current();
    wsr_wasm::session::put_file(wsr_core::content::CONTENT_FILE.as_bytes(), &content.to_bytes()).unwrap();
    let mut args = std::env::args().skip(1);
    let input = args.next().expect("complete GAMExx.DAT");
    let output = std::path::PathBuf::from(args.next().expect("output directory"));
    std::fs::create_dir_all(&output).unwrap();
    let mut ui = Scripted::default();
    let mut rng = Rng::from_state(1);
    let mut g =
        wsr_wasm::session::fresh_game(&wsr_core::logic::newgame::LocalTime::HARNESS, &mut rng);
    save::read_save_file(
        &mut g,
        &mut ui,
        &mut rng,
        &std::fs::read(input).unwrap(),
        false,
    )
    .unwrap();
    assert!(g.game_end_year > g.cur_year, "Use an unfinished game");
    let corp = (131..=1580)
        .find(|&id| {
            g.entity_status[id] == 1
                && g.shares_outstanding[id] > Ext::from_f64(0.0)
                && (g.cap_hist_close[(3, id)] > Ext::from_f64(0.0)
                    || g.price_hist_high[(0, id)] != Ext::from_f64(0.0)
                    || g.earnings_yr2_ago[id] != Ext::from_f64(0.0)
                    || g.earnings_yr3_ago[id] != Ext::from_f64(0.0))
        })
        .expect("Save needs an active company with price history");
    // Preserve whether names are embedded. A source that needs CORPNAME.DAT must
    // still load it in the browser instead of serializing this helper's empty names.
    g.game_number = 48;
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.game_in_progress = 1;
    g.game_end_flag = "N".into();
    g.active_entity = 1;
    g.sel_entity = 1;
    g.sel_corp_name = g.player_name[1].clone();
    g.active_entity_name = g.player_name[1].clone();
    g.cur_player_name = g.player_name[1].clone();
    g.clock_state = "STOP".into();
    // Browser fixtures exercise human controls even when the source is an AI campaign save.
    g.action_code = "".into();
    assert!(wsr_core::logic::shell::desk::control_enabled(&g, 1));
    g.ticker_on = 0;
    g.ticker_stop_req = "N".into();
    g.player_select_last_corp[1] = corp;
    g.last_sel_entity = corp;
    g.player_suppress_earnings[1] = 0;
    g.cur_suppress_earnings = 0;
    // Loading runs one ticker step. It reaches the unselected, followed company's earnings popup.
    g.sim_co_count = corp - 1;
    g.turn_action_count = 0;
    std::fs::write(
        output.join("GAME48.DAT"),
        save::write_save_file(&mut g, &mut ui, &mut rng),
    )
    .unwrap();
    g.sim_co_count = 499; // The load lifecycle advances to the exact month-end slot (500).
    g.clock_state = "STOP".into();
    g.ticker_stop_req = "Y".into();
    g.solo_human_mode = 1;
    g.game_number = 49;
    g.num_players = 1;
    g.is_computer_player[1] = 0;
    std::fs::write(
        output.join("GAME49.DAT"),
        save::write_save_file(&mut g, &mut ui, &mut rng),
    )
    .unwrap();
    // Commodity controls become available after the initial January suspension.
    g.sim_co_count = 599;
    g.game_number = 45;
    std::fs::write(output.join("GAME45.DAT"), save::write_save_file(&mut g, &mut ui, &mut rng)).unwrap();
    g.sim_co_count = 499;
    g.game_number = 49;
    // A corrupt import must not replace the live session or its last good saved bytes.
    for company in 11..=1600 {
        if !g.dividend_per_share[company].is_finite() {
            g.dividend_per_share[company] = Ext::from_f64(0.0);
        }
    }
    let exchange_rate = g.exchange_rate;
    g.exchange_rate = f64::INFINITY;
    std::fs::write(
        output.join("GAME47.DAT"),
        save::write_save_file(&mut g, &mut ui, &mut rng),
    )
    .unwrap();
    g.exchange_rate = exchange_rate;
    let issuer = (131..=1580).find(|&id| g.entity_status[id] == 1).unwrap();
    g.bonds_issued[issuer] = 100;
    g.bond_issue_info[issuer] = format!("{}cv|0|@04.00", g.cur_year + 5).as_str().into();
    std::fs::write(
        output.join("GAME46.DAT"),
        save::write_save_file(&mut g, &mut ui, &mut rng),
    )
    .unwrap();
    println!(
        "Created earnings-chart, solo-human and invalid-finances fixtures in {}",
        output.display()
    );
}
