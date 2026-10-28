//! Differential tests of `logic::commod_ai::physical`: SellPhysCommod, BuyPhysCommod, AiTradePhysCommod
//! and ChargeCommodStorageCosts against the original.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use wsr_core::logic::commod_ai::physical::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const SELL_PHYS: u32 = 0x65db62;
const BUY_PHYS: u32 = 0x65f521;
const AI_TRADE_PHYS: u32 = 0x66393c;
const CHARGE_STORAGE: u32 = 0x661bd7;
/// BorrowToCoverPurchase (hooked: a player's purchase financed with a loan).
const BORROW: u32 = 0x5246b6;

/// Owners of physical records (field 2).
fn phys_owners(g: &Globals) -> Vec<(i32, i32)> {
    let n: i32 = g.phys_commod_rec[0].to_display().parse().unwrap_or(0);
    (1..=n)
        .filter(|&i| !g.phys_commod_rec[i].is_empty())
        .map(|i| (i, g.phys_commod_rec[i].to_display().split('|').nth(1).unwrap().parse().unwrap()))
        .collect()
}

#[test]
fn diff_sell_phys_commod() {
    let hits = Hits::default();
    par_seeds(0..48u64, |seed| {
        let (mut g, owners) = cworld(seed + 500);
        let mut r = Rand(seed + 21);
        let recs = phys_owners(&g);
        if recs.is_empty() {
            return;
        }
        let (rec, owner) = recs[cast::<usize, _>(r.range(0, cast::<i64, _>(recs.len()) - 1))];
        let mut e = owner;
        let mut rec_arg = rec;
        match seed % 8 {
            0 => {
                // by CurCommodity
                rec_arg = 0;
                g.cur_commodity = st(g.phys_commod_rec[rec].to_display().split('|').next().unwrap());
            }
            1 => e = owners[0], // maybe not the owner
            2 => {
                g.entity_status[e] = 2; // defunct: only its fate is shown
                g.ent_misc_data[e] = st(&format!("{}", r.range(131, 1580)));
            }
            3 => rec_arg = cast::<i32, _>(recs.len()) + 5, // beyond the list
            _ => {}
        }
        let silent = if seed % 3 == 0 { 0 } else { 2 };
        let pct: f32 = cast::<f32, _>([Ext::from_f64(0.0), Ext::from_f64(20.0), Ext::from_f64(50.0), Ext::from_f64(100.0), Ext::from_f64(1.0)][cast::<usize, _>(r.range(0, 4))]);
        let delivery = [0, 0, 1, -1][cast::<usize, _>(r.range(0, 3))];
        g.active_entity = e;
        g.active_entity_name = g.ent_name[e].clone();
        let input = format!("{}", r.range(1, 40));
        let mut answers: Vec<&str> = Vec::new();
        if silent == 0 {
            answers.push(&input);
        }
        let args = [e.to_string(), rec_arg.to_string(), silent.to_string(), format!("f:{pct}"), delivery.to_string()];
        let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let rng_start = rng0(seed);
        let Some(o) = run(SELL_PHYS, &argv, &g, rng_start, &answers, &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Io::new(&answers);
        let mut rng = Rng::from_state(rng_start);
        sell_phys_commod(&mut g, &mut ui, &mut rng, e, rec_arg, silent, pct, delivery);
        check(&g, &rng, &ui.rec, &o, &format!("seed {seed}"));
        if g.phys_commod_rec[0] != g0.phys_commod_rec[0] {
            hits.hit("removed");
        } else if g.phys_commod_rec.as_slice() != g0.phys_commod_rec.as_slice() {
            hits.hit(if silent == 0 { "partial_ui" } else { "partial" });
        }
    });
    hits.require(&["removed", "partial", "partial_ui"]);
}

#[test]
fn diff_buy_phys_commod() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(0..56u64, |seed| {
        let (mut g, owners) = cworld(seed + 600);
        let mut r = Rand(seed + 23);
        let commod = PHYS[cast::<usize, _>(seed % 7)];
        g.cur_commodity = st(commod);
        let ai_mode = [0, 1, 2][cast::<usize, _>(seed / 7 % 3)];
        // a player or an operating company (banks and insurers, industries 1 and 2, can't buy)
        let e = if ai_mode == 0 && r.chance(Ext::from_f64(0.4)) { 1 } else { [owners[1], owners[2], owners[4]][cast::<usize, _>(r.range(0, 2))] };
        if e > 130 {
            g.industry_num[e] = [7, 15, 37, 50, 2][cast::<usize, _>(r.range(0, 4))];
        }
        g.active_entity = e;
        g.active_entity_name = g.ent_name[e].clone();
        if seed % 5 == 0 {
            g.peak_oil_crisis = 1;
            g.oil_price = cast::<f32, _>(Ext::from_f64(3.0));
        }
        g.cash[e] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(4000.0));
        if ai_mode == 0 && seed % 2 == 0 {
            // short of cash: the purchase is financed (BorrowToCoverPurchase)
            g.cash[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1.0));
            g.t_bills[e] = Ext::from_f64(0.0);
        }
        g.cur_player = 1;
        let input = format!("{}", r.range(1, 6));
        let mut answers: Vec<&str> = Vec::new();
        if ai_mode == 0 {
            answers.push(&input);
            answers.push(if r.chance(Ext::from_f64(0.8)) { "IDYES" } else { "IDNO" });
        }
        let mut ui = Io::new(&answers);
        let args = [e.to_string(), ai_mode.to_string()];
        let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
        let rng_start = rng0(seed);
        let Some(o) = run_hooked(BUY_PHYS, &argv, &g, rng_start, &answers, &[], &[BORROW]) else {
            return;
        };
        let g0 = g.clone();
        let mut rng = Rng::from_state(rng_start);
        buy_phys_commod(&mut g, &mut ui, &mut rng, e, ai_mode);
        check(&g, &rng, &ui.rec, &o, &format!("seed {seed} {commod}{__wsr_text_13548}{ai_mode}",__wsr_text_13548=__content.string(13548)));
        if g.phys_commod_rec.as_slice() != g0.phys_commod_rec.as_slice() {
            hits.hit(if ai_mode == 0 { "bought_ui" } else { "bought_ai" });
        }
        if o.log.iter().any(|l| l.starts_with("CALL BorrowToCoverPurchase(")) {
            hits.hit(__content.string(13624));
        }
    });
    hits.require(&["bought_ui", "bought_ai", __content.string(13624)]);
}

#[test]
fn diff_ai_trade_phys_commod() {
    let hits = Hits::default();
    par_seeds(0..64u64, |seed| {
        let (mut g, owners) = cworld(seed + 700);
        let mut r = Rand(seed + 29);
        // many records so the random-record branch can pick one
        let n: i32 = g.phys_commod_rec[0].to_display().parse().unwrap();
        let mut k = n;
        for i in 0..r.range(0, 14) {
            let c = PHYS[cast::<usize, _>(r.range(0, 6))];
            let o = cast::<i32, _>(r.range(131, 1580));
            if g.entity_status[o] != 1 || i % 3 == 0 {
                continue;
            }
            k += 1;
            g.phys_commod_rec[k] = st(&format!("{c}|{o}|{}|{:.2}", r.range(1, 3000), f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(900.0)))));
        }
        g.phys_commod_rec[0] = st(&k.to_string());
        if seed % 4 == 1 {
            // cheap markets: buying
            g.oil_price = cast::<f32, _>(Ext::from_f64(0.5));
            g.wheat_price = cast::<f32, _>(Ext::from_f64(2.0));
            g.corn_price = cast::<f32, _>(Ext::from_f64(2.0));
            g.silver_price = cast::<f32, _>(Ext::from_f64(8.0));
            g.gold_price = cast::<f32, _>(Ext::from_f64(900.0));
        } else if seed % 4 == 2 {
            // expensive markets: selling
            g.oil_price = cast::<f32, _>(Ext::from_f64(1.8));
            g.wheat_price = cast::<f32, _>(Ext::from_f64(14.0));
            g.corn_price = cast::<f32, _>(Ext::from_f64(12.0));
            g.silver_price = cast::<f32, _>(Ext::from_f64(45.0));
            g.bitcoin_price = 300000;
            g.ethereum_price = 8000;
        }
        g.last_commod_seller_ent = if r.chance(Ext::from_f64(0.2)) { owners[1] } else { 0 };
        let e = if seed % 3 == 0 { 0 } else { owners[cast::<usize, _>(r.range(0, 9))] };
        let rng_start = rng0(seed);
        let Some(o) = run(AI_TRADE_PHYS, &[&e.to_string()], &g, rng_start, &[], &[]) else {
            return;
        };
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        let v = ai_trade_phys_commod(&mut g, &mut ui, &mut rng, e);
        check(&g, &rng, &ui, &o, &format!("seed {seed}"));
        assert_eq!(v, cast::<i32, _>(o.return_value), "seed {seed}: result");
        hits.hit(&v.to_string());
    });
    hits.require(&["-1", "1", "0"]);
}

#[test]
fn diff_charge_commod_storage_costs() {
    let hits = Hits::default();
    par_seeds(0..24u64, |seed| {
        let (mut g, _) = cworld(seed + 800);
        let mut r = Rand(seed + 31);
        g.cur_quarter = cast::<i32, _>(seed % 4) + 1;
        g.sim_co_count = [100, 700, 1600][cast::<usize, _>(r.range(0, 2))];
        let recs = phys_owners(&g);
        if let Some(&(_, o)) = recs.first() {
            g.cur_player = if o <= g.num_players { o } else { 1 };
            if seed % 2 == 0 {
                g.active_entity = o;
                g.sel_entity = o;
                g.news_msg = Default::default();
            }
        }
        // big grain stocks for the spoilage table
        let n: i32 = g.phys_commod_rec[0].to_display().parse().unwrap();
        for i in 1..=n {
            let t = g.phys_commod_rec[i].to_display();
            if t.starts_with("WHEAT") || t.starts_with("CORN") {
                let f: Vec<&str> = t.split('|').collect();
                let units = [60, 150, 400, 800, 2000, 3500, 6000][cast::<usize, _>(r.range(0, 6))];
                g.phys_commod_rec[i] = st(&format!("{}|{}|{}|{}", f[0], f[1], units, f[3]));
            }
        }
        let rng_start = rng0(seed);
        let Some(o) = run(CHARGE_STORAGE, &[], &g, rng_start, &[], &[]) else {
            return;
        };
        let g0 = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng_start);
        charge_commod_storage_costs(&mut g, &mut ui, &mut rng);
        check(&g, &rng, &ui, &o, &format!("seed {seed}"));
        if g.phys_commod_rec.as_slice() != g0.phys_commod_rec.as_slice() {
            hits.hit("spoiled");
        }
        if !ui.news.is_empty() {
            hits.hit("news");
        }
    });
    hits.require(&["spoiled", "news"]);
}
