//! Differential tests of logic::corp_finance::bonds: IssueBondsMenu (0x5a40a2, with its GOSUBs
//! IssueBondsMenu_RateQuote / _PlaceWithInstitution), BondBuybackMenu (0x5a0aa1), PickConvertiblePrice
//! (0x688014), StockSplit (0x5bc3ff) and MenuCmd_PlaceBondsWithBank (0x556858).
//!
//! The player's answers are chosen by an adaptive Ui on the port's run (random per prompt kind) and
//! recorded; the same answer list then drives the original (one queue for MSGBOX and INPUTBOX$, as in
//! the harness). Message box texts are compared too. GetSelectedPlayerEtf and BorrowToCoverPurchase run
//! on both sides. `TIME$` is fixed by the harness, so `time_secs = 0`.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::BTreeSet;
use std::sync::Mutex;
use wsr_core::logic::corp_finance::bonds;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const SKIPS: &[(u32, u32)] = &[];

/// Answers prompts at random (by title) and records the answers for the original.
struct AUi {
    r: Rand,
    answers: Vec<String>,
    /// (title, text) of every MSGBOX (not the quick-message pop-ups).
    boxes: Vec<(String, String)>,
    quick: usize,
    /// Titles of the INPUTBOX$ prompts.
    inputs: Vec<String>,
    /// Share price (for conversion price answers) and a typical amount scale.
    price: Ext,
    scale: Ext,
}

impl AUi {
    fn new(seed: u64, price: Ext, scale: Ext) -> Self {
        AUi { r: Rand(seed.wrapping_mul(0x5851_f42d_4c95_7f2d) | 1), answers: vec![], boxes: vec![], quick: 0, inputs: vec![], price, scale }
    }
    fn amount(&mut self, scale: Ext) -> String {
        let v = match self.r.range(0, 9) {
            0 => Ext::from_f64(0.0),
            1 => cast::<Ext, _>(self.r.range(1, 60)),
            2..=6 => scale * cast::<Ext, _>(self.r.range(5, 100)) / Ext::from_f64(100.0),
            _ => scale * cast::<Ext, _>(self.r.range(100, 400)) / Ext::from_f64(100.0),
        };
        if self.r.chance(Ext::from_f64(0.2)) { format!("${v:.0}") } else { format!("{v:.1}") }
    }
}

impl Ui for AUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        if style == 0 && title.is_empty() {
            self.quick += 1;
            return 1;
        }
        self.boxes.push((title.to_display(), text.to_display()));
        let (a, v) = if style & 0xf == 3 || style & 0xf == 4 {
            let x = self.r.range(0, 99);
            if x < 62 {
                ("IDYES", 6)
            } else if x < 92 || style & 0xf == 4 {
                ("IDNO", 7)
            } else {
                ("IDCANCEL", 2)
            }
        } else {
            ("IDOK", 1)
        };
        self.answers.push(a.to_string());
        v
    }
    fn inputbox(&mut self, _prompt: &PbStr, title: &PbStr, _default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        let t = title.to_display();
        self.inputs.push(t.clone());
        let a = if self.r.chance(Ext::from_f64(0.06)) {
            String::new()
        } else if t.contains(__content.string(13654)) || t.contains(__content.string(3072)) || t.contains(__content.string(13655)) {
            let sc = self.scale;
            self.amount(sc)
        } else if t.contains(__content.string(13656)) {
            self.r.range(1, 22).to_string()
        } else if t.contains(__content.string(13657)) {
            format!("{:.2}", self.price * (Ext::from_f64(1.05) + cast::<Ext, _>(self.r.range(0, 25)) / Ext::from_f64(100.0)))
        } else if t.contains(__content.string(13658)) {
            self.r.range(0, 120).to_string()
        } else if t.contains(__content.string(13659)) {
            ["2", "3", "5", "10", "1", "0.5", "12", "4"][cast::<usize, _>(self.r.range(0, 7))].to_string()
        } else {
            "1".to_string()
        };
        self.answers.push(a.clone());
        PbStr::from(a.as_str())
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
}

/// Python `repr` of a latin-1 string (the harness log format).
fn py_repr(s: &str) -> String {
    let __content = wsr_core::content::current();
    let q = if s.contains('\'') && !s.contains('"') { '"' } else { '\'' };
    let mut out = String::new();
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (cast::<u32, _>(c)) < 0x20 || (0x7f..=0xa0).contains(&(cast::<u32, _>(c))) || cast::<u32, _>(c) == 0xad => {
                out.push_str(&format!("{__wsr_text_13660}{:02x}", cast::<u32, _>(c),__wsr_text_13660=__content.string(13660)))
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// Compare state, RND, message boxes (texts in order) and quick-message count.
fn check(what: &str, port: &Globals, ui: &AUi, rng: u32, o: &Original) {
    // EXT values (Cash) are f64 in the port: a borrowed amount can differ in the 80-bit rounding
    cmp().ignore("ActionCode").abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng, o.rng, "{what}: RND state");
    let orig: Vec<&String> = o.log.iter().filter(|l| l.starts_with("MSGBOX")).collect();
    let quick = o.log.iter().filter(|l| l.contains("CreateProcessA(0x0, 'qm.exe")).count();
    assert_eq!(ui.boxes.len(), orig.len(), "{what}: MSGBOX count; original log:\n{}", orig.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
    for ((title, text), l) in ui.boxes.iter().zip(&orig) {
        let want = format!("MSGBOX {} {} type=", py_repr(title), py_repr(text));
        assert!(l.starts_with(&want), "{what}: MSGBOX differs:\n port: {want}\n orig: {l}");
    }
    assert_eq!(ui.quick, quick, "{what}: quick message count");
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

/// A world with the current player's company `id` as `ActiveEntity`.
fn base(seed: u64) -> (Globals, Rand, i32) {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed * 7919 + 3);
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = g.player_name[g.cur_player].clone();
    let id = cast::<i32, _>(match r.range(0, 9) {
        0 => r.range(11, 85),
        1 => r.range(86, 130),
        _ => r.range(131, 1580),
    });
    g.entity_status[id] = 1;
    g.corp_controller[id] = g.cur_player;
    g.active_entity = id;
    g.sel_entity = if r.chance(Ext::from_f64(0.8)) { id } else { cast::<i32, _>(r.range(131, 1580)) };
    g.active_entity_name = g.ent_name[id].clone();
    g.app_title = st(__content.string(13661));
    g.currency_suffix = if r.chance(Ext::from_f64(0.2)) { st(" Euro") } else { PbStr::new() };
    g.etf_no_bonds_warn_plr = 0;
    (g, r, id)
}

fn bond_info(r: &mut Rand, g: &Globals, id: i32, conv: bool) -> PbStr {
    let __content = wsr_core::content::current();
    let mat = g.cur_year + cast::<i32, _>(r.range(0, 20));
    let coupon = cast::<Ext, _>(r.range(200, 1400)) / Ext::from_f64(100.0);
    if conv {
        let cap = g.market_cap[id] * cast::<Ext, _>(r.range(50, 130)) / Ext::from_f64(100.0);
        st(&format!("{mat}{__wsr_text_13555}{cap:.2}|@{coupon:05.2}",__wsr_text_13555=__content.string(13555)))
    } else {
        st(&format!("{mat}@{coupon:05.2}"))
    }
}

/// Give `id` a bond issue held partly (or entirely) by other entities.
fn give_bonds(g: &mut Globals, r: &mut Rand, id: i32) {
    let conv = r.chance(Ext::from_f64(0.35));
    g.bonds_issued[id] = r.range(50, 2000);
    g.bond_issue_info[id] = bond_info(r, g, id, conv);
    g.bond_price[id] = Cur::from_f64(cast::<Ext, _>(r.range(3000, 13000)) / Ext::from_f64(100.0));
    // clear existing holdings of this issue
    for h in (1..=130).chain(1581..=1583) {
        for slot in 1..=15 {
            if g.hold_bond_id[(slot, h)] == id {
                g.hold_bond_id[(slot, h)] = 0;
                g.hold_bond_face[(slot, h)] = 0;
            }
        }
    }
    let all_held = r.chance(Ext::from_f64(0.4));
    let mut left = g.bonds_issued[id];
    let n = r.range(1, 4);
    for k in 0..n {
        let h = match r.range(0, 3) {
            0 => g.cur_player,
            _ => cast::<i32, _>(r.range(11, 130)),
        };
        if h == id || g.entity_status[h] != 1 {
            continue;
        }
        let Some(slot) = (1..=15).find(|&s| g.hold_bond_id[(s, h)] == 0) else { continue };
        let face = if k == n - 1 && all_held { left } else { r.range(1, left.max(1)) };
        if face <= 0 {
            break;
        }
        left -= face;
        g.hold_bond_id[(slot, h)] = id;
        g.hold_bond_face[(slot, h)] = face;
        g.bond_hold_cost[(slot, h)] = cast::<Ext, _>(face) * cast::<Ext, _>(r.range(70, 110)) / Ext::from_f64(100.0);
        g.hold_bond_id[(0, h)] = cast::<i32, _>((1..=15).filter(|&s| g.hold_bond_id[(s, h)] != 0).count());
        if r.chance(Ext::from_f64(0.3)) && h > 10 {
            g.corp_controller[h] = if r.chance(Ext::from_f64(0.5)) { g.cur_player } else { cast::<i32, _>(r.range(1, 5)) };
        }
    }
}

/// Run the port with an adaptive Ui, then the original with the recorded answers.
fn diff_run(
    what: &str,
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng0: u32,
    mut ui: AUi,
    f: impl FnOnce(&mut Globals, &mut AUi, &mut Rng),
) -> Option<(Globals, AUi)> {
    let mut port = g.clone();
    let mut rr = Rng::from_state(rng0);
    f(&mut port, &mut ui, &mut rr);
    let answers: Vec<&str> = ui.answers.iter().map(|s| s.as_str()).collect();
    let o = run(addr, args, g, rng0, &answers, SKIPS)?;
    assert!(!o.log.iter().any(|l| l.starts_with("PBERR")), "{what}: runtime error in the original: {:?}", o.log.iter().filter(|l| l.starts_with("PBERR")).collect::<Vec<_>>());
    check(what, &port, &ui, rr.state(), &o);
    Some((port, ui))
}

#[test]
fn stock_split_diff() {
    let __content = wsr_core::content::current();
    if let Some(why) = unavailable() {
        eprintln!("skipped: {why}");
        return;
    }
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(0..60, |seed| {
        let (mut g, mut r, id) = base(seed);
        if id < 11 {
            return;
        }
        let price = [Ext::from_f64(5.0), Ext::from_f64(15.0), Ext::from_f64(40.0), Ext::from_f64(90.0), Ext::from_f64(150.0), Ext::from_f64(0.8)][cast::<usize, _>(r.range(0, 5))] * cast::<Ext, _>(r.range(80, 120)) / Ext::from_f64(100.0);
        g.market_cap[id] = g.shares_outstanding[id] * price;
        if r.chance(Ext::from_f64(0.1)) {
            g.shares_outstanding[id] = Ext::from_f64(0.0);
            g.market_cap[id] = price;
        }
        let ty = cast::<i32, _>(r.range(1, 2));
        let ui = AUi::new(seed, price, Ext::from_f64(10.0));
        let arg = ty.to_string();
        let what = format!("StockSplit seed {seed}");
        if let Some((port, ui)) = diff_run(&what, 0x5bc3ff, &[&arg], &g, cast::<u32, _>(r.next()), ui, |p, u, _| bonds::stock_split(p, u, ty)) {
            let mut s = seen.lock().unwrap();
            if port.shares_outstanding[id] != g.shares_outstanding[id] {
                s.insert(format!("{__wsr_text_13662}{ty}",__wsr_text_13662=__content.string(13662)));
            }
            if ui.quick > 1 {
                s.insert("error".into());
            }
        }
    });
    let s = seen.into_inner().unwrap();
    eprintln!("{__wsr_text_13663}{s:?}",__wsr_text_13663=__content.string(13663));
    if !s.is_empty() {
        assert!(s.contains("split1") && s.contains("split2") && s.contains("error"), "{s:?}");
    }
}

#[test]
fn pick_convertible_price_diff() {
    let __content = wsr_core::content::current();
    if let Some(why) = unavailable() {
        eprintln!("skipped: {why}");
        return;
    }
    let issued = Mutex::new(0);
    par_seeds(0..50, |seed| {
        let (mut g, mut r, id) = base(seed);
        if id < 11 {
            return;
        }
        let price = cast::<Ext, _>(r.range(1000, 30000)) / Ext::from_f64(100.0);
        g.market_cap[id] = g.shares_outstanding[id] * price;
        g.bonds_issued[id] = 0;
        g.credit_rating_str = st(["AAA", "BB", "C", "A"][cast::<usize, _>(r.range(0, 3))]);
        g.tmp_ext1 = if r.chance(Ext::from_f64(0.4)) { cast::<Ext, _>(r.range(1, 500)) } else { Ext::from_f64(0.0) };
        g.tmp_ext2 = cast::<Ext, _>(r.range(0, 1));
        let amount = r.range(50, 3000);
        let rate = cast::<Ext, _>(r.range(200, 1500)) / Ext::from_f64(100.0) + if r.chance(Ext::from_f64(0.3)) { Ext::from_f64(0.05) } else { Ext::from_f64(0.0) };
        let mat = g.cur_year + cast::<i32, _>(r.range(3, 20));
        let args = [format!("{__wsr_text_13664}{amount}",__wsr_text_13664=__content.string(13664)), format!("{__wsr_text_13665}{rate}",__wsr_text_13665=__content.string(13665)), mat.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let ui = AUi::new(seed, price, Ext::from_f64(100.0));
        let what = format!("PickConvertiblePrice seed {seed}");
        let mut res = 0;
        let out = diff_run(&what, 0x688014, &args, &g, cast::<u32, _>(r.next()), ui, |p, u, _| {
            res = bonds::pick_convertible_price(p, u, amount, rate, mat)
        });
        if out.is_some() {
            *issued.lock().unwrap() += res;
        }
    });
    let n = issued.into_inner().unwrap();
    eprintln!("issued: {n}");
}

fn issue_world(seed: u64) -> (Globals, Rand, i32) {
    let (mut g, mut r, id) = base(seed);
    let price = cast::<Ext, _>(r.range(500, 12000)) / Ext::from_f64(100.0);
    g.market_cap[id] = g.shares_outstanding[id] * price;
    if r.chance(Ext::from_f64(0.75)) {
        g.bonds_issued[id] = 0;
        g.bond_issue_info[id] = PbStr::new();
        g.bond_price[id] = Cur(0);
    } else {
        give_bonds(&mut g, &mut r, id);
    }
    if r.chance(Ext::from_f64(0.9)) {
        g.earnings_yr4_ago[id] = cast::<Ext, _>(r.range(1, 50));
    }
    g.player_recent_bond_issues[g.cur_player] = cast::<i32, _>(if r.chance(Ext::from_f64(0.85)) { r.range(0, 6) } else { r.range(7, 9) });
    g.superfund_corp = if r.chance(Ext::from_f64(0.05)) { cast::<i16, _>(id) } else { 0 };
    g.asbestos_corp = if r.chance(Ext::from_f64(0.05)) { -id } else { 0 };
    if r.chance(Ext::from_f64(0.05)) {
        g.base_interest_rate = cast::<f32, _>(Ext::from_f64(13.0));
    }
    g.cash[id] = cast::<Ext, _>(r.range(0, 3000));
    g.business_assets[id] = cast::<Ext, _>(r.range(100, 8000));
    g.bank_debt[id] = if r.chance(Ext::from_f64(0.6)) { cast::<Ext, _>(r.range(10, 1500)) } else { Ext::from_f64(0.0) };
    g.rand_seed = cast::<i32, _>(r.range(0, 32000));
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    (g, r, id)
}

#[test]
fn issue_bonds_menu_diff() {
    let __content = wsr_core::content::current();
    if let Some(why) = unavailable() {
        eprintln!("skipped: {why}");
        return;
    }
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(0..160, |seed| {
        let (g, mut r, id) = issue_world(seed);
        let price = g.market_cap[id] / g.shares_outstanding[id];
        let ui = AUi::new(seed, price, g.net_worth.max(Ext::from_f64(500.0)));
        let what = format!("IssueBondsMenu seed {seed}");
        if let Some((port, ui)) = diff_run(&what, 0x5a40a2, &[], &g, cast::<u32, _>(r.next()), ui, |p, u, rr| bonds::issue_bonds_menu(p, u, rr, 0)) {
            let mut s = seen.lock().unwrap();
            if port.bonds_issued[id] > 0 && g.bonds_issued[id] == 0 {
                s.insert(if port.bond_issue_info[id].to_display().contains("cv") { __content.string(8232) } else { __content.string(13666) });
            }
            if port.bank_debt[id] < g.bank_debt[id] {
                s.insert("paydown");
            }
            if ui.boxes.iter().any(|(t, _)| t.contains(__content.string(3057))) {
                s.insert(__content.string(13667));
            }
            if ui.boxes.iter().any(|(t, _)| t.contains(__content.string(13668))) {
                s.insert(__content.string(13669));
            }
            if ui.boxes.iter().any(|(t, _)| t.contains("Reverse")) {
                s.insert("reverse");
            }
            if g.bonds_issued[id] > 0 {
                s.insert("had bonds");
            }
            if (1..=130).any(|h| (0..=15).any(|k| port.hold_bond_id[(k, h)] == id && g.hold_bond_id[(k, h)] != id)) {
                s.insert("placed");
            }
        }
    });
    let s = seen.into_inner().unwrap();
    eprintln!("{__wsr_text_13663}{s:?}",__wsr_text_13663=__content.string(13663));
    if !s.is_empty() {
        for b in [__content.string(13666), __content.string(8232), "paydown", __content.string(13667), __content.string(13669), "had bonds", "placed"] {
            assert!(s.contains(b), "branch {b} not hit: {s:?}");
        }
    }
}

#[test]
fn bond_buyback_menu_diff() {
    let __content = wsr_core::content::current();
    if let Some(why) = unavailable() {
        eprintln!("skipped: {why}");
        return;
    }
    let seen = Mutex::new(BTreeSet::new());
    par_seeds(0..160, |seed| {
        let (mut g, mut r, id) = issue_world(seed);
        if r.chance(Ext::from_f64(0.9)) {
            give_bonds(&mut g, &mut r, id);
        }
        if r.chance(Ext::from_f64(0.4)) {
            g.bank_debt[id] = Ext::from_f64(0.0);
        }
        let must = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
        let price = g.market_cap[id] / g.shares_outstanding[id];
        let ui = AUi::new(seed, price, cast::<Ext, _>(g.bonds_issued[id].max(50)) * Ext::from_f64(0.5));
        let what = format!("BondBuybackMenu seed {seed}");
        let arg = must.to_string();
        if let Some((port, ui)) =
            diff_run(&what, 0x5a0aa1, &[&arg], &g, cast::<u32, _>(r.next()), ui, |p, u, rr| bonds::bond_buyback_menu(p, u, rr, must, 0))
        {
            let mut s = seen.lock().unwrap();
            if port.bonds_issued[id] < g.bonds_issued[id] {
                s.insert("retired");
            }
            if port.shares_outstanding[id] > g.shares_outstanding[id] {
                s.insert(__content.string(13670));
            }
            if port.bonds_issued[id] < g.bonds_issued[id] {
                if ui.inputs.iter().any(|t| t.starts_with(__content.string(13658))) {
                    s.insert("called");
                } else if !ui.boxes.iter().any(|(t, _)| t == __content.string(3266)) {
                    s.insert("public buyback");
                }
            }
            for (t, _) in &ui.boxes {
                if t.contains(__content.string(13671)) || t.contains(__content.string(13672)) || t.contains(__content.string(13673)) || t.contains(__content.string(13674)) {
                    s.insert("refused");
                }
                if t == __content.string(3266) {
                    s.insert("from holder");
                }
            }
        }
    });
    let s = seen.into_inner().unwrap();
    eprintln!("{__wsr_text_13663}{s:?}",__wsr_text_13663=__content.string(13663));
    if !s.is_empty() {
        for b in ["retired", "refused", "from holder", "called", "public buyback", __content.string(13670)] {
            assert!(s.contains(b), "branch {b} not hit: {s:?}");
        }
    }
}

#[test]
fn menu_cmd_place_bonds_with_bank_diff() {
    if let Some(why) = unavailable() {
        eprintln!("skipped: {why}");
        return;
    }
    let placed = Mutex::new(0);
    par_seeds(0..60, |seed| {
        let (mut g, mut r, id) = issue_world(seed);
        give_bonds(&mut g, &mut r, id);
        // banks/insurers that already hold the issue, controlled by companies
        for _ in 0..r.range(0, 6) {
            let h = cast::<i32, _>(r.range(11, 130));
            g.entity_status[h] = 1;
            g.corp_controller[h] = cast::<i32, _>(r.range(131, 1580));
            if let Some(slot) = (1..=15).find(|&s| g.hold_bond_id[(s, h)] == 0) {
                g.hold_bond_id[(slot, h)] = id;
                g.hold_bond_face[(slot, h)] = 1;
                g.bond_hold_cost[(slot, h)] = Ext::from_f64(1.0);
                g.hold_bond_id[(0, h)] = cast::<i32, _>((1..=15).filter(|&s| g.hold_bond_id[(s, h)] != 0).count());
            }
        }
        g.bonds_issued[id] += r.range(0, 500);
        g.player_recent_corp = cast::<i16, _>(id);
        let rng0 = cast::<u32, _>(r.next());
        // the exit path (no buyer: jump to ExecuteMenuCommand's epilogue) cannot run outside its caller
        let mut port = g.clone();
        let mut ui = AUi::new(seed, Ext::from_f64(1.0), Ext::from_f64(1.0));
        let mut rr = Rng::from_state(rng0);
        if bonds::menu_cmd_place_bonds_with_bank(&mut port, &mut ui, &mut rr) {
            return;
        }
        let what = format!("MenuCmd_PlaceBondsWithBank seed {seed}");
        let Some(o) = run(0x556858, &[], &g, rng0, &[], SKIPS) else { return };
        check(&what, &port, &ui, rr.state(), &o);
        if (11..=130).any(|h| (1..=15).any(|k| port.hold_bond_face[(k, h)] > g.hold_bond_face[(k, h)])) {
            *placed.lock().unwrap() += 1;
        }
    });
    eprintln!("placed: {}", placed.into_inner().unwrap());
}
