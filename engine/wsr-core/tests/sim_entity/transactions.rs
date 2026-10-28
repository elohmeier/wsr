//! Differential tests of `logic::sim_entity::transactions`: SellStockHolding, ForcedSellStocks,
//! ForcedSellBonds, CoverCashDeficit, LiquidateFuturesPositions (with SettleFuturesContract and
//! BookFuturesGainLoss) and EtfDropOneAdviserContract against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::transactions::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SELL_STOCK_HOLDING: u32 = 0x4ed93b;
const FORCED_SELL_STOCKS: u32 = 0x4ff864;
const FORCED_SELL_BONDS: u32 = 0x500723;
const COVER_CASH_DEFICIT: u32 = 0x6625d7;
const LIQUIDATE_FUTURES: u32 = 0x62e730;
const SETTLE_FUTURES: u32 = 0x625ce5;
const BOOK_FUTURES_GAIN: u32 = 0x626b81;
const ETF_DROP_ADVISER: u32 = 0x64e6ba;

/// Seeds to run: `n`, or the comma-separated list in `$SEEDS` (debugging).
fn seeds(n: std::ops::RangeInclusive<u64>) -> Vec<u64> {
    match std::env::var("SEEDS") {
        Ok(v) => v.split(',').map(|x| x.parse().unwrap()).collect(),
        Err(_) => n.collect(),
    }
}

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x5bd1_e995
}

/// Compares state, RND state and the number of message boxes of a port run with the original's.
#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(o), "{what}: message boxes {:?} vs {:?}", ui.msgs, o.log);
}

/// A stock holding (slot, holder, stock) of an active holder in an active stock, if any.
fn holdings(g: &Globals, holders: std::ops::RangeInclusive<i32>) -> Vec<(i32, i32, i32)> {
    let mut v = Vec::new();
    for h in holders {
        if g.entity_status[h] != 1 {
            continue;
        }
        for slot in 1..=15 {
            let c = cast::<i32, _>(g.hold_stock_id[(slot, h)]);
            if c > 10 && g.entity_status[c] == 1 && g.hold_stock_pct[(slot, h)] > 0 && c != h {
                v.push((slot, h, c));
            }
        }
    }
    v
}

#[test]
fn diff_sell_stock_holding_explicit() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=12), |seed| {
        let mut g = world(seed);
        let mut r = Rand(seed * 7 + 1);
        let list = holdings(&g, 1..=1600);
        let (slot, seller, mut stock) = list[cast::<usize, _>(r.range(0, cast::<i64, _>(list.len()) - 1))];
        let mode = cast::<Ext, _>(seed % 3);
        if seed % 4 == 0 {
            g.hold_stock_basis[(slot, seller)] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
        }
        if seed % 5 == 0 {
            // a fund's shares (funds are 1581..1583, industry 71)
            stock = 1582;
            g.entity_status[stock] = 1;
            g.hold_stock_id[(slot, seller)] = cast::<i16, _>(stock);
        }
        g.tmp_ext1 = cast::<Ext, _>(seller);
        g.tmp_ext2 = if seed % 2 == 0 { cast::<Ext, _>(g.hold_stock_pct[(slot, seller)]) } else { cast::<Ext, _>(r.range(1, 40)) };
        g.tmp_ext3 = mode;
        g.news_msg = if seed % 3 == 1 { st(__content.string(13571)) } else { PbStr::new() };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        sell_stock_holding(&mut port, &mut ui, &mut rng, stock);
        let Some(o) = run(SELL_STOCK_HOLDING, &[&stock.to_string()], &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_13736}{seller}{__wsr_text_13936}{stock}{__wsr_text_13548}{mode}",__wsr_text_13736=__content.string(13736), __wsr_text_13936=__content.string(13936), __wsr_text_13548=__content.string(13548)));
    });
}

#[test]
fn diff_sell_stock_holding_random_pick() {
    par_seeds(seeds(1..=8), |seed| {
        let mut g = world(seed + 100);
        // make several stocks overvalued with a projected-earnings field above the projection
        let mut r = Rand(seed);
        for e in 131..=1580 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.proj_qtr_earnings[e] = -Ext::from_f64(50.0);
                g.market_cap[e] *= Ext::from_f64(3.0);
            }
        }
        g.cur_quarter = g.cur_year % 5;
        if g.cur_quarter == 0 {
            g.cur_quarter = 1;
            g.cur_year += 1;
        }
        g.tmp_ext3 = Ext::from_f64(2.0);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        sell_stock_holding(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = run(SELL_STOCK_HOLDING, &["0"], &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
    });
}

#[test]
fn diff_forced_sell_stocks() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=12), |seed| {
        let mut g = world(seed + 200);
        let mut r = Rand(seed * 3 + 5);
        let list = holdings(&g, 1..=1600);
        let (slot, seller, _) = list[cast::<usize, _>(r.range(0, cast::<i64, _>(list.len()) - 1))];
        g.cash[seller] = -f64r(&mut r, Ext::from_f64(1.0), if seed % 2 == 0 { Ext::from_f64(50.0) } else { Ext::from_f64(3000.0) });
        g.t_bills[seller] = if seed % 3 == 0 { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
        if seed % 4 == 1 {
            g.hold_stock_basis[(slot, seller)] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
            g.hold_stock_pct[(slot, seller)] = 81;
        }
        if seed % 5 == 2 {
            g.cur_player = seller;
            g.hold_stock_pct[(slot, seller)] = 100;
        }
        let mode = cast::<i32, _>(seed % 2);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = forced_sell_stocks(&mut port, &mut ui, &mut rng, seller, mode);
        let Some(o) = run(FORCED_SELL_STOCKS, &[&seller.to_string(), &mode.to_string()], &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed}{__wsr_text_13736}{seller}",__wsr_text_13736=__content.string(13736)));
        assert_eq!(Some(rv), o.return_str, "seed {seed}: sold list");
    });
}

#[test]
fn diff_forced_sell_bonds() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=16), |seed| {
        let mut g = world(seed + 300);
        let mut r = Rand(seed * 11 + 3);
        let id = match seed % 4 {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            1 => cast::<i32, _>(r.range(11, 85)),
            2 => cast::<i32, _>(r.range(86, 130)),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.entity_status[id] = 1;
        g.cash[id] = -f64r(&mut r, Ext::from_f64(1.0), if seed % 2 == 0 { Ext::from_f64(300.0) } else { Ext::from_f64(6000.0) });
        g.t_bills[id] = if seed % 3 == 0 { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20.0)) } else { Ext::from_f64(0.0) };
        if seed % 5 != 4 {
            g.long_gov_bond_face[id] = r.range(0, 3000);
            g.lt_gov_bond_cost[id] = cast::<Ext, _>(g.long_gov_bond_face[id]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
            g.short_gov_bond_face[id] = r.range(0, 2000);
            g.st_gov_bond_cost[id] = cast::<Ext, _>(g.short_gov_bond_face[id]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.05));
        }
        // corporate bonds held by this entity
        for slot in 1..=15 {
            g.hold_bond_id[(slot, id)] = 0;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                let b = cast::<i32, _>(r.range(131, 1580));
                g.bonds_issued[b] = g.bonds_issued[b].max(500);
                if g.bond_issue_info[b].is_empty() {
                    g.bond_issue_info[b] = st(&format!("{}     8.50", g.cur_year + 5));
                }
                g.bond_price[b] = wsr_core::pb::numeric::Cur::from_f64(f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(120.0)));
                g.hold_bond_id[(slot, id)] = b;
                g.hold_bond_face[(slot, id)] = r.range(1, 300);
                g.bond_hold_cost[(slot, id)] = cast::<Ext, _>(g.hold_bond_face[(slot, id)]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.1));
            }
        }
        let mode = cast::<i32, _>(seed % 3);
        let total_in = if seed % 4 == 3 { Ext::from_f64(12.0) } else { Ext::from_f64(0.0) };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = forced_sell_bonds(&mut port, &mut ui, &mut rng, id, mode, total_in, Ext::from_f64(0.0));
        let args = [id.to_string(), mode.to_string(), format!("{__wsr_text_14156}{total_in}",__wsr_text_14156=__content.string(14156)), "e:0".to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(FORCED_SELL_BONDS, &args, &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed} id {id}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)));
        let orig = o.return_float.unwrap_or(Ext::from_f64(0.0));
        assert!((rv - orig).abs() <= Ext::from_f64(1e-9) * rv.abs().max(Ext::from(Ext::from_f64(1.0))), "seed {seed}: result {rv} vs {orig}");
    });
}

/// Physical commodity records `" CODE|owner|qty|cost "` for some entities.
fn add_phys_commods(g: &mut Globals, r: &mut Rand, owner: i32) {
    let codes = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN"];
    let mut n = 0;
    for i in 1..=12 {
        let o = if i % 3 == 0 { owner } else { cast::<i32, _>(r.range(131, 1580)) };
        let code = codes[cast::<usize, _>(r.range(0, 4))];
        g.phys_commod_rec[i] = st(&format!(" {code}|{o}|{}|{:.2} ", r.range(1, 500), f64r(r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
        n = i;
    }
    g.phys_commod_rec[0] = st(&n.to_string());
}

#[test]
fn diff_cover_cash_deficit() {
    par_seeds(seeds(1..=10), |seed| {
        let mut g = world(seed + 400);
        let mut r = Rand(seed * 5 + 9);
        let id = if seed % 2 == 0 { cast::<i32, _>(r.range(131, 1580)) } else { cast::<i32, _>(r.range(86, 130)) };
        g.entity_status[id] = 1;
        if seed % 5 != 0 {
            add_phys_commods(&mut g, &mut r, id);
        }
        g.cash[id] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        g.t_bills[id] = match seed % 3 {
            0 => Ext::from_f64(0.0),
            1 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)),
            _ => Ext::from_f64(2000.0),
        };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = cover_cash_deficit(&mut port, &mut ui, &mut rng, id, &wsr_core::logic::newgame::LocalTime::HARNESS);
        let Some(o) = run(COVER_CASH_DEFICIT, &[&id.to_string()], &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed} id {id}"));
        assert_eq!(cast::<u32, _>(rv), o.return_value, "seed {seed}: result");
    });
}

/// The current (human) player with subsidiaries' stock: the interactive path (the player becomes the
/// acting entity, `SelectActingEntity(1)`; YES/NO, then a pick from the physical holdings, which is sold).
#[test]
fn diff_cover_cash_deficit_player() {
    let __content = wsr_core::content::current();
    use super::script::{Hits, check as script_check};
    let hits = Hits::default();
    par_seeds(seeds(1..=16), |seed| {
        let mut g = world(seed + 450);
        let mut r = Rand(seed * 13 + 5);
        let id = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        g.cur_player = id;
        g.cur_player_name = g.player_name[id].clone();
        g.is_computer_player[id] = 0;
        g.tmp_subsid_stock_value = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        g.active_entity = if seed % 2 == 0 { id } else { cast::<i32, _>(r.range(131, 1580)) };
        add_phys_commods(&mut g, &mut r, id);
        // an overdraft beyond the credit line
        g.cash[id] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)) - Ext::from_f64(100_000.0);
        g.t_bills[id] = if seed % 3 == 0 { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)) } else { Ext::from_f64(0.0) };
        let mut answers: Vec<String> = vec![if seed % 5 == 4 { "IDNO" } else { "IDYES" }.into()];
        answers.push(format!("pick:{}", r.range(0, 5)));
        answers.push(["100", "50", "IDYES", ""][cast::<usize, _>(r.range(0, 3))].into());
        let args = vec![id.to_string()];
        let rv = std::cell::Cell::new(0);
        let Some((ui, port, o)) = script_check(seed, COVER_CASH_DEFICIT, &args, &g, &answers, &[], cmp(), |p, ui, rng| {
            rv.set(cover_cash_deficit(p, ui, rng, id, &wsr_core::logic::newgame::LocalTime::HARNESS));
        }) else {
            return;
        };
        assert_eq!(cast::<u32, _>(rv.get()), o.return_value, "seed {seed}: result");
        if port.phys_commod_rec.as_slice() != g.phys_commod_rec.as_slice() {
            hits.add("sold");
        }
        hits.record(&ui);
    });
    hits.assert(&["sold", __content.string(14171)]);
}

/// Futures records `CODE|owner|qty|month|year|cost|price` for `owner` and some others.
fn add_futures(g: &mut Globals, r: &mut Rand, owner: i32, n: i32) {
    let codes = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS"];
    let mut k = 0;
    for i in 1..=n {
        let o = if i % 2 == 0 { owner } else { cast::<i32, _>(r.range(131, 1580)) };
        let code = codes[cast::<usize, _>(r.range(0, 5))];
        let mut qty = cast::<i32, _>(r.range(1, 40));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            qty = -qty;
        }
        let month = r.range(1, 12);
        let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
        let cost = f64r(r, Ext::from_f64(10.0), Ext::from_f64(2000.0));
        let price = f64r(r, Ext::from_f64(2.0), Ext::from_f64(2000.0));
        g.futures_recs[i] = st(&format!("{code}|{o}|{qty}|{month}|{year}|{cost:.2}|{price:.2}"));
        k = i;
    }
    g.futures_recs[0] = st(&k.to_string());
}

#[test]
fn diff_liquidate_futures_positions() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=16), |seed| {
        let mut g = world(seed + 500);
        let mut r = Rand(seed * 13 + 1);
        // (no insurers: CalcEntityFinancials of an insurer holding futures raises error 9 in the original,
        // which the finance port does not model)
        let id = match seed % 4 {
            0 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        g.entity_status[id] = 1;
        if seed % 3 == 0 {
            g.industry_num[id] = [7, 15, 50, 37, 28][cast::<usize, _>(r.range(0, 4))];
        }
        if seed % 8 != 7 {
            add_futures(&mut g, &mut r, id, 14);
        }
        g.cash[id] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
        let mode = [1, 2, 3, 4][cast::<usize, _>(seed % 4)];
        let news = cast::<i32, _>(seed % 2);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let rv = liquidate_futures_positions(&mut port, &mut ui, &mut rng, id, mode, news);
        let args = [id.to_string(), mode.to_string(), news.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(LIQUIDATE_FUTURES, &args, &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed} id {id}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)));
        assert_eq!(cast::<u32, _>(rv), o.return_value, "seed {seed}: result");
    });
}

#[test]
fn diff_settle_futures_and_book_gain() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=10), |seed| {
        let mut g = world(seed + 600);
        let mut r = Rand(seed * 17 + 3);
        let owner = cast::<i32, _>(r.range(131, 1580));
        add_futures(&mut g, &mut r, owner, 10);
        let rec = cast::<i32, _>(r.range(1, 10));
        let news = cast::<i32, _>(seed % 4);
        let no_comm = cast::<i32, _>(seed % 2);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        settle_futures_contract(&mut port, &mut ui, &mut rng, rec, news, no_comm, 0);
        let args = [rec.to_string(), news.to_string(), no_comm.to_string(), "0".to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(SETTLE_FUTURES, &args, &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("settle seed {seed}"));

        // BookFuturesGainLoss directly, all book modes
        let mut g = world(seed + 650);
        let e = if seed % 3 == 0 { 1 } else { cast::<i32, _>(r.range(131, 1580)) };
        g.entity_status[e] = 1;
        g.sel_entity = if seed % 2 == 0 { e } else { 0 };
        g.sim_co_count = if seed % 4 == 0 { e } else { 500 };
        g.futures_batch_gain = if seed % 5 == 0 { -Ext::from_f64(12.0) } else { Ext::from_f64(0.0) };
        let code = ["OIL", "GOLD", "STOCKS", "WHEAT", "BITCOIN"][cast::<usize, _>(seed % 5)];
        if seed % 2 == 1 {
            g.industry_num[e] = [7, 50, 37, 15, 3][cast::<usize, _>(seed % 5)];
        }
        let amt = f64r(&mut r, -Ext::from_f64(200.0), Ext::from_f64(200.0));
        let mode = cast::<i32, _>(seed % 5);
        let rs = rng0(seed + 1);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        book_futures_gain_loss(&mut port, &mut ui, &mut rng, amt, e, &st(code), mode);
        let args = [format!("e:{amt}"), e.to_string(), format!("{__wsr_text_13622}{code}",__wsr_text_13622=__content.string(13622)), mode.to_string()];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(BOOK_FUTURES_GAIN, &args, &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("book seed {seed}"));
    });
}

#[test]
fn diff_etf_drop_one_adviser_contract() {
    par_seeds(seeds(1..=6), |seed| {
        let mut g = world(seed + 700);
        let mut r = Rand(seed * 19 + 7);
        for f in 1581..=1600 {
            g.entity_status[f] = 1;
            g.industry_num[f] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { 71 } else { 70 };
            g.fund_adviser[f] = cast::<i32, _>(r.range(86, 140));
            g.fund_fee_pct[f] = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.2), Ext::from_f64(1.0)));
        }
        let adviser = g.fund_adviser[1581 + cast::<i32, _>(r.range(0, 19))];
        let flag = cast::<i32, _>(seed % 2);
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_drop_one_adviser_contract(&mut port, &mut ui, &mut rng, adviser, flag);
        let Some(o) = run(ETF_DROP_ADVISER, &[&adviser.to_string(), &flag.to_string()], &g, rs, &[], &[]) else { return };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
    });
}
