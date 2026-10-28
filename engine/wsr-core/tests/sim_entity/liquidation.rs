//! Differential tests of `logic::sim_entity::liquidation`: ClearAllHoldings, ForceCoverShorts,
//! LiquidateCorporation (automatic and interactive) and BankruptcyLiquidation against the original.
#![allow(clippy::manual_is_multiple_of)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use wsr_core::logic::sim_entity::liquidation::*;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const CLEAR_ALL_HOLDINGS: u32 = 0x49261c;
const LIQUIDATE_CORPORATION: u32 = 0x51b258;
const BANKRUPTCY_LIQUIDATION: u32 = 0x4e7250;
/// No callee is skipped (BondBuybackMenu and SellBusinessAssets run in the interactive liquidation).
const SKIPS: &[(u32, u32)] = &[];

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x1b87_3593
}

/// Compares state, RND state and the number of message boxes of a port run with the original's.
#[track_caller]
fn check(port: &Globals, rng: &Rng, ui: &Rec, o: &Original, what: &str) {
    let __content = wsr_core::content::current();
    if std::env::var("DBG").is_ok() {
        for l in o.log.iter().filter(|l| !l.starts_with("API")) {
            eprintln!("{what}: {l}");
        }
        eprintln!("{what}: port msgs {:?}{__wsr_text_13611}{:?}", ui.msgs, ui.news,__wsr_text_13611=__content.string(13611));
    }
    cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "{what}: RND state");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(o),
        "{what}: message boxes {:?} vs {:?}",
        ui.msgs,
        o.log
    );
}

#[test]
fn diff_clear_all_holdings() {
    par_seeds(1..=2, |seed| {
        let g = world(seed + 900);
        let mode = if seed == 1 { 2 } else { 0 };
        let mut port = g.clone();
        clear_all_holdings(&mut port, mode);
        let Some(o) = run(CLEAR_ALL_HOLDINGS, &[&mode.to_string()], &g, 1, &[], &[]) else {
            return;
        };
        cmp().assert(&port, &o.globals);
    });
}

/// A corp of the world prepared as a candidate for a taxable liquidation.
fn liq_world(seed: u64, interactive: bool) -> (Globals, i32) {
    let mut g = world(seed + 1100);
    let mut r = Rand(seed * 31 + 3);
    let c = loop {
        let c = cast::<i32, _>(r.range(131, 1580));
        if g.industry_num[c] < 69 {
            break c;
        }
    };
    g.entity_status[c] = 1;
    g.business_assets[c] = if interactive && seed % 4 == 1 {
        Ext::from_f64(0.3)
    } else if interactive && seed % 4 == 3 {
        f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(400.0))
    } else {
        Ext::from_f64(0.0)
    };
    // bonds outstanding (interactive: the buy-back menu asks how many bonds to buy back)
    g.bonds_issued[c] = if interactive && (seed == 6 || seed % 5 == 4) { r.range(100, 900) } else { 0 };
    if g.bonds_issued[c] > 0 {
        g.bond_issue_info[c] = st(&format!("{}@{:.2}", g.cur_year + 8, f64r(&mut r, Ext::from(Ext::from_f64(5.0)), Ext::from(Ext::from_f64(9.0)))));
        g.bond_price[c] = wsr_core::pb::numeric::Cur::from_f64(f64r(&mut r, Ext::from_f64(80.0), Ext::from_f64(110.0)));
    }
    if interactive {
        g.active_entity = c;
        g.active_entity_name = g.ent_name[c].clone();
    }
    g.working_capital[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0));
    g.cash[c] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(2000.0));
    g.bank_debt[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0))
    } else {
        Ext::from_f64(0.0)
    };
    for p in 1..=5 {
        g.player_advances[(p, c)] = Ext::from_f64(0.0);
    }
    if interactive && seed % 3 == 0 {
        g.player_advances[(1, c)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
    }
    // its own holdings: none for the automatic mode (no subsidiaries), a few small stakes otherwise
    for slot in 0..=15 {
        g.hold_stock_id[(slot, c)] = 0;
        g.hold_stock_pct[(slot, c)] = 0;
        g.hold_stock_basis[(slot, c)] = Ext::from_f64(0.0);
    }
    if interactive {
        let mut n = 0;
        for slot in 1..=r.range(0, 3) {
            let h = cast::<i32, _>(r.range(131, 1580));
            if g.entity_status[h] == 1 && h != c {
                g.hold_stock_id[(cast::<i32, _>(slot), c)] = cast::<i16, _>(h);
                g.hold_stock_pct[(cast::<i32, _>(slot), c)] = cast::<i16, _>(r.range(1, 15));
                g.hold_stock_basis[(cast::<i32, _>(slot), c)] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(300.0));
                n += 1;
            }
        }
        g.hold_stock_id[(0, c)] = n;
    }
    // holders of its stock: some players and corps
    for h in [
        1,
        2,
        cast::<i32, _>(r.range(11, 130)),
        cast::<i32, _>(r.range(131, 1580)),
        cast::<i32, _>(r.range(131, 1580)),
    ] {
        if h != c && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
            let slot = cast::<i32, _>(r.range(1, 15));
            g.hold_stock_id[(slot, h)] = cast::<i16, _>(c);
            g.hold_stock_pct[(slot, h)] = [5, 20, 30, 100][cast::<usize, _>(r.range(0, 3))];
            g.hold_stock_basis[(slot, h)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0));
        }
    }
    // a pending merger on it, the selected entity
    if seed % 5 == 2 {
        g.merger_target = c;
        g.merger_acquirer = cast::<i32, _>(r.range(131, 1580));
    }
    if seed % 4 == 3 {
        g.last_sel_entity = c;
    }
    g.corp_controller[c] = if seed % 3 == 0 {
        0
    } else {
        cast::<i32, _>(r.range(11, 1580))
    };
    g.tax_loss_carry[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
        -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0))
    } else {
        Ext::from_f64(0.0)
    };
    g.difficulty_level = cast::<i32, _>(seed % 3 + 1);
    (g, c)
}

#[test]
fn diff_liquidate_corporation_auto() {
    let __content = wsr_core::content::current();
    par_seeds(1..=8, |seed| {
        let (mut g, c) = liq_world(seed, false);
        let mode = if seed % 2 == 0 { 2 } else { 1 };
        if seed % 4 == 1 {
            g.ai_target_corp = c;
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = Rec::default();
        liquidate_corporation(&mut port, &mut ui, &mut rng, c, mode, &wsr_core::logic::newgame::LocalTime::HARNESS);
        let Some(o) = run(
            LIQUIDATE_CORPORATION,
            &[&c.to_string(), &mode.to_string()],
            &g,
            rs,
            &[],
            SKIPS,
        ) else {
            return;
        };
        check(
            &port,
            &rng,
            &ui,
            &o,
            &format!("seed {seed} corp {c}{__wsr_text_13548}{mode}",__wsr_text_13548=__content.string(13548)),
        );
        if std::env::var("COV").is_ok() {
            eprintln!("seed {seed}{__wsr_text_14163}{}", port.entity_status[c] != 1,__wsr_text_14163=__content.string(14163));
        }
    });
}

#[test]
fn diff_liquidate_corporation_interactive() {
    let __content = wsr_core::content::current();
    use super::script::{Hits, check as script_check, seeds};
    let hits = Hits::default();
    par_seeds(seeds(1..=24), |seed| {
        let (mut g, c) = liq_world(seed, true);
        if seed % 4 == 2 {
            g.asbestos_corp = if seed % 8 == 2 { c } else { -c };
            g.total_assets = f64r(&mut Rand(seed), Ext::from_f64(100.0), Ext::from_f64(4000.0));
        }
        // answers: mostly YES (and amounts for the bond buy-back / asset sale dialogs); seed 7 declines
        // the second question, seed 8 the first
        let mut r = Rand(seed * 7 + 1);
        let answers: Vec<String> = match seed {
            7 => vec!["IDYES".into(), "IDNO".into()],
            8 => vec!["IDNO".into()],
            _ if g.bonds_issued[c] > 0 && seed % 3 != 0 => {
                // the liquidation (and the set-aside) confirmed, then the bond buy-back: call (YES) or buy
                // back (NO) 100% / all, confirmed
                let mut a: Vec<String> = vec!["IDYES".into()];
                if seed % 4 == 2 {
                    a.push("IDYES".into());
                }
                a.push(if seed % 2 == 0 { "IDYES" } else { "IDNO" }.into());
                a.push(if seed % 2 == 0 { "100".to_string() } else { g.bonds_issued[c].to_string() });
                a.extend(["IDYES", "IDYES", "IDYES"].map(String::from));
                a
            }
            _ => {
                let mut a: Vec<String> = vec!["IDYES".into(), "IDYES".into()];
                for _ in 0..8 {
                    a.push(match r.range(0, 5) {
                        0 => "IDNO".into(),
                        1 => "pick:0".into(),
                        2 => g.bonds_issued[c].to_string(),
                        3 => "100".into(),
                        _ => "IDYES".into(),
                    });
                }
                a
            }
        };
        let args = vec![c.to_string(), "0".to_string()];
        let compare = cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9));
        let Some((ui, port, _)) = script_check(seed, LIQUIDATE_CORPORATION, &args, &g, &answers, SKIPS, compare, |p, ui, rng| {
            liquidate_corporation(p, ui, rng, c, 0, &wsr_core::logic::newgame::LocalTime::HARNESS);
        }) else {
            return;
        };
        hits.record(&ui);
        if g.bonds_issued[c] > 0 && port.bonds_issued[c] < g.bonds_issued[c] {
            hits.add("bonds bought back");
        }
        if g.business_assets[c] >= Ext::from_f64(1.0) && port.business_assets[c] < g.business_assets[c] {
            hits.add("assets sold");
        }
        if std::env::var("COV").is_ok() {
            eprintln!("seed {seed}{__wsr_text_14163}{}", port.entity_status[c] != 1,__wsr_text_14163=__content.string(14163));
        }
    });
    hits.assert(&["bonds bought back", __content.string(14164), __content.string(14165)]);
}

/// A world with a bankrupt entity of the kind `kind` (0 = operating corp, 1 = bank, 2 = insurer):
/// stockholders (incl. player shorts), bondholders, player advances, a lender.
fn bk_world(seed: u64, kind: u64) -> (Globals, i32) {
    let __content = wsr_core::content::current();
    let mut g = world(seed + 1300);
    let mut r = Rand(seed * 17 + kind * 101 + 9);
    let c = cast::<i32, _>(match kind {
        1 => r.range(12, 85),
        2 => r.range(86, 130),
        _ => loop {
            let c = r.range(131, 1580);
            if g.industry_num[cast::<i32, _>(c)] < 69 {
                break c;
            }
        },
    });
    g.entity_status[c] = 1;
    g.lender_id = cast::<i32, _>(g.lending_bank[c]);
    let debt_free = kind == 2 && (seed / 2) % 2 == 0;
    g.bank_debt[c] = if debt_free {
        Ext::from_f64(0.0)
    } else {
        f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0))
    };
    if kind == 0 && seed % 3 != 0 {
        // deeply distressed: low credit rating through the reorganization rounds
        g.bank_debt[c] = f64r(&mut r, Ext::from_f64(5000.0), Ext::from_f64(60000.0));
        g.business_assets[c] = if seed % 3 == 2 {
            g.bank_debt[c] * f64r(&mut r, Ext::from_f64(1.15), Ext::from_f64(1.6))
        } else {
            f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2000.0))
        };
        g.working_capital[c] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(500.0));
        g.t_bills[c] = Ext::from_f64(0.0);
        for slot in 0..=15 {
            g.hold_stock_id[(slot, c)] = 0;
            g.hold_stock_pct[(slot, c)] = 0;
        }
    }
    g.cash[c] = f64r(&mut r, -Ext::from_f64(500.0), Ext::from_f64(300.0));
    g.goodwill[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(100.0))
    } else {
        Ext::from_f64(0.0)
    };
    g.bonds_issued[c] = if debt_free { 0 } else { r.range(0, 2000) };
    for p in 1..=5 {
        g.player_advances[(p, c)] = Ext::from_f64(0.0);
    }
    if !debt_free && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
        g.player_advances[(1, c)] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(80.0));
        g.player_advances[(2, c)] = f64r(&mut r, -Ext::from_f64(5.0), Ext::from_f64(80.0));
    }
    // stockholders: players (one short), banks, corps
    for (i, h) in [
        1,
        2,
        cast::<i32, _>(r.range(11, 85)),
        cast::<i32, _>(r.range(86, 1580)),
        g.lender_id,
    ]
    .into_iter()
    .enumerate()
    {
        if h == c {
            continue;
        }
        let slot = cast::<i32, _>(r.range(1, 15));
        g.hold_stock_id[(slot, h)] = cast::<i16, _>(c);
        g.hold_stock_pct[(slot, h)] = match i {
            1 => -(cast::<i16, _>(r.range(1, 6))),
            _ => cast::<i16, _>(r.range(1, 60)),
        };
        g.hold_stock_basis[(slot, h)] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(400.0));
        g.entity_status[h] = 1;
    }
    // bondholders
    if g.bonds_issued[c] > 0 {
        g.bond_issue_info[c] = st(&format!("{} 9.50", g.cur_year + 5));
        for h in [
            1,
            cast::<i32, _>(r.range(11, 85)),
            cast::<i32, _>(r.range(86, 130)),
            cast::<i32, _>(r.range(1581, 1583)),
        ] {
            let slot = cast::<i32, _>(r.range(1, 15));
            g.hold_bond_id[(slot, h)] = c;
            g.hold_bond_face[(slot, h)] = r.range(1, g.bonds_issued[c]);
            g.bond_hold_cost[(slot, h)] =
                cast::<Ext, _>(g.hold_bond_face[(slot, h)]) * f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.0));
        }
    }
    // depositors of a failing bank
    if kind == 1 {
        for e in 1..=1600 {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.05))) {
                g.lending_bank[e] = cast::<i16, _>(c);
            }
        }
        g.lending_bank[1] = cast::<i16, _>(c);
        g.bank_c_ds[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        g.bank_demand_deposits[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        g.bank_cust_deposits[c] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30000.0));
        g.country_idx[c] = cast::<i32, _>(seed % 2);
        if seed % 4 != 3 {
            g.bank_demand_deposits[c] = f64r(&mut r, Ext::from_f64(1.0e5), Ext::from_f64(6.0e5));
        }
    }
    if seed % 3 == 0 {
        g.asbestos_corp = if seed % 2 == 0 { c } else { -c };
    }
    if seed % 4 == 1 {
        g.superfund_corp = cast::<i16, _>(c);
    }
    if seed % 5 == 2 {
        g.merger_target = c;
        g.merger_acquirer = cast::<i32, _>(r.range(131, 1580));
    }
    if seed % 5 == 3 {
        g.merger_tip_corp = cast::<i16, _>(c);
    }
    g.contingency_reserve[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(30.0))
    } else {
        Ext::from_f64(0.0)
    };
    g.tax_loss_carry[c] = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(300.0))
    } else {
        Ext::from_f64(0.0)
    };
    g.news_msg = if seed % 2 == 0 {
        st(__content.string(13571))
    } else {
        st("")
    };
    g.new_mgmt_list = if seed % 3 == 1 {
        st("0131/0200/")
    } else {
        st("")
    };
    (g, c)
}

fn run_bankruptcy(seed: u64, kind: u64, mode: i32) {
    let __content = wsr_core::content::current();
    let (mut g, c) = bk_world(seed, kind);
    g.tmp_ext1 = cast::<Ext, _>(mode);
    g.tmp_ext2 = cast::<Ext, _>(seed % 2);
    if std::env::var("COV").is_ok() {
        let mut h = g.clone();
        wsr_core::logic::finance::calc_entity_financials(
            &mut h,
            &mut Rec::default(),
            &mut Rng::from_state(1),
            c,
        );
        eprintln!(
            "pre: nw {}{__wsr_text_13848}{} cnw {}{__wsr_text_13850}{} ind {}",
            h.net_worth,
            h.tmp_total_debt,
            h.tmp_credit_net_worth,
            h.credit_rating_num[c],
            h.industry_num[c]
        ,__wsr_text_13848=__content.string(13848), __wsr_text_13850=__content.string(13850));
    }
    let rs = rng0(seed + kind * 1000);
    let mut port = g.clone();
    let mut rng = Rng::from_state(rs);
    let mut ui = Rec::default();
    bankruptcy_liquidation(&mut port, &mut ui, &mut rng, c, 0);
    let Some(o) = run(
        BANKRUPTCY_LIQUIDATION,
        &[&c.to_string()],
        &g,
        rs,
        &[],
        SKIPS,
    ) else {
        return;
    };
    check(
        &port,
        &rng,
        &ui,
        &o,
        &format!("seed {seed}{__wsr_text_13623}{kind}{__wsr_text_13548}{mode} corp {c}",__wsr_text_13623=__content.string(13623), __wsr_text_13548=__content.string(13548)),
    );
    if std::env::var("COV").is_ok() {
        eprintln!(
            "seed {seed}{__wsr_text_13623}{kind}{__wsr_text_13548}{mode}: status {}{__wsr_text_13848}{:.1} -> {:.1}{__wsr_text_13850}{}{__wsr_text_13611}{}",
            port.entity_status[c],
            g.bank_debt[c],
            port.bank_debt[c],
            port.credit_rating_num[c],
            ui.news.len()
        ,__wsr_text_13623=__content.string(13623), __wsr_text_13548=__content.string(13548), __wsr_text_13848=__content.string(13848), __wsr_text_13850=__content.string(13850), __wsr_text_13611=__content.string(13611));
    }
}

#[test]
fn diff_bankruptcy_liquidation_complete() {
    par_seeds(1..=6, |seed| run_bankruptcy(seed, seed % 3, 7));
}

#[test]
fn diff_bankruptcy_reorganization() {
    par_seeds(1..=10, |seed| run_bankruptcy(seed, 0, 11));
}

#[test]
fn diff_bankruptcy_bank_and_insurer() {
    par_seeds(1..=8, |seed| run_bankruptcy(seed, 1 + seed % 2, 11));
}
