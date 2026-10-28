//! Save/load round trips for `SaveGame` (0x4857af) / `LoadGame` (0x48a1d4) and the GAMExx.DAT codec.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


use wsr_core::logic::holdings;
use wsr_core::pb::codec;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::save::{self, LoadError};
use wsr_core::state::Globals;

const VERSION: &str = "9.85";

/// xorshift64* — deterministic test data without dependencies.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + cast::<i64, _>(self.next() % cast::<u64, _>(hi - lo + 1))
    }
    fn f(&mut self, scale: Ext) -> Ext {
        let u = cast::<Ext, _>(self.next() >> 11) / cast::<Ext, _>(1u64 << 53);
        (u * Ext::from_f64(2.0) - Ext::from_f64(1.0)) * scale
    }
    fn word(&mut self, n: usize) -> PbStr {
        let v: Vec<u8> = (0..n).map(|_| b'A' + cast::<u8, _>(self.next() % 26)).collect();
        PbStr(v)
    }
}

fn st(x: &str) -> PbStr {
    PbStr::from(x)
}

fn fresh() -> Globals {
    let mut g = Globals::default();
    g.version_str = st(VERSION);
    g
}

/// A plausible mid-game state: 2 players, a few active banks/insurers/corps/funds, holdings with gaps,
/// option/futures/swap records, price histories.
fn sample(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut r = Rng(seed);
    let mut g = fresh();
    g.turn_action_count = 1234;
    g.save_toggle = 0;
    g.active_entity = 1;
    g.active_entity_name = st("Alice Raider");
    g.sel_corp_name = st("Acme Widgets Inc.");
    g.sel_entity = 140;
    g.long_game_marker = 1;
    g.asset_sale_desc = st("no sale");
    g.entity_borrow_rate = cast::<f32, _>(Ext::from_f64(7.25));
    g.bank_loans_total = r.f(Ext::from_f64(1e7));
    g.max_bond_issue = 150_000;
    g.long_bond_price = cast::<f32, _>(Ext::from_f64(98.5));
    g.cap_gains_tax_rate = 20;
    g.bank_cd_rate = cast::<f32, _>(Ext::from_f64(3.1));
    g.central_bank_name = st(__content.string(4373));
    g.cheat_mode = st("N");
    g.sim_co_count = 1600;
    g.has_cpu_players = st("N");
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.currency_name = st("Dollars");
    g.difficulty_level = 2;
    g.currency_symbol = st("$");
    g.currency_suffix = st("");
    g.recent_mgmt_change_list = st("12,140");
    g.gdp_level = 21_000_000;
    g.gdp_growth = cast::<f32, _>(Ext::from_f64(2.4));
    g.housing_starts = 1_400_000;
    g.stock_index_divisor = r.f(Ext::from_f64(100.0)).abs() + Ext::from_f64(1.0);
    g.sofr_rate = cast::<f32, _>(Ext::from_f64(4.33));
    g.monetary_stance = -1;
    g.new_mgmt_list = st("");
    g.merger_proposer_player = 1;
    g.merger_acquirer = 140;
    g.merger_target = 141;
    g.merger_price_per_share.set(Ext::from_f64(42.17));
    g.million_prefix_lc = st("m");
    g.million_prefix_uc = st("M");
    g.world_generated = 1;
    g.name_list_dirty = 1;
    g.oil_price = cast::<f32, _>(Ext::from_f64(71.3));
    g.oil_crisis_level = cast::<f32, _>(Ext::from_f64(0.5));
    g.cur_player = 1;
    g.cur_player_name = st("Alice Raider");
    g.num_players = 2;
    g.base_interest_rate = cast::<f32, _>(Ext::from_f64(4.5));
    g.exe_name = st("WSR.EXE");
    g.cur_quarter = 3;
    g.currency_code = st("US");
    g.game_end_year = 2060;
    g.short_bond_desc = st("2-Yr Treasury Notes");
    g.short_bond_yield = cast::<f32, _>(Ext::from_f64(4.1));
    g.short_bond_price = cast::<f32, _>(Ext::from_f64(99.25));
    g.scenario_tag = st("");
    g.rand_seed = 987_654;
    g.lookup_entity_name = st("");
    g.target_corp = 150;
    g.sel_industry = 7;
    g.prime_rate = cast::<f32, _>(Ext::from_f64(7.5));
    g.stock_index = Ext::from_f64(12_345.678);
    g.market_valuation = cast::<f32, _>(Ext::from_f64(1.05));
    g.max_stock_offering_amt = 100_000;
    g.start_year = 2024;
    g.cur_year = 2026;
    g.gdp_growth_target = cast::<f32, _>(Ext::from_f64(2.0));
    g.interest_rate_target = cast::<f32, _>(Ext::from_f64(4.0));
    g.oil_target_price = cast::<f32, _>(Ext::from_f64(75.0));
    g.long_bond_desc = st("30-Yr Treasury Bonds");
    g.long_bond_yield = cast::<f32, _>(Ext::from_f64(4.6));
    g.ticker_text = st("ACME 12.50 +0.25  BIGB 3.10 -0.05");
    g.ticker_speed = 50;
    g.corp_tax_rate = 21;
    g.app_title = st(__content.string(13661));
    for p in 1..=2 {
        g.player_select_last_corp[p] = 140 + p;
        g.player_auto_save[p] = 1;
        g.player_auto_add_stream[p] = 1;
        g.player_suppress_cash_warn[p] = p - 1;
        g.player_ceo_of[p] = 139 + p;
        g.ent_name[p] = if p == 1 { st("Alice Raider") } else { st("Bob Bidder") };
        g.ent_symbol[p] = st(if p == 1 { "ALICE" } else { "BOB" });
        g.player_prepaid_tax[p] = r.f(Ext::from_f64(1e3));
        g.player_cap_gains[p] = r.f(Ext::from_f64(1e5));
        g.player_notoriety[p] = 3;
        g.player_law_firm[p] = 2;
        g.player_name[p] = g.ent_name[p].clone();
        g.player_turn_order[p] = p;
        // No short positions in this sample, so the margin balance must be 0: RecalcShortMarginBalances
        // (run by SaveGame) would otherwise fold it into Cash. The temporary Cash/margin adjustment in
        // SaveGame is covered against the original in diff_save.rs.
        g.player_short_margin_bal[p] = Ext::from_f64(0.0);
        let _ = r.f(Ext::from_f64(1e4));
        g.player_cheat_code[p] = 0;
        g.player_growth_throttle[p] = 60;
        g.player_loan_offers_avail[p] = 4;
        g.sweep_cash_to_loan[p] = 1;
    }
    // player 1 has a streaming-quotes list
    g.watch_list[(0, 1)] = 3;
    for j in 1..=15 {
        g.watch_list[(j, 1)] = if j <= 3 { 140 + j } else { 0 };
        g.stream_prev_price_str[(j, 1)] = st("12.50");
        g.stream_cur_price[(j, 1)] = st("12.75");
        g.stream_price_dir[(j, 1)] = 1;
    }
    for i in 1..=60 {
        g.news_headline[i] = PbStr::from(format!("{__wsr_text_13752}{i}: markets move",__wsr_text_13752=__content.string(13752)).as_str());
    }
    for i in 1..=10 {
        g.lawsuit_rec[i] = st("");
    }
    for i in 0..=55 {
        g.country_name[i] = r.word(6);
        g.country_display_name[i] = r.word(8);
    }
    for i in 0..=70 {
        g.ind_name[i] = r.word(10);
    }
    for i in 1..=70 {
        g.ind_demand[i] = r.int(0, 1 << 40);
        g.ind_growth_rate[i] = cast::<i16, _>(r.int(-20, 40));
        g.ind_growth_forecast[i] = cast::<i16, _>(r.int(-20, 40));
    }
    for i in 1..=200 {
        g.event_status[i] = cast::<i16, _>(r.int(0, 2));
        g.event_state[i] = cast::<i32, _>(r.int(0, 5));
    }
    // entities: players 1..2, banks 11..20, insurers 86..90, corps 131..200, funds 1581..1590
    let active = |i: i32| (1..=2).contains(&i) || (11..=20).contains(&i) || (86..=90).contains(&i) || (131..=200).contains(&i) || (1581..=1590).contains(&i);
    for i in 1..=1600 {
        g.ent_misc_data[i] = if i % 7 == 0 { st("misc") } else { st("") };
        g.entity_status[i] = if active(i) { 1 } else if i % 97 == 0 { 2 } else { 0 };
        g.lending_bank[i] = cast::<i16, _>(11 + i % 10);
        if g.entity_status[i] == 1 {
            g.cash[i] = r.f(Ext::from_f64(1e6));
            g.corp_controller[i] = i % 3;
            g.accrued_pl[i] = r.f(Ext::from_f64(1e4));
            g.bank_debt[i] = r.f(Ext::from_f64(1e6)).abs();
            g.earnings_ytd[i] = r.f(Ext::from_f64(1e5));
            g.extraordinary_pl[i] = r.int(-1000, 1000);
            g.credit_rating_num[i] = cast::<i32, _>(r.int(1, 10));
            g.t_bills[i] = r.f(Ext::from_f64(1e5)).abs();
        }
        if i >= 11 {
            g.country_idx[i] = cast::<i32, _>(r.int(0, 55));
            g.corp_prior_net_worth[i] = r.f(Ext::from_f64(1e6));
            g.industry_num[i] = cast::<i16, _>(r.int(1, 70));
            g.ent_name[i] = PbStr::from(format!("{__wsr_text_13628}{i}{__wsr_text_13753}",__wsr_text_13628=__content.string(13628), __wsr_text_13753=__content.string(13753)).as_str());
            g.ent_symbol[i] = r.word(1 + cast::<usize, _>(i % 5));
            g.extra_items_ytd[i] = r.f(Ext::from_f64(100.0));
            g.auto_pilot[i] = i % 2;
            if g.entity_status[i] == 1 {
                g.dividend_per_share[i] = r.f(Ext::from_f64(5.0)).abs();
                g.market_cap[i] = r.f(Ext::from_f64(1e9)).abs();
                g.profit_margin[i] = cast::<i16, _>(r.int(-10, 30));
                g.bonds_issued[i] = r.int(0, 1_000_000);
                g.bond_issue_info[i] = st("2031|5.25");
                g.bond_price[i].set(Ext::from_f64(101.125));
                g.shares_outstanding[i] = r.f(Ext::from_f64(1e8)).abs();
                g.growth_rate[i] = cast::<i16, _>(r.int(-5, 25));
                g.investor_sentiment[i] = cast::<i32, _>(r.int(-3, 3));
                g.ctrl_since_year[i] = 2025;
                g.tax_credits[i] = r.f(Ext::from_f64(1e3)).abs();
                g.reserve_adj_pending[i] = r.f(Ext::from_f64(10.0));
                g.cap_hist_close[(0, i)] = if i % 3 == 0 { Ext::from_f64(0.0) } else { Ext::from_f64(1.0) };
                for j in 1..=60 {
                    g.price_hist_high[(j, i)] = r.f(Ext::from_f64(100.0)).abs();
                    g.cap_hist_close[(j, i)] = r.f(Ext::from_f64(100.0)).abs();
                    g.price_hist_low[(j, i)] = r.f(Ext::from_f64(100.0)).abs();
                }
            }
        }
        if (131..=1600).contains(&i) {
            g.corp_hedging_gain[i] = r.f(Ext::from_f64(1e3));
            if g.entity_status[i] == 1 {
                g.business_assets[i] = r.f(Ext::from_f64(1e7)).abs();
                g.working_capital[i] = r.f(Ext::from_f64(1e6));
                g.goodwill[i] = r.f(Ext::from_f64(1e5)).abs();
                g.contingency_reserve[i] = r.f(Ext::from_f64(1e3)).abs();
            }
        }
    }
    for i in 11..=85 {
        g.bank_demand_deposits[i] = r.f(Ext::from_f64(1e7)).abs();
        g.bank_consumer_loans[i] = r.int(0, 1 << 30);
        g.bank_c_ds[i] = r.f(Ext::from_f64(1e6)).abs();
        g.bank_cust_deposits[i] = r.f(Ext::from_f64(1e7)).abs();
        g.bank_alloc_prime_mort_pct[i] = 30;
        g.bank_alloc_cash_pct[i] = 10;
    }
    for i in 1..=130 {
        if g.entity_status[i] == 1 {
            g.long_gov_bond_face[i] = r.int(0, 1_000_000);
            g.lt_gov_bond_cost[i] = r.f(Ext::from_f64(1e6)).abs();
        }
    }
    // stock holdings with gaps (packing has to compact them); an inactive (status 2) holder gets wiped
    for &owner in &[1, 2, 12, 140, 150, 291] {
        for slot in [2, 5, 9, 15] {
            g.hold_stock_id[(slot, owner)] = cast::<i16, _>(131 + slot * 3);
            g.hold_stock_pct[(slot, owner)] = cast::<i16, _>(slot * 4);
            g.hold_stock_basis[(slot, owner)] = r.f(Ext::from_f64(1e4)).abs();
        }
        g.hold_stock_id[(0, owner)] = 15; // stale count
    }
    // bond holdings with gaps, incl. the bond fund 1582
    for &owner in &[1, 12, 87, 1582] {
        g.entity_status[1582] = 1;
        for slot in [3, 4, 11] {
            g.hold_bond_id[(slot, owner)] = 140 + slot;
            g.hold_bond_face[(slot, owner)] = 10_000 * cast::<i64, _>(slot);
            g.bond_hold_cost[(slot, owner)] = r.f(Ext::from_f64(1e4)).abs();
        }
    }
    // records with gaps
    g.option_recs[1] = st("C|140|2026|3|15.00|100");
    g.option_recs[4] = st("P|141|2026|4|10.00|50");
    g.futures_recs[2] = st("GOLD|1|2027|1|100");
    g.futures_recs[7] = st("CORN|140|2026|4|5000");
    g.commod_margin_bal[1] = Ext::from_f64(12_345.5);
    g.commod_margin_bal[140] = -Ext::from_f64(500.25);
    g.swap_contracts[3] = st("140|141|1000|4.5");
    g.phys_commod_rec[1] = st("GOLD|1|100");
    g.asset_sale_offers[5] = st("140|7|5000");
    g.price_alerts[2] = st("GOLD|>|2500");
    for i in 1..=50 {
        g.asset_buy_watch_list[i] = if i < 4 { 140 + i } else { 0 };
    }
    // index / player / fund price histories written unconditionally (i = 0, 6..10, 1601..1605, 1606)
    for &i in &[0, 1, 6, 10, 1601, 1605, 1606] {
        g.cap_hist_close[(0, i)] = Ext::from_f64(5.0);
        for j in 1..=60 {
            g.price_hist_high[(j, i)] = r.f(Ext::from_f64(1000.0)).abs();
            g.cap_hist_close[(j, i)] = r.f(Ext::from_f64(1000.0)).abs();
            g.price_hist_low[(j, i)] = r.f(Ext::from_f64(1000.0)).abs();
        }
    }
    for i in 1581..=1600 {
        g.fund_adviser[i] = cast::<i32, _>(r.int(0, 2));
        g.fund_fee_pct[i] = cast::<f32, _>(Ext::from_f64(1.5));
        g.fund_bonus_year[i] = 2027;
    }
    g.gold_price = cast::<f32, _>(Ext::from_f64(2412.5));
    g.gold_target_price = cast::<f32, _>(Ext::from_f64(2500.0));
    g.gold_price_min = cast::<f32, _>(Ext::from_f64(1250.0));
    g.gold_price_max = cast::<f32, _>(Ext::from_f64(3000.0));
    g.corn_price = cast::<f32, _>(Ext::from_f64(4.37));
    g.wheat_price = cast::<f32, _>(Ext::from_f64(5.81));
    g.silver_price = cast::<f32, _>(Ext::from_f64(29.9));
    for i in 1..=10 {
        g.futures_basis[i] = cast::<i16, _>(i * 2);
    }
    g.commod_trade_mode = st("F");
    g.start_net_worth = 1_000;
    g.loan_limit_multiple = cast::<f64, _>(Ext::from_f64(2.5));
    g.subprime_crisis_state = 1;
    g.market_val_target = cast::<f64, _>(Ext::from_f64(1.1));
    for i in 3..=69 {
        g.ind_demand_gap[i] = cast::<f64, _>(r.f(Ext::from_f64(10.0)));
    }
    g.stock_index_base = 1000;
    g.next_market_cycle_year = 2029;
    g.auto_save_state = 0;
    g.metals_price_base = 150;
    g.cpu_max_corps = 80;
    g.bitcoin_price = 65_000;
    g.ethereum_price = 3_200;
    g.save_extra_str[3] = st("extra3");
    g.market_cycle_plan = st("BULL|BEAR");
    g.scenario_headline = st("");
    g
}

fn lines(t: &[u8]) -> Vec<&[u8]> {
    codec::lines(t)
}

#[test]
fn save_load_save_is_byte_identical() {
    let __content = wsr_core::content::current();
    let mut g = sample(0x5eed_1234_abcd_ef01);
    // LoadGame runs UpdateMetalsPriceBase (recomputes MetalsPriceBase from the date); start from a state where
    // it is already consistent so save → load → save can be byte-identical.
    wsr_core::logic::futures::update_metals_price_base(&mut g, &mut wsr_core::ui::Scripted::default());
    // SaveGame first re-marks short and futures margins (RecalcShortMarginBalances,
    // RecalcCommodMarginBalances), which moves margin in/out of Cash; the temporary Cash adjustment that
    // follows must leave that value unchanged.
    let mut norm = g.clone();
    wsr_core::logic::shorts::recalc_short_margin_balances(&mut norm);
    wsr_core::logic::futures::recalc_commod_margin_balances(
        &mut norm,
        &mut wsr_core::ui::Scripted::default(),
        &mut wsr_core::pb::rng::Rng::default(),
        0,
    );
    let cash1 = norm.cash[1];
    let text1 = save::save_game(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());

    // side effects of SaveGame
    assert_eq!(g.save_toggle, 1, "SaveToggle flipped");
    assert_eq!(g.player_short_margin_bal[1], Ext::from_f64(0.0), "no short positions, no margin");
    assert!((g.cash[1] - cash1).abs() < Ext::from_f64(1e-6), "Cash restored after the temporary adjustment");
    assert_eq!(g.hold_stock_id[(0, 140)], 4, "holdings packed");
    assert_eq!(g.hold_stock_id[(1, 140)], 131 + 15 * 3, "last slot moved to the first gap");
    assert_eq!(g.entity_status[291], 2);
    assert_eq!(g.hold_stock_id[(5, 291)], 0, "status-2 holder wiped by ClearAllHoldings(2)");
    assert_eq!(g.option_recs[0], "2");
    assert_eq!(g.futures_recs[0], "2");
    assert_eq!(g.hold_bond_id[(0, 1582)], 3);

    let l = lines(&text1);
    assert_eq!(l[0], b"1234", "first line is TurnActionCount");
    assert_eq!(l[1], b"1", "second line is the flipped SaveToggle");
    assert_eq!(l[3], b"\"Alice Raider\"");
    assert_eq!(l[6], format!("\"{VERSION}\"").as_bytes(), "VersionStr");
    // the PRINTed names list ends the file (NameListDirty = 1)
    let last = l.last().unwrap();
    assert!(last.starts_with(b"1600|"), "{:?}", String::from_utf8_lossy(last));
    assert!(last.ends_with(b"|  Company 1600 Holdings |"));
    assert_eq!(l.len() - l.iter().position(|x| x.starts_with(b"0011|")).unwrap(), 1590);

    // load into a fresh state and save again
    let mut h = fresh();
    let info = save::load_game(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &text1).expect("load");
    assert_eq!(info.version, 9.85f32);
    assert!(!info.needs_corp_names);
    assert_eq!(h.start_year, 2024);
    assert_eq!(h.cur_year, 2026);
    assert_eq!(h.game_end_year, 2060);
    assert_eq!(h.loaded_end_year_dup, 2060);
    assert_eq!(h.ind_name[71], __content.string(6736));
    assert_eq!(h.market_cycle_plan, "BULL|BEAR");
    // re-marked by SaveGame, then saved with WRITE # (15 significant digits)
    assert!((h.commod_margin_bal[140] - g.commod_margin_bal[140]).abs() <= g.commod_margin_bal[140].abs() * Ext::from_f64(1e-14));
    assert_eq!(h.ent_name[150], "Company 150 Holdings");
    assert_eq!(h.ent_symbol[150], g.ent_symbol[150]);
    // the players' cash in the file includes the short margin balance
    let want = g.cash[1] + g.player_short_margin_bal[1];
    assert!((h.cash[1] - want).abs() <= want.abs() * Ext::from_f64(1e-14));
    h.save_toggle = 0; // SaveGame flips it again
    let text2 = save::save_game(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    if text1 != text2 {
        let (a, b) = (lines(&text1), lines(&text2));
        let k = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        panic!(
            "{__wsr_text_13749}{}: {:?} vs {:?} (lens {} / {}); context {:?}",
            k + 1,
            a.get(k).map(|x| String::from_utf8_lossy(x)),
            b.get(k).map(|x| String::from_utf8_lossy(x)),
            a.len(),
            b.len(),
            a[k.saturating_sub(40)..k].iter().map(|x| String::from_utf8_lossy(x).into_owned()).collect::<Vec<_>>()
        ,__wsr_text_13749=__content.string(13749));
    }
}

#[test]
fn encoded_file_round_trip_and_headline_repair() {
    let __content = wsr_core::content::current();
    let mut g = sample(77);
    g.news_headline[3] = PbStr(b"Gold hits \xa3 2,000 and \xa5 300,000".to_vec());
    let file = save::write_save_file(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    assert!(file.starts_with(format!("@EC{VERSION}\r\n").as_bytes()));
    let plain = { save::save_game(&mut sample(77), &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default()) };
    assert_eq!(codec::lines(&file).len(), codec::lines(&plain).len() + 1);

    // lossy (original) decoder: £/¥ come back as CHR$(202)/CHR$(204); MenuOpenSavedGame repairs headlines
    let mut h = fresh();
    save::read_save_file(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &file, false).expect("load");
    assert_eq!(h.news_headline[3].as_bytes(), b"Gold hits \xca 2,000 and \xcc 300,000");
    save::open_saved_game_fixups(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &st("12:00:00"), &st("01-02-2026"));
    // (UpdateMarketDisplay at the end of the fix-ups may post news, shifting the headlines up)
    assert!((1..=60).any(|i| h.news_headline[i] == g.news_headline[3]), "{__wsr_text_14118}",__wsr_text_14118=__content.string(14118));
    assert_eq!(h.game_in_progress, 1);

    // fixed decoder gives the exact text back
    let mut k = fresh();
    save::read_save_file(&mut k, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &file, true).expect("load");
    assert_eq!(k.news_headline[3], g.news_headline[3]);
    // ... and the same state as loading the plain text directly (no codec involved)
    let mut k_ref = fresh();
    let decoded = wsr_core::pb::codec::decode_save(&file, true);
    save::load_game(&mut k_ref, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &decoded.plain).expect("load plain");
    assert_eq!(
        save::save_game(&mut k, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default()),
        save::save_game(&mut k_ref, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default())
    );
}

#[test]
fn old_versions_are_rejected() {
    let __content = wsr_core::content::current();
    let mut g = sample(5);
    g.version_str = st("5.5");
    let text = save::save_game(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    let mut h = fresh();
    h.turn_action_count = 99;
    match save::load_game(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &text) {
        Err(LoadError::VersionRejected { message }) => {
            assert_eq!(message, __content.string(14448))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(h.turn_action_count, 0);
    assert_eq!(h.game_end_flag, "Y");
    assert_eq!(h.saved_game_version, "");

    // files without @EC header are refused by EncodeDecodeSaveGameFile
    let mut h = fresh();
    match save::read_save_file(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), b"1\r\n0\r\n", false) {
        Err(e @ LoadError::OldFileFormat { .. }) => {
            assert_eq!(
                e.old_format_message(&st("GAME01.DAT")).unwrap(),
                __content.string(14449)
            )
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn truncated_file_is_input_past_end() {
    let mut g = sample(9);
    let text = save::save_game(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    let cut = &text[..text.len() / 2];
    let mut h = fresh();
    assert_eq!(save::load_game(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), cut), Err(LoadError::Runtime { err: 62 }));
    assert_eq!(h.game_end_flag, "Y");
}

#[test]
fn clean_name_list_needs_corpname_dat() {
    let mut g = sample(11);
    g.name_list_dirty = 0;
    let text = save::save_game(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    assert!(!lines(&text).last().unwrap().starts_with(b"1600|"));
    let mut h = fresh();
    let info = save::load_game(&mut h, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &text).expect("load");
    assert!(info.needs_corp_names);
    save::load_corp_names(&mut h, b"0011|03|ABC  |  Abc Corp |\r\n0012|00|XY   |  Xy Inc |\r\n@END\r\n").unwrap();
    assert_eq!(h.ent_name[11], "Abc Corp");
    assert_eq!(h.ent_symbol[12], "XY");
    assert_eq!(h.country_idx[11], 3);
}

#[test]
fn pack_holding_arrays_compacts_and_counts() {
    let mut g = fresh();
    g.entity_status[140] = 1;
    g.entity_status[1583] = 1;
    for (slot, id) in [(1, 150), (3, 151), (4, 152), (15, 153)] {
        g.hold_stock_id[(slot, 140)] = id;
        g.hold_stock_pct[(slot, 140)] = 10;
        g.hold_stock_basis[(slot, 140)] = cast::<Ext, _>(id);
    }
    g.hold_stock_pct[(2, 140)] = 7; // garbage in an empty slot is cleared
    g.hold_bond_id[(2, 1583)] = 200;
    g.hold_bond_id[(5, 1583)] = -1; // negative ids are neither used nor empty
    g.price_alerts[100] = st("x");
    let ev = holdings::pack_holding_arrays(&mut g);
    assert!(ev.is_empty());
    let ids: Vec<i16> = (0..=5).map(|s| g.hold_stock_id[(s, 140)]).collect();
    assert_eq!(ids, vec![4, 150, 153, 151, 152, 0]);
    assert_eq!(g.hold_stock_pct[(2, 140)], 10);
    assert_eq!(g.hold_stock_basis[(2, 140)], Ext::from_f64(153.0));
    assert_eq!(g.hold_bond_id[(0, 1583)], 1);
    assert_eq!(g.hold_bond_id[(1, 1583)], 200);
    assert_eq!(g.hold_bond_id[(5, 1583)], -1);
    assert_eq!(g.price_alerts[0], "1");
    assert_eq!(g.price_alerts[1], "x");
    assert_eq!(g.option_recs[0], "0");

    // a moved holding with a negative basis below 80% triggers RecaptureExcessLoss on the next holder
    let mut g = fresh();
    g.num_players = 1;
    g.entity_status[140] = 1;
    g.entity_status[141] = 1;
    g.hold_stock_id[(3, 140)] = 160;
    g.hold_stock_basis[(3, 140)] = -Ext::from_f64(5.0);
    g.hold_stock_id[(0, 141)] = 1;
    g.hold_stock_id[(1, 141)] = 160;
    g.hold_stock_pct[(1, 141)] = 50;
    g.hold_stock_basis[(1, 141)] = -Ext::from_f64(2.5);
    let ev = holdings::pack_holding_arrays(&mut g);
    assert_eq!(ev, vec![holdings::PackEvent::ExcessLossRecaptured { owner_id: 141, corp_id: 160, amount: Ext::from(2.5) }]);
    assert_eq!(g.extraordinary_pl[141], 2); // (longlong)ROUND(0 + 2.5), half-even
    assert_eq!(g.hold_stock_basis[(1, 141)], Ext::from_f64(0.0));
}
