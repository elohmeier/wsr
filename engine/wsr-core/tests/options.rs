//! Tests of `logic::options`: hand-checked unit tests and differential tests against the original
//! routines run under emulation (tools/emu/game.py) on random but plausible option books.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::logic::options::*;
use wsr_core::logic::earnings::{balance_cash_t_bills, calc_cur_month_of_year};
use wsr_core::logic::valuation::calc_dividend_yield;
use wsr_core::pb::numeric::str_num;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::Ui;

/// Records message boxes and news headlines.
#[derive(Default)]
struct Rec {
    msgs: Vec<String>,
    news: Vec<String>,
}

impl Ui for Rec {
    fn msgbox(&mut self, text: &PbStr, _style: i32, _title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        1
    }
    fn inputbox(&mut self, _p: &PbStr, _t: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, _t: &PbStr, _l: &[PbStr], _y: bool) -> wsr_core::ui::ReportAction {
        wsr_core::ui::ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}

const CALC_OPTION_PRICE: u32 = 0x603c8e;
const CALC_SPREAD_FACTOR: u32 = 0x606399;
const CALC_OPTIONS_VALUE: u32 = 0x5f23e1;
const CALC_VESTED_OPTIONS_VALUE: u32 = 0x5ff8c0;
const GET_PLR_NAKED_LIAB: u32 = 0x60769f;
const GET_OPTIONED_STOCK_PCT: u32 = 0x5f8b12;
const GET_NET_STOCK_PCT_AFTER_OPTIONS: u32 = 0x608a27;
const GET_MAX_OPTIONS_PCT: u32 = 0x606a14;
const ADD_OR_REMOVE_OPTION_REC: u32 = 0x5e922a;
const MERGE_MATCHING_OPTION_REC: u32 = 0x5f7ea7;
const SORT_OPTION_RECS: u32 = 0x60661c;
const TRANSFER_OPTIONS_HOLDER: u32 = 0x5fc45b;
const SETTLE_OPTION_REC: u32 = 0x5f379e;
const CLOSE_OPTION_POSITIONS: u32 = 0x5f3e5f;
const CHECK_NAKED_OPTION_MARGIN: u32 = 0x601c99;
const CALC_ENTITY_FINANCIALS: u32 = 0x426610;
const CALC_CUR_MONTH_OF_YEAR: u32 = 0x5f3697;
const BALANCE_CASH_TBILLS: u32 = 0x69bb51;

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

fn str15(x: Ext) -> String {
    str_num(x, 15)
}

/// A random option book on a random market.
struct Book {
    g: Globals,
    corps: Vec<i32>,
    /// Holders of option records (players and corps).
    holders: Vec<i32>,
}

fn rand_f(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + cast::<Ext, _>(r.next() >> 11) / cast::<Ext, _>(1u64 << 53) * (hi - lo)
}

/// One option record in the game's format (`STR$` fields).
fn make_rec(r: &mut Rand, g: &Globals, corp: i32, holder: i32, tag: Option<&str>) -> String {
    let opt_type = r.range(1, 2);
    let mut qty = r.range(1, 25);
    if r.chance(Ext::from_f64(0.5)) {
        qty = -qty;
    }
    let mc = g.market_cap[corp];
    let strike = (mc * rand_f(r, Ext::from_f64(0.3), Ext::from_f64(2.6)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
    let month = [1, 4, 7, 10, r.range(1, 12)][cast::<usize, _>(r.range(0, 4))];
    let year = g.cur_year + cast::<i32, _>(r.range(-1, 2));
    let prem = match tag {
        Some(t) => t.to_string(),
        None => {
            let p = (mc * rand_f(r, Ext::from_f64(0.0), Ext::from_f64(0.3)) * cast::<Ext, _>(qty.abs()) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
            str15(if qty < 0 { -p } else { p })
        }
    };
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        str15(Ext::from(cast::<Ext, _>(corp))).trim(),
        str15(Ext::from(cast::<Ext, _>(opt_type))),
        str15(Ext::from(cast::<Ext, _>(holder))),
        str15(Ext::from(cast::<Ext, _>(qty))),
        str15(strike),
        str15(Ext::from(cast::<Ext, _>(month))),
        str15(Ext::from(cast::<Ext, _>(year))),
        prem
    )
}

fn book(seed: u64, n_recs: i64) -> Book {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_year = 2020 + cast::<i32, _>(r.range(0, 5));
    g.start_year = 2019;
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = if r.chance(Ext::from_f64(0.15)) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { cast::<i32, _>(r.range(1, 1600)) };
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(r.range(15, 140)) / Ext::from_f64(10.0));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.active_entity = g.cur_player;
    g.currency_symbol = st("$");
    g.million_prefix_uc = st("MILLION");
    for p in 1..=g.num_players {
        g.entity_status[p] = 1;
        g.cash[p] = rand_f(&mut r, -Ext::from_f64(50.0), Ext::from_f64(5000.0));
        g.t_bills[p] = rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0));
        g.is_computer_player[p] = cast::<i32, _>(r.range(0, 1));
        g.ent_name[p] = PbStr::from(format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556)).as_str());
        g.ent_symbol[p] = PbStr::from(format!("PL{p}").as_str());
        g.player_name[p] = g.ent_name[p].clone();
    }
    let mut corps = Vec::new();
    while corps.len() < 30 {
        let id = cast::<i32, _>(match r.range(0, 9) {
            0 => r.range(1581, 1590),
            1..=3 => r.range(11, 250),
            _ => r.range(251, 1580),
        });
        if !corps.contains(&id) {
            corps.push(id);
        }
    }
    for &c in &corps {
        g.entity_status[c] = 1;
        g.market_cap[c] = cast::<Ext, _>(r.range(30, 800_000)) / Ext::from_f64(100.0);
        g.shares_outstanding[c] = cast::<Ext, _>(r.range(5, 3000)) / Ext::from_f64(100.0);
        g.industry_num[c] = if c > 1580 { 71 } else { cast::<i16, _>(r.range(3, 69)) };
        g.proj_qtr_earnings[c] = rand_f(&mut r, -Ext::from_f64(0.05), Ext::from_f64(0.1)) * g.market_cap[c];
        g.earnings_y1[c] = rand_f(&mut r, -Ext::from_f64(0.1), Ext::from_f64(0.4)) * g.market_cap[c];
        let px = g.market_cap[c] / g.shares_outstanding[c];
        g.dividend_per_share[c] = if r.chance(Ext::from_f64(0.6)) { px * rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.08)) } else { Ext::from_f64(0.0) };
        g.ent_misc_data[c] = PbStr::from(
            format!("a|b|c|d|e|f|g|h|i|j|k|{}{__wsr_text_13932}", str15(g.market_cap[c] * rand_f(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(0.02)))).trim(),__wsr_text_13932=__content.string(13932)).as_str(),
        );
        g.corp_controller[c] = if r.chance(Ext::from_f64(0.4)) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { c };
        g.cash[c] = rand_f(&mut r, -Ext::from_f64(100.0), Ext::from_f64(2000.0));
        g.t_bills[c] = rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0));
        g.ent_name[c] = PbStr::from(format!("{__wsr_text_13933}{c}{__wsr_text_13934}",__wsr_text_13933=__content.string(13933), __wsr_text_13934=__content.string(13934)).as_str());
        g.ent_symbol[c] = PbStr::from(format!("C{c}").as_str());
        g.credit_rating_num[c] = cast::<i32, _>(r.range(0, 10));
        // monthly price history (CalcStockVolatility), for most corps
        if r.chance(Ext::from_f64(0.75)) {
            g.cap_hist_close[(0, c)] = cast::<Ext, _>(r.range(1, 60));
            for m in 1..=60 {
                let close = g.market_cap[c] * rand_f(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4));
                g.cap_hist_close[(m, c)] = close;
                g.price_hist_high[(m, c)] = close * rand_f(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.2));
                g.price_hist_low[(m, c)] = close * rand_f(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.0));
            }
        }
    }
    // recently changed dividends
    let mut list = Vec::new();
    for &c in &corps {
        if r.chance(Ext::from_f64(0.2)) {
            list.push(format!("{c:04}"));
        }
    }
    g.recent_dividend_change_list = PbStr::from(list.join("|").as_str());
    let mut holders: Vec<i32> = (1..=g.num_players).collect();
    for &c in corps.iter().take(8) {
        holders.push(c);
    }
    // stock holdings of the holders (long and short) in the underlyings
    for &h in &holders {
        let mut n = 0;
        for &c in &corps {
            if c != h && r.chance(Ext::from_f64(0.12)) && n < 15 {
                n += 1;
                g.hold_stock_id[(n, h)] = cast::<i16, _>(c);
                let pct = cast::<i16, _>(r.range(1, 40));
                g.hold_stock_pct[(n, h)] = if r.chance(Ext::from_f64(0.25)) { -pct } else { pct };
                g.hold_stock_basis[(n, h)] = cast::<Ext, _>(r.range(1, 10000));
            }
        }
        g.hold_stock_id[(0, h)] = cast::<i16, _>(n);
    }
    // the option book: packed in 1..n, a few gaps further on
    let mut k = 0;
    for _ in 0..n_recs {
        let corp = corps[cast::<usize, _>(r.range(0, cast::<i64, _>(corps.len()) - 1))];
        let mut holder = holders[cast::<usize, _>(r.range(0, cast::<i64, _>(holders.len()) - 1))];
        if holder == corp {
            holder = 1;
        }
        let tag = match r.range(0, 9) {
            0 => Some("GRANT"),
            1 => Some("VESTED"),
            _ => None,
        };
        let tag = if holder <= 10 { tag } else { None };
        k += if r.chance(Ext::from_f64(0.05)) { 2 } else { 1 };
        g.option_recs[k] = PbStr::from(make_rec(&mut r, &g, corp, holder, tag).as_str());
    }
    let count = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
    g.option_recs[0] = PbStr::from(str15(cast::<Ext, _>(count)).trim());
    Book { g, corps, holders }
}

/// RND state both sides start from.
const RNG0: u32 = 0x1234_5678;

fn rng() -> Rng {
    Rng::from_state(RNG0)
}

fn run(addr: u32, args: &[String], g: &Globals) -> Option<Original> {
    run_skip(addr, args, g, &[])
}

fn run_skip(addr: u32, args: &[String], g: &Globals, skips: &[(u32, u32)]) -> Option<Original> {
    run_original_with(addr, &Opts { args: args.to_vec(), skips: skips.to_vec(), rng: Some(RNG0), ..Opts::default() }, g)
}

/// State comparison; `TickerPauseUntil` is set by PauseTicker (UI timing behind PostNews popups, not ported).
fn cmp() -> Compare {
    Compare::default().ignore("TickerPauseUntil")
}

fn a(x: i32) -> String {
    x.to_string()
}

fn v(x: i32) -> String {
    format!("v:{x}")
}

fn e(x: Ext) -> String {
    format!("e:{x:?}")
}

fn sarg(x: &str) -> String {
    format!("s:{x}")
}

/// Float results agree to within 80-bit vs f64 rounding.
#[track_caller]
fn assert_close(port: Ext, orig: Ext, what: &str) {
    let d = (port - orig).abs();
    assert!(d <= Ext::from_f64(1e-9) * port.abs().max(orig.abs()) || d < Ext::from_f64(1e-9), "{what}: port {port} vs original {orig}");
}

/// Indices of the non-empty records.
fn recs(g: &Globals) -> Vec<i32> {
    (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).collect()
}

fn pick<T: Copy>(r: &mut Rand, xs: &[T]) -> T {
    xs[cast::<usize, _>(r.range(0, cast::<i64, _>(xs.len()) - 1))]
}

/// Original log lines that are not harmless API calls.
fn ui_lines(o: &Original) -> Vec<&String> {
    o.log.iter().filter(|l| l.starts_with("MSGBOX") || l.starts_with("PBERR")).collect()
}

// ---------------------------------------------------------------------------------------------------------
// unit tests
// ---------------------------------------------------------------------------------------------------------

#[test]
fn spread_factor_cases() {
    // within 2%: 1.02 (SINGLE)
    assert_eq!(calc_spread_factor(1, Ext::from(Ext::from_f64(100.0)), Ext::from(Ext::from_f64(101.0))), cast::<Ext, _>(1.02f32));
    // 10% out of the money call: 1 + 0.4 * 0.1
    let x = calc_spread_factor(1, Ext::from_f64(100.0), Ext::from_f64(110.0));
    assert!((x - (Ext::from_f64(1.0) + cast::<Ext, _>(0.4f32) * Ext::from_f64(0.1))).abs() < Ext::from_f64(1e-15), "{x}");
    // deep in the money call capped at 1.05, deep out at 1.15
    assert_eq!(calc_spread_factor(1, Ext::from(Ext::from_f64(200.0)), Ext::from(Ext::from_f64(100.0))), cast::<Ext, _>(1.05f32));
    assert_eq!(calc_spread_factor(1, Ext::from(Ext::from_f64(100.0)), Ext::from(Ext::from_f64(300.0))), cast::<Ext, _>(1.15f32));
    assert_eq!(calc_spread_factor(2, Ext::from(Ext::from_f64(100.0)), Ext::from(Ext::from_f64(300.0))), cast::<Ext, _>(1.05f32));
    assert_eq!(calc_spread_factor(1, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(300.0))), Ext::from_f64(0.0)); // ERROR 151 path
}

#[test]
fn record_add_remove_and_counts() {
    let mut g = Globals::default();
    let mut ui = Rec::default();
    g.option_recs[0] = st("0");
    g.entity_status[20] = 1;
    add_or_remove_option_rec(&mut g, &mut ui, &st("20| 1| 3| 5| 100| 4| 2021| 7.5"), 0);
    add_or_remove_option_rec(&mut g, &mut ui, &st("20| 2| 3|-2| 90| 4| 2021|-3"), 0);
    assert_eq!(g.option_recs[0], st("2"));
    assert_eq!(g.option_recs[2], st("20| 2| 3|-2| 90| 4| 2021|-3"));
    assert_eq!(g.has_options_outstanding[20], 1);
    assert_eq!(get_optioned_stock_pct(&mut g, &mut ui, 3, 20, 0), 0); // long call, short put
    g.option_recs[1] = st("20| 1| 3|-5| 100| 4| 2021|-7.5");
    assert_eq!(get_optioned_stock_pct(&mut g, &mut ui, 3, 20, 0), 5);
    assert_eq!(get_max_options_pct(&g, 20, 1, &st("S"), 1), 95);
    assert_eq!(get_max_options_pct(&g, 20, 1, &st("S"), 0), 10);
    add_or_remove_option_rec(&mut g, &mut ui, &PbStr::new(), 1);
    assert_eq!(g.option_recs[0], st("1"));
    assert!(g.option_recs[1].is_empty());
    assert!(ui.msgs.is_empty());
}

#[test]
fn merge_sums_contracts_and_premiums() {
    let mut g = Globals::default();
    g.cur_year = 2021;
    g.option_recs[0] = st("1");
    g.option_recs[1] = st("20| 1| 3| 5| 100| 4| 2022| 7.5");
    assert_eq!(merge_matching_option_rec(&mut g, &st("20| 1| 3| 2| 100| 4| 2022| 1.25")), 1);
    assert_eq!(g.option_recs[1], st(" 20| 1| 3| 7| 100| 4| 2022| 8.75"));
    // opposite side: no merge
    assert_eq!(merge_matching_option_rec(&mut g, &st("20| 1| 3|-2| 100| 4| 2022|-1")), 0);
}

#[test]
fn sort_by_expiry() {
    let mut g = Globals::default();
    let rs = ["20| 1| 3| 5| 100| 7| 2022| 1", "20| 1| 3| 5| 100| 4| 2022| 2", "20| 1| 3| 5| 100| 10| 2021| 3"];
    for (i, x) in rs.iter().enumerate() {
        g.option_recs[cast::<i32, _>(i) + 1] = st(x);
    }
    g.option_recs[0] = st("3");
    sort_option_recs(&mut g, &mut Rec::default());
    assert_eq!(g.option_recs[1], st(rs[2]));
    assert_eq!(g.option_recs[2], st(rs[1]));
    assert_eq!(g.option_recs[3], st(rs[0]));
}

#[test]
fn cur_month_and_dividend_yield() {
    let mut g = Globals::default();
    g.num_players = 2;
    g.cur_quarter = 3;
    assert_eq!(calc_cur_month_of_year(&mut g, 750), Ext::from_f64(7.5));
    assert_eq!(calc_cur_month_of_year(&mut g, 2), Ext::from_f64(9.0));
    g.cur_quarter = 0;
    assert_eq!(calc_cur_month_of_year(&mut g, 3), cast::<Ext, _>(0.01f32));
    assert_eq!(g.cur_quarter, 1);
    g.entity_status[30] = 1;
    g.market_cap[30] = Ext::from_f64(200.0);
    g.shares_outstanding[30] = Ext::from_f64(4.0);
    g.dividend_per_share[30] = Ext::from_f64(1.5); // 1.5 * 100 * 4 / 200 = 3%
    assert_eq!(calc_dividend_yield(&mut g, 30), Ext::from_f64(3.0));
    g.dividend_per_share[30] = Ext::from_f64(60.0); // 120%: reset
    assert_eq!(calc_dividend_yield(&mut g, 30), Ext::from_f64(0.0));
    assert_eq!(g.dividend_per_share[30], Ext::from_f64(0.0));
}

// ---------------------------------------------------------------------------------------------------------
// differential tests
// ---------------------------------------------------------------------------------------------------------

fn diff_option_price(seed: u64) {
    let __content = wsr_core::content::current();
    let b = book(seed, 40);
    let mut r = Rand(seed + 1000);
    let rs = recs(&b.g);
    for k in 0..6 {
        let mut g = b.g.clone();
        let (args, from_rec) = if k % 2 == 0 {
            let ri = pick(&mut r, &rs);
            (vec![a(0), e(Ext::from(Ext::from_f64(0.0))), a(0), a(0), a(0), a(ri)], ri)
        } else {
            let corp = pick(&mut r, &b.corps);
            let strike = g.market_cap[corp] * rand_f(&mut r, Ext::from_f64(0.2), Ext::from_f64(3.0));
            let ty = cast::<i32, _>(r.range(1, 2));
            let year = g.cur_year + cast::<i32, _>(r.range(0, 3));
            let month = cast::<i32, _>(r.range(1, 12));
            if r.chance(Ext::from_f64(0.2)) {
                g.sim_co_count = corp - 1;
            }
            (vec![a(corp), e(-strike), a(year), a(month), a(ty), a(0)], 0)
        };
        let Some(o) = run(CALC_OPTION_PRICE, &args, &g) else { return };
        let mut port = g.clone();
        let p = if from_rec > 0 {
            calc_option_price(&mut port, &mut Rec::default(), &mut rng(), 0, Ext::from_f64(0.0), 0, 0, 0, from_rec)
        } else {
            let n: Vec<i32> = [0, 2, 3, 4].iter().map(|&i| args[i].parse().unwrap()).collect();
            let strike: Ext = args[1][2..].parse().unwrap();
            calc_option_price(&mut port, &mut Rec::default(), &mut rng(), n[0], strike, n[1], n[2], n[3], 0)
        };
        assert!(ui_lines(&o).is_empty(), "{:?}", o.log);
        assert_close(p, o.return_float.unwrap(), &format!("seed {seed}{__wsr_text_13935}{k} {args:?}",__wsr_text_13935=__content.string(13935)));
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn calc_option_price_matches_original_1() {
    for seed in 1..4 {
        diff_option_price(seed);
    }
}

#[test]
fn calc_option_price_matches_original_2() {
    for seed in 4..7 {
        diff_option_price(seed);
    }
}

/// Every record of a book, priced in one emulated run per record: covers all expiry buckets.
#[test]
fn calc_option_price_all_buckets() {
    let mut b = book(77, 0);
    let mut r = Rand(77);
    let corp = b.corps[3];
    // with a price history: CalcStockVolatility (VolAdjust) takes part
    b.g.cap_hist_close[(0, corp)] = Ext::from_f64(7.0);
    for m in 1..=60 {
        b.g.cap_hist_close[(m, corp)] = b.g.market_cap[corp] * (Ext::from_f64(1.0) + Ext::from_f64(0.01) * cast::<Ext, _>(m % 9));
        b.g.price_hist_high[(m, corp)] = b.g.market_cap[corp] * (Ext::from_f64(1.1) + Ext::from_f64(0.02) * cast::<Ext, _>(m % 5));
        b.g.price_hist_low[(m, corp)] = b.g.market_cap[corp] * (Ext::from_f64(0.9) - Ext::from_f64(0.03) * cast::<Ext, _>(m % 4));
    }
    b.g.dividend_per_share[corp] = Ext::from_f64(0.03) * b.g.market_cap[corp] / b.g.shares_outstanding[corp];
    // whole months -1..26 for calls and puts, in and out of the money
    let mut k = 1;
    for months in -1..27 {
        for ty in 1..=2 {
            let strike = b.g.market_cap[corp] * pick(&mut r, &[Ext::from_f64(0.5), Ext::from_f64(0.97), Ext::from_f64(1.03), Ext::from_f64(1.9), Ext::from_f64(2.8)]);
            let total = b.g.cur_year * 12 + months;
            let rec = format!("{corp}| {ty}| 1| 3| {}| {}| {}| 1", str15(strike).trim(), total % 12 + 1, total / 12);
            b.g.option_recs[k] = st(&rec);
            k += 1;
        }
    }
    b.g.option_recs[0] = PbStr::from(str15(cast::<Ext, _>(k - 1)).trim());
    for ri in 1..k {
        let Some(o) = run(CALC_OPTION_PRICE, &[a(0), e(Ext::from_f64(0.0)), a(0), a(0), a(0), a(ri)], &b.g) else { return };
        let mut port = b.g.clone();
        let p = calc_option_price(&mut port, &mut Rec::default(), &mut rng(), 0, Ext::from_f64(0.0), 0, 0, 0, ri);
        assert_close(p, o.return_float.unwrap(), &format!("rec {ri}: {}", b.g.option_recs[ri]));
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn calc_spread_factor_matches_original() {
    let g = Globals::default();
    let mut r = Rand(5);
    for _ in 0..8 {
        let ty = cast::<i32, _>(r.range(1, 2));
        let sv = rand_f(&mut r, Ext::from_f64(1.0), Ext::from_f64(1000.0));
        let kv = sv * rand_f(&mut r, Ext::from_f64(0.3), Ext::from_f64(2.5)) * if r.chance(Ext::from_f64(0.3)) { -Ext::from_f64(1.0) } else { Ext::from_f64(1.0) };
        let Some(o) = run(CALC_SPREAD_FACTOR, &[a(ty), e(sv), e(kv)], &g) else { return };
        assert_close(calc_spread_factor(ty, sv, kv), o.return_float.unwrap(), "spread");
    }
}

#[test]
fn calc_options_value_matches_original() {
    for seed in 10..14 {
        let b = book(seed, 60);
        let mut r = Rand(seed);
        for _ in 0..2 {
            let holder = pick(&mut r, &b.holders);
            let Some(o) = run(CALC_OPTIONS_VALUE, &[a(holder)], &b.g) else { return };
            let mut port = b.g.clone();
            calc_options_value(&mut port, &mut Rec::default(), &mut rng(), holder);
            assert!(ui_lines(&o).is_empty(), "{:?}", o.log);
            cmp().rel_tol(Ext::from_f64(1e-10)).assert(&port, &o.globals);
        }
    }
}

#[test]
fn calc_vested_options_value_matches_original() {
    let mut hits = 0;
    for seed in 20..28 {
        let mut b = book(seed, 30);
        let mut r = Rand(seed);
        let corp = pick(&mut r, &b.corps);
        for i in recs(&b.g) {
            let x = b.g.option_recs[i].to_display();
            if x.ends_with("|VESTED") && x.split('|').next().unwrap().trim() == corp.to_string() {
                b.g.option_recs[i] = PbStr::new();
            }
        }
        // place the simulation clock so that the option vests now (current or next quarter), mostly
        let (sim, next) = match (corp > 250, r.range(0, 2)) {
            (true, 0) => (cast::<i32, _>(r.range(1, 250)), false),
            (true, 1) => (cast::<i32, _>(r.range(cast::<i64, _>(corp), 1600)), true),
            (false, 0 | 1) => (cast::<i32, _>(r.range(cast::<i64, _>(corp), 250)), false),
            _ => (cast::<i32, _>(r.range(1, 1600)), r.chance(Ext::from_f64(0.5))),
        };
        b.g.sim_co_count = sim.max(b.g.num_players + 1);
        let q = b.g.cur_quarter + cast::<i32, _>(next);
        let (q, y) = if q == 5 { (1, b.g.cur_year + 1) } else { (q, b.g.cur_year) };
        let strike = b.g.market_cap[corp] * rand_f(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.2));
        let rec = format!("{corp}| 1| 1| 4| {}| {}| {}|VESTED", str15(strike).trim(), 3 * q - 2, y);
        b.g.option_recs[999] = st(&rec);
        b.g.option_recs[0] = st("990");
        let Some(o) = run(CALC_VESTED_OPTIONS_VALUE, &[a(corp)], &b.g) else { return };
        let mut port = b.g.clone();
        let x = calc_vested_options_value(&mut port, corp);
        if x != Ext::from_f64(0.0) {
            hits += 1;
        }
        assert_close(x, o.return_float.unwrap_or(Ext::from_f64(0.0)), &format!("seed {seed}"));
        cmp().assert(&port, &o.globals);
    }
    assert!(hits > 0, "no vested value exercised");
}

#[test]
fn get_plr_naked_liab_matches_original() {
    let __content = wsr_core::content::current();
    let mut nonzero = 0;
    for seed in 30..36 {
        let b = book(seed, 80);
        let mut r = Rand(seed);
        for k in 0..3 {
            let writer = pick(&mut r, &b.holders);
            let (stock, ty) = if k == 0 { (0, 0) } else { (pick(&mut r, &b.corps), cast::<i32, _>(r.range(1, 2))) };
            let Some(o) = run(GET_PLR_NAKED_LIAB, &[a(writer), a(stock), a(ty)], &b.g) else { return };
            let mut port = b.g.clone();
            let mut ui = Rec::default();
            let x = get_plr_naked_liab(&mut port, &mut ui, &mut rng(), writer, stock, ty);
            if x != Ext::from_f64(0.0) {
                nonzero += 1;
            }
            assert!(ui_lines(&o).is_empty() && ui.msgs.is_empty(), "{:?} {:?}", o.log, ui.msgs);
            assert_close(x, o.return_float.unwrap(), &format!("seed {seed} writer {writer}{__wsr_text_13936}{stock}{__wsr_text_13737}{ty}",__wsr_text_13936=__content.string(13936), __wsr_text_13737=__content.string(13737)));
            cmp().assert(&port, &o.globals);
        }
    }
    assert!(nonzero > 3, "too few liabilities exercised: {nonzero}");
}

#[test]
fn position_queries_match_original() {
    for seed in 40..46 {
        let b = book(seed, 80);
        let mut r = Rand(seed);
        for _ in 0..2 {
            // pick a (holder, corp) pair from a record so there is something to count
            let ri = pick(&mut r, &recs(&b.g));
            let rec = b.g.option_recs[ri].as_bytes().to_vec();
            let fields: Vec<Ext> = String::from_utf8_lossy(&rec).split('|').map(|x| x.trim().parse().unwrap_or(Ext::from_f64(0.0))).collect();
            let (corp, holder) = (cast::<i32, _>(fields[0]), cast::<i32, _>(fields[2]));
            let calls_only = cast::<i32, _>(r.range(0, 1));
            let Some(o) = run(GET_OPTIONED_STOCK_PCT, &[v(holder), v(corp), v(calls_only)], &b.g) else { return };
            let mut port = b.g.clone();
            let x = get_optioned_stock_pct(&mut port, &mut Rec::default(), holder, corp, calls_only);
            assert_eq!(x, cast::<i32, _>(o.return_value), "GetOptionedStockPct seed {seed}");
            cmp().assert(&port, &o.globals);

            let Some(o) = run(GET_NET_STOCK_PCT_AFTER_OPTIONS, &[a(holder), a(corp), a(calls_only)], &b.g) else { return };
            let mut port = b.g.clone();
            let x = get_net_stock_pct_after_options(&mut port, &mut Rec::default(), holder, corp, calls_only);
            assert_eq!(x, cast::<i32, _>(o.return_value), "GetNetStockPctAfterOptions seed {seed}");
            cmp().assert(&port, &o.globals);

            let ty = cast::<i32, _>(r.range(1, 2));
            let side = if r.chance(Ext::from_f64(0.5)) { "L" } else { "S" };
            let over = cast::<i32, _>(r.range(0, 1));
            let Some(o) = run(GET_MAX_OPTIONS_PCT, &[a(corp), a(ty), sarg(side), a(over)], &b.g) else { return };
            let x = get_max_options_pct(&b.g, corp, ty, &st(side), over);
            assert_eq!(x, cast::<i32, _>(o.return_value), "GetMaxOptionsPct seed {seed} {side} {ty} {over}");
            cmp().assert(&b.g, &o.globals);
        }
    }
}

#[test]
fn record_maintenance_matches_original() {
    for seed in 50..55 {
        let b = book(seed, 50);
        let mut r = Rand(seed);
        // add
        let corp = pick(&mut r, &b.corps);
        let text = make_rec(&mut r, &b.g, corp, 1, None);
        let Some(o) = run(ADD_OR_REMOVE_OPTION_REC, &[sarg(&text), a(0)], &b.g) else { return };
        let mut port = b.g.clone();
        add_or_remove_option_rec(&mut port, &mut Rec::default(), &st(&text), 0);
        cmp().assert(&port, &o.globals);
        // remove
        let ri = pick(&mut r, &recs(&b.g));
        let Some(o) = run(ADD_OR_REMOVE_OPTION_REC, &[sarg(""), a(ri)], &b.g) else { return };
        let mut port = b.g.clone();
        add_or_remove_option_rec(&mut port, &mut Rec::default(), &PbStr::new(), ri);
        cmp().assert(&port, &o.globals);
        // merge: a copy of an existing record with another size / premium
        let ri = pick(&mut r, &recs(&b.g));
        let old = b.g.option_recs[ri].to_display();
        let mut f: Vec<String> = old.split('|').map(String::from).collect();
        let q: i64 = f[3].trim().parse().unwrap();
        f[3] = str15(cast::<Ext, _>(q.signum() * r.range(1, 60)));
        if !f[7].contains("GRANT") && !f[7].contains("VESTED") {
            f[7] = str15(cast::<Ext, _>(q.signum() * r.range(1, 5000)) / Ext::from_f64(100.0));
        }
        let new = f.join("|");
        let Some(o) = run(MERGE_MATCHING_OPTION_REC, &[sarg(&new)], &b.g) else { return };
        let mut port = b.g.clone();
        let x = merge_matching_option_rec(&mut port, &st(&new));
        assert_eq!(x, cast::<i32, _>(o.return_value), "merge {new}");
        cmp().assert(&port, &o.globals);
        // sort
        let Some(o) = run(SORT_OPTION_RECS, &[], &b.g) else { return };
        let mut port = b.g.clone();
        sort_option_recs(&mut port, &mut Rec::default());
        cmp().assert(&port, &o.globals);
        // transfer
        let old_h = pick(&mut r, &b.holders);
        let new_h = pick(&mut r, &b.corps);
        let Some(o) = run(TRANSFER_OPTIONS_HOLDER, &[v(old_h), v(new_h), a(1), v(0)], &b.g) else { return };
        let mut port = b.g.clone();
        transfer_options_holder(&mut port, &mut Rec::default(), old_h, new_h, 1);
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn settle_option_rec_matches_original() {
    for seed in 60..68 {
        let b = book(seed, 40);
        let mut r = Rand(seed);
        for _ in 0..2 {
            let ri = pick(&mut r, &recs(&b.g));
            let Some(o) = run(SETTLE_OPTION_REC, &[a(ri)], &b.g) else { return };
            let mut port = b.g.clone();
            let mut ui = Rec::default();
            let x = settle_option_rec(&mut port, &mut ui, ri);
            assert_eq!(x, cast::<i32, _>(o.return_value), "seed {seed} rec {}", b.g.option_recs[ri]);
            cmp().assert(&port, &o.globals);
        }
    }
}

#[test]
fn close_option_positions_holder_matches_original() {
    let mut closed = 0;
    for seed in 70..78 {
        let b = book(seed, 60);
        let mut r = Rand(seed);
        for _ in 0..3 {
            let mut g = b.g.clone();
            let holder = pick(&mut r, &b.holders);
            let corp = if r.chance(Ext::from_f64(0.5)) { 0 } else { pick(&mut r, &b.corps) };
            let filter = cast::<i32, _>(r.range(0, 4));
            g.tmp_ext1 = if r.chance(Ext::from_f64(0.3)) { Ext::from_f64(99.0) } else { Ext::from_f64(0.0) };
            g.tmp_ext2 = if r.chance(Ext::from_f64(0.3)) { Ext::from_f64(99.0) } else { Ext::from_f64(0.0) };
            let Some(o) = run(CLOSE_OPTION_POSITIONS, &[v(holder), v(corp), v(0), v(filter)], &g) else { return };
            let mut port = g.clone();
            let x = close_option_positions(&mut port, &mut Rec::default(), &mut rng(), holder, corp, 0, filter);
            if x != 0 {
                closed += 1;
            }
            assert_eq!(x, cast::<i32, _>(o.return_value), "seed {seed} holder {holder} corp {corp} filter {filter}");
            cmp().assert(&port, &o.globals);
        }
    }
    assert!(closed > 3, "too few closings exercised: {closed}");
}

#[test]
fn close_option_positions_corp_matches_original() {
    for seed in 80..86 {
        let b = book(seed, 60);
        let mut r = Rand(seed);
        let ri = pick(&mut r, &recs(&b.g));
        let corp: i32 = b.g.option_recs[ri].to_display().split('|').next().unwrap().trim().parse().unwrap();
        let mode = cast::<i32, _>(seed % 2);
        let Some(o) = run(CLOSE_OPTION_POSITIONS, &[v(0), v(corp), v(mode), v(0)], &b.g) else { return };
        let mut port = b.g.clone();
        let mut ui = Rec::default();
        let x = close_option_positions(&mut port, &mut ui, &mut rng(), 0, corp, mode, 0);
        assert_eq!(x, cast::<i32, _>(o.return_value), "seed {seed} corp {corp} mode {mode}");
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn check_naked_option_margin_matches_original() {
    let mut forced = 0;
    for seed in 90..100 {
        let mut b = book(seed, 120);
        let mut r = Rand(seed);
        // a corp writer with many short options
        let writer = b.holders[cast::<usize, _>(b.g.num_players) + cast::<usize, _>(r.range(0, 7))];
        for k in 0..12 {
            let corp = pick(&mut r, &b.corps);
            if corp == writer {
                continue;
            }
            let rec = make_rec(&mut r, &b.g, corp, writer, None);
            let mut f: Vec<String> = rec.split('|').map(String::from).collect();
            let q: i64 = f[3].trim().parse().unwrap();
            f[3] = str15(cast::<Ext, _>(-(q.abs())));
            f[7] = str15(-(cast::<Ext, _>(r.range(1, 1000))) / Ext::from_f64(10.0));
            b.g.option_recs[900 + k] = st(&f.join("|"));
        }
        let count = recs(&b.g).len();
        b.g.option_recs[0] = PbStr::from(str15(cast::<Ext, _>(count)).trim());
        // CalcEntityFinancials is skipped on both sides: its results are part of the state
        b.g.tmp_credit_net_worth = rand_f(&mut r, -Ext::from_f64(100.0), Ext::from_f64(3000.0));
        b.g.net_worth = rand_f(&mut r, -Ext::from_f64(100.0), Ext::from_f64(3000.0));
        b.g.long_options_value = rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
        b.g.options_net_value = rand_f(&mut r, -Ext::from_f64(2000.0), Ext::from_f64(10.0));
        b.g.credit_rating_num[writer] = cast::<i32, _>(r.range(0, 10));
        if r.chance(Ext::from_f64(0.5)) {
            b.g.active_entity = writer;
        }
        b.g.cur_player = if r.chance(Ext::from_f64(0.5)) { b.g.corp_controller[writer].min(b.g.num_players) } else { b.g.cur_player };
        let Some(o) = run_skip(CHECK_NAKED_OPTION_MARGIN, &[a(writer)], &b.g, &[(CALC_ENTITY_FINANCIALS, 1)]) else {
            return;
        };
        let mut port = b.g.clone();
        let mut ui = Rec::default();
        check_naked_option_margin_with(&mut port, &mut ui, &mut rng(), writer, &mut |_, _, _, _| {});
        let before = recs(&b.g).len();
        if recs(&port).len() != before || port.option_recs.as_slice() != b.g.option_recs.as_slice() {
            forced += 1;
        }
        let orig_msgs = o.log.iter().filter(|l| l.contains("TMP_QM")).count();
        eprintln!("seed {seed}: writer {writer}, {} port popups, {} news, original QM writes {orig_msgs}", ui.msgs.len(), ui.news.len());
        cmp().assert(&port, &o.globals);
    }
    assert!(forced > 1, "too few forced cover-ups: {forced}");
}

#[test]
fn helpers_match_original() {
    for seed in 110..116 {
        let b = book(seed, 0);
        let mut r = Rand(seed);
        let mut g = b.g.clone();
        g.cur_quarter = cast::<i32, _>(r.range(0, 4));
        let p = cast::<i32, _>(r.range(0, 1700));
        let Some(o) = run(CALC_CUR_MONTH_OF_YEAR, &[a(p)], &g) else { return };
        let mut port = g.clone();
        assert_close(calc_cur_month_of_year(&mut port, p), o.return_float.unwrap(), "month");
        cmp().assert(&port, &o.globals);

        for id in [pick(&mut r, &b.corps), 1, pick(&mut r, &b.corps)] {
            let mut g = g.clone();
            g.cash[id] = rand_f(&mut r, -Ext::from_f64(500.0), Ext::from_f64(500.0));
            g.t_bills[id] = rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0));
            let Some(o) = run(BALANCE_CASH_TBILLS, &[a(id)], &g) else { return };
            let mut port = g.clone();
            balance_cash_t_bills(&mut port, id);
            cmp().assert(&port, &o.globals);
        }
    }
}
