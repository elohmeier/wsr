//! GOSUB blocks of SimProcessNextEntity (see [`super`]) for players: quarterly CEO pay and income tax
//! (`SimPlayer_QuarterlyPay` 0x4d9853), the solvency check with margin calls and bankruptcy
//! (`SimPlayer_CheckSolvency` 0x4daa4b) and the overdraft prompt (`SimPlayer_OverdraftPrompt` 0x4dc1f1).
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::Frame;
#[allow(unused_imports)]
use super::prelude::*;

use super::accounting::{
    draw_credit_line, forced_bond_sales, sell_holding_for_cash, working_capital_cash,
};
use super::earnings_news::pay_bond_interest;
use super::quarter_start::{
    call_in_player_advances, cancel_pending_merger, clear_insider_tip, pack_holding_arrays,
    revoke_ex_ceo_options,
};
use crate::logic::cashflow::calc_player_income_tax;
use crate::logic::control::find_option_positions;
use crate::logic::earnings::{balance_cash_t_bills, calc_ceo_pay};
use crate::logic::finance::{calc_entity_financials, get_controller_type};
use crate::logic::futures::{add_or_remove_futures_rec, recalc_commod_margin_balances};
use crate::logic::market::{rebuild_streaming_quotes_panel, update_market_display};
use crate::logic::options::{add_or_remove_option_rec, close_option_positions, get_plr_naked_liab};
use crate::logic::report::show_quick_message;
use crate::logic::shorts::find_short_positions;
use crate::logic::swaps::cancel_entity_swaps;
use crate::pb::string::{CaseMode, string_fill, ucase};

/// GOSUB SimPlayer_QuarterlyPay (0x4d9853): bond interest, the CEO's quarterly pay (and year-end bonus,
/// or the golden parachute when the player lost control), the quarterly estimated / year-end income tax
/// with headlines, the year-end reset of the player's income accounts, then the cash and solvency check.
pub(super) fn quarterly_pay(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    f.line_marker = PbStr::from("31000");
    let e = f.ent_id;
    if g.player_name[e] == "...OUT..." || g.num_players < e {
        return;
    }
    if !f.skips(0x4d5ae2) {
        pay_bond_interest(g, ui, rng, f);
    }
    f.other_id = g.player_ceo_of[e];
    if 10 < f.other_id {
        let c = f.other_id;
        f.slot = 1;
        if e != g.corp_controller[c] && g.cur_quarter == 4 {
            g.news_msg = g.ent_symbol[e].clone()
                + __content.string(11070)
                + &g.ent_symbol[c]
                + __content.string(11071);
            f.slot = 3;
            if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
                g.news_msg = g.ent_symbol[e].clone()
                    + __content.string(11072)
                    + &g.ent_symbol[c];
            }
            post_news(g, ui, c, 0);
            g.player_ceo_of[e] = 0;
            g.player_ceo_since_year[e] = 0;
            g.auto_pilot[c] = g.auto_pilot[e];
            if !f.skips(0x5fd937) {
                revoke_ex_ceo_options(g, ui);
            }
        }
        f.amount = calc_ceo_pay(g, ui, rng, c, 1) * cast::<Ext, _>(f.slot);
        g.ceo_bonus_multiple = g.ceo_bonus_multiple * f.amount;
        f.amount = sf(0.25f32) * f.amount;
        g.cash[e] = f.amount + g.cash[e];
        g.accrued_pl[e] = f.amount + g.accrued_pl[e];
        g.cash[c] = g.cash[c] - f.amount;
        g.accrued_pl[c] = g.accrued_pl[c] - f.amount;
        f.msg_part3 = f.fmt_money.clone();
        if f.amount > Ext::from_f64(1000.0) {
            f.msg_part3 = PbStr::from("* ###,");
        }
        g.news_msg = g.ent_symbol[e].clone()
            + __content.string(11073)
            + &g.ent_symbol[c]
            + " is "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(f.amount, f.msg_part3.as_bytes())
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ".";
        post_news(g, ui, 0, 0);
        if g.cur_quarter == 4 && g.ceo_bonus_multiple > Ext::from_f64(0.0) {
            f.msg_part3 = f.fmt_money.clone();
            if g.ceo_bonus_multiple > Ext::from_f64(1000.0) {
                f.msg_part3 = PbStr::from("* ###,");
            }
            g.news_msg = PbStr::from(__content.string(11074))
                + &g.ent_symbol[e]
                + " from "
                + &g.ent_symbol[c]
                + " is "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(g.ceo_bonus_multiple, f.msg_part3.as_bytes())
                + " "
                + &g.million_prefix_lc
                + "il."
                + &g.currency_suffix
                + ".";
            post_news(g, ui, e, 0);
            g.cash[e] = g.ceo_bonus_multiple + g.cash[e];
            g.accrued_pl[e] = g.ceo_bonus_multiple + g.accrued_pl[e];
            g.cash[c] = g.cash[c] - g.ceo_bonus_multiple;
            g.accrued_pl[c] = g.accrued_pl[c] - g.ceo_bonus_multiple;
        }
    }
    f.line_marker = PbStr::from("31000-PLUS");
    f.amount = g.player_cap_gains[e];
    f.tax_due = calc_player_income_tax(g, e);
    f.taxable_income = g.tmp_ext1;
    g.tmp_ext1 = Ext::from_f64(0.0);
    f.cap_gain_share = f.tax_due - g.player_prepaid_tax[e];
    f.tax_payment = f.cap_gain_share * sf(0.9f32);
    if g.cur_quarter == 4 {
        f.tax_payment = f.cap_gain_share;
        g.player_prepaid_tax[e] = Ext::from_f64(0.0);
    } else if f.tax_payment > Ext::from_f64(0.0) {
        calc_entity_financials(g, ui, rng, e);
        if f.tax_payment > g.net_worth - g.player_prepaid_tax[e] {
            f.tax_payment = sf(0.5f32) * f.tax_payment;
        }
        if f.tax_payment > g.net_worth - g.player_prepaid_tax[e] {
            g.news_msg = g.player_name[e].clone() + __content.string(11075);
            post_news(g, ui, 0, 0);
            if !f.skips(0x4daa4b) {
                check_solvency(g, ui, rng, f);
            }
            return;
        }
        g.player_prepaid_tax[e] = f.tax_payment + g.player_prepaid_tax[e];
    } else {
        g.news_msg =
            PbStr::from(__content.string(11076)) + &g.player_name[e];
        if g.cur_player == e {
            post_news(g, ui, 0, 0);
        }
        f.tax_payment = Ext::from_f64(0.0);
    }
    g.cash[e] = g.cash[e] - f.tax_payment;
    if g.is_computer_player[e] == 0 {
        f.msg_part1 = PbStr::from(__content.string(11077));
        f.msg_part3 = f.fmt_money.clone();
        if f.tax_payment.abs() > Ext::from_f64(1000.0) {
            f.msg_part3 = PbStr::from("* ###,");
        }
        if f.tax_payment > Ext::from_f64(0.0) && g.cur_quarter == 4 {
            f.msg_part1 = PbStr::from(__content.string(11078));
        }
        if f.tax_payment < Ext::from_f64(0.0) {
            f.msg_part1 = PbStr::from(__content.string(11079));
        }
        if f.tax_payment != Ext::from_f64(0.0) {
            g.news_msg = g.player_name[e].clone()
                + " "
                + &f.msg_part1
                + " is "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(f.tax_payment.abs(), f.msg_part3.as_bytes())
                + " "
                + &g.million_prefix_lc
                + "il.";
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    if g.cur_quarter == 4 {
        f.msg_part2 = trim(g.currency_symbol.as_bytes());
        f.msg_part3 = f.fmt_money.clone();
        if f.tax_due > Ext::from_f64(999.0) {
            f.msg_part3 = PbStr::from("* ###,");
        }
        g.news_msg = g.player_name[e].clone()
            + __content.string(11080)
            + &str_s(cast::<Ext, _>(g.cur_year))
            + " was: ";
        f.msg_part1 = PbStr::from(__content.string(11081));
        if f.tax_due > Ext::from_f64(0.0) {
            f.msg_part1 = f.msg_part2.clone()
                + &fmt_trim(f.tax_due, f.msg_part3.as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".";
        }
        g.news_msg = g.news_msg.clone() + &f.msg_part1;
        if f.amount > Ext::from_f64(0.0) {
            post_news(g, ui, 0, 0);
            g.news_msg = g.player_name[e].clone()
                + __content.string(11082)
                + &f.msg_part2
                + &fmt_trim(f.amount, f.msg_part3.as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".";
        }
        post_news(g, ui, 0, 0);
        if g.is_computer_player[e] == 0 {
            g.news_msg = g.player_name[e].clone()
                + " had"
                + &str_s(cast::<Ext, _>(g.cur_year))
                + __content.string(11083)
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(f.taxable_income, f.msg_part3.as_bytes())
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix
                + ".";
            post_news(g, ui, 0, 0);
        }
        g.accrued_pl[e] = Ext::from_f64(0.0);
        g.extraordinary_pl[e] = 0;
        g.div_recvd1[e] = Ext::from_f64(0.0);
        g.div_recvd2[e] = Ext::from_f64(0.0);
        if g.player_cap_gains[e] > Ext::from_f64(0.0) {
            g.player_cap_gains[e] = Ext::from_f64(0.0);
        }
        if g.game_end_year - 1 <= g.cur_year {
            g.player_prepaid_tax[e] = Ext::from_f64(0.0);
        }
    }
    find_short_positions(g, ui, rng, 0, 0);
    if g.cash[e] < Ext::from_f64(0.0) {
        balance_cash_t_bills(g, e);
    }
    if g.cash[e] < Ext::from_f64(0.0) && !f.skips(0x4d2976) {
        working_capital_cash(g, ui, rng, f);
    }
    if !f.skips(0x4daa4b) {
        check_solvency(g, ui, rng, f);
    }
}

/// GOSUB SimPlayer_CheckSolvency (0x4daa4b): a player with negative net worth is fired as CEO; a cash
/// deficit is covered from T-bills, the credit line, bond and option sales, advances, commodity margin,
/// stock sales (the human player is asked) and prepaid taxes; a broke player goes bankrupt (out of the
/// game, lender's bad-debt loss); margin calls on the bank loan and short positions (repeated at
/// difficulty 3 for other players).
pub(super) fn check_solvency(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    loop {
        f.line_marker = PbStr::from("32300");
        let e = f.ent_id;
        if g.short_pos_flag != -1 {
            f.tmp_long = cast::<i64, _>(find_short_positions(g, ui, rng, 0, e));
        }
        f.close_mode = 0;
        calc_entity_financials(g, ui, rng, e);
        if g.net_worth < Ext::from_f64(0.0)
            && 10 < g.player_ceo_of[e]
            && e != g.corp_controller[g.player_ceo_of[e]]
        {
            fired_as_ceo(g, ui, rng, f);
        }
        if g.tmp_subsid_stock_value > Ext::from_f64(0.0) {
            f.close_mode = 99;
        }
        if g.cash[e] < Ext::from_f64(0.0) {
            balance_cash_t_bills(g, e);
        }
        if g.cash[e] < Ext::from_f64(0.0) && g.unused_credit_line > Ext::from_f64(0.0) {
            draw_credit_line(g, ui, rng, f);
        }
        if g.cash[e] < Ext::from_f64(0.0) && !f.skips(0x4d30ba) {
            forced_bond_sales(g, ui, rng, f);
        }
        if g.cash[e] < Ext::from_f64(0.0) && !f.skips(0x5059ac) {
            call_in_player_advances(g, ui, rng, e, 1);
        }
        f.opt_pos_result = find_option_positions(g, ui, e, 0, 0, 0);
        if f.opt_pos_result.abs() == 41 || f.opt_pos_result.abs() == 42 {
            naked_option_check(g, ui, rng, f);
        }
        if g.cash[e] < Ext::from_f64(0.0) {
            balance_cash_t_bills(g, e);
        }
        if g.cash[e] < Ext::from_f64(0.0) {
            f.other_id = 0;
            while g.cash[e] < Ext::from_f64(0.0) {
                f.tmp_long = cast::<i64, _>(f.other_id);
                if 10 < f.tmp_long {
                    g.news_msg = PbStr::from(__content.string(11084))
                        + &g.player_name[e]
                        + __content.string(11085)
                        + &g.ent_symbol[cast::<i32, _>(f.tmp_long)];
                }
                g.tmp_ext1 = cast::<Ext, _>(f.close_mode);
                g.tmp_ext2 = Ext::from_f64(0.0);
                f.other_id = close_option_positions(g, ui, rng, e, 0, 0, 0);
                if cast::<Ext, _>(f.tmp_long) != cast::<Ext, _>(f.other_id)
                    && 10 < f.tmp_long
                    && !g.news_msg.is_empty()
                {
                    post_news(g, ui, 0, 0);
                }
                if f.other_id < 1 {
                    if !g.news_msg.is_empty() {
                        post_news(g, ui, 0, 0);
                    }
                    break;
                }
                if find_option_positions(g, ui, e, 0, 0, 0) == -41 {
                    break;
                }
            }
        }
        if g.cash[e] < -sf(1e-5f32) {
            f.tmp_long = 0;
            if g.cur_player == e || g.solo_human_mode == 1 {
                f.tmp_long = 1;
            } else if 0 < e && e < 11 && g.is_computer_player[g.cur_player] == 1 {
                f.tmp_long = 1;
            }
            if g.cash[e] < Ext::from_f64(0.0) && g.t_bills[e] > Ext::from_f64(0.0) {
                g.cash[e] = g.t_bills[e] + g.cash[e];
                g.t_bills[e] = Ext::from_f64(0.0);
            }
            if f.tmp_long == 1 {
                update_market_display(g, ui, rng, f.time_secs);
                if !f.skips(0x6625d7) {
                    super::transactions::cover_cash_deficit(g, ui, rng, e, &f.now.clone());
                }
                if !(g.cash[e] < Ext::from_f64(0.0)) {
                    return;
                }
                f.other_id = 0;
                if !f.skips(0x632988) {
                    let now = f.now;
                    f.other_id = super::mgmt::commodity_margin_call(g, ui, rng, e, &now);
                }
                if !(g.cash[e] < Ext::from_f64(0.0)) {
                    return;
                }
                if !f.skips(0x4dc1f1) {
                    overdraft_prompt(g, ui, rng, f);
                }
            } else {
                if !f.skips(0x6625d7) {
                    super::transactions::cover_cash_deficit(g, ui, rng, e, &f.now.clone());
                }
                if !(g.cash[e] < Ext::from_f64(0.0)) {
                    return;
                }
                recalc_commod_margin_balances(g, ui, rng, e);
                if g.commod_margin_bal[e] > Ext::from_f64(0.0) && !f.skips(0x62e730) {
                    f.other_id =
                        super::transactions::liquidate_futures_positions(g, ui, rng, e, 1, 1);
                }
                if !(g.cash[e] < Ext::from_f64(0.0)) {
                    return;
                }
                sell_holding_for_cash(g, ui, rng, f);
            }
        }
        if g.cash[e] < -sf(1e-5f32) && !f.skips(0x5059ac) {
            call_in_player_advances(g, ui, rng, e, 2);
        }
        if g.cash[e] < -sf(1e-5f32) && g.player_prepaid_tax[e] > Ext::from_f64(0.0) {
            g.cash[e] = g.player_prepaid_tax[e] + g.cash[e];
            g.player_prepaid_tax[e] = Ext::from_f64(0.0);
            calc_entity_financials(g, ui, rng, e);
            if g.cash[e] > Ext::from_f64(0.0) {
                g.news_msg =
                    g.player_name[e].clone() + __content.string(11086);
                post_news(g, ui, e, 0);
            }
        }
        calc_entity_financials(g, ui, rng, e);
        if g.cash[e] < -sf(1e-5f32)
            && g.commod_margin_bal[e] != Ext::from_f64(0.0)
            && g.tmp_subsid_stock_value == Ext::from_f64(0.0)
            && g.tmp_corp_bonds_value == Ext::from_f64(0.0)
            && g.tmp_gov_bonds_value == Ext::from_f64(0.0)
            && g.net_worth < Ext::from_f64(0.0)
            && g.long_options_value == Ext::from_f64(0.0)
            && !f.skips(0x62e730)
        {
            f.other_id = super::transactions::liquidate_futures_positions(g, ui, rng, e, 2, 1);
        }
        if g.cash[e] < Ext::from_f64(0.0) {
            calc_entity_financials(g, ui, rng, e);
            if !(g.net_worth > Ext::from_f64(0.0)) {
                player_bankruptcy(g, ui, rng, f);
            }
            return;
        }
        g.tmp_bank_loan = g.bank_debt[e];
        if !(g.tmp_bank_loan > Ext::from_f64(0.0)) && !(g.player_short_margin_bal[e] > Ext::from_f64(0.0)) {
            return;
        }
        calc_entity_financials(g, ui, rng, e);
        f.other_id = find_short_positions(g, ui, rng, 0, e);
        if f.other_id == 3 && g.net_worth > Ext::from_f64(0.0) {
            let lim = g.tmp_credit_net_worth * Ext::from_f64(3.0);
            if !(g.tmp_bank_loan > lim) && !(g.player_short_margin_bal[e] > lim) {
                return;
            }
            if g.player_short_margin_bal[e] > g.tmp_credit_net_worth * Ext::from_f64(3.0) {
                f.tries = 0;
                while g.tmp_credit_net_worth > Ext::from_f64(0.0)
                    && g.player_short_margin_bal[e] > g.tmp_credit_net_worth * Ext::from_f64(3.0)
                {
                    f.tries += 1;
                    if !f.skips(0x5dfe3a) {
                        super::mgmt::force_cover_shorts(g, ui, rng, 3, e, 0, 1);
                    }
                    if f.tries == 999 {
                        let mut m = PbStr::from(__content.string(11087));
                        show_quick_message(g, ui, &mut m);
                        return;
                    }
                }
                if !g.news_msg.is_empty() {
                    post_news(g, ui, e, 0);
                }
            }
            calc_entity_financials(g, ui, rng, e);
        }
        if !(g.tmp_bank_loan > g.tmp_credit_net_worth * Ext::from_f64(3.0)) && !(g.cash[e] < Ext::from_f64(0.0)) {
            return;
        }
        f.amount = g.tmp_bank_loan - g.tmp_credit_net_worth * Ext::from_f64(3.0);
        if f.amount > g.tmp_bank_loan || g.tmp_credit_net_worth < Ext::from_f64(0.0) {
            f.amount = g.tmp_bank_loan * sf(0.2f32);
            if Ext::from_f64(50.0) > f.amount {
                f.amount = g.tmp_bank_loan * sf(0.5f32);
            }
            if Ext::from_f64(20.0) > f.amount {
                f.amount = g.tmp_bank_loan;
                g.tmp_bank_loan = Ext::from_f64(0.0);
            }
        }
        if g.tmp_credit_net_worth < Ext::from_f64(0.0)
            && g.rand_seed % 3 == 1
            && instr(1, g.ticker_text.as_bytes(), b"SPIRAL") == 0
        {
            g.ticker_text = g.ticker_text.clone()
                + __content.string(11088)
                + &ucase(g.player_name[e].as_bytes(), CaseMode::Ansi)
                + __content.string(11089);
        }
        g.cash[e] = g.cash[e] - f.amount;
        g.bank_debt[e] = g.bank_debt[e] - f.amount;
        let l = g.lender_id;
        g.cash[l] = f.amount + g.cash[l];
        f.msg_part1 = g.currency_symbol.clone()
            + &fmt_trim(f.amount, b"* ###,")
            + " "
            + &g.million_prefix_uc
            + "ILLION";
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
        if f.amount > Ext::from_f64(0.0) {
            g.news_msg = ucase(g.player_name[e].as_bytes(), CaseMode::Ansi)
                + __content.string(11090)
                + &f.msg_part1;
        }
        g.ticker_text = g.ticker_text.clone() + "    " + &g.news_msg;
        f.margin_call_count += 1;
        if f.margin_call_count == 999 {
            return;
        }
        if get_controller_type(g, e) == 2 {
            return;
        }
        if g.tmp_bank_loan == Ext::from_f64(0.0) {
            return;
        }
        if g.cur_player == e || g.difficulty_level != 3 {
            return;
        }
    }
}

/// The player with negative net worth loses the CEO job at a company he doesn't control
/// (SimPlayer_CheckSolvency).
fn fired_as_ceo(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    let c = g.player_ceo_of[e];
    f.msg_part1 = PbStr::new();
    f.cmp_tmp = 15;
    if len(g.player_name[e].as_bytes()) < f.cmp_tmp {
        f.msg_part1 = PbStr::from(__content.string(11091));
    }
    g.news_msg = g.player_name[e].clone()
        + __content.string(11092)
        + &f.msg_part1
        + __content.string(11093)
        + &g.ent_symbol[c];
    f.msg_part1 = PbStr::from(__content.string(11094));
    if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0) {
        f.msg_part1 = PbStr::from(__content.string(11095));
    }
    let mut m =
        g.news_msg.clone() + __content.string(11096) + &f.msg_part1 + __content.string(11097);
    show_quick_message(g, ui, &mut m);
    post_news(g, ui, 0, 0);
    if g.is_computer_player[e] == 0 {
        let c = g.player_ceo_of[e];
        g.auto_pilot[c] = 0;
        f.cash_flow_score = g.corp_controller[c];
        let p = f.cash_flow_score;
        if 0 < p && p <= g.num_players && g.is_computer_player[p] == 0 {
            g.auto_pilot[c] = g.auto_pilot[p];
        }
    }
    g.player_ceo_of[e] = 0;
    g.player_ceo_since_year[e] = 0;
    if !f.skips(0x5fd937) {
        revoke_ex_ceo_options(g, ui);
    }
}

/// Forced closing of naked option positions when the player's credit can't cover the liability
/// (SimPlayer_CheckSolvency).
fn naked_option_check(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    calc_entity_financials(g, ui, rng, e);
    f.naked_liab = g.long_options_value - g.options_net_value;
    if f.naked_liab > sf(0.01f32) {
        f.naked_liab = get_plr_naked_liab(g, ui, rng, e, 0, 0);
    }
    if !(g.tmp_credit_net_worth < f.naked_liab * sf(0.333f32)) {
        return;
    }
    if g.net_worth > f.naked_liab * sf(0.5f32) && 7 < g.credit_rating_num[e] && !(g.cash[e] < Ext::from_f64(0.0)) {
        return;
    }
    f.tmp_long = to_quad(Ext::from_f64(7.0) * rng.rnd() + Ext::from_f64(2.0));
    f.player_count = to_long(cast::<Ext, _>(f.tmp_long));
    f.idx = 1;
    while f.idx <= f.player_count {
        f.msg_part1 = PbStr::from("sell");
        if find_option_positions(g, ui, e, 0, 0, 0) == -41 {
            f.msg_part1 = PbStr::from(__content.string(11098));
        }
        g.tmp_ext1 = cast::<Ext, _>(f.close_mode);
        if !(g.net_worth < Ext::from_f64(0.0)) {
            g.tmp_ext2 = Ext::from_f64(0.0);
        } else {
            g.tmp_ext2 = Ext::from_f64(99.0);
        }
        f.other_id = close_option_positions(g, ui, rng, e, 0, 0, 0);
        if 10 < f.other_id {
            g.news_msg = g.player_name[e].clone()
                + __content.string(11099)
                + &f.msg_part1
                + __content.string(11100)
                + &g.ent_symbol[f.other_id];
            if g.tmp_credit_net_worth > Ext::from_f64(0.0) {
                calc_entity_financials(g, ui, rng, e);
                f.naked_liab = get_plr_naked_liab(g, ui, rng, e, 0, 0);
                if !(f.naked_liab > sf(0.01f32)) {
                    break;
                }
                if f.naked_liab * sf(0.333f32) < g.tmp_credit_net_worth {
                    break;
                }
                if g.net_worth > f.naked_liab * sf(0.5f32)
                    && 7 < g.credit_rating_num[e]
                    && !(g.cash[e] < Ext::from_f64(0.0))
                {
                    break;
                }
            }
        }
        f.idx += 1;
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, e, 0);
    }
}

/// The player goes bankrupt (SimPlayer_CheckSolvency): swaps cancelled, headline and ticker, the lender
/// writes off the bank loan, the player is out of the game, holdings and option/futures/commodity
/// records are removed.
fn player_bankruptcy(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    let e = f.ent_id;
    cancel_entity_swaps(g, ui, rng, e, 1, 2);
    g.pending_popup_entity = e;
    g.pending_popup_text = ucase(f.ent_name.as_bytes(), CaseMode::Ansi);
    g.pending_popup_pic = rng.rnd_range(478, 477);
    // GetRandomRemark (0x480da1): TmpNewsStr = WSRDLL.DLL!RemarksData(1) (external DLL, see
    // logic::report) — taken as "".
    g.tmp_news_str = PbStr::new();
    f.msg_part1 = f.ent_name.clone() + __content.string(11101);
    f.msg_part2 = PbStr::from(__content.string(11102));
    match g.rand_seed % 4 {
        1 => f.msg_part2 = PbStr::from(__content.string(11103)),
        2 => f.msg_part2 = PbStr::from(__content.string(11104)),
        3 => f.msg_part2 = PbStr::from(__content.string(11105)),
        _ => {}
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.news_msg = f.msg_part1.clone() + &f.msg_part2;
    post_news(g, ui, 0, 0);
    g.ticker_text = PbStr::from("  ")
        + &string_fill(5, b".")
        + &ucase(f.ent_name.as_bytes(), CaseMode::Ansi)
        + __content.string(11106);
    // (UI) CONTROL SET TEXT hDlgMain, 123, TickerText
    g.lender_id = cast::<i32, _>(g.lending_bank[e]);
    let l = g.lender_id;
    f.amount = g.bank_debt[e];
    if f.amount > Ext::from_f64(0.0) {
        g.bank_debt[e] = Ext::from_f64(0.0);
        g.bad_debt_reserve[l] = g.bad_debt_reserve[l] - f.amount;
        if g.bad_debt_reserve[l] < Ext::from_f64(0.0) {
            g.accrued_pl[l] = (g.bad_debt_reserve[l] + g.accrued_pl[l]) - g.exchange_rate * Ext::from_f64(3.0);
            g.bad_debt_reserve[l] = g.exchange_rate * Ext::from_f64(3.0);
        }
        g.news_msg = g.ent_symbol[l].clone()
            + __content.string(11107)
            + &fmt_trim(f.amount, b"* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "il. on "
            + &g.player_name[e]
            + " loan.";
    }
    g.player_name[e] = PbStr::from("...OUT...");
    if g.cur_player == e {
        g.cur_player_name = g.player_name[e].clone();
    }
    g.cash[e] = Ext::from_f64(0.0);
    g.accrued_pl[e] = Ext::from_f64(0.0);
    g.extraordinary_pl[e] = 0;
    g.entity_status[e] = 2;
    g.ent_misc_data[e] = PbStr::from("BKRUP");
    if g.is_computer_player[e] == 1 {
        g.cpu_max_corps += 15;
    }
    if g.cur_player == e {
        g.turn_action_count = 7;
    }
    g.solo_human_mode = 0;
    if g.cur_player == e {
        rebuild_streaming_quotes_panel(g, ui, 3);
    }
    f.idx = 1;
    while f.idx <= 15 {
        g.hold_stock_id[(f.idx, e)] = 0;
        g.hold_stock_pct[(f.idx, e)] = 0;
        f.idx += 1;
    }
    g.player_short_margin_bal[e] = Ext::from_f64(0.0);
    f.idx = 1;
    while f.idx <= 1000 {
        let i = f.idx;
        if val(parse_any(g.option_recs[i].as_bytes(), b"|", 3).as_bytes()) == cast::<Ext, _>(e) {
            add_or_remove_option_rec(g, ui, &PbStr::new(), f.idx);
        }
        if val(parse_any(g.futures_recs[i].as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(e) {
            add_or_remove_futures_rec(g, &PbStr::new(), f.idx);
        }
        if val(parse_any(g.phys_commod_rec[i].as_bytes(), b"|", 2).as_bytes()) == cast::<Ext, _>(e) {
            g.phys_commod_rec[i] = PbStr::new();
        }
        f.idx += 1;
    }
    if !f.skips(0x49117c) {
        pack_holding_arrays(g, ui);
    }
    if cast::<i32, _>(g.insider_tippee) == e {
        g.tipster_state[cast::<i32, _>(g.insider_tippee)] = -1;
        clear_insider_tip(g);
        if e == g.insider_probe_player {
            g.insider_probe_player = 0;
        }
    }
    if e == g.merger_proposer_player && !f.skips(0x52962a) {
        cancel_pending_merger(g, ui, 0);
    }
}

/// GOSUB SimPlayer_OverdraftPrompt (0x4dc1f1): the human player with an overdraft is asked whether to
/// choose the stock to sell (the trading dialog) or let the computer sell (SimEntity_SellHoldingForCash),
/// until the cash is positive or nothing is left to sell.
pub(super) fn overdraft_prompt(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut Frame) {
    let __content = crate::content::current();
    loop {
        update_market_display(g, ui, rng, f.time_secs);
        let e = f.ent_id;
        calc_entity_financials(g, ui, rng, e);
        if !(g.cash[e] < Ext::from_f64(0.0)) {
            return;
        }
        if !f.skips(0x49117c) {
            pack_holding_arrays(g, ui);
        }
        f.tries = cast::<i32, _>(g.hold_stock_id[(0, e)]);
        if f.tries == 0 {
            return;
        }
        if f.tries == 1 {
            sell_holding_for_cash(g, ui, rng, f);
            return;
        }
        f.amount = g.cash[e].abs();
        f.msg_part1 = g.currency_symbol.clone()
            + &fmt_trim(f.amount, b"* ###,.00")
            + " "
            + &g.million_prefix_uc
            + "ILLION"
            + &g.currency_suffix
            + "!";
        f.msg_part2 = PbStr::from(__content.string(11108))
            + __content.string(11109)
            + &f.msg_part1
            + "\r"
            + "\r"
            + __content.string(11110)
            + "\r"
            + "\r"
            + __content.string(11111);
        let title = PbStr::from(__content.string(11112)) + &g.cur_player_name;
        f.answer = ui.msgbox(&f.msg_part2, 0x2024, &title);
        if f.answer == 6 {
            crate::logic::economy::close_all_dialogs(g, ui, rng, 1);
            if !f.skips(0x4721dc) {
                let now = f.now;
                crate::logic::trading::select_acting_entity(g, ui, rng, 1, &now);
            }
            // (UI) DisableMainControls
            if !f.skips(0x5ab449) {
                crate::logic::trading::sell_stock(g, ui, rng, f.time_secs);
            }
            // (UI) EnableMainControls
        } else {
            sell_holding_for_cash(g, ui, rng, f);
        }
        update_market_display(g, ui, rng, f.time_secs);
        if !(g.cash[f.ent_id] < Ext::from_f64(0.0)) {
            return;
        }
    }
}
