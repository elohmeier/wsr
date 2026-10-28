//! Differential tests of `logic::commod_ai::limits`: CalcFuturesPositionRoom, ApplyFuturesTradeImpact,
//! CalcCommodTradeLimit and EnforceCommodityPositionLimits against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use wsr_core::logic::commod_ai::limits::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;

const POSITION_ROOM: u32 = 0x634bd2;
const TRADE_IMPACT: u32 = 0x6343f9;
const TRADE_LIMIT: u32 = 0x636c54;
const ENFORCE_LIMITS: u32 = 0x639a64;

#[test]
fn diff_calc_futures_position_room() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..48u64, |seed| {
        let (mut g, owners) = cworld(seed + 100);
        let mut r = Rand(seed + 5);
        // big positions so that the caps bind
        let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
        for i in 1..=n {
            if r.chance(Ext::from_f64(0.3)) {
                let rec = g.futures_recs[i].to_display();
                let f: Vec<&str> = rec.split('|').collect();
                let big = r.range(1000, 60000) * if f[2].starts_with('-') { -1 } else { 1 };
                g.futures_recs[i] = st(&format!("{}|{}|{}|{}", f[0], f[1], big, f[3..].join("|")));
            }
        }
        g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
        let e = owners[cast::<usize, _>(r.range(0, 9))];
        let commod = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "STOCKS", "BITCOIN"][cast::<usize, _>(seed % 7)];
        let action = ["BUY", "SELL", ""][cast::<usize, _>(seed / 7 % 3)];
        let args = [e.to_string(), format!("{__wsr_text_13622}{commod}",__wsr_text_13622=__content.string(13622)), format!("{__wsr_text_13622}{action}",__wsr_text_13622=__content.string(13622))];
        let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let Some(o) = run(POSITION_ROOM, &argv, &g, 1, &[], &[]) else {
            return;
        };
        let room = calc_futures_position_room(&mut g, e, &st(commod), &st(action));
        assert_eq!(room, cast::<i32, _>(o.return_value), "seed {seed}");
        cmp().assert(&g, &o.globals);
        hits.hit(if room == 0 { "zero" } else { "room" });
    });
    hits.require(&["zero", "room"]);
}

#[test]
fn diff_apply_futures_trade_impact() {
    let __content = wsr_core::content::current();
    par_seeds(0..40u64, |seed| {
        let (mut g, _) = cworld(seed + 200);
        let mut r = Rand(seed + 9);
        let commod = if seed == 39 { "XYZ" } else { COMMODS[cast::<usize, _>(seed % 8)] };
        let dir = if r.chance(Ext::from_f64(0.5)) { 1 } else { -1 };
        let qty: i64 = [5, 150, 2000, 40000, -300][cast::<usize, _>(r.range(0, 4))];
        if seed % 5 == 0 {
            g.oil_target_price = cast::<f32, _>(Ext::from_f64(0.16)); // re-seeded below 0.15 after a sale
        }
        let rng_start = rng0(seed);
        let args = [format!("{__wsr_text_13622}{commod}",__wsr_text_13622=__content.string(13622)), dir.to_string(), format!("q:{qty}")];
        let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let Some(o) = run(TRADE_IMPACT, &argv, &g, rng_start, &[], &[]) else {
            return;
        };
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        apply_futures_trade_impact(&mut g, &mut ui, &mut rng, &st(commod), dir, qty);
        check(&g, &rng, &ui, &o, &format!("seed {seed} {commod}"));
    });
}

#[test]
fn diff_calc_commod_trade_limit() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..96u64, |seed| {
        let (mut g, owners) = cworld(seed + 300);
        let mut r = Rand(seed + 13);
        let commod = COMMODS[cast::<usize, _>(seed % 8)];
        g.cur_commodity = st(commod);
        let hold_kind = cast::<i32, _>(seed / 8 % 2);
        g.cur_player = 1;
        g.is_computer_player[1] = if seed % 11 == 0 { 1 } else { 0 };
        let e = if r.chance(Ext::from_f64(0.5)) { 1 } else { owners[4] }; // player 1 or a corp it controls
        g.ai_target_corp = if seed % 13 == 0 { e } else { 0 };
        // group / global positions of various sizes in this commodity
        let scale = [0, 1, 10, 100, 1000, 10000][cast::<usize, _>(r.range(0, 5))];
        let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
        let mut k = n;
        for _ in 0..r.range(0, 4) {
            k += 1;
            let o = if r.chance(Ext::from_f64(0.6)) { e } else { owners[cast::<usize, _>(r.range(0, 9))] };
            g.futures_recs[k] = st(&format!("{commod}|{o}|{}|6|{}|100|10", r.range(1, 40) * scale, g.cur_year + 1));
        }
        g.futures_recs[0] = st(&k.to_string());
        let pn: i32 = g.phys_commod_rec[0].to_display().parse().unwrap();
        let mut k = pn;
        for _ in 0..r.range(0, 3) {
            k += 1;
            let o = if r.chance(Ext::from_f64(0.6)) { e } else { owners[cast::<usize, _>(r.range(0, 9))] };
            g.phys_commod_rec[k] = st(&format!("{commod}|{o}|{}|100", r.range(1, 40) * scale));
        }
        g.phys_commod_rec[0] = st(&k.to_string());
        g.tmp_ext1 = [Ext::from_f64(0.0), Ext::from_f64(0.0), Ext::from_f64(99.0), -Ext::from_f64(99.0)][cast::<usize, _>(r.range(0, 3))];
        let req: i64 = [1, 50, 500, 5000, 100000][cast::<usize, _>(r.range(0, 4))];
        let args = [e.to_string(), hold_kind.to_string(), format!("q:{req}")];
        let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let Some(o) = run(TRADE_LIMIT, &argv, &g, 1, &[], &[]) else {
            return;
        };
        let mut ui = Rec::default();
        let v = calc_commod_trade_limit(&mut g, &mut ui, e, hold_kind, req);
        assert_eq!(Some(cast::<Ext, _>(v)), o.return_float, "seed {seed} {commod}{__wsr_text_13623}{hold_kind}",__wsr_text_13623=__content.string(13623));
        check(&g, &Rng::from_state(o.rng), &ui, &o, &format!("seed {seed} {commod}"));
        hits.hit(match v {
            99 => "probe99",
            -99 => "probe-99",
            0 => "zero",
            x if x == req => "full",
            _ => "limited",
        });
        if !ui.msgs.is_empty() {
            hits.hit("msg");
        }
    });
    hits.require(&["probe99", "probe-99", "zero", "full", "limited", "msg"]);
}

/// A world where players and their companies hold too many contracts/units of the checked commodity.
fn enforce_world(seed: u64) -> (wsr_core::state::Globals, usize) {
    let (mut g, owners) = cworld(seed + 400);
    let mut r = Rand(seed + 17);
    let idx = cast::<usize, _>(seed % 8);
    g.sim_co_count = 250 * cast::<i32, _>(r.range(0, 5)) + cast::<i32, _>(idx) + 1;
    let commod = COMMODS[idx];
    let limits = [(15000, 175000), (5000, 8000), (20000, 10000), (10000, 25000), (10000, 40000), (0, 5000), (2000, 2000), (5000, 5000)];
    let (storage, position) = limits[idx];
    g.player_name[2] = st(if seed % 9 == 4 { "...OUT..." } else { "Player Number 2" });
    let n: i32 = g.futures_recs[0].to_display().parse().unwrap();
    let mut k = n;
    for _ in 0..r.range(1, 4) {
        k += 1;
        let o = if r.chance(Ext::from_f64(0.5)) { 1 } else { owners[4] };
        let q = (cast::<i64, _>(position) / 2 + r.range(0, cast::<i64, _>(position))) * if r.chance(Ext::from_f64(0.3)) { -1 } else { 1 };
        g.futures_recs[k] = st(&format!("{commod}|{o}|{q}|{}|{}|{:.2}|{:.2}", r.range(1, 12), g.cur_year + 1, f64r(&mut r, Ext::from(Ext::from_f64(100.0)), Ext::from(Ext::from_f64(5000.0))), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
    }
    g.futures_recs[0] = st(&k.to_string());
    if storage > 0 {
        let pn: i32 = g.phys_commod_rec[0].to_display().parse().unwrap();
        let mut k = pn;
        for o in [1, owners[4]] {
            if (1..=pn).any(|i| g.phys_commod_rec[i].to_display().starts_with(&format!("{commod}|{o}|"))) {
                continue;
            }
            k += 1;
            g.phys_commod_rec[k] = st(&format!("{commod}|{o}|{}|{:.2}", cast::<i64, _>(storage) / 2 + r.range(0, cast::<i64, _>(storage)), f64r(&mut r, Ext::from(Ext::from_f64(10.0)), Ext::from(Ext::from_f64(900.0)))));
        }
        g.phys_commod_rec[0] = st(&k.to_string());
    }
    g.cur_player = if r.chance(Ext::from_f64(0.7)) { 1 } else { 2.min(g.num_players) };
    (g, idx)
}

#[test]
fn diff_enforce_commodity_position_limits() {
    let hits = Hits::default();
    par_seeds(0..40u64, |seed| {
        let (mut g, idx) = enforce_world(seed);
        let rng_start = rng0(seed);
        let Some(o) = run(ENFORCE_LIMITS, &[], &g, rng_start, &[], &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        enforce_commodity_position_limits(&mut g, &mut ui, &mut rng);
        check(&g, &rng, &ui, &o, &format!("seed {seed}"));
        if g.futures_recs[0] != g0.futures_recs[0] || g.futures_recs.as_slice() != g0.futures_recs.as_slice() {
            hits.hit("futures");
        }
        if g.phys_commod_rec.as_slice() != g0.phys_commod_rec.as_slice() {
            hits.hit(if idx >= 6 { "crypto" } else { "phys" });
        }
        if !ui.msgs.is_empty() {
            hits.hit("msg");
        }
    });
    hits.require(&["futures", "phys", "crypto", "msg"]);
    let _ = PbStr::new();
}
