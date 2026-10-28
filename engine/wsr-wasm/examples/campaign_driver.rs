//! Persistent native peer for tools/emu/campaign.py. Deliberately has no extra dependencies.
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};
use wsr_core::{
    logic::{
        self,
        newgame::LocalTime,
        shell::{self, Env, Files},
    },
    pb::{pbstr::PbStr, rng::Rng},
    save,
    snapshot::{Reader, SnapVal},
    state::Globals,
    ui::{ReportAction, Ui},
};
fn js(s: &str) -> String {
    wsr_wasm::summary::json_str(s)
}
fn pb(s: &PbStr) -> String {
    let mut out = String::new();
    s.write(&mut out);
    out
}
fn array(xs: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", xs.into_iter().collect::<Vec<_>>().join(","))
}
fn value<T: SnapVal + Default>(r: &Reader, k: &str) -> Result<T, String> {
    let mut v = T::default();
    r.val(k, &mut v)?;
    Ok(v)
}
fn text(r: &Reader, k: &str) -> Result<String, String> {
    Ok(value::<PbStr>(r, k)?.to_display())
}
fn assets(dir: &Path) -> Result<Files, String> {
    let __content = wsr_core::content::current();
    let mut f = Files::default();
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let p = e.map_err(|e| e.to_string())?.path();
        let Some(n) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if p.is_file()
            && (n.to_ascii_uppercase().ends_with(".DAT")
                || n.to_ascii_uppercase().ends_with(".WSR"))
        {
            f.files.insert(
                Files::key(n.as_bytes()),
                std::fs::read(&p).map_err(|e| e.to_string())?,
            );
        }
    }
    if let Some(b) = f.read(b"MSCNEWS.DAT") {
        logic::economy::set_mscnews_dat(Some(b.clone()));
    }
    for n in 1..=4 {
        if let Some(b) = f.read(format!("{__wsr_text_14367}{n}{__wsr_text_14368}",__wsr_text_14367=__content.string(14367), __wsr_text_14368=__content.string(14368)).as_bytes()) {
            logic::turn::set_scenwin_file(n, Some(b.clone()));
        }
    }
    Ok(f)
}
struct Host<'a> {
    now: LocalTime,
    files: &'a RefCell<Files>,
    q: VecDeque<String>,
    replies: Vec<String>,
    events: Vec<String>,
}
impl Host<'_> {
    fn answer(&mut self) -> Option<String> {
        self.q
            .front()
            .filter(|s| !s.starts_with("pick:") && !s.starts_with("sym:"))
            .cloned()
            .map(|_| self.q.pop_front().unwrap())
    }
}
impl Ui for Host<'_> {
    fn local_time(&self) -> LocalTime {
        self.now
    }
    fn write_file(&mut self, n: &PbStr, b: &[u8]) {
        self.files.borrow_mut().write(n.as_bytes(), b.to_vec());
    }
    fn msgbox(&mut self, t: &PbStr, style: i32, title: &PbStr) -> i32 {
        if style == 0 && title.is_empty() {
            self.events
                .push(format!("{{\"kind\":\"notice\",\"text\":{}}}", pb(t)));
            return 1;
        }
        let reply = self
            .answer()
            .map(|s| match s.as_str() {
                "IDOK" => 1,
                "IDCANCEL" => 2,
                "IDABORT" => 3,
                "IDRETRY" => 4,
                "IDIGNORE" => 5,
                "IDYES" => 6,
                "IDNO" => 7,
                _ => s.parse().unwrap_or(0),
            })
            .unwrap_or(if matches!(style & 15, 3 | 4) { 6 } else { 1 });
        self.replies.push(reply.to_string());
        self.events.push(format!(
            "{{\"kind\":\"msgbox\",\"title\":{},\"text\":{},\"style\":{style},\"reply\":{reply}}}",
            pb(title),
            pb(t)
        ));
        reply
    }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        let sym = self.q.front().is_some_and(|s| s.starts_with("sym:"));
        let symbol_dialog = sym || prompt.to_display().ends_with(__content.string(6717));
        let answer = if sym {
            self.q.pop_front().unwrap()[4..].to_owned()
        } else if symbol_dialog {
            default.to_display()
        } else {
            self.answer().unwrap_or_default()
        };
        if symbol_dialog {
            if answer.is_empty() {
                self.replies.push("click:2".into())
            } else {
                self.replies
                    .extend([format!("text:310:{answer}"), "click:1".into()]);
            }
        } else {
            self.replies.push(answer.clone());
            self.events.push(format!(
                "{{\"kind\":\"inputbox\",\"reply\":{}}}",
                pb(&PbStr::from(answer.as_str()))
            ));
        }
        let _ = title;
        PbStr::from(answer.as_str())
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], yes_no: bool) -> ReportAction {
        self.events.push(format!(
            "{{\"kind\":\"report\",\"title\":{},\"lines\":{}}}",
            pb(title),
            array(lines.iter().map(pb))
        ));
        if self.q.front().is_some_and(|s| s.starts_with("pick:")) {
            let tok = self.q.pop_front().unwrap();
            let k: usize = tok[5..].parse().unwrap_or(0);
            let links: Vec<_> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.to_display().contains("@C") || l.to_display().contains("@I"))
                .collect();
            if !links.is_empty() {
                let (idx, line) = links[k % links.len()];
                self.replies
                    .extend([format!("select:301:{idx}"), "click:1".into()]);
                let s = line.to_display();
                return ReportAction::Link(PbStr::from(&s[s.rfind('@').unwrap()..]));
            }
        }
        if yes_no && self.q.front().is_some_and(|s| s == "IDNO") {
            self.q.pop_front();
            self.replies.push("click:2".into());
            return ReportAction::No;
        }
        self.replies.push("click:1".into());
        ReportAction::Ok
    }
    fn choose(&mut self, title: &PbStr, _: &[PbStr], _: &[PbStr]) -> Option<usize> {
        // Automated campaigns cancel optional custom dialogs. Explicit choices need an adapter,
        // rather than guessing a Windows control id from a browser option index.
        self.events
            .push(format!("{{\"kind\":\"choice\",\"title\":{}}}", pb(title)));
        self.replies.push("close".into());
        None
    }
    fn chart(&mut self, _: &logic::info_screens::chart::ChartSpec) -> bool {
        self.replies.push("click:1".into());
        false
    }
    fn news(&mut self, _: &PbStr) {}
}
fn snapshot(g: &Globals, out: &Path) -> Result<(), String> {
    std::fs::write(out.join("current.json"), g.to_json()).map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let __content = wsr_core::content::current();
    wsr_wasm::session::put_file(wsr_core::content::CONTENT_FILE.as_bytes(), &__content.to_bytes())?;
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() != 4 {
        return Err("campaign_driver prepare|serve SAVE_OR_STATE ASSETS OUTPUT".into());
    }
    let input = PathBuf::from(&a[1]);
    let output = PathBuf::from(&a[3]);
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let mut files = assets(Path::new(&a[2]))?;
    let mut rng = Rng::from_state(1);
    let mut g = wsr_wasm::session::fresh_game(&LocalTime::HARNESS, &mut rng);
    if a[0] == "prepare" {
        if let Some(names) = files.read(b"CORPNAME.DAT") {
            save::load_corp_names(&mut g, names).map_err(|e| format!("{__wsr_text_14369}{e:?}",__wsr_text_14369=__content.string(14369)))?;
        }
        let mut ui = wsr_core::ui::Scripted::default();
        let info = save::read_save_file(
            &mut g,
            &mut ui,
            &mut rng,
            &std::fs::read(input).map_err(|e| e.to_string())?,
            false,
        )
        .map_err(|e| format!("save: {e:?}"))?;
        if info.needs_corp_names && files.read(b"CORPNAME.DAT").is_none() {
            return Err("Save requires CORPNAME.DAT".into());
        }
        if !(1..=g.num_players).contains(&g.cur_player) {
            return Err("Save has no current player".into());
        }
        // Common host starting conditions, applied once before either trajectory begins.
        g.game_in_progress = 1;
        g.clock_state = "RUN".into();
        g.ticker_on = 1;
        g.ticker_stop_req = "N".into();
        snapshot(&g, &output)?;
        return Ok(());
    }
    if a[0] != "serve" {
        return Err("Unknown mode".into());
    }
    g.from_json(&std::fs::read_to_string(input).map_err(|e| e.to_string())?)?;
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let r = Reader::parse(&line)?;
        let op = text(&r, "op")?;
        let arg = value::<i32>(&r, "arg")?;
        let now = LocalTime {
            year: value(&r, "year")?,
            month: value(&r, "month")?,
            day: value(&r, "day")?,
            hour: value(&r, "hour")?,
            minute: value(&r, "minute")?,
            second: value(&r, "second")?,
            millis: value(&r, "millis")?,
        };
        if op == "init" {
            rng = Rng::from_state(value::<i64>(&r, "rng")? as u32);
        }
        let env = Env::new(&now).with_files(files);
        env.files.borrow_mut().written.clear();
        let count = value::<i32>(&r, "answer_count")?;
        let q = (0..count)
            .map(|i| text(&r, &format!("{__wsr_text_14370}{i}",__wsr_text_14370=__content.string(14370))))
            .collect::<Result<VecDeque<_>, _>>()?;
        let mut ui = Host {
            now,
            files: &env.files,
            q,
            replies: vec![],
            events: vec![],
        };
        match op.as_str() {
            "init" | "state" => {}
            "tick" => logic::turn::ticker_sim_steps(&mut g, &mut ui, &mut rng, 1, &now),
            "entity" => logic::sim_entity::sim_process_next_entity(&mut g, &mut ui, &mut rng, &now),
            "end_turn" => logic::turn::end_turn(&mut g, &mut ui, &mut rng, &now),
            "select" => {
                g.target_corp = arg;
                logic::market::select_target_corp(&mut g, &mut ui, &mut rng, now.second);
            }
            "trade" => {
                g.action_menu_mode = value(&r, "mode")?;
                shell::menu::do_transaction_menu_action(&mut g, &mut ui, &mut rng, &env, arg);
            }
            "pause" => logic::turn::pause_ticker(&mut g, &now, arg as f32),
            "notice" => {
                logic::report::show_quick_message(&mut g, &mut ui, &mut value::<PbStr>(&r, "text")?)
            }
            "interest_rates" => logic::report::show_interest_rates(&mut g, &mut ui, &mut rng),
            "save" => {
                shell::app::save_game(&mut g, &mut ui, &mut rng, &env);
            }
            "reload" => shell::app::menu_open_saved_game(&mut g, &mut ui, &mut rng, &env),
            "clock" => {
                g.clock_state = if arg == 1 { "RUN" } else { "STOP" }.into();
                g.ticker_on = arg;
                g.ticker_stop_req = "N".into();
            }
            _ => return Err(format!("Unknown operation: {op}")),
        }
        snapshot(&g, &output)?;
        let mut changes = vec![];
        for (name, bytes) in &env.files.borrow().written {
            if let Some(bytes) = bytes {
                std::fs::write(output.join(name), bytes).map_err(|e| e.to_string())?;
            }
            changes.push(format!(
                "{{\"name\":{},\"deleted\":{}}}",
                js(name),
                bytes.is_none()
            ));
        }
        writeln!(stdout,"{{\"rng\":{},\"answers\":{},\"unused\":{},\"events\":{},\"files\":{},\"year\":{},\"quarter\":{}{__wsr_text_14371}{},\"game_over\":{}}}",rng.state(),array(ui.replies.iter().map(|s|js(s))),array(ui.q.iter().map(|s|js(s))),array(ui.events.clone()),array(changes),g.cur_year,g.cur_quarter,g.sim_co_count,g.game_end_flag=="Y",__wsr_text_14371=__content.string(14371)).map_err(|e|e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
        drop(ui);
        files = env.files.into_inner();
    }
    Ok(())
}
