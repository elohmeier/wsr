//! Differential tests of the cheat menu (logic::info_screens::cheat): CheatMenu (0x67383a) through its
//! dialog (option click + button), DlgCheatMenu_Go (0x67515d) with a preset option, CheatInsiderMergerTip
//! (0x677230) and CheatDisableAntitrust (0x680954).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use wsr_core::logic::info_screens::cheat::{
    cheat_disable_antitrust, cheat_insider_merger_tip, cheat_menu, cheat_menu_options, dlg_cheat_menu_go,
};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Shared-queue UI like the harness: every MSGBOX and INPUTBOX pops the next answer (`IDYES` … or a
/// number / a text); quick messages (style 0, no title) take none. `choice` answers the cheat dialog.
#[derive(Default)]
struct Tui {
    q: VecDeque<String>,
    choice: Option<usize>,
    chose: usize,
    /// `MSGBOX 'title' 'text' type=0x..` lines in the harness log format.
    msgs: Vec<String>,
    quicks: usize,
    quick_texts: Vec<String>,
    inputs: usize,
    news: Vec<String>,
    options: Vec<String>,
}

const ANSWERS: [(&str, i32); 7] =
    [("IDOK", 1), ("IDCANCEL", 2), ("IDABORT", 3), ("IDRETRY", 4), ("IDIGNORE", 5), ("IDYES", 6), ("IDNO", 7)];

/// Python `repr` of a latin-1 string (the harness logs message boxes with `%r`).
fn py_repr(b: &[u8]) -> String {
    let __content = wsr_core::content::current();
    let q = if b.contains(&b'\'') && !b.contains(&b'"') { '"' } else { '\'' };
    let mut o = String::new();
    o.push(q);
    for &c in b {
        match c {
            b'\\' => o.push_str("\\\\"),
            b'\r' => o.push_str("\\r"),
            b'\n' => o.push_str("\\n"),
            b'\t' => o.push_str("\\t"),
            c if c as char == q => {
                o.push('\\');
                o.push(q)
            }
            c if c < 0x20 || (0x7f..0xa1).contains(&c) || c == 0xad => o.push_str(&format!("{__wsr_text_13660}{c:02x}",__wsr_text_13660=__content.string(13660))),
            c => o.push(c as char),
        }
    }
    o.push(q);
    o
}

impl Ui for Tui {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        if style == 0 && title.is_empty() {
            self.quicks += 1;
            self.quick_texts.push(text.to_display());
            return 1;
        }
        self.msgs.push(format!("MSGBOX {} {} type=0x{:x}", py_repr(title.as_bytes()), py_repr(text.as_bytes()), style));
        match self.q.pop_front() {
            Some(a) => ANSWERS.iter().find(|x| x.0 == a).map(|x| x.1).unwrap_or_else(|| a.parse().unwrap()),
            None => {
                if matches!(style & 0xf, 3 | 4) {
                    6
                } else {
                    1
                }
            }
        }
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
        self.inputs += 1;
        PbStr::from(self.q.pop_front().unwrap_or_default().as_str())
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
    fn choose(&mut self, _title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        self.chose += 1;
        self.options = options.iter().map(|o| o.to_display()).collect();
        self.choice
    }
}

/// Control ids of the dialog's options in screen order (add cash, merger, earnings, antitrust).
const OPT_IDS: [i32; 4] = [1015, 1005, 1010, 1012];

/// The original's message boxes in the format of [`Tui::msgs`].
fn orig_msgboxes(o: &Original) -> Vec<String> {
    o.log
        .iter()
        .filter(|l| l.starts_with("MSGBOX"))
        .map(|l| l[..l.find(" -> ").unwrap()].to_string())
        .collect()
}

fn orig_quicks(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("API CreateProcessA(") && l.contains("qm.exe")).count()
}

fn orig_inputs(o: &Original) -> usize {
    o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count()
}

/// Branch coverage: fragments of message box lines / news / quick message counts seen.
#[derive(Default)]
struct Hits(Mutex<Vec<String>>);

impl Hits {
    fn record(&self, ui: &Tui) {
        let mut h = self.0.lock().unwrap();
        h.extend(ui.msgs.iter().cloned());
        h.extend(ui.news.iter().cloned());
        h.extend(ui.quick_texts.iter().cloned());
        h.extend(ui.options.iter().cloned());
    }
    fn add(&self, s: &str) {
        self.0.lock().unwrap().push(s.to_string());
    }
    fn assert(&self, want: &[&str]) {
        if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
            return;
        }
        let h = self.0.lock().unwrap();
        let missing: Vec<&&str> = want.iter().filter(|w| !h.iter().any(|x| x.contains(**w))).collect();
        assert!(missing.is_empty(), "branches not hit: {missing:?}");
    }
}

/// Runs the port on a copy of `g` and the original with the equivalent answers (`dialog` actions
/// first, then the shared MSGBOX/INPUTBOX answers); compares state, RND, message boxes (text and
/// title), quick messages, input boxes and news counts.
#[allow(clippy::too_many_arguments)]
fn check(
    seed: u64,
    addr: u32,
    args: &[&str],
    g: &Globals,
    choice: Option<usize>,
    dialog: &[String],
    answers: &[String],
    port: impl Fn(&mut Globals, &mut Tui, &mut Rng),
) -> Option<(Tui, Globals, Original)> {
    let rng0 = cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = Tui { q: answers.iter().cloned().collect(), choice, ..Tui::default() };
    port(&mut pg, &mut ui, &mut rng);
    let mut oa: Vec<String> = dialog.to_vec();
    oa.extend(answers.iter().cloned());
    let oa: Vec<&str> = oa.iter().map(|s| s.as_str()).collect();
    let o = run(addr, args, g, rng0, &oa, &[])?;
    let d = cmp().ignore("hCheatDlg").diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs, orig_msgboxes(&o), "msgboxes seed {seed}");
    assert_eq!(ui.quicks, orig_quicks(&o), "quick messages seed {seed}: {:?}", o.log);
    assert_eq!(ui.inputs, orig_inputs(&o), "inputs seed {seed}");
    let orig_news = (1..=60).filter(|&k| o.globals.news_headline[k] != g.news_headline[k]).count();
    let port_news = (1..=60).filter(|&k| pg.news_headline[k] != g.news_headline[k]).count();
    assert_eq!(port_news, orig_news, "news seed {seed}");
    Some((ui, pg, o))
}

/// A world with the cheat menu's state randomised: cheat mode, game state, the player's cheat code
/// and cash, the tipster and insider tip state, probes, pending mergers, currency, antitrust cheats
/// and lawsuit records.
fn setup(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x51_7cc1_b727_220a) | 1);
    g.news_msg = PbStr::new();
    g.cheat_mode = st(if r.chance(Ext::from_f64(0.04)) { "N" } else { "Y" });
    g.game_end_flag = st(if r.chance(Ext::from_f64(0.03)) { "Y" } else { "N" });
    g.game_in_progress = if r.chance(Ext::from_f64(0.03)) { 0 } else { 1 };
    let cp = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player = cp;
    g.cur_player_name = g.player_name[cp].clone();
    if g.cur_player_name.is_empty() {
        g.cur_player_name = word(&mut r, 8);
    }
    g.entity_status[cp] = if r.chance(Ext::from_f64(0.03)) { 0 } else { 1 };
    g.player_cheat_code[cp] = if r.chance(Ext::from_f64(0.5)) { 0 } else { cast::<i32, _>(r.range(7, 11)) };
    g.long_game_marker = if r.chance(Ext::from_f64(0.2)) { 6 } else { 0 };
    if r.chance(Ext::from_f64(0.15)) {
        g.game_end_year = g.start_year + 40;
    }
    if r.chance(Ext::from_f64(0.12)) {
        g.cur_year = g.game_end_year - cast::<i32, _>(r.range(1, 2));
    }
    g.rand_seed = cast::<i32, _>(r.range(-100000, 100000));
    match r.range(0, 9) {
        0 => g.cash[cp] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.45)),
        1 => g.cash[cp] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.01)),
        2 => g.cash[cp] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(40.0)),
        _ => {}
    }
    if r.chance(Ext::from_f64(0.3)) {
        // a small-time investor: net worth (cash only) between 1 and 50 times the exchange rate
        for s in 1..=15 {
            g.hold_stock_id[(s, cp)] = 0;
            g.hold_stock_pct[(s, cp)] = 0;
        }
        g.hold_stock_id[(0, cp)] = 0;
        g.bank_debt[cp] = Ext::from_f64(0.0);
        g.cash[cp] = g.exchange_rate * f64r(&mut r, Ext::from_f64(2.0), Ext::from_f64(40.0));
    }
    g.tipster_state[cp] = [-1, 0, 0, 0, 1, 2, 3, 4][cast::<usize, _>(r.range(0, 7))];
    g.insider_probe_player = match r.range(0, 3) {
        0 => cp,
        1 => cast::<i32, _>(r.range(1, 4)),
        _ => 0,
    };
    g.merger_tip_corp = 0;
    g.insider_earn_tip_corp = 0;
    g.insider_tippee = 0;
    g.merger_target = 0;
    g.merger_acquirer = 0;
    let pick = |r: &mut Rand, g: &Globals| loop {
        let c = cast::<i32, _>(r.range(131, 1580));
        if g.entity_status[c] == 1 {
            return c;
        }
    };
    match r.range(0, 5) {
        0 | 1 => {}
        2 => {
            g.merger_tip_corp = cast::<i16, _>(pick(&mut r, &g));
            if r.chance(Ext::from_f64(0.4)) {
                g.merger_target = cast::<i32, _>(g.merger_tip_corp);
                g.merger_acquirer = if r.chance(Ext::from_f64(0.8)) { pick(&mut r, &g) } else { 0 };
            }
        }
        _ => {
            let c = pick(&mut r, &g);
            g.insider_earn_tip_corp = if r.chance(Ext::from_f64(0.6)) { cast::<i16, _>(c) } else { -(cast::<i16, _>(c)) };
            if r.chance(Ext::from_f64(0.15)) {
                g.entity_status[c] = 2;
            }
        }
    }
    if g.merger_tip_corp != 0 || g.insider_earn_tip_corp != 0 {
        g.insider_tippee = if r.chance(Ext::from_f64(0.7)) { cast::<i16, _>(cp) } else if r.chance(Ext::from_f64(0.5)) { 0 } else { cast::<i16, _>(r.range(1, 4)) };
    } else if r.chance(Ext::from_f64(0.1)) {
        g.insider_tippee = cast::<i16, _>(cp);
    }
    if r.chance(Ext::from_f64(0.2)) {
        g.merger_target = pick(&mut r, &g);
        g.merger_acquirer = pick(&mut r, &g);
    }
    if r.chance(Ext::from_f64(0.4)) {
        g.insider_tip_event_tick = cast::<i16, _>(g.cur_quarter);
    } else {
        g.insider_tip_event_tick = cast::<i16, _>(r.range(0, 1500));
    }
    let iet = (cast::<i32, _>(g.insider_earn_tip_corp)).abs();
    if iet > 10 && r.chance(Ext::from_f64(0.5)) {
        g.proj_qtr_earnings[iet] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(20.0));
    }
    g.currency_code = st(["USA", "UK", "AUS", "CAN", "NZ", "SAF", "DAN", "SWE", "SWI", "NOR", "HK", "SNG", "EUR"]
        [cast::<usize, _>(r.range(0, 12))]);
    g.private_antitrust_off = cast::<i32, _>(r.range(0, 1));
    g.govt_antitrust_off = cast::<i32, _>(r.range(0, 1));
    for k in 1..=10 {
        g.lawsuit_rec[k] = match r.range(0, 3) {
            0 => st(&format!("1 | {}| {}|0|5", r.range(131, 1500), r.range(1, 4))),
            1 => st(&format!("2|{}|{}|0|5", r.range(1, 4), r.range(131, 1500))),
            _ => PbStr::new(),
        };
    }
    g.antitrust_defendant = cast::<i32, _>(r.range(0, 1500));
    g.antitrust_plaintiff = cast::<i32, _>(r.range(0, 4));
    g.antitrust_damages = r.range(0, 5000);
    g
}

/// MSGBOX / INPUTBOX answers: a few random tokens.
fn answers(r: &mut Rand) -> Vec<String> {
    // Numeric tokens can land on either INPUTBOX or MSGBOX in these randomized flows; every token
    // must also be a valid integer reply for the emulator's MessageBoxA stub.
    let pool = ["IDYES", "IDYES", "IDNO", "IDCANCEL", "IDOK", "250", "-30", "-100000", "0", "5000000", "40", "1500"];
    (0..r.range(0, 6)).map(|_| pool[cast::<usize, _>(r.range(0, cast::<i64, _>(pool.len()) - 1))].to_string()).collect()
}

#[allow(non_snake_case)]
fn MENU_WANT(__content: &wsr_core::content::ContentPack) -> Vec<&str> { vec![
    "Seek",
    __content.string(13860),
    __content.string(13861),
    __content.string(5746),
    __content.string(5755),
    __content.string(5794),
    __content.string(5800),
    __content.string(5809),
    __content.string(5814),
    __content.string(5819),
    __content.string(5820),
    __content.string(5874),
    __content.string(13862),
    __content.string(5994),
    __content.string(6025),
    __content.string(6035),
    __content.string(6013),
    "quick",
    "cash changed",
] }

#[test]
fn cheat_menu_vs_original() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=400, |seed| {
        let g = setup(seed);
        let mut r = Rand(seed.wrapping_mul(0x2f69_3a1b) | 7);
        let choice = match r.range(0, 9) {
            0 => None,
            1 | 2 => Some(0),
            3 | 4 => Some(1),
            5 | 6 => Some(2),
            _ => Some(3),
        };
        let mut dialog = Vec::new();
        if let Some(k) = choice {
            dialog.push(format!("click:{}", OPT_IDS[k]));
        }
        dialog.push("click:1".to_string());
        let ans = answers(&mut r);
        let Some((ui, pg, o)) = check(seed, 0x67383a, &[], &g, choice, &dialog, &ans, |g, ui, rng| cheat_menu(g, ui, rng, 0))
        else {
            return;
        };
        // the option labels the port offers are the original's dialog controls
        if ui.chose > 0 {
            let d = o.dialogs.iter().find(|d| d.title.to_display().starts_with(__content.string(13863))).expect("cheat dialog");
            for (k, id) in OPT_IDS.iter().enumerate() {
                assert_eq!(ui.options[k], d.control(*id).unwrap().text.to_display(), "{__wsr_text_13864}{k} seed {seed}",__wsr_text_13864=__content.string(13864));
            }
        }
        hits.record(&ui);
        if ui.quicks > 0 {
            hits.add("quick");
        }
        if pg.cash[g.cur_player] != g.cash[g.cur_player] {
            hits.add("cash changed");
        }
        let _ = cheat_menu_options;
    });
    hits.assert(&MENU_WANT(&__content));
}

#[allow(non_snake_case)]
fn GO_WANT(__content: &wsr_core::content::ContentPack) -> Vec<&str> { vec![
    __content.string(5806),
    __content.string(5808),
    __content.string(13865),
    __content.string(5817),
    __content.string(5882),
    __content.string(5881),
    __content.string(13866),
    __content.string(13867),
    __content.string(13868),
    __content.string(5920),
    __content.string(5927),
    __content.string(13869),
    __content.string(13870),
    __content.string(13871),
    __content.string(13872),
    __content.string(13873),
    __content.string(13874),
    __content.string(13875),
    __content.string(13876),
    __content.string(13877),
    __content.string(13878),
] }

/// DlgCheatMenu_Go called directly with `TmpExt1` = 2 or 3 (the tip options), varied tip state.
#[test]
fn dlg_cheat_menu_go_vs_original() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=500, |seed| {
        let mut g = setup(seed + 10_000);
        let mut r = Rand(seed.wrapping_mul(0x6a09_e667) | 5);
        // mostly an established tipster with a running tip
        if r.chance(Ext::from_f64(0.7)) {
            let cp = g.cur_player;
            g.tipster_state[cp] = [-1, 1, 2, 3, 4, 4][cast::<usize, _>(r.range(0, 5))];
            if g.merger_tip_corp != 0 || g.insider_earn_tip_corp != 0 {
                g.insider_tippee = cast::<i16, _>(cp);
            }
            if g.cur_year >= g.game_end_year - 2 {
                g.cur_year = g.game_end_year - 3;
            }
        }
        if r.chance(Ext::from_f64(0.3)) {
            g.news_msg = st("Stale headline");
        }
        g.tmp_ext1 = cast::<Ext, _>(r.range(2, 3));
        // These situations need ownership/short-position relationships that independent random
        // fields almost never produce. Keep the first cases deterministic while retaining the
        // randomized worlds for the remaining branches.
        if seed <= 10 {
            g.cur_player = 1;
            g.num_players = 2;
            g.cur_player_name = g.player_name[1].clone();
            g.cur_year = g.game_end_year - 5;
            g.entity_status[1] = 1;
            g.entity_status[2] = 1;
            g.entity_status[11] = 1;
            g.entity_status[131] = 1;
            g.hold_stock_id.reset();
            g.hold_stock_pct.reset();
            g.hold_stock_basis.reset();
            g.cash[1] = Ext::from_f64(1000.0);
            g.bank_debt[1] = Ext::from_f64(0.0);
            g.merger_tip_corp = 0;
            g.merger_target = 0;
            g.insider_earn_tip_corp = 0;
            g.insider_tippee = 1;
            g.tipster_state[1] = 2;
            g.tmp_ext1 = Ext::from_f64(3.0);
            if seed == 1 {
                g.insider_earn_tip_corp = 131;
                for (owner, stock) in [(2, 11), (11, 131)] {
                    g.hold_stock_id[(0, owner)] = 1;
                    g.hold_stock_id[(1, owner)] = cast::<i16, _>(stock);
                    g.hold_stock_pct[(1, owner)] = 100;
                }
                g.corp_controller[11] = 2;
                g.corp_controller[131] = 2;
            } else if seed == 2 {
                g.insider_earn_tip_corp = -131;
                g.hold_stock_id[(0, 1)] = 1;
                g.hold_stock_id[(1, 1)] = 131;
                g.hold_stock_pct[(1, 1)] = -10;
            } else {
                g.tmp_ext1 = Ext::from_f64(2.0);
                g.tipster_state[1] = 4;
                g.insider_tippee = 0;
                g.insider_probe_player = 2;
            }
        }
        let ans = answers(&mut r);
        let Some((ui, _, _)) = check(
            seed,
            0x67515d,
            &["v:0", "v:0x111", "v:1", "v:0"],
            &g,
            None,
            &[],
            &ans,
            |g, ui, rng| dlg_cheat_menu_go(g, ui, rng, 0),
        ) else {
            return;
        };
        hits.record(&ui);
    });
    hits.assert(&GO_WANT(&__content));
}

#[allow(non_snake_case)]
fn TIP_WANT(__content: &wsr_core::content::ContentPack) -> Vec<&str> { vec![
    __content.string(13862),
    __content.string(13879),
    __content.string(5960),
    __content.string(5956),
    __content.string(5973),
    __content.string(6002),
    __content.string(13880),
    __content.string(13881),
    __content.string(5994),
    __content.string(13882),
    __content.string(5987),
    __content.string(13883),
    __content.string(13884),
] }

/// CheatInsiderMergerTip called directly (worlds without a running tip most of the time).
#[test]
fn cheat_insider_merger_tip_vs_original() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=300, |seed| {
        let mut g = setup(seed + 20_000);
        let mut r = Rand(seed.wrapping_mul(0x3c6e_f372) | 3);
        let cp = g.cur_player;
        if r.chance(Ext::from_f64(0.8)) {
            g.merger_tip_corp = 0;
            g.insider_earn_tip_corp = 0;
            g.merger_target = 0;
        }
        g.tipster_state[cp] = cast::<i16, _>(r.range(0, 3));
        if r.chance(Ext::from_f64(0.3)) {
            g.currency_code = st("UK");
        }
        if r.chance(Ext::from_f64(0.15)) {
            g.cash[cp] = g.exchange_rate * Ext::from_f64(40000.0);
        }
        // make some companies look like candidates (rating, float) and give some falling histories
        for _ in 0..r.range(0, 30) {
            let c = cast::<i32, _>(r.range(131, 1580));
            if r.chance(Ext::from_f64(0.5)) {
                for k in 50..=60 {
                    g.cap_hist_close[(k, c)] = cast::<Ext, _>(61 - k) * Ext::from_f64(3.0);
                }
            }
        }
        let ans: Vec<String> = if r.chance(Ext::from_f64(0.8)) { vec!["IDYES".into()] } else { vec!["IDNO".into()] };
        let Some((ui, _, _)) =
            check(seed, 0x677230, &[], &g, None, &[], &ans, |g, ui, rng| cheat_insider_merger_tip(g, ui, rng, 0))
        else {
            return;
        };
        hits.record(&ui);
    });
    hits.assert(&TIP_WANT(&__content));
}

#[allow(non_snake_case)]
fn ANTI_WANT(__content: &wsr_core::content::ContentPack) -> Vec<&str> { vec![
    __content.string(6013),
    __content.string(6022),
    __content.string(13885),
    __content.string(6025),
    "'Do you want to turn off",
    __content.string(6035),
    "lawsuits cleared",
] }

/// CheatDisableAntitrust called directly.
#[test]
fn cheat_disable_antitrust_vs_original() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(1..=120, |seed| {
        let g = setup(seed + 30_000);
        let mut r = Rand(seed.wrapping_mul(0x9b05_688c) | 9);
        let ans: Vec<String> = (0..r.range(0, 3))
            .map(|_| ["IDYES", "IDNO", "IDCANCEL", "IDOK"][cast::<usize, _>(r.range(0, 3))].to_string())
            .collect();
        let Some((ui, pg, _)) = check(seed, 0x680954, &[], &g, None, &[], &ans, |g, ui, _| cheat_disable_antitrust(g, ui))
        else {
            return;
        };
        hits.record(&ui);
        if pg.lawsuit_rec.as_slice() != g.lawsuit_rec.as_slice() {
            hits.add("lawsuits cleared");
        }
    });
    hits.assert(&ANTI_WANT(&__content));
}
