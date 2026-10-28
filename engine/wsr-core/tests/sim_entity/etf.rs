//! Differential tests of `logic::sim_entity::etf`: GetEtfSmallestHolding, EtfWriteCoveredCalls (with
//! PickCoveredCallFund), AiCreateOptionPosition, FindOptionCounterparty and AiCorpSellNakedPuts.
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::etf::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const GET_ETF_SMALLEST: u32 = 0x64c9d1;
const ETF_WRITE_CALLS: u32 = 0x603825;
const AI_CREATE_OPTION: u32 = 0x5fef2c;
const FIND_COUNTERPARTY: u32 = 0x602925;
const SELL_NAKED_PUTS: u32 = 0x5fe20b;
/// Routines skipped in the original: none.
const SKIPS: &[(u32, u32)] = &[];

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x1b87_3593
}

#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    let mut port = port.clone();
    align_option_premiums(&mut port, &o.globals);
    if std::env::var("COV").is_ok() {
        let n = (0..=1000)
            .filter(|&i| !port.option_recs[i].is_empty())
            .count();
        eprintln!("COV {what}: option recs {n}, news {:?}", ui.news);
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).assert(&port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?}",
        ui.msgs
    );
}

/// Option premiums are `CalcOptionPrice * pct` in 80-bit precision; the port's `calc_option_price`
/// returns f64, so the STR$ of a premium (last field of an option record) can differ in the 15th digit.
/// Takes the original's record where only that field differs, by at most 1e-12 relative.
fn align_option_premiums(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (
            port.option_recs[i].to_display(),
            orig.option_recs[i].to_display(),
        );
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        let n = pa.len();
        if n < 2 || n != pb.len() || pa[..n - 1] != pb[..n - 1] {
            continue;
        }
        let (x, y): (Ext, Ext) = (
            pa[n - 1].trim().parse().unwrap_or(Ext::NAN),
            pb[n - 1].trim().parse().unwrap_or(Ext::NAN),
        );
        if (x - y).abs() <= Ext::from_f64(1e-12) * y.abs() {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

/// [`world`] with funds 1581..1600 (industry 71, advisers, 12..15 holdings), share prices of 5..900,
/// solid credit ratings for many corps and some option records.
fn etf_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed * 31 + 7);
    if g.start_year == g.cur_year && g.cur_quarter == 1 {
        g.start_year -= 1;
    }
    for e in 131..=1600 {
        // share price 5..900 for most corps
        if r.chance(cast::<f64, _>(Ext::from_f64(0.9))) {
            g.shares_outstanding[e] = g.market_cap[e] / f64r(&mut r, Ext::from_f64(5.0), Ext::from_f64(900.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.credit_rating_num[e] = cast::<i32, _>(r.range(7, 10));
        }
    }
    // smaller stakes, so that most stocks keep a public float
    for e in 1..=1600 {
        for slot in 1..=15 {
            if g.hold_stock_pct[(slot, e)] > 12 && r.chance(cast::<f64, _>(Ext::from_f64(0.85))) {
                g.hold_stock_pct[(slot, e)] = cast::<i16, _>(r.range(1, 12));
            }
        }
    }
    for f in 1581..=1600 {
        if !r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
            continue;
        }
        g.entity_status[f] = 1;
        g.industry_num[f] = 71;
        g.fund_adviser[f] = cast::<i32, _>(r.range(86, 1580));
        g.ctrl_since_year[f] = g.cur_year - cast::<i32, _>(r.range(0, 5));
        g.corp_controller[f] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)))
        } else {
            0
        };
        g.bonds_issued[f] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { r.range(1, 500) } else { 0 };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.bank_debt[f] = Ext::from_f64(0.0);
        }
        g.cash[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(40000.0));
        g.t_bills[f] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(40000.0));
        let n = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            15
        } else {
            cast::<i32, _>(r.range(10, 15))
        };
        for slot in 1..=15 {
            if slot <= n {
                g.hold_stock_id[(slot, f)] = cast::<i16, _>(r.range(131, 1580));
                g.hold_stock_pct[(slot, f)] = cast::<i16, _>(r.range(0, 9));
            } else {
                g.hold_stock_id[(slot, f)] = 0;
                g.hold_stock_pct[(slot, f)] = 0;
            }
        }
        g.hold_stock_id[(0, f)] = cast::<i16, _>(n);
    }
    for p in 1..=5 {
        g.auto_pilot[p] = cast::<i32, _>(r.range(0, 1));
    }
    let n = cast::<i32, _>(r.range(0, 40));
    for i in 1..=n {
        let corp = r.range(131, 1580);
        let holder = r.range(1, 1600);
        let qty = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            r.range(1, 20)
        } else {
            -r.range(1, 20)
        };
        let strike = g.market_cap[cast::<i32, _>(corp)] * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.3));
        g.option_recs[i] = PbStr::from(
            format!(
                "{corp}|{}|{holder}|{qty}|{strike:.4}|{}|{}|{:.4}",
                r.range(1, 2),
                r.range(1, 12),
                g.cur_year + cast::<i32, _>(r.range(0, 1)),
                f64r(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(50.0)))
            )
            .as_str(),
        );
    }
    g.option_recs[0] = PbStr::from(n.to_string().as_str());
    if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        g.news_msg = PbStr::from(__content.string(13571));
    }
    g
}

#[test]
fn diff_get_etf_smallest_holding() {
    par_seeds(1..=8, |seed| {
        let g = etf_world(seed);
        let mut r = Rand(seed * 5 + 3);
        let cands: Vec<i32> = (11..=1600)
            .filter(|&e| g.entity_status[e] == 1 && g.hold_stock_id[(0, e)] >= 15)
            .collect();
        let e = if cands.is_empty() || r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            cast::<i32, _>(r.range(11, 1600))
        } else {
            cands[cast::<usize, _>(r.range(0, cast::<i64, _>(cands.len()) - 1))]
        };
        let rs = rng0(seed);
        let mut port = g.clone();
        let rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let v = get_etf_smallest_holding(&mut port, &mut ui, e);
        let Some(o) = run(GET_ETF_SMALLEST, &[&e.to_string()], &g, rs, &[], SKIPS) else {
            return;
        };
        let what = format!("seed {seed} etf {e}");
        check(&port, &rng, &ui, &o, &what);
        assert_eq!(v, cast::<i32, _>(o.return_value), "{what}: result");
    });
}

#[test]
fn diff_etf_write_covered_calls() {
    par_seeds(1..=10, |seed| {
        let mut g = etf_world(seed + 50);
        let mut r = Rand(seed * 11 + 1);
        g.covered_call_fund_id = match r.range(0, 2) {
            0 => 0,
            _ => cast::<i32, _>(r.range(1581, 1600)),
        };
        g.sim_co_count = cast::<i32, _>(r.range(10, 1600));
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        etf_write_covered_calls(&mut port, &mut ui, &mut rng);
        let Some(o) = run(ETF_WRITE_CALLS, &[], &g, rs, &[], SKIPS) else {
            return;
        };
        check(&port, &rng, &ui, &o, &format!("seed {seed}"));
    });
}

#[test]
fn diff_ai_create_option_position() {
    let __content = wsr_core::content::current();
    par_seeds(1..=10, |seed| {
        let g = etf_world(seed + 100);
        let mut r = Rand(seed * 13 + 5);
        let is_etf = cast::<i32, _>(seed % 2);
        let buyer = if is_etf == 1 {
            cast::<i32, _>(r.range(1581, 1600))
        } else {
            cast::<i32, _>(r.range(100, 1580))
        };
        let corp = cast::<i32, _>(r.range(131, 1580));
        let tran = cast::<i32, _>(r.range(1, 2));
        let pct = cast::<i32, _>(r.range(1, 9));
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let v =
            ai_create_option_position(&mut port, &mut ui, &mut rng, buyer, corp, pct, tran, is_etf);
        let args = [
            format!("{__wsr_text_13741}{buyer}",__wsr_text_13741=__content.string(13741)),
            format!("{__wsr_text_13741}{corp}",__wsr_text_13741=__content.string(13741)),
            format!("v:{pct}"),
            format!("{__wsr_text_13741}{tran}",__wsr_text_13741=__content.string(13741)),
            format!("{__wsr_text_13741}{is_etf}",__wsr_text_13741=__content.string(13741)),
        ];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(AI_CREATE_OPTION, &args, &g, rs, &[], SKIPS) else {
            return;
        };
        let what =
            format!("seed {seed}{__wsr_text_14157}{buyer} corp {corp} pct {pct} tran {tran} etf {is_etf}",__wsr_text_14157=__content.string(14157));
        check(&port, &rng, &ui, &o, &what);
        assert_eq!(v, cast::<i32, _>(o.return_value), "{what}: result");
    });
}

#[test]
fn diff_find_option_counterparty() {
    let __content = wsr_core::content::current();
    par_seeds(1..=10, |seed| {
        let g = etf_world(seed + 150);
        let mut r = Rand(seed * 17 + 9);
        let corp = r.range(131, 1580);
        let holder = r.range(1, 1600);
        let qty = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            r.range(1, 9)
        } else {
            -r.range(1, 9)
        };
        let strike = g.market_cap[cast::<i32, _>(corp)] * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        let prem = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(60.0));
        let rec = format!(
            " {corp}|{}| {holder}| {qty}| {strike}| {}| {}| {prem}",
            r.range(1, 2),
            r.range(1, 12),
            g.cur_year + 1
        );
        let premium = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { -prem } else { prem };
        let risk = [cast::<Ext, _>(0.26f32), Ext::from_f64(0.1), cast::<Ext, _>(0.24f32), Ext::from_f64(0.5)][cast::<usize, _>(r.range(0, 3))];
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        let v = find_option_counterparty(
            &mut port,
            &mut ui,
            &mut rng,
            &PbStr::from(rec.as_str()),
            premium,
            risk,
        );
        let args = [
            format!("s:{rec}"),
            format!("{__wsr_text_14156}{premium:?}",__wsr_text_14156=__content.string(14156)),
            format!("{__wsr_text_14156}{risk:?}",__wsr_text_14156=__content.string(14156)),
        ];
        let args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(FIND_COUNTERPARTY, &args, &g, rs, &[], SKIPS) else {
            return;
        };
        let what = format!(
            "seed {seed} rec {rec}{__wsr_text_14158}{risk} recs before {}",
            (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count()
        ,__wsr_text_14158=__content.string(14158));
        check(&port, &rng, &ui, &o, &what);
        assert_eq!(v, cast::<i32, _>(o.return_value), "{what}: result");
    });
}

#[test]
fn diff_ai_corp_sell_naked_puts() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let mut g = etf_world(seed + 200);
        let mut r = Rand(seed * 19 + 11);
        g.sim_co_count = cast::<i32, _>(r.range(10, 1600));
        let (seller, target) = match seed % 4 {
            // a fund, picking itself
            0 | 1 => (
                cast::<i32, _>(r.range(1584, 1600)),
                if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
                    0
                } else {
                    cast::<i32, _>(r.range(131, 1580))
                },
            ),
            // a corporation with a 45..78% holding
            2 => {
                let s = cast::<i32, _>(r.range(131, 1580));
                g.entity_status[s] = 1;
                // holdings in slots 1..n
                let n = cast::<i32, _>(r.range(1, 3));
                for k in 1..=15 {
                    g.hold_stock_id[(k, s)] = if k <= n { cast::<i16, _>(r.range(131, 1580)) } else { 0 };
                    g.hold_stock_pct[(k, s)] = if k <= n { cast::<i16, _>(r.range(30, 80)) } else { 0 };
                }
                g.hold_stock_id[(0, s)] = cast::<i16, _>(n);
                g.credit_rating_num[s] = cast::<i32, _>(r.range(6, 10));
                // a computer-run corporate parent with voting control
                let k = cast::<i32, _>(r.range(131, 1580));
                g.entity_status[k] = 1;
                g.hold_stock_id[(1, k)] = cast::<i16, _>(s);
                g.hold_stock_pct[(1, k)] = cast::<i16, _>(r.range(40, 70));
                g.hold_stock_id[(0, k)] =
                    cast::<i16, _>((1..=15).filter(|&j| g.hold_stock_id[(j, k)] != 0).count());
                g.corp_controller[s] = k;
                (s, 0)
            }
            _ => (cast::<i32, _>(r.range(131, 1580)), cast::<i32, _>(r.range(131, 1580))),
        };
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        ai_corp_sell_naked_puts(&mut port, &mut ui, &mut rng, seller, target);
        let Some(o) = run(
            SELL_NAKED_PUTS,
            &[&seller.to_string(), &target.to_string()],
            &g,
            rs,
            &[],
            SKIPS,
        ) else {
            return;
        };
        let recs = |g: &Globals| (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
        check(
            &port,
            &rng,
            &ui,
            &o,
            &format!(
                "seed {seed}{__wsr_text_13736}{seller}{__wsr_text_13552}{target} recs before {}",
                recs(&g)
            ,__wsr_text_13736=__content.string(13736), __wsr_text_13552=__content.string(13552)),
        );
    });
}
