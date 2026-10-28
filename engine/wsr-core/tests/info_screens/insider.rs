//! Differential tests of `logic::info_screens::insider`: InsiderEarnTipEvent (0x67cbdd) and
//! CheatInsiderEarningsTip (0x678965, with its GOSUB InsiderEarningsTip_StoryText) against the original
//! (full state, RND state, message boxes / pop-ups).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::{Rand, par_seeds};
use std::collections::BTreeSet;
use std::sync::Mutex;
use wsr_core::logic::info_screens::insider::{cheat_insider_earnings_tip, insider_earn_tip_event};
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const EVENT: u32 = 0x67cbdd;
const CHEAT_TIP: u32 = 0x678965;

/// Runs the original at `addr` and the port on `g0` (MSGBOX `answers`) and compares state, RND state
/// and the number of message boxes / pop-ups. Returns the port's state and Ui.
fn check(
    addr: u32,
    g0: &Globals,
    seed: u64,
    answers: &[&str],
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng),
) -> Option<(Globals, Rec)> {
    let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b9) ^ 0x1357_2468;
    let o = run(addr, &[], g0, rng0, answers, &[])?;
    let mut g = g0.clone();
    let mut ui = Rec::default();
    ui.answers.extend(answers.iter().map(|a| if *a == "IDNO" { 7 } else { 6 }));
    let mut rng = Rng::from_state(rng0);
    port(&mut g, &mut ui, &mut rng);
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    // error 9 of the original's ProfitMargin(InsiderEarnTipCorp) for a downside tip (BUG 410)
    let oob = cast::<usize, _>(g.profit_margin.oob_count());
    assert_eq!(errs.len(), oob, "seed {seed}: runtime errors in the original: {errs:?}");
    cmp().assert(&g, &o.globals);
    assert_eq!(rng.state(), o.rng, "seed {seed}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "seed {seed}: message boxes / pop-ups {:?}\n{:?}", ui.msgs, o.log);
    Some((g, ui))
}

fn base(seed: u64) -> (Globals, Rand) {
    let g = world(seed);
    let r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x7777_1234);
    (g, r)
}

/// A pool of eligible upside/downside tip candidates with real corporate shareholders. Merely
/// assigning CorpController is insufficient: the offer recomputes control from stock holdings.
fn eligible_tip_candidates(g: &mut Globals, player: i32) {
    for holder in 1..=1600 {
        for slot in 0..=15 {
            g.hold_stock_id[(slot, holder)] = 0;
            g.hold_stock_pct[(slot, holder)] = 0;
        }
        g.corp_controller[holder] = 0;
    }
    for slot in 0..=15 { g.hold_bond_id[(slot, player)] = 0; }
    g.corp_controller[player] = player;
    g.cash[player] = Ext::from_f64(100_000.0);
    g.t_bills[player] = Ext::from_f64(0.0);
    g.bank_debt[player] = Ext::from_f64(0.0);
    g.player_ceo_of[player] = 0;
    g.insider_earn_tip_corp = 0;
    g.insider_tippee = 0;
    g.merger_tip_corp = 0;
    g.insider_probe_player = 0;
    g.informant_role = st("audit");
    g.ind_demand_gap[7] = cast::<f64, _>(Ext::from_f64(0.0));
    for corp in 131..=230 {
        let parent = corp + 369;
        g.entity_status[corp] = 1;
        g.entity_status[parent] = 1;
        g.industry_num[corp] = 7;
        g.industry_num[parent] = 70;
        g.hold_stock_id[(0, parent)] = 1;
        g.hold_stock_id[(1, parent)] = cast::<i16, _>(corp);
        g.hold_stock_pct[(1, parent)] = 40;
        g.market_cap[corp] = Ext::from_f64(1_000.0);
        g.business_assets[corp] = Ext::from_f64(1_000_000.0);
        g.cash[corp] = Ext::from_f64(100.0);
        g.t_bills[corp] = Ext::from_f64(0.0);
        g.bank_debt[corp] = Ext::from_f64(0.0);
        g.bonds_issued[corp] = 0;
        g.earnings_y1[corp] = Ext::from_f64(10_000.0);
        g.earnings_ytd[corp] = Ext::from_f64(5_000.0);
        g.profit_margin[corp] = if corp % 2 == 0 { -10 } else { 10 };
        g.investor_sentiment[corp] = 0;
        g.ent_misc_data[corp] = st("");
    }
}

/// Gives player `p` a long (`sign` 1) or short (-1) stake in `corp`.
fn give_stake(g: &mut Globals, p: i32, corp: i32, sign: i16) {
    let n = cast::<i32, _>(g.hold_stock_id[(0, p)]) + 1;
    if n > 15 {
        return;
    }
    g.hold_stock_id[(n, p)] = sign * cast::<i16, _>(corp);
    g.hold_stock_pct[(n, p)] = sign * 5;
    g.hold_stock_basis[(n, p)] = Ext::from_f64(100.0);
    g.hold_stock_id[(0, p)] = cast::<i16, _>(n);
}

fn record(set: &Mutex<BTreeSet<String>>, key: impl Into<String>) {
    set.lock().unwrap().insert(key.into());
}

#[test]
fn diff_insider_earn_tip_event() {
    let hit = Mutex::new(BTreeSet::new());
    par_seeds(1..=160, |seed| {
        let (mut g, mut r) = base(seed);
        g.num_players = cast::<i32, _>(r.range(1, 4));
        let c = cast::<i32, _>(r.range(131, 1580));
        let tippee = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.entity_status[c] = if r.chance(Ext::from_f64(0.95)) { 1 } else { 2 };
        // These ids are operating companies; bank/insurer industries would index financial-institution
        // arrays outside their 11..85 / 86..130 bounds in the original's valuation callees.
        g.industry_num[c] = if r.chance(Ext::from_f64(0.03)) { 70 } else { cast::<i16, _>(r.range(3, 69)) };
        let down = r.chance(Ext::from_f64(0.5));
        g.insider_earn_tip_corp = if down { cast::<i16, _>(-c) } else { cast::<i16, _>(c) };
        g.insider_tippee = if r.chance(Ext::from_f64(0.97)) { cast::<i16, _>(tippee) } else { 0 };
        let state = cast::<i16, _>(r.range(0, 4));
        g.tipster_state[tippee] = state;
        g.tip_industry = if r.chance(Ext::from_f64(0.9)) { g.industry_num[c] } else { cast::<i16, _>(r.range(1, 69)) };
        g.insider_tip_event_tick = if r.chance(Ext::from_f64(0.75)) { cast::<i16, _>(g.cur_quarter) } else { cast::<i16, _>(g.cur_quarter % 4 + 1) };
        g.insider_tip_event_qtr = cast::<i16, _>(r.range(1, 4));
        g.sim_co_count = if r.chance(Ext::from_f64(0.6)) { c } else { cast::<i32, _>(r.range(1, 1600)) };
        g.insider_probe_player = if r.chance(Ext::from_f64(0.5)) { tippee } else { 0 };
        g.currency_code = st(["USA", "UK"][cast::<usize, _>(r.range(0, 1))]);
        g.new_mgmt_list = st(["", "0131/", "XXXX/0200/0300/0400/0500/0600/"][cast::<usize, _>(r.range(0, 2))]);
        if r.chance(Ext::from_f64(0.5)) {
            give_stake(&mut g, tippee, c, if down { -1 } else { 1 });
        }
        if r.chance(Ext::from_f64(0.15)) {
            g.news_msg = st("A pending headline");
        }
        // trend of the tipped stock: rising / falling / flat closes
        let cur = cast::<i32, _>(g.cap_hist_close[(0, c)]);
        let trend = cast::<Ext, _>(r.range(-1, 1));
        let mut v = g.cap_hist_close[(cur, c)];
        for k in 1..=24 {
            let mut slot = cur - k;
            if slot < 1 {
                slot += 60;
            }
            v *= Ext::from_f64(1.0) - trend * Ext::from_f64(0.04);
            g.cap_hist_close[(slot, c)] = v;
        }
        let Some((g1, ui)) = check(EVENT, &g, seed, &[], |g, ui, rng| insider_earn_tip_event(g, ui, rng)) else {
            return;
        };
        let event_qtr = cast::<i32, _>(g.insider_tip_event_tick) == g.cur_quarter;
        let breaks = event_qtr && g.sim_co_count == c;
        record(&hit, format!("event_qtr={event_qtr} breaks={breaks} state={state} down={down}"));
        if g1.insider_earn_tip_corp == 0 {
            record(&hit, "cleared");
        }
        if !ui.msgs.is_empty() {
            record(&hit, format!("popup state={state} breaks={breaks}"));
        }
        if g1.profit_margin.oob_count() > 0 {
            record(&hit, "probe dropped (down)");
        }
        if g1.insider_probe_player == 0 && g.insider_probe_player != 0 {
            record(&hit, "probe cleared");
        }
        if g1.news_headline[60] != g.news_headline[60] {
            record(&hit, format!("news event_qtr={event_qtr} breaks={breaks}"));
        }
    });
    let hit = hit.into_inner().unwrap();
    eprintln!("{hit:#?}");
    if std::env::var("WSR_ONLY_SEED").is_ok() {
        return;
    }
    for k in [
        "cleared",
        "probe cleared",
        "news event_qtr=false breaks=false",
        "news event_qtr=true breaks=false",
        "news event_qtr=true breaks=true",
        "popup state=1 breaks=true",
        "popup state=2 breaks=true",
        "popup state=3 breaks=true",
        "popup state=4 breaks=true",
    ] {
        assert!(hit.contains(k), "branch not hit: {k}");
    }
    for st in 0..=4 {
        for down in [false, true] {
            for (eq, br) in [(true, true), (true, false), (false, false)] {
                let k = format!("event_qtr={eq} breaks={br} state={st} down={down}");
                assert!(hit.contains(&k), "branch not hit: {k}");
            }
        }
    }
}

#[test]
fn diff_cheat_insider_earnings_tip() {
    let __content = wsr_core::content::current();
    let hit = Mutex::new(BTreeSet::new());
    par_seeds(1..=64, |seed| {
        let (mut g, mut r) = base(seed);
        let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.cur_player = p;
        let state = if seed <= 16 { cast::<i16, _>((seed - 1) % 4) } else { cast::<i16, _>(r.range(0, 3)) };
        g.tipster_state[p] = state;
        g.informant_role = if r.chance(Ext::from_f64(0.5)) { st("") } else { st("audit") };
        g.currency_code = st(["USA", "UK"][cast::<usize, _>(r.range(0, 1))]);
        g.currency_suffix = st(if r.chance(Ext::from_f64(0.5)) { "" } else { " Pounds" });
        g.insider_probe_player = if r.chance(Ext::from_f64(0.6)) { 0 } else { p };
        g.cash[p] = if r.chance(Ext::from_f64(0.1)) { f64r(&mut r, Ext::from_f64(40_000.0), Ext::from_f64(90_000.0)) } else { f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(20_000.0)) };
        g.player_ceo_of[p] = if r.chance(Ext::from_f64(0.3)) { cast::<i32, _>(r.range(131, 1580)) } else { 0 };
        g.sim_co_count = cast::<i32, _>(r.range(1, 1600));
        match r.range(0, 9) {
            0 => {
                // a pending tip: the offer ends at once
                g.insider_earn_tip_corp = cast::<i16, _>(r.range(131, 1580));
                g.insider_tippee = cast::<i16, _>(p);
            }
            1 => g.merger_tip_corp = cast::<i16, _>(r.range(131, 1580)),
            _ => {}
        }
        if r.chance(Ext::from_f64(0.2)) {
            g.news_msg = st("A pending headline");
        }
        // candidates: publicly held, company-controlled operating corps with a big business
        for c in 131..=1580 {
            if r.chance(Ext::from_f64(0.3)) {
                g.corp_controller[c] = cast::<i32, _>(r.range(131, 1580));
                g.business_assets[c] = g.business_assets[c] * Ext::from_f64(4.0) + Ext::from_f64(2000.0);
                if r.chance(Ext::from_f64(0.5)) {
                    g.profit_margin[c] = cast::<i16, _>(r.range(-18, -3));
                    g.investor_sentiment[c] = cast::<i32, _>(r.range(-20, 5));
                } else {
                    g.profit_margin[c] = cast::<i16, _>(r.range(5, 21));
                    g.investor_sentiment[c] = cast::<i32, _>(r.range(0, 30));
                    g.earnings_ytd[c] = g.earnings_ytd[c].abs();
                }
            }
        }
        let answer = if seed <= 16 {
            eligible_tip_candidates(&mut g, p);
            if (seed - 1) / 4 % 2 == 0 { "IDYES" } else { "IDNO" }
        } else if r.chance(Ext::from_f64(0.8)) { "IDYES" } else { "IDNO" };
        let Some((g1, ui)) = check(CHEAT_TIP, &g, seed, &[answer], |g, ui, rng| {
            cheat_insider_earnings_tip(g, ui, rng, 0)
        }) else {
            return;
        };
        let tipped = cast::<i32, _>(g1.insider_tippee) == p && g.insider_tippee == 0 && g1.insider_earn_tip_corp != 0;
        let offered = ui.titles.iter().any(|t| t.contains(__content.string(13903)) || t.contains(__content.string(13904)));
        record(&hit, format!("state={state}{__wsr_text_13905}{offered} tipped={tipped}",__wsr_text_13905=__content.string(13905)));
        if tipped {
            record(&hit, format!("tip down={}", g1.insider_earn_tip_corp < 0));
            if g1.insider_probe_player == p && g.insider_probe_player == 0 {
                record(&hit, "probe");
            }
        }
        if ui.msgs.iter().any(|m| m.contains(__content.string(1927))) {
            record(&hit, format!("story state={state}"));
        }
        if ui.titles.iter().any(|t| t.contains(__content.string(13906))) {
            record(&hit, format!("nothing state={state}"));
        }
    });
    let hit = hit.into_inner().unwrap();
    eprintln!("{hit:#?}");
    for k in ["tip down=true", "tip down=false", "story state=2", "story state=3", "nothing state=0"] {
        assert!(hit.contains(k), "branch not hit: {k}");
    }
    for st in 0..=3 {
        for k in [format!("state={st} offered=true tipped=true"), format!("state={st} offered=true tipped=false")] {
            assert!(hit.contains(&k), "branch not hit: {k}");
        }
    }
}
