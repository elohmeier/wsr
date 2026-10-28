//! Differential tests of logic::corp_finance::stock: AdjustContingencyReserve, RestructuringWriteOff,
//! StockBuybackMenu, GreenmailBuyback, StockOfferingMenu, ExtraordinaryDividend and
//! CancelMergerOnViolation against the original routines.
use wsr_core::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use wsr_core::logic::cashflow::get_etf_adviser_human_status;
use wsr_core::logic::corp_finance::stock::*;
use wsr_core::logic::newgame::LocalTime;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const ADJUST_CONTINGENCY_RESERVE: u32 = 0x59fcdd;
const RESTRUCTURING_WRITE_OFF: u32 = 0x5aa186;
const STOCK_BUYBACK_MENU: u32 = 0x59e01f;
const GREENMAIL_BUYBACK: u32 = 0x521526;
const STOCK_OFFERING_MENU: u32 = 0x5b0f50;
const EXTRAORDINARY_DIVIDEND: u32 = 0x5b3dae;
const CANCEL_MERGER_ON_VIOLATION: u32 = 0x5b63a3;

/// LboTenderOffer (hooked: its calls are counted).
const LBO_TENDER: u32 = 0x51f21c;

/// A UI answering like the harness: MSGBOX and INPUTBOX$ pop the next answer of one shared queue
/// (`IDYES`/`IDNO`/`IDOK`/`IDCANCEL`/number for a box, the text for an input box; default YES/OK and
/// ""); ShowQuickMessage pop-ups (style 0, no title: qm.exe in the original) take no answer. `sym:TEXT`
/// answers the stock-symbol dialog (InBox mode 3, a dialog in the original: `text:310:TEXT`, `click:1`;
/// `sym:` alone = Cancel, `click:2`); message boxes don't take it.
#[derive(Default)]
struct Seq {
    answers: VecDeque<String>,
    msgs: Vec<String>,
    inputs: usize,
    /// The harness answers for the original, in call order (what each box / dialog of the port took).
    log: Vec<String>,
}

impl Seq {
    fn new(answers: &[&str]) -> Self {
        Seq { answers: answers.iter().map(|a| a.to_string()).collect(), ..Default::default() }
    }
}

impl Ui for Seq {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(format!("[{}] {}", title.to_display(), text.to_display()));
        let kind = style & 0xf;
        let default = if kind == 3 || kind == 4 { 6 } else { 1 };
        if style == 0 && title.is_empty() {
            return default;
        }
        let a = if self.answers.front().is_some_and(|a| a.starts_with("sym:")) {
            None
        } else {
            self.answers.pop_front()
        };
        let v = match a.as_deref() {
            None => default,
            Some("IDOK") => 1,
            Some("IDCANCEL") => 2,
            Some("IDYES") => 6,
            Some("IDNO") => 7,
            Some(n) => n.parse().unwrap_or(default),
        };
        self.log.push(v.to_string());
        v
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        if prompt.to_display().ends_with(__content.string(6717)) {
            // the symbol dialog (no INPUTBOX$ in the original): a `sym:` answer, else its default action
            // (OK with the prefilled text)
            return match self.answers.pop_front_if(|a| a.starts_with("sym:")) {
                Some(a) if a.len() == 4 => {
                    self.log.push("click:2".into());
                    PbStr::new()
                }
                Some(a) => {
                    self.log.push(format!("text:310:{}", &a[4..]));
                    self.log.push("click:1".into());
                    PbStr::from(&a[4..])
                }
                None => {
                    self.log.push("click:1".into());
                    default.clone()
                }
            };
        }
        self.inputs += 1;
        let a = self.answers.pop_front().unwrap_or_default();
        self.log.push(a.clone());
        PbStr::from(a.as_str())
    }
    fn report(&mut self, _title: &PbStr, _lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.log.push("click:1".into()); // OK, no pick
        ReportAction::Ok
    }
    fn news(&mut self, _headline: &PbStr) {}
}

/// Port result (globals, RND state, message boxes) vs. the original's; panics on differences.
#[track_caller]
fn check(what: &str, port: &Globals, port_rng: &Rng, ui: &Seq, o: &Original) {
    // the symbol dialog's window handle (UI only; the original keeps it after DIALOG END)
    let d = cmp().ignore("DlgSymbolEntry").diffs(port, &o.globals);
    let log: Vec<&String> = o.log.iter().filter(|l| !l.starts_with("API")).take(30).collect();
    assert!(
        d.is_empty(),
        "{what}: {} differences:\n{}\nlog: {log:?}\nport msgs: {:?}",
        d.len(),
        d.iter().take(30).cloned().collect::<Vec<_>>().join("\n"),
        ui.msgs
    );
    assert_eq!(port_rng.state(), o.rng, "{what}: RND state; log {log:?}; port msgs {:?}", ui.msgs);
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(errs.is_empty(), "{what}: runtime errors in the original: {errs:?}");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs log {log:?}", ui.msgs);
    let inputs = o.log.iter().filter(|l| l.starts_with("INPUTBOX")).count();
    assert_eq!(ui.inputs, inputs, "{what}: input boxes");
}

/// Runs the original with the harness answers the port's [`Seq`] logged.
fn run_with(addr: u32, args: &[String], g: &Globals, rs: u32, ui: &Seq, skips: &[(u32, u32)]) -> Option<Original> {
    let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let oa: Vec<&str> = ui.log.iter().map(|s| s.as_str()).collect();
    run_hooked(addr, &args, g, rs, &oa, skips, &[LBO_TENDER])
}

/// The original called LboTenderOffer.
fn lbo_called(o: &Original) -> bool {
    o.log.iter().any(|l| l.starts_with("CALL LboTenderOffer("))
}

const NOW: LocalTime = LocalTime::HARNESS;

/// The common world plus what the player menus read: unique symbols, an active company (`ae`), no
/// selected ETF, cheat mode off.
fn menu_world(seed: u64, r: &mut Rand) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    for e in 11..=1600 {
        g.ent_symbol[e] = st(&format!("S{e}"));
    }
    for p in 1..=5 {
        g.ent_name[p] = g.player_name[p].clone();
        g.ent_symbol[p] = st(&format!("P{p}"));
    }
    // holdings small enough that no company is more than 100% owned (PublicPct >= 0; a negative public
    // float would make the LBO prompt of StockBuybackMenu loop forever, in the original too)
    for h in 1..=1600 {
        for s in 1..=15 {
            let p = g.hold_stock_pct[(s, h)];
            if p > 0 {
                g.hold_stock_pct[(s, h)] = p % 12 + 1;
            }
        }
    }
    g.cheat_mode = st("N");
    g.app_title = st(__content.string(13661));
    g.sel_entity = 0;
    g.last_sel_entity = 0;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.is_computer_player[g.cur_player] = 0;
    g.cur_player_name = g.player_name[g.cur_player].clone();
    g.scenario_num = 0;
    g
}

fn set_active(g: &mut Globals, ae: i32) {
    g.active_entity = ae;
    g.active_entity_name = g.ent_name[ae].clone();
}

fn active_corp(g: &Globals, r: &mut Rand, lo: i64, hi: i64) -> i32 {
    for _ in 0..200 {
        let c = cast::<i32, _>(r.range(lo, hi));
        if g.entity_status[c] == 1 {
            return c;
        }
    }
    cast::<i32, _>(lo)
}

/// Gives `holder` a block of `corp` stock in a free slot (or the first slot).
fn give_holding(g: &mut Globals, holder: i32, corp: i32, pct: i16, basis: Ext) {
    let slot = (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0).unwrap_or(1);
    g.hold_stock_id[(slot, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(slot, holder)] = pct;
    g.hold_stock_basis[(slot, holder)] = basis;
    g.hold_stock_id[(0, holder)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, holder)] != 0).count());
}

static HITS: [AtomicUsize; 32] = [const { AtomicUsize::new(0) }; 32];

fn hit(k: usize) {
    HITS[k].fetch_add(1, Relaxed);
}

fn hits(k: usize) -> usize {
    HITS[k].load(Relaxed)
}

// ------------------------------------------------------------------------- AdjustContingencyReserve

#[test]
fn diff_adjust_contingency_reserve() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 7919 + 3);
        let mut g = menu_world(seed + 3000, &mut r);
        let ae = match seed % 4 {
            0 => active_corp(&g, &mut r, 11, 85),
            1 => active_corp(&g, &mut r, 86, 130),
            _ => active_corp(&g, &mut r, 131, 1580),
        };
        set_active(&mut g, ae);
        let mode = if seed % 2 == 0 { 6 } else { 7 };
        if r.chance(Ext::from_f64(0.3)) {
            g.contingency_reserve[ae] = Ext::from_f64(0.0);
        } else {
            g.contingency_reserve[ae] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(80.0));
        }
        g.proj_qtr_earnings[ae] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(60.0));
        if r.chance(Ext::from_f64(0.4)) {
            g.reserve_adj_pending[ae] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(10.0));
        }
        let answers: Vec<&str> = match seed % 6 {
            0 => vec!["IDOK", "1"],
            1 => vec!["IDOK", "99999", "0.5"],
            2 => vec!["IDOK", "0"],
            3 => vec!["IDOK", "1000", "2"],
            4 => vec!["IDOK", "0.01"],
            _ => vec!["IDOK", "-3"],
        };
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        adjust_contingency_reserve(&mut port, &mut ui, &mut rng, mode);
        let Some(o) = run_with(ADJUST_CONTINGENCY_RESERVE, &[mode.to_string()], &g, rs, &ui, &[]) else {
            return;
        };
        if port.reserve_adj_pending[ae] != Ext::from_f64(0.0) {
            hit(0);
        }
        check(&format!("AdjustContingencyReserve seed {seed}{__wsr_text_13548}{mode} ae {ae}",__wsr_text_13548=__content.string(13548)), &port, &rng, &ui, &o);
    });
    assert!(hits(0) > 0, "no adjustment stored");
}

// --------------------------------------------------------------------------- RestructuringWriteOff

#[test]
fn diff_restructuring_write_off() {
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 6007 + 1);
        let mut g = menu_world(seed + 3100, &mut r);
        let ae = if seed % 6 == 0 { active_corp(&g, &mut r, 11, 130) } else { active_corp(&g, &mut r, 131, 1580) };
        set_active(&mut g, ae);
        if seed % 6 != 1 {
            g.credit_rating_num[ae] = cast::<i32, _>(r.range(5, 10));
            if (131..=1580).contains(&ae) {
                g.industry_num[ae] = cast::<i16, _>(r.range(3, 69));
            }
        }
        if (131..=1600).contains(&ae) {
            g.business_assets[ae] = if seed % 6 == 2 { Ext::from_f64(150.0) } else { f64r(&mut r, Ext::from_f64(300.0), Ext::from_f64(6000.0)) };
            g.goodwill[ae] = if r.chance(Ext::from_f64(0.5)) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) } else { Ext::from_f64(0.0) };
        }
        g.profit_margin[ae] = cast::<i16, _>(r.range(-10, 29));
        if seed % 8 == 5 {
            g.event_player = cast::<i16, _>(g.cur_player);
            g.scenario_num = 40;
            g.event_status[40] = 1;
            g.scenario_stage = 3;
        }
        if seed % 5 == 3 {
            g.event_status[102] = 2;
        }
        let answers: Vec<&str> = match seed % 5 {
            0 => vec!["3"],
            1 => vec!["999", "10"],
            2 => vec!["0"],
            3 => vec!["50"],
            _ => vec!["25"],
        };
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        restructuring_write_off(&mut port, &mut ui, &mut rng, &NOW);
        let Some(o) = run_with(RESTRUCTURING_WRITE_OFF, &[], &g, rs, &ui, &[]) else {
            return;
        };
        if port.turn_action_count != g.turn_action_count {
            hit(1);
            if port.extraordinary_pl[ae] != g.extraordinary_pl[ae] {
                hit(2);
            }
        }
        check(&format!("RestructuringWriteOff seed {seed} ae {ae}"), &port, &rng, &ui, &o);
    });
    assert!(hits(1) > 0 && hits(2) > 0, "write-off branches not hit: {} {}", hits(1), hits(2));
}

// ------------------------------------------------------------------------------- StockBuybackMenu

#[test]
fn diff_stock_buyback_menu() {
    let __content = wsr_core::content::current();
    par_seeds((1..=120).filter(|s| std::env::var("SEED").map_or(true, |v| v == s.to_string())), |seed| {
        let mut r = Rand(seed * 5003 + 9);
        let mut g = menu_world(seed + 3200, &mut r);
        let ae = if seed % 5 == 0 { active_corp(&g, &mut r, 11, 130) } else { active_corp(&g, &mut r, 131, 1580) };
        set_active(&mut g, ae);
        if seed % 3 == 0 {
            // a private corporate holder
            let h = active_corp(&g, &mut r, 131, 1580);
            if h != ae {
                give_holding(&mut g, h, ae, cast::<i16, _>(r.range(5, 40)), Ext::from_f64(100.0));
            }
        }
        let mode = if seed % 2 == 0 { 1 } else { 2 };
        if seed % 4 == 1 {
            // a cash-rich, well-rated company: the buy-back can go through
            g.cash[ae] = g.market_cap[ae] * Ext::from_f64(40.0) + Ext::from_f64(1000.0);
            g.credit_rating_num[ae] = 9;
        }
        if seed % 3 != 0 {
            // bonds outstanding in the game's formats (convertible: `YYYYcv|cap|@cc.cc`), repriced after the
            // buy-back
            let mat = g.cur_year + cast::<i32, _>(r.range(1, 12));
            let coupon = cast::<Ext, _>(r.range(40, 200)) / Ext::from_f64(20.0);
            g.bonds_issued[ae] = r.range(100, 3000);
            g.bond_issue_info[ae] = if seed % 2 == 1 {
                st(&format!("{mat}{__wsr_text_13555}{:.2}|@{coupon:05.2}", g.market_cap[ae] * f64r(&mut r, Ext::from(Ext::from_f64(0.3)), Ext::from(Ext::from_f64(1.5))),__wsr_text_13555=__content.string(13555)))
            } else {
                st(&format!("{mat}@{coupon:05.2}"))
            };
        }
        // The original's report dialog pauses the ticker (PauseTicker sets TickerPauseUntil, UI state the
        // port doesn't model), so GreenmailBuyback's TickerSimSteps(1) returns at once there: pause it here.
        g.ticker_pause_until = 1_000_000_000;
        let mut answers: Vec<&str> = match seed % 4 {
            0 => vec!["IDOK", "5"],
            1 => vec!["150", "IDOK", "3"],
            2 => vec!["0"],
            _ => vec!["10"],
        };
        // greenmail (mode 1): the shareholder's symbol in the symbol dialog (a corporate holder of the
        // company's stock, or Cancel)
        let holder = (131..=1580).find(|&h| h != ae && g.entity_status[h] == 1 && (1..=15).any(|s| cast::<i32, _>(g.hold_stock_id[(s, h)]) == ae));
        let sym = match holder {
            Some(h) if seed % 6 != 4 => format!("sym:{}", g.ent_symbol[h].to_display()),
            _ => "sym:".to_string(),
        };
        if mode == 1 {
            answers.insert(0, &sym);
        }
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        stock_buyback_menu(&mut port, &mut ui, &mut rng, &NOW, mode);
        let Some(o) = run_with(STOCK_BUYBACK_MENU, &[mode.to_string()], &g, rs, &ui, &[]) else {
            return;
        };
        if ui.inputs > 0 {
            hit(3);
        }
        if lbo_called(&o) {
            hit(12);
            if port.shares_outstanding[ae] != g.shares_outstanding[ae] {
                hit(15);
            }
        }
        check(&format!("StockBuybackMenu seed {seed}{__wsr_text_13548}{mode} ae {ae}",__wsr_text_13548=__content.string(13548)), &port, &rng, &ui, &o);
    });
    assert!(hits(3) > 0, "LBO percentage never asked");
    assert!(hits(12) > 0, "LboTenderOffer never called");
    assert!(hits(15) > 0, "no buy-back completed by LboTenderOffer");
}

// ------------------------------------------------------------------------------- GreenmailBuyback

#[test]
fn diff_greenmail_buyback() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut r = Rand(seed * 4001 + 5);
        let mut g = menu_world(seed + 3300, &mut r);
        let ae = active_corp(&g, &mut r, 131, 1580);
        set_active(&mut g, ae);
        let seller = match seed % 8 {
            0 => g.cur_player,
            1 => ae,
            _ => active_corp(&g, &mut r, 11, 1580),
        };
        if seller > 10 && seller != ae && seed % 8 != 2 {
            give_holding(&mut g, seller, ae, cast::<i16, _>(r.range(1, 40)), Ext::from_f64(50.0));
            if seed % 4 == 3 {
                // controlled by a computer-controlled company
                g.corp_controller[seller] = active_corp(&g, &mut r, 131, 1580);
            }
        }
        let answers: Vec<&str> = match seed % 3 {
            0 => vec!["5"],
            1 => vec!["999", "IDOK", "2"],
            _ => vec!["0"],
        };
        // the BYREF parameter is TargetCorp at the game's only call site (StockBuybackMenu)
        g.target_corp = seller;
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        greenmail_buyback(&mut port, &mut ui, &mut rng, &NOW);
        let Some(o) = run_with(GREENMAIL_BUYBACK, &[seller.to_string()], &g, rs, &ui, &[]) else {
            return;
        };
        if ui.inputs > 0 {
            hit(4);
        }
        if lbo_called(&o) {
            hit(13);
        }
        check(&format!("GreenmailBuyback seed {seed}{__wsr_text_13736}{seller} ae {ae}",__wsr_text_13736=__content.string(13736)), &port, &rng, &ui, &o);
    });
    assert!(hits(4) > 0, "greenmail percentage never asked");
    assert!(hits(13) > 0, "greenmail LboTenderOffer never called");
}

// ------------------------------------------------------------------------------ StockOfferingMenu

/// GetSelectedPlayerEtf (0x652b17) as the original computes it.
fn selected_player_etf(g: &mut Globals) -> i32 {
    let mut ui = Seq::default();
    let mut rng = Rng::from_state(1);
    let sel = g.sel_entity;
    if get_etf_adviser_human_status(g, &mut ui, &mut rng, sel) == 2 {
        return sel;
    }
    let last = g.last_sel_entity;
    if get_etf_adviser_human_status(g, &mut ui, &mut rng, last) == 2 {
        return last;
    }
    0
}

/// Makes fund 1581 an active ETF advised by a company the current player controls, and selects it.
fn select_player_etf(g: &mut Globals, r: &mut Rand) {
    let etf = 1581;
    let adviser = active_corp(g, r, 131, 1580);
    g.entity_status[etf] = 1;
    g.industry_num[etf] = 71;
    g.ctrl_since_year[etf] = g.cur_year - 2;
    g.fund_adviser[etf] = adviser;
    g.industry_num[adviser] = 37;
    g.corp_controller[adviser] = g.cur_player;
    // GetControllerType = 1 needs the player to hold the adviser's stock
    give_holding(g, g.cur_player, adviser, 60, Ext::from_f64(100.0));
    g.sel_entity = etf;
}

/// Answers for StockOfferingMenu: `shares` = a share count in the allowed range (5..25% of the
/// outstanding shares, in millions x 100 like the original's prompt).
fn offering_answers(seed: u64, offering_type: i32, so: Ext, buyer_sym: &str) -> Vec<String> {
    let shares = |frac: Ext| format!("{:.0}", (so * Ext::from_f64(100.0) * frac).round());
    let v: Vec<String> = if offering_type == 1 {
        match seed % 4 {
            0 => vec![shares(Ext::from(Ext::from_f64(0.5))), shares(Ext::from(Ext::from_f64(0.12)))],
            1 => vec![shares(Ext::from(Ext::from_f64(0.01))), shares(Ext::from(Ext::from_f64(0.2)))],
            2 => vec!["0".into()],
            _ => vec![shares(Ext::from(Ext::from_f64(0.07)))],
        }
    } else {
        // YES, then NO: a company the player controls (its symbol is asked)
        let sym = format!("sym:{buyer_sym}");
        match seed % 5 {
            0 => vec!["IDNO".into(), sym, shares(Ext::from(Ext::from_f64(0.1)))],
            1 => vec!["IDYES".into(), "IDYES".into(), shares(Ext::from(Ext::from_f64(0.2)))],
            2 => vec!["IDYES".into(), "IDNO".into(), sym.clone(), shares(Ext::from(Ext::from_f64(0.06)))],
            3 => vec!["IDCANCEL".into()],
            _ => vec!["IDNO".into(), if seed.is_multiple_of(3) { "sym:".into() } else { sym }, shares(Ext::from(Ext::from_f64(0.24)))],
        }
    };
    v
}

#[test]
fn diff_stock_offering_menu() {
    let __content = wsr_core::content::current();
    par_seeds(1..=40, |seed| {
        let mut r = Rand(seed * 3001 + 7);
        let mut g = menu_world(seed + 3400, &mut r);
        let ae = if seed % 10 == 0 { active_corp(&g, &mut r, 11, 130) } else { active_corp(&g, &mut r, 131, 1580) };
        set_active(&mut g, ae);
        let offering_type = if seed % 2 == 0 { 1 } else { 2 };
        g.recent_offerings.as_mut_slice().iter_mut().for_each(|x| *x = 0);
        if seed % 7 == 0 {
            g.recent_offerings[3] = if r.chance(Ext::from_f64(0.5)) { ae } else { -ae };
        }
        if seed % 11 == 4 {
            g.asbestos_corp = -ae;
        }
        if seed % 9 == 2 {
            g.earnings_yr3_ago[ae] = Ext::from_f64(0.0);
            g.earnings_yr4_ago[ae] = Ext::from_f64(0.0);
        }
        if seed % 6 == 5 {
            g.fad_industry = cast::<i32, _>(g.industry_num[ae]);
            g.fad_valuation_pct = 151;
        }
        g.max_stock_offering_amt = if seed % 8 == 6 { 1000 } else { 1_000_000_000 };
        // a related buyer for the private placement
        let buyer = active_corp(&g, &mut r, 131, 1580);
        g.corp_controller[buyer] = g.cur_player;
        let picked = if seed % 13 == 6 { cast::<i32, _>(r.range(11, 85)) } else { buyer };
        let sym = g.ent_symbol[picked].to_display();
        let answers = offering_answers(seed, offering_type, g.shares_outstanding[ae], &sym);
        let answers: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        stock_offering_menu(&mut port, &mut ui, &mut rng, &NOW, offering_type);
        let Some(o) = run_with(STOCK_OFFERING_MENU, &[offering_type.to_string()], &g, rs, &ui, &[]) else {
            return;
        };
        if port.recent_offerings.as_slice() != g.recent_offerings.as_slice() {
            hit(14);
        }
        if port.turn_action_count != g.turn_action_count {
            hit(5 + cast::<usize, _>(offering_type));
        }
        check(&format!("StockOfferingMenu seed {seed}{__wsr_text_13737}{offering_type} ae {ae}",__wsr_text_13737=__content.string(13737)), &port, &rng, &ui, &o);
    });
    assert!(hits(6) > 0 && hits(7) > 0, "offerings done: public {}{__wsr_text_13738}{}", hits(6), hits(7),__wsr_text_13738=__content.string(13738));
    assert!(hits(14) > 0, "AddRecentOffering never reached");
}

#[test]
fn diff_stock_offering_menu_etf() {
    let __content = wsr_core::content::current();
    par_seeds(1..=18, |seed| {
        let mut r = Rand(seed * 2003 + 1);
        let mut g = menu_world(seed + 3500, &mut r);
        let ae = if seed % 3 == 0 { g.cur_player } else { active_corp(&g, &mut r, 131, 1580) };
        set_active(&mut g, ae);
        select_player_etf(&mut g, &mut r);
        g.recent_offerings.as_mut_slice().iter_mut().for_each(|x| *x = 0);
        g.max_stock_offering_amt = 1_000_000_000;
        // nobody holds a large block of the fund (else it loses its ETF status in UpdateCorpControl)
        for h in 1..=1600 {
            for s in 1..=15 {
                if g.hold_stock_id[(s, h)] == 1581 {
                    g.hold_stock_pct[(s, h)] = 2;
                }
            }
        }
        if seed % 4 != 1 {
            g.cur_quarter = 4;
            g.sim_co_count = 1200;
        }
        // 11-month gain of the fund: rising price history
        let cur = cast::<i32, _>(g.cap_hist_close[(0, 1581)]);
        for k in 0..60 {
            let mut slot = cur - k;
            if slot < 1 {
                slot += 60;
            }
            let v = if seed % 5 == 2 { Ext::from_f64(100.0) } else { Ext::from_f64(100.0) * (Ext::from_f64(1.0) - Ext::from_f64(0.02) * cast::<Ext, _>(k)) };
            g.cap_hist_close[(slot, 1581)] = v;
        }
        let offering_type = if seed % 6 == 5 { 2 } else { 1 };
        let shares = format!("{:.0}", (g.shares_outstanding[1581] * Ext::from_f64(100.0) * Ext::from_f64(0.1)).round());
        let answers: Vec<&str> = match seed % 3 {
            0 => vec!["IDYES", &shares],
            1 => vec!["IDNO", "0"],
            _ => vec!["IDCANCEL"],
        };
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        assert_eq!(selected_player_etf(&mut port.clone()), 1581);
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        stock_offering_menu(&mut port, &mut ui, &mut rng, &NOW, offering_type);
        let Some(o) = run_with(STOCK_OFFERING_MENU, &[offering_type.to_string()], &g, rs, &ui, &[]) else {
            return;
        };
        if port.shares_outstanding[1581] != g.shares_outstanding[1581] {
            hit(8);
        }
        check(&format!("StockOfferingMenu(ETF) seed {seed}{__wsr_text_13737}{offering_type} ae {ae}",__wsr_text_13737=__content.string(13737)), &port, &rng, &ui, &o);
    });
    assert!(hits(8) > 0, "no ETF offering");
}

// -------------------------------------------------------------------------- ExtraordinaryDividend

fn dividend_world(seed: u64, r: &mut Rand) -> (Globals, i32) {
    let mut g = menu_world(seed, r);
    let ae = match seed % 8 {
        0 => active_corp(&g, r, 11, 130),
        _ => active_corp(&g, r, 131, 1580),
    };
    set_active(&mut g, ae);
    if !seed.is_multiple_of(3) {
        g.credit_rating_num[ae] = cast::<i32, _>(r.range(4, 10));
        g.cash[ae] = f64r(r, Ext::from_f64(100.0), Ext::from_f64(3000.0));
    }
    // holders of the payer: players, companies (some 80% parents), short sellers
    for k in 0..r.range(1, 6) {
        let h = if k == 0 { g.cur_player } else { active_corp(&g, r, 11, 1580) };
        if h == ae || (1..=15).any(|s| cast::<i32, _>(g.hold_stock_id[(s, h)]) == ae) {
            continue;
        }
        let pct = [3, 15, 25, 85, -5][cast::<usize, _>(r.range(0, 4))];
        give_holding(&mut g, h, ae, pct, f64r(r, Ext::from_f64(1.0), Ext::from_f64(200.0)));
    }
    if seed % 5 == 1 {
        g.merger_target = ae;
        g.merger_acquirer = active_corp(&g, r, 131, 1580);
    }
    if seed % 7 == 3 {
        g.last_extra_div_corp = ae;
    }
    (g, ae)
}

fn dividend_answers(seed: u64) -> Vec<&'static str> {
    match seed % 5 {
        0 => vec!["IDYES", "5", "IDYES", "IDYES"],
        1 => vec!["IDYES", "999999", "2", "IDYES", "IDYES"],
        2 => vec!["IDYES", "1", "IDNO"],
        3 => vec!["IDYES", "0"],
        _ => vec!["IDYES", "50", "IDYES", "IDYES"],
    }
}

#[test]
fn diff_extraordinary_dividend() {
    let __content = wsr_core::content::current();
    par_seeds(1..=40, |seed| {
        let mut r = Rand(seed * 1009 + 11);
        let (g, ae) = dividend_world(seed + 3600, &mut r);
        let answers = dividend_answers(seed);
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        extraordinary_dividend(&mut port, &mut ui, &mut rng, &NOW);
        let Some(o) = run_with(EXTRAORDINARY_DIVIDEND, &[], &g, rs, &ui, &[]) else {
            return;
        };
        if port.turn_action_count != g.turn_action_count {
            hit(9);
            if port.merger_target == 0 && g.merger_target != 0 {
                hit(10);
            }
        }
        check(&format!("ExtraordinaryDividend seed {seed} ae {ae}"), &port, &rng, &ui, &o);
    });
    assert!(hits(9) > 0 && hits(10) > 0, "{__wsr_text_13739}{} merger canceled {}", hits(9), hits(10),__wsr_text_13739=__content.string(13739));
}

#[test]
fn diff_extraordinary_dividend_etf() {
    par_seeds(1..=10, |seed| {
        let mut r = Rand(seed * 1013 + 17);
        let (mut g, ae) = dividend_world(seed + 3700, &mut r);
        select_player_etf(&mut g, &mut r);
        let etf = 1581;
        g.bank_debt[etf] = Ext::from_f64(0.0);
        g.bonds_issued[etf] = if seed % 5 == 4 { 10 } else { 0 };
        g.cash[etf] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(2000.0));
        g.extraordinary_pl[etf] = if seed % 3 == 0 { 0 } else { r.range(-50, 100) };
        for k in 0..3 {
            let h = if k == 0 { g.cur_player } else { active_corp(&g, &mut r, 131, 1580) };
            give_holding(&mut g, h, etf, [5, 30, 10][k], f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0)));
        }
        let answers: Vec<&str> = match seed % 3 {
            0 => vec!["IDYES", "5"],
            1 => vec!["IDNO", "IDYES", "3", "IDYES", "IDYES"],
            _ => vec!["IDYES", "1"],
        };
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        assert_eq!(selected_player_etf(&mut port.clone()), 1581);
        let mut rng = Rng::from_state(rs);
        let mut ui = Seq::new(&answers);
        extraordinary_dividend(&mut port, &mut ui, &mut rng, &NOW);
        let Some(o) = run_with(EXTRAORDINARY_DIVIDEND, &[], &g, rs, &ui, &[]) else {
            return;
        };
        if port.cash[etf] < g.cash[etf] {
            hit(11);
        }
        check(&format!("ExtraordinaryDividend(ETF) seed {seed} ae {ae}"), &port, &rng, &ui, &o);
    });
    assert!(hits(11) > 0, "no ETF distribution");
}

// ------------------------------------------------------------------------ CancelMergerOnViolation

#[test]
fn diff_cancel_merger_on_violation() {
    par_seeds(1..=6, |seed| {
        let mut r = Rand(seed * 911 + 2);
        let mut g = menu_world(seed + 3800, &mut r);
        let ae = if seed == 1 { 5 } else { active_corp(&g, &mut r, 131, 1580) };
        set_active(&mut g, ae);
        g.merger_target = if seed == 2 { 0 } else { ae };
        g.merger_acquirer = active_corp(&g, &mut r, 131, 1580);
        g.merger_proposer_player = 1;
        g.next_takeover_tick = 77;
        let rs = cast::<u32, _>(r.next());
        let mut port = g.clone();
        let rng = Rng::from_state(rs);
        let mut ui = Seq::default();
        cancel_merger_on_violation(&mut port, &mut ui);
        let Some(o) = run_with(CANCEL_MERGER_ON_VIOLATION, &[], &g, rs, &ui, &[]) else {
            return;
        };
        check(&format!("CancelMergerOnViolation seed {seed}"), &port, &rng, &ui, &o);
    });
}
