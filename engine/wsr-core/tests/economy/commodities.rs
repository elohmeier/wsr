//! Tests of logic::economy::commodities: UpdateCryptoPrices (0x6b68ef), CryptoBanRumorEvent (0x63ac02),
//! ShockCommodityPrice (0x635b16), RebaseCommodityTargetsFromHistory (0x63556a).
use wsr_core::pb::real::{Ext, cast};


use super::*;
use wsr_core::logic::economy::{
    crypto_ban_rumor_event, rebase_commodity_targets_from_history, shock_commodity_price, update_crypto_prices,
};

/// Peer routines owned by other submodules, skipped in the original (none here).
const PEER_SKIPS: &[(u32, u32)] = &[];

/// `world` plus the crypto state (sometimes uninitialised) and commodity history.
fn crypto_world(seed: u64) -> Globals {
    let mut g = world(seed);
    let mut r = Rand(seed * 1009 + 17);
    if r.chance(Ext::from_f64(0.2)) {
        g.bitcoin_target_price = 0;
    } else {
        g.bitcoin_target_price = cast::<i32, _>(r.range(5000, 150_000));
        g.ethereum_target_price = cast::<i32, _>(r.range(300, 8000));
    }
    g.crypto_boom_years = if r.chance(Ext::from_f64(0.3)) { 0 } else { cast::<i32, _>(r.range(5, 20)) };
    g.btc_trend_target = if r.chance(Ext::from_f64(0.2)) { 0 } else { cast::<i32, _>(r.range(-20, 300)) };
    g.eth_trend_target = if r.chance(Ext::from_f64(0.2)) { 0 } else { cast::<i32, _>(r.range(-20, 300)) };
    g.btc_trend_step_pct = [0, 6, 12, 19, 20][cast::<usize, _>(r.range(0, 4))];
    g.eth_trend_step_pct = [0, 6, 12, 19, 20][cast::<usize, _>(r.range(0, 4))];
    g.crypto_ban_state = [0, 0, 1, -1, 2][cast::<usize, _>(r.range(0, 4))];
    g.market_val_target = cast::<f64, _>(if r.chance(Ext::from_f64(0.1)) { Ext::from_f64(0.0) } else { f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(1.5)) });
    if r.chance(Ext::from_f64(0.1)) {
        g.gold_target_price = cast::<f32, _>(Ext::from_f64(0.5));
    }
    g.futures_basis[7] = cast::<i16, _>(r.range(150, 300));
    g.futures_basis[8] = cast::<i16, _>(r.range(150, 300));
    g.metals_price_base = cast::<i32, _>(r.range(0, 2000));
    g.peak_oil_crisis = cast::<i16, _>(r.range(-1, 2));
    for col in 6..=10 {
        let reff = match col {
            6 => Ext::from_f64(100.0),
            7 => cast::<Ext, _>(g.metals_price_base) + Ext::from_f64(1400.0),
            8 => cast::<Ext, _>(g.metals_price_base) * Ext::from_f64(0.01) + Ext::from_f64(20.0),
            _ => Ext::from_f64(6.25),
        };
        let lvl = [Ext::from_f64(0.6), Ext::from_f64(1.0), Ext::from_f64(1.5)][cast::<usize, _>(r.range(0, 2))];
        for p in 1..=60 {
            g.cap_hist_close[(p, col)] = if r.chance(Ext::from_f64(0.05)) { Ext::from_f64(0.0) } else { reff * lvl * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.2)) };
        }
    }
    if r.chance(Ext::from_f64(0.15)) {
        g.cap_hist_close[(60, 7)] = Ext::from_f64(0.0);
    }
    g
}

#[test]
fn shock_commodity_price_unit() {
    let mut g = Globals::default();
    g.oil_price = cast::<f32, _>(Ext::from_f64(0.1));
    g.oil_target_price = cast::<f32, _>(Ext::from_f64(1.0));
    let mut rng = Rng::from_state(1);
    shock_commodity_price(&mut g, &mut rng, b"OIL", 20);
    assert_eq!(g.oil_price, cast::<f32, _>(cast::<Ext, _>(0.3f32) * (cast::<Ext, _>(0.01f32) * Ext::from_f64(20.0) + Ext::from_f64(1.0))));
    assert_eq!(g.oil_target_price, cast::<f32, _>(cast::<Ext, _>(1.1f32) * (cast::<Ext, _>(0.01f32) * Ext::from_f64(20.0) + Ext::from_f64(1.0))));
    let before = g.clone();
    shock_commodity_price(&mut g, &mut rng, b"PLATINUM", 20);
    assert_eq!(g.oil_price, before.oil_price);
}

#[test]
fn rebase_targets_skip_quarter() {
    let mut g = crypto_world(3);
    g.cur_year = 2000;
    g.cur_quarter = 1; // 2001 MOD 4 = 1
    let before = g.clone();
    let mut rng = Rng::from_state(5);
    rebase_commodity_targets_from_history(&mut g, &mut rng);
    assert_eq!(rng.state(), 5);
    assert_eq!(g.gold_target_price, before.gold_target_price);
}

#[test]
fn diff_update_crypto_prices() {
    diff::par_seeds(1..=40, |seed| {
        let mut g = crypto_world(seed);
        if seed % 5 == 0 {
            g.rand_seed = 16; // MOD 15 = 1
        }
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x9e37_79b9);
        check(0x6b68ef, &[], &g, rng0, &|g, _ui, rng| update_crypto_prices(g, rng));
    });
}

#[test]
fn diff_shock_commodity_price() {
    diff::par_seeds(1..=30, |seed| {
        let g = crypto_world(seed);
        let name = ["OIL", "GOLD", "SILVER", "WHEAT", "CORN", "BITCOIN"][cast::<usize, _>(seed % 6)];
        let pct = [-60, -20, 0, 15, 45, 200][cast::<usize, _>((seed / 6) % 6)];
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x2545_f491);
        let opts = Opts {
            args: vec![format!("s:{name}"), format!("{pct}")],
            rng: Some(rng0),
            skips: PEER_SKIPS.to_vec(),
            ..Opts::default()
        };
        let Some(o) = run_original_with(0x635b16, &opts, &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        shock_commodity_price(&mut port, &mut rng, name.as_bytes(), pct);
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND sequence");
    });
}

#[test]
fn diff_rebase_commodity_targets_from_history() {
    diff::par_seeds(1..=40, |seed| {
        let g = crypto_world(seed);
        let rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x0808_8405);
        check(0x63556a, &[], &g, rng0, &|g, _ui, rng| rebase_commodity_targets_from_history(g, rng));
    });
}

#[test]
fn diff_crypto_ban_rumor_event() {
    diff::par_seeds(1..=32, |seed| {
        let mut g = crypto_world(seed);
        pow_safe(&mut g, seed);
        let mut r = Rand(seed * 4241 + 5);
        g.btc_ban_rumor_count = cast::<i32, _>(r.range(0, 2));
        g.btc_ban_rumor_country = s(["", "China", "U.S."][cast::<usize, _>(r.range(0, 2))]);
        g.suppress_popups = if r.chance(Ext::from_f64(0.2)) { 1 } else { 0 };
        g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
        for p in 1..=5 {
            g.is_computer_player[p] = if r.chance(Ext::from_f64(0.3)) { 1 } else { 0 };
            if r.chance(Ext::from_f64(0.6)) {
                g.cash[p] = f64r(&mut r, Ext::from_f64(6500.0), Ext::from_f64(20000.0));
            }
        }
        // crypto futures / physical records of players and their companies
        let nf = cast::<i32, _>(r.range(0, 8));
        g.futures_recs[0] = s(&nf.to_string());
        for k in 1..=nf {
            let owner = if r.chance(Ext::from_f64(0.6)) { r.range(1, 5) } else { r.range(131, 400) };
            let c = ["BITCOIN", "ETHEREUM", "GOLD"][cast::<usize, _>(r.range(0, 2))];
            g.futures_recs[k] = s(&format!("{c}|{owner}|{}|6|2030|1|2", r.range(1, 200)));
        }
        let np = cast::<i32, _>(r.range(0, 6));
        g.phys_commod_rec[0] = s(&np.to_string());
        for k in 1..=np {
            let owner = if r.chance(Ext::from_f64(0.6)) { r.range(1, 5) } else { r.range(131, 400) };
            let c = ["BITCOIN", "ETHEREUM", "OIL"][cast::<usize, _>(r.range(0, 2))];
            g.phys_commod_rec[k] = s(&format!("{c}|{owner}|{}|100", r.range(1, 200)));
        }
        let allow = cast::<i32, _>(seed % 2);
        let mut rng0 = (cast::<u32, _>(seed)).wrapping_mul(0x6c07_8965) ^ 0x1234;
        if seed % 3 != 0 {
            // steer into the rumor (first RND * 10 <= 1)
            g.start_year = g.cur_year - 3;
            while Rng::from_state(rng0).rnd() * Ext::from_f64(10.0) > Ext::from_f64(1.0) {
                rng0 = rng0.wrapping_add(7919);
            }
        }
        let opts = Opts { args: vec![allow.to_string()], rng: Some(rng0), skips: PEER_SKIPS.to_vec(), ..Opts::default() };
        let Some(o) = run_original_with(0x63ac02, &opts, &g) else { return };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Scripted::default();
        crypto_ban_rumor_event(&mut port, &mut ui, &mut rng, allow);
        assert!(unexpected(&o.log).is_empty(), "{:?}", unexpected(&o.log));
        let port_msgs = ui.log.iter().filter(|l| l.starts_with("MSGBOX")).count();
        assert_eq!(port_msgs, orig_msgs(&o.log), "message boxes: port {:?}\norig {:?}", ui.log, o.log);
        if std::env::var_os("ECON_STATS").is_some() {
            eprintln!("STAT ban={} rumors={} msgs={}", port.crypto_ban_state, port.btc_ban_rumor_count - g.btc_ban_rumor_count, port_msgs);
        }
        cmp().assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "RND sequence");
    });
}

