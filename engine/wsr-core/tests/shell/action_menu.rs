//! Tests of logic::shell::action_menu: ShowActionMenu (0x4131ea) with its modal loop, the button callbacks
//! and DlgProcActionMenu against the original (emulator harness).
use wsr_core::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::common::*;
use super::diff::*;
use super::support::*;
use std::collections::VecDeque;
use std::sync::Mutex;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::logic::shell::Env;
use wsr_core::logic::shell::action_menu::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SHOW_ACTION_MENU: u32 = 0x4131ea;
const NOW: LocalTime = LocalTime::HARNESS;

/// Routines skipped in the original and the port (address, stdcall args).
const SKIPS: &[(u32, u32)] = &[
    (0x41ef3f, 1), // TickerSimSteps
    (0x4400d7, 0), // UpdateMarketDisplay
    (0x557bbf, 0), // BuildMenuAdviceText (PORT-TODO)
    (0x552f0b, 1), // DoTransactionMenuAction
    (0x4721dc, 1), // SelectActingEntity
    (0x40f506, 0), // ShowHelp
];

fn skip_addrs(skips: &[(u32, u32)]) -> Vec<u32> {
    skips.iter().map(|s| s.0).collect()
}

/// Answers `choose` from tokens: `btn:k` (command button k mod count, 0-based), `player`, `corp`, `close`,
/// `x` (close box); default CLOSE. Records every menu shown and the equivalent harness action.
#[derive(Default)]
struct MUi {
    q: VecDeque<String>,
    menus: Vec<(String, Vec<String>)>,
    actions: Vec<String>,
    msgs: usize,
    news: usize,
}

impl Ui for MUi {
    fn msgbox(&mut self, _text: &PbStr, style: i32, title: &PbStr) -> i32 {
    let __content = wsr_core::content::current();
        self.msgs += 1;
        if style == 0 && title.is_empty() { return 1; }
        // The real ticker can reach an overdraft. Let the computer choose the mandatory stock
        // sales instead of requesting manual selection and then cancelling that selection forever.
        let reply = if title.to_display().starts_with(__content.string(14119)) { 7 }
            else if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 };
        self.actions.push(reply.to_string());
        reply
    }
    fn inputbox(&mut self, _p: &PbStr, _t: &PbStr, _d: &PbStr) -> PbStr {
        PbStr::new()
    }
    fn report(&mut self, _t: &PbStr, _l: &[PbStr], _y: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _h: &PbStr) {
        self.news += 1;
    }
    fn choose(&mut self, title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        self.menus.push((title.to_display(), options.iter().map(|o| o.to_display()).collect()));
        let n = options.len() - 3;
        let tok = self.q.pop_front().unwrap_or_else(|| "close".into());
        let (k, act) = match tok.as_str() {
            "player" => (Some(n), format!("click:{PLAYER_BUTTON_ID}")),
            "corp" => (Some(n + 1), format!("click:{CORP_BUTTON_ID}")),
            "x" => (None, "close".to_string()),
            t if t.starts_with("btn:") => {
                let k = t[4..].parse::<usize>().unwrap() % n;
                (Some(k), format!("click:{}", BUTTON_IDS[k]))
            }
            _ => (Some(n + 2), format!("click:{CLOSE_BUTTON_ID}")),
        };
        self.actions.push(act);
        k
    }
}

/// The original's action menus: (title, button texts in the order of the port's options).
fn orig_menus(o: &Original) -> Vec<(String, Vec<String>)> {
    o.dialogs
        .iter()
        .filter(|d| d.control(PLAYER_BUTTON_ID).is_some() && d.control(CORP_BUTTON_ID).is_some())
        .map(|d| {
            let mut texts = Vec::new();
            for id in BUTTON_IDS.iter().chain([PLAYER_BUTTON_ID, CORP_BUTTON_ID, CLOSE_BUTTON_ID].iter()) {
                if let Some(c) = d.control(*id) {
                    texts.push(c.text.to_display());
                }
            }
            (d.title.to_display(), texts)
        })
        .collect()
}

fn dedup(v: &[(String, Vec<String>)]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for x in v {
        if out.last() != Some(x) {
            out.push(x.clone());
        }
    }
    out
}

fn cmpr() -> Compare {
    // EXT fields use f64 in the port; subtraction at a credit-limit boundary leaves sub-picounit
    // differences from the original's 80-bit intermediates.
    cmp().abs_tol(Ext::from_f64(1e-10)).ignore("hDlgTransMenu").ignore("TickerPauseUntil")
}

fn rng_of(seed: u64) -> u32 {
    cast::<u32, _>(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 20)
}

/// The shared synthetic world uses display text for bonds. A real ticker can convert them, which
/// requires the game's `YYYYcv|conversion market cap|@coupon` record rather than a missing price.
fn valid_convertible_terms(g: &mut Globals) {
    let __content = wsr_core::content::current();
    for issuer in 11..=1600 {
        let text = g.bond_issue_info[issuer].to_display();
        if g.bonds_issued[issuer] > 0 && text.contains("cv") && !text.contains('|') {
            let maturity = &text[..4];
            let coupon = text.split_whitespace().last().unwrap();
            let conversion_cap = (g.market_cap[issuer] * Ext::from_f64(1.2)).max(Ext::from_f64(1.0));
            g.bond_issue_info[issuer] = st(&format!("{maturity}{__wsr_text_13555}{conversion_cap:.2}|@{coupon}",__wsr_text_13555=__content.string(13555)));
        }
    }
}

/// A world with a selected / acting entity of a random kind and random menu/ticker state.
fn setup(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed ^ 0x5eed_ac71);
    let cp = g.cur_player;
    let kind = r.range(0, 9);
    let pick = |r: &mut Rand, lo: i64, hi: i64| cast::<i32, _>(r.range(lo, hi));
    let ent = match kind {
        0 => cp,
        1 => pick(&mut r, 11, 85),
        2 => pick(&mut r, 86, 130),
        3 => 1600,
        4 => pick(&mut r, 1581, 1585),
        _ => pick(&mut r, 131, 1580),
    };
    if (131..=1580).contains(&ent) && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        g.industry_num[ent] = [2, 37, 70, 69, 3][cast::<usize, _>(r.range(0, 4))];
    }
    if ent > 1580 {
        g.industry_num[ent] = 71;
    }
    g.sel_entity = ent;
    g.sel_corp_name = g.ent_name[ent].clone();
    g.sel_industry = if ent > 10 { cast::<i32, _>(g.industry_num[ent]) } else { cast::<i32, _>(r.range(0, 71)) };
    if ent > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        g.corp_controller[ent] = cp;
        g.entity_status[ent] = 1;
    }
    g.active_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { ent };
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        let other = match r.range(0, 3) {
            0 => pick(&mut r, 11, 85),
            1 => pick(&mut r, 86, 130),
            _ => pick(&mut r, 131, 1600),
        };
        g.active_entity = other;
    }
    g.active_entity_name = g.ent_name[g.active_entity].clone();
    g.tmp_ext1 = match r.range(0, 20) {
        0 => Ext::from_f64(0.0),
        1 => Ext::from_f64(8.0),
        2 => Ext::from_f64(3.5),
        _ => cast::<Ext, _>(r.range(1, 7)),
    };
    g.turn_action_count = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { cast::<i32, _>(r.range(5, 7)) } else { cast::<i32, _>(r.range(0, 4)) };
    g.action_code = PbStr::from(["", "", "AUTO", "SUPP"][cast::<usize, _>(r.range(0, 3))]);
    g.clock_state = PbStr::from(["", "RUN", "STOP", ""][cast::<usize, _>(r.range(0, 3))]);
    g.ticker_stop_req = PbStr::from(if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { "N" } else { "Y" });
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.sim_co_count = cast::<i32, _>(r.range(1585, 1600));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
        g.game_end_flag = st("Y");
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) && g.player_ceo_of[cp] == 0 {
        g.player_ceo_of[cp] = pick(&mut r, 131, 1580);
    }
    let ae = g.active_entity;
    if ae > 10 && g.industry_num[ae] == 1 && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
        g.cash[ae] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
        g.bank_debt[ae] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        // a short position of the current player
        let slot = 15;
        g.hold_stock_id[(slot, cp)] = cast::<i16, _>(r.range(131, 1580));
        g.hold_stock_pct[(slot, cp)] = -(cast::<i16, _>(r.range(1, 10)));
        g.hold_stock_id[(0, cp)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, cp)] != 0).count());
    }
    g
}

/// Runs `port` and the original at `addr` (args) with the answer tokens; compares state, RND, message
/// boxes, the skip trace and (when `menus`) the menus shown.
#[allow(clippy::too_many_arguments)]
fn check(
    seed: u64,
    addr: u32,
    args: &[&str],
    g: &Globals,
    tokens: &[&str],
    skips: &[(u32, u32)],
    menus: bool,
    port: impl Fn(&mut Globals, &mut MUi, &mut Rng, &Env),
) -> Option<(MUi, Globals, Original)> {
    let __content = wsr_core::content::current();
    let rng0 = rng_of(seed);
    let mut pg = g.clone();
    let mut rng = Rng::from_state(rng0);
    let mut ui = MUi { q: tokens.iter().map(|s| s.to_string()).collect(), ..MUi::default() };
    let sk = skip_addrs(skips);
    let env = Env::skipping(&NOW, &sk);
    port(&mut pg, &mut ui, &mut rng, &env);
    let acts: Vec<&str> = ui.actions.iter().map(|s| s.as_str()).collect();
    let o = run(addr, args, g, rng0, &acts, skips)?;
    if std::env::var("WSR_LOG").is_ok() {
        eprintln!("seed {seed} tokens {tokens:?}{__wsr_text_14120}{acts:?}\nport menus {:?}", ui.menus,__wsr_text_14120=__content.string(14120));
    }
    let d = cmpr().diffs(&pg, &o.globals);
    assert!(d.is_empty(), "seed {seed}: {} differences:\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
    assert_eq!(rng.state(), o.rng, "rng seed {seed}");
    assert_eq!(ui.msgs, orig_msgs(&o), "msgs seed {seed}");
    assert_same_skips(seed, &env.skipped(), &o);
    if menus {
        assert_eq!(dedup(&ui.menus), dedup(&orig_menus(&o)), "menus seed {seed}");
    }
    Some((ui, pg, o))
}

#[derive(Default)]
struct Hits(Mutex<Vec<String>>);
impl Hits {
    fn add(&self, s: impl Into<String>) {
        self.0.lock().unwrap().push(s.into());
    }
    fn assert(&self, want: &[&str]) {
        if unavailable().is_some() || std::env::var("WSR_ONLY_SEED").is_ok() {
            return;
        }
        let h = self.0.lock().unwrap();
        let missing: Vec<_> = want.iter().filter(|w| !h.iter().any(|x| x.contains(**w))).collect();
        assert!(missing.is_empty(), "branches not hit: {missing:?}");
    }
}

#[test]
fn diff_show_action_menu() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..300, |seed| {
        let g = setup(seed);
        let mut r = Rand(seed ^ 0x70c3);
        let n = r.range(0, 3);
        let toks: Vec<String> = (0..n)
            .map(|_| match r.range(0, 9) {
                0 => "player".to_string(),
                1 => "corp".to_string(),
                2 => "x".to_string(),
                _ => format!("btn:{}", r.range(0, 14)),
            })
            .collect();
        let toks: Vec<&str> = toks.iter().map(|s| s.as_str()).collect();
        let Some((ui, pg, o)) =
            check(seed, SHOW_ACTION_MENU, &["v:0"], &g, &toks, SKIPS, true, |g, ui, rng, env| show_action_menu(g, ui, rng, env))
        else {
            return;
        };
        hits.add(format!("mode{}", pg.action_menu_mode));
        for (t, opts) in &ui.menus {
            hits.add(t.clone());
            hits.extend(opts);
        }
        for l in &o.log {
            if l.starts_with("SKIP") {
                hits.add(l.clone());
            }
        }
        if g.cash[g.active_entity] != pg.cash[g.active_entity] {
            hits.add("bank repay");
        }
        if ui.menus.is_empty() {
            hits.add("no dialog");
        }
    });
    // Randomized worlds do not reliably select a player while opening entity research.
    // Exercise that menu explicitly so its final "My Corporations" button is covered.
    let mut player_menu = setup(5000);
    player_menu.sel_entity = player_menu.cur_player;
    player_menu.tmp_ext1 = Ext::from_f64(1.0);
    player_menu.turn_action_count = 0;
    if let Some((ui, _, _)) = check(5000, SHOW_ACTION_MENU, &["v:0"], &player_menu, &["close"], SKIPS, true,
        |g, ui, rng, env| show_action_menu(g, ui, rng, env)) {
        for (_, options) in &ui.menus { hits.extend(options); }
    }
    hits.assert(&[
        "mode1", "mode2", "mode3", "mode4", "mode5", "mode6", "mode7", "no dialog", "bank repay",
        __content.string(14121), __content.string(9923), __content.string(14122), __content.string(14123),
        __content.string(14124), __content.string(14125), __content.string(14126),
        __content.string(9920), __content.string(9922), __content.string(9935), __content.string(9946),
        __content.string(9954), __content.string(9968), __content.string(9962), __content.string(9963),
        __content.string(9991), __content.string(9992), __content.string(14127), __content.string(9999),
        __content.string(10007), __content.string(10004), __content.string(14128),
        "SKIP DoTransactionMenuAction", "SKIP SelectActingEntity", "SKIP TickerSimSteps", "SKIP UpdateMarketDisplay",
    ]);
}

trait Extend {
    fn extend(&self, v: &[String]);
}
impl Extend for Hits {
    fn extend(&self, v: &[String]) {
        self.0.lock().unwrap().extend(v.iter().cloned());
    }
}

/// The real TickerSimSteps behind the menu (Close / destroy steps) on a few seeds.
#[test]
fn diff_show_action_menu_with_ticker() {
    let skips: Vec<(u32, u32)> = SKIPS.iter().copied().filter(|s| s.0 != 0x41ef3f && s.0 != 0x4400d7).collect();
    par_seeds(0..24, |seed| {
        let mut g = setup(seed + 1000);
        g.tmp_ext1 = cast::<Ext, _>(1 + seed % 7);
        g.clock_state = st("");
        g.ticker_stop_req = st("N");
        g.turn_action_count = cast::<i32, _>(seed % 5);
        g.sim_co_count = 200 + cast::<i32, _>(seed) * 50;
        g.game_end_flag = st("N");
        // The random world gives dormant player slots bond holdings. The actual game never does;
        // a bond redemption would index PlayerCapGains(1..5) with 6..10 and abort the original tick.
        for h in g.num_players + 1..=10 {
            for slot in 0..=15 {
                g.hold_bond_id[(slot, h)] = 0;
                g.hold_bond_face[(slot, h)] = 0;
                g.bond_hold_cost[(slot, h)] = Ext::from_f64(0.0);
            }
        }
        check(seed, SHOW_ACTION_MENU, &["v:0"], &g, &[], &skips, true, |g, ui, rng, env| show_action_menu(g, ui, rng, env));
    });
}

#[test]
fn diff_action_menu_buttons() {
    const BTNS: [u32; 15] = [
        0x416b75, 0x416bc9, 0x416c1d, 0x416c71, 0x416cc5, 0x416d19, 0x416d6d, 0x416dc1, 0x416e15, 0x416e69, 0x416ebd,
        0x416f11, 0x416f65, 0x416fb9, 0x41700d,
    ];
    par_seeds(0..15, |seed| {
        let g = setup(seed);
        let k = cast::<usize, _>(seed);
        check(seed, BTNS[k], &["v:0", "v:273", "v:0", "v:0"], &g, &[], SKIPS, false, |g, ui, rng, env| {
            action_menu_btn(g, ui, rng, env, cast::<i32, _>(k) + 1)
        });
    });
}

#[test]
fn diff_action_menu_close_and_dlgproc() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..60, |seed| {
        let mut g = setup(seed);
        valid_convertible_terms(&mut g);
        let mut r = Rand(seed ^ 0xc105e);
        g.action_menu_open = cast::<i32, _>(r.range(-1, 2));
        g.dlg_open_db_search = cast::<i32, _>(r.range(0, 1));
        let skips: Vec<(u32, u32)> = if seed % 3 == 0 {
            // Nested reports inside real ticker steps must run their market updates on both sides.
            SKIPS.iter().copied().filter(|s| !matches!(s.0, 0x41ef3f | 0x4400d7)).collect()
        } else {
            SKIPS.to_vec()
        };
        match seed % 4 {
            0 | 1 => {
                check(seed, 0x417061, &["v:0", "v:273", "v:0", "v:0"], &g, &[], &skips, false, |g, ui, rng, env| {
                    action_menu_close(g, ui, rng, env)
                });
                hits.add("close");
            }
            2 => {
                // WM_DESTROY of an open menu
                check(seed, 0x41628e, &["v:0", "v:2", "v:0", "v:0"], &g, &[], &skips, false, |g, ui, rng, env| {
                    dlg_proc_action_menu_destroy(g, ui, rng, env)
                });
                hits.add("destroy");
            }
            _ => {
                let msg = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 16 } else { 83 };
                let m = format!("v:{msg}");
                check(seed, 0x41628e, &["v:0", &m, "v:0", "v:0"], &g, &[], &skips, false, |g, ui, rng, env| {
                    if msg == 16 {
                        dlg_proc_action_menu_close(g, ui, rng, env)
                    } else {
                        dlg_proc_action_menu_help(g, ui, rng, env)
                    }
                });
                hits.add(format!("{__wsr_text_14129}{msg}",__wsr_text_14129=__content.string(14129)));
            }
        }
    });
    hits.assert(&["close", "destroy", "msg16", "msg83"]);
}

#[test]
fn diff_action_menu_player_and_corp_buttons() {
    par_seeds(0..40, |seed| {
        let mut g = setup(seed + 500);
        let mut r = Rand(seed ^ 0xb7);
        g.action_menu_mode = cast::<i32, _>(r.range(1, 7));
        g.turn_action_count = cast::<i32, _>(r.range(0, 4));
        let mut skips = SKIPS.to_vec();
        if seed % 2 == 0 {
            skips.push((0x4131ea, 1)); // ShowActionMenu
        }
        if seed % 4 < 2 {
            check(seed, 0x41715d, &["v:0", "v:273", "v:0", "v:0"], &g, &[], &skips, true, |g, ui, rng, env| {
                action_menu_player_btn(g, ui, rng, env)
            });
        } else {
            check(seed, 0x4171d7, &["v:0", "v:273", "v:0", "v:0"], &g, &[], &skips, true, |g, ui, rng, env| {
                action_menu_select_corp_btn(g, ui, rng, env)
            });
        }
    });
}
