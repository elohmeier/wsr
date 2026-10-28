//! ShowFuturesPositions (0x62cbaf), ListPhysCommodHoldings (0x65c08f), ConsolidatePhysCommodRecs (0x665a58).
use wsr_core::pb::real::{Ext, cast};


use super::common::{Rec, assert_reports, cmp, names, orig_msgs, orig_reports, port_reports, run, st};
use super::diff::Rand;
use wsr_core::logic::reports_corp::consolidate_phys_commod_recs;
use wsr_core::logic::reports_lists::{list_phys_commod_holdings, show_futures_positions};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SHOW_FUTURES_POSITIONS: u32 = 0x62cbaf;
const LIST_PHYS_COMMOD_HOLDINGS: u32 = 0x65c08f;
const CONSOLIDATE_PHYS_COMMOD_RECS: u32 = 0x665a58;

const COMMODS: [&str; 8] = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN", "ETHEREUM"];

/// A plausible world with commodity prices, futures bases, futures records and physical holdings
/// (records on a few owners: `owners[0..8]`).
fn world(seed: u64) -> (Globals, Vec<i32>) {
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    names(&mut g, &mut r);
    let f32r = |r: &mut Rand, lo: Ext, hi: Ext| cast::<f32, _>(lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000)) / Ext::from_f64(1e6));
    let f64r = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.game_end_flag = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { "Y" } else { "N" });
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.start_year = cast::<i32, _>(r.range(1990, 2010));
    g.cur_year = g.start_year + cast::<i32, _>(r.range(0, 40));
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = [0, 3, 200, 300, 600, 800, 1100, 1300, 1600][cast::<usize, _>(r.range(0, 8))];
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(18.0)));
    g.exchange_rate = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.oil_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(0.3), Ext::from_f64(3.0)));
    g.gold_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(25000.0)));
    g.silver_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(400.0)));
    g.wheat_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0)));
    g.corn_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(19.0)));
    g.bitcoin_price = cast::<i32, _>(r.range(100, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(10, 5000));
    g.stock_index = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(160000.0));
    g.metals_price_base = cast::<i32, _>(r.range(0, 9000));
    g.rand_seed = cast::<i32, _>(r.range(-100000, 100000));
    for k in 1..=10 {
        g.futures_basis[k] = cast::<i16, _>(r.range(-120, 320));
    }
    g.gold_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(300.0), Ext::from_f64(25000.0)));
    g.silver_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(4.0), Ext::from_f64(400.0)));
    g.wheat_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(20.0)));
    g.corn_target_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(19.0)));
    g.cur_commodity = st(COMMODS[cast::<usize, _>(r.range(0, 7))]);
    g.currency_name = st(["Dollars", "Yen", "Euros", "Pound"][cast::<usize, _>(r.range(0, 3))]);
    g.currency_suffix = st(["", "", " Pound", " SwFr", " Dollar"][cast::<usize, _>(r.range(0, 4))]);
    g.million_prefix_lc = st(["m", "b"][cast::<usize, _>(r.range(0, 1))]);
    for p in 1..=5 {
        g.is_computer_player[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
    }
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(cast::<f64, _>(Ext::from_f64(0.95))) };
        g.entity_status[e] = if active { 1 } else { 0 };
        g.commod_margin_bal[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0)) } else { Ext::from_f64(0.0) };
    }
    let owners: Vec<i32> = (0..8)
        .map(|k| cast::<i32, _>(match k % 4 {
            0 => r.range(1, cast::<i64, _>(g.num_players)),
            1 => r.range(131, 1580),
            2 => r.range(11, 130),
            _ => r.range(131, 1580),
        }))
        .collect();
    let n = cast::<i32, _>(r.range(0, 60));
    for i in 1..=n {
        let c = COMMODS[cast::<usize, _>(r.range(0, 7))];
        let owner = owners[cast::<usize, _>(r.range(0, 7))];
        let qty = r.range(1, 60) * if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { -1 } else { 1 };
        let year = g.cur_year + cast::<i32, _>(r.range(0, 3));
        let cost = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(2000.0));
        let price = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(99999.0)) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(999.0)) };
        g.futures_recs[i] = st(&format!("{c}|{owner}|{qty}|{}|{year}|{cost:.4}|{price:.3}", r.range(1, 12)));
    }
    g.futures_recs[0] = st(&n.to_string());
    let n = cast::<i32, _>(r.range(0, 30));
    for i in 1..=n {
        let c = ["GOLD", "SILVER", "OIL", "WHEAT", "CORN", "BITCOIN", "ETHEREUM", "GOLD"][cast::<usize, _>(r.range(0, 7))];
        let owner = owners[cast::<usize, _>(r.range(0, 7))];
        g.phys_commod_rec[i] = st(&format!("{c}|{owner}|{}|{:.2}", r.range(1, 50000), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    g.phys_commod_rec[0] = st(&n.to_string());
    // SelEntity stays 0: with a selected entity ShowReportDialog runs UpdateMarketDisplay, whose market
    // tick re-values every company; on this minimal world (no balance sheets) the original's CalcStockPrice
    // and CalcEntityFinancials_Totals hit runtime errors whose handlers (ShowRuntimeError) the ports don't
    // model. ActiveEntity 0 makes `ActiveEntity = SelEntity` true (mode 1 -> 4).
    g.active_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 0 } else { owners[cast::<usize, _>(r.range(0, 7))] };
    (g, owners)
}

fn has_futures(g: &Globals, e: i32) -> bool {
    (1..=1000).any(|i| !g.futures_recs[i].is_empty() && g.futures_recs[i].to_display().split('|').nth(1) == Some(&e.to_string()))
}

fn has_phys(g: &Globals, e: i32) -> bool {
    (1..=1000).any(|i| !g.phys_commod_rec[i].is_empty() && g.phys_commod_rec[i].to_display().split('|').nth(1) == Some(&e.to_string()))
}

/// MSGBOX answer as (harness name, Ui result).
fn answer(yes: bool) -> (&'static str, i32) {
    if yes { ("IDYES", 6) } else { ("IDNO", 7) }
}

#[test]
fn show_futures_positions_matches_original() {
    let __content = wsr_core::content::current();
    let mut reports = 0;
    let mut cases = 0;
    let mut quick = 0;
    for seed in 1u64..=14 {
        let (g0, owners) = world(seed * 7717);
        let mut r = Rand(seed * 31337);
        for k in 0..4 {
            let mut g = g0.clone();
            let mut ent = owners[cast::<usize, _>(r.range(0, 7))];
            if k == 3 {
                ent = cast::<i32, _>(r.range(11, 1600)); // usually nothing at all
            }
            if k == 2 && seed % 3 == 0 {
                // no futures but physical holdings: offers the physical list
                for i in 1..=1000 {
                    if g.futures_recs[i].to_display().split('|').nth(1) == Some(&ent.to_string()) {
                        g.futures_recs[i] = PbStr::new();
                    }
                }
                if !has_phys(&g, ent) {
                    g.phys_commod_rec[29] = st(&format!("SILVER|{ent}|1234|55.5"));
                    g.phys_commod_rec[0] = st("29");
                }
            }
            if seed == 5 && k == 1 {
                g.entity_status[ent] = 2; // defunct
            }
            let mode = cast::<i32, _>(r.range(0, 4));
            if k == 2 && seed % 2 == 0 {
                // the "first in mode 2/3" shortcut: only one matching record
                g.cur_commodity = st(COMMODS[cast::<usize, _>(r.range(0, 7))]);
            }
            let (ans, ans_ui) = answer(r.chance(cast::<f64, _>(Ext::from_f64(0.6))));
            let rng0 = cast::<u32, _>(r.next());
            let Some(o) = run(SHOW_FUTURES_POSITIONS, &[&format!("v:{ent}"), &format!("{__wsr_text_13741}{mode}",__wsr_text_13741=__content.string(13741))], &g, rng0, &[ans, ans])
            else {
                return;
            };
            let mut port = g.clone();
            let mut ui = Rec::default();
            ui.answers.push_back(ans_ui);
            ui.answers.push_back(ans_ui);
            let mut rng = Rng::from_state(rng0);
            let res = show_futures_positions(&mut port, &mut ui, &mut rng, ent, mode);
            let orig = orig_reports(&o);
            reports += orig.len();
            cases += 1;
            if orig.is_empty() {
                quick += 1;
            }
            eprintln!(
                "seed {seed}/{k}: ent {ent}{__wsr_text_13548}{mode}{__wsr_text_2520}{} phys {}: {}{__wsr_text_13751}{} msgs, result {}",
                has_futures(&g, ent),
                has_phys(&g, ent),
                orig.len(),
                orig_msgs(&o),
                o.return_value
            ,__wsr_text_13548=__content.string(13548), __wsr_text_2520=__content.string(2520), __wsr_text_13751=__content.string(13751));
            assert_reports(&port_reports(&ui, &[]), &orig);
            assert_eq!(res, cast::<i32, _>(o.return_value), "result");
            assert_eq!(rng.state(), o.rng, "RND state");
            assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}, log {:?}", ui.msgs, o.log);
            cmp().assert(&port, &o.globals);
        }
    }
    eprintln!("{cases} cases, {reports}{__wsr_text_13751}{quick} without report",__wsr_text_13751=__content.string(13751));
    assert!(reports > 10);
}

#[test]
fn list_phys_commod_holdings_matches_original() {
    let __content = wsr_core::content::current();
    let mut reports = 0;
    for seed in 1u64..=10 {
        let (mut g, owners) = world(seed * 104729);
        let mut r = Rand(seed * 271);
        if seed % 4 == 0 {
            // an unknown commodity name: error message boxes and early exit
            g.phys_commod_rec[30] = st(&format!("PLATINUM|{}|10|5", owners[1]));
            g.phys_commod_rec[0] = st("30");
        }
        let ent = if seed % 4 == 0 {
            owners[1]
        } else if seed % 5 == 0 {
            cast::<i32, _>(r.range(11, 1600))
        } else {
            owners[cast::<usize, _>(r.range(0, 7))]
        };
        let sel = cast::<i32, _>(r.range(0, 1));
        // YES to " [runtime resource wording] " would run SelectActingEntity (not ported)
        let (ans, ans_ui) = answer(r.chance(cast::<f64, _>(Ext::from_f64(0.5))) && (has_phys(&g, ent) || has_futures(&g, ent)));
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(LIST_PHYS_COMMOD_HOLDINGS, &[&format!("v:{ent}"), &format!("v:{sel}")], &g, rng0, &[ans, ans, ans])
        else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Rec::default();
        for _ in 0..3 {
            ui.answers.push_back(ans_ui);
        }
        let mut rng = Rng::from_state(rng0);
        let res = list_phys_commod_holdings(&mut port, &mut ui, &mut rng, ent, sel);
        let orig = orig_reports(&o);
        reports += orig.len();
        eprintln!("seed {seed}: ent {ent} sel {sel}: {}{__wsr_text_13751}{} msgs", orig.len(), orig_msgs(&o),__wsr_text_13751=__content.string(13751));
        assert_reports(&port_reports(&ui, &[]), &orig);
        assert_eq!(res, cast::<i32, _>(o.return_value), "result");
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}, log {:?}", ui.msgs, o.log);
        cmp().assert(&port, &o.globals);
    }
    assert!(reports > 3);
}

#[test]
fn consolidate_phys_commod_recs_matches_original() {
    for seed in 1u64..=6 {
        let (mut g, owners) = world(seed * 15485863);
        // several records of the same commodity on one company
        let corp = owners[1];
        for i in 31..=40 {
            let c = ["GOLD", "OIL", "GOLD", " GOLD ", "CORN"][cast::<usize, _>(i % 5)];
            g.phys_commod_rec[i] = st(&format!("{c}|{corp}|{}|{}.25", i * 7, i * 3));
        }
        g.phys_commod_rec[0] = st("40");
        for ent in [corp, owners[0], owners[2]] {
            let Some(o) = run(CONSOLIDATE_PHYS_COMMOD_RECS, &[&ent.to_string()], &g, 1, &[]) else { return };
            let mut port = g.clone();
            consolidate_phys_commod_recs(&mut port, ent);
            cmp().assert(&port, &o.globals);
        }
    }
}

#[test]
fn consolidate_merges_pairwise() {
    let mut g = Globals::default();
    g.phys_commod_rec[1] = st("GOLD|200|10|1.5");
    g.phys_commod_rec[2] = st("OIL|200|5|2");
    g.phys_commod_rec[3] = st("GOLD|200|20|3");
    g.phys_commod_rec[4] = st("GOLD|200|30|4");
    g.phys_commod_rec[5] = st("GOLD|5|30|4");
    consolidate_phys_commod_recs(&mut g, 200);
    // pass 1: 1+3, then 4 stays (3 is gone); pass 2: 1+4
    assert_eq!(g.phys_commod_rec[1], "GOLD|200|60|8.5");
    assert_eq!(g.phys_commod_rec[2], "OIL|200|5|2");
    assert!(g.phys_commod_rec[3].is_empty() && g.phys_commod_rec[4].is_empty());
    assert_eq!(g.phys_commod_rec[5], "GOLD|5|30|4");
    // players are never consolidated
    let mut h = Globals::default();
    h.phys_commod_rec[1] = st("GOLD|2|10|1");
    h.phys_commod_rec[2] = st("GOLD|2|10|1");
    consolidate_phys_commod_recs(&mut h, 2);
    assert_eq!(h.phys_commod_rec[2], "GOLD|2|10|1");
}
