//! Differential tests of `logic::events::merger`: MergerTakeoverEvents (0x4b37c8) in its three modes and
//! InsiderTradingProbeVerdict (0x67c24a) against the original routines.
//!
//! `TIME$` is fixed by the harness (10:00:00), so the port gets `time_secs = 0`.
use wsr_core::pb::real::{Ext, cast};


use super::check::*;
use super::common::*;
use super::diff::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::events::merger::*;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cux;
use wsr_core::state::Globals;

const MERGER_TAKEOVER_EVENTS: u32 = 0x4b37c8;
const INSIDER_TRADING_PROBE_VERDICT: u32 = 0x67c24a;
/// BuyStock (0x4efe60) and MergeCompanies (0x509293): hooked (their calls are logged for the counts).
const BUY_STOCK: u32 = 0x4efe60;
const MERGE_COMPANIES: u32 = 0x509293;

/// Gives the port MSCNEWS.DAT from extracted/app (the original reads the same file).
fn init_files() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted/app/Mscnews.dat");
    wsr_core::logic::economy::set_mscnews_dat(std::fs::read(p).ok());
}

/// Count of hits per branch label.
struct Hits(Vec<(String, AtomicUsize)>);

impl Hits {
    fn new(labels: &[&str]) -> Self {
        Hits(labels.iter().map(|l| (l.to_string(), AtomicUsize::new(0))).collect())
    }
    fn hit(&self, label: &str) {
        self.0.iter().find(|(l, _)| l == label).unwrap().1.fetch_add(1, Ordering::Relaxed);
    }
    #[track_caller]
    fn assert_all(&self) {
        if unavailable().is_some() {
            return;
        }
        let missing: Vec<&str> = self.0.iter().filter(|(_, n)| n.load(Ordering::Relaxed) == 0).map(|(l, _)| l.as_str()).collect();
        let counts: Vec<String> = self.0.iter().map(|(l, n)| format!("{l}={}", n.load(Ordering::Relaxed))).collect();
        assert!(missing.is_empty(), "branches not hit: {missing:?} ({counts:?})");
    }
}

fn currency(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    match r.range(0, 3) {
        0 => {
            g.currency_code = st("USA");
            g.currency_suffix = st("");
            g.currency_name = st(__content.string(6643));
        }
        1 => {
            g.currency_code = st("GBR");
            g.currency_suffix = st(" Pounds");
            g.currency_symbol = st("£");
            g.currency_name = st(__content.string(6644));
        }
        2 => {
            g.currency_code = st("JAP");
            g.currency_suffix = st("");
            g.currency_symbol = st("¥");
            g.currency_name = st("Yen");
        }
        _ => {
            g.currency_code = st("EUR");
            g.currency_suffix = st("");
            g.currency_name = st("Euros");
        }
    }
}

// ------------------------------------------------------------------------- InsiderTradingProbeVerdict

#[test]
fn diff_insider_trading_probe_verdict() {
    let __content = wsr_core::content::current();
    init_files();
    let hits = Hits::new(&["guilty", "big_fine", __content.string(13825), "cpu_cleared", "tippee_skip"]);
    par_seeds(1..=40, |seed| {
        let mut r = Rand(seed * 7919 + 17);
        let mut g = world(seed + 3100);
        g.num_players = cast::<i32, _>(r.range(2, 4));
        let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.entity_status[p] = 1;
        g.is_computer_player[p] = if seed % 9 == 4 { 1 } else { 0 };
        if seed % 13 == 6 {
            g.player_name[p] = st("...OUT...");
        }
        g.insider_probe_player = if seed % 17 == 16 { 0 } else { p };
        g.insider_tippee = if seed % 11 == 5 { cast::<i16, _>(p) } else { cast::<i16, _>(r.range(0, 5)) };
        if cast::<i32, _>(g.insider_tippee) == p && seed % 11 != 5 {
            g.insider_tippee = 0;
        }
        g.tipster_state[p] = [0, 1, 4, -1][cast::<usize, _>(r.range(0, 3))];
        g.cur_quarter = cast::<i32, _>(r.range(1, 4));
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        currency(&mut g, &mut r);
        if seed % 4 == 0 {
            g.cash[p] = f64r(&mut r, Ext::from_f64(300_000.0), Ext::from_f64(2_000_000.0));
        }
        if seed % 6 == 1 {
            g.cash[p] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
        }
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(INSIDER_TRADING_PROBE_VERDICT, &[], &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        insider_trading_probe_verdict(&mut port, &mut ui, &mut rng);
        check(&format!("InsiderTradingProbeVerdict seed {seed}{__wsr_text_13565}{p}",__wsr_text_13565=__content.string(13565)), &port, &rng, &ui, &o);
        if port.cash[p] < g.cash[p] {
            hits.hit("guilty");
            if g.cash[p] > Ext::from_f64(300_000.0) {
                hits.hit("big_fine");
            }
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(13825))) {
            hits.hit(__content.string(13825));
        }
        if g.insider_probe_player == p && g.is_computer_player[p] == 1 {
            hits.hit("cpu_cleared");
        }
        if cast::<i32, _>(g.insider_tippee) == p && port.insider_probe_player == p {
            hits.hit("tippee_skip");
        }
    });
    hits.assert_all();
}

// ------------------------------------------------------------------------- MergerTakeoverEvents

/// A pending or tipped merger: acquirer and target corps (active, 131..1580, industry < 70).
fn two_corps(g: &Globals, r: &mut Rand) -> (i32, i32) {
    let a = active_corp(g, r);
    let mut t = active_corp(g, r);
    while t == a {
        t = active_corp(g, r);
    }
    (a, t)
}

/// Gives player `p` (and a corp controlled by `p`) a stake in `corp`.
fn give_stake(g: &mut Globals, p: i32, corp: i32, pct: i16) {
    for slot in 1..=15 {
        if g.hold_stock_id[(slot, p)] == 0 || cast::<i32, _>(g.hold_stock_id[(slot, p)]) == corp {
            g.hold_stock_id[(slot, p)] = cast::<i16, _>(corp);
            g.hold_stock_pct[(slot, p)] = pct;
            g.hold_stock_basis[(slot, p)] = Ext::from_f64(50.0);
            break;
        }
    }
    g.hold_stock_id[(0, p)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, p)] != 0).count());
}

fn opts_hooks() -> Vec<u32> {
    vec![BUY_STOCK, MERGE_COMPANIES]
}

/// Option premiums written by AiCorpSellNakedPuts are `CalcOptionPrice * pct` in 80-bit precision; the
/// STR$ of a premium (last field of an option record) can differ in the 15th digit. Takes the original's
/// record where only that field differs, by at most 1e-12 relative.
fn align_option_premiums(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (port.option_recs[i].to_display(), orig.option_recs[i].to_display());
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        let n = pa.len();
        if n < 2 || n != pb.len() || pa[..n - 1] != pb[..n - 1] {
            continue;
        }
        let (x, y): (Ext, Ext) =
            (pa[n - 1].trim().parse().unwrap_or(Ext::NAN), pb[n - 1].trim().parse().unwrap_or(Ext::NAN));
        if (x - y).abs() <= Ext::from_f64(1e-12) * y.abs() {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

/// Runs both; `Some((port, ui, error_exit))`. An error 9 of the original inside MergerTakeoverEvents
/// (ON ERROR -> ShowRuntimeError message box, exit) is compared without the message box.
fn run_mte(what: &str, g: &Globals, rs: u32, mode: i32, answers: &[&str]) -> Option<(Globals, Rec, bool, bool, bool)> {
    let __content = wsr_core::content::current();
    let o = run_hooked(MERGER_TAKEOVER_EVENTS, &[&mode.to_string()], g, rs, answers, &[], &opts_hooks())?;
    let mut port = g.clone();
    let mut rng = Rng::from_state(rs);
    let mut ui = Rec::default();
    for a in answers {
        ui.answers.push_back(if *a == "IDNO" { 7 } else { 6 });
    }
    merger_takeover_events(&mut port, &mut ui, &mut rng, 0, mode);
    align_option_premiums(&mut port, &o.globals);
    let c = cmp().rel_tol(Ext::from_f64(1e-9));
    let own_err = o.log.iter().any(|l| l.starts_with("PBERR") && l.contains("in 0x4b37c8"));
    if own_err {
        let d = c.diffs(&port, &o.globals);
        assert!(d.is_empty(), "{what} (error exit): {} differences:\n{}", d.len(), d.join("\n"));
        assert_eq!(rng.state(), o.rng, "{what}: RND state");
        assert_eq!(ui.msgs.len() + 1, orig_msgs(&o), "{what}: message boxes {:?}", ui.msgs);
    } else {
        check_with(what, &c, &port, &rng, &ui, &o);
    }
    let merged = o.log.iter().any(|l| l.starts_with("CALL MergeCompanies("))
        || ui.news.iter().any(|n| n.contains(__content.string(13826)));
    let bought = o.log.iter().any(|l| l.starts_with("CALL BuyStock("));
    Some((port, ui, own_err, merged, bought))
}

/// Mode 1: the vote on a pending merger (postponed, canceled, executed; tipped corp with insider checks).
#[test]
fn diff_merger_vote() {
    let __content = wsr_core::content::current();
    init_files();
    let hits = Hits::new(&[
        "postponed",
        "called_off",
        __content.string(13667),
        "weak_credit",
        "insider_kill",
        "ready_to_vote",
        "regulators_kill",
        "voted_down",
        __content.string(13827),
        "not_approved",
        "no_merger",
    ]);
    par_seeds(1..=120, |seed| {
        let mut r = Rand(seed * 104_729 + 3);
        let mut g = world(seed + 3300);
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.is_computer_player[g.cur_player] = 0;
        let (a, t) = two_corps(&g, &mut r);
        g.cheat_mode = st(if r.chance(Ext::from_f64(0.5)) { "Y" } else { "N" });
        if seed % 20 != 19 {
            g.merger_acquirer = a;
            g.merger_target = t;
        }
        g.next_takeover_tick = cast::<i32, _>(r.range(0, 1600));
        g.merger_postpone_key = if r.chance(Ext::from_f64(0.3)) { a + t } else { 0 };
        g.merger_proposer_player = match seed % 6 {
            0 => 0,
            1 => {
                let o = if g.num_players > 1 { g.num_players } else { 2 };
                if o == g.cur_player { 0 } else { o }
            }
            _ => g.cur_player,
        };
        let premium = [-Ext::from_f64(5.0), Ext::from_f64(1.0), Ext::from_f64(10.0), Ext::from_f64(30.0), Ext::from_f64(50.0), Ext::from_f64(79.0), Ext::from_f64(95.0)][cast::<usize, _>(r.range(0, 6))];
        g.merger_price_per_share = Cux::default();
        g.merger_price_per_share.set(g.market_cap[t] * (Ext::from_f64(1.0) + premium / Ext::from_f64(100.0)));
        g.credit_rating_num[t] = if seed % 7 == 3 { 1 } else { cast::<i32, _>(r.range(2, 10)) };
        if seed % 25 == 9 {
            g.entity_status[a] = 2;
        }
        // acquirer holds target stock: MergeCompanies path (skipped); with all 15 slots taken by other
        // stocks FindHoldingSlot returns 0: regulators kill the merger
        match seed % 5 {
            0 => {
                for slot in 1..=15 {
                    g.hold_stock_id[(slot, a)] = cast::<i16, _>(131 + slot);
                    g.hold_stock_pct[(slot, a)] = 1;
                }
                g.hold_stock_id[(0, a)] = 15;
            }
            1 => {
                // wholly owned by the acquirer
                for h in 1..=1600 {
                    for slot in 1..=15 {
                        if cast::<i32, _>(g.hold_stock_id[(slot, h)]) == t {
                            g.hold_stock_id[(slot, h)] = 0;
                            g.hold_stock_pct[(slot, h)] = 0;
                        }
                    }
                }
                give_stake(&mut g, a, t, 100);
            }
            _ => {
                if r.chance(Ext::from_f64(0.6)) {
                    give_stake(&mut g, a, t, cast::<i16, _>(r.range(5, 60)));
                }
            }
        }
        let cp = g.cur_player;
        match seed % 4 {
            0 | 1 if seed % 5 != 1 => give_stake(&mut g, cp, t, [2, 10, 30, 55][cast::<usize, _>(r.range(0, 3))]),
            2 => give_stake(&mut g, cp, a, 10),
            _ => {}
        }
        if seed % 3 != 0 {
            // tipped merger
            g.merger_tip_corp = cast::<i16, _>(t);
            g.insider_tippee = if r.chance(Ext::from_f64(0.85)) { cast::<i16, _>(cp) } else { 0 };
            g.insider_probe_player = if r.chance(Ext::from_f64(0.4)) { cast::<i32, _>(g.insider_tippee) } else { 0 };
            g.tipster_state[cp] = [0, 1, 2, 4][cast::<usize, _>(r.range(0, 3))];
            if r.chance(Ext::from_f64(0.3)) {
                g.wholly_owned_by = cast::<i16, _>(a);
            }
        } else {
            g.merger_tip_corp = 0;
        }
        g.rand_seed = cast::<i32, _>(r.range(0, 32000));
        let rs = cast::<u32, _>(r.next());
        let Some((port, ui, err, merged, _)) = run_mte(&format!("MergerTakeoverEvents(1) seed {seed} acq {a}{__wsr_text_13552}{t}",__wsr_text_13552=__content.string(13552)), &g, rs, 1, &[]) else {
            return;
        };
        let news = ui.news.join("|") + "|" + &ui.msgs.join("|") + "|" + &port.news_msg.to_display();
        for (l, pat) in [
            ("postponed", __content.string(13828)),
            ("called_off", __content.string(13829)),
            (__content.string(13667), __content.string(13830)),
            ("weak_credit", __content.string(13831)),
            ("insider_kill", __content.string(13832)),
            ("ready_to_vote", __content.string(13833)),
            ("regulators_kill", __content.string(13826)),
            ("voted_down", __content.string(13834)),
            (__content.string(13827), __content.string(13835)),
            ("not_approved", __content.string(13836)),
        ] {
            if news.contains(pat) {
                hits.hit(l);
            }
        }
        if g.merger_target == 0 {
            hits.hit("no_merger");
        }
        let _ = (err, merged);
    });
    hits.assert_all();
}

/// Modes 0 and 2: deal selection, antitrust review, the offer to a human holder or immediate execution.
#[test]
fn diff_merger_takeover_attempt() {
    let __content = wsr_core::content::current();
    init_files();
    let hits = Hits::new(&["offer", "halted_comment", __content.string(13837), "tip_tick_moved", "picked_review", "early_exit", "error_exit", "stake_bought"]);
    par_seeds(1..=160, |seed| {
        let mut r = Rand(seed * 15_485_863 + 7);
        let mut g = world(seed + 3700);
        let mode = if seed % 3 == 0 { 2 } else { 0 };
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let cp = g.cur_player;
        g.is_computer_player[cp] = 0;
        g.merger_acquirer = 0;
        g.merger_target = 0;
        g.next_takeover_tick = if seed % 10 == 7 { 900 } else { 0 };
        g.cheat_mode = st(if seed % 2 == 0 || seed % 4 == 1 { "Y" } else { "N" });
        g.halt_comment_count1 = 0;
        g.cpu_max_corps = cast::<i32, _>(r.range(3, 30));
        let (_, t) = two_corps(&g, &mut r);
        g.takeover_candidate_id = if r.chance(Ext::from_f64(0.8)) { t } else { 0 };
        g.crowded_industry = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(g.industry_num[t]) } else { 0 };
        g.cash[t] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0));
        g.t_bills[t] = Ext::from_f64(0.0);
        g.market_cap[t] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(800.0));
        // big corps around: likely acquirers
        for _ in 0..200 {
            let c = cast::<i32, _>(r.range(86, 1580));
            if g.entity_status[c] == 1 && g.industry_num[c] < 70 {
                g.market_cap[c] = f64r(&mut r, Ext::from_f64(800.0), Ext::from_f64(6000.0));
                g.corp_controller[c] = 0;
                // (insurers 86..130 keep industry 2: the game never gives them another one, and
                // CalcEntityFinancials would index BusinessAssets(131 TO 1600) for them -> error 9)
                if c > 130 && r.chance(Ext::from_f64(0.5)) {
                    g.industry_num[c] = g.industry_num[t];
                }
            }
        }
        g.corp_controller[t] = cast::<i32, _>(r.range(11, 1600));
        // a strong 2-month move of the target's stock (insider commentary)
        if seed % 4 == 1 {
            let cur = cast::<i32, _>(g.cap_hist_close[(0, t)]);
            let up = r.chance(Ext::from_f64(0.6));
            for k in 0..6 {
                let mut slot = cur - k;
                if slot < 1 {
                    slot += 60;
                }
                let f = if up { Ext::from_f64(1.0) / Ext::from_f64(1.2f64).powi(k) } else { Ext::from_f64(1.2f64).powi(k) };
                g.cap_hist_close[(slot, t)] = Ext::from_f64(100.0) * f;
            }
        }
        if seed % 5 != 4 {
            give_stake(&mut g, cp, t, [1, 5, 15, 30][cast::<usize, _>(r.range(0, 3))]);
        }
        // RandSeed % 25 = 7 makes any industry pair fit
        g.rand_seed = if r.chance(Ext::from_f64(0.7)) { 25 * cast::<i32, _>(r.range(0, 1000)) + 7 } else { cast::<i32, _>(r.range(0, 32000)) };
        if mode == 2 {
            g.merger_tip_corp = cast::<i16, _>(t);
            g.insider_tippee = if r.chance(Ext::from_f64(0.8)) { cast::<i16, _>(cp) } else { 0 };
            g.insider_tip_event_tick = if r.chance(Ext::from_f64(0.7)) { cast::<i16, _>(g.sim_co_count) } else { cast::<i16, _>(r.range(0, 1600)) };
            if seed % 12 == 0 {
                g.entity_status[t] = 2;
            }
        } else if r.chance(Ext::from_f64(0.2)) {
            g.merger_tip_corp = cast::<i16, _>(r.range(131, 1580));
        }
        let rs = cast::<u32, _>(r.next());
        let Some((port, ui, err, merged, bought)) = run_mte(&format!("MergerTakeoverEvents({mode}) seed {seed}{__wsr_text_13552}{t}",__wsr_text_13552=__content.string(13552)), &g, rs, mode, &[]) else {
            return;
        };
        if err {
            hits.hit("error_exit");
        }
        if port.merger_target != 0 && g.merger_target == 0 {
            hits.hit("offer");
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(5227)) || m.contains(__content.string(5228))) {
            hits.hit("halted_comment");
        }
        if merged {
            hits.hit(__content.string(13837));
        }
        if bought {
            hits.hit("stake_bought");
        }
        if mode == 2 && port.insider_tip_event_tick != g.insider_tip_event_tick {
            hits.hit("tip_tick_moved");
        }
        if port.tmp_ext3 == Ext::from_f64(0.0) && (ui.reports.len() + ui.msgs.len() > 0 || port.merger_target != 0) {
            hits.hit("picked_review");
        }
        if ui.msgs.is_empty() && ui.news.is_empty() {
            hits.hit("early_exit");
        }
    });
    hits.assert_all();
}
