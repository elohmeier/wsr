//! Crypto and commodity shocks: `UpdateCryptoPrices` (0x6b68ef), `CryptoBanRumorEvent` (0x63ac02),
//! `ShockCommodityPrice` (0x635b16), `RebaseCommodityTargetsFromHistory` (0x63556a) with its GOSUBs
//! `RebaseCommodityTargets_Analyze` (0x63567b) and `RebaseCommodityTargets_SetTarget` (0x635768).
use crate::pb::real::{Ext, cast};


use super::*;
use crate::logic::finance::calc_entity_financials;
use crate::logic::valuation::update_main_commodity_display;
use crate::pb::string::parse_any;

/// Port of UpdateCryptoPrices (0x6b68ef).
/// Moves the bitcoin and ethereum prices toward their targets. On first use (a target of 0) it picks
/// `CryptoBoomYears`, the trend targets and step percentages, targets around 100,000 / 3,500, prices
/// within 90..110 % of the targets and the futures bases `FuturesBasis(7/8)` (200..280). Afterwards, once
/// gold has a target and the market valuation is set: with `RandSeed MOD 15 = 1` (or a maxed trend step)
/// a new bitcoin target from the gold target, the years since the start (boom phase) and the stock
/// index (cut to 10 % during a crypto ban), a new trend target and basis; ethereum likewise (1 in 15, from
/// the silver target). Each price then moves 6..20 % (growing step) plus up to 12 % of the gap, in the
/// wrong direction with probability 1/5.
///
/// The ON ERROR handler only shows ShowRuntimeError.
pub fn update_crypto_prices(g: &mut Globals, rng: &mut Rng) {
    if g.crypto_boom_years == 0 {
        g.crypto_boom_years = rng.rnd_range(20, 5);
    }
    let mut boom_year = g.cur_year - g.start_year;
    if boom_year < 1 {
        boom_year = 1;
    } else if g.crypto_boom_years < boom_year {
        boom_year = g.crypto_boom_years;
    }
    let past_boom = (g.cur_year - g.start_year) - boom_year;
    if 0 < past_boom {
        boom_year = boom_year - past_boom;
    }
    if boom_year < -5 {
        boom_year = -5;
    }
    if g.btc_trend_target == 0 {
        g.btc_trend_target = rng.rnd_range(300, 1);
    }
    if g.eth_trend_target == 0 {
        g.eth_trend_target = rng.rnd_range(300, 1);
    }
    if g.btc_trend_step_pct == 0 {
        g.btc_trend_step_pct = 6;
    }
    if g.eth_trend_step_pct == 0 {
        g.eth_trend_step_pct = 6;
    }
    if g.bitcoin_target_price == 0 || g.ethereum_target_price == 0 {
        g.bitcoin_target_price = to_long(Ext::from_f64(20000.0) * rng.rnd() + Ext::from_f64(100000.0));
        g.ethereum_target_price = to_long(Ext::from_f64(1000.0) * rng.rnd() + Ext::from_f64(3500.0));
        let t = cast::<Ext, _>(g.bitcoin_target_price);
        g.bitcoin_price = to_long(t * sf(0.9f32) + t * (sf(0.2f32) * rng.rnd()));
        let t = cast::<Ext, _>(g.ethereum_target_price);
        g.ethereum_price = to_long(t * sf(0.9f32) + t * (sf(0.2f32) * rng.rnd()));
        g.futures_basis[7] = cast::<i16, _>(to_quad(Ext::from_f64(80.0) * rng.rnd() + Ext::from_f64(200.0)));
        g.futures_basis[8] = cast::<i16, _>(to_quad(Ext::from_f64(80.0) * rng.rnd() + Ext::from_f64(200.0)));
        return;
    }
    if Ext::from_f64(1.0) > cast::<Ext, _>(g.gold_target_price) || g.market_val_target == Ext::from_f64(0.0) || g.market_valuation == Ext::from_f32(0.0f32) {
        return;
    }
    let mut index_boost = to_long(sf(0.2f32) * g.stock_index);
    if 6000 < index_boost {
        index_boost = 5000;
    }
    if g.crypto_ban_state == 1 {
        index_boost = to_long(Ext::from_f64(1.0) * rng.rnd() * cast::<Ext, _>(index_boost));
    }
    // bitcoin
    if g.rand_seed % 15 == 1 || g.btc_trend_step_pct == 20 {
        let old_target = g.bitcoin_target_price;
        let r = cast::<Ext, _>(rng.rnd_range(2, 1));
        let x = cast::<Ext, _>(g.gold_target_price) * (r * cast::<Ext, _>(boom_year) + Ext::from_f64(17.0));
        g.bitcoin_target_price = to_long(x * (sf(0.8f32) * rng.rnd() + sf(0.6f32)));
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
            g.bitcoin_target_price = g.bitcoin_target_price.wrapping_add(index_boost);
        }
        g.bitcoin_target_price = to_long(Ext::from_f64(0.5) * (cast::<Ext, _>(old_target) + cast::<Ext, _>(g.bitcoin_target_price)));
        if g.crypto_ban_state == 1 {
            g.bitcoin_target_price = to_long(sf(0.1f32) * cast::<Ext, _>(g.bitcoin_target_price));
        }
        g.btc_trend_target = rng.rnd_range(300, 250);
        if Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
            g.btc_trend_target = rng.rnd_range(320, 1) - 20;
        }
        let gap = g.btc_trend_target - cast::<i32, _>(g.futures_basis[7]);
        g.futures_basis[7] = cast::<i16, _>(to_quad(sf(0.2f32) * (Ext::from_f64(2.0) * rng.rnd()) * cast::<Ext, _>(gap) + cast::<Ext, _>(g.futures_basis[7])));
        g.btc_trend_step_pct = 6;
    } else {
        g.btc_trend_step_pct = g.btc_trend_step_pct.wrapping_add(1);
    }
    if 20 < g.btc_trend_step_pct {
        g.btc_trend_step_pct = 20;
    }
    let gap = g.bitcoin_target_price.wrapping_sub(g.bitcoin_price);
    let a = cast::<Ext, _>(gap) * (sf(0.12f32) * rng.rnd());
    let mut price_step = to_long(a + cast::<Ext, _>(g.btc_trend_step_pct) * sf(0.01f32) * cast::<Ext, _>(gap));
    if Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
        price_step = price_step.wrapping_mul(-1);
    }
    g.bitcoin_price = g.bitcoin_price.wrapping_add(price_step);
    // ethereum
    if Ext::from_f64(15.0) * rng.rnd() > Ext::from_f64(14.0) || g.eth_trend_step_pct == 20 {
        let old_target = g.ethereum_target_price;
        let r = cast::<Ext, _>(rng.rnd_range(2, 1));
        let x = cast::<Ext, _>(g.silver_target_price) * (r * cast::<Ext, _>(boom_year) + Ext::from_f64(40.0));
        g.ethereum_target_price = to_long(x * (sf(0.6f32) * rng.rnd() + sf(0.7f32)));
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
            g.ethereum_target_price = to_long(cast::<Ext, _>(index_boost) * Ext::from_f64(0.5) + cast::<Ext, _>(g.ethereum_target_price));
        }
        g.ethereum_target_price = to_long(Ext::from_f64(0.5) * (cast::<Ext, _>(old_target) + cast::<Ext, _>(g.ethereum_target_price)));
        if g.crypto_ban_state == 1 {
            g.ethereum_target_price = to_long(sf(0.1f32) * cast::<Ext, _>(g.ethereum_target_price));
        }
        g.eth_trend_target = rng.rnd_range(300, 250);
        if Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
            g.eth_trend_target = rng.rnd_range(320, 1) - 20;
        }
        let gap = g.eth_trend_target - cast::<i32, _>(g.futures_basis[8]);
        g.futures_basis[8] = cast::<i16, _>(to_quad(sf(0.2f32) * (Ext::from_f64(2.0) * rng.rnd()) * cast::<Ext, _>(gap) + cast::<Ext, _>(g.futures_basis[8])));
        g.eth_trend_step_pct = 6;
    } else {
        g.eth_trend_step_pct = g.eth_trend_step_pct.wrapping_add(1);
    }
    if 20 < g.eth_trend_step_pct {
        g.eth_trend_step_pct = 20;
    }
    let gap = g.ethereum_target_price.wrapping_sub(g.ethereum_price);
    let a = cast::<Ext, _>(gap) * (sf(0.12f32) * rng.rnd());
    let mut price_step = to_long(a + cast::<Ext, _>(g.eth_trend_step_pct) * sf(0.01f32) * cast::<Ext, _>(gap));
    if Ext::from_f64(5.0) * rng.rnd() > Ext::from_f64(4.0) {
        price_step = price_step.wrapping_mul(-1);
    }
    g.ethereum_price = g.ethereum_price.wrapping_add(price_step);
}

/// Port of CryptoBanRumorEvent (0x63ac02).
/// Unless a ban is in force (`CryptoBanState = 1`): at most twice per game (`BtcBanRumorCount`), from the
/// third year on, with probability 1/10 (no pop-ups, no earlier "no ban" decision -1), posts the rumor
/// that the U.S. or China (alternating) plans to ban bitcoin / cryptocurrencies. Then, unless the state
/// is 2, re-decides the state: 0, or — when a human player with a net worth of at least
/// `ExchangeRate * 6500` holds (directly or through controlled companies) more than 150 crypto units in
/// futures and physical records (bitcoin 1, ethereum 0.1 each) — -1, or 1 (ban) with `allow_ban = 1`.
pub fn crypto_ban_rumor_event(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, allow_ban: i32) {
    let __content = crate::content::current();
    if g.crypto_ban_state == 1 {
        return;
    }
    if g.btc_ban_rumor_count < 2
        && g.suppress_popups == 0
        && g.crypto_ban_state != -1
        && Ext::from_f64(10.0) * rng.rnd() <= Ext::from_f64(1.0)
        && g.start_year + 2 <= g.cur_year
    {
        g.btc_ban_rumor_count += 1;
        if g.btc_ban_rumor_country.is_empty() {
            g.btc_ban_rumor_country = if Ext::from_f64(3.0) * rng.rnd() <= Ext::from_f64(1.0) { s("U.S.") } else { s("China") };
        } else if g.btc_ban_rumor_country == "China" {
            g.btc_ban_rumor_country = s("U.S.");
        } else {
            g.btc_ban_rumor_country = s("China");
        }
        let mut rumor_text = s(__content.string(4370)) + &g.btc_ban_rumor_country + __content.string(4371);
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            rumor_text = replace(rumor_text.as_bytes(), b"BitCoin", __content.bytes(4372));
        }
        g.news_msg = rumor_text.clone();
        post_news(g, ui, 0, 0);
        show_quick_message(g, ui, &mut rumor_text);
    }
    if g.crypto_ban_state == 2 {
        return;
    }
    g.crypto_ban_state = 0;
    let num_players = g.num_players;
    for player_id in 1..=num_players {
        if g.entity_status[player_id] != 1 || g.is_computer_player[player_id] == 1 {
            continue;
        }
        calc_entity_financials(g, ui, rng, player_id);
        if !(g.net_worth >= g.exchange_rate * Ext::from_f64(6500.0)) {
            continue;
        }
        let mut crypto_exposure: Ext = Ext::from_f64(0.0);
        let fut_count = to_long(val(g.futures_recs[0].as_bytes()));
        for fut_idx in 1..=fut_count {
            let rec = g.futures_recs[fut_idx].clone();
            crypto_exposure = add_crypto_exposure(g, &rec, player_id, crypto_exposure);
        }
        let phys_count = to_long(val(g.phys_commod_rec[0].as_bytes()));
        for phys_idx in 1..=phys_count {
            let rec = g.phys_commod_rec[phys_idx].clone();
            crypto_exposure = add_crypto_exposure(g, &rec, player_id, crypto_exposure);
        }
        if crypto_exposure > Ext::from_f64(150.0) {
            g.crypto_ban_state = -1;
            if allow_ban == 1 {
                g.crypto_ban_state = 1;
            }
            return;
        }
    }
}

/// One futures / physical record of CryptoBanRumorEvent: owner `player_id` or a company it controls.
fn add_crypto_exposure(g: &mut Globals, rec: &PbStr, player_id: i32, exposure: Ext) -> Ext {
    let owner_id = to_long(val(parse_any(rec.as_bytes(), b"|", 2).as_bytes()));
    if player_id != owner_id && player_id != g.corp_controller[owner_id] {
        return exposure;
    }
    g.cur_commodity = parse_any(rec.as_bytes(), b"|", 1);
    let qty = to_quad(val(parse_any(rec.as_bytes(), b"|", 3).as_bytes()));
    if g.cur_commodity == "BITCOIN" {
        cast::<Ext, _>(qty) + exposure
    } else if g.cur_commodity == "ETHEREUM" {
        cast::<Ext, _>(qty) * sf(0.1f32) + exposure
    } else {
        exposure
    }
}

/// Port of ShockCommodityPrice (0x635b16).
/// Moves a commodity price by `pct_change` percent: `"OIL"` (price first raised to at least 0.30),
/// `"GOLD"`/`"SILVER"` (both metals, clamped to `MetalsPriceBase` + 140..4800 and
/// `MetalsPriceBase / 100` + 4..41 with random re-entry values) or `"WHEAT"`/`"CORN"` (both grains, clamped
/// to 2..17). The targets move by the factor x1.1 (rise) or x0.93 (fall); for metals and grains the
/// min/max range becomes price..target. Other names change nothing. Ends with UpdateMainCommodityDisplay.
pub fn shock_commodity_price(g: &mut Globals, rng: &mut Rng, commod_name: &[u8], pct_change: i32) {
    let name = PbStr::from(commod_name);
    let mut commod_group = name.clone();
    if name == "GOLD" || name == "SILVER" {
        commod_group = s("METALS");
    }
    if name == "WHEAT" || name == "CORN" {
        commod_group = s("GRAIN");
    }
    let price_factor: Ext = sf(0.01f32) * cast::<Ext, _>(pct_change) + Ext::from_f64(1.0);
    let target_factor: Ext = if Ext::from_f64(1.0) >= price_factor { sf(0.93f32) * price_factor } else { sf(1.1f32) * price_factor };
    let base = cast::<Ext, _>(g.metals_price_base);
    if commod_group == "OIL" {
        if sf(0.3f32) > cast::<Ext, _>(g.oil_price) {
            g.oil_price = cast::<f32, _>(Ext::from_f32(0.3f32));
        }
        g.oil_price = cast::<f32, _>(cast::<f32, _>(price_factor * cast::<Ext, _>(g.oil_price)));
        g.oil_target_price = cast::<f32, _>(cast::<f32, _>(target_factor * cast::<Ext, _>(g.oil_target_price)));
    } else if commod_group == "METALS" {
        g.gold_price = cast::<f32, _>(cast::<f32, _>(price_factor * cast::<Ext, _>(g.gold_price)));
        if base + Ext::from_f64(140.0) > cast::<Ext, _>(g.gold_price) {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(140.0) + base));
        }
        if !(base + Ext::from_f64(4800.0) >= cast::<Ext, _>(g.gold_price)) {
            g.gold_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1000.0) * rng.rnd() + Ext::from_f64(4800.0) + base));
        }
        g.gold_target_price = cast::<f32, _>(cast::<f32, _>(target_factor * cast::<Ext, _>(g.gold_target_price)));
        g.gold_price_min = cast::<f32, _>(g.gold_price);
        g.gold_price_max = cast::<f32, _>(g.gold_target_price);
        g.silver_price = cast::<f32, _>(cast::<f32, _>(price_factor * cast::<Ext, _>(g.silver_price)));
        if base * sf(0.01f32) + Ext::from_f64(4.0) > cast::<Ext, _>(g.silver_price) {
            g.silver_price = cast::<f32, _>(cast::<f32, _>((sf(0.3f32) * rng.rnd() + Ext::from_f64(4.0)) + base * sf(0.01f32)));
        }
        if !(base * sf(0.01f32) + Ext::from_f64(41.0) >= cast::<Ext, _>(g.silver_price)) {
            g.silver_price = cast::<f32, _>(cast::<f32, _>((Ext::from_f64(4.0) * rng.rnd() + Ext::from_f64(39.0)) + base * sf(0.01f32)));
        }
        g.silver_target_price = cast::<f32, _>(cast::<f32, _>(target_factor * cast::<Ext, _>(g.silver_target_price)));
        g.silver_price_min = cast::<f32, _>(g.silver_price);
        g.silver_price_max = cast::<f32, _>(g.silver_target_price);
    } else if commod_group == "GRAIN" {
        g.wheat_price = cast::<f32, _>(cast::<f32, _>(price_factor * cast::<Ext, _>(g.wheat_price)));
        if Ext::from_f64(2.0) > cast::<Ext, _>(g.wheat_price) {
            g.wheat_price = cast::<f32, _>(cast::<f32, _>(sf(0.2f32) * rng.rnd() + sf(1.8f32)));
        }
        if !(Ext::from_f64(17.0) >= cast::<Ext, _>(g.wheat_price)) {
            g.wheat_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(17.0)));
        }
        g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(target_factor * cast::<Ext, _>(g.wheat_target_price)));
        g.wheat_price_min = cast::<f32, _>(g.wheat_price);
        g.wheat_price_max = cast::<f32, _>(g.wheat_target_price);
        g.corn_price = cast::<f32, _>(cast::<f32, _>(price_factor * cast::<Ext, _>(g.corn_price)));
        if Ext::from_f64(2.0) > cast::<Ext, _>(g.corn_price) {
            g.corn_price = cast::<f32, _>(cast::<f32, _>(sf(0.2f32) * rng.rnd() + sf(1.8f32)));
        }
        if !(Ext::from_f64(17.0) >= cast::<Ext, _>(g.corn_price)) {
            g.corn_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(1.0) * rng.rnd() + Ext::from_f64(17.0)));
        }
        g.corn_target_price = cast::<f32, _>(cast::<f32, _>(target_factor * cast::<Ext, _>(g.corn_target_price)));
        g.corn_price_min = cast::<f32, _>(g.corn_price);
        g.corn_price_max = cast::<f32, _>(g.corn_target_price);
    }
    update_main_commodity_display(g);
}

/// Port of RebaseCommodityTargetsFromHistory (0x63556a).
/// Except when `(CurYear + CurQuarter) MOD 4 = 1` or without a full gold history (`CapHistClose(60, 7) =
/// 0`): for oil (only outside a peak-oil crisis), gold, silver, wheat and corn in turn, each with
/// probability 1/3, compares the 60-month price history (series 6..10) with a reference price (100,
/// `MetalsPriceBase` + 1400, `MetalsPriceBase / 100` + 20, 6.25, 6.25). When more than 50 months lie above
/// it, the target becomes 75..90 % of the reference; more than 50 at or below it, 110..125 %. Oil also
/// loses 6 % of its price; a wheat target also sets the corn target (up to 0.60 lower) and — the GOSUB
/// jumps to the routine's exit — ends the routine, so corn is then not analysed (quirk). A corn target sets
/// the wheat target to corn + 0.50.
pub fn rebase_commodity_targets_from_history(g: &mut Globals, rng: &mut Rng) {
    if (g.cur_year + g.cur_quarter) % 4 == 1 {
        return; // EmptyStub_6358ca (0x6358ca): the routine's exit
    }
    if g.cap_hist_close[(60, 7)] == Ext::from_f64(0.0) {
        return;
    }
    if (cast::<i32, _>(g.peak_oil_crisis)) < 1 {
        if rebase_targets_analyze(g, rng, 6, Ext::from_f64(100.0)) {
            return;
        }
    }
    let base = cast::<Ext, _>(g.metals_price_base);
    let refs = [(7, base + Ext::from_f64(1400.0)), (8, base * sf(0.01f32) + Ext::from_f64(20.0)), (9, sf(6.25f32)), (10, sf(6.25f32))];
    for (hist_col, ref_price) in refs {
        if rebase_targets_analyze(g, rng, hist_col, ref_price) {
            return;
        }
    }
}

/// GOSUB RebaseCommodityTargets_Analyze (0x63567b) with RebaseCommodityTargets_SetTarget (0x635768);
/// `true` when SetTarget jumped to the routine's exit (wheat).
fn rebase_targets_analyze(g: &mut Globals, rng: &mut Rng, hist_col: i32, cur_price: Ext) -> bool {
    if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
        return false;
    }
    let mut count_above = 0;
    let mut count_below = 0;
    let mut trend_dir = 0;
    for period in 1..=60 {
        let h = g.cap_hist_close[(period, hist_col)];
        if h != Ext::from_f64(0.0) {
            if h <= cur_price {
                count_below += 1;
            } else {
                count_above += 1;
            }
        }
    }
    if 50 < count_above {
        trend_dir = 1;
    }
    if 50 < count_below {
        trend_dir = -1;
    }
    if trend_dir == 0 {
        return false;
    }
    // GOSUB RebaseCommodityTargets_SetTarget
    let new_target: Ext = if trend_dir == 1 {
        cur_price * (sf(0.15f32) * rng.rnd() + sf(0.75f32))
    } else {
        cur_price * (sf(0.15f32) * rng.rnd() + sf(1.1f32))
    };
    match hist_col {
        6 => {
            g.oil_target_price = cast::<f32, _>(cast::<f32, _>(sf(0.01f32) * new_target));
            g.oil_price = cast::<f32, _>(cast::<f32, _>(sf(0.94f32) * cast::<Ext, _>(g.oil_price)));
        }
        7 => g.gold_target_price = cast::<f32, _>(cast::<f32, _>(new_target)),
        8 => g.silver_target_price = cast::<f32, _>(cast::<f32, _>(new_target)),
        9 => {
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(new_target));
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(new_target - sf(0.6f32) * rng.rnd()));
            return true; // jmp 0x6358ca: exits RebaseCommodityTargetsFromHistory
        }
        10 => {
            g.corn_target_price = cast::<f32, _>(cast::<f32, _>(new_target));
            g.wheat_target_price = cast::<f32, _>(cast::<f32, _>(Ext::from_f64(0.5) + new_target));
        }
        _ => {}
    }
    false
}
