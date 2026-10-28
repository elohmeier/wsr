//! Host session: original desk actions, a persistent virtual data directory, and live state.
use wsr_core::pb::real::{Ext, cast};


use crate::{CONTENT, FILES, GAME, RNG, result, summary};
use wsr_core::content::{ContentPack, TextId, CONTENT_FILE};
use wsr_core::logic::{self, newgame::LocalTime, shell::{Env, HostRequest, desk}};
use wsr_core::pb::{pbstr::PbStr, rng::Rng, numeric::to_long};
use wsr_core::{state::Globals, ui::{DialogKind, ReportAction, Ui}};

/// Simulation writes and shell writes share one ordered file view for this command. A ticker
/// step after a manual save can autosave the same slot again; that later write must win.
struct FileUi<'a> {
    inner: &'a mut dyn Ui,
    files: &'a std::cell::RefCell<logic::shell::Files>,
    error: &'a std::cell::RefCell<Option<String>>,
}

impl Ui for FileUi<'_> {
    fn safety_checks(&self) -> bool { true }
    fn game_error(&mut self, text: &PbStr, title: &PbStr) {
        *self.error.borrow_mut() = Some(text.to_display());
        self.inner.game_error(text, title);
    }
    fn local_time(&self) -> LocalTime { self.inner.local_time() }
    fn write_file(&mut self, name: &PbStr, data: &[u8]) {
        self.files.borrow_mut().write(name.as_bytes(), data.to_vec());
        self.inner.write_file(name, data);
    }
    fn save_confirmation(&mut self, text: &PbStr, title: &PbStr) { self.inner.save_confirmation(text, title); }
    fn end_dialog(&mut self, dialog: DialogKind) { self.inner.end_dialog(dialog); }
    fn random_remark(&mut self) -> PbStr { self.inner.random_remark() }
    fn high_score(&mut self, path: &PbStr, name: &PbStr, score: i64) -> i32 { self.inner.high_score(path, name, score) }
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 { self.inner.msgbox(text, style, title) }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr { self.inner.inputbox(prompt, title, default) }
    fn form(&mut self, form: &wsr_core::ui::Form) -> Option<wsr_core::ui::FormResponse> { self.inner.form(form) }
    fn symbol_input(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr, companies: &[wsr_core::ui::SymbolChoice]) -> PbStr { self.inner.symbol_input(prompt, title, default, companies) }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction { self.inner.report(title, lines, yes_no) }
    fn report_with_actions(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool, options: &wsr_core::ui::ReportOptions) -> ReportAction { self.inner.report_with_actions(title, lines, yes_no, options) }
    fn news(&mut self, headline: &PbStr) { self.inner.news(headline); }
    fn ownership_diagram(&mut self, spec: &logic::info_screens::ownership::OwnershipDiagram) -> ReportAction { self.inner.ownership_diagram(spec) }
    fn option_station(&mut self, spec: &logic::trade_screens::option_station::OptionStation)
        -> Option<Vec<logic::trade_screens::option_station::StationAction>> { self.inner.option_station(spec) }
    fn chart(&mut self, spec: &logic::info_screens::chart::ChartSpec) -> bool { self.inner.chart(spec) }
    fn choose(&mut self, title: &PbStr, prompt: &[PbStr], options: &[PbStr]) -> Option<usize> { self.inner.choose(title, prompt, options) }
}

/// Platform-independent startup state from WinMain (0x40148f), before opening the trading desk.
/// Data-directory validation and config loading happen when a game is started, after file import.
pub fn fresh_game(now: &LocalTime, rng: &mut Rng) -> Globals {
    let mut g = Globals::default();
    g.rand_seed = to_long(cast::<Ext, _>(now.timer_ms()) / Ext::from_f64(1000.0));
    if g.rand_seed > 30_000 { g.rand_seed = to_long(cast::<Ext, _>(g.rand_seed) / Ext::from_f64(3.0)); }
    rng.randomize_timer(now.timer_ms());
    g.rand_seed = to_long(rng.rnd() * Ext::from_f64(10000.0) + cast::<Ext, _>(g.rand_seed));
    g.app_title = PbStr::from("Wall Street Raider");
    g.exe_name = PbStr::from("WSR.EXE");
    g.version_str = PbStr::from("9.85");
    g.ticker_text = CONTENT.with(|content| content.borrow().as_ref()
        .map(|content| content.text(TextId::STARTUP_TICKER).clone()).unwrap_or_default());
    if CONTENT.with(|content| content.borrow().is_some()) {
        logic::newgame_helpers::init_month_names(&mut g);
        logic::news::load_currency_headlines(&mut g);
    }
    FILES.with(|f| {
        if let Some(bytes) = f.borrow().read(b"REGINFO.DAT") { import_registration(&mut g, bytes); }
    });
    g
}

fn import_registration(g: &mut Globals, bytes: &[u8]) {
    if let Ok(record) = wsr_core::pb::numeric::InputReader::new(bytes).try_input_str()
        && wsr_core::pb::string::parse_count_any(&record, b"|") == 2 {
        g.licensee_name = wsr_core::pb::string::parse_any(&record, b"|", 1);
        g.registration_num = wsr_core::pb::string::parse_any(&record, b"|", 2);
    }
}

pub fn put_file(name: &[u8], data: &[u8]) -> Result<(), String> {
    let key = logic::shell::Files::key(name);
    // Validate the candidate before publishing either its bytes or its lookups.
    if key == CONTENT_FILE {
        let content = ContentPack::parse(data).map_err(|error| error.to_string())?;
        let changed = FILES.with(|files| files.borrow().read(name).map(Vec::as_slice) != Some(data));
        let content = std::sync::Arc::new(content);
        wsr_core::content::install(content.clone());
        CONTENT.with(|current| *current.borrow_mut() = Some(content));
        if changed { crate::REMARKS.with(|remarks| *remarks.borrow_mut() = Default::default()); }
    }
    // Imported files are already persisted by the host; only subsequent writes are changes.
    FILES.with(|f| { f.borrow_mut().files.insert(key.clone(), data.to_vec()); });
    match key.as_str() {
        // Keep this game's names in its next save when the future-game template changes.
        "CORPNAME.DAT" => GAME.with(|g| { if let Some(g) = g.borrow_mut().as_mut() { g.name_list_dirty = 1; } }),
        "REGINFO.DAT" => GAME.with(|g| { if let Some(g) = g.borrow_mut().as_mut() { import_registration(g, data); } }),
        "MSCNEWS.DAT" => logic::economy::set_mscnews_dat(Some(data.to_vec())),
        "SCENWIN1.WSR" | "SCENWIN2.WSR" | "SCENWIN3.WSR" | "SCENWIN4.WSR" => {
            let n = cast::<i32, _>(key.as_bytes()[7] - b'0');
            logic::turn::set_scenwin_file(n, Some(data.to_vec()));
        }
        _ => {}
    }
    Ok(())
}

fn missing_content() -> String {
    "{\"ok\":false,\"error\":\"Reopen the original installer to prepare runtime game content. Your saves are preserved.\"}".into()
}

fn environment(now: &LocalTime) -> Env<'_> {
    let mut files = FILES.with(|f| f.borrow().clone());
    files.written.clear();
    let mut env = Env::new(now).with_files(files);
    env.repair_initial_shares = true;
    env
}

fn finish(env: &Env) -> String {
    // FileUi has already merged simulation writes with shell writes in execution order.
    FILES.with(|f| {
        let mut files = f.borrow_mut();
        for (name, bytes) in &env.files.borrow().written {
            match bytes {
                Some(bytes) => files.write(name.as_bytes(), bytes.clone()),
                None => files.delete(name.as_bytes()),
            }
        }
    });
    let requests: Vec<_> = env.requests.borrow().iter().map(|r| match r {
        HostRequest::OpenUrl(url) => format!("{{\"kind\":\"url\",\"value\":{}}}", summary::json_str(&url.to_display())),
        HostRequest::RunProgram(cmd) => format!("{{\"kind\":\"program\",\"value\":{}}}", summary::json_str(&cmd.to_display())),
        HostRequest::Exit => "{\"kind\":\"exit\"}".into(),
    }).collect();
    session_json(env.now, &requests)
}

pub fn session_json(now: &LocalTime, requests: &[String]) -> String {
    let countries = CONTENT.with(|content| content.borrow().as_ref().map(|content|
        logic::newgame_helpers::COUNTRY_DATA(content).iter()
            .map(|line| summary::json_str(line[..15].trim())).collect::<Vec<_>>().join(",")
    ).unwrap_or_default());
    let files = FILES.with(|f| f.borrow().files.iter().map(|(name, bytes)|
        format!("{{\"name\":{},\"size\":{}}}", summary::json_str(name), bytes.len())
    ).collect::<Vec<_>>().join(","));
    GAME.with(|cell| {
        let game = cell.borrow();
        let Some(g) = game.as_ref() else {
            return format!("{{\"ok\":true,\"summary\":null,\"countries\":[{countries}],\"files\":[{files}],\"requests\":[]}}");
        };
        let env = environment(now);
        let menus = desk::desk_menu_bar(g, &env).iter().map(|m|
            format!("{{\"group\":{},\"id\":{},\"label\":{},\"disabled\":{}}}",
                summary::json_str(&m.popup), m.id, summary::json_str(&m.text.to_display()),
                m.grayed && !(m.id == desk::cmd::CUSTOMIZER && env.files.borrow().read(b"CORPNAME.DAT").is_some()))
        ).collect::<Vec<_>>().join(",");
        let controls = desk::desk_controls(g, now).iter().filter(|c| c.callback.is_some()).map(|c|
            format!("{{\"id\":{},\"label\":{}}}", c.id, summary::json_str(&c.text.to_display()))
        ).collect::<Vec<_>>().join(",");
        format!("{{\"ok\":true,\"summary\":{},\"countries\":[{countries}],\"menus\":[{menus}],\"controls\":[{controls}],\"files\":[{files}],\"requests\":[{}]}}",
            summary::summary_json(g), requests.join(","))
    })
}

/// 0 state, 1 new game, 2 desk click, 3 menu command, 4 symbol, 5 ticker step,
/// 6 select a company, 7 select an industry, 8 quote of the day.
/// All UI runs synchronously on the calling worker.
pub fn command_json(ui: &mut dyn Ui, command: u32, arg: i32, text: &[u8]) -> String {
    if command == 1 && CONTENT.with(|content| content.borrow().is_none()) {
        return missing_content();
    }
    let now = ui.local_time();
    if command == 0 { return session_json(&now, &[]); }
    let env = environment(&now);
    let file_checkpoint = (command == 1).then(|| FILES.with(|files| files.borrow().clone()));
    let mut file_ui = FileUi { inner: ui, files: &env.files, error: &env.error };
    let ui: &mut dyn Ui = &mut file_ui;
    let error = GAME.with(|cell| RNG.with(|rng| {
        let mut game = cell.borrow_mut();
        let mut rng = rng.borrow_mut();
        if command == 8 {
            let mut empty = Globals::default();
            logic::shell::quotes::show_quote_of_day(game.as_mut().unwrap_or(&mut empty), ui, &env);
            return None;
        }
        if command == 1 {
            let mut new_rng = *rng;
            let mut g = fresh_game(&now, &mut new_rng);
            let files = env.files.borrow().clone();
            logic::newgame_helpers::load_config_and_check_files(&mut g, ui, &|n| files.read(n.as_bytes()).cloned());
            if g.game_end_flag == "Y" { return Some("Import the original game data files before starting a new game."); }
            let Some(bytes) = files.read(b"CORPNAME.DAT") else {
                return Some("Import CORPNAME.DAT before starting a new game. It supplies the company countries used to build the funds.");
            };
            if wsr_core::save::load_corp_names(&mut g, bytes).is_err() {
                return Some("Unable to read CORPNAME.DAT. Import the original company-name file.");
            }
            logic::shell::settings::menu_new_game(&mut g, ui, &mut new_rng, &env);
            if g.game_in_progress == 0 { return Some("New game cancelled."); }
            if let Err(error) = wsr_core::validation::validate_game(&g) {
                ui.game_error(&PbStr::from(format!("Unable to create the new game: {error}").as_str()), &g.app_title);
                return Some("The new game contains invalid financial state.");
            }
            *game = Some(g);
            *rng = new_rng;
            return None;
        }
        let Some(g) = game.as_mut() else { return Some("No game loaded."); };
        let control = match command {
            2 => Some(arg),
            4 => Some(desk::ctl::GO),
            6 => Some(desk::ctl::SELECT_CORP),
            7 => Some(desk::ctl::INDUSTRY_COMBO),
            _ => None,
        };
        if control.is_some_and(|id| !desk::control_enabled(g, id)) {
            return Some("That action is unavailable in the current game state.");
        }
        if command == 5 && (g.game_in_progress == 0 || g.game_end_flag == "Y") {
            return Some("The game clock is stopped because the game is over.");
        }
        if command == 3 && desk::desk_menu_bar(g, &env).iter().any(|m| m.id == arg && m.grayed) {
            return Some("That menu action is unavailable in the current game state.");
        }
        match command {
            2 => desk::desk_click(g, ui, &mut rng, &env, arg, 0),
            3 if arg == desk::cmd::NEW_GAME => return Some("Use the New Game action to restart the session."),
            3 => desk::desk_command(g, ui, &mut rng, &env, arg),
            4 => desk::desk_go(g, ui, &mut rng, &env, &PbStr(text.to_vec())),
            5 => logic::shell::app::ticker_step(g, ui, &mut rng, &env),
            6 if (11..=1600).contains(&arg) => {
                g.target_corp = arg;
                logic::market::select_target_corp(g, ui, &mut rng, now.second);
            }
            7 if (0..=71).contains(&arg) => {
                let caption = PbStr::from(format!("{arg:02}").as_str());
                desk::desk_industry_selection(g, &caption, 9);
                desk::desk_industry_selection(g, &caption, 10);
            }
            _ => return Some("Unknown action."),
        }
        None
    }));
    if error.is_some() || env.error.borrow().is_some() {
        if let Some(files) = file_checkpoint { FILES.with(|current| *current.borrow_mut() = files); }
        let message = env.error.borrow().clone().unwrap_or_else(|| error.unwrap().into());
        return format!("{{\"ok\":false,\"error\":{}}}", summary::json_str(&message));
    }
    finish(&env)
}

/// Load with the original shell's complete post-load lifecycle and current virtual files.
pub fn open_game_json(ui: &mut dyn Ui, bytes: &[u8], fixed: bool, game_number: i32) -> String {
    if CONTENT.with(|content| content.borrow().is_none()) { return missing_content(); }
    let now = ui.local_time();
    let env = environment(&now);
    let file_checkpoint = FILES.with(|files| files.borrow().clone());
    let mut file_ui = FileUi { inner: ui, files: &env.files, error: &env.error };
    let ui: &mut dyn Ui = &mut file_ui;
    let outcome = RNG.with(|rng| {
        let mut live_rng = rng.borrow_mut();
        let mut rng = *live_rng;
        let mut g = fresh_game(&now, &mut rng);
        // GameNumber belongs to MenuOpenSavedGame's file selection, not the serialized state.
        g.game_number = game_number.clamp(1, 50);
        // WinMain loads names/countries before MenuOpenSavedGame. Older saves can omit them, and
        // the ETF migration needs those countries before the post-load lifecycle runs.
        let has_names = if let Some(names) = env.files.borrow().read(b"CORPNAME.DAT") {
            if let Err(e) = wsr_core::save::load_corp_names(&mut g, names) { return Err(format!("CORPNAME.DAT: {e:?}")); }
            true
        } else { false };
        match wsr_core::save::read_save_file(&mut g, ui, &mut rng, bytes, fixed) {
            Ok(info) => {
                if info.needs_corp_names && !has_names { return Err("This save requires CORPNAME.DAT. Import it and open the save again.".into()); }
                logic::shell::app::repair_legacy_dividends(&mut g, ui);
                wsr_core::validation::validate_game(&g).map_err(|error| format!("Unable to load: {error}"))?;
                if !logic::shell::app::finish_loaded_game(&mut g, ui, &mut rng, &env) {
                    return Err(env.error.borrow().clone().unwrap_or_else(|| "ETF migration could not complete".into()));
                }
                wsr_core::validation::validate_game(&g).map_err(|error| format!("Unable to load: {error}"))?;
                if let Some(error) = env.error.borrow().clone() { return Err(error); }
                GAME.with(|cell| *cell.borrow_mut() = Some(g));
                *live_rng = rng;
                Ok(())
            }
            Err(e) => Err(format!("{e:?}")),
        }
    });
    match outcome {
        Ok(()) => finish(&env),
        Err(e) => {
            FILES.with(|files| *files.borrow_mut() = file_checkpoint);
            format!("{{\"ok\":false,\"error\":{}}}", summary::json_str(&e))
        }
    }
}

#[cfg(test)]
mod tests {
use wsr_core::pb::real::{Ext, cast};

    use super::*;

    struct QuoteUi { now: LocalTime, shown: Vec<PbStr> }
    impl Ui for QuoteUi {
        fn local_time(&self) -> LocalTime { self.now }
        fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 { 1 }
        fn inputbox(&mut self, _: &PbStr, _: &PbStr, default: &PbStr) -> PbStr { default.clone() }
        fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction { ReportAction::Ok }
        fn news(&mut self, _: &PbStr) {}
        fn choose(&mut self, _: &PbStr, prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
            assert_eq!(options, &[PbStr::from("OK")]);
            self.shown.extend_from_slice(prompt);
            Some(0)
        }
    }

    #[test]
    fn startup_quotes_follow_the_host_calendar_across_restarts() {
        crate::install_test_content();
        let content = wsr_core::content::current();
        let entries = TextId::identifiers().zip(wsr_core::content::REQUIRED_IDS).map(|(id, &number)| {
            (id, if number == 10180 { PbStr::from("@IT") } else { content.text(id).clone() })
        });
        let content = ContentPack::from_entries(entries).unwrap().to_bytes();
        let mut quotes = Vec::new();
        for day in [1, 3, 4, 366] {
            quotes.extend_from_slice(format!("@IT{day:03}\r\n").as_bytes());
            quotes.extend(wsr_core::pb::codec::encode_line(format!("Test quote {day}.").as_bytes()));
            quotes.extend_from_slice(b"\r\n");
        }
        quotes.extend_from_slice(b"@END\r\n");
        for (year, month, day, expected) in [
            (2000, 1, 3, Some(3)), (2000, 1, 3, Some(3)), (2000, 1, 4, Some(4)),
            (2000, 12, 31, Some(366)), (2001, 1, 1, Some(1)), (2001, 1, 2, None),
        ] {
            FILES.with(|files| *files.borrow_mut() = Default::default());
            GAME.with(|game| *game.borrow_mut() = None);
            RNG.with(|rng| *rng.borrow_mut() = Rng::from_state(123456));
            put_file(CONTENT_FILE.as_bytes(), &content).unwrap();
            put_file(b"QUOTEWSR.DAT", &quotes).unwrap();
            let before = FILES.with(|files| files.borrow().files.clone());
            let mut ui = QuoteUi { now: LocalTime { year, month, day, ..LocalTime::HARNESS }, shown: vec![] };
            let result = command_json(&mut ui, 8, 0, b"");
            assert!(result.contains("\"ok\":true"), "{result}");
            let expected: Vec<_> = expected.into_iter().map(|day|
                PbStr::from(format!("Test quote {day}. ").as_str())
            ).collect();
            assert_eq!(ui.shown, expected);
            assert!(GAME.with(|game| game.borrow().is_none()));
            assert_eq!(RNG.with(|rng| rng.borrow().state()), 123456);
            FILES.with(|files| {
                assert_eq!(files.borrow().files, before);
                assert!(files.borrow().written.is_empty());
            });
        }
    }

    fn load_fixture() -> Globals {
        let mut g = Globals::default();
        g.version_str = "9.85".into(); g.game_number = 1; g.name_list_dirty = 1;
        g.num_players = 2; g.cur_player = 1; g.active_entity = 1; g.sel_entity = 1;
        g.player_turn_order[1] = 1; g.player_turn_order[2] = 2;
        g.entity_status[1] = 1; g.entity_status[2] = 1;
        g.start_year = 2024; g.cur_year = 2025; g.game_end_year = 2030; g.cur_quarter = 1;
        g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0)); g.sim_co_count = 100;
        g
    }

    #[test]
    fn invalid_finances_and_failed_etf_migration_leave_the_live_session_and_files_intact() {
        crate::install_test_content();
        for migration in [false, true] {
            let mut incoming = load_fixture();
            if !migration { incoming.exchange_rate = f64::INFINITY; incoming.entity_status[1581] = 2; }
            let bytes = wsr_core::save::write_save_file(&mut incoming, &mut wsr_core::ui::Scripted::default(), &mut Rng::default());
            let mut live = load_fixture(); live.cash[1] = Ext::from_f64(500.0); live.entity_status[1581] = 2;
            let before = live.to_json();
            GAME.with(|g| *g.borrow_mut() = Some(live));
            RNG.with(|r| *r.borrow_mut() = Rng::from_state(123456));
            FILES.with(|f| {
                *f.borrow_mut() = logic::shell::Files::default();
                f.borrow_mut().write(b"GAME01.DAT", b"last good save".to_vec());
                f.borrow_mut().write(b"LASTGAME.DAT", b"1\r\n".to_vec());
            });
            let before_files = FILES.with(|f| f.borrow().clone());
            let result = open_game_json(&mut wsr_core::ui::Scripted::default(), &bytes, false, 7);
            assert!(result.contains("\"ok\":false"), "{result}");
            assert!(result.contains(if migration { "ETF" } else { "ExchangeRate" }), "{result}");
            assert_eq!(GAME.with(|g| g.borrow().as_ref().unwrap().to_json()), before);
            assert_eq!(RNG.with(|r| r.borrow().state()), 123456);
            FILES.with(|f| {
                assert_eq!(f.borrow().files, before_files.files);
                assert_eq!(f.borrow().written, before_files.written);
            });
        }
    }

    #[test]
    fn disabled_commands_cannot_change_the_game_or_rng() {
        crate::install_test_content();
        let mut g = Globals::default();
        g.game_in_progress = 1; g.game_end_flag = PbStr::from("Y");
        GAME.with(|cell| *cell.borrow_mut() = Some(g));
        RNG.with(|r| *r.borrow_mut() = Rng::from_state(123));
        let mut ui = wsr_core::ui::Scripted::default();
        let before = command_json(&mut ui, 0, 0, b"");
        for (command, arg) in [(2, 105), (2, 244), (2, 249), (3, 615), (5, 0)] {
            assert!(command_json(&mut ui, command, arg, b"").contains("\"ok\":false"));
            assert_eq!(command_json(&mut ui, 0, 0, b""), before);
            assert_eq!(RNG.with(|r| r.borrow().state()), 123);
        }
        GAME.with(|cell| {
            let mut game = cell.borrow_mut(); let g = game.as_mut().unwrap();
            g.game_end_flag = PbStr::from("N"); g.solo_human_mode = 1;
        });
        assert!(command_json(&mut ui, 2, 244, b"").contains("\"ok\":false"));
        GAME.with(|cell| {
            let mut game = cell.borrow_mut(); let g = game.as_mut().unwrap();
            g.solo_human_mode = 0; g.action_code = PbStr::from("AUTO");
        });
        for (command, arg) in [(2, 105), (4, 0), (6, 131), (7, 1)] {
            assert!(command_json(&mut ui, command, arg, b"IBM").contains("\"ok\":false"));
        }
        assert!(ui.log.is_empty());
    }

    #[test]
    fn company_template_edits_preserve_current_names_in_future_saves() {
        crate::install_test_content();
        let mut g = Globals::default();
        g.ent_name[11] = PbStr::from("Current Bank");
        g.name_list_dirty = 0;
        GAME.with(|cell| *cell.borrow_mut() = Some(g));
        put_file(b"CORPNAME.DAT", b"0011|00|NEW|Future Bank|\r\n@END|\r\n").unwrap();
        GAME.with(|cell| {
            let game = cell.borrow();
            let g = game.as_ref().unwrap();
            assert_eq!(g.ent_name[11], PbStr::from("Current Bank"));
            assert_eq!(g.name_list_dirty, 1);
        });
    }

    #[test]
    fn autosave_after_a_shell_save_is_the_persisted_version() {
        crate::install_test_content();
        let env = environment(&LocalTime::HARNESS);
        let mut host = wsr_core::ui::Scripted::default();
        let mut ui = FileUi { inner: &mut host, files: &env.files, error: &env.error };
        env.files.borrow_mut().write(b"GAME07.DAT", b"manual save".to_vec());
        ui.write_file(&PbStr::from("GAME07.DAT"), b"later autosave");
        finish(&env);
        assert_eq!(FILES.with(|f| f.borrow().read(b"GAME07.DAT").cloned()).unwrap(), b"later autosave");
        // A subsequent shell write must also take precedence over that autosave.
        env.files.borrow_mut().write(b"GAME07.DAT", b"last shell save".to_vec());
        finish(&env);
        assert_eq!(FILES.with(|f| f.borrow().read(b"GAME07.DAT").cloned()).unwrap(), b"last shell save");
    }

    #[test]
    fn rejected_game_replacements_preserve_the_current_game_and_rng() {
        crate::install_test_content();
        GAME.with(|g| *g.borrow_mut() = Some(Globals::default()));
        RNG.with(|r| *r.borrow_mut() = Rng::from_state(123456));
        let mut ui = wsr_core::ui::Scripted::default();
        let before = command_json(&mut ui, 0, 0, b"");
        assert!(open_game_json(&mut ui, b"invalid save", false, 7).contains("\"ok\":false"));
        assert_eq!(RNG.with(|r| r.borrow().state()), 123456);
        assert_eq!(command_json(&mut ui, 0, 0, b""), before);
        // A new game without the required data must leave the ongoing session intact too.
        assert!(command_json(&mut ui, 1, 0, b"").contains("\"ok\":false"));
        assert_eq!(RNG.with(|r| r.borrow().state()), 123456);
        assert_eq!(command_json(&mut ui, 0, 0, b""), before);
    }
}

/// # Safety
/// Both input slices must refer to readable wasm memory for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsr_put_file(name: *const u8, name_len: usize, data: *const u8, data_len: usize) -> *mut u8 {
    let out = match put_file(unsafe { std::slice::from_raw_parts(name, name_len) }, unsafe { std::slice::from_raw_parts(data, data_len) }) {
        Ok(()) => "{\"ok\":true}".to_string(),
        Err(error) => format!("{{\"ok\":false,\"error\":{}}}", summary::json_str(&error)),
    };
    result(out.as_bytes())
}

/// # Safety
/// The name slice must refer to readable wasm memory for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wsr_get_file(name: *const u8, len: usize) -> *mut u8 {
    let name = unsafe { std::slice::from_raw_parts(name, len) };
    FILES.with(|f| result(f.borrow().read(name).map(Vec::as_slice).unwrap_or_default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn wsr_file_changes() -> *mut u8 {
    let changes = FILES.with(|f| std::mem::take(&mut f.borrow_mut().written));
    let items = changes.iter().map(|(name, bytes)| format!("{{\"name\":{},\"deleted\":{}}}",
        summary::json_str(name), bytes.is_none())).collect::<Vec<_>>().join(",");
    result(format!("[{items}]").as_bytes())
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
/// # Safety
/// `text`/`len` must refer to readable wasm memory for the duration of the call.
pub unsafe extern "C" fn wsr_command(command: u32, arg: i32, text: *const u8, len: usize) -> *mut u8 {
    let text = unsafe { std::slice::from_raw_parts(text, len) };
    result(command_json(&mut crate::host::HostUi, command, arg, text).as_bytes())
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
/// # Safety
/// `data`/`len` must refer to readable wasm memory for the duration of the call.
pub unsafe extern "C" fn wsr_open_game(data: *const u8, len: usize, fixed: u32, game_number: i32) -> *mut u8 {
    let data = unsafe { std::slice::from_raw_parts(data, len) };
    result(open_game_json(&mut crate::host::HostUi, data, fixed != 0, game_number).as_bytes())
}
