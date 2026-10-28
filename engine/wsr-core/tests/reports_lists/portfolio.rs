//! ShowPortfolioListing (0x4422a4) and ShowOptionPositions (0x5f51fc): unit and differential tests.
use wsr_core::pb::real::{Ext, cast};


use super::common::{assert_reports, cmp, orig_msgs, orig_reports, port_reports, st, Rec};
use super::diff::{Compare, Opts, Original, Rand, run_original_with};
use wsr_core::logic::reports_lists::{show_option_positions, show_portfolio_listing};
use wsr_core::pb::numeric::str_num;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;
use wsr_core::ui::id;

const SHOW_PORTFOLIO_LISTING: u32 = 0x4422a4;
const SHOW_OPTION_POSITIONS: u32 = 0x5f51fc;
const SKIP_ALERTS: (u32, u32) = (0x6830fc, 1);

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

fn str15(x: Ext) -> String {
    str_num(x, 15)
}

fn rand_f(r: &mut Rand, lo: Ext, hi: Ext) -> Ext {
    lo + cast::<Ext, _>(r.next() >> 11) / cast::<Ext, _>(1u64 << 53) * (hi - lo)
}

/// Floats: EXT is f64 in the port.
fn cmpf() -> Compare {
    cmp().rel_tol(Ext::from_f64(1e-12))
}

fn run(addr: u32, args: &[String], g: &Globals, rng: u32, answers: &[&str]) -> Option<Original> {
    run_original_with(addr, &Opts { args: args.to_vec(), rng: Some(rng), answers, ..Opts::default() }, g)
}

fn currency(g: &mut Globals, r: &mut Rand) {
    let __content = wsr_core::content::current();
    let (name, suffix, sym) = [
        ("Dollars", "", "$"),
        ("Pounds", " Pound", "£"),
        ("Yen", " Yen", "¥"),
        ("Euros", " Euro", "€"),
        (__content.string(14001), " SwFr", "SFr"),
        ("Dollars", " Dollar", "$"),
    ][cast::<usize, _>(r.range(0, 5))];
    g.currency_name = s(name);
    g.currency_suffix = s(suffix);
    g.currency_symbol = s(sym);
    g.million_prefix_uc = s("M");
    g.million_prefix_lc = s("m");
}

// --------------------------------------------------------------------------------- option book

/// One option record in the game's format (`STR$` fields).
fn make_rec(r: &mut Rand, g: &Globals, corp: i32, holder: i32, tag: Option<&str>) -> String {
    let opt_type = r.range(1, 2);
    let mut qty = r.range(1, 25);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        qty = -qty;
    }
    let mc = g.market_cap[corp];
    let strike = (mc * rand_f(r, Ext::from_f64(0.3), Ext::from_f64(2.6)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
    let month = [1, 4, 7, 10, r.range(1, 12)][cast::<usize, _>(r.range(0, 4))];
    let year = g.cur_year + cast::<i32, _>(r.range(0, 2));
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

struct Book {
    g: Globals,
    corps: Vec<i32>,
    holders: Vec<i32>,
}

/// A random option book on a random market (like tests/options.rs), with names and currency.
fn book(seed: u64, n_recs: i64) -> Book {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_year = 2020 + cast::<i32, _>(r.range(0, 5));
    g.start_year = 2019;
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    g.sim_co_count = cast::<i32, _>(r.range(1, 1600));
    g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(r.range(15, 140)) / Ext::from_f64(10.0));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.active_entity = g.cur_player;
    g.long_bond_desc = s(&format!("{} LONG T-BOND  7.25", g.cur_year + 20));
    g.short_bond_desc = s(&format!("{} SHORT T-NOTE 04.50", g.cur_year + 3));
    g.long_bond_price = cast::<f32, _>(Ext::from_f64(97.5));
    g.short_bond_price = cast::<f32, _>(Ext::from_f64(101.25));
    currency(&mut g, &mut r);
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
        g.dividend_per_share[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { px * rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.08)) } else { Ext::from_f64(0.0) };
        g.ent_misc_data[c] = PbStr::from(
            format!("a|b|c|d|e|f|g|h|i|j|k|{}{__wsr_text_13932}", str15(g.market_cap[c] * rand_f(&mut r, Ext::from(Ext::from_f64(0.0)), Ext::from(Ext::from_f64(0.02)))).trim(),__wsr_text_13932=__content.string(13932)).as_str(),
        );
        g.corp_controller[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { c };
        g.cash[c] = rand_f(&mut r, -Ext::from_f64(100.0), Ext::from_f64(2000.0));
        g.t_bills[c] = rand_f(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0));
        g.ent_name[c] = PbStr::from(format!("{__wsr_text_14002}{c}{__wsr_text_14003}",__wsr_text_14002=__content.string(14002), __wsr_text_14003=__content.string(14003)).as_str());
        g.ent_symbol[c] = PbStr::from(format!("C{c}").as_str());
        g.credit_rating_num[c] = cast::<i32, _>(r.range(0, 10));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.75))) {
            g.cap_hist_close[(0, c)] = cast::<Ext, _>(r.range(1, 60));
            for m in 1..=60 {
                let close = g.market_cap[c] * rand_f(&mut r, Ext::from_f64(0.6), Ext::from_f64(1.4));
                g.cap_hist_close[(m, c)] = close;
                g.price_hist_high[(m, c)] = close * rand_f(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.2));
                g.price_hist_low[(m, c)] = close * rand_f(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.0));
            }
        }
    }
    let mut holders: Vec<i32> = (1..=g.num_players).collect();
    for &c in corps.iter().take(8) {
        holders.push(c);
    }
    for &h in &holders {
        let mut n = 0;
        for &c in &corps {
            if c != h && r.chance(cast::<f64, _>(Ext::from_f64(0.12))) && n < 15 {
                n += 1;
                g.hold_stock_id[(n, h)] = cast::<i16, _>(c);
                let pct = cast::<i16, _>(r.range(1, 40));
                g.hold_stock_pct[(n, h)] = if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) { -pct } else { pct };
                g.hold_stock_basis[(n, h)] = cast::<Ext, _>(r.range(1, 10000));
            }
        }
        g.hold_stock_id[(0, h)] = cast::<i16, _>(n);
    }
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
        k += 1;
        g.option_recs[k] = PbStr::from(make_rec(&mut r, &g, corp, holder, tag).as_str());
    }
    let count = (1..=1000).filter(|&i| !g.option_recs[i].is_empty()).count();
    g.option_recs[0] = PbStr::from(str15(cast::<Ext, _>(count)).trim());
    Book { g, corps, holders }
}

/// Runs both sides of ShowOptionPositions and compares reports, messages, return value, RND and state.
fn check_option_positions(g: &Globals, holder: i32, list_mode: i32, corp_filter: i32, rng0: u32, answer: Option<i32>) {
    let __content = wsr_core::content::current();
    let answers: Vec<&str> = match answer {
        Some(id::NO) => vec!["IDNO"],
        Some(_) => vec!["IDYES"],
        None => vec![],
    };
    let args = vec![format!("v:{holder}"), format!("v:{list_mode}"), format!("v:{corp_filter}")];
    let Some(o) = run(SHOW_OPTION_POSITIONS, &args, g, rng0, &answers) else { return };
    let mut port = g.clone();
    let mut ui = Rec::default();
    if let Some(a) = answer {
        ui.answers.push_back(a);
    }
    let mut rng = Rng::from_state(rng0);
    let ret = show_option_positions(&mut port, &mut ui, &mut rng, holder, list_mode, corp_filter);
    let orig = orig_reports(&o);
    eprintln!(
        "{__wsr_text_13986}{holder}{__wsr_text_13548}{list_mode} filter {corp_filter}: {}{__wsr_text_14004}{} lines), {} msgs, {:.2}{__wsr_text_13748}",
        orig.len(),
        orig.first().map_or(0, |r| r.1.len()),
        orig_msgs(&o),
        o.elapsed
    ,__wsr_text_13986=__content.string(13986), __wsr_text_13548=__content.string(13548), __wsr_text_14004=__content.string(14004), __wsr_text_13748=__content.string(13748));
    assert!(!o.log.iter().any(|l| l.starts_with("PBERR")), "{:?}", o.log);
    assert_reports(&port_reports(&ui, &[]), &orig);
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}{__wsr_text_13982}{:?}", ui.msgs, o.log,__wsr_text_13982=__content.string(13982));
    assert_eq!(cast::<u32, _>(ret), o.return_value, "return value");
    assert_eq!(rng.state(), o.rng, "RND state");
    cmpf().assert(&port, &o.globals);
}

#[test]
fn show_option_positions_matches_original() {
    let mut n = 0;
    for seed in 1u64..=5 {
        let b = book(seed, 25 + 10 * cast::<i64, _>(seed));
        let mut r = Rand(seed * 7717);
        for k in 0..4 {
            let holder = b.holders[cast::<usize, _>(r.range(0, cast::<i64, _>(b.holders.len()) - 1))];
            let (mode, filter) = if k == 0 { (0, 0) } else { (cast::<i32, _>(r.range(0, 4)), b.corps[cast::<usize, _>(r.range(0, 29))]) };
            check_option_positions(&b.g, holder, mode, if mode == 0 { 0 } else { filter }, cast::<u32, _>(r.next()), None);
            n += 1;
        }
    }
    eprintln!("{n} option listings compared");
}

#[test]
fn show_option_positions_without_positions() {
    let b = book(11, 30);
    // a corp without option records: offers the active entity's / the player's positions (YES / NO),
    // or the "Nothing to display" popup
    let no_opts = *b
        .corps
        .iter()
        .find(|&&c| (1..=1000).all(|i| !b.g.option_recs[i].to_display().contains(&format!("|{}|", str15(Ext::from(cast::<Ext, _>(c)))))))
        .unwrap();
    check_option_positions(&b.g, no_opts, 0, 0, 5, Some(id::YES));
    check_option_positions(&b.g, no_opts, 0, 0, 5, Some(id::NO));
    let mut g = b.g.clone();
    for i in 0..=1000 {
        g.option_recs[i] = PbStr::new();
    }
    g.option_recs[0] = s("0");
    check_option_positions(&g, no_opts, 0, 0, 5, None);
    check_option_positions(&g, g.cur_player, 0, 0, 5, None);
    // liquidated holder
    let mut g = b.g.clone();
    g.entity_status[no_opts] = 2;
    g.ent_misc_data[no_opts] = s("BKRUP");
    check_option_positions(&g, no_opts, 0, 0, 5, None);
}

// --------------------------------------------------------------------------------- portfolio world

/// A plausible mid-game world (like tests/finance.rs) with names, currency and holdings.
fn world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rand(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut g = Globals::default();
    let f32r = |r: &mut Rand, lo: Ext, hi: Ext| cast::<f32, _>(lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000)) / Ext::from_f64(1e6));
    let f64r = |r: &mut Rand, lo: Ext, hi: Ext| lo + (hi - lo) * cast::<Ext, _>(r.range(0, 1_000_000_000)) / Ext::from_f64(1e9);
    g.world_generated = 1;
    g.game_end_flag = s("N");
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.active_entity = g.cur_player;
    g.difficulty_level = cast::<i32, _>(r.range(1, 4));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    // quarter 1 or 3 and SimCoCount <= 500 keep bond-pricing exponents integral (harness limitation, see
    // tests/finance.rs pow_safe)
    g.cur_quarter = if seed.is_multiple_of(2) { 1 } else { 3 };
    g.sim_co_count = if seed.is_multiple_of(3) { 300 } else { 450 };
    g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(16.0)));
    g.prime_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, Ext::from_f64(0.5), Ext::from_f64(3.0)));
    g.sofr_rate = cast::<f32, _>(g.base_interest_rate + f32r(&mut r, -Ext::from_f64(0.5), Ext::from_f64(0.5)));
    g.gdp_growth = cast::<f32, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f32r(&mut r, -Ext::from_f64(14.0), -Ext::from_f64(10.5)) } else { f32r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(8.0)) });
    g.monetary_policy = cast::<i16, _>(r.range(0, 2));
    g.exchange_rate = cast::<f64, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { Ext::from_f64(1.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0)) });
    g.loan_limit_multiple = cast::<f64, _>((cast::<Ext, _>(r.range(20, 35))) * Ext::from_f64(0.1));
    g.corp_tax_rate = cast::<i32, _>(r.range(15, 45));
    g.bank_cd_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(9.0)));
    g.long_bond_desc = s(&format!("{} LONG T-BOND {:5.2}", g.cur_year + cast::<i32, _>(r.range(8, 30)), f32r(&mut r, Ext::from(Ext::from_f64(2.0)), Ext::from(Ext::from_f64(12.0)))));
    g.short_bond_desc = s(&format!("{} SHORT T-NOTE {:05.2}", g.cur_year + cast::<i32, _>(r.range(0, 9)), f32r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(10.0)))));
    g.long_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(70.0), Ext::from_f64(130.0)));
    g.short_bond_price = cast::<f32, _>(f32r(&mut r, Ext::from_f64(85.0), Ext::from_f64(110.0)));
    g.long_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(12.0)));
    g.short_bond_yield = cast::<f32, _>(f32r(&mut r, Ext::from_f64(1.0), Ext::from_f64(10.0)));
    currency(&mut g, &mut r);
    for p in 1..=5 {
        g.is_computer_player[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
    }
    for e in 1..=1600 {
        let active = if e <= 10 { e <= g.num_players } else { r.chance(cast::<f64, _>(Ext::from_f64(0.93))) };
        g.entity_status[e] = if active { 1 } else if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { 0 } else { 2 };
        g.ent_name[e] = s(&format!("{__wsr_text_13758}{e} {}", ["Corp", "Holdings International", "Co"][cast::<usize, _>(r.range(0, 2))],__wsr_text_13758=__content.string(13758)));
        g.ent_symbol[e] = s(&format!("E{e}"));
        g.cash[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.t_bills[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(1000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0)) } else { Ext::from_f64(0.0) };
        g.lending_bank[e] = cast::<i16, _>(r.range(11, 50));
        g.credit_rating_num[e] = cast::<i32, _>(r.range(0, 10));
        g.long_gov_bond_face[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { r.range(1, 500) } else { 0 };
        g.short_gov_bond_face[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { r.range(1, 500) } else { 0 };
        g.lt_gov_bond_cost[e] = cast::<Ext, _>(g.long_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2));
        g.st_gov_bond_cost[e] = cast::<Ext, _>(g.short_gov_bond_face[e]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.1));
        g.corp_controller[e] = match r.range(0, 5) {
            0 => 0,
            1 | 2 => cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))),
            _ => cast::<i32, _>(r.range(11, 1600)),
        };
        let mut n = 0;
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
                n = slot;
                g.hold_stock_id[(slot, e)] = cast::<i16, _>(r.range(11, 1600));
                let pct = cast::<i16, _>(r.range(1, 100));
                g.hold_stock_pct[(slot, e)] = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { -pct.min(30) } else { pct };
                g.hold_stock_basis[(slot, e)] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0)) } else { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)) };
            }
            if !(131..=1580).contains(&e) && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                g.hold_bond_id[(slot, e)] = cast::<i32, _>(r.range(11, 1600));
                g.hold_bond_face[(slot, e)] = r.range(1, 300);
                g.bond_hold_cost[(slot, e)] = cast::<Ext, _>(g.hold_bond_face[(slot, e)]) * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.2));
            }
        }
        g.hold_stock_id[(0, e)] = cast::<i16, _>(n);
        if e > 10 {
            g.industry_num[e] = match e {
                11..=85 => 1,
                86..=130 => 2,
                1581..=1583 => 71,
                _ => cast::<i16, _>(r.range(3, 70)),
            };
            g.market_cap[e] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
            g.shares_outstanding[e] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
            g.dividend_per_share[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.05)) } else { Ext::from_f64(0.0) };
            g.accrued_pl[e] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(200.0));
            g.earnings_y1[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(100.0));
            g.earnings_ytd[e] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(60.0));
            g.proj_qtr_earnings[e] = f64r(&mut r, -Ext::from_f64(10.0), Ext::from_f64(30.0));
            if e >= g.sim_co_count && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                g.bonds_issued[e] = r.range(1, 1500);
                let mat = g.cur_year + cast::<i32, _>(r.range(1, 25));
                let coupon = f32r(&mut r, Ext::from_f64(2.0), Ext::from_f64(15.0));
                g.bond_issue_info[e] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                    let cap = g.market_cap[e] * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(2.0));
                    s(&format!("{mat}|{cap:.2}|cv|{coupon:05.2}"))
                } else {
                    s(&format!("{mat}|0||{coupon:05.2}"))
                };
                g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(130.0)));
            }
        }
        if (131..=1600).contains(&e) {
            g.business_assets[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
            g.working_capital[e] = f64r(&mut r, -Ext::from_f64(100.0), Ext::from_f64(1000.0));
        }
        if (11..=85).contains(&e) {
            g.bank_demand_deposits[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(20000.0));
            g.bank_cust_deposits[e] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0));
        }
    }
    g
}

/// Holdings in corps below SimCoCount are fine; bonds held must be of issuers with bonds.
fn fix_bond_holdings(g: &mut Globals) {
    for e in 1..=1600 {
        for slot in 1..=15 {
            let b = g.hold_bond_id[(slot, e)];
            if b > 0 && g.bonds_issued[b] == 0 {
                g.hold_bond_id[(slot, e)] = 0;
                g.hold_bond_face[(slot, e)] = 0;
            }
        }
    }
}

fn check_portfolio(g: &Globals, entity: i32, bond_mode: i32, position_mode: i32, rng0: u32) {
    let __content = wsr_core::content::current();
    let args = vec![entity.to_string(), bond_mode.to_string(), position_mode.to_string()];
    let hooks = if std::env::var("RL_DEBUG").is_ok() { vec![0x426610, 0x494007, 0x41f71b, 0x49b447, 0x42c72a] } else { vec![] };
    // CheckPriceAlerts (called by CalcStockPrice via CalcAnalystRating) is skipped in the original;
    // ShowCashFlowProjection runs on both sides (logic::cashflow).
    let skips = vec![SKIP_ALERTS];
    let opts = Opts { args: args.clone(), rng: Some(rng0), hooks, skips, ..Opts::default() };
    let Some(o) = run_original_with(SHOW_PORTFOLIO_LISTING, &opts, g) else { return };
    if std::env::var("RL_DEBUG").is_ok() {
        for l in &o.log { eprintln!("LOG {l}"); }
    }
    let mut port = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    show_portfolio_listing(&mut port, &mut ui, &mut rng, entity, bond_mode, position_mode);
    let orig = orig_reports(&o);
    if std::env::var("RL_DUMP").is_ok() {
        for (_, l) in &orig {
            for x in l {
                eprintln!("DUMP {}", x.to_display());
            }
        }
    }
    eprintln!(
        "{__wsr_text_13847}{entity}{__wsr_text_13623}{}{__wsr_text_14005}{bond_mode} pos {position_mode}: {}{__wsr_text_14004}{} lines), {} msgs, {:.2}{__wsr_text_13748}",
        g.tmp_ext1,
        orig.len(),
        orig.iter().map(|r| r.1.len()).sum::<usize>(),
        orig_msgs(&o),
        o.elapsed
    ,__wsr_text_13847=__content.string(13847), __wsr_text_13623=__content.string(13623), __wsr_text_14005=__content.string(14005), __wsr_text_14004=__content.string(14004), __wsr_text_13748=__content.string(13748));
    assert!(!o.log.iter().any(|l| l.starts_with("PBERR")), "{:?}", o.log);
    assert_reports(&port_reports(&ui, &[]), &orig);
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages: port {:?}{__wsr_text_13982}{:?}", ui.msgs, o.log,__wsr_text_13982=__content.string(13982));
    assert_eq!(rng.state(), o.rng, "RND state");
    cmpf().assert(&port, &o.globals);
}

#[test]
fn show_portfolio_listing_matches_original() {
    let __content = wsr_core::content::current();
    let mut n = 0;
    for seed in 1u64..=8 {
        let mut g = world(seed);
        fix_bond_holdings(&mut g);
        let mut r = Rand(seed * 104_729);
        for k in 0..4 {
            let e = loop {
                let e = cast::<i32, _>(match k {
                    0 => r.range(1, cast::<i64, _>(g.num_players)),
                    1 => r.range(11, 130),
                    2 => r.range(131, 1580),
                    _ => [r.range(1, cast::<i64, _>(g.num_players)), r.range(1581, 1600)][cast::<usize, _>(r.range(0, 1))],
                });
                if g.entity_status[e] == 1 {
                    break e;
                }
            };
            let mut g = g.clone();
            let kind = cast::<Ext, _>(r.range(1, 3));
            g.tmp_ext1 = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { -kind } else { kind };
            g.entity_pick_mode = st(["", "GetCorp", __content.string(6093)][cast::<usize, _>(r.range(0, 2))]);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.cur_player = e.min(g.num_players);
            }
            let bond_mode = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { 1 } else { 0 };
            let pos = [0, 0, 1, -1][cast::<usize, _>(r.range(0, 3))];
            check_portfolio(&g, e, bond_mode, pos, cast::<u32, _>(r.next()));
            n += 1;
        }
    }
    eprintln!("{n} portfolio listings compared");
}

#[test]
fn show_portfolio_listing_with_options_and_edge_cases() {
    // listings with option positions; no stocks/bonds -> option listing
    for seed in 21u64..=24 {
        let mut g = world(seed);
        fix_bond_holdings(&mut g);
        let mut r = Rand(seed);
        let holders: Vec<i32> = (1..=g.num_players).chain((0..3).map(|_| cast::<i32, _>(r.range(131, 1580)))).collect();
        let corps: Vec<i32> = (0..12).map(|_| cast::<i32, _>(r.range(131, 1580))).filter(|&c| g.entity_status[c] == 1).collect();
        let mut k = 0;
        for _ in 0..30 {
            let corp = corps[cast::<usize, _>(r.range(0, cast::<i64, _>(corps.len()) - 1))];
            let holder = holders[cast::<usize, _>(r.range(0, cast::<i64, _>(holders.len()) - 1))];
            if holder == corp || g.entity_status[holder] != 1 {
                continue;
            }
            let tag = if holder <= 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { Some("GRANT") } else { None };
            k += 1;
            g.option_recs[k] = PbStr::from(make_rec(&mut r, &g, corp, holder, tag).as_str());
        }
        g.option_recs[0] = PbStr::from(str15(cast::<Ext, _>(k)).trim());
        for &h in holders.iter().take(4) {
            if g.entity_status[h] != 1 {
                continue;
            }
            let mut g = g.clone();
            g.tmp_ext1 = [Ext::from_f64(1.0), -Ext::from_f64(1.0), Ext::from_f64(3.0), -Ext::from_f64(3.0)][cast::<usize, _>(r.range(0, 3))];
            g.entity_pick_mode = st("GetCorp");
            check_portfolio(&g, h, 0, [0, 1, -1][cast::<usize, _>(r.range(0, 2))], cast::<u32, _>(r.next()));
        }
        // a holder without stocks or bonds
        let h = holders[0];
        for slot in 0..=15 {
            g.hold_stock_id[(slot, h)] = 0;
            g.hold_stock_pct[(slot, h)] = 0;
            g.hold_bond_id[(slot, h)] = 0;
        }
        g.long_gov_bond_face[h] = 0;
        g.short_gov_bond_face[h] = 0;
        g.tmp_ext1 = Ext::from_f64(1.0);
        check_portfolio(&g, h, 0, 0, 7);
    }
    // nothing at all: popup
    let mut g = world(31);
    let e = 1;
    for slot in 0..=15 {
        g.hold_stock_id[(slot, e)] = 0;
        g.hold_bond_id[(slot, e)] = 0;
    }
    g.long_gov_bond_face[e] = 0;
    g.short_gov_bond_face[e] = 0;
    for c in 11..=1600 {
        g.player_advances[(e, c)] = Ext::from_f64(0.0);
    }
    g.tmp_ext1 = Ext::from_f64(3.0);
    check_portfolio(&g, e, 0, 0, 9);
    // liquidated entity
    let mut g = world(32);
    g.entity_status[200] = 2;
    check_portfolio(&g, 200, 0, 0, 9);
}

#[test]
fn no_positions_popups() {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    g.num_players = 1;
    g.cur_player = 1;
    g.active_entity = 1;
    g.entity_status[1] = 1;
    g.ent_name[1] = s("Ann");
    g.lending_bank[1] = 11;
    g.entity_status[11] = 1;
    g.tmp_ext1 = Ext::from_f64(1.0);
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(1);
    show_portfolio_listing(&mut g, &mut ui, &mut rng, 1, 0, 0);
    assert_eq!(
        ui.msgs,
        vec![__content.string(14006)]
    );
    assert!(ui.reports.is_empty());
    assert_eq!(g.tmp_ext1, Ext::from_f64(0.0));
    let mut ui = Rec::default();
    assert_eq!(show_option_positions(&mut g, &mut ui, &mut rng, 1, 0, 0), 0);
    assert_eq!(ui.msgs, vec![__content.string(14445)]);
}
