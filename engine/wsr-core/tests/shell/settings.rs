//! Tests of logic::shell::settings: differential tests of the File / Game Options / Settings menu
//! entries against the original.
use wsr_core::pb::real::cast;


#[allow(unused_imports)]
use super::common::*;
use super::diff::*;
use super::support::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::settings::*;
use wsr_core::logic::shell::{Env, Files, HostRequest};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const NOW: LocalTime = LocalTime::HARNESS;

/// Scripted UI: one answer queue like the harness's. `IDYES`/`IDNO`/`IDOK`/`IDCANCEL` answer message
/// boxes, `choose:k` / `choose:cancel` answer `choose` dialogs (no counterpart in the original: DLL /
/// external program dialogs), anything else is INPUTBOX text. Quick messages (style 0, no title) take
/// nothing.
#[derive(Default)]
struct SUi {
    q: VecDeque<String>,
    msgs: Vec<String>,
    titles: Vec<String>,
    inputs: usize,
}

fn is_box(a: &str) -> bool {
    a.starts_with("ID")
}

impl Ui for SUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        self.titles.push(title.to_display());
        if style == 0 && title.is_empty() {
            return 1;
        }
        let yes_no = matches!(style & 0xf, 3 | 4);
        match self.q.front() {
            Some(a) if is_box(a) => match self.q.pop_front().unwrap().as_str() {
                "IDOK" => 1,
                "IDCANCEL" => 2,
                "IDYES" => 6,
                "IDNO" => 7,
                _ => 0,
            },
            _ => {
                if yes_no {
                    6
                } else {
                    1
                }
            }
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.inputs += 1;
        match self.q.front() {
            Some(a) if !is_box(a) && !a.starts_with("choose:") => PbStr::from(self.q.pop_front().unwrap().as_str()),
            _ => PbStr::new(),
        }
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _headline: &PbStr) {}
    fn choose(&mut self, _title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        match self.q.front() {
            Some(a) if a.starts_with("choose:") => {
                let a = self.q.pop_front().unwrap();
                a[7..].parse().ok().filter(|&k: &usize| k < options.len())
            }
            _ => None,
        }
    }
}

struct Run {
    g: Globals,
    ui: SUi,
    o: Original,
    requests: Vec<HostRequest>,
    files: Files,
}

/// Runs the port (`port`) and the original routine `addr` on `g` with the same answers, files and
/// skips; compares globals (`cmp`), RND state, message box / quick message / inputbox counts, the skip
/// trace, the files written and the URLs / programs launched.
#[allow(clippy::too_many_arguments)]
fn check(
    seed: u64,
    addr: u32,
    g: &Globals,
    answers: &[&str],
    files: &[(&str, &[u8])],
    skips: &[(u32, u32)],
    cmp: Compare,
    port: impl Fn(&mut Globals, &mut dyn Ui, &mut Rng, &Env),
) -> Option<Run> {
    let rng0 = super::script::rng_of(seed);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = SUi { q: answers.iter().map(|s| s.to_string()).collect(), ..SUi::default() };
    let skip_addrs: Vec<u32> = skips.iter().map(|s| s.0).collect();
    let mut f = Files::default();
    for (n, d) in files {
        f.files.insert(Files::key(n.as_bytes()), d.to_vec());
    }
    let env = Env::skipping(&NOW, &skip_addrs).with_files(f);
    port(&mut pg, &mut ui, &mut rng, &env);
    let oa: Vec<&str> = answers.iter().copied().filter(|a| !a.starts_with("choose:")).collect();
    let opts = Opts {
        rng: Some(rng0),
        answers: &oa,
        skips: skips.to_vec(),
        files: files.iter().map(|(n, d)| (n.to_string(), d.to_vec())).collect(),
        no_default_files: true,
        ..Opts::default()
    };
    let o = run_original_with(addr, &opts, g)?;
    if std::env::var_os("WSR_LOG").is_some() {
        for l in o.log.iter().filter(|l| !l.starts_with("API") || l.contains("Shell") || l.contains("Process")) {
            eprintln!("seed {seed} orig: {l}");
        }
        eprintln!("seed {seed} port msgs: {:?}", ui.titles);
    }
    let d = cmp.diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "msgs seed {seed}: {:?}\n{:?}", ui.titles, o.log);
    let orig_inputs = o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count();
    assert_eq!(ui.inputs, orig_inputs, "inputs seed {seed}");
    assert_same_skips(seed, &env.skipped(), &o);
    // files
    let files = env.files.borrow().clone();
    let mut ow: Vec<(String, Option<Vec<u8>>)> =
        o.files.iter().filter(|(k, _)| k.as_str() != "TMP_QM.DAT").map(|(k, v)| (k.clone(), v.clone())).collect();
    ow.sort();
    let pw: Vec<(String, Option<Vec<u8>>)> = files.written.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert_eq!(pw, ow, "files written, seed {seed}");
    // URLs and programs
    let requests = env.requests.borrow().clone();
    let urls: Vec<String> = requests
        .iter()
        .filter_map(|r| match r {
            HostRequest::OpenUrl(u) => Some(u.to_display()),
            _ => None,
        })
        .collect();
    let ourls: Vec<String> = o
        .log
        .iter()
        .filter(|l| l.starts_with("API ShellExecuteA("))
        .map(|l| l.split('\'').nth(3).unwrap_or("").replace("\\\\", "\\"))
        .collect();
    assert_eq!(urls, ourls, "urls seed {seed}");
    let progs: Vec<String> = requests
        .iter()
        .filter_map(|r| match r {
            HostRequest::RunProgram(u) => Some(u.to_display()),
            _ => None,
        })
        .collect();
    let oprogs: Vec<String> = o
        .log
        .iter()
        .filter(|l| l.starts_with("API CreateProcessA(") && !l.contains("qm.exe"))
        .map(|l| l.split('\'').nth(1).unwrap_or("").replace("\\\\", "\\"))
        .collect();
    assert_eq!(progs, oprogs, "programs seed {seed}: {:?}", o.log.iter().filter(|l| l.contains("Process")).collect::<Vec<_>>());
    Some(Run { g: pg, ui, o, requests, files })
}

fn base(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    g.data_path = st("C:\\WSR\\");
    g.app_title = st(__content.string(13661));
    g.version_str = st("9.85");
    g.exe_name = st("WSR.EXE");
    g
}

#[derive(Default)]
struct Hit(Mutex<Vec<String>>);

impl Hit {
    fn add(&self, s: String) {
        self.0.lock().unwrap().push(s);
    }
    fn assert(&self, want: &[&str]) {
        if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
            return;
        }
        let h = self.0.lock().unwrap();
        for w in want {
            assert!(h.iter().any(|x| x.contains(w)), "branch not hit: {w:?}");
        }
    }
}

// --------------------------------------------------------------------------------------- toggles

type Port = fn(&mut Globals, &mut dyn Ui, &mut Rng, &Env);

#[test]
fn diff_simple_toggles() {
    let toggles: [(u32, Port, fn(&mut Globals, i32)); 8] = [
        (0x40d305, menu_toggle_suppress_popups, |g, v| g.suppress_popups = v),
        (0x40d157, menu_toggle_suppress_cash_warn, |g, v| g.cur_suppress_cash_warn = v),
        (0x40d4e0, menu_toggle_suppress_earnings, |g, v| g.cur_suppress_earnings = v),
        (0x40d670, menu_toggle_auto_save, |g, v| g.cur_auto_save = v),
        (0x40d7f1, menu_toggle_exercise_options, |g, v| g.cur_exercise_options = v),
        (0x40dc69, menu_toggle_make_delivery, |g, v| g.cur_make_delivery = v),
        (0x40def1, menu_toggle_take_delivery, |g, v| g.cur_take_delivery = v),
        (0x40e591, menu_toggle_auto_add_stream, |g, v| g.auto_add_stream_list = v),
    ];
    par_seeds(1..=32, |seed| {
        let (addr, port, setv) = toggles[cast::<usize, _>(seed % 8)];
        let mut g = base(seed);
        g.cur_player = [0, 1, 2, 3][cast::<usize, _>((seed / 8) % 4)];
        setv(&mut g, cast::<i32, _>((seed / 2) % 2) + if seed % 11 == 0 { 2 } else { 0 });
        if seed % 5 == 0 {
            g.pref_currency = PbStr::new();
            g.ticker_speed = 0;
        }
        check(seed, addr, &g, &[], &[], &[], cmp(), |g, ui, rng, env| port(g, ui, rng, env));
    });
}

#[test]
fn diff_toggle_chart_size() {
    par_seeds(1..=8, |seed| {
        let mut g = base(seed);
        g.chart_size = [0, 1, 2, 3][cast::<usize, _>(seed % 4)];
        g.cur_player = cast::<i32, _>(seed % 2);
        check(seed, 0x40e21c, &g, &[], &[], &[(0x6141b1, 1)], cmp(), menu_toggle_chart_size);
    });
}

#[test]
fn diff_toggle_cheat_mode() {
    let hit = Hit::default();
    par_seeds(1..=16, |seed| {
        let mut g = base(seed);
        g.cheat_mode = st(["N", "Y", "", "Y"][cast::<usize, _>(seed % 4)]);
        g.scenario_tag = st("TAG");
        g.scenario_num = if seed % 8 == 1 { 3 } else { 0 };
        g.merger_tip_corp = if seed % 8 == 5 { 200 } else { 0 };
        g.insider_earn_tip_corp = if seed % 16 == 9 { 300 } else { 0 };
        g.game_in_progress = if seed % 6 == 3 { 0 } else { 1 };
        g.action_menu_open = 1;
        g.dlg_open_db_search = 1;
        let r = check(seed, 0x40cb57, &g, &[], &[], &[], cmp(), menu_toggle_cheat_mode);
        if let Some(r) = r {
            hit.add(format!("{} {:?}", r.g.cheat_mode, r.ui.msgs));
        }
    });
    hit.assert(&["Y [\"Cheatmode is now ON", "N [\"Cheatmode is now OFF", "pending", "right now -- \\r\\r\""]);
}

#[test]
fn diff_toggle_autopilot_global() {
    let __content = wsr_core::content::current();
    let hit = Hit::default();
    par_seeds(1..=16, |seed| {
        let mut g = base(seed);
        g.cur_player = if seed % 7 == 0 { 0 } else { 1 + cast::<i32, _>(seed % 2) };
        g.game_in_progress = if seed % 9 == 4 { 0 } else { 1 };
        if seed % 10 == 3 {
            g.game_end_flag = st("Y");
        }
        if g.cur_player > 0 {
            let cp = g.cur_player;
            g.auto_pilot[cp] = cast::<i32, _>(seed % 2);
            // give the player some companies and a CEO post
            for c in [140, 200, 333, 1000] {
                g.corp_controller[c] = cp;
                g.entity_status[c] = 1;
            }
            g.player_ceo_of[cp] = if seed % 3 == 0 { 0 } else { 200 };
            if seed == 2 {
                // UpdateCorpControl recomputes control from holdings before adding the CEO notice.
                for holder in 1..=1600 {
                    for slot in 1..=15 {
                        if g.hold_stock_id[(slot, holder)] == 200 { g.hold_stock_id[(slot, holder)] = 0; }
                    }
                }
                g.hold_stock_id[(1, cp)] = 200;
                g.hold_stock_pct[(1, cp)] = 60;
                g.hold_stock_id[(0, cp)] = cast::<i16, _>((1..=15).filter(|&slot| g.hold_stock_id[(slot, cp)] != 0).count());
            }
        }
        let ans: &[&str] = if seed % 4 == 1 { &["IDNO"] } else { &[] };
        if let Some(r) = check(seed, 0x40e734, &g, ans, &[], &[], cmp(), menu_toggle_auto_pilot_global) {
            hit.add(r.ui.msgs.join("|"));
        }
    });
    hit.assert(&[__content.string(14136), __content.string(14137), __content.string(14138), __content.string(14139), __content.string(14140)]);
}

#[test]
fn diff_set_growth_throttle() {
    let cases: [&[&str]; 8] = [
        &["IDNO"],
        &["IDYES", ""],
        &["IDYES", "25", "IDOK", "IDYES"],
        &["IDYES", "-5", "IDOK", "IDNO"],
        &["IDYES", "99", "-10", "IDOK", "IDYES"],
        &["IDYES", "$ 4,0", "IDOK", "IDYES"],
        &["IDYES", "abc", "-11", "0", "IDOK", "IDYES"],
        &["IDYES", "-3", "IDOK", "IDYES"],
    ];
    par_seeds(1..=16, |seed| {
        let mut g = base(seed);
        g.cur_player = 1 + cast::<i32, _>(seed % 3);
        if seed == 9 {
            g.game_end_flag = st("Y");
        }
        let cp = g.cur_player;
        g.player_growth_throttle[cp] = cast::<i16, _>(seed % 40);
        for c in 131..=400 {
            if c % 3 == 0 {
                g.corp_controller[c] = cp;
                g.auto_pilot[c] = cast::<i32, _>(c % 2);
                g.growth_rate[c] = cast::<i16, _>((c * 7) % 70) - 10;
            }
        }
        check(seed, 0x40edbc, &g, cases[cast::<usize, _>(seed % 8)], &[], &[], cmp(), menu_set_growth_throttle);
    });
}

#[test]
fn diff_ticker_speed() {
    let cases: [&[&str]; 6] = [&[""], &["30"], &["150", "0"], &["-4", "x", ""], &["101", "1"], &["1,00"]];
    par_seeds(1..=12, |seed| {
        let mut g = base(seed);
        g.ticker_speed = if seed % 4 == 0 { 0 } else { 70 };
        check(seed, 0x40c88a, &g, cases[cast::<usize, _>(seed % 6)], &[], &[], cmp(), menu_ticker_speed);
    });
}

// ------------------------------------------------------------------------------- currency, law firm

#[test]
fn diff_currency() {
    let __content = wsr_core::content::current();
    par_seeds(1..=16, |seed| {
        let mut g = base(seed);
        g.world_generated = cast::<i32, _>(seed % 2);
        g.game_in_progress = cast::<i32, _>((seed / 2) % 2);
        if seed % 5 == 0 {
            g.currency_code = PbStr::new();
        }
        let k = cast::<usize, _>(seed * 7 % 15);
        let code = CURRENCIES(&__content)[k].0;
        let data = format!("{code}\r\n").into_bytes();
        let files: &[(&str, &[u8])] = &[("TMPDAT.WSR", &data)];
        let ans = format!("choose:{k}");
        let addr = if seed % 3 == 0 { 0x41dd39 } else { 0x40ca39 };
        let r = check(seed, addr, &g, &[ans.as_str()], files, &[], cmp(), move |g, ui, rng, env| {
            if addr == 0x41dd39 {
                run_currency_selector(g, ui, rng, env)
            } else {
                menu_select_currency(g, ui, rng, env)
            }
        });
        if let Some(r) = r {
            assert_eq!(r.g.pref_currency.to_display(), code);
        }
    });
}

#[test]
fn currency_menu_text() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.currency_code = st("JAP");
    assert_eq!(update_currency_menu_text(&g).to_display(), "Currenc&y (This game): The Yen");
    g.currency_code = st("NZ");
    g.currency_name = st(__content.string(6653));
    assert_eq!(update_currency_menu_text(&g).to_display(), __content.string(14141));
}

#[test]
fn diff_law_firm() {
    par_seeds(1..=4, |seed| {
        let mut g = base(seed);
        g.cur_player = 1 + cast::<i32, _>(seed % 3);
        g.player_law_firm[g.cur_player] = 2;
        // the harness's LawFirm stub returns 0: the original stores it; the port asks `choose`
        let ans = ["choose:0", "choose:2", "choose:cancel", "choose:1"][cast::<usize, _>(seed % 4)];
        let want = [1, 3, 2, 2][cast::<usize, _>(seed % 4)];
        let r = check(seed, 0x40ced1, &g, &[ans], &[], &[], cmp().ignore("PlayerLawFirm"), menu_select_law_firm);
        if let Some(r) = r {
            let cp = g.cur_player;
            assert_eq!(r.o.globals.player_law_firm[cp], 0);
            assert_eq!(r.g.player_law_firm[cp], want);
        }
    });
}

// --------------------------------------------------------------------------- misc menu entries

#[test]
fn diff_high_score_and_about() {
    par_seeds(1..=4, |seed| {
        let mut g = base(seed);
        if seed % 2 == 0 {
            g.licensee_name = st("Jane Doe");
            g.registration_num = st("12345");
        }
        if seed <= 2 {
            let scores: &[u8] = b"\"John Q. Public|   0    | 09-01-2010 | xxx |\"\r\n";
            check(seed, 0x40cf78, &g, &[], &[("SCORES.WSR", scores)], &[], cmp(), menu_high_score);
        } else {
            check(seed, 0x40f571, &g, &[], &[], &[], cmp(), show_about_box);
        }
    });
}

#[test]
fn diff_clear_chart_history() {
    par_seeds(1..=10, |seed| {
        let mut g = base(seed);
        g.cur_player = 1;
        g.sel_entity = [1, 0, 1700, 250, 600][cast::<usize, _>(seed % 5)];
        let ans: &[&str] = if seed % 2 == 0 { &["IDNO"] } else { &[] };
        check(seed, 0x40e372, &g, ans, &[], &[], cmp(), menu_clear_chart_history);
    });
}

#[test]
fn diff_exit_game() {
    par_seeds(1..=6, |seed| {
        let mut g = base(seed);
        g.clock_state = st(if seed % 3 == 0 { "RUN" } else { "" });
        g.ticker_on = 1;
        if seed == 5 {
            g.game_end_flag = st("Y");
        }
        g.chart_entity = 300;
        let ans: &[&str] = if seed % 2 == 0 { &["IDNO"] } else { &[] };
        let r = check(seed, 0x40c6c2, &g, ans, &[], &[(0x41ef3f, 1)], cmp(), menu_exit_game);
        if let Some(r) = r {
            assert_eq!(r.requests.contains(&HostRequest::Exit), seed % 2 == 1 || seed == 5);
        }
    });
}

#[test]
fn diff_web_pages() {
    let routines: [(u32, Port); 8] = [
        (0x40f8be, promo_speculator),
        (0x40fb76, open_strategy_manual),
        (0x4103cc, open_tutorial_page),
        (0x40f7ba, open_order_manual_page),
        (0x4102f0, open_forum_page),
        (0x4102b3, menu_upgrades),
        (0x40f506, show_help),
        (0x40d06c, menu_customizer),
    ];
    let corpname = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extracted/app/CORPNAME.DAT")).unwrap_or_default();
    par_seeds(1..=32, |seed| {
        let (addr, port) = routines[cast::<usize, _>(seed % 8)];
        let mut g = base(seed);
        g.ticker_on = cast::<i32, _>(seed % 2);
        g.game_in_progress = cast::<i32, _>((seed / 8) % 2);
        if (seed / 2) % 2 == 0 {
            g.licensee_name = st("Jane Doe");
        }
        let mut files: Vec<(&str, &[u8])> = vec![("CORPNAME.DAT", &corpname)];
        match (seed / 8) % 4 {
            0 => files.extend([("WSRBOOK.DOC", &b"x"[..]), ("WSRBOOK.HTM", &b"x"[..]), ("CUSTOMIZ.EXE", &b"x"[..])]),
            1 => files.push(("WSR_PDF.PDF", &b"x"[..])),
            2 => files.push(("CUSTOMIZ.EXE", &b"x"[..])),
            _ => {}
        }
        let ans: &[&str] = if seed % 3 == 0 { &["IDNO"] } else { &[] };
        check(seed, addr, &g, ans, &files, &[], cmp(), port);
    });
    // the small ones without own menu entries
    par_seeds(1..=6, |seed| {
        let g = base(seed);
        let addr = [0x5c30e5, 0x410144, 0x410144][cast::<usize, _>(seed % 3)];
        check(seed, addr, &g, &[], &[], &[], cmp(), move |g, ui, rng, env| match addr {
            0x5c30e5 => open_order_web_page(env),
            _ => open_updates_page(g, ui, rng, env, 0),
        });
    });
}

// -------------------------------------------------------------------------------------- new game

#[test]
fn diff_new_game() {
    let __content = wsr_core::content::current();
    let hit = Hit::default();
    par_seeds(1..=16, |seed| {
        let mut g = base(seed);
        g.game_in_progress = if seed % 4 == 0 { 1 } else { cast::<i32, _>(seed % 2) };
        g.win_version = st(if seed % 3 == 0 { "XP" } else { "7" });
        g.cmd_line_mode = st(if seed % 5 == 0 { "INSTALL" } else { "" });
        let n = cast::<usize, _>(seed % 6);
        let mut files: Vec<(String, Vec<u8>)> = vec![("GTIME.DAT".into(), b"1\r\n".to_vec())];
        for k in 1..=n {
            let body = if k == n.saturating_sub(1) && seed % 2 == 1 { b"@DUMMY\r\n".to_vec() } else { b"\"9.85\"\r\n".to_vec() };
            files.push((format!("{__wsr_text_14142}{k:02}{__wsr_text_14143}",__wsr_text_14142=__content.string(14142), __wsr_text_14143=__content.string(14143)), body));
        }
        if seed == 7 {
            g.game_in_progress = 0; // exercise the full save-slot scan instead of relaunching
            for k in 1..=52 {
                files.push((format!("{__wsr_text_14142}{k:02}{__wsr_text_14143}",__wsr_text_14142=__content.string(14142), __wsr_text_14143=__content.string(14143)), b"x\r\n".to_vec()));
            }
        }
        let fr: Vec<(&str, &[u8])> = files.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        // the setup itself and the ticker loop are other routines' business
        let skips = [(0x473e5b, 0), (0x41d9c2, 1)];
        if let Some(r) = check(seed, 0x40a74a, &g, &[], &fr, &skips, cmp(), menu_new_game) {
            hit.add(format!("{__wsr_text_14144}{} req {:?}", r.g.game_number, r.requests,__wsr_text_14144=__content.string(14144)));
            let _ = r.files;
        }
    });
    hit.assert(&["RunProgram", "Exit", "game 50", "game 1 "]);
}

#[test]
fn settings_menu_labels() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.cur_player = 1;
    g.player_growth_throttle[1] = 25;
    g.ticker_speed = 50;
    assert_eq!(settings_menu_label(&g, menu_id::GROWTH_THROTTLE).unwrap().to_display(), __content.string(14450));
    assert_eq!(settings_menu_label(&g, menu_id::TICKER_SPEED).unwrap().to_display(), "Ticker &Speed: 50");
    assert!(settings_menu_label(&g, menu_id::EXIT).is_none());
}
