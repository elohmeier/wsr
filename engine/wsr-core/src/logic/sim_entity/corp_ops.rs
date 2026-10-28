//! Per-entity quarterly operations run from the UpdateEconomy GOSUB of the simulation
//! (`SimCorpQuarterOperations` with the bank asset allocation, government bond trades and credit-line
//! draws), plus the asbestos liability and the AI R&D/marketing boost of the AsbestosSuperfund GOSUB.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::comparison_to_empty,
    clippy::manual_clamp,
    clippy::neg_multiply,
    clippy::collapsible_match
)]
use crate::pb::real::{Ext, cast};


use super::fin_inst::ai_fin_inst_bond_trading;
#[allow(unused_imports)]
use super::prelude::*;
use super::transactions::sell_stock_holding;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{
    balance_cash_t_bills, calc_accrued_bond_interest, calc_bank_deposit_cost,
};
use crate::logic::finance::{
    calc_entity_financials, get_bank_cd_rate, get_controller_type, get_industry_category,
    get_managing_player, get_market_interest_rate,
};
use crate::logic::news::is_in_recent_headlines;
use crate::logic::report::show_quick_message;
use crate::logic::research::bank_regulator_lending_check;
use crate::logic::shorts::find_short_positions;
use crate::logic::valuation::calc_industry_outlook;

/// Port of SimCorpQuarterOperations (0x4fc8a9).
/// One quarter of operations of `entity_id` (a SUB; nothing is returned). Operating corps (131..1600 with
/// business assets): operating profit from the margin (field 30 of `EntMiscData` or the industry outlook),
/// AI growth/R&D adjustments, R&D tax credits. Everybody: T-bill interest, bank-loan interest to the
/// lender, interest on player advances. Banks (11..85): emergency loans, loan/securities income, CD and
/// deposit costs, deposit growth and mix, regulators' credit freeze and largest-loan reduction. Insurers:
/// underwriting profit, reserve growth. Entities <= 130: `BankAllocateAssets` (mode 3, then 2 for a
/// lopsided plan), and very cash-rich AI financial institutions buy government bonds or trade bonds.
pub fn sim_corp_quarter_operations(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
) {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let e = entity_id;
    if 1 > e || g.entity_status[e] != 1 {
        return;
    }
    let mut disc_spend: i32;
    let mut margin_str = PbStr::new();
    let mut disc_spend_str = PbStr::new();
    let mut cust_deposits: Ext = Ext::from_f64(0.0);
    let mut rate_tmp: Ext;
    if 130 < e && g.business_assets[e] > sf(0.01f32) {
        if 20 < len(g.ent_misc_data[e].as_bytes()) {
            margin_str = parse_any(g.ent_misc_data[e].as_bytes(), b"|", 30);
        }
        let margin_pct: f32;
        if margin_str == "" {
            let ind = g.industry_num[e];
            calc_industry_outlook(g, rng, ind);
            rate_tmp = g.tmp_ext1;
            if Ext::from_f64(50.0) < rate_tmp {
                rate_tmp = Ext::from_f64(50.0);
            } else if rate_tmp < -Ext::from_f64(20.0) {
                rate_tmp = -Ext::from_f64(20.0);
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            rate_tmp = cast::<Ext, _>(g.profit_margin[e]) + rate_tmp;
            rate_tmp = rate_tmp - cast::<Ext, _>(g.disc_spend_pct[e]);
            margin_pct = cast::<f32, _>(sf(0.01f32) * rate_tmp);
            disc_spend = cast::<i32, _>(g.disc_spend_pct[e]);
        } else {
            margin_pct = cast::<f32, _>(sf(0.01f32) * val(margin_str.as_bytes()));
            let mp = cast::<Ext, _>(margin_pct);
            if get_managing_player(g, e) == 0 {
                'adj: {
                    if sf(0.05f32) > mp && !(7 <= g.credit_rating_num[e]) {
                        if -4 < g.growth_rate[e] {
                            g.growth_rate[e] =
                                cast::<i16, _>(to_quad(cast::<Ext, _>(g.growth_rate[e]) - (sf(3.5f32) * rng.rnd() + Ext::from_f64(1.0))));
                        }
                        break 'adj;
                    }
                    if sf(0.1f32) > mp && 7 > g.credit_rating_num[e] {
                        if 0 < g.growth_rate[e] {
                            g.growth_rate[e] =
                                cast::<i16, _>(to_quad(cast::<Ext, _>(g.growth_rate[e]) - sf(3.2f32) * rng.rnd()));
                        }
                        break 'adj;
                    }
                    if sf(0.03f32) * rng.rnd() + sf(0.1f32) > mp && 7 > g.credit_rating_num[e] {
                        g.growth_rate[e] = g.growth_rate[e].wrapping_sub(1);
                        break 'adj;
                    }
                    if sf(0.05f32) * rng.rnd() + sf(0.2f32) < mp && 0 < g.monetary_stance {
                        g.growth_rate[e] =
                            cast::<i16, _>(to_quad((Ext::from_f64(1.0) + cast::<Ext, _>(g.growth_rate[e])) + sf(3.2f32) * rng.rnd()));
                    }
                }
                if get_controller_type(g, e) == 1 {
                    let thr = cast::<i32, _>(g.player_growth_throttle[g.corp_controller[e]]);
                    if thr < cast::<i32, _>(g.growth_rate[e]) {
                        disc_spend = cast::<i32, _>(g.growth_rate[e]) - thr;
                        if disc_spend > 5 {
                            disc_spend = to_long(cast::<Ext, _>(disc_spend) / Ext::from_f64(3.0));
                            g.growth_rate[e] = cast::<i16, _>(cast::<i32, _>(g.growth_rate[e]) - disc_spend);
                        } else if disc_spend > 0 {
                            g.growth_rate[e] = g.growth_rate[e].wrapping_sub(1);
                        }
                    }
                }
            }
            disc_spend_str = parse_any(g.ent_misc_data[e].as_bytes(), b"|", 29);
            disc_spend = to_long(val(disc_spend_str.as_bytes()));
        }
        let op_profit = cast::<f32, _>((cast::<Ext, _>(margin_pct) * g.business_assets[e]) * sf(0.04f32));
        g.accrued_pl[e] = cast::<Ext, _>(op_profit) + g.accrued_pl[e];
        g.cash[e] = cast::<Ext, _>(op_profit) + g.cash[e];
        let industry_id = g.industry_num[e];
        if disc_spend > 10 && get_industry_category(industry_id) == 1 {
            disc_spend -= 10;
            if disc_spend > 0 {
                let mortgage_income =
                    ((sf(0.01f32) * cast::<Ext, _>(disc_spend)) * g.business_assets[e]) * sf(0.2f32);
                let tax_credit = sf(0.05f32) * mortgage_income;
                g.tax_credits[e] = tax_credit + g.tax_credits[e];
            }
        }
        if 10 < g.corp_controller[e] {
            balance_cash_t_bills(g, e);
            if g.cash[e] < -Ext::from_f64(100.0) {
                if Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(5.0) < cast::<Ext, _>(g.disc_spend_pct[e]) {
                    g.disc_spend_pct[e] =
                        cast::<i16, _>(cast::<i32, _>(g.disc_spend_pct[e]) - (g.rand_seed % 2 + 1));
                }
                if Ext::from_f64(10.0) * rng.rnd() < cast::<Ext, _>(g.growth_rate[e]) {
                    g.growth_rate[e] = cast::<i16, _>(cast::<i32, _>(g.growth_rate[e]) - (g.rand_seed % 3 + 1));
                }
            }
            if val(margin_str.as_bytes()) > Ext::from_f64(0.0)
                && val(margin_str.as_bytes()) - val(disc_spend_str.as_bytes()) > Ext::from_f64(28.0)
                && Ext::from_f64(5.0) * rng.rnd() + Ext::from_f64(8.0) < cast::<Ext, _>(g.disc_spend_pct[e])
            {
                g.disc_spend_pct[e] = cast::<i16, _>(to_quad(sf(0.8f32) * cast::<Ext, _>(g.disc_spend_pct[e])));
            }
        }
    }
    calc_entity_financials(g, ui, rng, e);
    balance_cash_t_bills(g, e);
    rate_tmp = sf(0.0025f32) * cast::<Ext, _>(g.t_bill_rate);
    let mut tbill_interest = (g.t_bills[e] * rate_tmp) * sf(0.2f32);
    if !(11 > e) && 85 >= e {
        cust_deposits = g.bank_cust_deposits[e];
        if Ext::from_f64(0.0) > cust_deposits {
            cust_deposits = Ext::from_f64(0.0);
        }
    }
    g.accrued_pl[e] = tbill_interest + g.accrued_pl[e];
    g.t_bills[e] = tbill_interest + g.t_bills[e];
    if 10 < e && e < 86 && g.cash[e] < Ext::from_f64(0.0) && !(g.t_bills[e] > Ext::from_f64(0.0)) {
        rate_tmp = g.cash[e].abs();
        if rate_tmp > Ext::from_f64(2000.0) - g.bank_debt[e] {
            rate_tmp = Ext::from_f64(2000.0) - g.bank_debt[e];
        }
        g.bank_debt[e] = rate_tmp + g.bank_debt[e];
        g.cash[e] = rate_tmp + g.cash[e];
        if 11 < e {
            let lb = cast::<i32, _>(g.lending_bank[e]);
            if !(1..=1600).contains(&lb) {
                // Cash(LendingBank): error 9 -> ON ERROR handler (ShowRuntimeError) and exit
                return;
            }
            g.cash[lb] = g.cash[lb] - rate_tmp;
        }
    }
    let mut interest_cost: Ext;
    if g.bank_debt[e] > Ext::from_f64(0.0) {
        rate_tmp = cast::<Ext, _>(g.borrow_rate);
        if 10 < e && g.industry_num[e] == 1 {
            rate_tmp = cast::<Ext, _>(g.entity_borrow_rate);
        }
        interest_cost = (sf(0.0005f32) * rate_tmp) * g.bank_debt[e];
        g.accrued_pl[e] = g.accrued_pl[e] - interest_cost;
        let mut pays_cash = 1;
        if g.credit_rating_str == "D" && interest_cost > g.t_bills[e] + g.cash[e] && !(85 >= e) {
            pays_cash = 0;
        }
        if pays_cash == 0 {
            g.bank_debt[e] = interest_cost + g.bank_debt[e];
        } else {
            g.cash[e] = g.cash[e] - interest_cost;
        }
        g.lender_id = cast::<i32, _>(g.lending_bank[e]);
        if 10 < g.lender_id {
            let l = g.lender_id;
            g.accrued_pl[l] = interest_cost + g.accrued_pl[l];
            if pays_cash == 1 {
                g.cash[l] = interest_cost + g.cash[l];
            }
        }
    }
    if 10 < e && g.tmp_player_advances != Ext::from_f64(0.0) {
        interest_cost = (sf(0.0005f32) * cast::<Ext, _>(g.prime_rate)) * g.tmp_player_advances;
        g.accrued_pl[e] = g.accrued_pl[e] - interest_cost;
        let mut pays_cash = 1;
        if g.credit_rating_str == "D" {
            pays_cash = 0;
        }
        if pays_cash == 1 {
            g.cash[e] = g.cash[e] - interest_cost;
        }
        let n = g.num_players;
        for p in 1..=n {
            if g.player_advances[(p, e)] != Ext::from_f64(0.0) {
                tbill_interest = (sf(0.0005f32) * cast::<Ext, _>(g.prime_rate)) * g.player_advances[(p, e)];
                g.accrued_pl[p] = tbill_interest + g.accrued_pl[p];
                if pays_cash == 1 {
                    g.cash[p] = tbill_interest + g.cash[p];
                } else {
                    g.player_advances[(p, e)] = tbill_interest + g.player_advances[(p, e)];
                }
            }
        }
    }
    if 130 < e {
        return;
    }
    if 10 < e && g.industry_num[e] == 1 {
        bank_operations(g, ui, rng, e, cust_deposits);
    }
    if 10 < e && g.industry_num[e] == 2 {
        let ds = cast::<i32, _>(g.growth_rate[e]);
        let mut mi = -Ext::from_f64(1.0) * (sf(0.7f32) * cast::<Ext, _>(ds));
        mi = (mi + cast::<Ext, _>(g.profit_margin[e]))
            - ((Ext::from_f64(1.0) + cast::<Ext, _>(g.disc_spend_pct[e])) + cast::<Ext, _>(g.difficulty_level));
        rate_tmp = (g.ins_reserves[e] * (sf(0.01f32) * mi)) * sf(0.05f32);
        g.insur_uw_profit_ytd[e] = rate_tmp + g.insur_uw_profit_ytd[e];
        g.accrued_pl[e] = rate_tmp + g.accrued_pl[e];
        g.cash[e] = rate_tmp + g.cash[e];
        let mut secs_income = Ext::from_f64(0.0);
        if g.subprime_secs[e] > 0 {
            rate_tmp = sf(0.0005f32) * get_market_interest_rate(g, rng, 3);
            secs_income = rate_tmp * cast::<Ext, _>(g.subprime_secs[e]);
        }
        g.accrued_pl[e] = secs_income + g.accrued_pl[e];
        g.cash[e] = secs_income + g.cash[e];
        rate_tmp = (g.ins_reserves[e] * (sf(0.01f32) * cast::<Ext, _>(g.growth_rate[e]))) * sf(0.05f32);
        g.cash[e] = rate_tmp + g.cash[e];
        g.ins_reserves[e] = rate_tmp + g.ins_reserves[e];
    }
    if bank_allocate_assets(g, ui, rng, e, 3, 0) == 1 && get_controller_type(g, e) != 1 {
        bank_allocate_assets(g, ui, rng, e, 2, 0);
    }
    if 10 < e && e < 131 {
        if g.t_bills[e] + g.cash[e] > g.exchange_rate * Ext::from_f64(110000.0)
            && get_managing_player(g, e) == 0
            && !(sf(5.7f32) >= cast::<Ext, _>(g.base_interest_rate))
        {
            let choice = rng.rnd_range(5, 1);
            let bond_cost = to_quad((g.t_bills[e] + g.cash[e]) * sf(0.06f32));
            if choice == 1 || choice == 2 {
                if 85 < e {
                    let max_bond_trade = check_govt_bond_large_trade(g, ui, 7, 0);
                    let cur_bond_trade = check_govt_bond_large_trade(g, ui, 8, 0);
                    if !(max_bond_trade * sf(0.9f32) >= cur_bond_trade + cast::<Ext, _>(bond_cost)) {
                        return;
                    }
                    if choice == 1 {
                        let bond_face = to_quad(
                            sf(0.999f32) * (cast::<Ext, _>(bond_cost) / (cast::<Ext, _>(g.long_bond_price) * sf(0.01f32))),
                        );
                        g.long_gov_bond_face[e] =
                            to_quad(cast::<Ext, _>(bond_face) + cast::<Ext, _>(g.long_gov_bond_face[e]));
                        g.lt_gov_bond_cost[e] = cast::<Ext, _>(bond_cost) + g.lt_gov_bond_cost[e];
                        let accrued_int = calc_accrued_bond_interest(g, bond_face, 1);
                        g.accrued_pl[e] = g.accrued_pl[e] - accrued_int;
                    } else {
                        let bond_face = to_quad(
                            sf(0.999f32) * (cast::<Ext, _>(bond_cost) / (cast::<Ext, _>(g.short_bond_price) * sf(0.01f32))),
                        );
                        g.short_gov_bond_face[e] =
                            to_quad(cast::<Ext, _>(bond_face) + cast::<Ext, _>(g.short_gov_bond_face[e]));
                        g.st_gov_bond_cost[e] = cast::<Ext, _>(bond_cost) + g.st_gov_bond_cost[e];
                        let accrued_int = calc_accrued_bond_interest(g, bond_face, 2);
                        g.accrued_pl[e] = g.accrued_pl[e] - accrued_int;
                    }
                    g.cash[e] = g.cash[e] - cast::<Ext, _>(bond_cost);
                }
            } else {
                // choice 3, 4 or 5
                ai_fin_inst_bond_trading(g, ui, rng, e);
            }
        }
    }
}

/// The bank block of SimCorpQuarterOperations (entity 11..85 with industry 1).
fn bank_operations(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, e: i32, cust_deposits: Ext) {
    let __content = crate::content::current();
    let mut rate_tmp = sf(0.0005f32) * get_market_interest_rate(g, rng, 1);
    let mut consumer_income = rate_tmp * cast::<Ext, _>(g.bank_consumer_loans[e]);
    rate_tmp = sf(0.0005f32) * get_market_interest_rate(g, rng, 2);
    let mut mortgage_income = rate_tmp * cast::<Ext, _>(g.bank_mortgage_loans[e]);
    let mut secs_income = Ext::from_f64(0.0);
    if g.subprime_secs[e] > 0 {
        rate_tmp = sf(0.0005f32) * get_market_interest_rate(g, rng, 3);
        secs_income = rate_tmp * cast::<Ext, _>(g.subprime_secs[e]);
    }
    rate_tmp = (mortgage_income + consumer_income) + secs_income;
    g.accrued_pl[e] = rate_tmp + g.accrued_pl[e];
    g.cash[e] = rate_tmp + g.cash[e];
    rate_tmp = sf(0.0005f32) * get_bank_cd_rate(g, e);
    let cd_interest = g.bank_c_ds[e] * rate_tmp;
    g.accrued_pl[e] = g.accrued_pl[e] - cd_interest;
    g.cash[e] = g.cash[e] - cd_interest;
    secs_income = calc_bank_deposit_cost(g, e);
    secs_income = sf(0.2f32) * secs_income;
    g.accrued_pl[e] = g.accrued_pl[e] - secs_income;
    g.cash[e] = g.cash[e] - secs_income;
    let mut capital_ratio: Ext = (g.net_worth + g.bad_debt_reserve[e])
        / ((g.bank_demand_deposits[e] + g.bank_c_ds[e]) + cust_deposits);
    capital_ratio = Ext::from_f64(100.0) * capital_ratio;
    if g.growth_rate[e] != 0 {
        let gr: Ext = crate::pb::numeric::double(cast::<Ext, _>(g.growth_rate[e]) * sf(0.0005f32)); // DOUBLE temp
        rate_tmp = (g.bank_demand_deposits[e] + g.bank_c_ds[e]) * gr;
        consumer_income = (sf(0.01f32)
            * ((Ext::from_f64(95.0) - sf(1.1f32) * cast::<Ext, _>(g.base_interest_rate))
                - cast::<Ext, _>(g.credit_rating_num[e]) * sf(0.4f32)))
            * rate_tmp;
        if consumer_income.abs() > rate_tmp.abs() * sf(0.95f32) {
            consumer_income = rate_tmp * sf(0.95f32);
        }
        if rate_tmp > Ext::from_f64(0.0) {
            mortgage_income = rate_tmp - consumer_income;
        } else {
            mortgage_income = -Ext::from_f64(1.0) * (rate_tmp.abs() - consumer_income.abs());
        }
        g.cash[e] = rate_tmp + g.cash[e];
        g.bank_c_ds[e] = consumer_income + g.bank_c_ds[e];
        g.bank_demand_deposits[e] = mortgage_income + g.bank_demand_deposits[e];
        if g.bank_debt[e] > g.exchange_rate * Ext::from_f64(1000.0) {
            consumer_income = g.bank_debt[e] * sf(0.2f32);
            g.cash[e] = consumer_income + g.cash[e];
            g.bank_c_ds[e] = consumer_income + g.bank_c_ds[e];
        }
    }
    if (g.cur_quarter + g.cur_year + e) % 5 == 1 {
        if get_controller_type(g, e) != 1 && g.sim_co_count > 1400 {
            if sf(3.2f32) > capital_ratio {
                g.growth_rate[e] = cast::<i16, _>(rng.rnd_range(4, 0) - 5);
                if g.profit_margin[e] < 0 && g.disc_spend_pct[e] < 10 {
                    g.disc_spend_pct[e] = cast::<i16, _>(rng.rnd_range(8, 1) + cast::<i32, _>(g.disc_spend_pct[e]));
                }
            } else if Ext::from_f64(15.0) >= capital_ratio {
                g.growth_rate[e] = cast::<i16, _>(rng.rnd_range(8, 0) - 1);
            } else {
                g.growth_rate[e] = cast::<i16, _>(rng.rnd_range(4, 1) + 6);
                if g.profit_margin[e] > 7 && g.disc_spend_pct[e] > 6 {
                    g.disc_spend_pct[e] = cast::<i16, _>(to_quad(sf(0.6f32) * cast::<Ext, _>(g.disc_spend_pct[e])));
                }
            }
        }
        let total = g.bank_demand_deposits[e] + g.bank_c_ds[e];
        if total > Ext::from_f64(0.0) && g.bank_c_ds[e] > Ext::from_f64(0.0) && g.bank_demand_deposits[e] > Ext::from_f64(0.0) {
            let shift = total * sf(0.01f32);
            if g.bank_c_ds[e] / total < sf(0.9f32) {
                g.bank_c_ds[e] = shift + g.bank_c_ds[e];
                g.bank_demand_deposits[e] = g.bank_demand_deposits[e] - shift;
            } else {
                g.bank_c_ds[e] = g.bank_c_ds[e] - shift;
                g.bank_demand_deposits[e] = shift + g.bank_demand_deposits[e];
            }
        }
    }
    if g.credit_frozen[e] == 1
        && get_controller_type(g, e) != 1
        && bank_regulator_lending_check(g, ui, e, 1) == 0
    {
        g.credit_frozen[e] = 0;
    }
    if 0 < g.bank_largest_debtor[e] {
        let d0 = g.bank_largest_debtor[e];
        if g.bank_debt[d0] > g.exchange_rate * Ext::from_f64(10000.0) {
            let debtor_id = g.bank_largest_debtor[e];
            rate_tmp = g.bank_debt[debtor_id] * sf(0.05f32);
            if g.bank_debt[e] > Ext::from_f64(0.0) {
                rate_tmp = Ext::from_f64(2.0) * rate_tmp;
            }
            g.bank_debt[debtor_id] = g.bank_debt[debtor_id] - rate_tmp;
            g.cash[debtor_id] = g.cash[debtor_id] - rate_tmp;
            g.cash[e] = rate_tmp + g.cash[e];
            if debtor_id > 85
                && g.cur_player == g.corp_controller[debtor_id]
                && g.active_entity == debtor_id
                && g.sim_co_count > 1250
            {
                let mut who = PbStr::from(__content.string(10767));
                if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                    who = PbStr::from(__content.string(10768));
                }
                g.news_msg = who
                    + " order "
                    + &g.ent_symbol[e]
                    + __content.string(10769)
                    + &g.ent_symbol[debtor_id];
                post_news(g, ui, debtor_id, 0);
            }
        }
    }
}

/// Port of BankAllocateAssets (0x69c127) with its GOSUBs BankAllocate_SellLosingStocks (0x69ee25) and
/// BankAllocateAssets_Exit (0x69f07c).
/// Asset allocation of the bank `bank_ref` (11..85, not switched off with `BankAllocOff`). For AI-run
/// banks after the first year: shifts 5 points between bonds and prime mortgages by interest rates and
/// bond losses, and sells a losing stock (`SellStockHolding`) if subsidiaries' stock is > 10% of net
/// worth. `alloc_mode` 1: recapitalises a bank in trouble and sets deposits for a 7..11% (or 11..15%)
/// capital ratio and a new growth rate; modes 1, 2: a new random target allocation (prime mortgages,
/// subprime securities, consumer loans, bonds, cash) adjusted to sum to 100 (popup "under new
/// management" while a game is running); mode 3: up to five passes moving 12% of the cash into the most
/// under-allocated category or selling 9% of an over-allocated one (government bonds through
/// [`trade_gov_bonds`], corporate bonds at 99.5% of their price), then balances cash/T-bills. Returns 1 if
/// some target percentage exceeds 51, else 0. `force_bond_sale` <> 0 sells bonds even at high rates.
pub fn bank_allocate_assets(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    bank_ref: i32,
    alloc_mode: i32,
    force_bond_sale: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError(.., debugPart) (error path not ported)
    let mut result = 0;
    let b = bank_ref;
    if b < 11 || b > 85 {
        return result;
    }
    if g.entity_status[b] != 1 && alloc_mode == 3 {
        return result;
    }
    if 10 < g.corp_controller[b] {
        g.bank_alloc_off[b] = 0;
    }
    if g.bank_alloc_off[b] != 0 {
        return result;
    }
    calc_entity_financials(g, ui, rng, b);
    if get_controller_type(g, b) != 1 && g.start_year < g.cur_year {
        if 5 <= g.credit_rating_num[b]
            || g.bond_unrealized_loss >= Ext::from_f64(0.0)
            || g.bond_unrealized_loss.abs()
                <= (g.tmp_corp_bonds_value + g.tmp_gov_bonds_value) * sf(0.1f32)
        {
            if sf(9.5f32) < cast::<Ext, _>(g.base_interest_rate) && g.bank_alloc_prime_mort_pct[b] > 10 {
                g.bank_alloc_bonds_pct[b] = g.bank_alloc_bonds_pct[b].wrapping_add(5);
                g.bank_alloc_prime_mort_pct[b] = g.bank_alloc_prime_mort_pct[b].wrapping_sub(5);
            }
        } else if g.bank_alloc_bonds_pct[b] > 5 && sf(7.5f32) > cast::<Ext, _>(g.long_bond_yield) {
            g.bank_alloc_bonds_pct[b] = g.bank_alloc_bonds_pct[b].wrapping_sub(5);
            g.bank_alloc_prime_mort_pct[b] = g.bank_alloc_prime_mort_pct[b].wrapping_add(5);
        }
        if g.tmp_subsid_stock_value > Ext::from_f64(0.0)
            && g.tmp_subsid_stock_value > g.net_worth * sf(0.1f32)
            && g.net_worth > Ext::from_f64(0.0)
        {
            bank_allocate_sell_losing_stocks(g, ui, rng, b);
        }
    }
    if alloc_mode == 1 && g.entity_status[b] == 1 {
        let equity = g.bad_debt_reserve[b] + g.net_worth;
        if equity < Ext::from_f64(0.0) {
            g.cash[b] = ((equity.abs() + g.cash[b]) + g.exchange_rate * Ext::from_f64(10000.0))
                + g.exchange_rate * (Ext::from_f64(5000.0) * rng.rnd());
            calc_entity_financials(g, ui, rng, b);
        }
        let mut pct = to_long(Ext::from_f64(8.0) * rng.rnd() + Ext::from_f64(3.0));
        if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
            pct += 4;
        }
        let capital_ratio = cast::<f32, _>(cast::<Ext, _>(pct) * sf(0.01f32));
        let target_deposits = equity / cast::<Ext, _>(capital_ratio);
        let shortfall = target_deposits
            - ((g.bank_demand_deposits[b] + g.bank_c_ds[b]) + g.bank_cust_deposits[b]);
        if shortfall > Ext::from_f64(0.0) {
            g.cash[b] = shortfall + g.cash[b];
            g.bank_c_ds[b] = shortfall * sf(0.89f32) + g.bank_c_ds[b];
            g.bank_demand_deposits[b] = shortfall * sf(0.11f32) + g.bank_demand_deposits[b];
            g.bad_debt_reserve[b] = shortfall * sf(0.008f32) + g.bad_debt_reserve[b];
        }
        g.growth_rate[b] = cast::<i16, _>(rng.rnd_range(7, 1));
    }
    if 3 > alloc_mode {
        if g.game_in_progress == 1 {
            let mut msg = g.ent_name[b].clone()
                + __content.string(10770);
            show_quick_message(g, ui, &mut msg);
        }
        g.bank_alloc_prime_mort_pct[b] = cast::<i16, _>(to_quad(Ext::from_f64(11.0) * rng.rnd() + Ext::from_f64(25.0)));
        g.bank_alloc_subprime_pct[b] = cast::<i16, _>(to_quad(Ext::from_f64(11.0) * rng.rnd() + Ext::from_f64(10.0)));
        g.bank_alloc_consumer_pct[b] = cast::<i16, _>(to_quad(Ext::from_f64(11.0) * rng.rnd() + Ext::from_f64(20.0)));
        g.bank_alloc_bonds_pct[b] = cast::<i16, _>(to_quad(Ext::from_f64(11.0) * rng.rnd() + Ext::from_f64(15.0)));
        g.bank_alloc_cash_pct[b] = cast::<i16, _>(to_quad(Ext::from_f64(2.0) * rng.rnd() + sf(9.6f32)));
        let pct_sum = cast::<i16, _>(cast::<i32, _>(g.bank_alloc_subprime_pct[b])
            + cast::<i32, _>(g.bank_alloc_prime_mort_pct[b])
            + cast::<i32, _>(g.bank_alloc_consumer_pct[b])
            + cast::<i32, _>(g.bank_alloc_bonds_pct[b])
            + cast::<i32, _>(g.bank_alloc_cash_pct[b]));
        let mut pct_excess = cast::<i16, _>(cast::<i32, _>(pct_sum) - 100);
        let mut adj_iter = 0;
        loop {
            if pct_excess == 0 {
                break;
            }
            adj_iter += 1;
            let chance = |rng: &mut Rng| Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0);
            if pct_excess > 0 {
                if g.bank_alloc_prime_mort_pct[b] > 29 && chance(rng) {
                    g.bank_alloc_prime_mort_pct[b] -= 1;
                    pct_excess -= 1;
                } else if g.bank_alloc_subprime_pct[b] > 14 && chance(rng) {
                    g.bank_alloc_subprime_pct[b] -= 1;
                    pct_excess -= 1;
                } else if g.bank_alloc_consumer_pct[b] > 24 && chance(rng) {
                    g.bank_alloc_consumer_pct[b] -= 1;
                    pct_excess -= 1;
                } else if g.bank_alloc_bonds_pct[b] > 19 && chance(rng) {
                    g.bank_alloc_bonds_pct[b] -= 1;
                    pct_excess -= 1;
                } else if g.bank_alloc_cash_pct[b] > 10 && chance(rng) {
                    g.bank_alloc_cash_pct[b] -= 1;
                    pct_excess -= 1;
                }
            } else if pct_excess < 0 {
                if g.bank_alloc_prime_mort_pct[b] < 31 && chance(rng) {
                    g.bank_alloc_prime_mort_pct[b] += 1;
                    pct_excess += 1;
                } else if g.bank_alloc_subprime_pct[b] < 16 && chance(rng) {
                    g.bank_alloc_subprime_pct[b] += 1;
                    pct_excess += 1;
                } else if g.bank_alloc_consumer_pct[b] < 26 && chance(rng) {
                    g.bank_alloc_consumer_pct[b] += 1;
                    pct_excess += 1;
                } else if g.bank_alloc_bonds_pct[b] < 21 && chance(rng) {
                    g.bank_alloc_bonds_pct[b] += 1;
                    pct_excess += 1;
                } else if g.bank_alloc_cash_pct[b] < 11 && chance(rng) {
                    g.bank_alloc_cash_pct[b] += 1;
                    pct_excess += 1;
                }
            }
            if !(adj_iter <= 50) {
                break;
            }
        }
    }
    if alloc_mode != 3 {
        return result;
    }
    let mut asset_amt = [Ext::from_f64(0.0f64); 6];
    let mut actual_pct = [0i16; 6];
    for pass in 1..=5 {
        if pass > 1 {
            calc_entity_financials(g, ui, rng, b);
        }
        asset_amt[1] = cast::<Ext, _>(g.bank_mortgage_loans[b]);
        asset_amt[2] = cast::<Ext, _>(g.subprime_secs[b]);
        asset_amt[3] = cast::<Ext, _>(g.bank_consumer_loans[b]);
        asset_amt[4] = g.tmp_corp_bonds_value + g.tmp_gov_bonds_value;
        asset_amt[5] = g.t_bills[b] + g.cash[b];
        let mut total_assets = Ext::from_f64(0.0);
        for amt in asset_amt.iter().skip(1) {
            total_assets = *amt + total_assets;
        }
        if !(total_assets >= g.exchange_rate * Ext::from_f64(500.0)) {
            return result;
        }
        let mut pct_sum: i16 = 0;
        for cat in 1..=5 {
            actual_pct[cat] = cast::<i16, _>(to_quad((asset_amt[cat] / total_assets) * Ext::from_f64(100.0)));
            pct_sum = cast::<i16, _>(cast::<i32, _>(actual_pct[cat]) + cast::<i32, _>(pct_sum));
        }
        if pct_sum < 100 {
            actual_pct[1] = actual_pct[1].wrapping_add(1);
        }
        if pct_sum > 100 && actual_pct[1] > 0 {
            actual_pct[1] -= 1;
        }
        let mut over: i32 = 0;
        let mut under: i32 = 0;
        // prime mortgages
        let p = cast::<i32, _>(g.bank_alloc_prime_mort_pct[b]);
        if p > 51 {
            result = 1;
        }
        if cast::<i32, _>(actual_pct[1]) > p + 2 || p == 0 {
            if g.bank_mortgage_loans[b] > 0 {
                over = 1;
            }
        } else if (cast::<i32, _>(actual_pct[1])) < p - 2 && p > 0 {
            under = 1;
        }
        // subprime securities
        let p = cast::<i32, _>(g.bank_alloc_subprime_pct[b]);
        if p > 51 {
            result = 1;
        }
        if cast::<i32, _>(actual_pct[2]) > p + 2 || p == 0 {
            if !(g.subprime_crisis_state > 0)
                && g.subprime_crisis_state != -2
                && over == 0
                && g.subprime_secs[b] > 0
            {
                over = 2;
            }
        } else if (cast::<i32, _>(actual_pct[2])) < p - 2 && p > 0 {
            under = 2;
        }
        // consumer loans
        let p = cast::<i32, _>(g.bank_alloc_consumer_pct[b]);
        if p > 51 {
            result = 1;
        }
        if cast::<i32, _>(actual_pct[3]) > p + 2 || p == 0 {
            if over == 0 && g.bank_consumer_loans[b] > 0 {
                over = 3;
            }
        } else if (cast::<i32, _>(actual_pct[3])) < p - 2 && p > 0 {
            under = 3;
        }
        // bonds
        let p = cast::<i32, _>(g.bank_alloc_bonds_pct[b]);
        if p > 51 {
            result = 1;
        }
        if cast::<i32, _>(actual_pct[4]) > p + 2 || p == 0 {
            if !(Ext::from_f64(12.0) < cast::<Ext, _>(g.base_interest_rate) && p > 0) && over == 0 && asset_amt[4] > Ext::from_f64(0.0) {
                over = 4;
            }
        } else if (cast::<i32, _>(actual_pct[4])) < p - 2
            && p > 0
            && !(sf(4.5f32) > cast::<Ext, _>(g.base_interest_rate))
        {
            under = 4;
        }
        // cash
        let p = cast::<i32, _>(g.bank_alloc_cash_pct[b]);
        if p > 51 {
            result = 1;
        }
        if cast::<i32, _>(actual_pct[5]) > p + 2 {
            over = 5;
        } else if (cast::<i32, _>(actual_pct[5])) < p - 2 {
            under = 5;
        }
        if over == 0 && under == 0 {
            return result;
        }
        if over == 0 && 0 < under && under < 5 && actual_pct[5] > 5 {
            over = 5;
        }
        if over == 5 {
            let mut invest = asset_amt[5] * sf(0.12f32);
            if pass == 4 && under == 0 {
                under = 4;
                invest = invest * sf(0.4f32);
            }
            if bank_regulator_lending_check(g, ui, b, 0) == 1 && under < 4 {
                continue;
            }
            match under {
                0 => {
                    invest = invest * sf(0.5f32);
                    if pass == 1 {
                        if g.bank_alloc_prime_mort_pct[b] > 0 {
                            g.bank_mortgage_loans[b] =
                                to_quad(invest + cast::<Ext, _>(g.bank_mortgage_loans[b]));
                        }
                    } else if pass == 2
                        && !(g.subprime_crisis_state > 0)
                        && !(Ext::from_f64(0.0) > cast::<Ext, _>(g.gdp_growth))
                    {
                        if g.bank_alloc_subprime_pct[b] > 0 {
                            g.subprime_secs[b] = to_quad(invest + cast::<Ext, _>(g.subprime_secs[b]));
                        }
                    } else if pass == 3 {
                        if g.bank_alloc_consumer_pct[b] > 0 {
                            g.bank_consumer_loans[b] =
                                to_quad(invest + cast::<Ext, _>(g.bank_consumer_loans[b]));
                        }
                    } else {
                        invest = Ext::from_f64(0.0);
                    }
                }
                1 => g.bank_mortgage_loans[b] = to_quad(invest + cast::<Ext, _>(g.bank_mortgage_loans[b])),
                2 => g.subprime_secs[b] = to_quad(invest + cast::<Ext, _>(g.subprime_secs[b])),
                3 => g.bank_consumer_loans[b] = to_quad(invest + cast::<Ext, _>(g.bank_consumer_loans[b])),
                4 => {
                    // Quirk: with a 0% bond target (forced on pass 4) the invested cash is lost.
                    if g.bank_alloc_bonds_pct[b] != 0 {
                        if g.long_bond_price == Ext::from_f32(0.0f32) {
                            g.long_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
                        }
                        if g.short_bond_price == Ext::from_f32(0.0f32) {
                            g.short_bond_price = cast::<f32, _>(Ext::from_f32(100.0f32));
                        }
                        invest = invest * sf(0.5f32);
                        if !(sf(4.5f32) >= cast::<Ext, _>(g.base_interest_rate)) && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0) {
                            let face = to_quad(invest / (sf(0.01f32) * cast::<Ext, _>(g.long_bond_price)));
                            trade_gov_bonds(g, b, face, 1, 1);
                        } else {
                            let face = to_quad(invest / (sf(0.01f32) * cast::<Ext, _>(g.short_bond_price)));
                            trade_gov_bonds(g, b, face, 2, 1);
                        }
                        invest = Ext::from_f64(0.0);
                    }
                }
                _ => {}
            }
            g.cash[b] = g.cash[b] - invest;
        } else if over > 0 {
            let mut sell_amt = asset_amt[cast::<usize, _>(over)] * sf(0.09f32);
            let amt = asset_amt[cast::<usize, _>(over)];
            let full_sale = |sell: &mut Ext, g: &Globals| {
                *sell = amt * sf(0.2f32);
                if amt < g.exchange_rate * Ext::from_f64(100.0) {
                    *sell = amt;
                }
            };
            match over {
                1 => {
                    if g.bank_alloc_prime_mort_pct[b] == 0 {
                        full_sale(&mut sell_amt, g);
                    }
                    g.bank_mortgage_loans[b] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[b]) - sell_amt);
                }
                2 => {
                    if g.bank_alloc_subprime_pct[b] == 0 {
                        full_sale(&mut sell_amt, g);
                    }
                    g.subprime_secs[b] = to_quad(cast::<Ext, _>(g.subprime_secs[b]) - sell_amt);
                }
                3 => {
                    if g.bank_alloc_consumer_pct[b] == 0 {
                        full_sale(&mut sell_amt, g);
                    }
                    g.bank_consumer_loans[b] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[b]) - sell_amt);
                }
                4 => {
                    sell_amt = Ext::from_f64(0.0);
                    if bank_regulator_lending_check(g, ui, b, 0) != 1 {
                        sell_bank_bonds(g, b, force_bond_sale);
                    }
                }
                _ => {}
            }
            g.cash[b] = sell_amt + g.cash[b];
            calc_entity_financials(g, ui, rng, b);
        } else if over == 0 && under == 5 {
            let mut sell = cast::<Ext, _>(g.bank_mortgage_loans[b]) * sf(0.02f32);
            g.cash[b] = sell + g.cash[b];
            g.bank_mortgage_loans[b] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[b]) - sell);
            if g.subprime_crisis_state != -2
                && !(g.subprime_crisis_state > 0)
                && !(Ext::from_f64(0.0) > cast::<Ext, _>(g.gdp_growth))
            {
                sell = cast::<Ext, _>(g.subprime_secs[b]) * sf(0.02f32);
                g.subprime_secs[b] = to_quad(cast::<Ext, _>(g.subprime_secs[b]) - sell);
                g.cash[b] = sell + g.cash[b];
            }
            sell = cast::<Ext, _>(g.bank_consumer_loans[b]) * sf(0.02f32);
            g.bank_consumer_loans[b] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[b]) - sell);
            g.cash[b] = sell + g.cash[b];
        }
    }
    balance_cash_t_bills(g, b);
    result
}

/// Bond sales of BankAllocateAssets mode 3 when bonds are over-allocated (CASE 4).
fn sell_bank_bonds(g: &mut Globals, b: i32, force_bond_sale: i32) {
    let mut bond_face: i64;
    let long_face = g.long_gov_bond_face[b];
    let short_face = g.short_gov_bond_face[b];
    if (cast::<Ext, _>(long_face)) > cast::<Ext, _>(short_face)
        && !(sf(11.5f32) < cast::<Ext, _>(g.base_interest_rate) && force_bond_sale == 0)
    {
        bond_face = to_quad(cast::<Ext, _>(g.long_gov_bond_face[b]) * sf(0.08f32));
        if g.bank_alloc_bonds_pct[b] == 0 {
            bond_face = to_quad(cast::<Ext, _>(g.long_gov_bond_face[b]) * sf(0.3f32));
        }
        if (cast::<Ext, _>(bond_face)) < Ext::from_f64(1.0) {
            bond_face = 1;
        }
        if !(cast::<Ext, _>(g.long_gov_bond_face[b]) > Ext::from_f64(100.0)) && g.bank_alloc_bonds_pct[b] == 0 {
            bond_face = g.long_gov_bond_face[b];
        }
        trade_gov_bonds(g, b, bond_face, 1, 2);
    }
    if !((cast::<Ext, _>(g.short_gov_bond_face[b])) < cast::<Ext, _>(g.long_gov_bond_face[b]))
        && cast::<Ext, _>(g.short_gov_bond_face[b]) > Ext::from_f64(0.0)
    {
        bond_face = to_quad(cast::<Ext, _>(g.short_gov_bond_face[b]) * sf(0.08f32));
        if g.bank_alloc_bonds_pct[b] == 0 {
            bond_face = to_quad(cast::<Ext, _>(g.short_gov_bond_face[b]) * sf(0.3f32));
        }
        if (cast::<Ext, _>(bond_face)) < Ext::from_f64(1.0) {
            bond_face = 1;
        }
        if !(cast::<Ext, _>(g.short_gov_bond_face[b]) > Ext::from_f64(100.0)) && g.bank_alloc_bonds_pct[b] == 0 {
            bond_face = g.short_gov_bond_face[b];
        }
        trade_gov_bonds(g, b, bond_face, 2, 2);
    }
    if 0 < g.hold_bond_id[(0, b)]
        && g.tmp_corp_bonds_value > g.tmp_gov_bonds_value
        && (force_bond_sale != 0 || sf(10.5f32) >= cast::<Ext, _>(g.base_interest_rate))
    {
        for slot in 1..=15 {
            let holding_id = g.hold_bond_id[(slot, b)];
            if holding_id == 0 || !(cast::<Ext, _>(g.hold_bond_face[(slot, b)]) >= Ext::from_f64(1.0)) {
                continue;
            }
            bond_face = to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, b)]) * sf(0.15f32));
            if g.bank_alloc_bonds_pct[b] == 0 {
                bond_face = to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, b)]) * sf(0.3f32));
            }
            if (cast::<Ext, _>(bond_face)) < Ext::from_f64(1.0) {
                bond_face = 1;
            }
            let mut cost_basis = g.bond_hold_cost[(slot, b)]
                * (cast::<Ext, _>(bond_face) / cast::<Ext, _>(g.hold_bond_face[(slot, b)]));
            if (cast::<Ext, _>(g.hold_bond_face[(slot, b)])) < Ext::from_f64(100.0) || g.bank_alloc_bonds_pct[b] == 0 {
                bond_face = g.hold_bond_face[(slot, b)];
                cost_basis = g.bond_hold_cost[(slot, b)];
            }
            let accrued_int = calc_accrued_bond_interest(g, bond_face, holding_id);
            let sale_proceeds =
                cast::<Ext, _>(bond_face) * ((sf(0.995f32) * g.bond_price[holding_id].get()) * sf(0.01f32));
            let realized_gain = sale_proceeds - cost_basis;
            g.accrued_pl[b] = accrued_int + g.accrued_pl[b];
            g.extraordinary_pl[b] = to_quad(realized_gain + cast::<Ext, _>(g.extraordinary_pl[b]));
            g.hold_bond_face[(slot, b)] =
                to_quad(cast::<Ext, _>(g.hold_bond_face[(slot, b)]) - cast::<Ext, _>(bond_face));
            g.bond_hold_cost[(slot, b)] = g.bond_hold_cost[(slot, b)] - cost_basis;
            if cast::<Ext, _>(g.hold_bond_face[(slot, b)]) == Ext::from_f64(0.0) {
                g.hold_bond_id[(slot, b)] = 0;
                g.bond_hold_cost[(slot, b)] = Ext::from_f64(0.0);
                if 0 < g.hold_bond_id[(0, b)] {
                    g.hold_bond_id[(0, b)] -= 1;
                }
            }
            g.cash[b] = (sale_proceeds + g.cash[b]) + accrued_int;
        }
    }
}

/// GOSUB BankAllocate_SellLosingStocks (0x69ee25): sells part of the first stock holding (2%, or 1% /
/// the rest for tiny stakes) with negative `EarningsYTD`.
fn bank_allocate_sell_losing_stocks(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, b: i32) {
    let __content = crate::content::current();
    for slot in 1..=15 {
        let holding_id = cast::<i32, _>(g.hold_stock_id[(slot, b)]);
        if holding_id > 10
            && g.hold_stock_pct[(slot, b)] > 0
            && g.hold_stock_pct[(slot, b)] < 100
            && g.earnings_ytd[holding_id] < Ext::from_f64(0.0)
        {
            g.tmp_ext1 = cast::<Ext, _>(b);
            g.tmp_ext2 = Ext::from_f64(2.0);
            if g.hold_stock_pct[(slot, b)] == 1 {
                g.tmp_ext2 = Ext::from_f64(1.0);
            }
            if g.hold_stock_pct[(slot, b)] < 3 {
                g.news_msg = g.ent_symbol[b].clone()
                    + __content.string(10771)
                    + &g.ent_symbol[holding_id]
                    + " stock";
                post_news(g, ui, b, holding_id);
            }
            g.tmp_ext3 = Ext::from_f64(2.0);
            sell_stock_holding(g, ui, rng, holding_id);
            return;
        }
    }
}

/// Port of TradeGovBonds (0x6a16ad).
/// Buys (`trade_action` 1) or sells (2) `face_amt` of the long (`bond_type` 1) or short (2) government
/// bond for `entity_ref` (active entities only) at `LongBondPrice` / `ShortBondPrice` with the accrued
/// interest; a sale books the gain/loss to `AccruedPL` (`ExtraordinaryPL` for funds) and is refused if
/// more than the holding. Always returns 0 (the result variable is never set).
pub fn trade_gov_bonds(
    g: &mut Globals,
    entity_ref: i32,
    face_amt: i64,
    bond_type: i32,
    trade_action: i32,
) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let e = entity_ref;
    if g.entity_status[e] != 1 {
        return 0;
    }
    let face = cast::<Ext, _>(face_amt);
    if trade_action == 1 {
        // debugMsg = "Buying BondType=" + STR$(bondType)
        let (purchase_cost, accrued_int);
        if bond_type == 1 {
            g.long_gov_bond_face[e] = to_quad(face + cast::<Ext, _>(g.long_gov_bond_face[e]));
            purchase_cost = (cast::<Ext, _>(g.long_bond_price) * face) * sf(0.01f32);
            g.lt_gov_bond_cost[e] = purchase_cost + g.lt_gov_bond_cost[e];
            accrued_int = calc_accrued_bond_interest(g, face_amt, 1);
        } else {
            g.short_gov_bond_face[e] = to_quad(face + cast::<Ext, _>(g.short_gov_bond_face[e]));
            purchase_cost = (cast::<Ext, _>(g.short_bond_price) * face) * sf(0.01f32);
            g.st_gov_bond_cost[e] = purchase_cost + g.st_gov_bond_cost[e];
            accrued_int = calc_accrued_bond_interest(g, face_amt, 2);
        }
        g.accrued_pl[e] = g.accrued_pl[e] - accrued_int;
        g.cash[e] = g.cash[e] - (accrued_int + purchase_cost);
    } else if trade_action == 2 {
        // debugMsg = "Selling BondType=" + STR$(bondType)
        let long = bond_type == 1;
        let held = cast::<Ext, _>(if long {
            g.long_gov_bond_face[e]
        } else {
            g.short_gov_bond_face[e]
        });
        if face > held {
            return 0;
        }
        let cost = if long {
            g.lt_gov_bond_cost[e]
        } else {
            g.st_gov_bond_cost[e]
        };
        let cost_basis = cost * (face / held);
        let accrued_int = calc_accrued_bond_interest(g, face_amt, if long { 1 } else { 2 });
        let price = cast::<Ext, _>(if long {
            g.long_bond_price
        } else {
            g.short_bond_price
        });
        let sale_proceeds = (sf(0.01f32) * face) * price;
        let gain_loss = sale_proceeds - cost_basis;
        if g.industry_num[e] == 71 {
            g.extraordinary_pl[e] = to_quad(gain_loss + cast::<Ext, _>(g.extraordinary_pl[e]));
        } else {
            g.accrued_pl[e] = gain_loss + g.accrued_pl[e];
        }
        g.accrued_pl[e] = accrued_int + g.accrued_pl[e];
        if long {
            g.long_gov_bond_face[e] = to_quad(cast::<Ext, _>(g.long_gov_bond_face[e]) - face);
            g.lt_gov_bond_cost[e] = g.lt_gov_bond_cost[e] - cost_basis;
        } else {
            g.short_gov_bond_face[e] = to_quad(cast::<Ext, _>(g.short_gov_bond_face[e]) - face);
            g.st_gov_bond_cost[e] = g.st_gov_bond_cost[e] - cost_basis;
        }
        g.cash[e] = (sale_proceeds + g.cash[e]) + accrued_int;
        if long {
            if cast::<Ext, _>(g.long_gov_bond_face[e]) == Ext::from_f64(0.0) {
                g.lt_gov_bond_cost[e] = Ext::from_f64(0.0);
            }
        } else if cast::<Ext, _>(g.short_gov_bond_face[e]) == Ext::from_f64(0.0) {
            g.st_gov_bond_cost[e] = Ext::from_f64(0.0);
        }
    }
    0
}

/// Port of CheckGovtBondLargeTrade (0x5bbcc5).
/// Market impact of a government bond trade of `face_amt` (DOUBLE result). The float of each issue is
/// half of the total issue (`ExchangeRate` * 125 billion grown 6% a year since the start, max 50 years)
/// minus the holdings of entities 1..130 and 1581..1583. `action` 7 returns the total issue, 8 the amount
/// held; trades below `ExchangeRate` * 500 (actions < 7) return 0. Actions 1/2 (buy/sell long), 3/4
/// (buy/sell short): trades over 0.5% of the float move the price (asks the player with a MSGBOX: the
/// price change in % if accepted, -99 if not; buying more than the float: -99 with a message); actions
/// 5/6 return 0.5% of the long/short float (`ExchangeRate` * 1001 if the float is tiny).
pub fn check_govt_bond_large_trade(
    g: &mut Globals,
    ui: &mut dyn Ui,
    action: i32,
    face_amt: i64,
) -> Ext {
    let __content = crate::content::current();
    let face = cast::<Ext, _>(face_amt);
    if !(face >= g.exchange_rate * Ext::from_f64(500.0)) && !(7 <= action) {
        return crate::pb::numeric::double(Ext::from_f64(0.0));
    }
    let mut total_issue = to_quad(g.exchange_rate * Ext::from_f64(125_000_000.0));
    let mut years = g.cur_year - g.start_year;
    if years > 0 {
        if years > 50 {
            years = 50;
        }
        for _ in 1..=years {
            total_issue = to_quad(sf(1.06f32) * cast::<Ext, _>(total_issue));
        }
    }
    let mut long_float = to_quad(cast::<Ext, _>(total_issue) * sf(0.5f32));
    let mut short_float = long_float;
    for i in 1..=1583 {
        if i <= 130 || i >= 1581 {
            long_float = to_quad(cast::<Ext, _>(long_float) - cast::<Ext, _>(g.long_gov_bond_face[i]));
            short_float = to_quad(cast::<Ext, _>(short_float) - cast::<Ext, _>(g.short_gov_bond_face[i]));
        }
    }
    if action > 6 {
        if action == 7 {
            return crate::pb::numeric::double(cast::<Ext, _>(total_issue));
        }
        if action == 8 {
            return crate::pb::numeric::double(cast::<Ext, _>(total_issue) - (cast::<Ext, _>(long_float) + cast::<Ext, _>(short_float)));
        }
        return crate::pb::numeric::double(Ext::from_f64(0.0));
    }
    let mut float_amt = long_float;
    let mut cur_price = cast::<Ext, _>(g.long_bond_price);
    if action == 3 || action == 4 || action == 6 {
        float_amt = short_float;
        cur_price = cast::<Ext, _>(g.short_bond_price);
    }
    if !(cast::<Ext, _>(float_amt) >= Ext::from_f64(1.0)) {
        float_amt = to_quad(g.exchange_rate * Ext::from_f64(499.0));
    }
    if !(cast::<Ext, _>(float_amt) >= g.exchange_rate * Ext::from_f64(500.0)) && !(4 >= action) {
        return crate::pb::numeric::double(g.exchange_rate * Ext::from_f64(1001.0));
    }
    let mut trade_pct = cast::<f32, _>((face / cast::<Ext, _>(float_amt)) * Ext::from_f64(100.0));
    trade_pct = trade_pct.abs();
    if !(sf(0.5f32) < cast::<Ext, _>(trade_pct)) {
        return crate::pb::numeric::double(Ext::from_f64(0.0));
    }
    if !(4 >= action) {
        return crate::pb::numeric::double(cast::<Ext, _>(float_amt) * sf(0.005f32));
    }
    if (action == 1 || action == 3) && !(face < cast::<Ext, _>(float_amt)) {
        if cast::<Ext, _>(float_amt) == Ext::from_f64(499.0) {
            float_amt = 500;
        }
        let mut msg = PbStr::from(__content.string(10772))
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(float_amt), b"#####################,")
            + " "
            + &g.million_prefix_lc
            + __content.string(2954)
            + __content.string(10773);
        show_quick_message(g, ui, &mut msg);
        return crate::pb::numeric::double(-Ext::from_f64(99.0));
    }
    let tp = cast::<Ext, _>(trade_pct);
    let mut price_pct: Ext = if tp > Ext::from_f64(50.0) {
        cast::<Ext, _>(tp * sf(1.8f32))
    } else if tp > Ext::from_f64(5.0) {
        tp * sf(1.6f32)
    } else if tp > Ext::from_f64(1.0) {
        tp * sf(1.2f32)
    } else {
        tp
    };
    if action == 2 || action == 4 {
        if Ext::from_f64(35.0) < price_pct {
            price_pct = Ext::from_f64(35.0);
        }
        price_pct = -Ext::from_f64(1.0) * price_pct;
    }
    let new_price = (price_pct + Ext::from_f64(100.0)) * (sf(0.01f32) * cur_price);
    let mut direction = PbStr::from("up");
    if action == 2 || action == 4 {
        direction = PbStr::from("down");
    }
    let msg = PbStr::from(__content.string(10774))
        + &direction
        + " the "
        + __content.string(10775)
        + &fmt_trim(cur_price, b"* ####.00")
        + " to "
        + &fmt_trim(new_price, b"* ###.00")
        + __content.string(10776)
        + "\r"
        + "\r"
        + __content.string(10777);
    let answer = ui.msgbox(&msg, 0x2123, &PbStr::from(__content.string(10778)));
    crate::pb::numeric::double(if answer == 6 { price_pct } else { -Ext::from_f64(99.0) })
}

/// Port of DrawCreditLineForCash (0x506842).
/// Covers a negative `Cash` of `entity_id` (active, not a bank) from its unused bank credit line
/// (`UnusedCreditLine` after [`calc_entity_financials`]); the entity being simulated (`SimCoCount`, > 85)
/// draws an extra 1.5..3% [runtime resource wording] / insurance reserves (at least 50..100% of its market
/// cap), within the line. Moves the amount from the lender `LenderId` to the entity's cash and bank debt.
/// Returns the amount drawn (EXT), 0 if nothing was drawn.
pub fn draw_credit_line_for_cash(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
) -> Ext {
    let e = entity_id;
    if g.entity_status[e] != 1 {
        return Ext::from_f64(0.0);
    }
    if 10 < e && g.industry_num[e] == 1 {
        return Ext::from_f64(0.0);
    }
    calc_entity_financials(g, ui, rng, e);
    if !(g.unused_credit_line > Ext::from_f64(0.0)) {
        return Ext::from_f64(0.0);
    }
    let mut draw_amt = if g.unused_credit_line > g.cash[e] * -Ext::from_f64(1.0) {
        -Ext::from_f64(1.0) * g.cash[e]
    } else {
        g.unused_credit_line
    };
    g.unused_credit_line = g.unused_credit_line - draw_amt;
    if g.sim_co_count == e && !(85 >= e) && g.unused_credit_line > Ext::from_f64(0.0) {
        let mut extra_draw = Ext::from_f64(0.0);
        if g.industry_num[e] == 2 {
            extra_draw = (sf(0.015f32) * rng.rnd() + sf(0.015f32)) * g.ins_reserves[e];
        } else if g.business_assets[e] > Ext::from_f64(0.0) {
            extra_draw = (sf(0.015f32) * rng.rnd() + sf(0.015f32)) * g.business_assets[e];
        }
        if extra_draw < g.market_cap[e] {
            extra_draw = (sf(0.5f32) * rng.rnd() + sf(0.5f32)) * g.market_cap[e];
        }
        if extra_draw > g.unused_credit_line {
            extra_draw = g.unused_credit_line;
        }
        draw_amt = extra_draw + draw_amt;
    }
    g.cash[e] = draw_amt + g.cash[e];
    if g.cash[e].abs() < sf(1e-6f32) && g.cash[e] < Ext::from_f64(0.0) {
        g.cash[e] = Ext::from_f64(0.0);
    }
    let l = g.lender_id;
    g.cash[l] = g.cash[l] - draw_amt;
    g.bank_debt[e] = draw_amt + g.bank_debt[e];
    draw_amt
}

/// Port of ProcessAsbestosLiability (0x506c63).
/// Asbestos lawsuits against `AsbestosCorp` (negative = heavier awards). Drops a fund that is the target;
/// replaces a defunct / D-rated target by a random entity 11..1600. When the target is being simulated
/// (`SimCoCount`) in its quarter (`(SimCoCount + CurYear) MOD 4 = CurQuarter`) it pays an award of 22..92%
/// of total assets (capped by net worth; minimum `ExchangeRate` * 100 per quarter; halved, or skipped with
/// probability 1/3, while players are short the stock) partly from [runtime resource wording] , with news.
/// The case may move to a random corp, end (no assets / random) or settle (margin rebound, news).
pub fn process_asbestos_liability(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let fmt_mask = b"* ###,";
    let corp_id = g.sim_co_count;
    let mut has_shorts = 0;
    if corp_id > 10
        && g.industry_num[corp_id] == 71
        && (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(corp_id)
    {
        g.asbestos_corp = 0;
        return;
    }
    let mut asbestos_id = to_long((cast::<Ext, _>(g.asbestos_corp)).abs());
    if !(1..=1600).contains(&asbestos_id) {
        // EntityStatus(0): error 9 -> ON ERROR handler (ShowRuntimeError) and exit
        return;
    }
    if !(g.entity_status[asbestos_id] == 1 && g.credit_rating_num[asbestos_id] != 0) {
        asbestos_id = rng.rnd_range(1600, 11);
    }
    if g.asbestos_corp < 0 {
        g.asbestos_corp = asbestos_id.wrapping_mul(-1);
    } else {
        g.asbestos_corp = asbestos_id;
    }
    if g.sim_co_count != asbestos_id || g.entity_status[asbestos_id] != 1 {
        return;
    }
    if (g.sim_co_count + g.cur_year) % 4 != g.cur_quarter {
        return;
    }
    g.tmp_symbol = g.ent_symbol[asbestos_id].clone();
    let mut msg_suffix = PbStr::from("s.");
    calc_entity_financials(g, ui, rng, corp_id);
    let award_rate: Ext = crate::pb::numeric::double(sf(0.7f32) * rng.rnd() + sf(0.22f32)); // DOUBLE
    let mut award = to_quad(award_rate * g.total_assets);
    if g.asbestos_corp < 0 {
        award = to_quad(sf(1.4f32) * cast::<Ext, _>(award));
    }
    if (cast::<Ext, _>(award)) < g.net_worth * sf(0.85f32) && g.net_worth > Ext::from_f64(0.0) {
        award = to_quad(g.net_worth * (sf(0.5f32) * rng.rnd() + sf(0.6f32)));
    }
    if cast::<Ext, _>(award) > g.net_worth * sf(1.1f32) {
        award = to_quad(sf(0.35f32) * cast::<Ext, _>(award));
    }
    if (cast::<Ext, _>(award)) < g.exchange_rate * Ext::from_f64(250.0) {
        award = to_quad(g.exchange_rate * Ext::from_f64(100.0) * cast::<Ext, _>(g.cur_quarter));
        msg_suffix = PbStr::from(".");
    }
    if find_short_positions(g, ui, rng, corp_id, 0) == 2 {
        has_shorts = 1;
        if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
            return;
        }
        award = to_quad(sf(0.5f32) * cast::<Ext, _>(award));
    }
    let a = cast::<Ext, _>(award);
    g.cash[corp_id] = g.cash[corp_id] - a;
    g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - a;
    let after_tax_cost = to_quad(a - a * (cast::<Ext, _>(g.corp_tax_rate) * sf(0.01f32)));
    let atc = cast::<Ext, _>(after_tax_cost);
    if g.contingency_reserve[corp_id] < atc * sf(0.9f32) {
        g.equity_income_ytd[corp_id] =
            g.contingency_reserve[corp_id] + g.equity_income_ytd[corp_id];
        g.contingency_reserve[corp_id] = Ext::from_f64(0.0);
    } else {
        g.equity_income_ytd[corp_id] = atc * sf(0.85f32) + g.equity_income_ytd[corp_id];
        g.contingency_reserve[corp_id] = g.contingency_reserve[corp_id] - atc * sf(0.85f32);
    }
    g.news_msg = g.tmp_symbol.clone()
        + __content.string(4728)
        + &g.currency_symbol
        + &fmt_trim(a, fmt_mask)
        + " "
        + &g.million_prefix_lc
        + "il.";
    if g.cur_quarter > 2 {
        g.news_msg = g.news_msg.clone() + __content.string(10779) + &msg_suffix;
    } else {
        let mut court_name = PbStr::from(__content.string(10780));
        if g.cur_quarter == 2 {
            court_name = PbStr::from("court");
        }
        g.news_msg = g.news_msg.clone() + __content.string(10781) + &court_name + __content.string(10782);
    }
    let ac = g.asbestos_corp;
    post_news(g, ui, ac, 0);
    if (cast::<Ext, _>(g.asbestos_corp)).abs() > Ext::from_f64(0.0) && g.rand_seed % 10 == 7 {
        g.asbestos_corp = rng.rnd_range(1580, 131);
        if g.entity_status[g.asbestos_corp] != 1 {
            g.asbestos_corp = 0;
        }
    }
    asbestos_id = to_long((cast::<Ext, _>(g.asbestos_corp)).abs());
    if asbestos_id > 130 && g.business_assets[asbestos_id] == Ext::from_f64(0.0) && Ext::from_f64(2.0) * rng.rnd() < Ext::from_f64(1.0) {
        g.asbestos_corp = 0;
    }
    if asbestos_id > 130 && g.business_assets[asbestos_id] > Ext::from_f64(0.0) && Ext::from_f64(40.0) * rng.rnd() < Ext::from_f64(1.0) {
        g.asbestos_corp = 0;
    }
    if asbestos_id == corp_id && has_shorts == 1 && Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) {
        g.asbestos_corp = 0;
        if g.industry_num[corp_id] < 70 && g.difficulty_level > 1 {
            g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_add(8);
        }
        g.investor_sentiment[corp_id] = 40;
    } else if g.industry_num[corp_id] < 70 {
        g.profit_margin[corp_id] = g.profit_margin[corp_id].wrapping_add(1);
    }
    // (ABS(AsbestosCorp) + CurYear) MOD 5 on the FPU (PB_FMod): exact for these integers
    let m = cast::<i64, _>((cast::<Ext, _>(g.asbestos_corp)).abs() + cast::<Ext, _>(g.cur_year)) % 5;
    if m == 2 && g.rand_seed % 5 <= g.cur_quarter {
        g.asbestos_corp = to_long((cast::<Ext, _>(g.asbestos_corp)).abs());
        let ac = g.asbestos_corp;
        if ac > 130 && g.entity_status[ac] == 1 && g.industry_num[ac] < 70 {
            g.profit_margin[ac] = cast::<i16, _>(to_quad(Ext::from_f64(10.0) * rng.rnd() + cast::<Ext, _>(g.profit_margin[ac])));
        }
        g.asbestos_corp = 0;
        g.news_msg = g.tmp_symbol.clone() + __content.string(10783);
        post_news(g, ui, corp_id, 0);
    }
}

/// Port of AiBoostRnDAndMarketing (0x4e07fa).
/// Above difficulty 1, scans corps 851..1600 (131..850 when `SimCoCount` = 117) in industries whose
/// demand gap is >= 15%: AI-voted corps with a large unused credit line and a margin above 10 raise their
/// growth rate by 2..22 (" [runtime resource wording] "), those with a margin below 10 raise R&D /
/// marketing spending by 9..14 points. Stops after exactly three actions. Posts a pending `NewsMsg` first
/// and the last headline built at the end.
///
/// Quirk: the country test (`CountryIdx < 26 AND CountryIdx > 27`, then a 6-in-7 skip) can never be true.
pub fn ai_boost_rnd_and_marketing(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // procTag = "BoostRND"; ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    if g.difficulty_level < 2 {
        return;
    }
    if g.news_msg != "" {
        post_news(g, ui, 0, 0);
    }
    let mut first_corp = 851;
    let mut last_corp = 1600;
    if g.sim_co_count == 117 {
        first_corp = 131;
        last_corp = 850;
    }
    let limit = last_corp;
    let mut action_count = 0;
    let mut action_text = PbStr::new();
    let mut corp_id = first_corp;
    while corp_id <= limit {
        let c = corp_id;
        'body: {
            if g.entity_status[c] != 1 {
                break 'body;
            }
            let industry = g.industry_num[c];
            if industry > 69 || g.disc_spend_pct[c] > 16 {
                break 'body;
            }
            // dead test: CountryIdx < 26 AND CountryIdx > 27 (AND <> 8: 1 < RND * 7 -> skip)
            if !(Ext::from_f64(15.0) <= g.ind_demand_gap[cast::<i32, _>(industry)]) {
                break 'body;
            }
            update_corp_control(g, ui, rng, c);
            if get_controller_type(g, c) != 1 {
                g.voting_controller = 0;
            }
            if g.voting_controller > 0 && g.voting_controller < 11 {
                break 'body;
            }
            calc_entity_financials(g, ui, rng, c);
            if !(g.unused_credit_line >= g.exchange_rate * Ext::from_f64(3000.0)) {
                break 'body;
            }
            if g.profit_margin[c] > 10
                && g.growth_rate[c] < 30
                && g.business_assets[c] < g.exchange_rate * Ext::from_f64(10000.0)
                && !(g.monetary_stance < 0)
            {
                g.growth_rate[c] =
                    cast::<i16, _>(to_quad((Ext::from_f64(2.0) + cast::<Ext, _>(g.growth_rate[c])) + Ext::from_f64(20.0) * rng.rnd()));
                g.news_msg =
                    g.ent_symbol[c].clone() + __content.string(10784);
                if is_in_recent_headlines(g, __content.bytes(10785)) == 1
                    && g.cur_player != g.corp_controller[c]
                {
                    g.news_msg = PbStr::new();
                }
                action_count += 1;
            }
            if g.profit_margin[c] < 10 && g.disc_spend_pct[c] < 12 {
                g.disc_spend_pct[c] = cast::<i16, _>(g.rand_seed % 6 + cast::<i32, _>(g.disc_spend_pct[c]) + 9);
                let ind_category = get_industry_category(g.industry_num[c]);
                if ind_category == 1 {
                    action_text = PbStr::from(__content.string(10786));
                } else if ind_category == 2 {
                    let mut verb = PbStr::from("boost");
                    if g.rand_seed % 4 == 1 {
                        verb = PbStr::from("up");
                    }
                    if g.rand_seed % 4 == 2 {
                        verb = PbStr::from(__content.string(10787));
                    }
                    action_text = verb + __content.string(10788);
                }
                if Ext::from_f64(5.0) * rng.rnd() < Ext::from_f64(1.0) || g.cur_player == g.corp_controller[c] {
                    g.news_msg = g.ent_symbol[c].clone() + " " + &action_text;
                }
                action_count += 1;
            }
            if action_count == 3 {
                corp_id = last_corp;
            }
        }
        corp_id += 1;
    }
    if g.news_msg != "" {
        post_news(g, ui, 0, 0);
    }
}
