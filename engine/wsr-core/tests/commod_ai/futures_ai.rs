//! Differential tests of `logic::commod_ai::futures_ai`: SettleExpiredFutures, AiFuturesTrading,
//! CommodityEtfTrading and LeveragedEtfFuturesUpdate against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use wsr_core::logic::commod_ai::desk::*;
use wsr_core::logic::commod_ai::futures_ai::*;
use wsr_core::logic::sim_entity::transactions::settle_futures_contract;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SETTLE_EXPIRED: u32 = 0x6259d2;
const AI_FUTURES: u32 = 0x62fac5;
const COMMODITY_ETF: u32 = 0x64c333;
const LEVERAGED_ETF: u32 = 0x653409;
/// TIME$ of the harness is fixed at 10:00:00.
const TIME_SECS: i32 = 0;

#[test]
fn diff_settle_expired_futures() {
    let hits = Hits::default();
    par_seeds(0..24u64, |seed| {
        let (mut g, owners) = cworld(seed + 1000);
        let mut r = Rand(seed + 37);
        g.sim_co_count = cast::<i32, _>(r.range(10, 1500));
        // Exercise expiry for every commodity, including physical delivery of short positions.
        let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
        for i in 1..=n {
            let t = g.futures_recs[i].to_display();
            let f: Vec<&str> = t.split('|').collect();
            let due = r.chance(Ext::from_f64(0.6));
            let (m, y) = if due {
                (r.range(1, 12), g.cur_year - cast::<i32, _>(r.range(0, 1)))
            } else {
                (r.range(1, 12), g.cur_year + 1 + cast::<i32, _>(r.range(0, 2)))
            };
            g.futures_recs[i] = st(&format!("{}|{}|{}|{}|{}|{}|{}", f[0], f[1], f[2], m, y, f[5], f[6]));
        }
        if seed % 3 == 0 {
            g.entity_status[owners[1]] = 0; // records of an inactive owner are dropped
        }
        g.sel_entity = owners[2];
        let rng_start = rng0(seed);
        let Some(o) = run(SETTLE_EXPIRED, &[], &g, rng_start, &[], &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        settle_expired_futures(&mut g, &mut ui, &mut rng);
        check(&g, &rng, &ui, &o, &format!("seed {seed}"));
        if g.futures_recs[0] != g0.futures_recs[0] {
            hits.hit("settled");
        }
    });
    hits.require(&["settled"]);
}

#[test]
fn diff_settle_futures_physical_delivery() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..70u64, |seed| {
        let (mut g, _) = cworld(seed + 8000);
        let commodity = PHYS[cast::<usize, _>(seed / 10)];
        let case = seed % 10;
        let owner = if case < 4 { 1 } else { 131 };
        g.num_players = 2;
        g.entity_status[owner] = 1;
        g.corp_controller[owner] = if case == 5 { 0 } else { 1 };
        g.is_computer_player[1] = 0;
        g.player_take_delivery[1] = if case == 2 { 0 } else { 1 };
        g.player_make_delivery[1] = if case == 3 { 0 } else { 1 };
        g.cash[owner] = Ext::from_f64(1e9);
        g.t_bills[owner] = Ext::from_f64(1e9);
        g.cur_player = 1;
        g.sel_entity = owner;
        g.active_entity = owner;
        g.futures_batch_gain = Ext::from_f64(42.0);
        for i in 0..=1000 {
            g.futures_recs[i] = st("");
            g.phys_commod_rec[i] = st("");
        }
        let short = matches!(case, 1 | 3 | 5 | 6);
        let quantity = if short { -10 } else if case == 7 { 2_000_000_000 } else { 10 };
        g.futures_recs[0] = st("1");
        g.futures_recs[1] = st(&format!("{commodity}|{owner}|{quantity}|1|{}|20|10", g.cur_year));
        if case == 8 || case == 9 {
            g.cash[owner] = Ext::from_f64(0.0);
            g.t_bills[owner] = if case == 9 { Ext::from_f64(30.0) } else { Ext::from_f64(0.0) };
            g.bank_debt[owner] = Ext::from_f64(1e12);
            g.lending_bank[owner] = 11;
            g.credit_frozen[11] = 1;
            g.hold_stock_id[(0, owner)] = 0;
        }
        if case == 1 || case == 3 || case == 5 || case == 6 {
            let held = if case == 6 { 3 } else { 30 };
            g.phys_commod_rec[0] = st("1");
            g.phys_commod_rec[1] = st(&format!("{commodity}|{owner}|{held}|10"));
        }
        let rng_start = rng0(seed);
        let Some(o) = run(0x625ce5, &["1", "1", "1", "1"], &g, rng_start, &[], &[]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        settle_futures_contract(&mut port, &mut ui, &mut rng, 1, 1, 1, 1);
        check(&port, &rng, &ui, &o, &format!("seed {seed} {commodity}{__wsr_text_13612}{case}",__wsr_text_13612=__content.string(13612)));
        if port.phys_commod_rec[1] != g.phys_commod_rec[1] {
            hits.hit(if short { "made delivery" } else { "took delivery" });
        }
        if ui.news.iter().any(|text| text.contains(__content.string(13613))) {
            hits.hit("limit blocked");
        }
        if (case == 2 || case == 3) && port.phys_commod_rec[1] == g.phys_commod_rec[1] {
            hits.hit("preference respected");
        }
        if ui.news.iter().any(|text| text.contains("dumps ")) {
            hits.hit("delivery shortfall");
        }
    });
    hits.require(&["made delivery", "took delivery", "limit blocked", "preference respected", "delivery shortfall"]);
}

/// A corp of a hedging industry as `SimCoCount`, set up to trade.
fn ai_world(seed: u64) -> Globals {
    let (mut g, _) = cworld(seed + 1100);
    let mut r = Rand(seed + 41);
    let industries: [i16; 18] = [2, 7, 8, 11, 15, 16, 23, 25, 28, 37, 50, 52, 69, 2, 37, 15, 28, 50];
    let ind = industries[cast::<usize, _>(seed % cast::<u64, _>(industries.len()))];
    let c = loop {
        let c = cast::<i32, _>(r.range(131, 1579));
        if g.entity_status[c] == 1 {
            break c;
        }
    };
    let c = if ind == 2 { cast::<i32, _>(r.range(86, 130)) } else { c };
    g.entity_status[c] = 1;
    g.industry_num[c] = ind;
    g.sim_co_count = c;
    g.cur_quarter = if c % 2 == 0 { 2 } else { 3 };
    g.start_year = g.cur_year - 2;
    g.business_assets[c] = f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(20000.0));
    g.cash[c] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(5000.0));
    g.credit_rating_num[c] = cast::<i32, _>(r.range(6, 10));
    g.corp_controller[c] = match seed % 5 {
        0 => 1, // a player's company (controller type 2 or 1)
        _ => 0,
    };
    // strip its existing futures for a low position count
    let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
    for i in 1..=n {
        if g.futures_recs[i].to_display().split('|').nth(1) == Some(&c.to_string()) {
            g.futures_recs[i] = st(&format!("GOLD|{}|1|6|{}|10|10", c + 1, g.cur_year + 1));
        }
    }
    g
}

#[test]
fn diff_ai_futures_trading() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..160u64, |seed| {
        let mut g = ai_world(seed);
        let c = g.sim_co_count;
        let rng_start = rng0(seed);
        let Some(o) = run(AI_FUTURES, &[], &g, rng_start, &[], &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        ai_futures_trading(&mut g, &mut ui, &mut rng, TIME_SECS);
        check(&g, &rng, &ui, &o, &format!("seed {seed} corp {c}{__wsr_text_1975}{}", g.industry_num[c],__wsr_text_1975=__content.string(1975)));
        if g.futures_recs[0] != g0.futures_recs[0] {
            hits.hit(&format!("trade{}", g.industry_num[c]));
        }
    });
    hits.require(&["trade2", "trade37", "trade50"]);
    let traded: usize = ["trade7", "trade8", "trade11", "trade16", "trade23", "trade25", "trade52", "trade69"]
        .iter()
        .map(|k| hits.get(k))
        .sum();
    assert!(traded > 0, "no oil hedges");
    assert!(hits.get("trade15") + hits.get("trade28") > 0, "no grain hedges");
}

fn etf_world(seed: u64) -> Globals {
    let (mut g, _) = cworld(seed + 1200);
    let mut r = Rand(seed + 43);
    g.entity_status[1600] = 1;
    g.industry_num[1600] = 71;
    g.sim_co_count = 1600;
    g.cash[1600] = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(20000.0));
    g.t_bills[1600] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
    g.credit_rating_num[1600] = cast::<i32, _>(r.range(8, 10));
    let adviser = cast::<i32, _>(r.range(86, 130));
    g.entity_status[adviser] = 1;
    g.corp_controller[adviser] = 0;
    g.fund_adviser[1600] = adviser;
    g.ctrl_since_year[1600] = g.cur_year - 2;
    if seed % 3 == 0 {
        g.cur_quarter = 1;
        g.start_year = g.cur_year;
        g.sim_co_count = 1600;
    } else {
        g.start_year = g.cur_year - 3;
    }
    g
}

#[test]
fn diff_commodity_etf_trading() {
    let hits = Hits::default();
    par_seeds(0..24u64, |seed| {
        let mut g = etf_world(seed);
        let rng_start = rng0(seed);
        let Some(o) = run(COMMODITY_ETF, &[], &g, rng_start, &[], &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        commodity_etf_trading(&mut g, &mut ui, &mut rng, TIME_SECS);
        check(&g, &rng, &ui, &o, &format!("seed {seed}"));
        if g.futures_recs[0] != g0.futures_recs[0] {
            hits.hit(if seed % 3 == 0 { "opened" } else { "traded" });
        }
    });
    hits.require(&["opened", "traded"]);
}

fn leveraged_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let (mut g, _) = cworld(seed + 1300);
    let mut r = Rand(seed + 47);
    g.start_year = g.cur_year - 4;
    g.sim_co_count = cast::<i32, _>(r.range(10, 1490));
    g.ent_name[1584] = st(if r.chance(Ext::from_f64(0.5)) { __content.string(2566) } else { __content.string(2567) });
    g.ent_name[1585] = st(__content.string(2568));
    g.etf_futures_halt_qtr = [0, 0, 1, 2, 3, 4][cast::<usize, _>(r.range(0, 5))];
    g.etf_futures_quit_year = g.cur_year - cast::<i32, _>(r.range(0, 6));
    g.suppress_popups = 0;
    // drop the world's records of the two funds (their delivery months might be out of the table)
    let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
    for i in 1..=n {
        let o = g.futures_recs[i].to_display().split('|').nth(1).unwrap().to_string();
        if o == "1584" || o == "1585" {
            g.futures_recs[i] = st(&format!("GOLD|500|1|6|{}|10|10", g.cur_year + 1));
        }
    }
    let cur_month = cast::<i32, _>(((cast::<Ext, _>(g.sim_co_count) / Ext::from_f64(500.0)).min(Ext::from_f64(3.0)) + cast::<Ext, _>(g.cur_quarter - 1) * Ext::from_f64(3.0)).ceil());
    let mut k = n;
    for e in [1584, 1585] {
        g.entity_status[e] = if r.chance(Ext::from_f64(0.25)) { 2 } else { 1 };
        g.industry_num[e] = 71;
        g.cash[e] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20000.0));
        g.t_bills[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10000.0));
        g.bank_debt[e] = if r.chance(Ext::from_f64(0.5)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) };
        g.bonds_issued[e] = 0;
        g.market_cap[e] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20000.0));
        g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
        g.ctrl_since_year[e] = g.cur_year - cast::<i32, _>(r.range(0, 2));
        g.fund_adviser[e] = cast::<i32, _>(r.range(86, 130));
        g.recent_offerings[cast::<i32, _>(r.range(1, 10))] = if r.chance(Ext::from_f64(0.4)) { e } else { 0 };
        for slot in 0..=15 {
            g.hold_stock_id[(slot, e)] = 0;
        }
        // index futures of the fund (3..8 months ahead) and a non-index one that gets settled
        for _ in 0..r.range(0, 4) {
            k += 1;
            let off = cast::<i32, _>(r.range(0, 8));
            let (mut m, mut y) = (cur_month + off, g.cur_year);
            if m > 12 {
                m -= 12;
                y += 1;
            }
            let q = r.range(1, 300) * if e == 1585 { -1 } else { 1 };
            g.futures_recs[k] = st(&format!("{__wsr_text_13614}{e}|{q}|{m}|{y}|{:.2}|{:.2}", f64r(&mut r, Ext::from(Ext::from_f64(100.0)), Ext::from(Ext::from_f64(9000.0))), f64r(&mut r, Ext::from(Ext::from_f64(100.0)), Ext::from(Ext::from_f64(900.0))),__wsr_text_13614=__content.string(13614)));
        }
        if r.chance(Ext::from_f64(0.3)) {
            k += 1;
            g.futures_recs[k] = st(&format!("OIL|{e}|3|5|{}|100|10", g.cur_year + 1));
        }
    }
    g.futures_recs[0] = st(&k.to_string());
    if seed % 5 == 3 {
        // a small fund without futures on the offering list: it may give up and liquidate
        let e = 1584 + cast::<i32, _>(seed / 5 % 2);
        g.entity_status[e] = 1;
        g.cash[e] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(3000.0));
        g.t_bills[e] = Ext::from_f64(0.0);
        g.recent_offerings[1] = e;
        g.rand_seed = 7 * cast::<i32, _>(r.range(0, 1000));
        g.ctrl_since_year[e] = if seed % 10 == 3 { g.cur_year } else { g.cur_year - 1 };
        let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
        for i in 1..=n {
            if g.futures_recs[i].to_display().split('|').nth(1) == Some(&e.to_string()) {
                g.futures_recs[i] = st(&format!("GOLD|500|1|6|{}|10|10", g.cur_year + 1));
            }
        }
    }
    if seed % 2 == 0 {
        // nobody else holds the funds' stock (public 100%): the reorganization with new money
        for h in 1..=1600 {
            for slot in 1..=15 {
                if matches!(g.hold_stock_id[(slot, h)], 1584 | 1585) {
                    g.hold_stock_id[(slot, h)] = 0;
                    g.hold_stock_pct[(slot, h)] = 0;
                }
            }
        }
    }
    g
}

#[test]
fn diff_leveraged_etf_futures_update() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..96u64, |seed| {
        let mut g = leveraged_world(seed);
        let rng_start = rng0(seed);
        let Some(o) = run(LEVERAGED_ETF, &[], &g, rng_start, &[], &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        leveraged_etf_futures_update(&mut g, &mut ui, &mut rng, &wsr_core::logic::newgame::LocalTime::HARNESS);
        check(&g, &rng, &ui, &o, &format!("seed {seed}"));
        for e in [1584, 1585] {
            if g0.entity_status[e] == 2 && g.entity_status[e] == 1 {
                hits.hit("restarted");
            }
        }
        if g.futures_recs.as_slice() != g0.futures_recs.as_slice() {
            hits.hit("futures");
        }
        for n in &ui.news {
            for (k, pat) in [
                (__content.string(10148), __content.string(13615)),
                (__content.string(10148), __content.string(13616)),
                (__content.string(10148), __content.string(13617)),
                ("new_stock", __content.string(13618)),
                ("ceased", __content.string(13619)),
                ("embattled", __content.string(13620)),
            ] {
                if n.contains(pat) {
                    hits.hit(k);
                }
            }
        }
        for e in [1584, 1585] {
            if g0.entity_status[e] == 1 && g.entity_status[e] != 1 {
                hits.hit(__content.string(13621));
            }
        }
    });
    eprintln!("leveraged ETF branches: {:?}", hits.0.lock().unwrap());
    hits.require(&["restarted", "futures", __content.string(10148), "new_stock", "embattled", "ceased"]);
}

const BUY_OR_COVER: u32 = 0x61cebb;
const SELL_OR_SHORT: u32 = 0x620f39;
/// DisableMainControls (0x4242c6): UI only.
const SKIP_DISABLE_CONTROLS: (u32, u32) = (0x4242c6, 0);

/// Closing trades of the commodity desk (`tran_type` 2): buying back shorts and selling longs.
#[test]
fn diff_futures_close_positions() {
    let hits = Hits::default();
    par_seeds(0..64u64, |seed| {
        let (mut g, owners) = cworld(seed + 1400);
        let mut r = Rand(seed + 53);
        let cover = seed % 2 == 0;
        let e = owners[cast::<usize, _>(r.range(0, 4))];
        let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
        g.cur_commodity = st(c);
        let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
        let k = n + 1;
        let qty: i64 = [1, 1, 2, 5, 30][cast::<usize, _>(r.range(0, 4))] * if cover { -1 } else { 1 };
        g.futures_recs[k] = st(&format!(
            "{c}|{e}|{qty}|{}|{}|{:.4}|{:.2}",
            r.range(1, 12),
            g.cur_year + cast::<i32, _>(r.range(0, 2)),
            f64r(&mut r, Ext::from(Ext::from_f64(10.0)), Ext::from(Ext::from_f64(3000.0))),
            f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))
        ));
        g.futures_recs[0] = st(&k.to_string());
        let rec = if seed % 9 == 4 { k + 3 } else { k };
        let input = format!("{}", r.range(0, qty.abs() + 1));
        let answers: Vec<&str> = if qty.abs() == 1 {
            vec![if r.chance(Ext::from_f64(0.8)) { "IDOK" } else { "IDCANCEL" }]
        } else {
            vec![&input]
        };
        let tran = if seed % 13 == 6 { 3 } else { 2 };
        let addr = if cover { BUY_OR_COVER } else { SELL_OR_SHORT };
        let args = [tran.to_string(), e.to_string(), rec.to_string()];
        let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let rng_start = rng0(seed);
        let Some(o) = run(addr, &argv, &g, rng_start, &answers, &[SKIP_DISABLE_CONTROLS]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Io::new(&answers);
        let mut rng = Rng::from_state(rng_start);
        if cover {
            futures_buy_or_cover(&mut g, &mut ui, &mut rng, tran, e, rec);
        } else {
            futures_sell_or_short(&mut g, &mut ui, &mut rng, tran, e, rec);
        }
        check(&g, &rng, &ui.rec, &o, &format!("seed {seed} {c} qty {qty}"));
        if g.futures_recs[k] != g0.futures_recs[k] {
            hits.hit(if g.futures_recs[0] != g0.futures_recs[0] { "closed" } else { "partial" });
        }
    });
    hits.require(&["closed", "partial"]);
}
