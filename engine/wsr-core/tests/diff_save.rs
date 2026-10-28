//! Differential tests of the save-game code against the original (tools/emu/game.py with its virtual
//! filesystem): SaveGame (0x4857af), EncodeDecodeSaveGameFile (0x5d212d, both directions), LoadGame
//! (0x48a1d4) and LoadCorpNames (0x42230e).
//!
//! RecalcShortMarginBalances (0x5e0ade) and RecalcCommodMarginBalances (0x61c8fc) run on both sides.

#![allow(clippy::field_reassign_with_default)]
use wsr_core::pb::real::{Ext, cast};


mod diff;

use diff::{Compare, Opts, run_original_with};
use wsr_core::pb::codec;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::save;
use wsr_core::state::Globals;

const SAVE_GAME: u32 = 0x4857af;
const LOAD_GAME: u32 = 0x48a1d4;
const ENCODE_DECODE: u32 = 0x5d212d;
const LOAD_CORP_NAMES: u32 = 0x42230e;
const SKIPS: [(u32, u32); 0] = [];


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
        g.player_short_margin_bal[p] = r.f(Ext::from_f64(1e4)).abs();
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

/// sample() plus per-seed variations of the parts SaveGame/LoadGame branch on.
fn state(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = sample(seed);
    let mut r = Rng(seed ^ 0xabcdef);
    g.data_path = st(["", "C:\\WSR\\", "D:\\Games\\W$R\\"][cast::<usize, _>(seed % 3)]);
    g.name_list_dirty = cast::<i16, _>(seed % 2);
    g.save_toggle = cast::<i32, _>(seed % 2);
    g.game_number = cast::<i32, _>(r.int(1, 50));
    for i in 1..=60 {
        if r.int(0, 3) == 0 {
            g.news_headline[i] = PbStr(format!("{__wsr_text_13754}{i}: commas, and {}", r.f(Ext::from(Ext::from_f64(1e6))),__wsr_text_13754=__content.string(13754)).into_bytes());
        }
    }
    if seed == 3 {
        // WRITE # of strings with quotes, commas and high bytes (such a file does not load back: see below)
        g.news_headline[7] = PbStr(b"He said \"sell\", then \xe9\xa3 \"".to_vec());
        g.sel_corp_name = st("A \"quoted\" corp, Inc.");
    }
    for i in 131..=200 {
        g.cash[i] = r.f(Ext::from_f64(1e9));
        g.market_cap[i] = r.f(Ext::from_f64(1e12)).abs();
    }
    g
}

fn show_line_diff(a: &[u8], b: &[u8]) -> String {
    let (x, y) = (codec::lines(a), codec::lines(b));
    let k = x.iter().zip(&y).position(|(p, q)| p != q).unwrap_or(x.len().min(y.len()));
    format!(
        "first difference at line {}: port {:?} vs original {:?} (lines {} / {})",
        k + 1,
        x.get(k).map(|l| String::from_utf8_lossy(l)),
        y.get(k).map(|l| String::from_utf8_lossy(l)),
        x.len(),
        y.len()
    )
}

#[track_caller]
fn assert_bytes(what: &str, port: &[u8], orig: &[u8]) {
    if port != orig {
        panic!("{what}: {}", show_line_diff(port, orig));
    }
}

fn file<'a>(o: &'a diff::Original, name: &str) -> &'a [u8] {
    o.files.get(name).and_then(|f| f.as_deref()).unwrap_or_else(|| panic!("{name} not written; files {:?}", o.files.keys()))
}

#[test]
fn save_game_matches_original() {
    for seed in 1u64..=4 {
        let g = state(seed);
        // Both sides start from the same RND state (futures pricing in RecalcCommodMarginBalances draws RND).
        let rnd0 = 0x1234_5678u32.wrapping_mul(cast::<u32, _>(seed) + 1);
        let opts = Opts { args: vec!["s:OLDGAME.DAT".into()], skips: SKIPS.to_vec(), rng: Some(rnd0), ..Opts::default() };
        let Some(o) = run_original_with(SAVE_GAME, &opts, &g) else { return };
        let mut port = g.clone();
        let mut rng = wsr_core::pb::rng::Rng::from_state(rnd0);
        let text = save::save_game(&mut port, &mut wsr_core::ui::Scripted::default(), &mut rng);
        assert_eq!(rng.state(), o.rng, "RND state after SaveGame");
        eprintln!("seed {seed}: {} bytes, {:.2}s; log {:?}", text.len(), o.elapsed, o.log);
        assert_bytes("OLDGAME.DAT", &text, file(&o, "OLDGAME.DAT"));
        assert_bytes("LASTGAME.DAT", &save::lastgame_dat(&port), file(&o, "LASTGAME.DAT"));
        // TickerPauseUntil: PauseTicker (UI) is not ported
        Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
    }
}

#[test]
fn large_integer_and_currency_save_items_match_the_original() {
    let mut g = sample(151);
    g.max_bond_issue = 9_007_199_254_740_995;
    g.max_stock_offering_amt = 900_719_925_474_099_500;
    g.merger_price_per_share = wsr_core::pb::numeric::Cux(900_719_925_474_099_500);
    g.bond_price[131] = wsr_core::pb::numeric::Cur(9_007_199_254_740_995);
    let opts = Opts { args: vec!["s:OLDGAME.DAT".into()], rng: Some(123456), ..Opts::default() };
    let Some(original) = run_original_with(SAVE_GAME, &opts, &g) else { return };
    let text = save::save_game(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::from_state(123456));
    assert_bytes("large numeric save items", &text, file(&original, "OLDGAME.DAT"));
    let start = fresh();
    let opts = Opts { args: vec!["s:OLDGAME.DAT".into()], files: vec![("OLDGAME.DAT".into(), text.clone())], ..Opts::default() };
    let Some(original) = run_original_with(LOAD_GAME, &opts, &start) else { return };
    let mut loaded = start;
    save::load_game(&mut loaded, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &text).unwrap();
    assert_eq!(loaded.max_bond_issue, original.globals.max_bond_issue);
    assert_eq!(loaded.max_stock_offering_amt, original.globals.max_stock_offering_amt);
    assert_eq!(loaded.merger_price_per_share, original.globals.merger_price_per_share);
    assert_eq!(loaded.bond_price[131], original.globals.bond_price[131]);
}

#[test]
fn encode_save_file_matches_original() {
    let __content = wsr_core::content::current();
    for seed in [5u64, 6] {
        let mut g = state(seed);
        let plain = save::save_game(&mut g, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
        let opts = Opts {
            args: vec!["2".into(), "s:GAME07.DAT".into()],
            files: vec![("OLDGAME.DAT".into(), plain.clone())],
            ..Opts::default()
        };
        let Some(o) = run_original_with(ENCODE_DECODE, &opts, &g) else { return };
        let enc = codec::encode_save(&plain, g.version_str.as_bytes());
        assert_bytes("GAME07.DAT", &enc, file(&o, "GAME07.DAT"));
        assert_eq!(o.files.get("OLDGAME.DAT"), Some(&None), "{__wsr_text_13755}",__wsr_text_13755=__content.string(13755));
        assert!(o.log.iter().any(|l| l.contains(__content.string(14465))), "{:?}", o.log);
    }
}

#[test]
fn decode_save_file_matches_original() {
    for seed in [7u64, 8] {
        let mut g = state(seed);
        g.news_headline[3] = PbStr(b"Gold hits \xa3 2,000 and \xa5 300,000".to_vec());
        let file_bytes = save::write_save_file(&mut g.clone(), &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
        let opts = Opts {
            args: vec!["1".into(), "s:GAME07.DAT".into()],
            files: vec![("GAME07.DAT".into(), file_bytes.clone())],
            ..Opts::default()
        };
        let Some(o) = run_original_with(ENCODE_DECODE, &opts, &g) else { return };
        let (plain, abort) = save::decode_save_file(&g, &file_bytes, false).unwrap();
        assert!(abort.is_none());
        assert_bytes("OLDGAME.DAT", &plain, file(&o, "OLDGAME.DAT"));
    }
}

#[test]
fn load_game_matches_original() {
    for seed in 9u64..=12 {
        let mut src = state(seed);
        let text = save::save_game(&mut src, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
        let mut start = fresh();
        start.data_path = src.data_path.clone();
        let opts = Opts {
            args: vec!["s:OLDGAME.DAT".into()],
            files: vec![("OLDGAME.DAT".into(), text.clone())],
            // unported routines the original calls during the load (see LoadInfo::skipped_calls)
            skips: vec![],
            hooks: vec![0x42230e, 0x63663c, 0x69c127],
            ..Opts::default()
        };
        let Some(o) = run_original_with(LOAD_GAME, &opts, &start) else { return };
        let mut port = start.clone();
        let info = match save::load_game(&mut port, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &text) {
            Ok(i) => i,
            Err(e) => panic!("port load failed: {e:?}; original log: {:?}", o.log),
        };
        if info.needs_corp_names {
            // the original calls LoadCorpNames, which reads the real CORPNAME.DAT
            let corp = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../extracted/app/CORPNAME.DAT")).unwrap();
            save::load_corp_names(&mut port, &corp).unwrap();
        }
        eprintln!("seed {seed}: {:.2}s, skipped {:?}, log {:?}", o.elapsed, info.skipped_calls, o.log);
        let orig_skips: Vec<&str> = o.log.iter().filter_map(|l| l.strip_prefix("SKIP ")).map(|l| l.split(' ').next().unwrap()).collect();
        let port_skips: Vec<&str> = info.skipped_calls.iter().map(|c| c.1).collect();
        assert_eq!(port_skips, orig_skips, "unported calls");
        let corp_calls = o.log.iter().filter(|l| l.starts_with("CALL LoadCorpNames")).count();
        assert_eq!(corp_calls, cast::<usize, _>(info.needs_corp_names), "LoadCorpNames calls");
        assert!(!o.log.iter().any(|l| l.starts_with("PBERR") || (l.starts_with("CALL ") && !l.starts_with("CALL LoadCorpNames"))), "{:?}", o.log);
        Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
    }
}

#[test]
fn load_legacy_commodity_units_matches_original() {
    let __content = wsr_core::content::current();
    for version in ["7.8", "7.9"] {
        let mut src = sample(781);
        src.version_str = st(version);
        // Legacy imports rebalance the banks: use their actual industry instead of sample()'s
        // deliberately arbitrary values (an industrial company in a bank slot hits PB error 9).
        for bank in 11..=85 { src.industry_num[bank] = 1; }
        for insurer in 86..=130 { src.industry_num[insurer] = 2; }
        src.futures_recs[0] = st("3");
        src.futures_recs[1] = st("GOLD|1|3|1|2027|20|10");
        src.futures_recs[2] = st("SILVER|1|-2|1|2027|20|10");
        src.futures_recs[3] = st("OIL|1|4|1|2027|20|10");
        src.futures_recs[7] = st("");
        src.phys_commod_rec[0] = st("3");
        src.phys_commod_rec[1] = st("GOLD|1|3|20");
        src.phys_commod_rec[2] = st("SILVER|1|2|20");
        src.phys_commod_rec[3] = st("BITCOIN|1|4|20");
        let text = save::save_game(&mut src, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
        let start = fresh();
        let rng0 = 123456;
        let opts = Opts {
            args: vec!["s:OLDGAME.DAT".into()], files: vec![("OLDGAME.DAT".into(), text.clone())],
            rng: Some(rng0), ..Opts::default()
        };
        let Some(o) = run_original_with(LOAD_GAME, &opts, &start) else { return };
        let mut port = start.clone();
        let mut rng = wsr_core::pb::rng::Rng::from_state(rng0);
        let info = save::load_game(&mut port, &mut wsr_core::ui::Scripted::default(), &mut rng, &text).unwrap();
        let comparison = Compare::default().ignore("TickerPauseUntil");
        if !comparison.diffs(&port, &o.globals).is_empty() {
            eprintln!("{__wsr_text_13756}{version}, original log: {:?}", o.log,__wsr_text_13756=__content.string(13756));
        }
        comparison.assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "{__wsr_text_13756}{version}",__wsr_text_13756=__content.string(13756));
        assert!(!info.skipped_calls.iter().any(|c| c.0 == 0x63663c));
        let units = if version == "7.8" { "30" } else { "3" };
        assert_eq!(port.phys_commod_rec[1], format!("GOLD|1|{units}|20").as_str());
        assert_eq!(port.phys_commod_rec[3], "BITCOIN|1|4|20");
    }
}

#[test]
fn load_corp_names_matches_original() {
    let mut g = fresh();
    g.ent_name[5] = st("stale");
    let Some(o) = run_original_with(LOAD_CORP_NAMES, &Opts::default(), &g) else { return };
    let corp = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../extracted/app/CORPNAME.DAT")).unwrap();
    let mut port = g.clone();
    save::load_corp_names(&mut port, &corp).unwrap();
    assert!(o.log.iter().any(|l| l.contains("CORPNAME.DAT")), "{:?}", o.log);
    assert!(!port.ent_name[200].is_empty());
    Compare::default().assert(&port, &o.globals);
}

/// WRITE # does not escape quotes, so a save with a quoted string desyncs INPUT # on load. Here the quoted
/// SelCorpName shifts the version field: both the original and the port reject the "version" with the
/// same MSGBOX text and reset the header fields.
#[test]
fn load_game_of_quoted_strings_fails_like_original() {
    let __content = wsr_core::content::current();
    let mut src = state(3);
    let text = save::save_game(&mut src, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default());
    let mut start = fresh();
    start.data_path = src.data_path.clone();
    let opts = Opts {
        args: vec!["s:OLDGAME.DAT".into()],
        files: vec![("OLDGAME.DAT".into(), text.clone())],
        skips: vec![],
        ..Opts::default()
    };
    let Some(o) = run_original_with(LOAD_GAME, &opts, &start) else { return };
    let mut port = start.clone();
    let Err(save::LoadError::VersionRejected { message }) = save::load_game(&mut port, &mut wsr_core::ui::Scripted::default(), &mut wsr_core::pb::rng::Rng::default(), &text) else {
        panic!("port did not reject the version")
    };
    let want = format!("{:?} type=0x2010", message.to_string());
    let want = want.replace('"', "'");
    assert!(o.log.iter().any(|l| l.starts_with("MSGBOX") && l.contains(&want)), "{want}{__wsr_text_13757}{:?}", o.log,__wsr_text_13757=__content.string(13757));
    Compare::default().ignore("TickerPauseUntil").assert(&port, &o.globals);
}
