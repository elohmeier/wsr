//! Differential tests of `logic::sim_entity::mgmt`: CheckMgmtChange, ForceCoverShorts and
//! CommodityMarginCall against the original routines.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::mgmt::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const CHECK_MGMT_CHANGE: u32 = 0x4ef5eb;
const FORCE_COVER_SHORTS: u32 = 0x5dfe3a;
const COMMODITY_MARGIN_CALL: u32 = 0x632988;

/// Port result (globals, RND state, message boxes) vs. the original's; panics on differences.
#[track_caller]
fn check(what: &str, port: &Globals, port_rng: &Rng, ui: &Rec, o: &Original) {
    let d = cmp().diffs(port, &o.globals);
    let log: Vec<&String> = o
        .log
        .iter()
        .filter(|l| !l.starts_with("API"))
        .take(20)
        .collect();
    assert!(
        d.is_empty(),
        "{what}: {} differences:\n{}\nlog: {log:?}",
        d.len(),
        d.iter().take(30).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(port_rng.state(), o.rng, "{what}: RND state");
    let errs: Vec<&String> = o.log.iter().filter(|l| l.starts_with("PBERR")).collect();
    assert!(
        errs.is_empty(),
        "{what}: runtime errors in the original: {errs:?}"
    );
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?} vs log {log:?}",
        ui.msgs
    );
}

fn active_corp(g: &Globals, r: &mut Rand) -> i32 {
    for _ in 0..100 {
        let c = cast::<i32, _>(r.range(131, 1580));
        if g.entity_status[c] == 1 && g.industry_num[c] < 70 {
            return c;
        }
    }
    131
}

// ----------------------------------------------------------------------------------- CheckMgmtChange

#[test]
fn diff_check_mgmt_change() {
    let __content = wsr_core::content::current();
    par_seeds(1..=16, |seed| {
        let mut r = Rand(seed * 7349 + 11);
        let mut g = world(seed + 900);
        let c = match seed % 8 {
            0 => cast::<i32, _>(r.range(1, 10)),
            1 => cast::<i32, _>(r.range(11, 1600)), // any (inactive / fund / industry 70)
            _ => active_corp(&g, &mut r),
        };
        let others: Vec<i32> = (0..r.range(0, 4))
            .map(|_| cast::<i32, _>(r.range(11, 1600)))
            .collect();
        let mut list = format!("{c:04}/");
        for o in &others {
            list += &format!("{o:04}/");
        }
        if seed % 7 == 3 {
            list = format!("{c:04}");
        }
        g.new_mgmt_list = st(&list);
        let n_recent = [0, 1, 2, 5, 12, 25][cast::<usize, _>(r.range(0, 5))];
        let mut recent = String::new();
        for k in 0..n_recent {
            let id = if k == 1 && seed % 3 == 0 {
                c
            } else {
                cast::<i32, _>(r.range(11, 1600))
            };
            recent += &format!("{id:04}/");
        }
        g.recent_mgmt_change_list = st(&recent);
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.corp_controller[c] = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
            g.cur_player = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.corp_controller[c]
            } else {
                1
            };
        }
        g.profit_margin[c] = [-20, -13, -6, 0, 5, 8, 20][cast::<usize, _>(r.range(0, 6))];
        g.rand_seed = cast::<i32, _>(r.range(0, 32000));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.news_msg = st("Earlier news item");
        }
        let rs = cast::<u32, _>(r.next());
        let Some(o) = run(CHECK_MGMT_CHANGE, &[], &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        check_mgmt_change(&mut port, &mut ui, &mut rng);
        check(
            &format!("CheckMgmtChange seed {seed} corp {c}{__wsr_text_13995}{list:?}",__wsr_text_13995=__content.string(13995)),
            &port,
            &rng,
            &ui,
            &o,
        );
    });
}

// ---------------------------------------------------------------------------------- ForceCoverShorts

/// Short positions of the players in `corp` (and some others).
fn add_shorts(g: &mut Globals, r: &mut Rand, corp: i32) {
    for p in 1..=g.num_players {
        g.player_short_margin_bal[p] = f64r(r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        g.player_cap_gains[p] = f64r(r, -Ext::from_f64(100.0), Ext::from_f64(500.0));
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
                let id = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                    corp
                } else {
                    cast::<i32, _>(r.range(131, 1580))
                };
                if (1..=15).any(|s| cast::<i32, _>(g.hold_stock_id[(s, p)]) == id) {
                    continue;
                }
                g.hold_stock_id[(slot, p)] = cast::<i16, _>(id);
                let pct = [1, 2, 5, 15, 22, 30][cast::<usize, _>(r.range(0, 5))];
                g.hold_stock_pct[(slot, p)] = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { -pct } else { pct };
                g.hold_stock_basis[(slot, p)] = f64r(r, Ext::from_f64(1.0), Ext::from_f64(900.0));
            }
        }
        g.hold_stock_id[(0, p)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, p)] != 0).count());
    }
}

#[test]
fn diff_force_cover_shorts() {
    let __content = wsr_core::content::current();
    par_seeds(1..=20, |seed| {
        let mut r = Rand(seed * 6151 + 5);
        let mut g = world(seed + 1000);
        let corp = active_corp(&g, &mut r);
        add_shorts(&mut g, &mut r, corp);
        if seed % 10 == 0 {
            g.player_name[g.num_players] = st("...OUT...");
        }
        let mode = cast::<i32, _>(seed % 5 + 1);
        let target = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            0
        } else {
            cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)))
        };
        let target = if mode == 3 || mode == 4 {
            target.max(1)
        } else {
            target
        };
        let pct = cast::<i32, _>(r.range(1, 6));
        let rs = cast::<u32, _>(r.next());
        let args = [
            format!("{__wsr_text_13741}{mode}",__wsr_text_13741=__content.string(13741)),
            format!("{__wsr_text_13741}{target}",__wsr_text_13741=__content.string(13741)),
            format!("{__wsr_text_13741}{corp}",__wsr_text_13741=__content.string(13741)),
            format!("v:{pct}"),
        ];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(FORCE_COVER_SHORTS, &args, &g, rs, &[], &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        force_cover_shorts(&mut port, &mut ui, &mut rng, mode, target, corp, pct);
        check(
            &format!("ForceCoverShorts seed {seed}{__wsr_text_13548}{mode}{__wsr_text_13552}{target}",__wsr_text_13548=__content.string(13548), __wsr_text_13552=__content.string(13552)),
            &port,
            &rng,
            &ui,
            &o,
        );
    });
}

// ------------------------------------------------------------------------------- CommodityMarginCall

/// Futures records `CODE|owner|qty|month|year|cost|price`, every other one of `owner`.
fn add_futures(g: &mut Globals, r: &mut Rand, owner: i32, n: i32) {
    let codes = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS"];
    for i in 1..=n {
        let o = if i % 2 == 0 {
            owner
        } else {
            cast::<i32, _>(r.range(131, 1580))
        };
        let code = codes[cast::<usize, _>(r.range(0, 5))];
        let mut qty = cast::<i32, _>(r.range(1, 40));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            qty = -qty;
        }
        let month = r.range(1, 12);
        let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
        let cost = f64r(r, Ext::from_f64(10.0), Ext::from_f64(2000.0));
        let price = f64r(r, Ext::from_f64(2.0), Ext::from_f64(2000.0));
        g.futures_recs[i] = st(&format!(
            "{code}|{o}|{qty}|{month}|{year}|{cost:.2}|{price:.2}"
        ));
    }
    g.futures_recs[0] = st(&n.to_string());
}

#[test]
fn diff_commodity_margin_call() {
    let __content = wsr_core::content::current();
    use super::script::{Hits, check as script_check, seeds};
    let hits = Hits::default();
    let closed = std::sync::atomic::AtomicUsize::new(0);
    par_seeds(seeds(1..=40), |seed| {
        let mut r = Rand(seed * 4241 + 3);
        let mut g = world(seed + 1100);
        let id = if seed % 4 != 1 {
            cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)))
        } else {
            active_corp(&g, &mut r)
        };
        g.entity_status[id] = 1;
        if id <= g.num_players && seed % 8 != 7 {
            // the player is the current one: the futures list (of the acting entity) shows its positions
            g.cur_player = id;
            g.cur_player_name = g.player_name[id].clone();
            g.is_computer_player[id] = 0;
        }
        if seed % 6 != 5 {
            add_futures(&mut g, &mut r, id, 12);
        }
        g.cash[id] = match seed % 5 {
            0 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)),
            _ => -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(400.0)),
        };
        g.t_bills[id] = match seed % 3 {
            0 => Ext::from_f64(0.0),
            1 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0)),
            _ => Ext::from_f64(2000.0),
        };
        g.commod_margin_bal[id] = if seed % 7 == 6 {
            Ext::from_f64(0.0)
        } else {
            f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0))
        };
        g.tmp_subsid_stock_value = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0))
        } else {
            Ext::from_f64(0.0)
        };
        g.active_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
            id
        } else {
            cast::<i32, _>(r.range(131, 1580))
        };
        g.cur_commodity = st("GOLD");
        g.action_menu_open = 1;
        // the YES/NO (or OK) box, then the futures list: pick a position (closing it asks for the number
        // of contracts: all or a few), or none (forced liquidation)
        let mut answers: Vec<String> = vec![if seed % 2 == 0 { "IDYES" } else { "IDNO" }.to_string()];
        if seed % 3 != 0 {
            answers.push(format!("pick:{}", r.range(0, 11)));
            answers.push(r.range(1, 40).to_string());
            answers.push("IDYES".to_string());
        }
        let args = vec![id.to_string()];
        let rv = std::cell::Cell::new(0);
        let Some((ui, port, o)) = script_check(seed, COMMODITY_MARGIN_CALL, &args, &g, &answers, &[], cmp(), |p, ui, rng| {
            rv.set(commodity_margin_call(p, ui, rng, id, &wsr_core::logic::newgame::LocalTime::HARNESS));
        }) else {
            return;
        };
        let what = format!("CommodityMarginCall seed {seed} id {id}");
        assert_eq!(cast::<u32, _>(rv.get()), o.return_value, "{what}: result");
        if ui.picks.iter().any(|(_, i)| i.is_some()) && port.futures_recs.as_slice() != g.futures_recs.as_slice() {
            closed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        hits.record(&ui);
    });
    let n = closed.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("coverage margin call: position picked and closed: {n}");
    if unavailable().is_none() {
        assert!(n > 0, "no position closed");
    }
    hits.assert(&[__content.string(11036), __content.string(14166), __content.string(14167)]);
}
