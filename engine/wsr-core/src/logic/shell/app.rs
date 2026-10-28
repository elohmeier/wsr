//! Host-driven ticker and file helpers.
use crate::pb::real::{Ext, cast};


use super::prelude::*;

/// Start the ticker. The host owns the event loop and calls [`ticker_step`] as time advances.
pub fn run_ticker_loop(g: &mut Globals, _ui: &mut dyn Ui, _rng: &mut Rng, _env: &Env) {
    g.clock_state = s("RUN");
    g.run_loop_ticker_refreshes = 0;
}

/// Advance the running ticker once, respecting the stop request and pause deadline.
pub fn ticker_step(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    if g.game_end_flag == "Y" || g.ticker_stop_req == "Y" {
        // RunTickerLoop jumps directly to cleanup for these conditions; unlike the
        // ordinary stopped-clock path, it does not clear TickerOn.
        finish_ticker_loop(g, env);
        return;
    }
    if g.clock_state != "RUN" {
        g.ticker_on = 0;
        finish_ticker_loop(g, env);
        return;
    }
    let now = timer(env.now);
    if (cast::<Ext, _>(g.ticker_pause_until)) < now {
        g.ticker_pause_until = 0;
    }
    if now <= cast::<Ext, _>(g.ticker_pause_until) {
        if g.run_loop_ticker_refreshes < 2 {
            crate::logic::turn::refresh_news_listbox(g);
            g.run_loop_ticker_refreshes += 1;
        }
    } else if !env.skips(0x41ef3f) {
        crate::logic::turn::ticker_sim_steps_skipping(g, ui, rng, 0, env.now, env.skip);
    }
}

fn finish_ticker_loop(g: &mut Globals, env: &Env) {
    if g.ticker_dirty.is_empty() && !env.skips(0x41e749) {
        crate::logic::turn::refresh_news_listbox(g);
    }
    g.clock_state = PbStr::new();
}

/// Apply the state effects of updating the file menu. Menu presentation is computed by the host.
pub fn update_file_menu_state(g: &mut Globals, _env: &Env) {
    if g.game_in_progress == 1 && g.game_end_flag == "Y" {
        g.auto_add_stream_list = 0;
    }
}

/// BASIC FileExists returns -1 when the file exists, otherwise zero.
pub fn file_exists(env: &Env, path: &PbStr) -> i32 {
    if env.files.borrow().exists(path.as_bytes()) { -1 } else { 0 }
}

/// Save the current game, optionally asking for another game number.
pub fn save_game_mode(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, mode: i32) -> Option<PbStr> {
    let __content = crate::content::current();
    if g.game_in_progress == 1 && g.game_end_flag == "Y" {
        let mut message = s(__content.string(10015));
        show_quick_message(g, ui, &mut message);
        return None;
    }
    if ui.safety_checks() {
        if let Err(error) = crate::validation::validate_game(g) {
            ui.game_error(&s(&format!("Unable to save: {error}. The previous saved file has been kept.")), &g.app_title);
            return None;
        }
    }
    let checkpoint = ui.safety_checks().then(|| (g.clone(), *rng));
    if mode < 2 { g.clock_state = s("STOP"); }
    if mode == 1 {
        loop {
            let prompt = s(__content.string(10016)) + &str_s(cast::<Ext, _>(g.game_number)) + ".";
            let input = ui.inputbox(&prompt, &s(__content.string(14455)), &str_trim(cast::<Ext, _>(g.game_number)));
            let num = to_quad(val(remove_any(input.as_bytes(), b"$ ,").as_bytes()));
            if num <= 0 { return None; }
            if num > 50 { continue; }
            let name = crate::save::game_file_name(cast::<i32, _>(num));
            let occupied = env.files.borrow().read(name.as_bytes()).is_some_and(|bytes| !bytes.split(|b| *b == b'\n').next().unwrap_or_default().windows(6).any(|w| w == b"@DUMMY"));
            if occupied {
                let n = if num < 10 { s("0") + &str_trim(cast::<Ext, _>(num)) } else { str_trim(cast::<Ext, _>(num)) };
                let answer = ui.msgbox(&(s(__content.string(10017)) + &n + __content.string(10018)), 8227, &g.app_title);
                if answer == id::CANCEL { return None; }
                if answer == id::NO { continue; }
            }
            g.game_number = cast::<i32, _>(num);
            break;
        }
    }
    let (name, bytes) = crate::save::save_game_prompt(g, ui, rng, mode, &time_str(env.now), &date_str(env.now))?;
    if ui.safety_checks() {
        if let Err(error) = crate::validation::validate_game(g) {
            let (before, before_rng) = checkpoint.unwrap();
            *g = before; *rng = before_rng;
            ui.game_error(&s(&format!("Unable to save: {error}. The previous saved file has been kept.")), &g.app_title);
            return None;
        }
    }
    {
        let mut files = env.files.borrow_mut();
        files.write(name.as_bytes(), bytes);
        files.write(b"LASTGAME.DAT", crate::save::lastgame_dat(g));
    }
    if g.sim_co_count == 1600 && g.cur_auto_save == 1 {
        let mut message = s(__content.string(10019)) + &str_s(cast::<Ext, _>(g.cur_quarter))
            + " of" + &str_s(cast::<Ext, _>(g.cur_year)) + "....";
        show_quick_message(g, ui, &mut message);
    } else {
        ui.save_confirmation(&(s(__content.string(10020)) + &g.data_path + &name), &g.app_title);
    }
    if !env.skips(0x46e001) { crate::logic::turn::pause_ticker(g, &ui.local_time(), 1.0f32); }
    if !env.skips(0x41e7d5) { crate::logic::news::post_news(g, ui, 0, 0); }
    if !env.skips(0x41ef3f) { crate::logic::turn::ticker_sim_steps_skipping(g, ui, rng, 4, env.now, env.skip); }
    Some(name)
}

/// Save the current game under its current game number.
pub fn save_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) -> Option<PbStr> {
    save_game_mode(g, ui, rng, env, 0)
}

/// Load encoded save bytes. The caller supplies the bytes and chooses whether to repair the legacy codec.
/// On success, apply the original post-load state fixups.
pub fn load_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, data: &[u8], fixed: bool)
    -> Result<crate::save::LoadInfo, crate::save::LoadError>
{
    if ui.safety_checks() {
        let mut candidate = g.clone();
        let mut candidate_rng = *rng;
        let files = env.files.borrow().clone();
        let requests = env.requests.borrow().clone();
        match load_game_inner(&mut candidate, ui, &mut candidate_rng, env, data, fixed) {
            Ok(info) => { *g = candidate; *rng = candidate_rng; return Ok(info); }
            Err(error) => {
                *env.files.borrow_mut() = files;
                *env.requests.borrow_mut() = requests;
                return Err(error);
            }
        }
    }
    load_game_inner(g, ui, rng, env, data, fixed)
}

fn load_game_inner(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env, data: &[u8], fixed: bool)
    -> Result<crate::save::LoadInfo, crate::save::LoadError>
{
    let info = crate::save::read_save_file(g, ui, rng, data, fixed)?;
    if env.repair_initial_shares { repair_legacy_dividends(g, ui); }
    if ui.safety_checks() { crate::validation::validate_game(g).map_err(|message| crate::save::LoadError::InvalidState { message })?; }
    if !finish_loaded_game(g, ui, rng, env) {
        return Err(crate::save::LoadError::InvalidState { message: "ETF migration could not complete".into() });
    }
    if ui.safety_checks() { crate::validation::validate_game(g).map_err(|message| crate::save::LoadError::InvalidState { message })?; }
    Ok(info)
}

/// Browser recovery for saves made after original bug 67 divided by a zero share count.
/// The invalid dividends contain no recoverable payout; reset them explicitly before any report
/// or simulation uses them. The source bytes are only replaced on a subsequent user save.
pub fn repair_legacy_dividends(g: &mut Globals, ui: &mut dyn Ui) -> usize {
    let mut count = 0;
    for c in 11..=1600 {
        if !g.dividend_per_share[c].is_finite() {
            g.dividend_per_share[c] = Ext::from_f64(0.0);
            count += 1;
        }
    }
    if count > 0 {
        let message = s("This save contains invalid dividends for ") + &str_trim(cast::<Ext, _>(count))
            + " companies. These dividends have been reset to zero so they cannot corrupt reports or the simulation.";
        ui.msgbox(&message, 8192, &g.app_title);
    }
    count
}

/// State and host-side work after `LoadGame`, in MenuOpenSavedGame order. A host that reads a save
/// separately must supply any required company names before calling this, since ETF migration uses
/// their countries. Most callers can use [`load_game`].
pub fn finish_loaded_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) -> bool {
    let __content = crate::content::current();
    if !env.skips(0x5e0ade) { crate::logic::shorts::recalc_short_margin_balances(g); }
    if !env.skips(0x6b68ef) { crate::logic::economy::update_crypto_prices(g, rng); }
    for i in 1..=60 {
        g.news_headline[i] = crate::pb::string::replace_any(g.news_headline[i].as_bytes(), &[202], &[163]);
        g.news_headline[i] = crate::pb::string::replace_any(g.news_headline[i].as_bytes(), &[204], &[165]);
    }
    if !env.skips(0x629b50) { crate::logic::futures::init_commodity_markets(g, ui, rng); }
    if g.game_end_flag == "Y" { g.game_end_flag = s("N"); return true; }
    g.game_in_progress = 1;
    if !env.skips(0x649fc9) && !crate::logic::newgame_helpers::upgrade_save_add_etfs(g, ui, rng) { return false; }
    if g.cur_quarter == 1 && g.start_year == g.cur_year {
        for i in 11..=1600 {
            if (g.earnings_yr2_ago[i] != Ext::from_f64(0.0) || g.earnings_yr3_ago[i] != Ext::from_f64(0.0)) && g.entity_status[i] == 1 {
                g.price_hist_high[(0, i)] = -Ext::from_f64(1.0);
            }
        }
    }
    env.files.borrow_mut().write(b"LASTGAME.DAT", crate::save::lastgame_dat(g));
    if !env.skips(0x424754) { crate::logic::turn::enable_main_controls(g); }
    if !env.skips(0x46e2a0) { update_file_menu_state(g, env); }
    if !env.skips(0x4a054a) { crate::logic::turn::update_date_display(g); }
    if !env.skips(0x4a070a) { crate::logic::market::show_next_earnings_date(g); }
    if g.sel_industry == 0 && g.sel_entity > 10 { g.sel_industry = cast::<i32, _>(g.industry_num[g.sel_entity]); }
    if g.news_msg.is_empty() {
        g.news_msg = s("...") + &ucase(g.app_title.as_bytes(), CaseMode::Ansi) + __content.string(10021) + &time_str(env.now) + " ON " + &date_str(env.now) + "...";
    }
    if !env.skips(0x41e7d5) { crate::logic::news::post_news(g, ui, 0, 0); }
    if !env.skips(0x41e749) { crate::logic::turn::refresh_news_listbox(g); }
    if !env.skips(0x46dc2b) { crate::logic::newgame::create_industry_combo(g); }
    if !env.skips(0x4400d7) { crate::logic::market::update_market_display(g, ui, rng, env.now.second); }
    if g.max_bond_issue == 0 { g.max_bond_issue = to_quad(g.exchange_rate * Ext::from_f64(150000.0)); }
    if g.max_stock_offering_amt == 0 { g.max_stock_offering_amt = to_quad(g.exchange_rate * Ext::from_f64(100000.0)); }
    if g.cur_player > 0 { g.last_sel_entity = g.player_select_last_corp[g.cur_player]; }
    if !env.skips(0x5dcee5) {
        // The state-only panel port omits its internal PauseTicker call.
        if !env.skips(0x46e001) { crate::logic::turn::pause_ticker(g, &ui.local_time(), 0.0f32); }
        crate::logic::market::rebuild_streaming_quotes_panel(g, ui, 0);
    }
    for p in 1..=g.num_players {
        if g.is_computer_player[p] == 1 {
            g.player_suppress_popups[p] = 1;
            g.player_suppress_earnings[p] = 1;
            g.player_suppress_cash_warn[p] = 1;
            g.player_auto_save[p] = 0;
            g.player_auto_add_stream[p] = 0;
            g.player_exercise_options[p] = 1;
            g.player_make_delivery[p] = 1;
            g.player_take_delivery[p] = 0;
        }
    }
    if !env.skips(0x473796) { crate::logic::turn::update_settings_menu_text(g); }
    if !env.skips(0x41ef3f) { crate::logic::turn::ticker_sim_steps_skipping(g, ui, rng, 1, env.now, env.skip); }
    true
}

/// Ask for a game number and load the corresponding file from the environment.
pub fn menu_open_saved_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    if g.win_version == "XP" { env.files.borrow_mut().delete(b"GTIME.DAT"); }
    let last = env.files.borrow().read(b"LASTGAME.DAT").map(|bytes| to_long(val(bytes))).unwrap_or(0);
    let mut number = 0;
    if (1..=50).contains(&last) {
        let message = s(__content.string(10022)) + &str_s(cast::<Ext, _>(last)) + ")";
        match ui.msgbox(&message, 8227, &g.app_title) {
            id::CANCEL => return,
            id::YES => number = last,
            _ => {}
        }
    }
    while number == 0 {
        let input = ui.inputbox(&s(__content.string(10023)), &s(__content.string(10024)), &PbStr::new());
        let candidate = to_long(val(remove_any(input.as_bytes(), b"$ ,").as_bytes()));
        if candidate == 0 { return; }
        if (1..=50).contains(&candidate) { number = candidate; }
    }
    let previous_number = g.game_number;
    g.game_number = number;
    let name = crate::save::game_file_name(number);
    let data = env.files.borrow().read(name.as_bytes()).cloned();
    match data {
        Some(bytes) if !bytes.split(|b| *b == b'\n').next().unwrap_or_default().windows(6).any(|w| w == b"@DUMMY") => {
            match load_game(g, ui, rng, env, &bytes, false) {
                Ok(_) => {}
                Err(e) => {
                    let message = s(&format!("Unable to load {}: {e:?}", name.to_display()));
                    if ui.safety_checks() { g.game_number = previous_number; ui.game_error(&message, &g.app_title); }
                    else { ui.msgbox(&message, 8208, &g.app_title); }
                }
            }
        }
        _ => {
            let message = s("File ") + &g.data_path + &name + __content.string(10025) + &g.data_path + __content.string(10026);
            if ui.safety_checks() { g.game_number = previous_number; ui.game_error(&message, &g.app_title); }
            else { ui.msgbox(&message, 8208, &g.app_title); }
        }
    }
}
