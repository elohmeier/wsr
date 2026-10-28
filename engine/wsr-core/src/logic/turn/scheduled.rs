//! `SimScheduledEvents` (0x4a395a): the events and AI routines of one tick, scheduled by `SimCoCount`.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::Env;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::economy::commodities::rebase_commodity_targets_from_history;
use crate::logic::economy::crises::tech_bubble_cycle;
use crate::logic::economy::crises2::subprime_crisis_update;
use crate::logic::economy::cycle::{gov_fiscal_policy_news, market_aftershock, market_reversal_event, plan_market_cycle};
use crate::logic::economy::industry::industry_fad_news;
use crate::logic::economy::macro_update::{govt_tax_policy_events, update_interest_rates, update_loan_limit_multiple};
use crate::logic::economy::quarterly::update_industry_long_term_growth;
use crate::logic::cashflow::cash_flow_alert;
use crate::logic::finance::{get_industry_category, get_managing_player};
use crate::logic::futures::recalc_commod_margin_balances;
use crate::logic::market::commodities::update_commodity_prices;
use crate::logic::market::{toggle_streaming_quote, update_market_display};
use crate::logic::options::check_naked_option_margin;
use crate::logic::reports_corp::{check_cross_ownership, update_antitrust_reserves};
use crate::logic::restructure::liquidate_sub_into_parent;
use crate::logic::reports_lists::{check_price_alerts, expire_price_alerts, update_streaming_quotes};
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::bonds::{convert_or_redeem_convertibles, force_regulated_call_sales};
use crate::logic::sim_entity::corp_ops::bank_allocate_assets;
use crate::logic::sim_entity::etf::{ai_corp_sell_naked_puts, pick_covered_call_fund};
use crate::logic::sim_entity::liquidation::liquidate_corporation;
use crate::logic::commod_ai::futures_ai::{ai_futures_trading, commodity_etf_trading, leveraged_etf_futures_update, settle_expired_futures_at};
use crate::logic::commod_ai::limits::enforce_commodity_position_limits;
use crate::logic::commod_ai::physical::{ai_trade_phys_commod, charge_commod_storage_costs};
use crate::logic::corp_ai::capital::{
    ai_bond_recapitalization, ai_corp_buys_stock, ai_random_bond_issue, process_stock_splits_all, revise_earnings_estimates,
};
use crate::logic::corp_ai::distress::{
    ai_bank_buys_loan, ai_buy_distressed_assets, ai_dividend_cuts_covenants, ai_holding_co_merger_plan,
    ai_liquidate_random_subsidiary, ai_manage_small_holdings, bank_bailout_injection,
};
use crate::logic::corp_ai::misc::{
    accrue_gov_bond_interest, amortize_loan_portfolios, asbestos_reserve_charge, charge_superfund_costs,
    check_excess_loss_recapture, pick_hot_stock_corp, rebase_stock_index_divisor, trim_recent_dividend_changes,
    turnaround_news, young_corp_margin_erosion,
};
use crate::logic::corp_ai::spinoff::ai_tax_free_spinoff;
use crate::logic::etf_derivs::corp_options::{
    ai_corp_options_trading, force_close_derivatives_bad_credit, force_close_excess_naked_options,
    force_insider_short_cover, grant_executive_options, vest_executive_options,
};
use crate::logic::etf_derivs::etf_mgmt::{ai_etf_invest_cash, etf_adviser_review, etf_compliance_check, revive_defunct_etfs};
use crate::logic::etf_derivs::etf_ops::{
    bond_etf_manage_portfolio, etf_pay_performance_bonus, etf_sell_ineligible_holding, etf_update_bonus_eligibility,
    index_etf_maintenance, prompt_reduce_etf_fee, remind_etf_manager_duties,
};
use crate::logic::etf_derivs::option_expiry::{option_expiry_reminder, process_expiring_options};
use crate::logic::etf_derivs::swaps_ai::{ai_create_swaps, swap_rate_shock_event};
use crate::logic::events::{
    ai_file_antitrust_suits, antitrust_suit_tick, govt_antitrust_enforcement, insider_trading_probe_verdict,
    merger_takeover_events, process_antitrust_suit_queue, process_malicious_lawsuits, random_corp_news_event,
};
use crate::logic::valuation::{calc_dividend_yield, calc_stock_price, count_active_corps, roll_price_history_month};

/// Port of SimScheduledEvents (0x4a395a).
/// Runs, before every tick's `SimProcessNextEntity`, the events and AI routines that are due at the
/// current `SimCoCount` (the tick number 1..1600 of the quarter): every tick the industry growth drift,
/// interest rates, tax policy, the scenario story, option/put writing, bond issues, market cycle and
/// cash-flow alerts; periodic ones (`SimCoCount % k`) such as fads, commodity prices, antitrust suits,
/// takeovers, price alerts and derivative close-outs; fixed ticks for the subprime crisis (135), loan
/// limits (960), executive options (250/260), month roll-overs of the price history (501/1001/1600),
/// tech bubbles (700/1400), the yearly bond/stock offering limits (1548), etc. It also repairs holdings
/// above 100% (tick 60), refreshes quotes of the selected companies now and then, posts the "labor unrest
/// in <country>" story (tick 28) and lets a struggling company raise its R&D / marketing spending.
///
/// `now`: `TIME$` (seconds) for the unrest story and the valuations, `DATE$` for the subprime crisis.
///
pub fn sim_scheduled_events(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    sim_scheduled_events_skipping(g, ui, rng, now, &[]);
}

/// [`sim_scheduled_events`] with routines (game addresses of ported callees) skipped like the test
/// harness does (`--skip`): for differential tests.
#[doc(hidden)]
pub fn sim_scheduled_events_skipping(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, skip: &[u32]) {
    let env = Env { now, skip };
    scheduled(g, ui, rng, &env);
}

pub(super) fn scheduled(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, env: &Env) {
    let __content = crate::content::current();
    let ts = time_secs(env.now);
    let mut pick: i32; // -0x88
    let mut watch_corp_id: i32; // -0x8c
    let mut sub_corp_id: i32; // -0x98
    let mut place_text: PbStr; // -0xa4
    let sc = |g: &Globals| g.sim_co_count;

    if !env.skips(0x4a56ec) {
        update_industry_long_term_growth(g, rng);
    }
    if !env.skips(0x4a5881) {
        pick_hot_stock_corp(g, rng);
    }
    if !env.skips(0x4a5a08) {
        random_corp_news_event(g, ui, rng, ts);
    }
    if !env.skips(0x4af1d2) {
        update_interest_rates(g, rng);
    }
    if sc(g) == 650 {
        if !env.skips(0x4afc92) {
            process_stock_splits_all(g, ui, rng);
        }
    }
    if !env.skips(0x4b819b) {
        young_corp_margin_erosion(g, rng);
    }
    if !env.skips(0x45d2c8) {
        govt_tax_policy_events(g, ui, rng);
    }
    if !env.skips(0x4b145b) {
        process_malicious_lawsuits(g, ui, rng);
    }
    if 85 < sc(g) {
        if !env.skips(0x62fac5) {
            ai_futures_trading(g, ui, rng, ts);
        }
    }
    if !env.skips(0x4b2114) {
        let s = sc(g);
        ai_manage_small_holdings(g, ui, rng, s, ts);
    }
    if !env.skips(0x53f52a) {
        process_antitrust_suit_queue(g, ui, rng);
    }
    if sc(g) == 135 && !env.skips(0x53b4c7) {
        subprime_crisis_update(g, ui, rng, &date_str(env.now));
    }
    if 200 < sc(g) && sc(g) % 117 == 1 {
        if !env.skips(0x66393c) {
            ai_trade_phys_commod(g, ui, rng, 0);
        }
    }
    if sc(g) == 960 && !env.skips(0x4bd563) {
        update_loan_limit_multiple(g, ui, rng);
    }
    if 1585 < sc(g) && (g.cur_year + sc(g)) % 2 == 1 {
        if !env.skips(0x64d7a0) {
            etf_adviser_review(g, ui, rng);
        }
    }
    if sc(g) % 81 == 47 && !env.skips(0x4bc887) {
        industry_fad_news(g, ui, rng, ts);
    }
    if 60 < sc(g) && sc(g) % 59 == 1 && !env.skips(0x62a6a2) {
        update_commodity_prices(g, ui, rng);
    }
    if 85 < sc(g) {
        if !env.skips(0x4bc3e7) {
            ai_bank_buys_loan(g, ui, rng);
        }
    }
    if 0 < g.superfund_corp && sc(g) == 745 {
        if !env.skips(0x4bbc32) {
            charge_superfund_costs(g, ui, rng);
        }
    }
    if sc(g) == 495 || sc(g) == 1175 || sc(g) == 1385 {
        if !env.skips(0x540401) {
            antitrust_suit_tick(g, ui, rng, ts);
        }
    }
    if sc(g) % 79 == 47 {
        if !env.skips(0x5407f6) {
            ai_file_antitrust_suits(g, ui, rng, ts);
        }
    }
    if sc(g) % 139 == 100 {
        if !env.skips(0x4b37c8) {
            merger_takeover_events(g, ui, rng, ts, 0);
        }
    }
    if g.next_takeover_tick == sc(g) {
        if !env.skips(0x4b37c8) {
            merger_takeover_events(g, ui, rng, ts, 1);
        }
    }
    if cast::<i32, _>(g.insider_tip_event_tick) == sc(g) && 10 < g.merger_tip_corp {
        if !env.skips(0x4b37c8) {
            merger_takeover_events(g, ui, rng, ts, 2);
        }
    }
    if sc(g) % 120 == 3 && 10 < g.sel_entity && g.auto_add_stream_list == 1 && !env.skips(0x5dda89) {
        toggle_streaming_quote(g, ui, rng, 1, 0);
    }
    if sc(g) % 50 == 7 {
        if !env.skips(0x4b30f8) {
            ai_liquidate_random_subsidiary(g, ui, rng, ts);
        }
    }
    if sc(g) == 1550 && g.cur_quarter == 3 {
        g.last_extra_div_corp = 0;
    }
    if !env.skips(0x490982) {
        super::scenario::maybe_start(g, ui, rng, env);
    }
    if !env.skips(0x48fe45) {
        super::scenario::advance_story(g, ui, rng, env);
    }
    if !env.skips(0x4b26e0) {
        ai_buy_distressed_assets(g, ui, rng);
    }
    if rng.rnd() * Ext::from_f64(4.0) <= Ext::from_f64(3.0) {
        if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) || 1585 < sc(g) {
            if !env.skips(0x5fe20b) {
                let s = sc(g);
                ai_corp_sell_naked_puts(g, ui, rng, s, 0);
            }
        }
    } else {
        if !env.skips(0x5fdd2a) {
            ai_corp_options_trading(g, ui, rng);
        }
    }
    if sc(g) == 60 {
        // Holdings above 100% (e.g. after mergers) are recomputed from the control figures.
        let mut entity_id = 11;
        while entity_id <= 1600 {
            if g.entity_status[entity_id] == 1 && g.hold_stock_id[(0, entity_id)] != 0 {
                let mut slot = 1;
                while slot <= 15 {
                    if 100 < g.hold_stock_pct[(slot, entity_id)] {
                        pick = cast::<i32, _>(g.hold_stock_id[(slot, entity_id)]);
                        g.hold_stock_pct[(slot, entity_id)] = 0;
                        update_corp_control(g, ui, rng, pick);
                        g.hold_stock_pct[(slot, entity_id)] = 100;
                        if 0 < g.public_pct {
                            g.hold_stock_pct[(slot, entity_id)] = g.public_pct;
                        }
                        if g.hold_stock_pct[(slot, entity_id)] < 1 {
                            g.hold_stock_id[(slot, entity_id)] = 0;
                            g.hold_stock_pct[(slot, entity_id)] = 0;
                            g.hold_stock_basis[(slot, entity_id)] = Ext::from_f64(0.0);
                            g.hold_stock_id[(0, entity_id)] = g.hold_stock_id[(0, entity_id)].wrapping_sub(1);
                        }
                    }
                    slot += 1;
                }
            }
            entity_id += 1;
        }
    }
    pick = g.ticker_speed;
    if g.ticker_speed < 9 {
        pick = to_long(cast::<Ext, _>(7 + pick) + rng.rnd() * Ext::from_f64(5.0));
    }
    // idiv by 0 would be a CPU fault in the original (only for TickerSpeed -12..-7)
    if g.rand_seed.checked_rem(pick) == Some(7) {
        if 10 < g.sel_entity {
            let e = g.sel_entity;
            calc_stock_price(g, ui, rng, e, ts);
            if !env.skips(0x4400d7) {
                update_market_display(g, ui, rng, ts);
            }
        }
        if 10 < g.last_sel_entity && g.entity_status[g.last_sel_entity] == 1 {
            let e = g.last_sel_entity;
            calc_stock_price(g, ui, rng, e, ts);
            let lse = g.last_sel_entity;
            update_streaming_quotes(g, ui, rng, lse - 1, lse + 1);
        }
        pick = to_long(rng.rnd() * sf(14.5f32) + Ext::from_f64(1.0));
        watch_corp_id = g.watch_list[(pick, g.cur_player)];
        if 10 < watch_corp_id && g.sel_entity != watch_corp_id {
            update_streaming_quotes(g, ui, rng, watch_corp_id - 1, watch_corp_id + 1);
        }
    }
    if sc(g) % 29 == 3 && !env.skips(0x4bb959) {
        super::ticker::ticker_watch_list_quotes(g, ui, rng);
    }
    if sc(g) == 6 || sc(g) % 16 == 3 {
        super::ticker::update_date_display(g);
    }
    if sc(g) == 498 || sc(g) == 998 || sc(g) == 1498 {
        if !env.skips(0x5ffdbc) {
            option_expiry_reminder(g, ui, rng);
        }
    }
    if sc(g) == 502 || sc(g) == 1002 || sc(g) == 1502 {
        if !env.skips(0x661bd7) {
            charge_commod_storage_costs(g, ui, rng);
        }
    }
    if sc(g) == 260 {
        if !env.skips(0x5fc7c1) {
            grant_executive_options(g, ui, rng);
        }
    }
    if sc(g) == 250 {
        if !env.skips(0x5fd441) {
            vest_executive_options(g, ui);
        }
    }
    if sc(g) == 12 || sc(g) == 510 || sc(g) == 910 || sc(g) == 1310 {
        if !env.skips(0x4b8397) {
            revise_earnings_estimates(g, ui, rng);
        }
    }
    if Ext::from_f64(24.0) < rng.rnd() * Ext::from_f64(25.0) {
        if !env.skips(0x4b8da9) {
            let s = sc(g);
            ai_bond_recapitalization(g, ui, rng, s);
        }
    }
    if sc(g) == 605 || sc(g) == 1200 {
        if !env.skips(0x4b9ef1) {
            bank_bailout_injection(g, ui, rng);
        }
    }
    if 130 < sc(g) && sc(g) < 1601 && g.industry_num[sc(g)] == 70 && g.entity_status[sc(g)] == 1 {
        // a holding company: liquidated (1 in 9), cuts a high dividend, or merges a subsidiary into itself
        if Ext::from_f64(8.0) < rng.rnd() * Ext::from_f64(9.0) {
            if !env.skips(0x51b258) {
                let s = sc(g);
                liquidate_corporation(g, ui, rng, s, 2, env.now);
            }
        } else if 10 < g.corp_controller[sc(g)] && g.tax_loss_carry[sc(g)] < -(Ext::from_f64(50.0) * g.exchange_rate) {
            let s = sc(g);
            if sf(1.4f32) < calc_dividend_yield(g, s) {
                g.dividend_per_share[s] = g.dividend_per_share[s] * (sf(0.3f32) * rng.rnd() + sf(0.6f32));
            }
        } else if 10 < g.corp_controller[sc(g)] {
            pick = rng.rnd_range(15, 1);
            let s = sc(g);
            if g.hold_stock_pct[(pick, s)] == 100 {
                sub_corp_id = cast::<i32, _>(g.hold_stock_id[(pick, s)]);
                if g.industry_num[sub_corp_id] == g.industry_num[s] {
                    let keep = (g.industry_num[sub_corp_id] == 70
                        && !(-(Ext::from_f64(10.0) * g.exchange_rate) <= g.tax_loss_carry[sub_corp_id]))
                        || (g.industry_num[sub_corp_id] == 70 && !(g.tax_credits[sub_corp_id] <= g.exchange_rate * Ext::from_f64(10.0)));
                    if !keep {
                        g.tmp_ext1 = Ext::from_f64(2.0);
                        if !env.skips(0x512aa9) {
                            liquidate_sub_into_parent(g, ui, rng, s, sub_corp_id, ts);
                        }
                    }
                } else if g.industry_num[sub_corp_id] == 70
                    && Ext::from_f64(0.0) <= g.tax_loss_carry[sub_corp_id]
                    && g.tax_credits[sub_corp_id] < g.exchange_rate * Ext::from_f64(5.0)
                {
                    g.tmp_ext1 = Ext::from_f64(2.0);
                    if !env.skips(0x512aa9) {
                        liquidate_sub_into_parent(g, ui, rng, s, sub_corp_id, ts);
                    }
                }
            }
        }
    }
    if (cast::<Ext, _>(g.base_interest_rate)) < sf(8.3f32) {
        if !env.skips(0x4f77b2) {
            ai_random_bond_issue(g, ui, rng);
        }
    }
    if 10 < sc(g) {
        if !env.skips(0x4ba8d5) {
            let s = sc(g);
            check_excess_loss_recapture(g, ui, s);
        }
    }
    if 130 < sc(g) && g.industry_num[sc(g)] == 70 && rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
        if !env.skips(0x4baad0) {
            ai_holding_co_merger_plan(g, ui, rng, ts);
        }
    }
    if sc(g) == 1548 && g.start_year + 3 < g.cur_year {
        if g.max_bond_issue == 0 {
            g.max_bond_issue = to_quad(g.exchange_rate * Ext::from_f64(150000.0));
        }
        g.max_bond_issue = to_quad(cast::<Ext, _>(g.max_bond_issue) * Ext::from_f64(1.014672));
        if g.max_stock_offering_amt == 0 {
            g.max_stock_offering_amt = to_quad(g.exchange_rate * Ext::from_f64(100000.0));
        }
        g.max_stock_offering_amt = to_quad(cast::<Ext, _>(g.max_stock_offering_amt) * Ext::from_f64(1.014672));
    }
    if sc(g) == 1595 {
        if !env.skips(0x4bc147) {
            asbestos_reserve_charge(g, ui, rng);
        }
    }
    if sc(g) == 1599 || g.active_corp_count == 0 {
        count_active_corps(g);
    }
    if (sc(g) == 700 || sc(g) == 1400) && !env.skips(0x482549) {
        tech_bubble_cycle(g, ui, rng);
    }
    if sc(g) == 220 || sc(g) == 780 || sc(g) == 1240 {
        if !env.skips(0x4bb0e6) {
            ai_dividend_cuts_covenants(g, ui, rng);
        }
    }
    if 75 < sc(g) && sc(g) % 75 == 1 {
        if -1 < g.short_pos_flag {
            find_short_positions(g, ui, rng, 0, 0);
        }
        recalc_commod_margin_balances(g, ui, rng, 0);
    }
    if sc(g) == 250 || sc(g) == 750 || sc(g) == 1250 {
        if !env.skips(0x5f2963) {
            process_expiring_options(g, ui, rng);
        }
        if !env.skips(0x6259d2) {
            settle_expired_futures_at(g, ui, rng, env.now);
        }
    }
    if sc(g) == 401 || sc(g) == 901 || sc(g) == 1401 {
        if !env.skips(0x5e319d) {
            force_insider_short_cover(g, ui, rng);
        }
    }
    if 10 < sc(g) && sc(g) < 1601 && g.corp_controller[sc(g)] < 11 && g.entity_status[sc(g)] == 1 {
        if !env.skips(0x5f92e9) {
            let s = sc(g);
            force_close_excess_naked_options(g, ui, rng, 0, s);
        }
    }
    if 275 < sc(g) && sc(g) % 275 == 1 && !env.skips(0x5f8e12) {
        force_regulated_call_sales(g, ui, rng);
    }
    if sc(g) == 1179 {
        if !env.skips(0x4bbfb1) {
            trim_recent_dividend_changes(g);
        }
    }
    if sc(g) == 0 {
        sub_corp_id = 11;
        while sub_corp_id <= 1600 {
            calc_stock_price(g, ui, rng, sub_corp_id, ts);
            sub_corp_id += 1;
        }
    }
    let tick = cast::<Ext, _>(sc(g));
    if (tick == Ext::from_f64(501.0) || tick == Ext::from_f64(1001.0) || tick == Ext::from_f64(1600.0)) && !env.skips(0x618537) {
        roll_price_history_month(g, ui, rng);
    }
    if 130 < sc(g) {
        if !env.skips(0x646a2d) {
            ai_create_swaps(g, ui, rng, ts);
        }
    }
    if sc(g) == 290 || sc(g) == 790 || sc(g) == 1290 {
        if !env.skips(0x64b0c9) {
            etf_compliance_check(g, ui, rng);
        }
    }
    if sc(g) == 1600 {
        if !env.skips(0x64c333) {
            commodity_etf_trading(g, ui, rng, ts);
        }
    }
    if sc(g) == 1584 && !env.skips(0x64cc22) {
        pick_covered_call_fund(g, ui, rng);
    }
    if sc(g) == 28 && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(3.0) {
        // labor unrest abroad (the country by year and the clock seconds)
        place_text = s("Vietnam");
        pick = ts;
        if pick % 3 == 1 {
            set(&mut place_text, __content.string(13482));
        }
        if pick % 3 == 2 {
            set(&mut place_text, "Laos");
        }
        if g.cur_year % 4 == 1 {
            set(&mut place_text, __content.string(6980));
            if pick % 3 == 1 {
                set(&mut place_text, __content.string(13506));
            }
            if pick % 3 == 2 {
                set(&mut place_text, __content.string(13507));
            }
        }
        if g.cur_year % 4 == 2 {
            set(&mut place_text, __content.string(13189));
            if pick % 4 == 1 {
                set(&mut place_text, __content.string(5400));
            }
            if pick % 4 == 2 {
                set(&mut place_text, "Ecuador");
            }
            if pick % 4 == 3 {
                set(&mut place_text, __content.string(13508));
            }
        }
        if g.cur_year % 4 == 3 {
            set(&mut place_text, "Serbia");
            if pick % 4 == 1 {
                set(&mut place_text, __content.string(13290));
            }
            if pick % 4 == 2 {
                set(&mut place_text, "Moldova");
            }
            if pick % 4 == 3 {
                set(&mut place_text, "Romania");
            }
        }
        g.news_msg = s(__content.string(13509)) + &place_text;
        if len(place_text.as_bytes()) < 9 {
            repl(&mut g.news_msg, "pay", "wages");
        }
        if 48 < pick {
            repl(&mut g.news_msg, "Unrest", __content.string(13510));
            if Ext::from_f64(2.0) < rng.rnd() * Ext::from_f64(3.0) {
                repl(&mut g.news_msg, "spreads", __content.string(13511));
            }
        } else if 36 < pick {
            repl(&mut g.news_msg, "Unrest", "Rioting");
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                repl(&mut g.news_msg, "spreads", __content.string(11135));
            }
        } else if 24 < pick {
            repl(&mut g.news_msg, __content.string(13512), __content.string(13513));
            if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                repl(&mut g.news_msg, "spread", __content.string(13514));
            }
        } else if 12 < pick {
            repl(&mut g.news_msg, __content.string(13512), __content.string(13515));
        }
        if pick % 5 == 4 {
            g.news_msg = s(__content.string(13516)) + &place_text;
        }
        if g.start_year == g.cur_year && g.cur_quarter == 1 {
            g.news_msg = PbStr::new();
        } else {
            post_news(g, ui, 0, 0);
        }
    }
    if (sc(g) == 805 || sc(g) == 1355) && !env.skips(0x63556a) {
        rebase_commodity_targets_from_history(g, rng);
    }
    if 0 < g.insider_probe_player && sc(g) % 370 == 210 && Ext::from_f64(4.0) < rng.rnd() * Ext::from_f64(5.0) {
        if !env.skips(0x67c24a) {
            insider_trading_probe_verdict(g, ui, rng);
        }
    }
    let tip = (cast::<Ext, _>(g.insider_earn_tip_corp)).abs();
    if Ext::from_f64(10.0) < tip && tip == cast::<Ext, _>(sc(g)) && cast::<i32, _>(g.insider_tip_event_qtr) != g.cur_quarter
        && !env.skips(0x67cbdd)
    {
        crate::logic::info_screens::insider::insider_earn_tip_event(g, ui, rng);
    }
    if sc(g) % 15 == 13 {
        if !env.skips(0x4bd8b4) {
            ai_corp_buys_stock(g, ui, rng, ts);
        }
    }
    if !env.skips(0x4bfbd3) {
        plan_market_cycle(g, ui, rng, ts);
    }
    if sc(g) == 850 {
        if !env.skips(0x64e961) {
            ai_etf_invest_cash(g, ui, rng, ts);
        }
    }
    if sc(g) == 73 {
        if !env.skips(0x64ee77) {
            revive_defunct_etfs(g, ui, rng, ts);
        }
    }
    if (502 < sc(g) && sc(g) < 570) || (1502 < sc(g) && sc(g) < 1570) {
        if !env.skips(0x68114c) {
            govt_antitrust_enforcement(g, ui, rng, ts);
        }
    }
    if 1585 < sc(g) {
        if !env.skips(0x64f724) {
            let s = sc(g);
            etf_sell_ineligible_holding(g, ui, rng, s);
        }
    }
    if (sc(g) == 11 || sc(g) == 511 || sc(g) == 1011) && !env.skips(0x4c3454) {
        market_reversal_event(g, ui, rng, ts, 0);
    }
    if sc(g) == g.market_reversal_tick && !env.skips(0x4c3454) {
        let t = g.market_reversal_tick;
        market_reversal_event(g, ui, rng, ts, t);
    }
    if (cast::<Ext, _>(g.market_aftershock_tick)).abs() == cast::<Ext, _>(sc(g)) && !env.skips(0x4c3d74) {
        market_aftershock(g, ui, rng);
    }
    if 70 < sc(g) {
        if sc(g) % 23 == 1 {
            if !env.skips(0x4c4033) {
                force_close_derivatives_bad_credit(g, ui, rng, 1);
            }
        }
        if sc(g) % 29 == 1 {
            if !env.skips(0x4c4033) {
                force_close_derivatives_bad_credit(g, ui, rng, 2);
            }
        }
    }
    if sc(g) == 1420 || (cast::<Ext, _>(g.fed_rate_shock_tick)).abs() == cast::<Ext, _>(sc(g)) {
        if !env.skips(0x647de7) {
            swap_rate_shock_event(g, ui, rng, ts);
        }
    }
    if 20 < sc(g) && sc(g) % 21 == 1 && !env.skips(0x6830fc) {
        check_price_alerts(g, ui, rng, 0);
    }
    if (sc(g) == 399 || sc(g) == 799 || sc(g) == 1099 || sc(g) == 1600) && !env.skips(0x684123) {
        expire_price_alerts(g, ui, rng);
    }
    if g.rand_seed % 10 == 1 {
        // a struggling company in a shortage industry raises its R&D / marketing spending
        pick = rng.rnd_range(1600, 131);
        if g.entity_status[pick] == 1
            && g.industry_num[pick] <= 69
            && Ext::from_f64(10.0) <= g.ind_demand_gap[cast::<i32, _>(g.industry_num[pick])]
            && get_managing_player(g, pick) <= 0
            && val(parse_any(g.ent_misc_data[pick].as_bytes(), b"|", 30).as_bytes()) <= Ext::from_f64(5.0)
            && g.disc_spend_pct[pick] <= 5
            && g.profit_margin[pick] <= -3
            && g.difficulty_level < g.credit_rating_num[pick]
        {
            let new_spend_pct = to_quad(rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(12.0));
            g.disc_spend_pct[pick] = cast::<i16, _>(new_spend_pct);
            watch_corp_id = get_industry_category(g.industry_num[pick]);
            place_text = s("R & D");
            if watch_corp_id == 2 {
                set(&mut place_text, __content.string(3515));
            }
            g.news_msg = s(__content.string(13517)) + &g.ent_symbol[pick] + __content.string(13518) + &place_text;
            post_news(g, ui, pick, 0);
        }
    }
    if 155 < sc(g) {
        if !env.skips(0x4c4e2c) {
            ai_tax_free_spinoff(g, ui, rng, ts);
        }
    }
    if 250 < sc(g) && sc(g) % 250 < 9 {
        if !env.skips(0x639a64) {
            enforce_commodity_position_limits(g, ui, rng);
        }
    }
    if 10 < g.pending_forced_conv_corp && g.pending_forced_conv_corp <= 1600 {
        if g.entity_status[g.pending_forced_conv_corp] != 1 {
            g.pending_forced_conv_corp = 0;
        } else {
            if Ext::from_f64(1000.0) < g.bond_price[g.pending_forced_conv_corp].get() && !env.skips(0x686c0f) {
                let c = g.pending_forced_conv_corp;
                convert_or_redeem_convertibles(g, ui, rng, c, 0, 1);
            }
            g.pending_forced_conv_corp = 0;
        }
    }
    if sc(g) % 203 == 202 {
        let mut entity_id = 1;
        while entity_id <= 1600 {
            balance_cash_t_bills(g, entity_id);
            if 11 <= entity_id && g.entity_status[entity_id] == 1 {
                if !env.skips(0x601c99) {
                    check_naked_option_margin(g, ui, rng, entity_id);
                }
                if g.corp_pct_not_corp_held[entity_id] <= 89 && !env.skips(0x481079) {
                    check_cross_ownership(g, ui, rng, entity_id, 0);
                }
            }
            entity_id += 1;
        }
    }
    if 10 < sc(g) && sc(g) < 86 && !env.skips(0x69c127) {
        let s = sc(g);
        bank_allocate_assets(g, ui, rng, s, 3, 0);
    }
    if sc(g) == 1600 {
        if !env.skips(0x4c572b) {
            accrue_gov_bond_interest(g);
        }
    }
    if (sc(g) == 610 || sc(g) == 1280) && !env.skips(0x4c5d0b) {
        gov_fiscal_policy_news(g, rng);
    }
    if sc(g) == 2 {
        if !env.skips(0x651df1) {
            etf_update_bonus_eligibility(g, ui);
        }
    }
    if sc(g) == 1 {
        if !env.skips(0x6520bf) {
            etf_pay_performance_bonus(g, ui, rng, ts);
        }
    }
    if sc(g) == 145 && g.cur_quarter == 1 {
        if !env.skips(0x6527da) {
            remind_etf_manager_duties(g, ui, rng);
        }
    }
    if 1480 < sc(g) && sc(g) < 1501 {
        if !env.skips(0x652b9a) {
            prompt_reduce_etf_fee(g, ui, rng);
        }
    }
    if sc(g) == 75 && g.cur_quarter == 1 && g.ai_hostile_merger_qtr < 0 {
        g.ai_hostile_merger_qtr = g.ai_hostile_merger_qtr + 1;
    }
    if g.start_year == g.cur_year && g.cur_quarter == 1 && sc(g) == 483 {
        if !env.skips(0x4c5fef) {
            rebase_stock_index_divisor(g, ui, rng);
        }
    }
    if sc(g) == 490 || sc(g) == 990 || sc(g) == 1590 {
        if !env.skips(0x4c6227) {
            amortize_loan_portfolios(g);
        }
    }
    // Bug (#110): the fiscal-policy news of ticks 610 / 1280 is run a second time.
    if (sc(g) == 610 || sc(g) == 1280) && !env.skips(0x4c5d0b) {
        gov_fiscal_policy_news(g, rng);
    }
    if sc(g) % 53 == 52 {
        if !env.skips(0x653409) {
            leveraged_etf_futures_update(g, ui, rng, env.now);
        }
    }
    if 1580 < sc(g) && sc(g) < 1586 {
        if !env.skips(0x652f2e) {
            index_etf_maintenance(g, ui, rng);
        }
    }
    if sc(g) == 25 || sc(g) == 525 || sc(g) == 1025 {
        if !env.skips(0x655197) {
            bond_etf_manage_portfolio(g, ui, rng, ts);
        }
    }
    if !env.skips(0x6a9490) {
        cash_flow_alert(g, ui, rng, ts);
    }
    if 100 < sc(g) && sc(g) % 101 == 1 && !env.skips(0x5469ca) {
        update_antitrust_reserves(g, ui, rng, 0);
    }
    if 130 < sc(g) {
        if !env.skips(0x4c67a5) {
            turnaround_news(g, rng);
        }
    }
}
