//! `BuySellLoans` (0x58a220) with its GOSUBs, and the loan-type dialog `ShowLoanTradeTypeDialog`
//! (0x5b856f).
//!
//! Pick list: the player picks a business loan in a report (`EntityPickMode = "GetCorp"`); a double click
//! runs `FollowReportLink` (0x46c9f4, called by the report dialog), which sets `TargetCorp`.
use crate::pb::real::{Ext, cast};


#[allow(unused_imports)]
use super::super::prelude::*;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::finance::{
    calc_entity_financials, get_controller_type, get_market_interest_rate,
};
use crate::logic::newgame_helpers::{
    bank_has_few_borrowers, calc_loan_sale_price_pct, sell_loan_to_bank,
};
use crate::logic::report::{show_quick_message, show_text_report};
use crate::logic::reports_lists::pb_array_sort;
use crate::logic::research::bank_regulator_lending_check;
use crate::logic::restructure::adjust_dialog_frame_size;
use crate::logic::sim_entity::corp_ops::bank_allocate_assets;
use crate::pb::string::{CaseMode, compare, remove_any, string_fill, ucase};

use super::super::bank::confirm_subprime_sale_discount;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// The locals of BuySellLoans that its GOSUBs share.
struct Frame {
    trade_type: i32,
    /// `amount`: the price factor of CalcPriceFactor, an amount to spend, a discount fraction …
    amount: Ext,
    borrower_id: i32,
    answer: i32,
    line_count: i32,
    /// `DIM listLines(1 TO 1600) AS STRING`.
    list_lines: Vec<PbStr>,
    /// `DIM sortKeys(1 TO 1600) AS LONG`.
    sort_keys: Vec<i32>,
    /// `DIM bankLoanCount(11 TO 85) AS LONG`, indexed by the bank id.
    bank_loan_count: Vec<i32>,
}

impl Frame {
    /// `bankLoanCount(bank)`; `None` outside 11..85 (error 9 in the original: the handler at 0x58d3a3
    /// shows the runtime error and leaves the SUB).
    fn blc(&mut self, bank: i32) -> Option<&mut i32> {
        if (11..=85).contains(&bank) {
            Some(&mut self.bank_loan_count[cast::<usize, _>(bank)])
        } else {
            None
        }
    }

    /// Port of BuySellLoans_CalcPriceFactor (0x58d16c): `amount` = the sale price (fraction of face) of
    /// `borrowerId`'s loan (`CalcLoanSalePricePct`, which also sets `CreditRatingStr`); a loan sold off
    /// (type 5) fetches at most 101%.
    fn calc_price_factor(&mut self, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
        let price_pct = calc_loan_sale_price_pct(g, ui, rng, self.borrower_id);
        self.amount = sf(0.01f32) * cast::<Ext, _>(price_pct);
        if self.trade_type == 5 && Ext::from_f64(1.0) < self.amount {
            self.amount = sf(1.01f32);
        }
    }

    /// Port of BuySellLoans_AskAnother (0x58d1b5): balances cash / T-bills and asks whether to trade
    /// another loan (YES clears the list). Returns false where the GOSUB jumps to the SUB's exit (a loan
    /// buyer without funds or loan offers left).
    fn ask_another(&mut self, g: &mut Globals, ui: &mut dyn Ui) -> bool {
    let __content = crate::content::current();
        let ae = g.active_entity;
        balance_cash_t_bills(g, ae);
        if self.trade_type == 1 {
            if g.cash[ae] + g.t_bills[ae] <= Ext::from_f64(0.0) {
                return false;
            }
            if g.player_loan_offers_avail[g.cur_player] < 1 {
                return false;
            }
        }
        let mut msg_text = s(__content.string(3682)) + &g.active_entity_name + "?";
        if self.trade_type == 5 {
            msg_text = s(__content.string(3683));
        }
        let title = g.app_title.clone();
        self.answer = ui.msgbox(&msg_text, 8228, &title);
        if self.answer == 6 {
            for corp_id in 1..=self.line_count + 1 {
                self.list_lines[cast::<usize, _>(corp_id)] = PbStr::new();
                self.sort_keys[cast::<usize, _>(corp_id)] = 0;
            }
        }
        true
    }
}

/// Port of BuySellLoans_Rebalance (0x58d367): ten passes of `BankAllocateAssets(ActiveEntity, 3, 0)`.
fn rebalance(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    for _ in 0..10 {
        let ae = g.active_entity;
        bank_allocate_assets(g, ui, rng, ae, 3, 0);
    }
}

/// Port of BuySellLoans (0x58a220) with its GOSUBs BuySellLoans_CalcPriceFactor (0x58d16c),
/// BuySellLoans_AskAnother (0x58d1b5), BuySellLoans_Rebalance (0x58d367) and the exit
/// BuySellLoans_ExitSub (0x58d431).
///
/// A bank (or insurer) buys or sells loans; `requested_type` 1..8 or, when 0, the choice of
/// [`show_loan_trade_type_dialog`] (`TmpExt1`): 1 buy business loans from other banks, 2/3/4 buy
/// consumer / prime mortgage / subprime mortgage loans, 5 sell business loans to other banks, 6/7/8 sell
/// consumer / prime mortgage / subprime loans. Purchases need funds over 0.5 and pass the regulators'
/// loans-to-deposits check (`BankRegulatorLendingCheck`).
///
/// * Type 1 needs a loan offer (`PlayerLoanOffersAvail`) and a positive net worth. Loans are offered by
///   banks with [runtime resource wording] ; the player picks one from a list (sorted by credit rating or
///   by name) or the program searches one of about the amount entered (80..110%, at most 35% of the
///   bank's loans or 10,000 million; [runtime resource wording] players, borrowers rated 2+), starting at a
///   random entity (`RND(850, 86)`) and once more from 86. The loan is bought through `SellLoanToBank`.
/// * Types 2..4 add the amount (`BankConsumerLoans` / `BankMortgageLoans` / `SubprimeSecs`) for cash;
///   no subprime purchases during the subprime crisis in odd years.
/// * Type 5 lists the bank's business loans; a bank with the cash to pay the price buys the picked one.
/// * Types 6..8 sell at face value; subprime securities at a discount (`ConfirmSubprimeSaleDiscount`)
///   charged to the bad-debt reserve or the accrued P/L.
///
/// Each trade posts news and counts as a turn action (at most 5 without `SoloHumanMode`).
pub fn buy_sell_loans(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, requested_type: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x58d3a3 (ShowRuntimeError, then the exit): only bankLoanCount() can fail here.
    let mut f = Frame {
        trade_type: 0,
        amount: Ext::from_f64(0.0),
        borrower_id: 0,
        answer: 0,
        line_count: 0,
        list_lines: vec![PbStr::new(); 1601],
        sort_keys: vec![0; 1601],
        bank_loan_count: vec![0; 86],
    };
    g.tmp_ext1 = Ext::from_f64(0.0);
    if requested_type == 0 {
        show_loan_trade_type_dialog(g, ui, 0);
    }
    f.trade_type = to_long(g.tmp_ext1);
    g.tmp_ext1 = Ext::from_f64(0.0);
    if 0 < requested_type {
        f.trade_type = requested_type;
    }
    let tt = f.trade_type;
    if tt < 1 || 8 < tt {
        return;
    }
    let ae = g.active_entity;
    calc_entity_financials(g, ui, rng, ae);
    let mut avail_funds = g.liquid_funds;
    let mut msg_text = PbStr::new();
    let mut field_text: PbStr;
    if tt == 1 {
        if Ext::from_f64(0.0) <= g.net_worth {
            if g.player_loan_offers_avail[g.cur_player] < 1 {
                msg_text = s(
                    __content.string(3684),
                ) + __content.string(3685);
            }
        } else {
            msg_text = s(
                __content.string(3686),
            ) + __content.string(3687)
                + &g.active_entity_name
                + __content.string(3688)
                + __content.string(3689)
                + __content.string(3690);
        }
        if !msg_text.is_empty() {
            show_quick_message(g, ui, &mut msg_text);
            return;
        }
    }
    if tt < 5 && avail_funds <= sf(0.5f32) {
        let mut m =
            g.active_entity_name.clone() + __content.string(3691);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if tt < 5 && bank_regulator_lending_check(g, ui, ae, 0) == 1 {
        msg_text = s(__content.string(3692))
            + &g.active_entity_name
            + __content.string(3693)
            + __content.string(3694)
            + __content.string(3695)
            + __content.string(3696)
            + __content.string(3697);
        ui.msgbox(&msg_text, 8256, &s(__content.string(3698)));
        return;
    }
    if tt == 1 {
        msg_text = s(__content.string(3699))
            + __content.string(3700)
            + "\r"
            + "\r"
            + __content.string(3701)
            + __content.string(3702)
            + __content.string(3703)
            + "\r"
            + "\r"
            + __content.string(3704)
            + __content.string(3705);
        f.answer = ui.msgbox(&msg_text, 8227, &s(__content.string(3706)));
        if f.answer == 2 {
            return;
        }
        for entity_id in 1..=1600 {
            if (10 < entity_id || entity_id <= g.num_players)
                && g.entity_status[entity_id] == 1
                && Ext::from_f64(1.0) <= g.bank_debt[entity_id]
                && 85 < entity_id
            {
                let Some(c) = f.blc(cast::<i32, _>(g.lending_bank[entity_id])) else {
                    return;
                };
                *c += 1;
            }
        }
        for entity_id in 11..=85 {
            if g.entity_status[entity_id] == 1 && g.active_entity != entity_id {
                if f.bank_loan_count[cast::<usize, _>(entity_id)] < 11 {
                    continue;
                }
                if get_controller_type(g, entity_id) != 2 {
                    f.line_count += 1;
                }
            }
        }
        if f.line_count == 0 {
            msg_text = s(
                __content.string(3707),
            ) + &g.active_entity_name
                + __content.string(3436);
            ui.msgbox(&msg_text, 8256, &s(__content.string(3708)));
            return;
        }
        f.line_count = 0;
    }
    let loan_type_desc = match tt {
        1 => s(__content.string(3709)),
        2 | 6 => s(__content.string(3710)),
        3 | 7 => s(__content.string(3711)),
        4 | 8 => s(__content.string(3712)),
        _ => s(__content.string(3713)),
    };
    if 0 < g.subprime_crisis_state && tt == 4 && g.cur_year % 2 == 1 {
        msg_text = s("banking");
        field_text = s("banks");
        if 85 < g.active_entity {
            msg_text = s(__content.string(2822));
            field_text = s(__content.string(3714));
        }
        let mut m = s(__content.string(3715))
            + &msg_text
            + __content.string(3716)
            + __content.string(3717)
            + &field_text
            + __content.string(3718)
            + __content.string(3719);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if f.answer == 6 || tt == 5 {
        loan_list(g, ui, rng, &mut f);
        return;
    }
    if tt < 5 {
        let mut bank_id = 0;
        let mut found = 0;
        loop {
            let ae = g.active_entity;
            msg_text = s(__content.string(3720))
                + &g.active_entity_name
                + __content.string(3721)
                + __content.string(3722)
                + &loan_type_desc
                + __content.string(3723)
                + __content.string(3724);
            avail_funds = g.cash[ae] + g.t_bills[ae];
            field_text = fmt_trim(avail_funds, b"* ###,.00");
            if found != -1 {
                loop {
                    let title = g.ent_symbol[ae].clone() + __content.string(3725);
                    let input_text = ui.inputbox(&msg_text, &title, &field_text);
                    f.amount = val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes());
                    if f.amount <= Ext::from_f64(0.0) {
                        return;
                    }
                    if !(avail_funds + sf(0.01f32) <= f.amount) {
                        break;
                    }
                }
            }
            if 1 < tt {
                buy_loan_pool(g, ui, rng, tt, f.amount);
                return;
            }
            if found == 1 {
                found = 0;
            }
            if bank_id == 0 {
                bank_id = rng.rnd_range(850, 86);
            }
            let calc_entity = g.active_entity;
            calc_entity_financials(g, ui, rng, calc_entity);
            let cap = g.exchange_rate * Ext::from_f64(10000.0);
            if g.tmp_bank_total_loans * sf(0.35f32) < f.amount && cap <= f.amount {
                if cap <= g.tmp_bank_total_loans * sf(0.35f32) || f.amount <= cap {
                    f.amount = g.tmp_bank_total_loans * sf(0.35f32);
                } else {
                    f.amount = cap;
                }
                let mut m = s(
                    __content.string(3726),
                ) + __content.string(3727)
                    + &fmt_trim(f.amount, b"* ***,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix;
                show_quick_message(g, ui, &mut m);
            }
            for corp_id in bank_id..=1600 {
                let ae = g.active_entity;
                if g.entity_status[corp_id] != 1 || cast::<i32, _>(g.lending_bank[corp_id]) == ae {
                    continue;
                }
                let debt = g.bank_debt[corp_id];
                if debt < f.amount * sf(0.8f32)
                    || f.amount * sf(1.1f32) < debt
                    || !(debt <= avail_funds * sf(0.9f32))
                {
                    continue;
                }
                g.lender_id = cast::<i32, _>(g.lending_bank[corp_id]);
                let lender = g.lender_id;
                if 0 < g.corp_controller[lender] && g.corp_controller[lender] < 11 {
                    continue;
                }
                // BankHasFewBorrowers(LenderId) gets the global BYREF; its CalcEntityFinancials(bank) sets
                // LenderId = LendingBank(bank) until CalcEntityFinancials(corpId) below restores it.
                // BUG (original, #310): the loan count tested is that of the lending bank's own lender.
                if 0 < bank_has_few_borrowers(g, ui, rng, lender) {
                    continue;
                }
                let Some(&mut count) = f.blc(g.lender_id) else {
                    return;
                };
                if !(10 < count) || !(g.exchange_rate * Ext::from_f64(10.0) <= g.bank_debt[corp_id]) {
                    continue;
                }
                calc_entity_financials(g, ui, rng, corp_id);
                if g.credit_rating_num[corp_id] < 2 {
                    continue;
                }
                let lender = g.lender_id;
                let Some(c) = f.blc(lender) else { return };
                *c -= 1;
                sell_loan_to_bank(g, ui, rng, ae, lender, corp_id);
                if 0 < instr(1, g.news_msg.as_bytes(), __content.bytes(3728))
                    && 0 < instr(1, g.news_msg.as_bytes(), g.ent_symbol[corp_id].as_bytes())
                {
                    let mut m = std::mem::take(&mut g.news_msg);
                    show_quick_message(g, ui, &mut m);
                    g.news_msg = m;
                    post_news(g, ui, 0, 0);
                    rebalance(g, ui, rng);
                }
                found = 1;
                g.turn_action_count = g.turn_action_count + 1;
                if tt == 1 {
                    let cp = g.cur_player;
                    if 0 < g.player_loan_offers_avail[cp] {
                        g.player_loan_offers_avail[cp] = g.player_loan_offers_avail[cp] - 1;
                        if g.player_loan_offers_avail[cp] < 1 {
                            return;
                        }
                    }
                }
                if 4 < g.turn_action_count && g.solo_human_mode == 0 {
                    return;
                }
                break; // corpId = 1600
            }
            if found == 0 {
                bank_id = 86;
                found = -1;
                continue;
            }
            if found == -1 {
                let mut m = s(__content.string(3729));
                show_quick_message(g, ui, &mut m);
                found = 0;
            }
            if !f.ask_another(g, ui) {
                return;
            }
            if f.answer != 6 {
                return;
            }
        }
    }
    sell_loan_pool(g, ui, &mut f, &loan_type_desc);
}

/// BuySellLoans, types 2..4: buys consumer loans / prime mortgages / subprime securities for `amount`
/// cash and confirms with the current yield (`GetMarketInterestRate(tradeType - 1)`).
fn buy_loan_pool(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, tt: i32, amount: Ext) {
    let __content = crate::content::current();
    let ae = g.active_entity;
    g.cash[ae] = g.cash[ae] - amount;
    if tt == 2 {
        g.bank_consumer_loans[ae] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[ae]) + amount);
    } else if tt == 3 {
        g.bank_mortgage_loans[ae] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[ae]) + amount);
    } else {
        g.subprime_secs[ae] = to_quad(cast::<Ext, _>(g.subprime_secs[ae]) + amount);
    }
    if g.cash[ae] < Ext::from_f64(0.0) {
        balance_cash_t_bills(g, ae);
    }
    let mut msg_text = s(__content.string(3710));
    if tt == 3 {
        msg_text = s(__content.string(3730));
    }
    if tt == 4 {
        msg_text = s(__content.string(3731));
    }
    g.news_msg = g.ent_symbol[ae].clone() + __content.string(3732) + &msg_text + ".";
    let mut field_text = fmt_trim(amount, b"* ###,");
    let mut confirm_text = s(__content.string(3733))
        + "\r"
        + "\r"
        + &g.active_entity_name
        + __content.string(3734)
        + &msg_text
        + " in "
        + __content.string(3308)
        + &trim(g.currency_symbol.as_bytes())
        + &field_text
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + "."
        + "\r"
        + "\r"
        + __content.string(3735);
    let rate = get_market_interest_rate(g, rng, tt - 1);
    field_text = fmt_trim(rate, b"* ###,.00") + "%.";
    confirm_text = confirm_text + &field_text;
    ui.msgbox(&confirm_text, 8192, &s(__content.string(3736)));
    post_news(g, ui, 0, 0);
    g.turn_action_count = g.turn_action_count + 1;
}

/// BuySellLoans, types 6..8: sells (part of) the consumer loans / prime mortgages / subprime securities.
fn sell_loan_pool(g: &mut Globals, ui: &mut dyn Ui, f: &mut Frame, loan_type_desc: &PbStr) {
    let __content = crate::content::current();
    let tt = f.trade_type;
    let ae = g.active_entity;
    let held = match tt {
        6 => g.bank_consumer_loans[ae],
        7 => g.bank_mortgage_loans[ae],
        _ => g.subprime_secs[ae],
    };
    if held <= 0 {
        let mut m = g.active_entity_name.clone() + __content.string(3737) + loan_type_desc + __content.string(3738);
        show_quick_message(g, ui, &mut m);
        return;
    }
    f.amount = cast::<Ext, _>(held);
    let field_text = fmt_trim(f.amount, b"* ###,");
    let mut sale_amount: i64;
    loop {
        let prompt = s(__content.string(3739))
            + &g.ent_symbol[ae]
            + __content.string(3740)
            + loan_type_desc
            + __content.string(3741);
        let title = g.active_entity_name.clone() + __content.string(3742);
        let input_text = ui.inputbox(&prompt, &title, &field_text);
        sale_amount = to_quad(val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes()));
        if sale_amount <= 0 {
            return;
        }
        if !(f.amount < cast::<Ext, _>(sale_amount)) {
            break;
        }
    }
    let mut proceeds = sale_amount;
    let mut discount_pct = 0;
    match tt {
        6 => {
            g.bank_consumer_loans[ae] =
                to_quad(cast::<Ext, _>(g.bank_consumer_loans[ae]) - cast::<Ext, _>(sale_amount))
        }
        7 => {
            g.bank_mortgage_loans[ae] =
                to_quad(cast::<Ext, _>(g.bank_mortgage_loans[ae]) - cast::<Ext, _>(sale_amount))
        }
        _ => {
            let quality = g.profit_margin[ae];
            discount_pct = confirm_subprime_sale_discount(g, ui, 1, quality);
            if discount_pct < 0 {
                return;
            }
            if 0 < discount_pct {
                f.amount = cast::<Ext, _>(discount_pct) * sf(0.01f32);
                let loss = f.amount * cast::<Ext, _>(proceeds);
                if ae < 86 && !(g.bad_debt_reserve[ae] <= loss) {
                    g.bad_debt_reserve[ae] = g.bad_debt_reserve[ae] - loss;
                } else {
                    g.accrued_pl[ae] = g.accrued_pl[ae] - loss;
                }
                proceeds = to_quad(cast::<Ext, _>(proceeds) - f.amount * cast::<Ext, _>(proceeds));
            }
            g.subprime_secs[ae] = to_quad(cast::<Ext, _>(g.subprime_secs[ae]) - cast::<Ext, _>(sale_amount));
        }
    }
    g.cash[ae] = g.cash[ae] + cast::<Ext, _>(proceeds);
    let mut msg_text = s(__content.string(3743))
        + "\r"
        + "\r"
        + &g.active_entity_name
        + __content.string(3012)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(cast::<Ext, _>(sale_amount), b"* ###,")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + __content.string(3744)
        + loan_type_desc;
    if 0 < discount_pct {
        msg_text = msg_text
            + __content.string(3745)
            + &str_s(cast::<Ext, _>(discount_pct))
            + __content.string(3746);
    }
    msg_text = msg_text + ".";
    ui.msgbox(&msg_text, 8192, &s(__content.string(3747)));
    g.news_msg = g.ent_symbol[ae].clone()
        + __content.string(3748)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(cast::<Ext, _>(sale_amount), b"* ###,")
        + " "
        + &g.million_prefix_lc
        + "il. of "
        + loan_type_desc
        + ".";
    post_news(g, ui, 0, 0);
    g.turn_action_count = g.turn_action_count + 1;
}

/// BuySellLoans, type 1 with the list (the player answered YES) and type 5: lists the loans offered
/// (type 1: loans of borrowers outside the banks, other than the player, of banks with more than 10
/// loans, at least 10 million, not owed to the buyer, at most 5 times its liquid funds, affordable, and
/// of a bank no other player controls) or held (type 5: the bank's own borrowers outside the banks that
/// the player does not control), lets the player pick one and trades it. Repeats while the player wants
/// another.
fn loan_list(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    f: &mut Frame,
) {
    let __content = crate::content::current();
    let tt = f.trade_type;
    let mut sort_mode = 0;
    loop {
        let mut found = 0;
        let mut msg_text = s(__content.string(3749));
        if tt == 5 {
            msg_text = s(__content.string(3750));
        }
        f.list_lines[1] = s(__content.string(3751)) + &msg_text + &g.active_entity_name + ":";
        f.list_lines[2] = string_fill(50, b" ");
        f.list_lines[3] =
            s("-------------------------- ------ -------------- -------------- ------");
        if tt == 5 {
            f.list_lines[3] = s("-------------------------- ------ -------------- -------------- ");
        }
        msg_text = s(__content.string(3752));
        if tt == 5 {
            msg_text = s("SELLING");
        }
        f.list_lines[4] = s(__content.string(3753)) + &msg_text;
        msg_text = s("BANK ");
        if tt == 5 {
            msg_text = s("     ");
        }
        f.list_lines[5] =
            s(__content.string(3754)) + &msg_text;
        f.list_lines[6] = f.list_lines[3].clone();
        f.line_count = 6;
        let mut corp_id = 1;
        while corp_id < 1601 {
            'next: {
                let c = corp_id;
                if g.entity_status[c] != 1 || !(Ext::from_f64(0.0) < g.bank_debt[c]) {
                    break 'next;
                }
                let ae = g.active_entity;
                if tt == 1 {
                    if 10 < c && c < 86 {
                        break 'next;
                    }
                    if g.cur_player == c {
                        break 'next;
                    }
                    if c <= g.num_players && g.difficulty_level < 3 {
                        break 'next;
                    }
                    let Some(&mut count) = f.blc(cast::<i32, _>(g.lending_bank[c])) else {
                        return;
                    };
                    if count < 11 || !(g.exchange_rate * Ext::from_f64(10.0) <= g.bank_debt[c]) {
                        break 'next;
                    }
                }
                if tt == 5 && 10 < c && c < 86 {
                    break 'next;
                }
                if tt == 1 {
                    let ctrl = g.corp_controller[c];
                    if ctrl <= g.num_players
                        && g.cur_player != ctrl
                        && 85 < c
                        && g.difficulty_level == 1
                    {
                        break 'next;
                    }
                    let bank_ctrl = g.corp_controller[cast::<i32, _>(g.lending_bank[c])];
                    if bank_ctrl <= g.num_players && g.cur_player != bank_ctrl {
                        break 'next;
                    }
                    if cast::<i32, _>(g.lending_bank[c]) == ae {
                        break 'next;
                    }
                    if (g.cash[ae] + g.t_bills[ae]) * Ext::from_f64(5.0) < g.bank_debt[c] {
                        break 'next;
                    }
                } else {
                    if cast::<i32, _>(g.lending_bank[c]) != ae {
                        break 'next;
                    }
                    if g.cur_player == g.corp_controller[c] {
                        break 'next;
                    }
                }
                f.borrower_id = c;
                f.calc_price_factor(g, ui, rng);
                if tt == 1 && g.cash[ae] + g.t_bills[ae] + g.cash[c] < f.amount * g.bank_debt[c] {
                    break 'next;
                }
                f.line_count += 1;
                found = 1;
                msg_text = ucase(g.ent_name[c].as_bytes(), CaseMode::Ansi) + &string_fill(28, b" ");
                msg_text = mid(msg_text.as_bytes(), 1, 28);
                let mut field_text = s("   ") + &g.credit_rating_str;
                field_text = right(field_text.as_bytes(), 3);
                msg_text = msg_text + &field_text + " ";
                field_text = right(fmt(g.bank_debt[c], b"* ###########.00").as_bytes(), 14);
                msg_text = msg_text + &field_text + " ";
                field_text = right(
                    fmt(g.bank_debt[c] * f.amount, b"* ###########.00").as_bytes(),
                    14,
                );
                msg_text = msg_text + &field_text + "    ";
                if tt == 1 {
                    field_text = g.ent_symbol[cast::<i32, _>(g.lending_bank[c])].clone();
                    msg_text = msg_text + &field_text;
                }
                let lc = cast::<usize, _>(f.line_count);
                f.list_lines[lc] =
                    msg_text.clone() + &string_fill(50, b" ") + "@C" + &fmt(cast::<Ext, _>(c), b"000#");
                f.sort_keys[lc] = g.credit_rating_num[c];
                if f.line_count == 1045 {
                    corp_id = 1600;
                }
            }
            corp_id += 1;
        }
        if found == 0 {
            let mut m =
                s(__content.string(3755)) + &g.active_entity_name + __content.string(3756);
            if tt == 5 {
                m = g.active_entity_name.clone() + __content.string(3757);
            }
            show_quick_message(g, ui, &mut m);
            return;
        }
        let n = cast::<usize, _>(f.line_count - 6);
        if tt == 1 && sort_mode == 0 {
            msg_text = s(__content.string(3758))
                + "\r"
                + "\r"
                + __content.string(3759);
            f.answer = ui.msgbox(&msg_text, 8228, &s(__content.string(3760)));
            if f.answer == 6 {
                sort_mode = 1;
                sort_by_rating(f, n);
            } else {
                sort_mode = -1;
            }
        } else if tt == 1 && sort_mode == 1 {
            sort_by_rating(f, n);
        } else if sort_mode == 0 {
            sort_mode = -1;
        }
        if sort_mode == -1 {
            // ARRAY SORT listLines(7) FOR lineCount - 6
            pb_array_sort::<PbStr, ()>(
                &mut f.list_lines[7..7 + n],
                None::<&mut [()]>,
                false,
                &|a, b| compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less,
            );
        }
        f.line_count += 3;
        f.list_lines[cast::<usize, _>(f.line_count)] = s("@END");
        let last_line = f.line_count;
        for k in 1..=last_line {
            g.text_lines[k] = f.list_lines[cast::<usize, _>(k)].clone();
        }
        g.entity_pick_mode = s("GetCorp");
        g.text_box_title =
            s(__content.string(3761));
        if tt == 5 {
            g.text_box_title = s(__content.string(3762));
        }
        g.target_corp = 0;
        g.text_has_links = 1;
        if tt == 1 {
            g.text_box_mode = s("30");
        }
        show_text_report(g, ui, rng);
        if g.target_corp == 0 {
            return;
        }
        msg_text = PbStr::new();
        let ae = g.active_entity;
        let target = g.target_corp;
        if tt == 1 {
            let bank_id = cast::<i32, _>(g.lending_bank[target]);
            let Some(c) = f.blc(bank_id) else { return };
            *c -= 1;
            sell_loan_to_bank(g, ui, rng, ae, bank_id, target);
            if Ext::from_f64(0.0) < g.cash[g.target_corp] {
                msg_text = s(__content.string(3763))
                    + &g.ent_name[g.target_corp]
                    + __content.string(3764);
                rebalance(g, ui, rng);
            }
        } else if tt == 5 {
            let mut bank_id = 0;
            f.borrower_id = target;
            f.calc_price_factor(g, ui, rng);
            let b = f.borrower_id;
            f.amount = f.amount * g.bank_debt[b];
            for corp_id in 11..=85 {
                if g.entity_status[corp_id] == 1
                    && g.active_entity != corp_id
                    && f.amount <= g.cash[corp_id] + g.cash[b]
                    && 1 <= g.credit_rating_num[b]
                {
                    bank_id = corp_id;
                }
            }
            msg_text = s(__content.string(3765));
            if g.credit_rating_num[b] == 0 {
                msg_text = s(
                    __content.string(3766),
                );
            }
            if bank_id == 0 {
                show_quick_message(g, ui, &mut msg_text);
                return;
            }
            msg_text = PbStr::new();
            sell_loan_to_bank(g, ui, rng, bank_id, ae, g.target_corp);
            if Ext::from_f64(10.0) < g.cash[g.target_corp] {
                msg_text = s(", and ")
                    + &g.ent_symbol[g.target_corp]
                    + __content.string(3767)
                    + &g.ent_name[bank_id];
            }
            rebalance(g, ui, rng);
        }
        let ae = g.active_entity;
        calc_entity_financials(g, ui, rng, ae);
        let mut m = g.news_msg.clone() + &msg_text;
        show_quick_message(g, ui, &mut m);
        post_news(g, ui, 0, 0);
        g.turn_action_count = g.turn_action_count + 1;
        let cp = g.cur_player;
        if tt == 1 && 0 < g.player_loan_offers_avail[cp] {
            g.player_loan_offers_avail[cp] = g.player_loan_offers_avail[cp] - 1;
            if g.player_loan_offers_avail[cp] < 1 {
                return;
            }
        }
        if tt == 5 && g.player_loan_offers_avail[cp] < 5 {
            g.player_loan_offers_avail[cp] = g.player_loan_offers_avail[cp] + 1;
        }
        if 4 < g.turn_action_count && g.solo_human_mode == 0 {
            return;
        }
        if !f.ask_another(g, ui) || f.answer != 6 {
            return;
        }
    }
}

/// `ARRAY SORT sortKeys(7) FOR n, TAGARRAY listLines(), DESCEND` (flag 0x8400): best rating first.
fn sort_by_rating(f: &mut Frame, n: usize) {
    pb_array_sort(
        &mut f.sort_keys[7..7 + n],
        Some(&mut f.list_lines[7..7 + n]),
        true,
        &|a: &i32, b: &i32| a < b,
    );
}

/// Port of ShowLoanTradeTypeDialog (0x5b856f) with its control callbacks LoanTradeDlg_OptBuyBusiness
/// (0x5b91ae), LoanTradeDlg_OptBuyConsumer (0x5b91f6), LoanTradeDlg_OptBuyPrimeMtg (0x5b9242),
/// LoanTradeDlg_OptBuySubprime (0x5b928e), LoanTradeDlg_OptSellBusiness (0x5b92da),
/// LoanTradeDlg_OptSellConsumer (0x5b9326), LoanTradeDlg_OptSellPrimeMtg (0x5b9372),
/// LoanTradeDlg_OptSellSubprime (0x5b93be), LoanTradeDlg_BtnGo (0x5b940a) and
/// LoanTradeDlg_BtnCancel (0x5b9489).
/// The modal "Buy/Sell Loans" dialog with eight option buttons (buy business / consumer / prime mortgage
/// / subprime loans, sell the same) and GO / CANCEL, shown through [`Ui::choose`]. The choice comes back
/// in `TmpExt1` (1..8; 0 = cancelled). The dialog handle `hLoanTradeDlg` (`DIALOG NEW`) and the colour
/// procedure DlgProcLoanTradeColors (0x5b8a80) are not modelled.
pub fn show_loan_trade_type_dialog(g: &mut Globals, ui: &mut dyn Ui, _h_parent: i32) {
    let __content = crate::content::current();
    adjust_dialog_frame_size(g.h_loan_trade_dlg, 0x80c0_0880, 0);
    let title = s(__content.string(3768));
    let prompt = [s(__content.string(3769))];
    let options = [
        s(__content.string(3770)),
        s(__content.string(3771)),
        s(__content.string(3772)),
        s(__content.string(1671)),
        s(__content.string(3773)),
        s(__content.string(3774)),
        s(__content.string(3775)),
        s(__content.string(1673)),
    ];
    match ui.choose(&title, &prompt, &options) {
        Some(k) => {
            // option button k (LoanTradeDlg_Opt*) sets TmpExt1 = k + 1, then GO
            g.tmp_ext1 = cast::<Ext, _>(k + 1);
            loan_trade_dlg_btn_go(g);
        }
        None => g.tmp_ext1 = Ext::from_f64(0.0), // LoanTradeDlg_BtnCancel
    }
}

/// Port of LoanTradeDlg_BtnGo (0x5b940a): keeps the selected type (`TmpExt1` 0..8, else 0) and ends the
/// dialog.
fn loan_trade_dlg_btn_go(g: &mut Globals) {
    let mut choice = to_long(g.tmp_ext1);
    if choice < 0 || 8 < choice {
        choice = 0;
    }
    g.tmp_ext1 = cast::<Ext, _>(choice);
}
