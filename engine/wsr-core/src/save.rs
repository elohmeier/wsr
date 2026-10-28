//! Saved games: `SaveGame` (0x4857af), `LoadGame` (0x48a1d4), the `GAMExx.DAT` codec wrapper
//! `EncodeDecodeSaveGameFile` (0x5d212d) and the state handling of its callers `SaveGamePrompt`
//! (0x40c01b) and `MenuOpenSavedGame` (0x40ae7f).
//!
//! Flow in the original:
//! * save: `SaveGamePrompt` → `SaveGame("OLDGAME.DAT")` writes the plain `WRITE #` text → `EncodeDecodeSaveGameFile(2,
//!   "GAMExx.DAT")` encodes it line by line (header `@EC<VersionStr>`) and deletes `OLDGAME.DAT`; `SaveGame` also
//!   writes `LASTGAME.DAT` (= `WRITE #, GameNumber`).
//! * load: `MenuOpenSavedGame` → `EncodeDecodeSaveGameFile(1, "GAMExx.DAT")` decodes into `OLDGAME.DAT` →
//!   `LoadGame("OLDGAME.DAT")` → post-load fix-ups ([`open_saved_game_fixups`]).
//!
//! Every statement below is commented with the original's order; the item types follow the disassembly
//! (`fild word/dword/qword`, `fld dword/xword`, `PB_CurLoad`/`PB_CuxLoad` for WRITE items; `fistp`/`fstp` widths for
//! INPUT targets). All version comparisons in both routines are SINGLE vs SINGLE (`version` is a SINGLE local
//! and the constants are SINGLE), except `4.79 < VAL(SavedGameVersion)` (EXT vs SINGLE constant).

// Statements keep the original BASIC shape (`x = x + y`, `a < 1 OR 5 < a`) on purpose.
#![allow(clippy::manual_range_contains, clippy::assign_op_pattern)]
use crate::pb::real::{Ext, cast};


use crate::logic::holdings::{self, PackEvent};
use crate::pb::codec;
use crate::pb::fileio::OutFile;
use crate::pb::numeric::{self, InputReader};
use crate::pb::string;
use crate::pb::pbstr::PbStr;
use crate::pb::numeric::*;
use crate::pb_write;
use crate::pb::rng::Rng;
use crate::state::Globals;
use crate::ui::Ui;

// ---------------------------------------------------------------------------------------------------------
// Small adapters over the runtime helpers in pb::string / pb::numeric.
// ---------------------------------------------------------------------------------------------------------

/// `VAL(s)` (0x6bd6bb), retaining the extended result.
fn val(s: &PbStr) -> Ext {
    numeric::val(s.as_bytes())
}

/// `VAL(s)` stored directly to a SINGLE (`fstp dword`).
fn val_single(s: &PbStr) -> f32 {
    cast::<f32, _>(numeric::val(s.as_bytes()))
}

/// `PARSE$(s, ANY "|", n)`.
fn parse_bar(s: &PbStr, n: i32) -> PbStr {
    string::parse_any(s.as_bytes(), b"|", n)
}

/// `RTRIM$(LTRIM$(s))`.
fn trim(s: &PbStr) -> PbStr {
    string::rtrim(string::ltrim(s.as_bytes()).as_bytes())
}

/// `STR$(n)` of a LONG (leading blank for n >= 0).
fn str_num(n: i64) -> PbStr {
    PbStr::from(numeric::str_num(cast::<Ext, _>(n), 15).as_str())
}

/// `RTRIM$(LTRIM$(STR$(n)))`.
fn str_trim(n: i64) -> PbStr {
    trim(&str_num(n))
}

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

// ---------------------------------------------------------------------------------------------------------
// SaveGame (0x4857af)
// ---------------------------------------------------------------------------------------------------------

/// Side information from `save_game` (in call order).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SaveInfo {
    /// Recaptures whose news was posted by `PackHoldingArrays` → `RecaptureExcessLoss`.
    pub pack_events: Vec<PackEvent>,
}

/// `SaveGame(fileName)` (0x4857af): the plain `OLDGAME.DAT` text. See [`save_game_info`].
pub fn save_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> Vec<u8> {
    save_game_info(g, ui, rng).0
}

/// `SaveGame` returning the side information as well.
///
/// Side effects like the original: holdings of non-active entities cleared and all holding arrays packed,
/// `SaveToggle` flipped, `SaveExtraStr(1..2)` and `SaveMiscQuad(1..10)` refreshed, and the temporary
/// `Cash(p) += PlayerShortMarginBal(p)` adjustment for players 1..5 (undone after the item is written —
/// `Cash` ends as `(c + m) - m`, which may differ from `c` in the last bit, as in the original).
///
/// Skipped UI: `CONTROL SET TEXT` of the ticker label (id 123) with `TickerText`.
/// `LASTGAME.DAT` is produced by [`lastgame_dat`]. A runtime error in the
/// original jumps to 0x48a10f (ShowRuntimeError, file left incomplete); out-of-range accesses here use the
/// `Arr` dummy element instead.
pub fn save_game_info(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> (Vec<u8>, SaveInfo) {
    let mut info = SaveInfo::default();
    // clearMode = 2: ClearAllHoldings(2); PackHoldingArrays
    holdings::clear_all_holdings(g, 2);
    info.pack_events = holdings::pack_holding_arrays(g);
    holdings::post_pack_events(g, ui, info.pack_events.clone());
    crate::logic::shorts::recalc_short_margin_balances(g); // 0x5e0ade
    // clearMode = 0: RecalcCommodMarginBalances(0) (0x61c8fc)
    crate::logic::futures::recalc_commod_margin_balances(g, ui, rng, 0);
    crate::logic::turn::pause_ticker(g, &ui.local_time(), 0.0f32);
    let version: f32 = cast::<f32, _>(val_single(&g.version_str)); // fstp dword [ebp-0x90]
    let unused_float: f32 = cast::<f32, _>(Ext::from_f32(0.0f32)); // local SINGLE, never assigned (PB zero-initialises locals)
    let mut margin_bal: Ext = Ext::from_f64(0.0); // local EXT

    let mut f = OutFile::default();
    let f = &mut f;
    // SaveToggle = (SaveToggle = 0)  → 1 / 0 (0x485884)
    g.save_toggle = if g.save_toggle == 0 { 1 } else { 0 };
    pb_write!(f, g.turn_action_count);
    pb_write!(f, g.save_toggle);
    pb_write!(f, g.active_entity);
    pb_write!(f, &g.active_entity_name);
    pb_write!(f, &g.sel_corp_name);
    pb_write!(f, g.sel_entity);
    pb_write!(f, &g.version_str);
    pb_write!(f, g.long_game_marker);
    pb_write!(f, g.asbestos_corp);
    pb_write!(f, &g.asset_sale_desc);
    pb_write!(f, g.antitrust_defendant);
    pb_write!(f, g.antitrust_market_share);
    pb_write!(f, g.antitrust_plaintiff);
    pb_write!(f, g.entity_borrow_rate);
    pb_write!(f, g.bank_loans_total);
    pb_write!(f, g.max_bond_issue);
    pb_write!(f, g.long_bond_price);
    pb_write!(f, g.bubble_state);
    pb_write!(f, g.cap_gains_tax_rate);
    pb_write!(f, g.bank_cd_rate);
    pb_write!(f, &g.central_bank_name);
    pb_write!(f, g.scenario_corp);
    pb_write!(f, g.event_player);
    pb_write!(f, g.save_unused24);
    pb_write!(f, &g.cheat_mode);
    pb_write!(f, g.adventure_cooldown);
    pb_write!(f, g.sim_co_count);
    pb_write!(f, &g.has_cpu_players);
    pb_write!(f, g.econ_scenario_id);
    pb_write!(f, g.exchange_rate);
    pb_write!(f, &g.currency_name);
    pb_write!(f, g.antitrust_damages);
    pb_write!(f, g.difficulty_level);
    pb_write!(f, &g.currency_symbol);
    pb_write!(f, g.econ_cycle_counter);
    pb_write!(f, g.cheat_eq_fund_target);
    pb_write!(f, &g.currency_suffix);
    pb_write!(f, &g.recent_mgmt_change_list);
    pb_write!(f, g.game_end_year as f64 - g.start_year as f64);
    pb_write!(f, g.gdp_level);
    pb_write!(f, g.gdp_growth);
    pb_write!(f, g.cheat_japan_target);
    pb_write!(f, g.malicious_suit_timer);
    pb_write!(f, g.malicious_suit_defendant);
    pb_write!(f, g.malicious_suit_plaintiff);
    pb_write!(f, g.housing_starts);
    pb_write!(f, g.stock_index_divisor);
    pb_write!(f, g.prev_extra_div_corp);
    pb_write!(f, g.sofr_rate);
    pb_write!(f, g.monetary_stance);
    pb_write!(f, &g.new_mgmt_list);
    pb_write!(f, g.merger_proposer_player);
    pb_write!(f, g.merger_acquirer);
    pb_write!(f, g.merger_target);
    pb_write!(f, g.merger_price_per_share); // PB_CuxLoad (0x485f80)
    pb_write!(f, g.next_takeover_tick);
    pb_write!(f, &g.million_prefix_lc);
    pb_write!(f, &g.million_prefix_uc);
    pb_write!(f, g.world_event_state);
    pb_write!(f, g.monetary_policy);
    pb_write!(f, g.world_generated);
    pb_write!(f, g.name_list_dirty);
    pb_write!(f, g.turnaround_pick_corp);
    pb_write!(f, &g.save_unused_str64);
    pb_write!(f, g.story_events111to120_done);
    pb_write!(f, g.scenario_stage);
    pb_write!(f, g.oil_price);
    pb_write!(f, g.peak_oil_crisis);
    pb_write!(f, g.oil_crisis_level);
    pb_write!(f, g.oil_crisis_stage);
    pb_write!(f, g.cur_player);
    pb_write!(f, &g.cur_player_name);
    pb_write!(f, g.num_players);
    pb_write!(f, g.base_interest_rate);
    pb_write!(f, g.hot_stock_corp);
    pb_write!(f, &g.exe_name);
    pb_write!(f, g.cur_quarter);
    pb_write!(f, g.adv_offering_corp);
    pb_write!(f, &g.currency_code);
    pb_write!(f, g.adv_restruct_corp);
    pb_write!(f, g.game_end_year);
    pb_write!(f, g.rumor_victim_id);
    pb_write!(f, g.libel_suit_defendant);
    pb_write!(f, &g.short_bond_desc);
    pb_write!(f, g.short_bond_yield);
    pb_write!(f, g.short_bond_price);
    pb_write!(f, g.scenario_num);
    pb_write!(f, &g.scenario_tag);
    pb_write!(f, g.rand_seed);
    pb_write!(f, &g.lookup_entity_name);
    pb_write!(f, g.target_corp);
    pb_write!(f, g.sel_industry);
    pb_write!(f, g.antitrust_settle_amt);
    pb_write!(f, g.prime_rate);
    pb_write!(f, g.stock_index);
    pb_write!(f, g.market_valuation);
    pb_write!(f, g.max_stock_offering_amt);
    pb_write!(f, g.strike_corp_code);
    pb_write!(f, g.last_extra_div_corp);
    pb_write!(f, g.superfund_end_year);
    pb_write!(f, g.superfund_corp);
    pb_write!(f, g.start_year as f64 - 2002.0); // fild word 2002; fild dword StartYear; fsubrp
    pb_write!(f, g.gdp_growth_target);
    pb_write!(f, g.interest_rate_target);
    pb_write!(f, g.oil_target_price);
    pb_write!(f, &g.long_bond_desc);
    pb_write!(f, g.long_bond_yield);
    pb_write!(f, &g.ticker_text);
    pb_write!(f, g.ticker_speed);
    pb_write!(f, g.takeover_candidate_id);
    pb_write!(f, g.corp_tax_rate);
    pb_write!(f, unused_float); // fld dword [ebp-0xa8]
    pb_write!(f, &g.app_title);
    pb_write!(f, g.cur_year as f64 - 2003.0);
    pb_write!(f, g.game_end_year as f64 - 1806.0);

    // per-player settings and streaming-quote watch lists (0x48672c)
    let loop_end = g.num_players;
    for i in 1..=loop_end {
        pb_write!(
            f,
            g.player_select_last_corp[i],
            g.player_last_stock[i],
            g.player_suppress_popups[i],
            g.player_suppress_earnings[i],
            g.player_auto_save[i]
        );
        if 5.01f32 <= version {
            pb_write!(f, g.player_auto_add_stream[i]);
        }
        if 9.1f32 <= version {
            pb_write!(f, g.player_suppress_cash_warn[i]);
        }
        pb_write!(f, g.watch_list[(0, i)]);
        if g.watch_list[(0, i)] != 0 {
            for j in 1..=15 {
                pb_write!(
                    f,
                    g.watch_list[(j, i)],
                    &g.stream_prev_price_str[(j, i)],
                    &g.stream_cur_price[(j, i)],
                    g.stream_price_dir[(j, i)]
                );
            }
        }
    }
    let loop_end = g.num_players;
    for i in 1..=loop_end {
        pb_write!(f, g.player_ceo_of[i], &g.ent_name[i], &g.ent_symbol[i]);
        pb_write!(f, g.player_prepaid_tax[i]);
    }
    for i in 1..=60 {
        pb_write!(f, &g.news_headline[i]);
    }
    let loop_end = g.num_players;
    for i in 1..=loop_end {
        pb_write!(
            f,
            g.player_cap_gains[i],
            g.player_notoriety[i],
            g.player_law_firm[i],
            &g.player_name[i],
            g.player_turn_order[i],
            g.player_startup_count[i]
        );
    }
    for i in 1..=10 {
        pb_write!(f, g.recent_antitrust_filers[i], g.recent_offerings[i], &g.lawsuit_rec[i]);
    }
    for i in 0..=55 {
        pb_write!(f, &g.country_name[i], &g.country_display_name[i]);
    }
    for i in 0..=70 {
        pb_write!(f, &g.ind_name[i]);
    }
    for i in 1..=70 {
        pb_write!(f, g.ind_demand[i]);
    }
    for i in 1..=200 {
        pb_write!(f, g.event_status[i]);
    }
    for i in 11..=1600 {
        pb_write!(f, g.country_idx[i], g.corp_prior_net_worth[i]);
    }
    for i in 1..=70 {
        pb_write!(
            f,
            g.ind_growth_rate[i],
            g.ind_growth_rate_now[i],
            g.ind_long_term_growth[i],
            g.ind_volatility[i],
            g.ind_growth_forecast[i]
        );
    }
    for i in 1..=100 {
        pb_write!(f, g.event_state[i]);
    }
    for i in 11..=85 {
        pb_write!(f, g.bank_demand_deposits[i], g.bad_debt_reserve[i], g.bank_consumer_loans[i], g.bank_mortgage_loans[i]);
        pb_write!(f, g.bank_c_ds[i], g.bank_cust_deposits[i]);
        pb_write!(f, g.bank_largest_debtor[i]);
    }
    for i in 1..=1600 {
        pb_write!(f, &g.ent_misc_data[i], g.entity_status[i]);
        pb_write!(f, g.credit_frozen[i]);
    }
    for i in 1..=130 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.long_gov_bond_face[i], g.lt_gov_bond_cost[i], g.short_gov_bond_face[i], g.st_gov_bond_cost[i]);
        }
    }
    for i in 86..=130 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.ins_reserves[i]);
            pb_write!(f, g.insur_uw_profit_ytd[i]);
        }
    }
    // cash: players' short-sale margin balance is folded into Cash for the file (0x487391)
    for i in 1..=1600 {
        pb_write!(f, g.lending_bank[i]);
        if g.entity_status[i] == 1 {
            if i < 6 {
                g.cash[i] = g.cash[i] + g.player_short_margin_bal[i];
                margin_bal = g.player_short_margin_bal[i];
                g.player_short_margin_bal[i] = Ext::from_f64(0.0);
            }
            pb_write!(f, g.cash[i], g.corp_controller[i]);
            if i < 6 {
                g.cash[i] = g.cash[i] - margin_bal;
                g.player_short_margin_bal[i] = margin_bal;
            }
            pb_write!(f, g.accrued_pl[i], g.bank_debt[i]);
            pb_write!(f, g.div_recvd1[i], g.div_recvd2[i]);
            pb_write!(f, g.earnings_ytd[i]);
            pb_write!(f, g.extraordinary_pl[i]);
            pb_write!(f, g.credit_rating_num[i]);
        }
    }
    for i in 11..=1600 {
        pb_write!(f, g.industry_num[i]);
        if g.entity_status[i] == 1 {
            pb_write!(
                f,
                g.dividend_per_share[i],
                g.market_cap[i],
                g.profit_margin[i],
                g.bonds_issued[i],
                &g.bond_issue_info[i]
            );
            pb_write!(f, g.bond_price[i], g.tax_loss_carry[i], g.equity_income_ytd[i]); // PB_CurLoad
            pb_write!(f, g.earnings_y1[i], g.disc_spend_pct[i], g.shares_outstanding[i]);
            pb_write!(f, g.earnings_yr2_ago[i], g.earnings_yr3_ago[i], g.earnings_yr4_ago[i]);
            pb_write!(f, g.investor_sentiment[i]);
            pb_write!(f, g.proj_qtr_earnings[i]);
            pb_write!(f, g.ctrl_since_year[i]);
        }
    }
    for i in 86..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.growth_rate[i]);
        }
    }
    for i in 131..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.business_assets[i], g.working_capital[i], g.goodwill[i]);
        }
    }
    for i in 1..=130 {
        if g.entity_status[i] == 1 {
            write_bond_holdings(f, g, i);
        }
    }
    for i in 1..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.hold_stock_id[(0, i)]);
            if g.hold_stock_id[(0, i)] != 0 {
                let count = cast::<i32, _>(g.hold_stock_id[(0, i)]);
                for j in 1..=count {
                    pb_write!(f, g.hold_stock_id[(j, i)], g.hold_stock_pct[(j, i)]);
                    pb_write!(f, g.hold_stock_basis[(j, i)]);
                }
            }
        }
    }
    let loop_end = g.num_players;
    for i in 1..=loop_end {
        for j in 11..=1600 {
            pb_write!(f, g.player_advances[(i, j)]);
        }
    }
    for i in 11..=1600 {
        pb_write!(f, g.extra_items_ytd[i], g.consol_tax_inc_ytd[i]);
    }
    for i in 1..=5 {
        pb_write!(f, g.player_recent_bond_issues[i]);
    }
    g.save_extra_str[1] = g.market_cycle_plan.clone();
    g.save_extra_str[2] = g.scenario_headline.clone();
    for i in 1..=5 {
        pb_write!(f, &g.save_extra_str[i], g.player_exercise_options[i]);
    }
    pb_write!(f, &g.option_recs[0]);
    let j = to_long(val(&g.option_recs[0]));
    if 0 < j {
        for i in 1..=j {
            pb_write!(f, &g.option_recs[i]);
        }
    }
    for i in 1..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.has_options_outstanding[i]);
        }
    }
    for i in 11..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.cap_hist_close[(0, i)]);
            if g.cap_hist_close[(0, i)] != Ext::from_f64(0.0) {
                write_price_history(f, g, i);
            }
        }
    }
    for i in 11..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.reserve_adj_pending[i]);
            if 85 < i {
                pb_write!(f, g.contingency_reserve[i]);
            }
        }
    }
    pb_write!(f, g.wealth_tax_rate);
    pb_write!(f, g.carbon_tax_rate);
    pb_write!(f, g.oil_windfall_tax_rate);
    pb_write!(f, g.holding_co_tax_rate);
    pb_write!(f, g.corp_shares_tax_country);
    pb_write!(f, g.health_care_tax_rate);
    for i in 1..=5 {
        pb_write!(f, g.player_ceo_since_year[i]);
    }
    pb_write!(f, g.gold_price, g.gold_target_price, g.gold_price_min, g.gold_price_max);
    pb_write!(f, g.silver_price, g.silver_target_price, g.silver_price_min, g.silver_price_max);
    pb_write!(f, g.wheat_price, g.wheat_target_price, g.wheat_price_min, g.wheat_price_max);
    pb_write!(f, g.corn_price, g.corn_target_price, g.corn_price_min, g.corn_price_max);
    for i in 1..=10 {
        pb_write!(f, g.futures_basis[i]);
    }
    // futures contracts, each followed by the margin balance of the account named in field 2
    pb_write!(f, &g.futures_recs[0]);
    if Ext::from_f64(0.0) < val(&g.futures_recs[0]) {
        let count = to_long(val(&g.futures_recs[0]));
        for j in 1..=count {
            pb_write!(f, &g.futures_recs[j]);
            let field = g.futures_recs[j].clone();
            let i = to_long(val(&parse_bar(&field, 2)));
            margin_bal = g.commod_margin_bal[i];
            pb_write!(f, margin_bal);
        }
    }
    pb_write!(f, &g.swap_contracts[0]);
    if Ext::from_f64(0.0) < val(&g.swap_contracts[0]) {
        let count = to_long(val(&g.swap_contracts[0]));
        for j in 1..=count {
            pb_write!(f, &g.swap_contracts[j]);
        }
    }
    pb_write!(f, g.covered_call_fund_id, &g.recent_dividend_change_list);
    // price histories of the indexes/players: i = 0, 1..5 (active players only), 6..10, then 1601..1605
    // (i = 11 jumps to 1601) (0x4888f8)
    let mut i = 0;
    loop {
        if i < 1 || 5 < i || g.entity_status[i] == 1 {
            if i == 11 {
                i = 1601;
            }
            pb_write!(f, g.cap_hist_close[(0, i)]);
            write_price_history(f, g, i);
        }
        i += 1;
        if 1605 < i {
            break;
        }
    }
    for i in 11..=1600 {
        if g.entity_status[i] == 1 {
            pb_write!(f, g.tax_credits[i]);
        }
    }
    pb_write!(f, g.fad_industry, g.fad_valuation_pct);
    for i in 1586..=1600 {
        pb_write!(f, g.fund_adviser[i], g.fund_fee_pct[i]);
    }
    pb_write!(f, &g.commod_trade_mode, g.start_net_worth);
    pb_write!(f, &g.phys_commod_rec[0]);
    if Ext::from_f64(0.0) < val(&g.phys_commod_rec[0]) {
        let count = to_long(val(&g.phys_commod_rec[0]));
        for j in 1..=count {
            pb_write!(f, &g.phys_commod_rec[j]);
        }
    }
    for i in 101..=200 {
        pb_write!(f, g.event_state[i]);
    }
    pb_write!(f, &g.asset_sale_offers[0]);
    let j = to_long(val(&g.asset_sale_offers[0]));
    if 0 < j {
        for i in 1..=j {
            pb_write!(f, &g.asset_sale_offers[i]);
        }
    }
    for i in 3..=69 {
        pb_write!(f, g.ind_demand_gap[i]);
    }
    pb_write!(f, g.cur_make_delivery, g.cur_take_delivery);
    for i in 1..=5 {
        pb_write!(f, g.player_make_delivery[i], g.player_take_delivery[i]);
    }
    if 7.5f32 <= version {
        pb_write!(
            f,
            g.loan_limit_multiple,
            g.insider_probe_player,
            g.merger_tip_corp,
            g.insider_earn_tip_corp,
            g.insider_tippee,
            g.insider_tip_event_tick,
            g.insider_tip_event_qtr,
            g.tip_industry
        );
        for i in 1..=5 {
            pb_write!(f, g.tipster_state[i], g.auto_pilot[i]);
        }
    }
    pb_write!(f, g.subprime_crisis_state, g.market_val_target);
    for i in 11..=130 {
        pb_write!(f, g.subprime_secs[i]);
    }
    for i in 131..=1600 {
        pb_write!(f, g.corp_hedging_gain[i]);
    }
    // SaveMiscQuad(1..10): (longlong)ROUND(x) of LONG/INTEGER globals
    g.save_misc_quad[1] = g.stock_index_base as Quad;
    g.save_misc_quad[2] = g.next_market_cycle_year as Quad;
    if 7.62f32 <= version {
        g.save_misc_quad[3] = g.auto_save_state as Quad;
    }
    g.save_misc_quad[4] = g.ai_hostile_merger_qtr as Quad;
    g.save_misc_quad[5] = g.bank_merger_player as Quad;
    g.save_misc_quad[6] = g.pandemic_state as Quad;
    g.save_misc_quad[7] = g.metals_price_base as Quad;
    g.save_misc_quad[8] = g.crypto_ban_state as Quad;
    g.save_misc_quad[9] = g.private_antitrust_off as Quad;
    g.save_misc_quad[10] = g.govt_antitrust_off as Quad;
    for i in 1..=10 {
        pb_write!(f, g.save_misc_quad[i]);
    }
    for i in 1..=5 {
        pb_write!(f, g.is_computer_player[i]);
        pb_write!(f, g.player_last_active_entity[i]);
        pb_write!(f, g.player_corps_controlled[i]);
    }
    pb_write!(f, g.cpu_max_corps);
    for i in 1..=5 {
        pb_write!(f, g.player_antitrust_level[i]);
    }
    for i in 11..=1600 {
        pb_write!(f, g.auto_pilot[i]);
    }
    pb_write!(f, g.cur_sweep_cash);
    let loop_end = g.num_players;
    for i in 1..=loop_end {
        pb_write!(f, g.sweep_cash_to_loan[i]);
    }
    if 7.83f32 < version {
        let loop_end = g.num_players;
        for i in 1..=loop_end {
            pb_write!(f, g.player_cheat_code[i]);
        }
    }
    if 7.92f32 < version {
        pb_write!(f, &g.price_alerts[0]);
        if Ext::from_f64(0.0) < val(&g.price_alerts[0]) {
            let loop_end = to_long(val(&g.price_alerts[0]));
            for i in 1..=loop_end {
                pb_write!(f, &g.price_alerts[i]);
            }
        }
        for i in 1..=50 {
            pb_write!(f, g.asset_buy_watch_list[i]);
        }
    }
    if 8.2f32 < version {
        for i in 1..=1600 {
            if g.entity_status[i] == 1 {
                pb_write!(f, g.t_bills[i]);
            }
        }
    }
    if 8.21f32 < version {
        for i in 11..=85 {
            pb_write!(f, g.bank_alloc_prime_mort_pct[i]);
            pb_write!(f, g.bank_alloc_subprime_pct[i]);
            pb_write!(f, g.bank_alloc_consumer_pct[i]);
            pb_write!(f, g.bank_alloc_bonds_pct[i]);
            pb_write!(f, g.bank_alloc_cash_pct[i]);
        }
    }
    if 8.25f32 < version {
        for i in 11..=85 {
            if g.entity_status[i] == 1 {
                pb_write!(f, g.growth_rate[i]);
            }
        }
    }
    if 8.5f32 < version {
        for i in 11..=85 {
            if g.entity_status[i] == 1 {
                pb_write!(f, g.bank_alloc_off[i]);
            }
        }
    }
    if 8.57f32 < version {
        for i in 1586..=1600 {
            if g.entity_status[i] == 1 {
                pb_write!(f, g.fund_bonus_year[i]);
            }
        }
    }
    if 8.6f32 <= version {
        let loop_end = g.num_players;
        for i in 1..=loop_end {
            pb_write!(f, g.player_growth_throttle[i]);
        }
    }
    if 8.72f32 < version {
        // bond holdings of funds 1582..1583 (written regardless of their status)
        for i in 1582..=1583 {
            write_bond_holdings(f, g, i);
        }
        if g.entity_status[1581] == 1 {
            pb_write!(
                f,
                g.long_gov_bond_face[1581],
                g.lt_gov_bond_cost[1581],
                g.short_gov_bond_face[1581],
                g.st_gov_bond_cost[1581]
            );
        }
        for i in 1581..=1585 {
            pb_write!(f, g.fund_adviser[i], g.fund_fee_pct[i], g.fund_bonus_year[i]);
        }
    }
    if 9.11f32 <= version {
        pb_write!(
            f,
            g.bitcoin_price,
            g.bitcoin_target_price,
            g.ethereum_price,
            g.ethereum_target_price,
            g.crypto_boom_years
        );
        let i = 1606;
        pb_write!(f, g.cap_hist_close[(0, 1606)]);
        write_price_history(f, g, i);
    }
    if 9.75f32 < version {
        let count = g.num_players;
        for j in 1..=count {
            pb_write!(f, g.player_loan_offers_avail[j]);
        }
    }
    // corporate names list as PRINT # lines "iiii|cc|SSSSS|  name |" (0x489dd4)
    if g.name_list_dirty == 1 {
        for i in 11..=1600 {
            let field = s("00") + &str_trim(cast::<i64, _>(i));
            let mut line_text = string::right(field.as_bytes(), 4) + "|";
            let mut field = s("00");
            if 0 < g.country_idx[i] {
                field = field + &str_trim(cast::<i64, _>(g.country_idx[i]));
            }
            line_text = line_text + &string::right(field.as_bytes(), 2) + "|";
            let field = g.ent_symbol[i].clone() + "    ";
            line_text = line_text + &string::mid(field.as_bytes(), 1, 5) + "|";
            let field = s("  ") + &g.ent_name[i] + " |";
            line_text = line_text + &field;
            f.print(&line_text);
        }
    }
    // (UI) CONTROL SET TEXT GetDlgItem(hDlgMain, 123), TickerText
    // CLOSE; LASTGAME.DAT ← WRITE GameNumber (see lastgame_dat)
    (std::mem::take(&mut f.data), info)
}

/// The bond-holding block `HoldBondId(0, i)` + `count × (id, face, cost)`.
fn write_bond_holdings(f: &mut OutFile, g: &Globals, i: i32) {
    pb_write!(f, g.hold_bond_id[(0, i)]);
    if g.hold_bond_id[(0, i)] != 0 {
        let count = g.hold_bond_id[(0, i)];
        for j in 1..=count {
            pb_write!(f, g.hold_bond_id[(j, i)], g.hold_bond_face[(j, i)], g.bond_hold_cost[(j, i)]);
        }
    }
}

/// `FOR j = 1 TO 60: WRITE PriceHistHigh(j, i), CapHistClose(j, i), PriceHistLow(j, i)`.
fn write_price_history(f: &mut OutFile, g: &Globals, i: i32) {
    for j in 1..=60 {
        pb_write!(f, g.price_hist_high[(j, i)], g.cap_hist_close[(j, i)], g.price_hist_low[(j, i)]);
    }
}



/// `LASTGAME.DAT` as written at the end of SaveGame (and by MenuOpenSavedGame): `WRITE #, GameNumber`.
pub fn lastgame_dat(g: &Globals) -> Vec<u8> {
    let mut f = OutFile::default();
    pb_write!(f, g.game_number);
    f.data
}

// ---------------------------------------------------------------------------------------------------------
// LoadGame (0x48a1d4)
// ---------------------------------------------------------------------------------------------------------

/// Text of the `ShowQuickMessage` LoadGame shows for saves older than 8.25 (0x48d… "version <= 8.21").
#[allow(non_snake_case)]
pub fn OLD_BANK_ALLOC_NOTICE(__content: &crate::content::ContentPack) -> &str { __content.string(13520) }

/// Result of a successful [`load_game`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LoadInfo {
    /// `version` local at the end: `VAL(SavedGameVersion)` as SINGLE.
    pub version: f32,
    /// `NameListDirty = 0`: the original now calls `LoadCorpNames` (0x42230e), which reads the corp names,
    /// symbols and countries from `CORPNAME.DAT`. Call [`load_corp_names`] with that file.
    pub needs_corp_names: bool,
    /// Texts the original shows with `ShowQuickMessage` (non-modal notices).
    pub notices: Vec<PbStr>,
    /// Unported routines the original calls during the load, in order: (address, name).
    pub skipped_calls: Vec<(u32, &'static str)>,
}

/// Why a load failed.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadError {
    /// LoadGame rejected the save version (MSGBOX style 0x2010 with this text). `GameEndFlag` = "Y", the
    /// loaded header fields are reset (TurnActionCount, ActiveEntity, ActiveEntityName, SelCorpName,
    /// SelEntity, SavedGameVersion), like the original.
    VersionRejected { message: PbStr },
    /// `EncodeDecodeSaveGameFile` refused the file (no `@EC` header or header version < 3):
    /// `ShowQuickMessage(" [runtime resource wording] " + fileName + " [runtime resource wording]  than v. "
    /// + VersionStr + "....")`. `file_version` is the text after `@EC` ("" without header).
    OldFileFormat { file_version: PbStr, version_str: PbStr },
    /// A PowerBASIC runtime error inside LoadGame (62 = input past end, 9 = subscript out of range). The
    /// original's handler (0x48eb06) sets `GameEndFlag = "Y"`, shows ShowRuntimeError (0x46ea48) and ends the
    /// main dialog (the program exits). State is partially loaded.
    Runtime { err: i32 },
    /// A guarded host rejected unsafe financial storage or references before publishing the load.
    InvalidState { message: String },
}

impl LoadError {
    /// The message the original shows for [`LoadError::OldFileFormat`], given the file name it was opened as.
    pub fn old_format_message(&self, file_name: &PbStr) -> Option<PbStr> {
    let __content = crate::content::current();
        match self {
            LoadError::OldFileFormat { version_str, .. } => Some(
                s(__content.string(13521)) + file_name + __content.string(13522) + version_str + "....",
            ),
            _ => None,
        }
    }
}

/// INPUT # reader with the runtime's error 62 ("input past end").
struct Rd<'a> {
    f: InputReader<'a>,
}

fn err62(e: u8) -> LoadError {
    LoadError::Runtime { err: cast::<i32, _>(e) }
}

impl<'a> Rd<'a> {
    fn new(data: &'a [u8]) -> Self {
        Rd { f: InputReader::new(data) }
    }
    /// `INPUT #f, x` (value on the FPU stack).
    fn num(&mut self) -> Result<Ext, LoadError> {
        self.f.try_input_num().map_err(err62)
    }
    fn exact(&mut self) -> Result<crate::pb::ext80::Ext80, LoadError> {
        self.f.try_input_num_ext().map_err(err62)
    }
    /// `fistp dword`
    fn long(&mut self) -> Result<Long, LoadError> {
        Ok(self.exact()?.to_i32())
    }
    /// `fistp word`
    fn int(&mut self) -> Result<Integer, LoadError> {
        let n = self.exact()?.to_i64();
        Ok(i16::try_from(n).unwrap_or(i16::MIN))
    }
    /// `fistp qword`
    fn quad(&mut self) -> Result<Quad, LoadError> {
        Ok(self.exact()?.to_i64())
    }
    /// `fstp dword`
    fn single(&mut self) -> Result<Single, LoadError> {
        Ok(self.exact()?.to_f32())
    }
    /// `fstp xword` / `fstp qword` (EXT / DOUBLE)
    fn ext(&mut self) -> Result<Ext, LoadError> {
        self.num()
    }
    /// `PB_CurStore`
    fn cur(&mut self) -> Result<Cur, LoadError> {
        let mut value = Cur::default();
        value.set_ext(self.exact()?);
        Ok(value)
    }
    /// `INPUT #f, s$`
    fn str(&mut self) -> Result<PbStr, LoadError> {
        self.f.try_input_str().map(PbStr).map_err(err62)
    }
    /// `LINE INPUT #f, s$`
    fn line(&mut self) -> Result<PbStr, LoadError> {
        self.f.try_line_input().map(PbStr).map_err(err62)
    }
}

/// Sum of the out-of-range access counters of every array LoadGame indexes with file-controlled subscripts
/// (counts, ids, NumPlayers): a change means the original would have raised error 9.
fn oob_total(g: &Globals) -> u64 {
    let mut n = 0u64;
    macro_rules! add {
        ($($a:ident),*) => { $( n += g.$a.oob_count() as u64; )* };
    }
    add!(
        player_select_last_corp, player_last_stock, player_suppress_popups, player_suppress_earnings, player_auto_save,
        player_auto_add_stream, player_suppress_cash_warn, watch_list, stream_prev_price_str, stream_cur_price,
        stream_price_dir, player_ceo_of, ent_name, ent_symbol, player_prepaid_tax, player_cap_gains, player_notoriety,
        player_law_firm, player_name, player_turn_order, player_startup_count, hold_bond_id, hold_bond_face,
        bond_hold_cost, hold_stock_id, hold_stock_pct, hold_stock_basis, player_advances, option_recs, futures_recs,
        commod_margin_bal, swap_contracts, phys_commod_rec, asset_sale_offers, auto_pilot, sweep_cash_to_loan,
        player_cheat_code, price_alerts, player_growth_throttle, is_computer_player, player_loan_offers_avail,
        country_idx, entity_status, corp_controller
    );
    n
}

/// `LoadGame(fileName)` (0x48a1d4) reading the plain (decoded) save text.
///
/// Skipped UI: `PauseTicker` (0x46e001), `MENU SET TEXT MenuSettings, 652, "&Cheat Mode is: Off"` when
/// `CheatMode = "N"`, the version-rejection MSGBOX (returned as [`LoadError::VersionRejected`]), the
/// bank-allocation `ShowQuickMessage` (returned in [`LoadInfo::notices`]), `CONTROL SET TEXT` of the ticker
/// label (123) with `TickerText`.
///
/// `rng` is the game's RND generator: saves of version 8.21 and older get a new random bank asset
/// allocation (`BankAllocateAssets(bank, 2, 0)` for every bank 11..85).
pub fn load_game(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, plain: &[u8]) -> Result<LoadInfo, LoadError> {
    let oob0 = oob_total(g);
    let mut info = LoadInfo::default();
    let mut r = Rd::new(plain);
    let res = load_body(g, ui, rng, &mut r, &mut info);
    let res = match res {
        Ok(()) if oob_total(g) != oob0 => Err(LoadError::Runtime { err: 9 }),
        other => other,
    };
    match res {
        Ok(()) => Ok(info),
        Err(e) => {
            if let LoadError::Runtime { .. } = e {
                // error handler 0x48eb06
                g.game_end_flag = s("Y");
            }
            Err(e)
        }
    }
}

fn load_body(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, r: &mut Rd, info: &mut LoadInfo) -> Result<(), LoadError> {
    let __content = crate::content::current();
    // (UI) PauseTicker
    g.turn_action_count = r.long()?;
    g.save_toggle = r.long()?;
    g.active_entity = r.long()?;
    g.active_entity_name = r.str()?;
    g.sel_corp_name = r.str()?;
    g.sel_entity = r.long()?;
    g.saved_game_version = r.str()?;
    let mut version: f32;
    if g.version_str == g.saved_game_version {
        version = val_single(&g.version_str);
    } else {
        version = val_single(&g.saved_game_version);
        // 0x48a35d: fld dword 3.1; fcomp version; je …; fild word 6; fcomp version; ja → reject
        if version != 3.1f32 && version < 6.0f32 {
            let message = s(__content.string(13523))
                + &g.saved_game_version
                + ").";
            g.turn_action_count = 0;
            g.active_entity = 0;
            g.active_entity_name = PbStr::new();
            g.sel_corp_name = PbStr::new();
            g.sel_entity = 0;
            g.saved_game_version = PbStr::new();
            g.game_end_flag = s("Y"); // CLOSE; GameEndFlag = "Y"; exit
            return Err(LoadError::VersionRejected { message });
        }
        // (SavedGameVersion = "" only after the rejection above)
    }
    g.long_game_marker = r.int()?;
    g.asbestos_corp = r.long()?;
    g.asset_sale_desc = r.str()?;
    g.antitrust_defendant = r.long()?;
    g.antitrust_market_share = r.long()?;
    g.antitrust_plaintiff = r.long()?;
    g.entity_borrow_rate = cast::<f32, _>(r.single()?);
    g.bank_loans_total = r.ext()?;
    g.max_bond_issue = r.quad()?;
    g.long_bond_price = cast::<f32, _>(r.single()?);
    g.bubble_state = r.long()?;
    g.cap_gains_tax_rate = r.long()?;
    g.bank_cd_rate = cast::<f32, _>(r.single()?);
    g.central_bank_name = r.str()?;
    g.scenario_corp = r.int()?;
    g.event_player = r.int()?;
    g.save_unused24 = r.int()?;
    g.cheat_mode = r.str()?;
    // (UI) IF CheatMode = "N" THEN MENU SET TEXT MenuSettings, 652, "&Cheat Mode is: Off"
    g.adventure_cooldown = r.int()?;
    g.sim_co_count = r.long()?;
    g.has_cpu_players = r.str()?;
    g.econ_scenario_id = r.long()?;
    g.exchange_rate = cast::<f64, _>(r.ext()?); // fstp qword (DOUBLE)
    g.currency_name = r.str()?;
    g.antitrust_damages = r.quad()?;
    g.difficulty_level = r.long()?;
    g.currency_symbol = r.str()?;
    g.econ_cycle_counter = r.int()?;
    g.cheat_eq_fund_target = r.int()?;
    g.currency_suffix = r.str()?;
    g.recent_mgmt_change_list = r.str()?;
    let _j = r.long()?; // GameEndYear - StartYear, discarded (local j)
    g.gdp_level = r.quad()?;
    g.gdp_growth = cast::<f32, _>(r.single()?);
    g.cheat_japan_target = r.int()?;
    g.malicious_suit_timer = r.int()?;
    g.malicious_suit_defendant = r.long()?;
    g.malicious_suit_plaintiff = r.long()?;
    g.housing_starts = r.long()?;
    g.stock_index_divisor = r.ext()?;
    g.prev_extra_div_corp = r.long()?;
    g.sofr_rate = cast::<f32, _>(r.single()?);
    g.monetary_stance = r.long()?;
    if Ext::from_f64(1.0) < (cast::<Ext, _>(g.monetary_stance)).abs() {
        g.monetary_stance = 0;
    }
    g.new_mgmt_list = r.str()?;
    if (cast::<Ext, _>(4.79f32)) < val(&g.saved_game_version) {
        g.merger_proposer_player = r.long()?;
        g.merger_acquirer = r.long()?;
        g.merger_target = r.long()?;
        g.merger_price_per_share.set_ext(r.exact()?); // PB_CuxStore
        g.next_takeover_tick = r.long()?;
    }
    g.million_prefix_lc = r.str()?;
    g.million_prefix_uc = r.str()?;
    g.world_event_state = r.int()?;
    g.monetary_policy = r.int()?;
    g.world_generated = r.long()?;
    g.name_list_dirty = r.int()?;
    g.turnaround_pick_corp = r.int()?;
    g.save_unused_str64 = r.str()?;
    g.story_events111to120_done = r.int()?;
    g.scenario_stage = r.int()?;
    g.oil_price = cast::<f32, _>(r.single()?);
    g.peak_oil_crisis = r.int()?;
    g.oil_crisis_level = cast::<f32, _>(r.single()?);
    g.oil_crisis_stage = r.long()?;
    g.cur_player = r.long()?;
    g.cur_player_name = r.str()?;
    g.num_players = r.long()?;
    g.base_interest_rate = cast::<f32, _>(r.single()?);
    g.hot_stock_corp = r.int()?;
    g.exe_name = r.str()?;
    g.cur_quarter = r.long()?;
    g.adv_offering_corp = r.int()?;
    g.currency_code = r.str()?;
    // repair the currency symbols the save codec mangles (see pb::codec)
    if g.currency_code == "UK" {
        g.currency_symbol = PbStr(vec![163, b' ']); // CHR$(163) + " "
    }
    if g.currency_code == "JAP" {
        g.currency_symbol = PbStr(vec![165, b' ']); // CHR$(165) + " "
    }
    g.adv_restruct_corp = r.int()?;
    g.loaded_end_year_dup = r.long()?;
    g.rumor_victim_id = r.long()?;
    g.libel_suit_defendant = r.int()?;
    g.short_bond_desc = r.str()?;
    g.short_bond_yield = cast::<f32, _>(r.single()?);
    g.short_bond_price = cast::<f32, _>(r.single()?);
    g.scenario_num = r.int()?;
    g.scenario_tag = r.str()?;
    g.rand_seed = r.long()?;
    g.lookup_entity_name = r.str()?;
    g.target_corp = r.long()?;
    g.sel_industry = r.long()?;
    g.antitrust_settle_amt = r.quad()?;
    g.prime_rate = cast::<f32, _>(r.single()?);
    g.stock_index = r.ext()?;
    g.market_valuation = cast::<f32, _>(r.single()?);
    g.max_stock_offering_amt = r.quad()?;
    g.strike_corp_code = r.int()?;
    g.last_extra_div_corp = r.long()?;
    g.superfund_end_year = r.long()?;
    g.superfund_corp = r.int()?;
    g.start_year = r.long()?;
    g.start_year += 2002;
    g.gdp_growth_target = cast::<f32, _>(r.single()?);
    g.interest_rate_target = cast::<f32, _>(r.single()?);
    g.oil_target_price = cast::<f32, _>(r.single()?);
    g.long_bond_desc = r.str()?;
    g.long_bond_yield = cast::<f32, _>(r.single()?);
    g.ticker_text = r.str()?;
    let i = r.long()?;
    if g.ticker_speed == 0 {
        // the configured speed (CONFIG) wins over the saved one
        g.ticker_speed = i;
    }
    g.takeover_candidate_id = r.long()?;
    g.corp_tax_rate = r.long()?;
    let _unused_float = r.single()?;
    g.app_title = r.str()?;
    g.cur_year = r.long()?;
    g.cur_year += 2003;
    g.game_end_year = r.long()?;
    g.game_end_year += 1806;

    let n = g.num_players; // loop bounds are evaluated once
    for i in 1..=n {
        g.player_select_last_corp[i] = r.long()?;
        g.player_last_stock[i] = r.long()?;
        g.player_suppress_popups[i] = r.long()?;
        g.player_suppress_earnings[i] = r.long()?;
        g.player_auto_save[i] = r.long()?;
        if 5.01f32 <= version {
            g.player_auto_add_stream[i] = r.long()?;
        }
        if 9.1f32 <= version {
            g.player_suppress_cash_warn[i] = r.long()?;
        }
        g.watch_list[(0, i)] = r.long()?;
        if g.watch_list[(0, i)] != 0 {
            for j in 1..=15 {
                g.watch_list[(j, i)] = r.long()?;
                g.stream_prev_price_str[(j, i)] = r.str()?;
                g.stream_cur_price[(j, i)] = r.str()?;
                g.stream_price_dir[(j, i)] = r.long()?;
            }
        }
    }
    let n = g.num_players;
    for i in 1..=n {
        g.player_ceo_of[i] = r.long()?;
        g.ent_name[i] = r.str()?;
        g.ent_symbol[i] = r.str()?;
        if version == Ext::from_f32(0.0f32) || 2.4f32 < version {
            g.player_prepaid_tax[i] = r.ext()?;
        }
    }
    for i in 1..=60 {
        g.news_headline[i] = r.str()?;
    }
    let n = g.num_players;
    for i in 1..=n {
        g.player_cap_gains[i] = r.ext()?;
        g.player_notoriety[i] = r.int()?;
        g.player_law_firm[i] = r.long()?;
        g.player_name[i] = r.str()?;
        g.player_turn_order[i] = r.long()?;
        g.player_startup_count[i] = r.long()?;
    }
    for i in 1..=10 {
        g.recent_antitrust_filers[i] = r.long()?;
        g.recent_offerings[i] = r.long()?;
        g.lawsuit_rec[i] = r.str()?;
    }
    for i in 0..=55 {
        g.country_name[i] = r.str()?;
        g.country_display_name[i] = r.str()?;
    }
    for i in 0..=70 {
        g.ind_name[i] = r.str()?;
    }
    g.ind_name[71] = s(__content.string(6736));
    for i in 1..=70 {
        g.ind_demand[i] = r.quad()?;
    }
    for i in 1..=200 {
        g.event_status[i] = r.int()?;
    }
    for i in 11..=1600 {
        g.country_idx[i] = r.long()?;
        g.corp_prior_net_worth[i] = r.ext()?;
    }
    for i in 1..=70 {
        g.ind_growth_rate[i] = r.int()?;
        g.ind_growth_rate_now[i] = r.int()?;
        g.ind_long_term_growth[i] = r.int()?;
        g.ind_volatility[i] = r.int()?;
        g.ind_growth_forecast[i] = r.int()?;
    }
    for i in 1..=100 {
        g.event_state[i] = r.long()?;
    }
    for i in 11..=85 {
        g.bank_demand_deposits[i] = r.ext()?;
        g.bad_debt_reserve[i] = r.ext()?;
        g.bank_consumer_loans[i] = r.quad()?;
        g.bank_mortgage_loans[i] = r.quad()?;
        if 8.21f32 <= version {
            g.bank_c_ds[i] = r.ext()?;
            g.bank_cust_deposits[i] = r.ext()?;
        } else {
            // older saves: deposits came from the obsolete array 0x6c5050
            g.bank_c_ds[i] = r.ext()?;
            g.obs_bank_deposits_old[i] = r.ext()?;
            g.bank_cust_deposits[i] = g.obs_bank_deposits_old[i];
            g.obs_bank_deposits_old[i] = Ext::from_f64(0.0);
        }
        g.bank_largest_debtor[i] = r.long()?;
    }
    for i in 1..=1600 {
        g.ent_misc_data[i] = r.str()?;
        g.entity_status[i] = r.int()?;
        g.credit_frozen[i] = r.long()?;
    }
    for i in 1..=130 {
        if g.entity_status[i] == 1 {
            g.long_gov_bond_face[i] = r.quad()?;
            g.lt_gov_bond_cost[i] = r.ext()?;
            g.short_gov_bond_face[i] = r.quad()?;
            g.st_gov_bond_cost[i] = r.ext()?;
        }
    }
    for i in 86..=130 {
        if g.entity_status[i] == 1 {
            g.ins_reserves[i] = r.ext()?;
            g.insur_uw_profit_ytd[i] = r.ext()?;
        }
    }
    for i in 1..=1600 {
        g.lending_bank[i] = r.int()?;
        if g.entity_status[i] == 1 {
            g.cash[i] = r.ext()?;
            g.corp_controller[i] = r.long()?;
            g.accrued_pl[i] = r.ext()?;
            g.bank_debt[i] = r.ext()?;
            g.div_recvd1[i] = r.ext()?;
            g.div_recvd2[i] = r.ext()?;
            g.earnings_ytd[i] = r.ext()?;
            g.extraordinary_pl[i] = r.quad()?;
            g.credit_rating_num[i] = r.long()?;
        }
    }
    for i in 11..=1600 {
        g.industry_num[i] = r.int()?;
        if g.entity_status[i] == 1 {
            g.dividend_per_share[i] = r.ext()?;
            g.market_cap[i] = r.ext()?;
            g.profit_margin[i] = r.int()?;
            g.bonds_issued[i] = r.quad()?;
            g.bond_issue_info[i] = r.str()?;
            g.bond_price[i] = r.cur()?;
            g.tax_loss_carry[i] = r.ext()?;
            g.equity_income_ytd[i] = r.ext()?;
            g.earnings_y1[i] = r.ext()?;
            g.disc_spend_pct[i] = r.int()?;
            g.shares_outstanding[i] = r.ext()?;
            g.earnings_yr2_ago[i] = r.ext()?;
            g.earnings_yr3_ago[i] = r.ext()?;
            g.earnings_yr4_ago[i] = r.ext()?;
            g.investor_sentiment[i] = r.long()?;
            g.proj_qtr_earnings[i] = r.ext()?;
            g.ctrl_since_year[i] = r.long()?;
        }
    }
    for i in 86..=1600 {
        if g.entity_status[i] == 1 {
            g.growth_rate[i] = r.int()?;
        }
    }
    for i in 131..=1600 {
        if g.entity_status[i] == 1 {
            g.business_assets[i] = r.ext()?;
            g.working_capital[i] = r.ext()?;
            g.goodwill[i] = r.ext()?;
        }
    }
    for i in 1..=130 {
        if g.entity_status[i] == 1 {
            read_bond_holdings(g, r, i)?;
        }
    }
    for i in 1..=1600 {
        if g.entity_status[i] == 1 {
            g.hold_stock_id[(0, i)] = r.int()?;
            if g.hold_stock_id[(0, i)] != 0 {
                let count = cast::<i32, _>(g.hold_stock_id[(0, i)]);
                for j in 1..=count {
                    g.hold_stock_id[(j, i)] = r.int()?;
                    g.hold_stock_pct[(j, i)] = r.int()?;
                    g.hold_stock_basis[(j, i)] = r.ext()?;
                }
            }
        }
    }
    let n = g.num_players;
    for i in 1..=n {
        for j in 11..=1600 {
            g.player_advances[(i, j)] = r.ext()?;
        }
    }
    for i in 11..=1600 {
        g.extra_items_ytd[i] = r.ext()?;
        g.consol_tax_inc_ytd[i] = r.ext()?;
    }
    for i in 1..=5 {
        g.player_recent_bond_issues[i] = r.long()?;
    }
    for i in 1..=5 {
        g.save_extra_str[i] = r.str()?;
        g.player_exercise_options[i] = r.long()?;
    }
    g.market_cycle_plan = g.save_extra_str[1].clone();
    g.scenario_headline = g.save_extra_str[2].clone();
    version = val_single(&g.saved_game_version);
    info.version = version;
    if 4.0f32 <= version {
        g.option_recs[0] = r.str()?;
        let j = to_long(val(&g.option_recs[0]));
        if 0 < j {
            for i in 1..=j {
                g.option_recs[i] = r.str()?;
            }
        }
        for i in 1..=1600 {
            if g.entity_status[i] == 1 {
                g.has_options_outstanding[i] = r.long()?;
            }
        }
    }
    if 4.5f32 <= version {
        for i in 11..=1600 {
            if g.entity_status[i] == 1 {
                g.cap_hist_close[(0, i)] = r.ext()?;
                if g.cap_hist_close[(0, i)] != Ext::from_f64(0.0) {
                    read_price_history(g, r, i)?;
                }
            }
        }
    }
    if 4.9f32 <= version {
        for i in 11..=1600 {
            if g.entity_status[i] == 1 {
                g.reserve_adj_pending[i] = r.ext()?;
                if 85 < i {
                    g.contingency_reserve[i] = r.ext()?;
                }
            }
        }
    }
    if 5.0f32 <= version {
        g.wealth_tax_rate = r.long()?;
        g.carbon_tax_rate = r.long()?;
        g.oil_windfall_tax_rate = r.long()?;
        g.holding_co_tax_rate = r.long()?;
        g.corp_shares_tax_country = r.long()?;
        g.health_care_tax_rate = r.long()?;
    }
    if 5.1f32 <= version {
        for i in 1..=5 {
            g.player_ceo_since_year[i] = r.long()?;
        }
    }
    // stage = "OK"
    if 6.0f32 <= version {
        g.gold_price = cast::<f32, _>(r.single()?);
        g.gold_target_price = cast::<f32, _>(r.single()?);
        g.gold_price_min = cast::<f32, _>(r.single()?);
        g.gold_price_max = cast::<f32, _>(r.single()?);
        g.silver_price = cast::<f32, _>(r.single()?);
        g.silver_target_price = cast::<f32, _>(r.single()?);
        g.silver_price_min = cast::<f32, _>(r.single()?);
        g.silver_price_max = cast::<f32, _>(r.single()?);
        g.wheat_price = cast::<f32, _>(r.single()?);
        g.wheat_target_price = cast::<f32, _>(r.single()?);
        g.wheat_price_min = cast::<f32, _>(r.single()?);
        g.wheat_price_max = cast::<f32, _>(r.single()?);
        g.corn_price = cast::<f32, _>(r.single()?);
        g.corn_target_price = cast::<f32, _>(r.single()?);
        g.corn_price_min = cast::<f32, _>(r.single()?);
        g.corn_price_max = cast::<f32, _>(r.single()?);
        for i in 1..=10 {
            g.futures_basis[i] = r.int()?;
        }
        g.futures_recs[0] = r.str()?;
        if Ext::from_f64(0.0) < val(&g.futures_recs[0]) {
            let count = to_long(val(&g.futures_recs[0]));
            for j in 1..=count {
                g.futures_recs[j] = r.str()?;
                let field = g.futures_recs[j].clone();
                let i = to_long(val(&parse_bar(&field, 2)));
                g.commod_margin_bal[i] = r.ext()?;
            }
        }
    }
    if 6.2f32 <= version {
        g.swap_contracts[0] = r.str()?;
        if Ext::from_f64(0.0) < val(&g.swap_contracts[0]) {
            let count = to_long(val(&g.swap_contracts[0]));
            for j in 1..=count {
                g.swap_contracts[j] = r.str()?;
            }
        }
    }
    if 6.5f32 <= version {
        g.covered_call_fund_id = r.long()?;
        g.recent_dividend_change_list = r.str()?;
        let mut i = 0;
        loop {
            if i < 1 || 5 < i || g.entity_status[i] == 1 {
                if i == 11 {
                    i = 1601;
                }
                g.cap_hist_close[(0, i)] = r.ext()?;
                read_price_history(g, r, i)?;
            }
            i += 1;
            if i >= 1606 {
                break;
            }
        }
        for i in 11..=1600 {
            if g.entity_status[i] == 1 {
                g.tax_credits[i] = r.ext()?;
            }
        }
    }
    if 6.6f32 <= version {
        g.fad_industry = r.long()?;
        g.fad_valuation_pct = r.long()?;
        for i in 1586..=1600 {
            g.fund_adviser[i] = r.long()?;
            g.fund_fee_pct[i] = cast::<f32, _>(r.single()?);
        }
    }
    if 6.7f32 <= version {
        g.commod_trade_mode = r.str()?;
        g.start_net_worth = r.long()?;
        g.phys_commod_rec[0] = r.str()?;
        if Ext::from_f64(0.0) < val(&g.phys_commod_rec[0]) {
            let count = to_long(val(&g.phys_commod_rec[0]));
            for j in 1..=count {
                g.phys_commod_rec[j] = r.str()?;
            }
        }
        for i in 101..=200 {
            g.event_state[i] = r.long()?;
        }
    }
    if 7.0f32 <= version {
        g.asset_sale_offers[0] = r.str()?;
        if Ext::from_f64(0.0) < val(&g.asset_sale_offers[0]) {
            let count = to_long(val(&g.asset_sale_offers[0]));
            for i in 1..=count {
                g.asset_sale_offers[i] = r.str()?;
            }
        }
        for i in 3..=69 {
            g.ind_demand_gap[i] = cast::<f64, _>(r.ext()?); // DOUBLE
        }
        g.cur_make_delivery = r.long()?;
        g.cur_take_delivery = r.long()?;
        for i in 1..=5 {
            g.player_make_delivery[i] = r.long()?;
            g.player_take_delivery[i] = r.long()?;
        }
    }
    if 7.5f32 <= version {
        g.loan_limit_multiple = cast::<f64, _>(r.ext()?); // DOUBLE
        g.insider_probe_player = r.long()?;
        g.merger_tip_corp = r.int()?;
        g.insider_earn_tip_corp = r.int()?;
        g.insider_tippee = r.int()?;
        g.insider_tip_event_tick = r.int()?;
        g.insider_tip_event_qtr = r.int()?;
        g.tip_industry = r.int()?;
        for i in 1..=5 {
            g.tipster_state[i] = r.int()?;
            g.auto_pilot[i] = r.long()?;
        }
    }
    if 7.52f32 <= version {
        g.subprime_crisis_state = r.int()?;
        g.market_val_target = cast::<f64, _>(r.ext()?); // DOUBLE
        for i in 11..=130 {
            g.subprime_secs[i] = r.quad()?;
        }
        for i in 131..=1600 {
            g.corp_hedging_gain[i] = r.ext()?;
        }
    }
    if 7.53f32 <= version {
        for i in 1..=10 {
            g.save_misc_quad[i] = r.quad()?;
        }
        g.stock_index_base = to_long(cast::<Ext, _>(g.save_misc_quad[1]));
        g.next_market_cycle_year = to_long(cast::<Ext, _>(g.save_misc_quad[2]));
        if 7.7f32 <= version {
            g.auto_save_state = to_long(cast::<Ext, _>(g.save_misc_quad[3]));
        }
        g.ai_hostile_merger_qtr = to_long(cast::<Ext, _>(g.save_misc_quad[4]));
        g.bank_merger_player = to_long(cast::<Ext, _>(g.save_misc_quad[5]));
        g.pandemic_state = to_integer(cast::<Ext, _>(g.save_misc_quad[6]));
        g.metals_price_base = to_long(cast::<Ext, _>(g.save_misc_quad[7]));
        // UpdateMetalsPriceBase (0x63a94e): recomputes MetalsPriceBase from the date and may raise the gold
        // prices, IndGrowthForecast(50), IndDemand(50) and post news.
        crate::logic::futures::update_metals_price_base(g, ui);
        g.crypto_ban_state = to_long(cast::<Ext, _>(g.save_misc_quad[8]));
        g.private_antitrust_off = to_long(cast::<Ext, _>(g.save_misc_quad[9]));
        g.govt_antitrust_off = to_long(cast::<Ext, _>(g.save_misc_quad[10]));
        if g.private_antitrust_off == 1 || g.govt_antitrust_off == 1 {
            g.antitrust_cheat_used = 1;
        }
    }
    if version < 7.61f32 {
        if g.has_cpu_players == "Y" {
            g.is_computer_player[1] = 1;
        }
        if g.cpu_max_corps == 0 {
            g.cpu_max_corps = 80;
        }
    } else {
        for i in 1..=5 {
            g.is_computer_player[i] = r.long()?;
            g.player_last_active_entity[i] = r.long()?;
            g.player_corps_controlled[i] = r.long()?;
        }
        g.cpu_max_corps = r.long()?;
    }
    // 7.5 … 7.6: AutoPilot was per player; extend it to every corp the player controls (except his CEO corp)
    let n = g.num_players;
    if 7.5f32 <= version && version <= 7.6f32 {
        for j in 1..=n {
            if g.player_name[j] == "...OUT..." {
                continue;
            }
            if g.auto_pilot[j] == 0 {
                continue;
            }
            for i in 11..=1600 {
                if g.entity_status[i] == 1 && j == g.corp_controller[i] {
                    g.auto_pilot[i] = 1;
                }
            }
            let ceo_corp_id = g.player_ceo_of[j];
            if 10 < ceo_corp_id {
                g.auto_pilot[ceo_corp_id] = 0;
            }
        }
    }
    if 7.62f32 <= version {
        for i in 1..=5 {
            g.player_antitrust_level[i] = r.long()?;
        }
        for i in 11..=1600 {
            g.auto_pilot[i] = r.long()?;
        }
    }
    if 7.8f32 <= version {
        g.cur_sweep_cash = r.long()?;
        let n = g.num_players;
        for i in 1..=n {
            g.sweep_cash_to_loan[i] = r.long()?;
        }
    }
    if version < 7.9f32 {
        crate::logic::shell::leftovers::convert_old_futures_units_state(g);
        info.notices.push(s(crate::logic::shell::leftovers::OLD_FUTURES_UNITS_NOTICE(&__content)));
    }
    let n = g.num_players;
    if 7.9f32 <= version {
        for i in 1..=n {
            g.player_cheat_code[i] = r.long()?;
        }
    }
    if 7.92f32 <= version {
        g.price_alerts[0] = r.str()?;
        if Ext::from_f64(0.0) < val(&g.price_alerts[0]) {
            let count = to_long(val(&g.price_alerts[0]));
            for j in 1..=count {
                g.price_alerts[j] = r.str()?;
            }
        }
    }
    if 7.93f32 <= version {
        for j in 1..=50 {
            g.asset_buy_watch_list[j] = r.long()?;
        }
    }
    if 8.2f32 < version {
        for i in 1..=1600 {
            if g.entity_status[i] == 1 {
                g.t_bills[i] = r.ext()?;
            }
        }
    }
    if version <= 8.21f32 {
        if version < 8.25f32 {
            for i in 11..=85 {
                crate::logic::sim_entity::corp_ops::bank_allocate_assets(g, ui, rng, i, 2, 0);
            }
            info.notices.push(s(OLD_BANK_ALLOC_NOTICE(&__content))); // ShowQuickMessage
        }
    } else {
        for i in 11..=85 {
            g.bank_alloc_prime_mort_pct[i] = r.int()?;
            g.bank_alloc_subprime_pct[i] = r.int()?;
            g.bank_alloc_consumer_pct[i] = r.int()?;
            g.bank_alloc_bonds_pct[i] = r.int()?;
            g.bank_alloc_cash_pct[i] = r.int()?;
        }
    }
    if 8.25f32 < version {
        for i in 11..=85 {
            if g.entity_status[i] == 1 {
                g.growth_rate[i] = r.int()?;
            }
        }
    }
    if 8.5f32 < version {
        for i in 11..=85 {
            if g.entity_status[i] == 1 {
                g.bank_alloc_off[i] = r.int()?;
            }
        }
    }
    if 8.57f32 < version {
        for i in 1586..=1600 {
            if g.entity_status[i] == 1 {
                g.fund_bonus_year[i] = r.long()?;
            }
        }
    }
    let n = g.num_players;
    if 8.6f32 <= version {
        for i in 1..=n {
            g.player_growth_throttle[i] = r.int()?;
        }
    } else {
        for i in 1..=n {
            if g.is_computer_player[i] == 0 {
                g.player_growth_throttle[i] = 60;
            }
        }
    }
    if 8.72f32 < version {
        for i in 1582..=1583 {
            read_bond_holdings(g, r, i)?;
        }
        if g.entity_status[1581] == 1 {
            g.long_gov_bond_face[1581] = r.quad()?;
            g.lt_gov_bond_cost[1581] = r.ext()?;
            g.short_gov_bond_face[1581] = r.quad()?;
            g.st_gov_bond_cost[1581] = r.ext()?;
        }
        for i in 1581..=1585 {
            g.fund_adviser[i] = r.long()?;
            g.fund_fee_pct[i] = cast::<f32, _>(r.single()?);
            g.fund_bonus_year[i] = r.long()?;
        }
    }
    if 9.11f32 <= version {
        g.bitcoin_price = r.long()?;
        g.bitcoin_target_price = r.long()?;
        g.ethereum_price = r.long()?;
        g.ethereum_target_price = r.long()?;
        g.crypto_boom_years = r.long()?;
        let i = 1606;
        g.cap_hist_close[(0, 1606)] = r.ext()?;
        read_price_history(g, r, i)?;
    }
    let n = g.num_players;
    if 9.75f32 < version {
        for j in 1..=n {
            g.player_loan_offers_avail[j] = r.long()?;
        }
    }
    if g.name_list_dirty == 0 {
        // CLOSE; LoadCorpNames (0x42230e) reads CORPNAME.DAT → load_corp_names
        info.needs_corp_names = true;
    } else {
        // stage = "NewNames%": the names list SaveGame PRINTs when NameListDirty = 1
        read_name_lines(g, || r.line())?;
    }
    // (UI) CONTROL SET TEXT GetDlgItem(hDlgMain, 123), TickerText
    Ok(())
}

fn read_bond_holdings(g: &mut Globals, r: &mut Rd, i: i32) -> Result<(), LoadError> {
    g.hold_bond_id[(0, i)] = r.long()?;
    if g.hold_bond_id[(0, i)] != 0 {
        let count = g.hold_bond_id[(0, i)];
        for j in 1..=count {
            g.hold_bond_id[(j, i)] = r.long()?;
            g.hold_bond_face[(j, i)] = r.quad()?;
            g.bond_hold_cost[(j, i)] = r.ext()?;
        }
    }
    Ok(())
}

fn read_price_history(g: &mut Globals, r: &mut Rd, i: i32) -> Result<(), LoadError> {
    for j in 1..=60 {
        g.price_hist_high[(j, i)] = r.ext()?;
        g.cap_hist_close[(j, i)] = r.ext()?;
        g.price_hist_low[(j, i)] = r.ext()?;
    }
    Ok(())
}

/// The `id|country|symbol|name` line loop shared by LoadGame (0x48e8ed) and LoadCorpNames (0x42230e):
/// read lines until one contains `@END` or the entity id read is 1600.
fn read_name_lines(g: &mut Globals, mut next_line: impl FnMut() -> Result<PbStr, LoadError>) -> Result<(), LoadError> {
    loop {
        let line_text = next_line()?;
        if 0 < string::instr(1, line_text.as_bytes(), b"@END") {
            break;
        }
        let field = parse_bar(&line_text, 1);
        let i = to_long(val(&field));
        let country_str = parse_bar(&line_text, 2);
        let round_tmp = to_quad(val(&country_str)); // (longlong)ROUND, low dword stored
        g.country_idx[i] = cast::<i32, _>(round_tmp);
        let symbol_str = parse_bar(&line_text, 3);
        g.ent_symbol[i] = trim(&symbol_str);
        let name_str = parse_bar(&line_text, 4);
        g.ent_name[i] = trim(&name_str);
        if i == 1600 {
            break;
        }
    }
    Ok(())
}


/// `LoadCorpNames` (0x42230e) applied to the contents of `CORPNAME.DAT`.
///
/// Skipped: the file-exists check and its MSGBOX ("Error: Can't find file CORPNAME.DAT at …"), and the
/// `@DUMMY` probe of the first line (the file is KILLed when it contains `@DUMMY` and WinVersion <> "XP" —
/// the host decides about the file). Reading past the end raises error 62 in the original.
pub fn load_corp_names(g: &mut Globals, corpname_dat: &[u8]) -> Result<(), LoadError> {
    let mut f = Rd::new(corpname_dat);
    read_name_lines(g, || f.line())
}

// ---------------------------------------------------------------------------------------------------------
// GAMExx.DAT files: EncodeDecodeSaveGameFile (0x5d212d)
// ---------------------------------------------------------------------------------------------------------

/// `SaveGame` + `EncodeDecodeSaveGameFile(2, …)`: the on-disk `GAMExx.DAT` bytes (`@EC<VersionStr>` header).
pub fn write_save_file(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> Vec<u8> {
    let plain = save_game(g, ui, rng);
    codec::encode_save(&plain, g.version_str.as_bytes())
}

/// Decoding half of `EncodeDecodeSaveGameFile` (mode <> 2, 0x5d2290): `GAMExx.DAT` → `OLDGAME.DAT` text.
///
/// Differences to [`codec::decode_save`] that matter for exactness: a file whose first line is not an
/// `@EC` header (or whose header version is < 3) is refused (the original never reads headerless files);
/// the first line is always consumed; a "CHARACTER - 39" abort (encoded byte 40..54 at position >= 6) ends
/// the decoding (EXIT SUB without closing; the output is taken to be the lines decoded so far — whether the
/// original's buffered OLDGAME.DAT keeps them depends on the runtime's flushing). Returns the plain text and
/// the abort message if one occurred.
pub fn decode_save_file(g: &Globals, data: &[u8], fixed: bool) -> Result<(Vec<u8>, Option<PbStr>), LoadError> {
    let __content = crate::content::current();
    let ls = codec::lines(data);
    // LINE INPUT #inFile, lineText (an empty file would raise error 62 in the original)
    let first: &[u8] = ls.first().copied().unwrap_or(b"");
    let mut file_version = PbStr::new();
    let is_ec = first.len() >= 3 && &first[..3] == b"@EC";
    if is_ec {
        file_version = PbStr(first[3..].to_vec()); // RIGHT$(lineText, LEN - 3)
    }
    let decode = (is_ec && val(&g.version_str) <= val(&file_version)) || Ext::from_f64(3.0) <= val(&file_version);
    if !decode {
        // ShowQuickMessage " [runtime resource wording] …"; KILL OLDGAME.DAT
        return Err(LoadError::OldFileFormat { file_version, version_str: g.version_str.clone() });
    }
    let mut plain = Vec::with_capacity(data.len());
    for &l in ls.iter().skip(1) {
        if fixed {
            if l == b"@EC" {
                continue; // structural skip kept in the fixed decoder too
            }
            plain.extend(codec::decode_line_fixed(l));
            plain.extend_from_slice(b"\r\n");
            continue;
        }
        match codec::decode_line(l) {
            Some(d) => {
                plain.extend(d);
                plain.extend_from_slice(b"\r\n");
            }
            None if l == b"@EC" => {} // RETURN without PRINT
            None => {
                // MSGBOX "CHARACTER - 39 = " + STR$(code - 39) + "  V&=" + STR$(code)
                let (code, _) = l
                    .iter()
                    .enumerate()
                    .map(|(k, &c)| (c, k + 1))
                    .find(|&(c, p)| p >= 6 && (40..=54).contains(&c))
                    .unwrap_or((0, 0));
                let msg = s(__content.string(13524)) + &str_num(cast::<i64, _>(code) - 39) + "  V&=" + &str_num(cast::<i64, _>(code));
                return Ok((plain, Some(msg)));
            }
        }
    }
    Ok((plain, None))
}

/// `EncodeDecodeSaveGameFile(1, …)` + `LoadGame`: load a `GAMExx.DAT` file. `fixed` selects
/// [`codec::decode_line_fixed`] instead of the original's lossy decoder. Run [`open_saved_game_fixups`]
/// afterwards for the state changes MenuOpenSavedGame makes.
pub fn read_save_file(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, data: &[u8], fixed: bool) -> Result<LoadInfo, LoadError> {
    let (plain, abort) = decode_save_file(g, data, fixed)?;
    let mut info = load_game(g, ui, rng, &plain)?;
    if let Some(m) = abort {
        info.notices.insert(0, m);
    }
    Ok(info)
}

// ---------------------------------------------------------------------------------------------------------
// Callers: SaveGamePrompt (0x40c01b) and MenuOpenSavedGame (0x40ae7f)
// ---------------------------------------------------------------------------------------------------------

/// `"GAME" + two-digit number + ".DAT"` as built by SaveGamePrompt / MenuOpenSavedGame.
pub fn game_file_name(game_number: i32) -> PbStr {
    let mut t = str_trim(cast::<i64, _>(game_number));
    if t.len() == 1 {
        t = s("0") + &t;
    }
    s("GAME") + &t + ".DAT"
}

/// The non-UI part of `SaveGamePrompt(saveMode)` (0x40c01b) from the bank loop on: returns the file name
/// and the encoded `GAMExx.DAT` bytes, or `None` when the game is over (" [runtime resource wording] ").
/// For "Save As" (mode 1) the caller asks for the number first and sets `GameNumber`. `time`/`date` are
/// `TIME$`/`DATE$` for the news line.
pub fn save_game_prompt(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, save_mode: i32, time: &PbStr, date: &PbStr) -> Option<(PbStr, Vec<u8>)> {
    let __content = crate::content::current();
    if g.game_in_progress == 1 && g.game_end_flag == "Y" {
        return None; // ShowQuickMessage " [runtime resource wording] "
    }
    if save_mode < 2 {
        g.clock_state = s("STOP");
    }
    for bank_id in 11..=85 {
        if g.entity_status[bank_id] == 1 {
            crate::logic::finance::calc_entity_financials(g, ui, rng, bank_id);
        }
    }
    if 10 < g.last_sel_entity {
        let p = g.cur_player;
        g.player_select_last_corp[p] = g.last_sel_entity;
    }
    let file_name = game_file_name(g.game_number);
    // (UI) save_mode <> 2: CONTROL SET TEXT 123, " [runtime resource wording] "
    if g.auto_save_state == 1 {
        g.auto_save_state = 2;
    }
    g.news_msg = s(__content.string(13525)) + &str_num(cast::<i64, _>(g.game_number)) + __content.string(13526) + &file_name + " AT " + time + " ON " + date + "...";
    let plain = save_game(g, ui, rng);
    g.auto_save_state = 0;
    let data = codec::encode_save(&plain, g.version_str.as_bytes());
    // shell::app::save_game_mode supplies PauseTicker, PostNews and TickerSimSteps after persisting.
    // The shell also shows EncodeDecodeSaveGameFile's confirmation after persisting the bytes.
    Some((file_name, data))
}

/// State changes `MenuOpenSavedGame` (0x40ae7f) makes after `LoadGame` returned (0x40b480 …).
///
/// Ported: the `CHR$(202)`→`£` / `CHR$(204)`→`¥` repair of `NewsHeadline(1..60)` (undoing the codec bug for
/// these two characters), `GameEndFlag "Y"` → `"N"` on a failed load, otherwise `GameInProgress = 1`,
/// `PriceHistHigh(0, i) = -1` in the first quarter, `SelIndustry`, `NewsMsg` (needs `TIME$`/`DATE$`),
/// `MaxBondIssue`/`MaxStockOfferingAmt` defaults, `LastSelEntity`, computer players' settings.
///
/// This legacy helper applies a subset of the post-load work. Hosts should use
/// [`crate::logic::shell::app::load_game`] for the full shell lifecycle, including news, menus,
/// streaming quotes and ticker steps. Skipped UI here: difficulty message (`RandSeed MOD 4 = 1`),
/// labels, menus, LASTGAME.DAT rewrite ([`lastgame_dat`]).
pub fn open_saved_game_fixups(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time: &PbStr, date: &PbStr) {
    let __content = crate::content::current();
    crate::logic::shorts::recalc_short_margin_balances(g); // 0x5e0ade
    crate::logic::economy::update_crypto_prices(g, rng); // 0x6b68ef
    // KILL OLDGAME.DAT
    for idx in 1..=60 {
        g.news_headline[idx] = string::replace_any(g.news_headline[idx].as_bytes(), &[202], &[163]);
        g.news_headline[idx] = string::replace_any(g.news_headline[idx].as_bytes(), &[204], &[165]);
    }
    // The host computes the currency caption with shell::settings::update_currency_menu_text.
    crate::logic::futures::init_commodity_markets(g, ui, rng); // 0x629b50
    // (UI) difficulty level message when RandSeed MOD 4 = 1; "MAJOR NEW FEATURES" text is built and discarded
    if g.game_end_flag == "Y" {
        g.game_end_flag = s("N");
        return;
    }
    g.game_in_progress = 1;
    if !crate::logic::newgame_helpers::upgrade_save_add_etfs(g, ui, rng) { return; } // 0x649fc9
    if g.cur_quarter == 1 && g.start_year == g.cur_year {
        for idx in 11..=1600 {
            if (g.earnings_yr2_ago[idx] != Ext::from_f64(0.0) || g.earnings_yr3_ago[idx] != Ext::from_f64(0.0)) && g.entity_status[idx] == 1 {
                g.price_hist_high[(0, idx)] = -Ext::from_f64(1.0);
            }
        }
    }
    // LASTGAME.DAT ← WRITE GameNumber; (UI) EnableMainControls, UpdateFileMenuState, UpdateDateDisplay,
    // ShowNextEarningsDate, labels 169/202
    if g.sel_industry == 0 && 10 < g.sel_entity {
        g.sel_industry = cast::<i32, _>(g.industry_num[g.sel_entity]);
    }
    if g.news_msg.is_empty() {
        let up = string::ucase(g.app_title.as_bytes(), string::CaseMode::Ansi);
        g.news_msg = s("...") + &up + __content.string(10021) + time + " ON " + date + "...";
    }
    // The full shell path posts the news, refreshes the list and builds the industry combo here.
    // UpdateMarketDisplay; TIME$ seconds for CalcStockPrice = VAL(RIGHT$(TIME$, 2))
    let time_secs = to_long(crate::pb::numeric::val(string::right(time.as_bytes(), 2).as_bytes()));
    crate::logic::market::update_market_display(g, ui, rng, time_secs);
    if g.max_bond_issue == 0 {
        g.max_bond_issue = to_quad(g.exchange_rate * Ext::from_f64(150000.0));
    }
    if g.max_stock_offering_amt == 0 {
        g.max_stock_offering_amt = to_quad(g.exchange_rate * Ext::from_f64(100000.0));
    }
    if 0 < g.cur_player {
        g.last_sel_entity = g.player_select_last_corp[g.cur_player];
    }
    // (UI) button 173; RebuildStreamingQuotesPanel
    let n = g.num_players;
    for idx in 1..=n {
        if g.is_computer_player[idx] == 1 {
            g.player_suppress_popups[idx] = 1;
            g.player_suppress_earnings[idx] = 1;
            g.player_suppress_cash_warn[idx] = 1;
            g.player_auto_save[idx] = 0;
            g.player_auto_add_stream[idx] = 0;
            g.player_exercise_options[idx] = 1;
            g.player_make_delivery[idx] = 1;
            g.player_take_delivery[idx] = 0;
        }
    }
    // The full shell path updates the settings menu and advances the ticker here.
}

