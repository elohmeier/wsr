//! Differential tests of `logic::trading::borrow`: BorrowToCoverPurchase (0x5246b6).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trading::borrow::borrow_to_cover_purchase;

const BORROW: u32 = 0x5246b6;

#[test]
fn diff_borrow_to_cover_purchase() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=120), |seed| {
        let mut g = world(seed + 9700);
        let mut r = Rand(seed * 2_654_435 + 17);
        let ae = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { pick_active(&mut r, &g, 11, 1580, &[]) };
        g.active_entity = ae;
        g.active_entity_name = g.ent_name[ae].clone();
        if seed % 15 == 0 {
            g.entity_status[ae] = 2;
        }
        g.cash[ae] = match r.range(0, 2) {
            0 => -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(400.0)),
            1 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)),
            _ => Ext::from_f64(0.0),
        };
        g.t_bills[ae] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200.0)) } else { Ext::from_f64(0.0) };
        g.unused_credit_line = match r.range(0, 2) {
            0 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0)),
            _ => f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(5000.0)),
        };
        // the caller's CalcEntityFinancials
        g.net_worth = f64r(&mut r, -Ext::from_f64(200.0), Ext::from_f64(4000.0));
        g.tmp_total_debt = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
        g.tmp_credit_net_worth = g.net_worth * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.2));
        let amt = match r.range(0, 4) {
            0 => -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0)),
            1 => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.5)),
            _ => f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(3000.0)),
        };
        let equity_loss = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(200.0)) } else { Ext::from_f64(0.0) };
        let debt_repaid: i64 = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(0, 500) } else { 0 };
        let answers: Vec<String> =
            (0..3).map(|_| ["IDYES", "IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 3))].to_string()).collect();
        let args = [format!("{__wsr_text_14156}{amt:?}",__wsr_text_14156=__content.string(14156)), format!("{__wsr_text_14156}{equity_loss:?}",__wsr_text_14156=__content.string(14156)), format!("{__wsr_text_13664}{debt_repaid}",__wsr_text_13664=__content.string(13664))];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let mut port_result = 0;
        let res_cell = std::cell::Cell::new(0);
        let Some((ui, _, o)) = check(seed, BORROW, &args, &g, &answers, &[], cmp, |g, ui, _rng| {
            let mut a = amt;
            res_cell.set(borrow_to_cover_purchase(g, ui, &mut a, equity_loss, debt_repaid));
        }) else {
            return;
        };
        port_result += res_cell.get();
        assert_eq!(cast::<u32, _>(port_result), o.return_value, "seed {seed}: result");
        hits.record(&ui);
        hits.add(&format!("<result {port_result}>"));
    });
    hits.assert(&["<result 0>", "<result 6>", "<result 7>", __content.string(11887), __content.string(14264)]);
}
