//! Differential tests of `logic::trading::options_menu`: BuyOptions (0x5e340d) with ShowOptionChain
//! (0x5f0388), and CheckCircularOptionHolding (0x606e94).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trading::options_menu::*;
use wsr_core::state::Globals;

const BUY_OPTIONS: u32 = 0x5e340d;
const CHECK_CIRCULAR: u32 = 0x606e94;

/// A unique stock symbol for `corp`.
fn unique_symbol(g: &mut Globals, corp: i32) -> String {
    let sym = format!("QZ{}", corp);
    g.ent_symbol[corp] = st(&sym);
    sym
}

/// `STR$`-like field text (leading blank for non-negative numbers).
fn fld(x: Ext) -> String {
    if x < Ext::from_f64(0.0) { format!("{x}") } else { format!(" {x}") }
}

/// Appends an option record `corp|type|holder|qty|strike|month|year|premium`.
#[allow(clippy::too_many_arguments)]
fn add_rec(g: &mut Globals, corp: i32, opt_type: i32, holder: i32, qty: i32, strike: Ext, month: i32, year: i32, prem: Ext) {
    let k = (1..=1000).find(|&k| g.option_recs[k].is_empty()).unwrap_or(1000);
    g.option_recs[k] = st(&format!(
        "{corp}|{}|{}|{}|{}|{}|{}|{}",
        fld(Ext::from(cast::<Ext, _>(opt_type))),
        fld(Ext::from(cast::<Ext, _>(holder))),
        fld(Ext::from(cast::<Ext, _>(qty))),
        fld(strike),
        fld(Ext::from(cast::<Ext, _>(month))),
        fld(Ext::from(cast::<Ext, _>(year))),
        fld(prem)
    ));
    let n = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
    g.option_recs[0] = st(&n.to_string());
}

/// Option records hold STR$ of EXT values (strike, premium) computed in 80-bit precision by the original;
/// the port's option pricing works in f64, so the 15th digit of such a field can differ. Takes the
/// original's record where only numeric fields differ, by at most 1e-12 relative (as tests/turn does).
fn align_option_recs(port: &mut Globals, orig: &Globals) {
    for i in 0..=1000 {
        let (a, b) = (port.option_recs[i].to_display(), orig.option_recs[i].to_display());
        if a == b {
            continue;
        }
        let (pa, pb): (Vec<&str>, Vec<&str>) = (a.split('|').collect(), b.split('|').collect());
        if pa.len() != pb.len() || pa.len() < 2 {
            continue;
        }
        let close = pa.iter().zip(&pb).all(|(x, y)| {
            x == y
                || match (x.trim().parse::<Ext>(), y.trim().parse::<Ext>()) {
                    (Ok(x), Ok(y)) => (x - y).abs() <= Ext::from_f64(1e-12) * y.abs().max(Ext::from_f64(1e-300)),
                    _ => false,
                }
        });
        if close {
            port.option_recs[i] = orig.option_recs[i].clone();
        }
    }
}

/// [`check`] with the option records aligned first ([`align_option_recs`]).
fn check_aligned(
    seed: u64,
    args: &[String],
    g: &Globals,
    answers: &[String],
    port: impl Fn(&mut Globals, &mut Script, &mut wsr_core::pb::rng::Rng),
) -> Option<(Script, Globals, Original)> {
    // run the original first through `check` with option records ignored, then compare them aligned
    let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7)).ignore("OptionRecs");
    let (ui, mut pg, o) = check(seed, BUY_OPTIONS, args, g, answers, &[], cmp, port)?;
    align_option_recs(&mut pg, &o.globals);
    for i in 0..=1000 {
        assert_eq!(pg.option_recs[i], o.globals.option_recs[i], "seed {seed}: OptionRecs({i})");
    }
    Some((ui, pg, o))
}

struct Case {
    g: Globals,
    opt_type: i32,
    answers: Vec<String>,
}

/// Pool of numbers: message box answers (6 = YES, 7 = NO, 2 = CANCEL) as well as entered percents.
fn nums(r: &mut Rand, n: usize, pool: &[i64]) -> Vec<String> {
    (0..n).map(|_| pool[cast::<usize, _>(r.range(0, cast::<i64, _>(pool.len()) - 1))].to_string()).collect()
}

fn case(seed: u64) -> Case {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 11_000);
    let mut r = Rand(seed * 69_069 + 1);
    let kind = seed % 10;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    let cp = g.cur_player;
    g.cur_player_name = st(&format!("Player Number {cp}"));
    g.app_title = st(__content.string(13661));
    g.is_computer_player[cp] = 0;
    g.active_entity = cp;
    g.active_entity_name = g.player_name[cp].clone();
    g.cash[cp] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(60_000.0));
    g.sim_co_count = cast::<i32, _>(r.range(600, 1400));
    g.option_recs[0] = st("0");
    g.player_exercise_options[cp] = cast::<i32, _>(r.range(0, 1));
    g.currency_code = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "USA" } else { "GBR" });
    let mut opt_type = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 1 } else { 2 };
    // the stock: a public company with a share price of 6..400
    let corp = pick_active(&mut r, &g, 131, 1580, &[]);
    clear_holders(&mut g, corp);
    for _ in 0..r.range(0, 3) {
        let h = pick_active(&mut r, &g, 11, 1580, &[corp]);
        give_holding(&mut g, h, corp, cast::<i16, _>(r.range(1, 20)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)));
    }
    g.credit_rating_num[corp] = cast::<i32, _>(r.range(2, 10));
    let price = f64r(&mut r, Ext::from_f64(6.0), Ext::from_f64(400.0));
    g.shares_outstanding[corp] = g.market_cap[corp] / price;
    let sym = unique_symbol(&mut g, corp);
    // a few computer-run companies able to write options
    for _ in 0..6 {
        let c = pick_active(&mut r, &g, 131, 1580, &[corp]);
        g.credit_rating_num[c] = 9;
        g.corp_controller[c] = 0;
        g.cash[c] += Ext::from_f64(5000.0);
    }
    let strike_txt = format!("{}", cast::<i64, _>(price.round()));
    let pick = format!("pick:{}", r.range(0, 23));
    let mut answers = vec![format!("sym:{sym}")];
    match kind {
        // a new position of the player
        0 | 1 => {
            if kind == 1 && opt_type == 2 && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                // an insider hedging: controls and owns the stock
                g.corp_controller[corp] = cp;
                give_holding(&mut g, cp, corp, cast::<i16, _>(r.range(0, 12)), Ext::from_f64(10.0));
            }
            answers.push(strike_txt);
            answers.push(pick);
            answers.extend(nums(&mut r, 4, &[1, 2, 3, 6, 6, 6, 7, 10, 25]));
        }
        // buying back short positions (one or two records)
        2 | 3 => {
            let n = if kind == 2 { 1 } else { 2 };
            for _ in 0..n {
                let strike = (g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
                let qty = -(cast::<i32, _>(r.range(1, 8)));
                let prem = (g.market_cap[corp] * f64r(&mut r, Ext::from_f64(0.01), Ext::from_f64(0.2)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
                let year = g.cur_year + 1;
                add_rec(&mut g, corp, opt_type, cp, qty, strike, cast::<i32, _>(r.range(1, 12)), year, prem);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.cash[cp] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5.0));
            }
            answers.extend(nums(&mut r, 1, &[6, 6, 6, 7, 2]));
            if n == 2 {
                answers.push(format!("pick:{}", r.range(0, 1)));
            }
            answers.extend(nums(&mut r, 4, &[1, 2, 3, 6, 6, 7, 8]));
            // (a declined buy-back goes on to a new purchase)
            answers.push(format!("{}", cast::<i64, _>(price.round())));
            answers.push(pick);
            answers.extend(nums(&mut r, 3, &[1, 2, 6, 6]));
        }
        // a company of the player (or a bank / insurer) buys
        4 => {
            let b = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { pick_active(&mut r, &g, 11, 130, &[]) } else { pick_active(&mut r, &g, 131, 1580, &[corp]) };
            g.active_entity = b;
            g.active_entity_name = g.ent_name[b].clone();
            g.corp_controller[b] = cp;
            g.cash[b] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30_000.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                give_holding(&mut g, b, corp, cast::<i16, _>(r.range(1, 15)), Ext::from_f64(10.0));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.corp_controller[corp] = cp;
            }
            answers.push(strike_txt);
            answers.push(pick);
            answers.extend(nums(&mut r, 4, &[1, 2, 3, 6, 6, 6, 7, 10]));
        }
        // refusals
        5 => {
            match (seed / 10) % 9 {
                0 => g.shares_outstanding[corp] = g.market_cap[corp] / f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(4.9)),
                1 => g.shares_outstanding[corp] = g.market_cap[corp] / f64r(&mut r, Ext::from_f64(1001.0), Ext::from_f64(3000.0)),
                2 => g.credit_rating_num[corp] = 0,
                3 => {
                    g.merger_target = corp;
                    g.merger_acquirer = pick_active(&mut r, &g, 131, 1580, &[corp]);
                    give_holding(&mut g, cp, corp, cast::<i16, _>(r.range(3, 20)), Ext::from_f64(10.0));
                }
                4 => {
                    // 998 records (of other stocks and holders)
                    let year = g.cur_year + 1;
                    for k in 1..=998 {
                        g.option_recs[k] = st(&format!("{}| 1| 99| 1| 100| 6| {year}| 5", 131 + (k % 400)));
                    }
                    g.option_recs[0] = st("998");
                }
                5 => {
                    // no public float
                    clear_holders(&mut g, corp);
                    let h = pick_active(&mut r, &g, 131, 1580, &[corp]);
                    give_holding(&mut g, h, corp, 100, Ext::from_f64(10.0));
                }
                6 => {
                    g.start_year = g.cur_year;
                    g.cur_quarter = 1;
                    g.sim_co_count = cast::<i32, _>(r.range(0, 500));
                }
                7 => answers[0] = "sym:".to_string(),
                _ => {
                    g.cash[cp] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(10.0));
                    g.t_bills[cp] = Ext::from_f64(0.0);
                    g.bank_debt[cp] = Ext::from_f64(1e6);
                }
            }
            answers.push(strike_txt);
            answers.push(pick);
            answers.extend(nums(&mut r, 3, &[1, 6, 6]));
        }
        // a fund advised by the player's company (or a fund that may not trade options)
        6 => {
            let etf = cast::<i32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(1581, 1585) } else { r.range(1586, 1600) });
            g.entity_status[etf] = 1;
            g.industry_num[etf] = 71;
            g.ent_name[etf] = st(&format!("{__wsr_text_14291}{etf}",__wsr_text_14291=__content.string(14291)));
            let adviser = pick_active(&mut r, &g, 131, 1580, &[corp]);
            g.industry_num[adviser] = 37;
            g.corp_controller[adviser] = cp;
            g.fund_adviser[etf] = adviser;
            g.ctrl_since_year[etf] = g.cur_year - 2;
            g.sel_entity = etf;
            g.cash[etf] = f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(20_000.0));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                g.industry_num[corp] = 70; // a holding company: fits every sector fund
            }
            answers.insert(0, ["6", "6", "7", "2"][cast::<usize, _>(r.range(0, 3))].to_string());
            answers.push(strike_txt);
            answers.push(pick);
            answers.extend(nums(&mut r, 4, &[1, 2, 6, 6, 7]));
        }
        // circular holdings: the stock holds the buying company (or its holder)
        7 => {
            opt_type = 1;
            let b = pick_active(&mut r, &g, 131, 1580, &[corp]);
            g.active_entity = b;
            g.active_entity_name = g.ent_name[b].clone();
            g.corp_controller[b] = cp;
            g.cash[b] = Ext::from_f64(20_000.0);
            clear_holders(&mut g, b);
            match r.range(0, 2) {
                0 => {
                    give_holding(&mut g, corp, b, cast::<i16, _>(r.range(3, 30)), Ext::from_f64(10.0));
                }
                1 => {
                    let h = pick_active(&mut r, &g, 131, 1580, &[corp, b]);
                    give_holding(&mut g, h, b, 40, Ext::from_f64(10.0));
                    clear_holders(&mut g, h);
                    give_holding(&mut g, corp, h, 45, Ext::from_f64(10.0));
                }
                _ => {
                    let h = pick_active(&mut r, &g, 131, 1580, &[corp, b]);
                    let h2 = pick_active(&mut r, &g, 131, 1580, &[corp, b, h]);
                    give_holding(&mut g, h, b, 40, Ext::from_f64(10.0));
                    clear_holders(&mut g, h);
                    give_holding(&mut g, h2, h, 45, Ext::from_f64(10.0));
                    give_holding(&mut g, corp, h, 25, Ext::from_f64(10.0));
                    clear_holders(&mut g, h2);
                    give_holding(&mut g, corp, h2, 60, Ext::from_f64(10.0));
                }
            }
            answers.push(strike_txt);
            answers.push(pick);
            answers.extend(nums(&mut r, 3, &[1, 6, 6]));
        }
        // too little money for 1%
        8 => {
            g.cash[cp] = f64r(&mut r, Ext::from_f64(0.01), Ext::from_f64(0.5));
            g.t_bills[cp] = Ext::from_f64(0.0);
            g.bank_debt[cp] = Ext::from_f64(1e7); // no unused credit line
            answers.push(strike_txt);
            answers.push(pick.clone());
            answers.extend(nums(&mut r, 1, &[6, 7]));
            answers.push(pick);
            answers.extend(nums(&mut r, 2, &[7, 2]));
        }
        // an out-of-range strike first, then no month picked ("No Option Selected": YES = cancel; the
        // original's chain dialog closes with its default button only when no dialog action is left)
        _ => {
            let _ = pick;
            answers.push("1".to_string());
            answers.push(strike_txt);
            answers.push("6".to_string());
        }
    }
    Case { g, opt_type, answers }
}

#[test]
fn diff_buy_options() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=260), |seed| {
        let c = case(seed);
        let args = [c.opt_type.to_string()];
        let Some((ui, pg, _)) = check_aligned(seed, &args, &c.g, &c.answers, |g, ui, rng| {
            buy_options(g, ui, rng, c.opt_type, 0)
        }) else {
            return;
        };
        hits.record(&ui);
        if pg.option_recs[0] != c.g.option_recs[0] || (1..=20).any(|k| pg.option_recs[k] != c.g.option_recs[k]) {
            hits.add("<recs changed>");
        }
    });
    hits.assert(&[
        "<recs changed>",
        __content.string(12482),
        __content.string(14292),
        __content.string(12367),
        __content.string(14293),
        __content.string(14294),
        __content.string(14295),
        __content.string(11731),
        __content.string(14296),
        __content.string(12046),
        __content.string(12458),
        __content.string(14265),
        __content.string(12341),
        __content.string(14297),
        __content.string(14298),
        __content.string(14242),
        __content.string(14243),
        __content.string(14299),
        __content.string(14300),
        __content.string(12342),
        __content.string(12520),
        __content.string(9865),
    ]);
}

#[test]
fn diff_check_circular_option_holding() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=40), |seed| {
        let mut g = world(seed + 12_000);
        let mut r = Rand(seed * 7 + 3);
        let buyer = if seed % 9 == 0 { 1 } else { pick_active(&mut r, &g, 131, 1580, &[]) };
        let stock = pick_active(&mut r, &g, 131, 1580, &[buyer]);
        g.currency_code = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { "USA" } else { "GBR" });
        if buyer > 10 {
            clear_holders(&mut g, buyer);
            match seed % 4 {
                0 => {
                    give_holding(&mut g, stock, buyer, cast::<i16, _>(r.range(1, 30)), Ext::from_f64(10.0));
                    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                        g.corp_controller[stock] = g.corp_controller[buyer];
                    }
                }
                1 => {
                    let h = pick_active(&mut r, &g, 131, 1580, &[buyer, stock]);
                    give_holding(&mut g, h, buyer, 40, Ext::from_f64(10.0));
                    clear_holders(&mut g, h);
                    give_holding(&mut g, stock, h, cast::<i16, _>(r.range(15, 60)), Ext::from_f64(10.0));
                }
                2 => {
                    let mut prev = buyer;
                    for _ in 0..r.range(2, 5) {
                        let h = pick_active(&mut r, &g, 131, 1580, &[buyer, stock, prev]);
                        give_holding(&mut g, h, prev, 50, Ext::from_f64(10.0));
                        clear_holders(&mut g, h);
                        prev = h;
                    }
                    give_holding(&mut g, stock, prev, 50, Ext::from_f64(10.0));
                }
                _ => {}
            }
        }
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let args = [buyer.to_string(), stock.to_string()];
        let Some((ui, _, o)) = check(seed, CHECK_CIRCULAR, &args, &g, &[], &[], cmp, |g, ui, rng| {
            check_circular_option_holding(g, ui, rng, buyer, stock);
        }) else {
            return;
        };
        let mut pg = g.clone();
        let mut rng = wsr_core::pb::rng::Rng::from_state(rng_of(seed));
        let mut ui2 = Script::default();
        let res = check_circular_option_holding(&mut pg, &mut ui2, &mut rng, buyer, stock);
        assert_eq!(cast::<u32, _>(res), o.return_value, "seed {seed}: result");
        hits.record(&ui);
    });
    hits.assert(&[__content.string(14301), __content.string(13671), "a long chain", "S.E.C."]);
}
