//! AiComputerPlayerTurn steps: start-ups, insurer bond purchases, bond issues, lawsuits, rumors, option
//! writing.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::prelude::*;
use super::buyer::show_pending_msg;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{balance_cash_t_bills, calc_projected_earnings};
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::get_largest_holding_id;
use crate::logic::newgame_helpers::{create_bank, create_insurer};
use crate::logic::sim_entity::asset_sales::start_new_corp_or_expand;
use crate::logic::sim_entity::bonds::{bond_buyback, issue_bonds as issue_bonds_routine};
use crate::logic::sim_entity::etf::ai_create_option_position;
use crate::logic::sim_entity::fin_inst::buy_corp_bonds;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::valuation::{calc_industry_outlook, calc_stock_price, update_price_history_tick};
use crate::pb::string::{CaseMode, ucase};


/// Port of AiPlayer_StartupCompany (0x5c9f3f), GOSUB of AiComputerPlayerTurn ("5500").
/// When the player controls fewer than `CpuMaxCorps` companies, has no start-up count left, controls its
/// main holding and nothing was bought this turn (`newCorpId = 0`), with chance 1/3 (not in the first
/// year): picks a rich company it controls (from a random start id, entity > 85, `Cash + the player's
/// T-bills` (sic, BUGS.md #271) above `ExchangeRate * 1000`, chance 1/3 each, slot 15 free) as the
/// founder, an industry with a strong outlook (or a bank / insurer when the group has none), and founds
/// the company (`CreateBank` / `CreateInsurer` / `StartNewCorpOrExpand` mode 1) with capital `bondAmt`
/// (a quarter of the founder's liquid funds, or its cash and T-bills below rating 9; at most
/// `ExchangeRate * (1300..3000)`), held 100% by the founder. Sets `newCorpId`, counts the action.
pub(crate) fn startup_company(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("5500");
    let p = f.player_id;
    if !(g.cpu_max_corps > g.player_corps_controlled[p]) {
        return;
    }
    if 0 < f.new_corp_id || 0 < g.player_startup_count[p] {
        return;
    }
    if g.ai_target_corp == 0 || p != g.corp_controller[g.ai_target_corp] {
        return;
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(1.0) {
        return;
    }
    if g.cur_year < g.start_year + 1 {
        return;
    }
    f.startup_ind = 0;
    f.acquirer_id = 0;
    f.trade_count = 0;
    f.defendant_id = to_long(rng.rnd() * Ext::from_f64(500.0));
    f.ent_id = 11;
    while f.ent_id < 1601 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && p == g.corp_controller[e] {
            if g.industry_num[e] == 1 && f.trade_count != 1 && f.trade_count != 3 {
                f.trade_count += 1;
            }
            if g.industry_num[e] == 2 && f.trade_count < 2 {
                f.trade_count += 2;
            }
            if f.defendant_id <= e
                && 85 < e
                && g.exchange_rate * Ext::from_f64(1000.0) < g.t_bills[p] + g.cash[e]
                && rng.rnd() * Ext::from_f64(3.0) <= Ext::from_f64(1.0)
                && g.hold_stock_id[(15, e)] == 0
            {
                f.acquirer_id = e;
                f.ent_id = 1600;
            }
        }
        f.ent_id += 1;
    }
    if f.acquirer_id == 0 {
        return;
    }
    f.industry = 3;
    while f.industry <= 69 {
        let ind = cast::<i32, _>(f.industry);
        if !(g.ind_growth_forecast[ind] < 10)
            && !((cast::<i32, _>(g.ind_growth_forecast[ind])) < cast::<i32, _>(g.ind_growth_rate[ind]) + 7)
        {
            calc_industry_outlook(g, rng, f.industry);
            if cast::<i32, _>(f.industry) == g.crowded_industry {
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                g.tmp_ext3 = Ext::from_f64(0.0);
            } else {
                f.cur_outlook = cast::<f32, _>(g.tmp_ext1);
                f.fut_outlook = cast::<f32, _>(g.tmp_ext2);
                if f.cur_outlook < f.fut_outlook && rng.rnd() * Ext::from_f64(9.0) + Ext::from_f64(23.0) < cast::<Ext, _>(f.fut_outlook) {
                    f.startup_ind = f.industry;
                }
                g.tmp_ext1 = Ext::from_f64(0.0);
                g.tmp_ext2 = Ext::from_f64(0.0);
                g.tmp_ext3 = Ext::from_f64(0.0);
                if Ext::from_f64(35.0) < cast::<Ext, _>(f.fut_outlook) {
                    break;
                }
                if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) && 2 < f.startup_ind {
                    break;
                }
            }
        }
        f.industry += 1;
    }
    pack_holding_arrays(g, ui);
    if f.startup_ind == 0 {
        if f.trade_count < 2 {
            f.startup_ind = 2;
        }
        if g.entity_status[130] == 1 {
            f.startup_ind = 0;
        }
        if f.trade_count == 0 || (f.trade_count == 2 && g.entity_status[85] == 0) {
            f.startup_ind = 1;
        }
    }
    if f.startup_ind == 0 {
        return;
    }
    f.industry = f.startup_ind;
    let a = f.acquirer_id;
    calc_entity_financials(g, ui, rng, a);
    f.bond_amt = to_quad(g.liquid_funds * sf(0.25f32));
    if g.credit_rating_num[a] < 9 {
        f.bond_amt = to_quad(g.t_bills[a] + g.cash[a]);
    }
    if g.exchange_rate * Ext::from_f64(3000.0) < cast::<Ext, _>(f.bond_amt) {
        f.bond_amt = to_quad((rng.rnd() * Ext::from_f64(1700.0) + Ext::from_f64(1300.0)) * g.exchange_rate);
    }
    f.target_id = 0;
    if f.industry < 3 || 69 < f.industry {
        if f.industry == 1 {
            f.ent_id = 61;
            while f.ent_id < 86 {
                if g.entity_status[f.ent_id] != 1 {
                    f.target_id = f.ent_id;
                    f.ent_id = 85;
                }
                f.ent_id += 1;
            }
            if f.target_id == 0 {
                return;
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = g.exchange_rate * Ext::from_f64(1000.0);
            create_bank(g, ui, rng, f.target_id, 1);
        } else if f.industry == 2 {
            f.ent_id = 121;
            while f.ent_id < 131 {
                if g.entity_status[f.ent_id] != 1 {
                    f.target_id = f.ent_id;
                    f.ent_id = 130;
                }
                f.ent_id += 1;
            }
            if f.target_id == 0 {
                return;
            }
            g.tmp_ext1 = g.exchange_rate * Ext::from_f64(1000.0);
            g.tmp_ext2 = g.tmp_ext1 * sf(0.4f32);
            create_insurer(g, ui, rng, f.target_id, 1);
        }
    } else {
        g.tmp_ext1 = g.exchange_rate * Ext::from_f64(100.0);
        f.target_id = start_new_corp_or_expand(g, ui, rng, f.industry, 1, f.time_secs);
    }
    if f.target_id == 0 {
        g.tmp_ext1 = Ext::from_f64(0.0);
        return;
    }
    let t = f.target_id;
    g.player_startup_count[p] += 1;
    if f.industry < 3 {
        g.cash[t] = (g.cash[t] + cast::<Ext, _>(f.bond_amt)) - g.exchange_rate * Ext::from_f64(1000.0);
    } else {
        g.cash[t] = (g.cash[t] + cast::<Ext, _>(f.bond_amt)) - g.exchange_rate * Ext::from_f64(100.0);
        g.business_assets[t] = g.business_assets[t] + g.cash[t] * sf(0.85f32);
        g.cash[t] = g.cash[t] * sf(0.15f32);
    }
    g.valuation_mode = 3;
    calc_stock_price(g, ui, rng, t, f.time_secs);
    g.cash[a] = g.cash[a] - cast::<Ext, _>(f.bond_amt);
    g.hold_stock_id[(15, a)] = cast::<i16, _>(t);
    g.hold_stock_pct[(15, a)] = 100;
    f.new_corp_id = t;
    g.hold_stock_basis[(15, a)] = cast::<Ext, _>(f.bond_amt);
    pack_holding_arrays(g, ui);
    g.market_cap[t] = cast::<Ext, _>(f.bond_amt) * sf(0.013f32);
    f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(5.0) - Ext::from_f64(2.0));
    g.profit_margin[t] = cast::<i16, _>(f.rnd_val);
    g.disc_spend_pct[t] = cast::<i16, _>(rng.rnd_range(10, 5));
    if 130 < t {
        f.rnd_val = to_quad(rng.rnd() * Ext::from_f64(10.0) + Ext::from_f64(20.0));
        g.growth_rate[t] = cast::<i16, _>(f.rnd_val);
    }
    update_corp_control(g, ui, rng, t);
    g.turn_action_count += 1;
}

/// Port of AiPlayer_InsurerBuyBonds (0x5caa87), GOSUB of AiComputerPlayerTurn ("6000", called twice).
/// Picks an insurer (86..130) the player controls with `Cash + TBills > ExchangeRate * 1200` (not the
/// main holding unless `BaseInterestRate >= 12.5`; with more than 6 controlled companies only by chance
/// 1/5). Unless it is light on subsidiaries and already holds over 20% of its assets in bonds: below 10.25%
/// rates (cash under 60% of the assets or rates under 8.5%, no subprime crisis) it puts 30% of its cash
/// and T-bills into subprime securities; then buys corporate bonds worth `TotalAssets * 0.35 - bonds held`
/// (at least `ExchangeRate * 100`) of the first issuer from a random start (86..235) with 300+ bonds out
/// at a price <= 86, rating 5..7, positive earnings, enough public bonds, and none of it held by the
/// player (sic, BUGS.md #270: the player's bond slots are checked, not the insurer's).
pub(crate) fn insurer_buy_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("6000");
    f.buyer_corp = 0;
    f.ent_id = 86;
    while f.ent_id < 131 {
        let e = f.ent_id;
        if g.entity_status[e] == 1
            && f.player_id == g.corp_controller[e]
            && (g.ai_target_corp != e || sf(12.5f32) <= cast::<Ext, _>(g.base_interest_rate))
            && !(6 < f.corps_controlled && rng.rnd() * Ext::from_f64(5.0) > Ext::from_f64(1.0))
            && g.exchange_rate * Ext::from_f64(1200.0) < g.t_bills[e] + g.cash[e]
        {
            f.buyer_corp = e;
            f.ent_id = 130;
        }
        f.ent_id += 1;
    }
    if f.buyer_corp == 0 {
        return;
    }
    let b = f.buyer_corp;
    calc_entity_financials(g, ui, rng, b);
    if g.tmp_subsid_stock_value < g.total_assets * sf(0.6f32) && g.total_assets * sf(0.2f32) < g.tmp_corp_bonds_value {
        return;
    }
    let bir = cast::<Ext, _>(g.base_interest_rate);
    if bir < sf(10.25f32)
        && (g.cash[b] < g.total_assets * sf(0.6f32) || bir < sf(8.5f32))
        && g.subprime_crisis_state < 1
    {
        f.bond_amt = to_quad((g.t_bills[b] + g.cash[b]) * sf(0.3f32));
        g.subprime_secs[b] = to_quad(cast::<Ext, _>(f.bond_amt) + cast::<Ext, _>(g.subprime_secs[b]));
        g.cash[b] = g.cash[b] - cast::<Ext, _>(f.bond_amt);
        balance_cash_t_bills(g, b);
    }
    f.bond_amt = to_quad(g.total_assets * sf(0.35f32) - g.tmp_corp_bonds_value);
    if (cast::<Ext, _>(f.bond_amt)) < g.exchange_rate * Ext::from_f64(100.0) {
        return;
    }
    f.pick_id = to_long(int(rng.rnd() * Ext::from_f64(150.0)) + Ext::from_f64(86.0));
    f.ent_id = f.pick_id;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && 300 <= g.bonds_issued[e] && g.bond_price[e].get() <= Ext::from_f64(86.0) {
            f.found = 1;
            f.slot = 1;
            while f.slot <= 15 {
                if e == g.hold_bond_id[(f.slot, f.player_id)] {
                    f.found = 0;
                }
                f.slot += 1;
            }
            if f.found != 0 {
                calc_entity_financials(g, ui, rng, e);
                if !((cast::<Ext, _>(g.bonds_public_held)) < cast::<Ext, _>(f.bond_amt) * sf(2.5f32))
                    && 4 < g.credit_rating_num[e]
                    && g.credit_rating_num[e] < 8
                    && Ext::from_f64(0.0) < g.extra_items_ytd[e] + cast::<Ext, _>(g.extraordinary_pl[e]) + g.proj_qtr_earnings[e]
                {
                    g.tmp_ext1 = cast::<Ext, _>(f.bond_amt);
                    g.tmp_ext2 = Ext::from_f64(1.0);
                    buy_corp_bonds(g, ui, rng, f.buyer_corp, e);
                    f.ent_id = 1600;
                }
            }
        }
        f.ent_id += 1;
    }
}

/// Port of AiPlayer_IssueBonds (0x5cb017), GOSUB of AiComputerPlayerTurn ("6500", called with
/// `buyerCorp` = the main holding, then with 0).
/// With `buyerCorp = 0` the issuer is the last company the player controls that qualifies (chance 1/5
/// or fewer than 8 controlled companies; bonds out: 50+ and rates above 9.75%; none: rates below 5.5%).
/// At rates of 9.75%+ a company with bonds priced below 80..85 buys them back (`BondBuyback` mode 1,
/// forced) when it has `ExchangeRate * 400` of cash or a rating of 7+. At rates up to 5.5% a profitable
/// (3 years ago) company with rating 9+ and no bonds issues bonds of 20% of its net worth (doubled for
/// industries 69+ under `ExchangeRate * 5000`; at least `ExchangeRate * 200`), first paying down its bank
/// debt (75% of the issue if the debt is larger; the cash goes to the bank `LenderId`), with news.
pub(crate) fn issue_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("6500");
    if f.buyer_corp == 0 {
        f.ent_id = 11;
        while f.ent_id <= 1600 {
            let e = f.ent_id;
            if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] {
                let go = rng.rnd() * Ext::from_f64(5.0) <= Ext::from_f64(1.0) || f.corps_controlled < 8;
                if go {
                    let bonds = g.bonds_issued[e];
                    let bir = cast::<Ext, _>(g.base_interest_rate);
                    let skip = (0 < bonds && bonds < 50)
                        || (0 < bonds && bir <= sf(9.75f32))
                        || (bonds == 0 && sf(5.5f32) <= bir);
                    if !skip {
                        f.buyer_corp = e;
                    }
                }
            }
            f.ent_id += 1;
        }
    }
    f.ent_id = f.buyer_corp;
    if f.ent_id == 0 || f.player_id != g.corp_controller[f.ent_id] {
        return;
    }
    let e = f.ent_id;
    calc_entity_financials(g, ui, rng, e);
    if sf(9.75f32) <= cast::<Ext, _>(g.base_interest_rate)
        && 0 < g.bonds_issued[e]
        && g.bond_price[e].get() < rng.rnd() * Ext::from_f64(5.0) + Ext::from_f64(80.0)
        && (g.exchange_rate * Ext::from_f64(400.0) < g.t_bills[e] + g.cash[e] || 6 < g.credit_rating_num[e])
    {
        g.tmp_ext2 = Ext::from_f64(1.0);
        g.tmp_ext3 = -Ext::from_f64(1.0);
        bond_buyback(g, ui, rng, e);
    }
    if !(cast::<Ext, _>(g.base_interest_rate) <= sf(5.5f32)) {
        return;
    }
    if !(Ext::from_f64(0.0) < g.earnings_yr3_ago[e]
        && 8 < g.credit_rating_num[e]
        && g.bonds_issued[e] == 0
        && 7 < g.credit_rating_num[e])
    {
        return;
    }
    f.amount = g.net_worth * sf(0.2f32);
    if 68 < g.industry_num[e] && f.amount < g.exchange_rate * Ext::from_f64(5000.0) {
        f.amount = Ext::from_f64(2.0) * f.amount;
    }
    if f.amount < g.exchange_rate * Ext::from_f64(200.0) {
        return;
    }
    f.cash_amt = to_quad(f.amount * sf(0.01f32));
    f.bond_amt = to_quad(cast::<Ext, _>(f.cash_amt) * Ext::from_f64(100.0));
    f.debt_paydown = to_quad(g.bank_debt[e]);
    if 0 < f.debt_paydown {
        if f.bond_amt < f.debt_paydown {
            f.debt_paydown = to_quad(cast::<Ext, _>(f.bond_amt) * sf(0.75f32));
        }
        g.bank_debt[e] = g.bank_debt[e] - cast::<Ext, _>(f.debt_paydown);
        if g.bank_debt[e] < Ext::from_f64(0.0) {
            g.bank_debt[e] = Ext::from_f64(0.0);
        }
        g.cash[e] = g.cash[e] - cast::<Ext, _>(f.debt_paydown);
        let l = g.lender_id;
        g.cash[l] = cast::<Ext, _>(f.debt_paydown) + g.cash[l];
    }
    g.tmp_ext1 = cast::<Ext, _>(f.bond_amt);
    issue_bonds_routine(g, ui, rng, e, Ext::from_f64(0.0), 0);
    if 10 < f.debt_paydown {
        g.news_msg =
            PbStr::from(__content.string(1803)) + &g.ent_symbol[e] + __content.string(1804);
        post_news(g, ui, e, 0);
    }
}

/// Port of AiPlayer_FileLawsuit (0x5cb6ea), GOSUB of AiComputerPlayerTurn ("7000".."7008").
/// At difficulty 3+, a player with a controlled main holding, notoriety up to 5 and a free last lawsuit
/// slot targets the largest holding of a random rival player (2..NumPlayers) that it doesn't control,
/// when that stake is worth 75%+ of the rival's assets, the rival's credit line is mostly used, its net
/// worth is at most 1100 and well below the player's (2.5x), and the target is small (net worth 1300,
/// assets 4000) unless the player is 6x richer. The plaintiff is the richest company the player controls
/// (not last time's, 700+ liquid funds, rating above 6 (5 for a rich player), not held 31%+ by an
/// outsider or held by the player / main holding unless rich). Files a harassing suit (`LawsuitRec`
/// "2|plaintiff|target|years|", not if one against the target is pending): costs the plaintiff 500 and
/// the target 500 less 50 per law-firm grade of the rival, raises notoriety, news and a message box.
pub(crate) fn file_lawsuit(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("7000");
    if g.difficulty_level < 3 {
        return;
    }
    if g.ai_target_corp == 0 || f.player_id != g.corp_controller[g.ai_target_corp] {
        return;
    }
    if 5 < g.player_notoriety[f.player_id] || !g.lawsuit_rec[10].is_empty() {
        return;
    }
    f.other_id = rng.rnd_range(g.num_players, 2);
    f.largest_hold_id = cast::<i16, _>(get_largest_holding_id(g, f.other_id));
    f.target_id = cast::<i32, _>(f.largest_hold_id);
    if f.target_id == 0 || g.ai_target_corp == f.target_id || f.player_id == g.corp_controller[f.target_id] {
        return;
    }
    f.trace_tag = PbStr::from("7001");
    f.target_pct = 0;
    f.ent_id = 1;
    while f.ent_id <= 15 {
        if cast::<i32, _>(g.hold_stock_id[(f.ent_id, f.other_id)]) == f.target_id {
            f.target_pct = cast::<i32, _>(g.hold_stock_pct[(f.ent_id, f.other_id)]);
        }
        f.ent_id += 1;
    }
    if f.target_pct < 1 {
        return;
    }
    calc_entity_financials(g, ui, rng, f.other_id);
    f.amount = g.net_worth;
    if !(g.total_assets * sf(0.75f32) <= g.market_cap[f.target_id] * cast::<Ext, _>(f.target_pct)) {
        return;
    }
    f.trace_tag = PbStr::from("7002");
    if !(g.unused_credit_line <= g.total_assets * sf(0.25f32)) {
        return;
    }
    f.trace_tag = PbStr::from("7003");
    calc_entity_financials(g, ui, rng, f.player_id);
    if Ext::from_f64(1100.0) < f.amount || !(f.amount * sf(2.5f32) <= g.net_worth) {
        return;
    }
    f.trace_tag = PbStr::from("7004");
    f.slot = 0;
    if f.amount * Ext::from_f64(6.0) < g.net_worth {
        f.slot = 1;
    }
    calc_entity_financials(g, ui, rng, f.target_id);
    if (Ext::from_f64(1300.0) < g.net_worth || Ext::from_f64(4000.0) < g.total_assets) && f.slot == 0 {
        return;
    }
    f.trace_tag = PbStr::from("7005");
    f.suit_target = 0;
    f.x_c8 = Ext::from_f64(0.0);
    f.trace_tag = PbStr::from(__content.string(1805));
    f.ent_id = 11;
    while f.ent_id <= 1600 {
        let e = f.ent_id;
        if g.entity_status[e] == 1 && f.player_id == g.corp_controller[e] && g.ai_last_suit_plaintiff != e {
            update_corp_control(g, ui, rng, e);
            let outsider = !(30 < g.largest_holder_pct
                || cast::<i32, _>(g.largest_holder) == f.player_id
                || cast::<i32, _>(g.largest_holder) == g.ai_target_corp);
            if outsider || f.slot != 0 {
                calc_entity_financials(g, ui, rng, e);
                if Ext::from_f64(700.0) < g.liquid_funds && 6 - f.slot < g.credit_rating_num[e] && f.x_c8 < g.net_worth {
                    f.suit_target = e;
                    f.x_c8 = g.net_worth;
                }
            }
        }
        f.ent_id += 1;
    }
    f.trace_tag = PbStr::from("7007");
    if f.suit_target == 0 {
        return;
    }
    f.trace_tag = PbStr::from("7008");
    f.pick_id = g.player_law_firm[f.other_id];
    if f.pick_id == 0 {
        f.pick_id = 2;
    }
    f.law_firm_adj = 2 - f.pick_id;
    f.pick_id = to_long(sf(0.1f32) * cast::<Ext, _>(f.law_firm_adj) * Ext::from_f64(500.0));
    f.defense_cost = 500 - f.pick_id;
    f.tmp_val = f.law_firm_adj;
    f.trade_count = (cast::<i32, _>(g.player_notoriety[f.player_id]) + 4) - f.tmp_val;
    if f.trade_count < 3 {
        f.trade_count = 3;
    }
    f.pending_msg = PbStr::from("2|")
        + &str_s(cast::<Ext, _>(f.suit_target))
        + "|"
        + &str_s(cast::<Ext, _>(f.target_id))
        + "|"
        + &str_s(cast::<Ext, _>(f.trade_count))
        + "|";
    f.cash_amt = 0;
    f.found = 1;
    f.ent_id = 1;
    while f.ent_id <= 10 {
        f.field_text = g.lawsuit_rec[f.ent_id].clone();
        if f.field_text.is_empty() && f.cash_amt == 0 {
            f.cash_amt = cast::<i64, _>(f.ent_id);
        }
        if !f.field_text.is_empty()
            && val(mid(f.field_text.as_bytes(), 1, 1).as_bytes()) == Ext::from_f64(2.0)
            && val(parse_any(f.field_text.as_bytes(), b"|", 3).as_bytes()) == cast::<Ext, _>(f.target_id)
        {
            f.found = 0;
        }
        f.ent_id += 1;
    }
    if f.cash_amt == 0 || f.found == 0 {
        return;
    }
    f.rnd_val = f.cash_amt;
    g.lawsuit_rec[cast::<i32, _>(f.rnd_val)] = f.pending_msg.clone();
    let s = f.suit_target;
    let t = f.target_id;
    g.ai_last_suit_plaintiff = s;
    g.cash[s] = g.cash[s] - Ext::from_f64(500.0);
    g.accrued_pl[s] = g.accrued_pl[s] - Ext::from_f64(500.0);
    g.cash[t] = g.cash[t] - cast::<Ext, _>(f.defense_cost);
    g.accrued_pl[t] = g.accrued_pl[t] - cast::<Ext, _>(f.defense_cost);
    calc_projected_earnings(g, ui, rng, s);
    calc_projected_earnings(g, ui, rng, t);
    let p = f.player_id;
    g.player_notoriety[p] += 1;
    f.pending_msg = PbStr::from(__content.string(1806));
    if 2 < g.player_notoriety[p] {
        f.pending_msg = PbStr::from(__content.string(1807));
    }
    if 4 < g.player_notoriety[p] {
        f.pending_msg = PbStr::from(__content.string(1808));
    }
    g.news_msg = f.pending_msg.clone() + " by " + &g.ent_symbol[s] + " vs. " + &g.ent_symbol[t];
    f.pending_msg = ucase(g.news_msg.as_bytes(), CaseMode::Ansi)
        + "! ("
        + &g.ent_name[s]
        + __content.string(1809)
        + &g.ent_name[p]
        + ".)";
    post_news(g, ui, 0, 0);
    show_pending_msg(g, ui, f);
}

/// Port of AiPlayer_SpreadRumors (0x5cc20c), GOSUB of AiComputerPlayerTurn ("8000").
/// At difficulty 2+ (or when a rival player spread rumors before, `LibelSuitDefendant` 2+): a player with
/// notoriety or such a rival (not while its own rumor victim's case is pending, chance 3/4) spreads a
/// false rumor about the largest holding (looked through a holding company) of the rumor-monger rival
/// (`LibelSuitDefendant`) or, for a well-rated player with notoriety under 3, of a random small, poorly
/// rated rival player whose assets are mostly stock and whose net worth is under a quarter of the
/// player's. The target's market cap drops 15% (price history tick), operating companies' margins by 5
/// and it becomes `RumorVictimId`; the player becomes `LibelSuitDefendant` with notoriety + 1; news and a
/// message box; counts the action.
pub(crate) fn spread_rumors(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.trace_tag = PbStr::from("8000");
    if g.difficulty_level < 2 && g.libel_suit_defendant < 2 {
        return;
    }
    let p = f.player_id;
    f.other_id = cast::<i32, _>(g.libel_suit_defendant);
    if g.player_notoriety[p] == 0 && f.other_id < 2 {
        return;
    }
    if 0 < g.rumor_victim_id && p < cast::<i32, _>(g.libel_suit_defendant) && rng.rnd() * Ext::from_f64(4.0) > Ext::from_f64(1.0) {
        return;
    }
    let mut picked = false;
    if f.other_id < 2 && 6 < g.credit_rating_num[p] && g.player_notoriety[p] < 3 {
        f.amount = g.net_worth;
        f.other_id = rng.rnd_range(g.num_players, 2);
        calc_entity_financials(g, ui, rng, f.other_id);
        if g.tmp_subsid_stock_value < g.total_assets * sf(0.8f32) {
            return;
        }
        if 5 < g.credit_rating_num[f.other_id] {
            return;
        }
        if g.exchange_rate * Ext::from_f64(300.0) < g.net_worth {
            return;
        }
        if f.amount < g.net_worth * Ext::from_f64(4.0) {
            return;
        }
        picked = true;
    }
    if !picked && f.other_id < 2 {
        return;
    }
    f.largest_hold_id = cast::<i16, _>(get_largest_holding_id(g, f.other_id));
    f.ent_id = cast::<i32, _>(f.largest_hold_id);
    if g.ai_target_corp == f.ent_id && 10 < f.ent_id {
        return;
    }
    if 10 < f.ent_id && g.industry_num[f.ent_id] == 70 {
        f.largest_hold_id = cast::<i16, _>(get_largest_holding_id(g, f.ent_id));
    }
    if g.entity_status[f.other_id] != 1 || f.largest_hold_id < 11 {
        return;
    }
    if g.ai_target_corp == f.ent_id && 10 < f.ent_id {
        return;
    }
    f.target_id = cast::<i32, _>(f.largest_hold_id);
    let t = f.target_id;
    g.news_msg = g.ent_symbol[t].clone() + __content.string(1810);
    if Ext::from_f64(200.0) < g.earnings_ytd[t] {
        g.news_msg = g.ent_symbol[t].clone() + __content.string(1811);
    }
    if !(g.t_bills[t] + g.cash[t] > Ext::from_f64(0.0)) {
        g.news_msg = g.ent_symbol[t].clone() + __content.string(1812);
    }
    if rng.rnd() * Ext::from_f64(3.0) > Ext::from_f64(2.0) {
        g.news_msg = PbStr::from(__content.string(1813)) + &g.ent_symbol[t] + ".";
    }
    if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) {
        g.news_msg = PbStr::from(__content.string(1814)) + &g.ent_symbol[t] + __content.string(1815);
    }
    f.pending_msg = ucase(g.news_msg.as_bytes(), CaseMode::Ansi)
        + __content.string(1816)
        + &g.ent_name[p]
        + __content.string(1817)
        + &g.ent_symbol[t]
        + ".)";
    post_news(g, ui, 0, 0);
    g.market_cap[t] = sf(0.85f32) * g.market_cap[t];
    update_price_history_tick(g, ui, rng, t);
    g.turn_action_count += 1;
    show_pending_msg(g, ui, f);
    if g.industry_num[t] < 70 {
        g.rumor_victim_id = t;
        g.profit_margin[t] = g.profit_margin[t] - 5;
    }
    g.libel_suit_defendant = cast::<i16, _>(p);
    g.player_notoriety[p] += 1;
}

/// Port of AiPlayer_WriteOptions (0x5cce66), GOSUB of AiComputerPlayerTurn ("9500").
/// The main holding writes calls (`AiCreateOptionPosition` type 1) on 5% (cash and T-bills over 6x the
/// market cap) or 3% (over 4x) of a random holding of its own that the player controls and holds under
/// 80% of, when the share price is above 6.3.
pub(crate) fn write_options(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    f.trace_tag = PbStr::from("9500");
    let a = g.ai_target_corp;
    f.pick_id = rng.rnd_range(cast::<i32, _>(g.hold_stock_id[(0, a)]), 1);
    f.target_id = cast::<i32, _>(g.hold_stock_id[(f.pick_id, a)]);
    f.target_pct = cast::<i32, _>(g.hold_stock_pct[(f.pick_id, a)]);
    if f.target_id == 0 || f.target_pct == 0 {
        return;
    }
    if 79 < f.target_pct {
        return;
    }
    if f.player_id != g.corp_controller[f.target_id] {
        return;
    }
    let t = f.target_id;
    f.target_pct = 0;
    if g.market_cap[t] * Ext::from_f64(6.0) < g.t_bills[a] + g.cash[a] {
        f.target_pct = 5;
    } else if g.market_cap[t] * Ext::from_f64(4.0) < g.t_bills[a] + g.cash[a] {
        f.target_pct = 3;
    }
    f.other_id = 0;
    if 0 < f.target_pct && sf(6.3f32) < g.market_cap[t] / g.shares_outstanding[t] {
        f.other_id = ai_create_option_position(g, ui, rng, a, t, f.target_pct, 1, 0);
    }
    if 0 < f.other_id {
        calc_entity_financials(g, ui, rng, g.ai_target_corp);
    }
}
