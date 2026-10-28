//! Differential tests of `logic::trading::stock`: BuyStock (0x4efe60), CalcTakeoverPremiumPrice
//! (0x528736), SellStock (0x5ab449) and PrivateBuyoutOffer (0x5bcd00).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use super::script::*;
use wsr_core::logic::trading::stock::*;
use wsr_core::state::Globals;

const BUY_STOCK: u32 = 0x4efe60;
const PREMIUM: u32 = 0x528736;
const SELL_STOCK: u32 = 0x5ab449;
const BUYOUT: u32 = 0x5bcd00;

fn yes_no(r: &mut Rand, n: usize, p_yes: Ext) -> Vec<String> {
    (0..n).map(|_| if r.chance(cast::<f64, _>(p_yes)) { "IDYES" } else { "IDNO" }.to_string()).collect()
}

#[test]
fn diff_calc_takeover_premium_price() {
    let __content = wsr_core::content::current();
    par_seeds(seeds(1..=40), |seed| {
        let mut g = world(seed + 9000);
        let mut r = Rand(seed * 131 + 7);
        let buyer = cast::<i32, _>(r.range(1, 1580));
        let target = cast::<i32, _>(r.range(11, 1580));
        let pct = [1, 3, 6, 10, 24, 25, 26, 40, 51, 100][cast::<usize, _>(r.range(0, 9))];
        let holder: i16 = match seed % 6 {
            0 => 0,
            1 => cast::<i16, _>(pct),
            // pct / holder * 5 just above an integer on the x87 (0.2 * 5)
            2 => [100, 50, 25][cast::<usize, _>(r.range(0, 2))],
            _ => cast::<i16, _>(r.range(1, 100)),
        };
        g.market_cap[target] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(5000.0));
        let port = calc_takeover_premium_price(&g, buyer, target, pct, holder);
        let args = [buyer.to_string(), target.to_string(), pct.to_string(), format!("{__wsr_text_13653}{holder}",__wsr_text_13653=__content.string(13653))];
        let opts = Opts { args: args.to_vec(), ..Opts::default() };
        let Some(o) = run_original_with(PREMIUM, &opts, &g) else {
            return;
        };
        let orig = o.return_float.expect("ST0 result");
        assert!(
            (port - orig).abs() <= Ext::from_f64(1e-12) * orig.abs().max(Ext::from(Ext::from_f64(1.0))),
            "seed {seed}: buyer {buyer}{__wsr_text_13552}{target} pct {pct}{__wsr_text_13774}{holder}: {port} vs {orig}"
        ,__wsr_text_13552=__content.string(13552), __wsr_text_13774=__content.string(13774));
    });
}

struct BuyCase {
    g: Globals,
    buyer: i32,
    target: i32,
    answers: Vec<String>,
}

/// A purchase of kind `seed % 9` in a random world.
fn buy_case(seed: u64) -> BuyCase {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 9100);
    let mut r = Rand(seed * 7919 + 3);
    let kind = seed % 9;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = st(&format!("Player Number {}", g.cur_player));
    let cp = g.cur_player;
    let mut target = pick_active(&mut r, &g, 131, 1580, &[]);
    let mut buyer;
    let mut mode = 0;
    let mut pct = r.range(1, 5);
    match kind {
        // the player buys a little
        0 => buyer = cp,
        // the player (or a company he controls) buys a big stake: tender offer, control, antitrust
        1 => {
            buyer = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { pick_active(&mut r, &g, 131, 1580, &[target]) };
            if buyer != cp {
                g.corp_controller[buyer] = cp;
            }
            pct = r.range(6, 60);
        }
        // a 100% purchase of a company of the same industry: liquidation into the parent
        2 => {
            buyer = pick_active(&mut r, &g, 131, 1580, &[target]);
            g.industry_num[target] = g.industry_num[buyer];
            if g.industry_num[buyer] < 3 || g.industry_num[buyer] > 69 {
                g.industry_num[buyer] = 20;
                g.industry_num[target] = 20;
            }
            g.corp_controller[buyer] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { cp } else { cast::<i32, _>(r.range(11, 1580)) };
            mode = [0, 0, 2, 2, 1][cast::<usize, _>(r.range(0, 4))];
            pct = 100;
            g.tax_loss_carry[target] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { Ext::from_f64(0.0) } else { -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0)) };
            g.rand_seed = cast::<i32, _>(r.range(0, 32000));
        }
        // the computer buys
        3 => {
            buyer = pick_active(&mut r, &g, 11, 1580, &[target]);
            mode = cast::<i32, _>(r.range(1, 3));
            pct = r.range(1, 40);
        }
        // a bank buys a bank
        4 => {
            buyer = pick_active(&mut r, &g, 11, 85, &[]);
            target = pick_active(&mut r, &g, 11, 85, &[buyer]);
            g.corp_controller[buyer] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { cast::<i32, _>(r.range(86, 1580)) };
            mode = [0, 0, 2][cast::<usize, _>(r.range(0, 2))];
            pct = r.range(20, 60);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.news_headline[60] = st(__content.string(14452));
            }
            g.country_idx[target] = [0, 11, 30, 3][cast::<usize, _>(r.range(0, 3))];
        }
        // fund buyers and fund targets
        5 => {
            buyer = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(1581, 1583)) } else { pick_active(&mut r, &g, 131, 1580, &[]) };
            g.entity_status[buyer] = 1;
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                target = cast::<i32, _>(r.range(1581, 1583));
                g.entity_status[target] = 1;
            }
            mode = cast::<i32, _>(r.range(0, 2));
            pct = r.range(1, 8);
        }
        // a merger step (mode 1)
        6 => {
            buyer = pick_active(&mut r, &g, 131, 1580, &[target]);
            mode = 1;
            pct = r.range(10, 70);
        }
        // the computer cannot pay / the player buys his own company
        7 => {
            buyer = pick_active(&mut r, &g, 131, 1580, &[target]);
            mode = cast::<i32, _>(r.range(0, 2));
            pct = r.range(5, 40);
            g.cash[buyer] = f64r(&mut r, -Ext::from_f64(20.0), Ext::from_f64(10.0));
            g.t_bills[buyer] = Ext::from_f64(0.0);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                g.corp_controller[target] = cp;
                buyer = cp;
                mode = 0;
            }
        }
        // odd arguments: buyer = target, more than the public float, a full portfolio
        _ => {
            buyer = pick_active(&mut r, &g, 11, 1580, &[target]);
            mode = cast::<i32, _>(r.range(0, 2));
            match r.range(0, 2) {
                0 => target = buyer,
                1 => pct = r.range(60, 100),
                _ => {
                    for s in 1..=15 {
                        if g.hold_stock_id[(s, buyer)] == 0 {
                            g.hold_stock_id[(s, buyer)] = cast::<i16, _>(pick_active(&mut r, &g, 131, 1580, &[target]));
                            g.hold_stock_pct[(s, buyer)] = cast::<i16, _>(r.range(1, 10));
                        }
                    }
                }
            }
        }
    }
    // free float: drop (most of) the other holders of the target
    if kind == 2 || r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
        clear_holders(&mut g, target);
        if kind != 2 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            // a few holders, sometimes a large one
            for _ in 0..r.range(1, 3) {
                let h = pick_active(&mut r, &g, 11, 1580, &[target, buyer]);
                let p = cast::<i16, _>(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { r.range(30, 55) } else { r.range(1, 15) });
                give_holding(&mut g, h, target, p, f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0)));
            }
        }
        if kind != 2 && r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            give_holding(&mut g, buyer, target, cast::<i16, _>(r.range(1, 30)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(300.0)));
        }
    }
    if kind != 7 && r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
        let need = g.market_cap[target] * cast::<Ext, _>(pct);
        g.cash[buyer] = need * f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(1.5));
    }
    g.tmp_ext1 = cast::<Ext, _>(pct);
    g.tmp_ext2 = cast::<Ext, _>(mode);
    if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
        g.news_msg = st("Pending headline from the caller");
    }
    let answers = yes_no(&mut r, 8, Ext::from_f64(0.8));
    BuyCase { g, buyer, target, answers }
}

#[test]
fn diff_buy_stock() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=270), |seed| {
        let c = buy_case(seed);
        let args = [format!("v:{}", c.buyer), format!("v:{}", c.target)]; // BYVAL LONGs
        if std::env::var("WSR_DEBUG").is_ok() {
            eprintln!(
                "seed {seed}: buyer {}{__wsr_text_13552}{} pct {}{__wsr_text_13548}{} ind {} {}",
                c.buyer, c.target, c.g.tmp_ext1, c.g.tmp_ext2, c.g.industry_num[c.buyer], c.g.industry_num[c.target]
            ,__wsr_text_13552=__content.string(13552), __wsr_text_13548=__content.string(13548));
        }
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, pg, _)) = check(seed, BUY_STOCK, &args, &c.g, &c.answers, &[], cmp, |g, ui, rng| {
            buy_stock(g, ui, rng, c.buyer, c.target, 0)
        }) else {
            return;
        };
        hits.record(&ui);
        if pg.cash[c.buyer] != c.g.cash[c.buyer] {
            hits.add("<bought>");
        }
        if pg.entity_status[c.target] == 2 && c.g.entity_status[c.target] == 1 {
            hits.add(__content.string(14341));
        }
    });
    hits.assert(&[
        "<bought>",
        __content.string(14341),
        __content.string(10971),
        __content.string(12816),
        __content.string(14342),
        __content.string(14343),
        __content.string(12817),
        __content.string(14110),
        __content.string(14344),
        __content.string(9864),
        __content.string(9702),
        __content.string(14345),
        __content.string(13571),
    ]);
}

struct SellCase {
    g: Globals,
    answers: Vec<String>,
}

/// The player's sale of kind `seed % 6`.
fn sell_case(seed: u64) -> SellCase {
    let mut g = world(seed + 9300);
    let mut r = Rand(seed * 104_729 + 11);
    let kind = seed % 6;
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = st(&format!("Player Number {}", g.cur_player));
    let cp = g.cur_player;
    // the acting entity: the player or a company he controls
    let ae = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cp } else { pick_active(&mut r, &g, 131, 1580, &[]) };
    g.active_entity = ae;
    g.active_entity_name = g.ent_name[ae].clone();
    g.corp_controller[ae] = if ae == cp { 0 } else { cp };
    g.sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { ae } else { pick_active(&mut r, &g, 131, 1580, &[]) };
    g.valuation_mode = 1;
    let mut answers = Vec::new();
    // the stock to sell
    let corp = pick_active(&mut r, &g, 131, 1580, &[ae]);
    match kind {
        // unlisted stock (no public float): private buyer offer
        0 | 1 => {
            clear_holders(&mut g, corp);
            let pct = [100, 100, 80, 60, 25][cast::<usize, _>(r.range(0, 4))];
            give_holding(&mut g, ae, corp, pct, f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(900.0)));
            g.difficulty_level = if r.chance(cast::<f64, _>(Ext::from_f64(0.85))) { cast::<i32, _>(r.range(2, 4)) } else { 1 };
            if kind == 1 {
                g.earnings_yr3_ago[corp] = Ext::from_f64(0.0);
                g.earnings_yr4_ago[corp] = Ext::from_f64(0.0);
                if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                    g.earnings_y1[corp] = Ext::from_f64(0.0);
                    g.earnings_yr2_ago[corp] = Ext::from_f64(0.0);
                }
            }
            // rich buyers (the offer for a control block is about 40x the company's value)
            for _ in 0..r.range(0, 6) {
                let b = pick_active(&mut r, &g, 131, 1580, &[corp, ae]);
                g.corp_controller[b] = 0;
                g.cash[b] = g.market_cap[corp] * f64r(&mut r, Ext::from_f64(30.0), Ext::from_f64(4000.0));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.ai_target_corp = pick_active(&mut r, &g, 131, 1580, &[corp, ae]);
                g.cash[g.ai_target_corp] = g.market_cap[corp] * Ext::from_f64(100.0);
            }
            answers.push("pick:0".to_string());
            answers.extend(yes_no(&mut r, 1, if kind == 1 { Ext::from_f64(0.3) } else { Ext::from_f64(0.7) }));
            // (a plain number: after a declined offer the "Sale Canceled" box takes it)
            answers.push(format!("{}", r.range(1, 30)));
            answers.extend(yes_no(&mut r, 2, Ext::from_f64(0.8)));
        }
        // a listed stock: percent entry, tax warning, private sale of an unseasoned company
        2 | 3 => {
            let corp = if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
                // a fund (ETF) share
                let f = cast::<i32, _>(r.range(1581, 1583));
                g.entity_status[f] = 1;
                f
            } else {
                corp
            };
            clear_holders(&mut g, corp);
            // kind 3, a company seller: an 80%+ stake with an excess loss account (negative basis)
            let excess_loss = kind == 3 && ae > 10 && r.chance(cast::<f64, _>(Ext::from_f64(0.7)));
            let pct = cast::<i16, _>(if excess_loss { r.range(80, 95) } else { r.range(5, 85) });
            let basis = if excess_loss || r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { -f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(300.0)) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(200.0)) };
            give_holding(&mut g, ae, corp, pct, basis);
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.earnings_yr3_ago[corp] = Ext::from_f64(0.0);
                g.earnings_yr4_ago[corp] = Ext::from_f64(0.0);
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
                g.player_cap_gains[ae] = -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0));
            }
            answers.push("pick:0".to_string());
            let entered = match if excess_loss { 4 } else { r.range(0, 4) } {
                4 => format!("{}", r.range(cast::<i64, _>(pct) - 79, cast::<i64, _>(pct) - 1)),
                0 => format!("{}", cast::<i64, _>(pct) + 5),
                1 => "0".to_string(),
                2 => format!("${pct}"),
                _ => format!("{}%", r.range(1, cast::<i64, _>(pct))),
            };
            answers.push(entered);
            answers.push(format!("{}", r.range(1, cast::<i64, _>(pct))));
            answers.extend(yes_no(&mut r, 3, Ext::from_f64(0.8)));
        }
        // nothing to sell / pick the own entity / no pick
        4 => {
            match r.range(0, 3) {
                3 => {
                    // a listed slot with 0% (counts as not owned)
                    for s in 0..=15 {
                        g.hold_stock_id[(s, ae)] = 0;
                        g.hold_stock_pct[(s, ae)] = 0;
                    }
                    give_holding(&mut g, ae, corp, 0, Ext::from_f64(5.0));
                    let other = pick_active(&mut r, &g, 131, 1580, &[ae, corp]);
                    give_holding(&mut g, ae, other, 5, Ext::from_f64(5.0));
                    answers.push(format!("pick:{}", r.range(0, 1)));
                }
                0 => {
                    for s in 0..=15 {
                        g.hold_stock_id[(s, ae)] = 0;
                        g.hold_stock_pct[(s, ae)] = 0;
                    }
                }
                1 => {
                    if ae > 10 {
                        give_holding(&mut g, ae, ae, 10, Ext::from_f64(5.0));
                    }
                    answers.push("pick:0".to_string());
                }
                _ => answers.push("IDOK".to_string()),
            }
        }
        // an ETF advised by the player sells
        _ => {
            let etf = cast::<i32, _>(r.range(1581, 1583));
            g.entity_status[etf] = 1;
            let adviser = pick_active(&mut r, &g, 131, 1580, &[]);
            g.fund_adviser[etf] = adviser;
            // an investment adviser (industry 37 or an insurer) of the human current player
            g.industry_num[adviser] = 37;
            g.corp_controller[adviser] = cp;
            g.is_computer_player[cp] = 0;
            g.sel_entity = etf;
            g.fund_bonus_year[etf] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { g.cur_year - 2 } else { 0 };
            let fc = pick_active(&mut r, &g, 131, 1580, &[etf]);
            give_holding(&mut g, etf, fc, cast::<i16, _>(r.range(1, 9)), f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)));
            answers.push(["IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 2))].to_string());
            answers.push("pick:0".to_string());
            answers.push(format!("{}", r.range(1, 5)));
            answers.extend(yes_no(&mut r, 2, Ext::from_f64(0.8)));
        }
    }
    SellCase { g, answers }
}

#[test]
fn diff_sell_stock() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=240), |seed| {
        let c = sell_case(seed);
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        // the harness clock is 10:00:00: TIME$ seconds = 0
        let Some((ui, pg, _)) = check(seed, SELL_STOCK, &[], &c.g, &c.answers, &[], cmp, |g, ui, rng| {
            sell_stock(g, ui, rng, 0)
        }) else {
            return;
        };
        hits.record(&ui);
        let ae = c.g.active_entity;
        if (1..=15).any(|s| pg.hold_stock_pct[(s, ae)] != c.g.hold_stock_pct[(s, ae)]) {
            hits.add("<sold>");
        }
    });
    hits.assert(&[
        "<sold>",
        __content.string(14346),
        __content.string(14347),
        __content.string(14348),
        __content.string(14349),
        __content.string(12881),
        __content.string(14350),
        __content.string(14351),
        __content.string(14352),
        __content.string(14353),
        __content.string(12851),
        __content.string(14354),
        __content.string(14355),
        __content.string(14336),
        __content.string(12869),
        __content.string(14356),
        __content.string(14357),
    ]);
}

#[test]
fn diff_private_buyout_offer() {
    let __content = wsr_core::content::current();
    let hits = Hits::default();
    par_seeds(seeds(1..=80), |seed| {
        let mut g = world(seed + 9500);
        let mut r = Rand(seed * 613 + 29);
        let seller = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players))) } else { pick_active(&mut r, &g, 11, 1580, &[]) };
        let corp = pick_active(&mut r, &g, 131, 1580, &[seller]);
        clear_holders(&mut g, corp);
        if seed % 8 != 0 {
            let pct = [100, 90, 51, 40, 25, 10][cast::<usize, _>(r.range(0, 5))];
            let basis = if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) { -f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(100.0)) } else { f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(600.0)) };
            give_holding(&mut g, seller, corp, pct, basis);
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
            g.earnings_yr3_ago[corp] = Ext::from_f64(0.0);
            g.earnings_yr4_ago[corp] = Ext::from_f64(0.0);
        }
        for _ in 0..r.range(0, 8) {
            let b = pick_active(&mut r, &g, 131, 1580, &[corp, seller]);
            g.cash[b] = g.market_cap[corp] * f64r(&mut r, Ext::from_f64(20.0), Ext::from_f64(5000.0));
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.ai_target_corp = pick_active(&mut r, &g, 131, 1580, &[corp, seller]);
        }
        let answers = yes_no(&mut r, 3, Ext::from_f64(0.7));
        let args = [seller.to_string(), corp.to_string()];
        let cmp = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-7));
        let Some((ui, _, o)) = check(seed, BUYOUT, &args, &g, &answers, &[], cmp, |g, ui, rng| {
            private_buyout_offer(g, ui, rng, seller, corp);
        }) else {
            return;
        };
        // the result (EAX): recompute it with the port on the same inputs
        let mut pg = g.clone();
        let mut rng = wsr_core::pb::rng::Rng::from_state(rng_of(seed));
        let mut ui2 = Script { q: answers.iter().cloned().collect(), ..Script::default() };
        let res = private_buyout_offer(&mut pg, &mut ui2, &mut rng, seller, corp);
        assert_eq!(cast::<u32, _>(res), o.return_value, "seed {seed}: result");
        hits.record(&ui);
        if res == 1 {
            hits.add("<sold>");
        }
    });
    hits.assert(&["<sold>", __content.string(12881), __content.string(14358), __content.string(14359), __content.string(14360)]);
}
