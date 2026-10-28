//! Differential tests of `logic::sim_entity::fin_inst` (bond trading and liquidity of banks/insurers).
#![allow(unused_imports)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::fin_inst::*;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

const AI_FIN_INST: u32 = 0x4dff05;
const AI_SELL_JUNK: u32 = 0x4f721b;
const BUY_CORP_BONDS: u32 = 0x4f59c5;
const SELL_CORP_BONDS: u32 = 0x4f670d;
const INSURER_GOV: u32 = 0x4e5b4b;
const TRADE_GOV: u32 = 0x6a16ad;
const SUBPRIME: u32 = 0x508952;
const BANK_LIQ: u32 = 0x4e635d;

/// Callees skipped in the original: none any more.
const SKIPS: &[(u32, u32)] = &[];
/// Hooked callees (their calls are counted): BankLiquidityAndCapitalCheck, BorrowToCoverPurchase.
const HOOKS: &[u32] = &[0x5021e2, 0x5246b6];
static LIQ_CHECKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static BORROWS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Asserts that the hooked callee counter `n` was reached (when the harness ran).
fn require(n: &std::sync::atomic::AtomicUsize, what: &str) {
    let __content = wsr_core::content::current();
    let k = n.load(std::sync::atomic::Ordering::Relaxed);
    eprintln!("{__wsr_text_13574}{what}: {k}",__wsr_text_13574=__content.string(13574));
    if unavailable().is_none() {
        assert!(k > 0, "not reached: {what}");
    }
}

/// `common::world` with a bond market the institutions can trade: larger issues, holdings of banks and
/// insurers, government bonds, subprime securities.
fn fworld(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) ^ 0x5151);
    for e in 11..=1600 {
        if g.bonds_issued[e] > 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            g.bonds_issued[e] = r.range(1000, 9000);
            g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(40.0), Ext::from_f64(140.0)));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(140.0), Ext::from_f64(400.0)));
            }
        }
    }
    for h in 11..=130 {
        g.long_gov_bond_face[h] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { r.range(0, 6000) } else { 0 };
        g.short_gov_bond_face[h] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { r.range(0, 6000) } else { 0 };
        g.lt_gov_bond_cost[h] = cast::<Ext, _>(g.long_gov_bond_face[h]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        g.st_gov_bond_cost[h] = cast::<Ext, _>(g.short_gov_bond_face[h]) * f64r(&mut r, Ext::from_f64(0.9), Ext::from_f64(1.05));
        g.subprime_secs[h] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { r.range(0, 8000) } else { 0 };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.cash[h] = f64r(&mut r, -Ext::from_f64(3000.0), Ext::from_f64(500.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.cash[h] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(20000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.bank_debt[h] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(4000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.t_bills[h] = Ext::from_f64(0.0);
        }
        // bond holdings (more of them, some large)
        let mut n = 0;
        for slot in 1..=15 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                let b = cast::<i32, _>(r.range(131, 1580));
                if g.bonds_issued[b] > 0 {
                    g.hold_bond_id[(slot, h)] = b;
                    g.hold_bond_face[(slot, h)] = r.range(1, g.bonds_issued[b].max(1));
                    g.bond_hold_cost[(slot, h)] = cast::<Ext, _>(g.hold_bond_face[(slot, h)]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
                    n += 1;
                }
            }
        }
        if n > 0 {
            g.hold_bond_id[(0, h)] = n;
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.corp_controller[h] = h;
        }
    }
    // BankAllocate_SellLosingStocks would call SellStockHolding, whose control/fund cascade is not fully
    // ported yet (transactions.rs): keep the banks' stock holdings profitable.
    for h in 11..=85 {
        for slot in 1..=15 {
            let c = cast::<i32, _>(g.hold_stock_id[(slot, h)]);
            if c > 10 && g.earnings_ytd[c] < Ext::from_f64(0.0) {
                g.earnings_ytd[c] = -g.earnings_ytd[c];
            }
        }
    }
    g.subprime_crisis_state = [0, 0, 0, -1, 1, 2, 3][cast::<usize, _>(r.range(0, 6))];
    g.player_recent_corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i16, _>(r.range(131, 1580)) } else { 0 };
    g.tmp_accrued_inc_tax = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
    g.capital_taxes_total = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
    g.last_ticker_news_kind = cast::<i32, _>(r.range(0, 3));
    g
}

/// Runs the original at `addr` and the port on copies of `g` with the same RND state; compares state,
/// RND state, message-box count and (if `ret`) the float result.
fn check(
    addr: u32,
    args: &[&str],
    g: &Globals,
    rng0: u32,
    port: impl FnOnce(&mut Globals, &mut Rec, &mut Rng) -> Option<Ext>,
) -> Option<Globals> {
    let __content = wsr_core::content::current();
    let o = run_hooked(addr, args, g, rng0, &[], SKIPS, HOOKS)?;
    for (pat, n) in [("CALL BankLiquidityAndCapitalCheck(", &LIQ_CHECKS), ("CALL BorrowToCoverPurchase(", &BORROWS)] {
        if o.log.iter().any(|l| l.starts_with(pat)) {
            n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let mut p = g.clone();
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(rng0);
    let ret = port(&mut p, &mut ui, &mut rng);
    cmp().assert(&p, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state");
    assert_eq!(ui.msgs.len(), orig_msgs(&o), "message boxes: {:?} / {:?}", ui.msgs, o.log);
    if let Some(x) = ret {
        let y = o.return_float.expect("float result");
        assert!((x - y).abs() <= Ext::from_f64(1e-9) * x.abs().max(Ext::from(Ext::from_f64(1.0))), "{__wsr_text_13794}{x} vs {y}",__wsr_text_13794=__content.string(13794));
    }
    Some(p)
}

/// Counts seeds where `f(before, after)` holds; asserts at the end that it happened at least `min` times.
struct Hits(std::sync::atomic::AtomicUsize, String, usize);

impl Hits {
    fn new(what: &str, min: usize) -> Self {
        Hits(std::sync::atomic::AtomicUsize::new(0), what.to_owned(), min)
    }
    fn count(&self, hit: bool) {
        if hit {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn check(&self) {
    let __content = wsr_core::content::current();
        if super::diff::unavailable().is_none() {
            let n = self.0.load(std::sync::atomic::Ordering::Relaxed);
            assert!(n >= self.2, "{}{__wsr_text_13645}{n}{__wsr_text_13646}", self.1,__wsr_text_13645=__content.string(13645), __wsr_text_13646=__content.string(13646));
        }
    }
}

#[test]
fn diff_ai_fin_inst_bond_trading() {
    let __content = wsr_core::content::current();
    let (buys, sells) = (Hits::new(__content.string(14159), 4), Hits::new("bond sales", 1));
    par_seeds(1..=48, |seed| {
        let mut g = fworld(seed);
        let mut r = Rand(seed);
        let id = cast::<i32, _>(r.range(11, 130));
        g.sim_co_count = id;
        if seed % 2 == 0 {
            // a healthy institution with cash to invest: no junk in its bond portfolio
            let mut n = 0;
            for slot in 1..=15 {
                let b = g.hold_bond_id[(slot, id)];
                if b != 0 && (n >= 4 || g.credit_rating_num[b] < 3 || g.bond_price[b].get() > Ext::from_f64(150.0)) {
                    g.hold_bond_id[(slot, id)] = 0;
                    g.hold_bond_face[(slot, id)] = 0;
                } else if b != 0 {
                    n += 1;
                }
            }
            g.hold_bond_id[(0, id)] = n;
            g.cash[id] = f64r(&mut r, Ext::from_f64(6000.0), Ext::from_f64(30000.0));
            g.corp_controller[id] = id;
            for e in 131..=1580 {
                if g.bonds_issued[e] > 1500 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(97.0)));
                }
            }
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b9);
        let a = id.to_string();
        let p = check(AI_FIN_INST, &[&a], &g, rng0, |g, ui, rng| {
            ai_fin_inst_bond_trading(g, ui, rng, id);
            None
        });
        if let Some(p) = p {
            let bought = (1..=15).any(|s| p.hold_bond_face[(s, id)] > g.hold_bond_face[(s, id)]);
            let sold = (1..=15).any(|s| p.hold_bond_face[(s, id)] < g.hold_bond_face[(s, id)]);
            buys.count(bought);
            sells.count(sold);
        }
    });
    buys.check();
    sells.check();
}

#[test]
fn diff_ai_rand_sell_junk_bonds() {
    let sells = Hits::new("junk bond sales", 5);
    par_seeds(1..=30, |seed| {
        let mut g = fworld(seed + 100);
        let mut r = Rand(seed);
        g.sim_co_count = cast::<i32, _>(r.range(5, 135));
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x85eb_ca6b);
        let id = g.sim_co_count;
        let p = check(AI_SELL_JUNK, &[], &g, rng0, |g, ui, rng| {
            ai_rand_sell_junk_bonds(g, ui, rng);
            None
        });
        if let Some(p) = p {
            sells.count((1..=15).any(|s| p.hold_bond_face[(s, id)] < g.hold_bond_face[(s, id)]));
        }
    });
    sells.check();
}

#[test]
fn diff_buy_and_sell_corp_bonds() {
    let __content = wsr_core::content::current();
    let trades = Hits::new("corp bond trades", 12);
    par_seeds(1..=32, |seed| {
        let mut g = fworld(seed + 200);
        let mut r = Rand(seed);
        let mut buyer = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { cast::<i32, _>(r.range(11, 130)) };
        if seed % 4 == 1 {
            // the financed purchase (below): an insurer with a credit line
            buyer = 86 + (cast::<i32, _>(seed) * 7) % 45;
            g.credit_frozen[buyer] = 0;
            let lb = cast::<i32, _>(g.lending_bank[buyer]);
            g.credit_frozen[lb] = 0;
        }
        let mut issuer = cast::<i32, _>(r.range(131, 1580));
        for _ in 0..50 {
            if g.bonds_issued[issuer] > 0 && g.entity_status[issuer] == 1 {
                break;
            }
            issuer = cast::<i32, _>(r.range(131, 1580));
        }
        let mut mode = [0, 1, 2][cast::<usize, _>(r.range(0, 2))];
        if seed % 4 == 1 {
            mode = 0;
        }
        let sell = seed % 2 == 0;
        if sell && r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
            let slot = cast::<i32, _>(r.range(1, 15));
            g.hold_bond_id[(slot, buyer)] = issuer;
            g.hold_bond_face[(slot, buyer)] = r.range(1, 3000);
        }
        if !sell && mode == 0 {
            // a player order: within the buyer's cash, or short of cash (BorrowToCoverPurchase, YES)
            g.cash[buyer] = f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(40000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.news_msg = st(__content.string(13571));
        }
        g.tmp_ext1 = cast::<Ext, _>(r.range(50, 3000));
        g.tmp_ext2 = cast::<Ext, _>(mode);
        if !sell && mode == 0 && seed % 4 == 1 {
            // a smaller order with 60% of its cost in cash: the rest is borrowed
            let mut r2 = Rand(seed ^ 0xb0b);
            g.tmp_ext1 = cast::<Ext, _>(r2.range(50, 300));
            g.cash[buyer] = g.tmp_ext1 * g.bond_price[issuer].get() * Ext::from_f64(0.01) * Ext::from_f64(0.6);
            g.t_bills[buyer] = Ext::from_f64(0.0);
            g.bonds_public_held = 5000;
            g.active_entity = buyer;
            g.active_entity_name = g.ent_name[buyer].clone();
            if std::env::var_os("WSR_DBG").is_some() {
                let mut h = g.clone();
                wsr_core::logic::finance::calc_entity_financials(&mut h, &mut Rec::default(), &mut Rng::from_state(1), buyer);
                eprintln!("seed {seed}{__wsr_text_14157}{buyer}{__wsr_text_14160}{}{__wsr_text_14161}{}{__wsr_text_13849}{}{__wsr_text_14162}{}", h.cash[buyer], h.liquid_funds, h.unused_credit_line, h.entity_status[buyer],__wsr_text_14157=__content.string(14157), __wsr_text_14160=__content.string(14160), __wsr_text_14161=__content.string(14161), __wsr_text_13849=__content.string(13849), __wsr_text_14162=__content.string(14162));
            }
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0xc2b2_ae35);
        let (b, i) = (format!("{__wsr_text_13741}{buyer}",__wsr_text_13741=__content.string(13741)), format!("{__wsr_text_13741}{issuer}",__wsr_text_13741=__content.string(13741)));
        let p = if sell {
            check(SELL_CORP_BONDS, &[&b, &i], &g, rng0, |g, ui, rng| {
                sell_corp_bonds(g, ui, rng, buyer, issuer);
                None
            })
        } else {
            check(BUY_CORP_BONDS, &[&b, &i], &g, rng0, |g, ui, rng| {
                buy_corp_bonds(g, ui, rng, buyer, issuer);
                None
            })
        };
        if let Some(p) = p {
            trades.count((1..=15).any(|s| p.hold_bond_face[(s, buyer)] != g.hold_bond_face[(s, buyer)]));
        }
    });
    trades.check();
    require(&BORROWS, "BorrowToCoverPurchase");
}

#[test]
fn diff_insurer_gov_bond_trading() {
    let (buys, sells) = (Hits::new("gov bond purchases", 2), Hits::new("gov bond sales", 2));
    par_seeds(1..=40, |seed| {
        let mut g = fworld(seed + 300);
        let mut r = Rand(seed);
        let id = cast::<i32, _>(r.range(80, 130));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.cash[id] = f64r(&mut r, -Ext::from_f64(500.0), Ext::from_f64(2000.0));
            g.t_bills[id] = Ext::from_f64(0.0);
            g.base_interest_rate = cast::<f32, _>(cast::<f32, _>(r.range(2, 9)));
        } else {
            g.base_interest_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(6.0), Ext::from_f64(12.0)));
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x27d4_eb2f);
        let a = id.to_string();
        let p = check(INSURER_GOV, &[&a], &g, rng0, |g, ui, rng| {
            insurer_gov_bond_trading(g, ui, rng, id);
            None
        });
        if let Some(p) = p {
            let ch = p.long_gov_bond_face[id] + p.short_gov_bond_face[id] - g.long_gov_bond_face[id] - g.short_gov_bond_face[id];
            buys.count(ch > 0);
            sells.count(ch < 0);
        }
    });
    buys.check();
    sells.check();
}

#[test]
fn diff_trade_gov_bonds() {
    let __content = wsr_core::content::current();
    par_seeds(1..=24, |seed| {
        let g = fworld(seed + 400);
        let mut r = Rand(seed);
        let id = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { cast::<i32, _>(r.range(1581, 1583)) } else { cast::<i32, _>(r.range(11, 130)) };
        let face = r.range(100, 7000);
        let (ty, act) = (cast::<i32, _>(r.range(1, 2)), cast::<i32, _>(r.range(1, 3)));
        let rng0 = cast::<u32, _>(seed);
        let (a, f, t, c) = (id.to_string(), format!("{__wsr_text_13664}{face}",__wsr_text_13664=__content.string(13664)), ty.to_string(), act.to_string());
        check(TRADE_GOV, &[&a, &f, &t, &c], &g, rng0, |g, _ui, _rng| {
            trade_gov_bonds(g, id, face, ty, act);
            None
        });
    });
}

#[test]
fn diff_subprime_secs_write_down() {
    par_seeds(1..=40, |seed| {
        let mut g = fworld(seed + 500);
        let mut r = Rand(seed);
        let id = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { cast::<i32, _>(r.range(11, 85)) } else { cast::<i32, _>(r.range(86, 130)) };
        g.cur_quarter = (g.cur_year + id).rem_euclid(4) + 1;
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.cur_quarter = cast::<i32, _>(r.range(1, 4));
        }
        g.gdp_growth = cast::<f32, _>(f32r(&mut r, -Ext::from_f64(6.0), Ext::from_f64(4.0)));
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x1656_67b1);
        let a = id.to_string();
        check(SUBPRIME, &[&a], &g, rng0, |g, _ui, rng| Some(subprime_secs_write_down(g, rng, id)));
    });
}

#[test]
fn diff_bank_system_liquidity() {
    par_seeds(1..=24, |seed| {
        let mut g = fworld(seed + 600);
        let mut r = Rand(seed);
        let id = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { 0 } else { cast::<i32, _>(r.range(11, 85)) };
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.cur_quarter = 4;
            g.start_year = g.cur_year - 12;
            g.sim_co_count = 1500;
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            // many strained banks
            for b in 11..=85 {
                if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
                    g.cash[b] = f64r(&mut r, -Ext::from_f64(3000.0), -Ext::from_f64(100.0));
                    g.t_bills[b] = Ext::from_f64(0.0);
                    g.bank_debt[b] = f64r(&mut r, Ext::from_f64(900.0), Ext::from_f64(1900.0));
                }
            }
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0xd3a2_646c);
        let a = id.to_string();
        check(BANK_LIQ, &[&a], &g, rng0, |g, ui, rng| {
            bank_system_liquidity(g, ui, rng, id);
            None
        });
    });
    require(&LIQ_CHECKS, "BankLiquidityAndCapitalCheck");
}

#[test]
fn diff_bank_allocate_assets() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let moved = AtomicUsize::new(0);
    par_seeds(1..=40, |seed| {
        let mut g = fworld(seed + 700);
        let mut r = Rand(seed);
        let id = cast::<i32, _>(r.range(11, 85));
        let mode = [1, 2, 3, 3, 3][cast::<usize, _>(r.range(0, 4))];
        let force = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { 1 } else { 0 };
        g.bank_alloc_off[id] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { 1 } else { 0 };
        for (k, p) in [
            &mut g.bank_alloc_prime_mort_pct,
            &mut g.bank_alloc_subprime_pct,
            &mut g.bank_alloc_consumer_pct,
            &mut g.bank_alloc_bonds_pct,
            &mut g.bank_alloc_cash_pct,
        ]
        .into_iter()
        .enumerate()
        {
            p[id] = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { 0 } else { cast::<i16, _>(r.range(5, 40)) + cast::<i16, _>(k) };
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.start_year = g.cur_year - 3;
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            g.bad_debt_reserve[id] = -Ext::from_f64(1e6);
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x7feb_352d);
        let (a, m, f) = (id.to_string(), mode.to_string(), force.to_string());
        let Some(o) = run(0x69c127, &[&a, &m, &f], &g, rng0, &[], SKIPS) else { return };
        let mut p = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        let ret = bank_allocate_assets(&mut p, &mut ui, &mut rng, id, mode, force);
        cmp().assert(&p, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND state");
        assert_eq!(cast::<u32, _>(ret), o.return_value, "result");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "messages");
        if p.cash[id] != g.cash[id] {
            moved.fetch_add(1, Ordering::Relaxed);
        }
    });
    assert!(moved.load(Ordering::Relaxed) > 5, "too few reallocations");
}
