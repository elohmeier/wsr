//! Tests of the new-game wizard and world builder (logic::newgame): hand-checked unit tests on tiny data
//! files, and the end-to-end differential test — the complete new world (all globals + RND state) built
//! by the original NewGameSetup (0x473e5b, run under emulation with its dialogs driven by scripted actions)
//! vs the port, for several sampled data files and dialog choices.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Rand, par_seeds, run_original_with};
use wsr_core::logic::newgame::{
    LocalTime, create_industry_combo, in_box_num_players, new_game_setup, show_startup_dialog,
};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const NEW_GAME_SETUP: u32 = 0x473e5b;

/// Callees made to return at once in the original: none (a computer player moving first takes its turn,
/// `AiComputerPlayerTurn`, on both sides).
const SKIPS: &[(u32, u32)] = &[];

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// The startup choices of a test game.
#[derive(Clone, Debug)]
struct Choices {
    cpu: bool,
    players: i32,
    humans: i32,
    currency: &'static str,
    /// New exchange rate entered (None: keep the default).
    rate: Option<&'static str>,
    names: Vec<String>,
    money: i32,
    years: i32,
    level: i32,
}

impl Choices {
    /// The scripted answers / dialog actions for the original (one queue, see tools/emu/game.py).
    fn original_answers(&self) -> Vec<String> {
        let mut a = vec![if self.cpu {
            "IDYES".to_string()
        } else {
            "IDNO".to_string()
        }];
        a.push(format!("text:310:{}", self.players));
        a.push("click:1".into());
        if self.cpu && self.players > 2 {
            a.push(self.humans.to_string());
        }
        if self.currency != "USA" {
            match self.rate {
                Some(r) => {
                    a.push("IDYES".into());
                    a.push(r.into());
                }
                None => a.push("IDNO".into()),
            }
        }
        for (k, n) in self.names.iter().enumerate() {
            a.push(format!("text:{}:{}", 342 + 2 * k, n));
        }
        a.push(format!("text:352:{}", self.money));
        a.push(format!("text:354:{}", self.years));
        a.push(format!("text:356:{}", self.level));
        a.push("click:1".into());
        if self.years > 35 {
            a.push("IDYES".into());
        }
        a
    }
}

/// Answers the port's dialogs from [`Choices`] and records everything shown.
struct ChoiceUi {
    c: Choices,
    log: Vec<String>,
}

impl Ui for ChoiceUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
    let __content = wsr_core::content::current();
        self.log.push(format!("MSGBOX [{title}] {text}"));
        let t = title.to_display();
        if t.ends_with(__content.string(13911)) {
            return if self.c.cpu { 6 } else { 7 };
        }
        if t == __content.string(6683) {
            return if self.c.rate.is_some() { 6 } else { 7 };
        }
        if style & 0xf == 3 || style & 0xf == 4 {
            6
        } else {
            1
        }
    }
    fn inputbox(&mut self, prompt: &PbStr, title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        self.log
            .push(format!("INPUTBOX [{title}] {prompt} ({default})"));
        let (t, p) = (title.to_display(), prompt.to_display());
        if t == __content.string(6635) {
            return st(&self.c.players.to_string());
        }
        if t == __content.string(6637) {
            return st(&self.c.humans.to_string());
        }
        if t == __content.string(6686) {
            return st(self.c.rate.unwrap_or(""));
        }
        if let Some(rest) = p.strip_prefix("Player ") {
            let k: usize = rest[..1].parse().unwrap();
            return st(&self.c.names[k - 1]);
        }
        if p.starts_with(__content.string(13912)) {
            return st(&self.c.money.to_string());
        }
        if p.starts_with(__content.string(13913)) {
            return st(&self.c.years.to_string());
        }
        if p.starts_with(__content.string(13914)) {
            return st(&self.c.level.to_string());
        }
        default.clone()
    }
    fn report(&mut self, title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
    let __content = wsr_core::content::current();
        self.log.push(format!("{__wsr_text_13539}{title}",__wsr_text_13539=__content.string(13539)));
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
    let __content = wsr_core::content::current();
        self.log.push(format!("{__wsr_text_13540}{headline}",__wsr_text_13540=__content.string(13540)));
    }
}

/// The state MenuNewGame (0x40a74a) leaves before NewGameSetup, plus what the tests vary. The fund slots
/// 1581..1600 are marked used (status 2) so InitETFs does not try to fill sector funds from a small world
/// (its holding picker loops until it finds 15 stocks per fund).
fn base_state(r: &mut Rand, currency: &str) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.app_title = st(__content.string(13661));
    g.game_number = cast::<i32, _>(r.range(1, 50));
    g.stock_index_base = 5000;
    g.currency_code = st(currency);
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    for c in 11..=1600 {
        g.country_idx[c] = cast::<i32, _>(r.range(0, 53));
    }
    for c in 1581..=1600 {
        g.entity_status[c] = 2;
    }
    g
}

/// The real WSR101.DAT with all but ~`keep` of its active companies made dormant (`ST=0`) and the fund
/// lines (1581..) removed.
fn sampled_data(r: &mut Rand, keep: Ext, famous: bool) -> Vec<u8> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../extracted/app/wsr101.dat"
    );
    let src = std::fs::read(path).expect("extracted/app/wsr101.dat");
    let mut out: Vec<u8> = Vec::new();
    for line in src.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let t: Vec<u8> = line.iter().copied().skip_while(|b| *b == b' ').collect();
        let mut l = line.to_vec();
        if t.first() == Some(&b'#') && t.len() >= 5 {
            let id: i32 = std::str::from_utf8(&t[1..5])
                .ok()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            if id >= 1581 {
                continue;
            }
            let keep_it = (famous && id == 140) || r.chance(keep);
            if let (false, Some(p)) = (keep_it, l.windows(4).position(|w| w == b"ST=1")) {
                l[p + 3] = b'0';
            }
        }
        out.extend_from_slice(&l);
        out.extend_from_slice(b"\r\n");
    }
    out
}

fn run_both(seed: u64, c: &Choices, mut g: Globals, data: Vec<u8>) {
    let __content = wsr_core::content::current();
    let answers = c.original_answers();
    let answers: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
    let opts = Opts {
        answers: &answers,
        skips: SKIPS.to_vec(),
        rng: Some(0x1234_5678 ^ cast::<u32, _>(seed)),
        files: vec![("WSR101.DAT".into(), data.clone())],
        ..Opts::default()
    };
    let Some(o) = run_original_with(NEW_GAME_SETUP, &opts, &g) else {
        return;
    };
    let mut rng = Rng::from_state(0x1234_5678 ^ cast::<u32, _>(seed));
    let mut ui = ChoiceUi {
        c: c.clone(),
        log: Vec::new(),
    };
    let files = |name: &str| {
        if name == "WSR101.DAT" {
            Some(data.clone())
        } else {
            None
        }
    };
    let written = new_game_setup(&mut g, &mut ui, &mut rng, &files, &LocalTime::HARNESS);
    let active = (11..=1580).filter(|&i| g.entity_status[i] == 1).count();
    eprintln!("seed {seed}: {c:?}: {active}{__wsr_text_13915}{:.1}{__wsr_text_13748}", o.elapsed,__wsr_text_13915=__content.string(13915), __wsr_text_13748=__content.string(13748));
    let msgs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("MSGBOX")).collect();
    assert!(
        o.log.iter().any(|l| l.contains(__content.string(13916))),
        "seed {seed}: the original did not finish:\n{:?}\nport UI: {:?}",
        msgs,
        ui.log
    );
    Compare::default()
        .ignore("TickerPauseUntil")
        .ignore("DlgSymbolEntry")
        .ignore("DlgStartup")
        .assert(&g, &o.globals);
    assert_eq!(rng.state(), o.rng, "seed {seed}: RND state");
    let cfg = o.files.get("CFIG.WSR").cloned().flatten();
    assert_eq!(
        written.into_iter().find(|w| w.0 == "cfig.wsr").map(|w| w.1),
        cfg,
        "seed {seed}{__wsr_text_13917}"
    ,__wsr_text_13917=__content.string(13917));
}

fn names(r: &mut Rand, n: i32) -> Vec<String> {
    const FIRST: [&str; 6] = ["Alice", "Bobby", "Carol", "Diego", "Erika", "Frank"];
    (0..n)
        .map(|k| {
            format!(
                "{} {}",
                FIRST[(cast::<usize, _>(k) + cast::<usize, _>(r.range(0, 5))) % 6],
                "Raider"
            )
        })
        .collect()
}

/// The complete new world vs the original, for sampled data files and varied dialog choices.
#[test]
fn new_game_matches_original() {
    if let Some(why) = diff::unavailable() {
        eprintln!("SKIPPED: {why}");
        return;
    }
    par_seeds(1..=6, |seed| {
        let mut r = Rand(0x4e47_0000 + seed);
        let currencies = ["USA", "UK", "JAP", "EUR", "ARG", "USA"];
        let currency = currencies[cast::<usize, _>(seed - 1)];
        let players = cast::<i32, _>(r.range(2, 5));
        let cpu = seed % 2 == 0;
        let humans = if cpu && players > 2 {
            cast::<i32, _>(r.range(1, cast::<i64, _>(players) - 1))
        } else {
            1
        };
        let rate = match currency {
            "JAP" => Some("160"),
            "ARG" => Some("400"),
            _ => None,
        };
        let c = Choices {
            cpu,
            players,
            humans,
            currency,
            rate,
            names: names(&mut r, players),
            money: [100, 500, 1000, 250][cast::<usize, _>(r.range(0, 3))],
            years: [12, 35, 40, 1][cast::<usize, _>(r.range(0, 3))],
            level: cast::<i32, _>(r.range(1, 4)),
        };
        let g = base_state(&mut r, currency);
        let data = sampled_data(&mut r, Ext::from_f64(0.01), seed % 3 == 0);
        run_both(seed, &c, g, data);
    });
}

// ------------------------------------------------------------------------------------------ unit tests

/// A small hand-made world (CRLF lines like the real file).
const TINY: &str = "REM test world\r\n\
**IND=01  INDNAM=BANKING\r\n\
#0011   ST=1  CASH=23000 PL1=3200\r\n\
#0012   ST=1 DV%=14  CASH=31000 PL1=1500  JBOS=12000  BCOUP=2023@03.00\r\n\
#0013   ST=0\r\n\
**IND=02  IDGRO%=05 IDVOL%=10 INDNAM=INSURANCE\r\n\
#0086   ST=1  CR=AA MAG=05 PL1=0800\r\n\
**IND=05  IDGRO%=07 IDVOL%=20 INDNAM=HOUSING DEVELOPMENT\r\n\
#0141   ST=1  CR=BBB MAG=08 BA%=12 GR%=05 DV%=20 PE=15\r\n\
#0142   ST=1  CR=B MAG=02 BA%=11 JBOS=0500 BCOUP=2026@07.50 CASH=00150\r\n\
141>30%\r\n\
**IND=70  INDNAM=HOLDING / TRADING COS.\r\n\
#1480   ST=1  CR=A MAG=09 BA%=11 CASH=05000\r\n\
141>45%\r\n\
142>85%\r\n\
@END\r\n";

fn tiny_game(c: &Choices) -> (Globals, ChoiceUi) {
    let mut r = Rand(7);
    let mut g = base_state(&mut r, c.currency);
    let mut ui = ChoiceUi {
        c: c.clone(),
        log: Vec::new(),
    };
    let mut rng = Rng::from_state(1);
    let files = |name: &str| {
        if name == "WSR101.DAT" {
            Some(TINY.as_bytes().to_vec())
        } else {
            None
        }
    };
    new_game_setup(&mut g, &mut ui, &mut rng, &files, &LocalTime::HARNESS);
    (g, ui)
}

fn two_humans() -> Choices {
    Choices {
        cpu: false,
        players: 2,
        humans: 2,
        currency: "USA",
        rate: None,
        names: vec!["  Alice  Smith ".into(), "Bob|by \"B\"".into()],
        money: 300,
        years: 12,
        level: 4,
    }
}

#[test]
fn tiny_world_is_parsed() {
    let __content = wsr_core::content::current();
    let (g, ui) = tiny_game(&two_humans());
    assert_eq!(g.num_players, 2);
    // names: trimmed, '|' and '"' blanked, double blanks collapsed once
    assert_eq!(g.player_name[1], "Alice Smith");
    assert_eq!(g.player_name[2], "Bob by B");
    assert_eq!(g.cash[1], Ext::from_f64(300.0));
    assert_eq!(g.start_net_worth, 300);
    assert_eq!((g.difficulty_level, g.cfg_difficulty_raw), (3, 4));
    // level 4: the capital taxes are not disabled
    assert_eq!(g.wealth_tax_rate, 0);
    assert!(
        ui.log
            .iter()
            .any(|l| l.contains(__content.string(13918)))
    );
    // the harness clock says 2000: the start year is never before 2024
    assert_eq!(
        (g.start_year, g.cur_year, g.game_end_year),
        (2024, 2024, 2036)
    );
    assert_eq!(g.long_bond_desc.as_bytes()[..5], *b"2044@");
    assert_eq!(g.short_bond_desc.as_bytes()[..5], *b"2034@");
    assert_eq!(g.gdp_level, 7000); // see BUGS.md: the random part always rounds away
    // industries
    assert_eq!(g.ind_name[2], "INSURANCE");
    assert_eq!(
        (
            g.ind_long_term_growth[5],
            g.ind_growth_rate_now[5],
            g.ind_volatility[5]
        ),
        (7, 7, 20)
    );
    // companies
    assert_eq!(
        (
            g.entity_status[11],
            g.entity_status[13],
            g.entity_status[141]
        ),
        (1, 0, 1)
    );
    assert_eq!(
        (g.industry_num[12], g.industry_num[86], g.industry_num[1480]),
        (1, 2, 70)
    );
    assert_eq!(g.lending_bank[12], 11); // a bank borrows from the previous bank
    assert_eq!(g.lending_bank[141], 12);
    assert_eq!(g.price_hist_high[(0, 13)], Ext::from_f64(0.0));
    // the 2023 bond of bank 12 was moved to StartYear..StartYear+4 and then the year rewritten in the
    // FinalizeWorld bond pass (maturity before the start year: never here)
    let y: i32 = String::from_utf8_lossy(&g.bond_issue_info[12].as_bytes()[..4])
        .parse()
        .unwrap();
    assert!((2024..=2028).contains(&y), "{}", g.bond_issue_info[12]);
    assert_eq!(g.bond_issue_info[12].as_bytes()[4..], *b"@03.00");
    // holdings of the holding company
    let held: Vec<(i16, i16)> = (1..=g.hold_stock_id[(0, 1480)])
        .map(|k| {
            (
                g.hold_stock_id[(cast::<i32, _>(k), 1480)],
                g.hold_stock_pct[(cast::<i32, _>(k), 1480)],
            )
        })
        .collect();
    assert_eq!(held, vec![(141, 45), (142, 85)]);
    // >= 80% of an operating company: controlled since 1950
    assert_eq!(g.ctrl_since_year[142], 1950);
    // turn order, game state, news header
    assert!(g.player_turn_order[1] == 1 || g.player_turn_order[1] == 2);
    assert_eq!(g.player_turn_order[1] + g.player_turn_order[2], 3);
    assert_eq!(g.cur_player, g.player_turn_order[1]);
    assert_eq!(g.sel_entity, g.cur_player);
    assert_eq!(
        (
            g.game_in_progress,
            g.world_generated,
            g.sim_co_count,
            g.econ_cycle_counter
        ),
        (1, 1, 6, 25)
    );
    assert_eq!(g.news_headline[60].as_bytes()[..5], *b"-----");
    assert_eq!(
        g.ind_combo_items[0].as_bytes()[..25],
        *__content.bytes(6735)
    );
}

#[test]
fn cancel_on_the_first_question_changes_nothing() {
    struct CancelUi;
    impl Ui for CancelUi {
        fn msgbox(&mut self, _t: &PbStr, _s: i32, _ti: &PbStr) -> i32 {
            2
        }
        fn inputbox(&mut self, _p: &PbStr, _t: &PbStr, d: &PbStr) -> PbStr {
            d.clone()
        }
        fn report(&mut self, _t: &PbStr, _l: &[PbStr], _y: bool) -> ReportAction {
            ReportAction::Ok
        }
        fn news(&mut self, _h: &PbStr) {}
    }
    let g0 = Globals::default();
    let mut g = g0.clone();
    let mut rng = Rng::from_state(5);
    new_game_setup(
        &mut g,
        &mut CancelUi,
        &mut rng,
        &|_: &str| None,
        &LocalTime::HARNESS,
    );
    assert!(Compare::default().diffs(&g, &g0).is_empty());
    assert_eq!(rng.state(), 5);
}

#[test]
fn computer_players_by_number_of_humans() {
    let __content = wsr_core::content::current();
    let mut c = two_humans();
    c.cpu = true;
    c.players = 5;
    c.humans = 2;
    c.names = (1..=5).map(|k| format!("{__wsr_text_13556}{k}{__wsr_text_13557}",__wsr_text_13556=__content.string(13556), __wsr_text_13557=__content.string(13557))).collect();
    let (g, ui) = tiny_game(&c);
    assert_eq!(g.has_cpu_players, "Y");
    let cpu: Vec<i32> = (1..=5).map(|k| g.is_computer_player[k]).collect();
    assert_eq!(cpu, vec![1, 0, 1, 0, 1]);
    assert_eq!(g.cpu_max_corps, 40);
    assert_eq!((g.player_law_firm[1], g.player_law_firm[2]), (3, 2));
    // a computer player never moves first (player 1 instead)
    assert!(g.is_computer_player[g.player_turn_order[1]] == 0 || g.player_turn_order[1] == 1);
    assert!(ui.log.iter().any(|l| l.contains(__content.string(6637))));
}

#[test]
fn currency_with_new_exchange_rate() {
    let __content = wsr_core::content::current();
    let mut c = two_humans();
    c.currency = "JAP";
    c.rate = Some("120");
    let (g, ui) = tiny_game(&c);
    assert_eq!(g.currency_symbol, PbStr(vec![165, b' ']));
    assert_eq!(g.million_prefix_lc, "b");
    // 120 yen per dollar, entered per thousand: 0.12 (SINGLE product)
    assert_eq!(g.exchange_rate, cast::<Ext, _>(120.0f32) * (cast::<Ext, _>(0.001f32)));
    assert_eq!(
        g.max_bond_issue,
        cast::<i64, _>((g.exchange_rate * Ext::from_f64(150000.0)).round())
    );
    assert!(
        ui.log
            .iter()
            .any(|l| l.contains(__content.string(14427))),
        "{:?}",
        ui.log
    );
    assert!(
        g.news_headline
            .iter()
            .any(|(_, h)| *h == "Conversion rate of 120.00 per U.S. dollar....")
    );
}

#[test]
fn number_of_players_dialog() {
    let __content = wsr_core::content::current();
    struct Ans(Vec<&'static str>, Vec<String>);
    impl Ui for Ans {
        fn msgbox(&mut self, t: &PbStr, _s: i32, _ti: &PbStr) -> i32 {
            self.1.push(t.to_display());
            1
        }
        fn inputbox(&mut self, p: &PbStr, t: &PbStr, d: &PbStr) -> PbStr {
            self.1.push(format!("{t}|{p}|{d}"));
            st(self.0.remove(0))
        }
        fn report(&mut self, _t: &PbStr, _l: &[PbStr], _y: bool) -> ReportAction {
            ReportAction::Ok
        }
        fn news(&mut self, _h: &PbStr) {}
    }
    let mut g = Globals::default();
    g.input_dlg_label = st(__content.string(6635));
    g.input_dlg_prompt = st(__content.string(13919));
    g.num_players = 4;
    g.ticker_on = 1;
    let mut ui = Ans(vec!["7"], Vec::new());
    in_box_num_players(&mut g, &mut ui);
    assert_eq!(
        ui.1[0],
        "Number of players|How many?\r\n\r\nClick 'OK' to enter.|4"
    );
    assert_eq!(ui.1[1], __content.string(6718));
    assert_eq!(
        (
            g.num_players,
            g.symbol_dlg_text.to_display(),
            g.clock_state.to_display()
        ),
        (7, "7".into(), "RUN".into())
    );
    let mut ui = Ans(vec![""], Vec::new());
    in_box_num_players(&mut g, &mut ui);
    assert_eq!(
        ui.1[0],
        "Number of players|How many?\r\n\r\nClick 'OK' to enter.|2"
    );
    assert_eq!(
        (g.num_players, g.symbol_dlg_text.to_display(), g.in_box_mode),
        (7, "Cancel".into(), 0)
    );
}

#[test]
fn startup_dialog_validates_and_retries() {
    let __content = wsr_core::content::current();
    struct Seq(Vec<&'static str>, Vec<String>, i32);
    impl Ui for Seq {
        fn msgbox(&mut self, t: &PbStr, _s: i32, _ti: &PbStr) -> i32 {
            self.1.push(t.to_display());
            self.2
        }
        fn inputbox(&mut self, p: &PbStr, _t: &PbStr, d: &PbStr) -> PbStr {
            self.1.push(format!("{p}={d}"));
            st(self.0.remove(0))
        }
        fn report(&mut self, _t: &PbStr, _l: &[PbStr], _y: bool) -> ReportAction {
            ReportAction::Ok
        }
        fn news(&mut self, _h: &PbStr) {}
    }
    let mut g = Globals::default();
    g.num_players = 2;
    let mut rng = Rng::from_state(3);
    // 1st OK: player 2 too short; 2nd OK: 50 years declined -> box reset to 35; 3rd OK: accepted
    let answers = vec![
        "Alice Smith",
        "Bob",
        "500",
        "12",
        "2", //
        "Alice Smith",
        "Bobby Jones",
        "500",
        "50",
        "2", //
        "Alice Smith",
        "Bobby Jones",
        "800",
        "35",
        "1",
    ];
    let mut ui = Seq(answers, Vec::new(), 7);
    show_startup_dialog(&mut g, &mut ui, &mut rng);
    assert!(ui.0.is_empty());
    assert!(
        ui.1.contains(&__content.string(14428).to_string())
    );
    assert!(ui.1.contains(&__content.string(6732).to_string()));
    assert!(
        ui.1.contains(&"Game Length: (1 to 999 years)=35".to_string()),
        "{:?}",
        ui.1
    );
    assert_eq!(ui.1[1], __content.string(13920));
    assert_eq!(
        (g.start_net_worth, g.game_end_year, g.difficulty_level),
        (800, 35, 1)
    );
    assert_eq!(g.pref_player_names[2], "Bobby Jones");
    let r = Rng::from_state(3).rnd();
    assert_eq!(
        g.market_valuation,
        cast::<f32, _>((Ext::from_f64(1.0) * Ext::from_f64(0.25)) * r + cast::<Ext, _>(0.65f32))
    );
}

#[test]
fn industry_combo_is_sorted() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.ind_name[1] = st("BANKING");
    g.ind_name[2] = st("AUTO");
    g.ind_name[3] = st("Zinc");
    create_industry_combo(&mut g);
    let pad = " ".repeat(75);
    // 68 empty names first (sorted by their number), then AUTO, BANKING, Zinc
    assert_eq!(g.ind_combo_items[1], st(&format!("{pad}04")));
    assert_eq!(g.ind_combo_items[69], st(&format!("AUTO{pad}02")));
    assert_eq!(g.ind_combo_items[70], st(&format!("BANKING{pad}01")));
    assert_eq!(g.ind_combo_items[71], st(&format!("Zinc{pad}03")));
    assert_eq!(
        g.ind_combo_items[0],
        st(&__content.string(14429).replacen("{}", &" ".repeat(53), 1))
    );
}
