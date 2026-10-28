//! Bank liquidity rescue: `BankLiquidityAndCapitalCheck` (0x5021e2) with its GOSUB
//! `BankLiquidity_FindBuyerBank` (0x504619), `BankSellSubprimeSecs` (0x501bad) and
//! `ConfirmSubprimeSaleDiscount` (0x4a2d43).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::corp_ai::distress::bank_bailout_injection;
use crate::logic::earnings::balance_cash_t_bills;
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::report::number_to_words;
use crate::logic::sim_entity::ai_corp::is_recent_offering;
use crate::logic::sim_entity::bonds::{issue_bonds, issue_stock};
use crate::logic::sim_entity::corp_ops::bank_allocate_assets;
use crate::logic::sim_entity::fin_inst::sell_corp_bonds;
use crate::logic::sim_entity::transactions::{forced_sell_bonds, forced_sell_stocks};

/// Port of ConfirmSubprimeSaleDiscount (0x4a2d43).
/// Discount (percent of face) at which [runtime resource wording] can be sold now: `-GdpGrowth`
/// rounded (at least 5 in a recession, "weak economy"); [runtime resource wording] +50 minus the seller's
/// `quality` (profit margin, clamped to -10..15), while concern is growing (`SubprimeCrisisState = -2`)
/// +15 minus half the quality; floored at 0. With `ask_user = 1` and a positive discount the player
/// confirms the loss in a YES/NO box; NO returns -1.
pub fn confirm_subprime_sale_discount(g: &mut Globals, ui: &mut dyn Ui, ask_user: i32, quality: i16) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x4a300e (ShowRuntimeError) — no error can occur here.
    let mut quality = quality;
    if quality < 16 {
        if quality < -10 {
            quality = -10;
        }
    } else {
        quality = 15;
    }
    let mut reason_text = PbStr::new();
    let mut discount_pct = to_long(-(cast::<Ext, _>(g.gdp_growth)));
    if (cast::<Ext, _>(g.gdp_growth)) < Ext::from_f64(0.0) {
        if discount_pct < 5 {
            discount_pct = 5;
        }
        reason_text = PbStr::from(__content.string(2986));
    }
    if g.subprime_crisis_state < 1 {
        if g.subprime_crisis_state == -2 {
            if reason_text.is_empty() {
                reason_text = PbStr::from(__content.string(2987));
            }
            discount_pct = to_long(cast::<Ext, _>(discount_pct + 15) - cast::<Ext, _>(quality) * sf(0.5f32));
        }
    } else {
        discount_pct = (discount_pct + 50) - cast::<i32, _>(quality);
        reason_text = PbStr::from(__content.string(2988));
    }
    if discount_pct < 0 {
        discount_pct = 0;
    }
    if 0 < discount_pct && ask_user == 1 {
        let msg_text = PbStr::from(__content.string(2989))
            + &reason_text
            + __content.string(2990)
            + __content.string(2991)
            + &number_to_words(discount_pct, 0, 1)
            + __content.string(2992)
            + __content.string(2993)
            + "\r"
            + "\r"
            + __content.string(2994);
        let answer = ui.msgbox(&msg_text, 0x2123, &PbStr::from(__content.string(2995)));
        if answer != 6 {
            discount_pct = -1;
        }
    }
    discount_pct
}

/// Port of BankSellSubprimeSecs (0x501bad).
/// Bank or insurer `bank_id` (11..130, active) sells a quarter of it [runtime resource wording]
/// the market discount ([`confirm_subprime_sale_discount`] without asking). With a discount over 10% a
/// random other active financial institution (11..130) not managed by a player and with cash + T-bills
/// above 1.5x the proceeds buys them (the discount goes to its bad-debt reserve for banks, else to its
/// P&L); otherwise they are liquidated. A bank books the loss against its bad-debt reserve (a negative
/// reserve is charged to P&L and reset to 250), an insurer against its P&L. Posts the headline.
pub fn bank_sell_subprime_secs(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, bank_id: i32) {
    let __content = crate::content::current();
    let seller_id = bank_id;
    if seller_id < 11 || 130 < seller_id || g.entity_status[seller_id] != 1 {
        return;
    }
    let profit_margin = g.profit_margin[seller_id];
    let discount_pct = confirm_subprime_sale_discount(g, ui, 0, profit_margin);
    let face_sold = to_quad(cast::<Ext, _>(g.subprime_secs[seller_id]) * sf(0.25f32));
    let proceeds = if discount_pct < 1 {
        face_sold
    } else {
        to_quad(cast::<Ext, _>(face_sold) - cast::<Ext, _>(face_sold) * (cast::<Ext, _>(discount_pct) * sf(0.01f32)))
    };
    g.cash[seller_id] = g.cash[seller_id] + cast::<Ext, _>(proceeds);
    g.subprime_secs[seller_id] = to_quad(cast::<Ext, _>(g.subprime_secs[seller_id]) - cast::<Ext, _>(face_sold));
    let loss = to_quad(cast::<Ext, _>(face_sold) - cast::<Ext, _>(proceeds));
    let buyer_id = rng.rnd_range(130, 11);
    let mut bought = false;
    if seller_id != buyer_id
        && g.entity_status[buyer_id] == 1
        && 10 < discount_pct
        && cast::<Ext, _>(proceeds) * sf(1.5f32) < g.cash[buyer_id] + g.t_bills[buyer_id]
        && get_managing_player(g, buyer_id) == 0
    {
        g.cash[buyer_id] = g.cash[buyer_id] - cast::<Ext, _>(proceeds);
        g.subprime_secs[buyer_id] = to_quad(cast::<Ext, _>(g.subprime_secs[buyer_id]) + cast::<Ext, _>(face_sold));
        if g.industry_num[buyer_id] == 1 {
            g.bad_debt_reserve[buyer_id] = g.bad_debt_reserve[buyer_id] + cast::<Ext, _>(loss);
        } else {
            g.accrued_pl[buyer_id] = g.accrued_pl[buyer_id] + cast::<Ext, _>(loss);
        }
        g.news_msg = g.ent_symbol[buyer_id].clone()
            + __content.string(2996)
            + &g.ent_symbol[seller_id]
            + " at";
        bought = true;
    }
    if !bought {
        g.news_msg = g.ent_symbol[seller_id].clone() + __content.string(2997);
    }
    if discount_pct < 1 {
        g.news_msg = g.news_msg.clone() + __content.string(2998);
    } else {
        g.news_msg = g.news_msg.clone() + &str_s(cast::<Ext, _>(discount_pct)) + __content.string(2999);
        if 40 < discount_pct {
            g.news_msg = replace(g.news_msg.as_bytes(), b" at", __content.bytes(3000));
        }
    }
    if seller_id < 86 {
        g.bad_debt_reserve[seller_id] = g.bad_debt_reserve[seller_id] - cast::<Ext, _>(loss);
        if g.bad_debt_reserve[seller_id] < Ext::from_f64(0.0) {
            g.accrued_pl[seller_id] = g.accrued_pl[seller_id] + g.bad_debt_reserve[seller_id];
            g.accrued_pl[seller_id] = g.accrued_pl[seller_id] - Ext::from_f64(250.0);
            g.bad_debt_reserve[seller_id] = Ext::from_f64(250.0);
        }
    } else if 0 < discount_pct {
        g.accrued_pl[seller_id] = g.accrued_pl[seller_id] - cast::<Ext, _>(loss);
    }
    post_news(g, ui, seller_id, buyer_id);
}

/// The locals of `BankLiquidityAndCapitalCheck` that its GOSUB `BankLiquidity_FindBuyerBank` shares.
#[derive(Default)]
struct Frame {
    bank_id: i32,
    controller_id: i32, // -0xd8
    buyer_bank_id: i32, // -0xdc
    tries: i32,         // -0xc4
    debtor_id: i32,     // -0xd0
    amount: Ext,        // -0x90
    debtor_cash: Ext,   // -0xb4
}

/// Port of BankLiquidity_FindBuyerBank (0x504619), GOSUB of `BankLiquidityAndCapitalCheck`.
/// Finds a bank to take over a loan package of `amount`: up to ~100 random banks 11..85 (other than
/// the seller, active, controlled by a corp, with cash + T-bills + `debtorCash` >= 90% of the amount;
/// a bank with a lower id than the seller also needs cash + T-bills above the amount), then the first
/// bank 11..85 in order that is not controlled or shares the seller's top controller (a player's bank
/// does not buy a loan of a corp rated 7+ with more than 3000 of debt) with enough funds. The buyer pays
/// `amount` in cash. `buyerBankId` = 0 if none.
fn find_buyer_bank(g: &mut Globals, rng: &mut Rng, f: &mut Frame) {
    f.buyer_bank_id = 0;
    f.tries = 0;
    while f.buyer_bank_id == 0 {
        f.tries += 1;
        f.controller_id = rng.rnd_range(85, 11);
        let c = f.controller_id;
        if f.bank_id == c {
            continue;
        }
        if g.entity_status[c] == 1
            && 10 < g.corp_controller[c]
            && !(f.amount * sf(0.9f32) > g.t_bills[c] + g.cash[c] + f.debtor_cash)
            && (f.bank_id < c || !(f.amount >= g.t_bills[c] + g.cash[c]))
        {
            f.buyer_bank_id = c;
            g.cash[c] = g.cash[c] - f.amount;
            break;
        }
        if 100 < f.tries && f.buyer_bank_id == 0 {
            f.buyer_bank_id = -1;
        }
    }
    if f.buyer_bank_id == -1 {
        f.tries = 11;
        while f.tries <= 85 {
            let t = f.tries;
            let mut take = false;
            if g.entity_status[t] == 1 && f.bank_id != t {
                let mut ok = true;
                if 0 < get_controller_type(g, t) {
                    // CorpController(0) for an uncontrolled seller reads the dummy element (0); the
                    // original raises no runtime error here (checked under emulation).
                    let top = g.corp_controller[f.bank_id];
                    ok = g.corp_controller[top] == g.corp_controller[t];
                }
                if ok {
                    let mut funds_check = true;
                    if get_controller_type(g, t) == 1 && 85 < f.debtor_id {
                        let d = f.debtor_id;
                        funds_check = !(g.credit_rating_num[d] < 7) && !(g.bank_debt[d] > g.exchange_rate * Ext::from_f64(3000.0));
                    }
                    if funds_check && !(f.amount >= g.t_bills[t] + g.cash[t] + f.debtor_cash) {
                        take = true;
                    }
                }
            }
            if take {
                f.buyer_bank_id = t;
                g.cash[t] = g.cash[t] - f.amount;
                break;
            }
            f.tries += 1;
        }
    }
    if f.buyer_bank_id == -1 {
        f.buyer_bank_id = 0;
    }
}

/// Port of BankLiquidity_ExitSub (0x504a7d): the common exit label of `BankLiquidityAndCapitalCheck`
/// (frame release only).
fn bank_liquidity_exit_sub() {}

/// Port of BankLiquidityAndCapitalCheck (0x5021e2).
/// Rescues bank `bank_id` (11..85, active) from negative cash, stopping as soon as the cash is no
/// longer negative: credit line capped at 2000 (the excess repaid to its lender); cash/T-bills
/// balanced; stock holdings dumped when subsidiaries are worth over half the net worth (unless player-
/// controlled with loans); up to 100 rounds of asset re-allocation; a 5% deposit inflow for a
/// well-capitalised bank; sales of 10% of consumer loans and 25% of mortgages to other banks
/// ([`find_buyer_bank`]); subprime securities sales; regulator-urged bond issues (corp- or computer-
/// controlled banks without bonds) and stock offerings (corp-controlled); calling in up to 10% of
/// business loans; bond sales; selling business loans of rated corps to other banks at a discount (up
/// to 50 rounds, then two bailout injections); a final forced sale of stock holdings (headline left in
/// `NewsMsg`, not posted). Still negative: `CreditFrozen = 1`.
pub fn bank_liquidity_and_capital_check(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, bank_id: i32) {
    // ON ERROR GOTO 0x504a30: ShowRuntimeError(…) and exit — no error path is reachable.
    run(g, ui, rng, bank_id);
    bank_liquidity_exit_sub();
}

fn run(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, bank_id: i32) {
    let __content = crate::content::current();
    let b = bank_id;
    if b < 11 || 85 < b || g.entity_status[b] != 1 {
        return;
    }
    let mut f = Frame { bank_id: b, ..Frame::default() };
    let mut msg_prefix: PbStr;
    let lender_id = cast::<i32, _>(g.lending_bank[b]);
    if Ext::from_f64(2000.0) < g.bank_debt[b] {
        f.amount = g.bank_debt[b] - Ext::from_f64(2000.0);
        g.cash[b] = g.cash[b] - f.amount;
        g.bank_debt[b] = g.bank_debt[b] - f.amount;
        if 10 < lender_id {
            g.cash[lender_id] = g.cash[lender_id] + f.amount;
        }
    }
    balance_cash_t_bills(g, b);
    if Ext::from_f64(0.0) <= g.cash[b] {
        return;
    }
    calc_entity_financials(g, ui, rng, b);
    let cust_deposits = g.bank_cust_deposits[b];
    if g.net_worth * sf(0.5f32) < g.tmp_subsid_stock_value {
        f.controller_id = g.corp_controller[b];
        if get_controller_type(g, b) == 2 {
            f.controller_id = 0;
        }
        if f.controller_id == 0 || 10 < f.controller_id || g.tmp_bank_total_loans < g.total_assets * sf(0.1f32) {
            f.controller_id = 0;
            let _ = forced_sell_stocks(g, ui, rng, b, f.controller_id);
            if Ext::from_f64(0.0) <= g.cash[b] {
                return;
            }
            calc_entity_financials(g, ui, rng, b);
        }
    }
    let mut iter = 100;
    loop {
        if g.bank_alloc_off[b] != 0 {
            break;
        }
        let prev_cash = to_quad(g.cash[b]);
        bank_allocate_assets(g, ui, rng, b, 3, 1);
        if cast::<Ext, _>(prev_cash) == g.cash[b] {
            break;
        }
        if Ext::from_f64(0.0) <= g.cash[b] {
            return;
        }
        iter -= 1;
        if iter == 0 {
            break;
        }
    }
    let mut ratio = Ext::from_f64(0.0);
    if Ext::from_f64(0.0) < g.bank_c_ds[b] + g.bank_demand_deposits[b] {
        ratio = (g.bad_debt_reserve[b] + g.net_worth) / (g.bank_demand_deposits[b] + g.bank_c_ds[b] + cust_deposits);
    }
    if sf(0.07f32) < ratio {
        let inflow = (g.bank_c_ds[b] + g.bank_demand_deposits[b]) * sf(0.05f32);
        g.cash[b] = g.cash[b] + inflow;
        g.bank_c_ds[b] = g.bank_c_ds[b] * sf(1.05f32);
        g.bank_demand_deposits[b] = g.bank_demand_deposits[b] * sf(1.05f32);
        if Ext::from_f64(0.0) <= g.cash[b] {
            return;
        }
    }
    msg_prefix = g.ent_symbol[b].clone() + __content.string(3001);
    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
        msg_prefix = g.ent_symbol[b].clone() + __content.string(3002);
    }
    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(5.0) {
        msg_prefix = g.ent_symbol[b].clone();
    }

    // consumer loans
    if 2000 < g.bank_consumer_loans[b] || g.total_assets * sf(0.15f32) < cast::<Ext, _>(g.bank_consumer_loans[b]) {
        f.amount = cast::<Ext, _>(g.bank_consumer_loans[b]) * sf(0.1f32);
        if g.cash[b].abs() < f.amount {
            f.amount = g.cash[b].abs();
        }
        g.bank_consumer_loans[b] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[b]) - f.amount);
        g.cash[b] = g.cash[b] + f.amount;
        find_buyer_bank(g, rng, &mut f);
        let buyer = f.buyer_bank_id;
        if 10 < buyer && buyer < 86 {
            if rng.rnd() * Ext::from_f64(8.0) < Ext::from_f64(1.0) {
                g.news_msg = msg_prefix.clone() + __content.string(3003) + &g.ent_symbol[buyer];
            }
            g.bank_consumer_loans[buyer] = to_quad(cast::<Ext, _>(g.bank_consumer_loans[buyer]) + f.amount);
        }
        if Ext::from_f64(0.0) <= g.cash[b] {
            return;
        }
    }
    // mortgage loans
    if 1500 < g.bank_mortgage_loans[b] || g.total_assets * sf(0.15f32) < cast::<Ext, _>(g.bank_mortgage_loans[b]) {
        f.amount = cast::<Ext, _>(g.bank_mortgage_loans[b]) * sf(0.25f32);
        if g.cash[b].abs() < f.amount {
            f.amount = g.cash[b].abs();
        }
        find_buyer_bank(g, rng, &mut f);
        g.bank_mortgage_loans[b] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[b]) - f.amount);
        g.cash[b] = g.cash[b] + f.amount;
        let buyer = f.buyer_bank_id;
        if 10 < buyer && buyer < 86 {
            if Ext::from_f64(7.0) < rng.rnd() * Ext::from_f64(8.0) {
                g.news_msg = msg_prefix.clone() + __content.string(3004) + &g.ent_symbol[buyer];
            }
            g.bank_mortgage_loans[buyer] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[buyer]) + f.amount);
        }
        if Ext::from_f64(0.0) <= g.cash[b] {
            return;
        }
    }
    if !g.news_msg.is_empty() {
        post_news(g, ui, b, 0);
    }
    if 1500 < g.subprime_secs[b] || g.total_assets * sf(0.15f32) < cast::<Ext, _>(g.subprime_secs[b]) {
        bank_sell_subprime_secs(g, ui, rng, b);
        if Ext::from_f64(0.0) <= g.cash[b] {
            return;
        }
    }

    // regulators: [runtime resource wording]
    let mut action_flag = 0;
    if 10 < g.corp_controller[b] {
        action_flag = 1;
    }
    if get_controller_type(g, b) == 2 {
        action_flag = 1;
    }
    if g.bonds_issued[b] == 0 && action_flag == 1 {
        calc_entity_financials(g, ui, rng, b);
        if 1 < g.credit_rating_num[b] && Ext::from_f64(0.0) < g.net_worth {
            f.amount = g.net_worth * sf(0.5f32);
            if g.cash[b].abs() < f.amount * sf(0.5f32) {
                f.amount = g.cash[b].abs() * Ext::from_f64(2.0);
            }
            if g.exchange_rate * Ext::from_f64(500.0) < f.amount && (cast::<Ext, _>(g.borrow_rate)) < sf(12.5f32) {
                g.tmp_ext1 = f.amount;
                g.news_msg = PbStr::from(__content.string(3005))
                    + &g.ent_symbol[b]
                    + __content.string(3006);
                if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
                    g.news_msg = g.ent_symbol[b].clone()
                        + __content.string(3007);
                }
                post_news(g, ui, b, 0);
                issue_bonds(g, ui, rng, b, Ext::from_f64(0.0), 0);
                if Ext::from_f64(0.0) <= g.cash[b] {
                    return;
                }
            }
        }
    }

    // regulators: stock offering
    f.controller_id = is_recent_offering(g, b);
    f.amount = g.cash[b].abs();
    if 10 < g.corp_controller[b]
        && f.controller_id != 1
        && 1 < g.credit_rating_num[b]
        && g.credit_rating_num[b] < 7
        && g.exchange_rate * Ext::from_f64(200.0) < f.amount
    {
        let mut pct = 24;
        ratio = g.market_cap[b] * cast::<Ext, _>(pct);
        if f.amount * Ext::from_f64(2.0) < ratio {
            pct = 12;
        }
        ratio = ratio * sf(0.5f32);
        if f.amount * Ext::from_f64(2.0) < ratio {
            pct = 6;
        }
        g.news_msg = PbStr::from(__content.string(3008)) + &g.ent_symbol[b] + __content.string(3009);
        post_news(g, ui, 0, 0);
        g.tmp_ext1 = cast::<Ext, _>(pct);
        issue_stock(g, ui, rng, b, 1);
        f.buyer_bank_id = rng.rnd_range(10, 1);
        if g.recent_offerings[f.buyer_bank_id] == 0 {
            g.recent_offerings[f.buyer_bank_id] = b;
        }
    }
    if Ext::from_f64(0.0) <= g.cash[b] {
        return;
    }

    // call in business loans ("Called from Part 10")
    calc_entity_financials(g, ui, rng, b);
    f.amount = g.cash[b].abs();
    ratio = Ext::from_f64(0.0);
    if Ext::from_f64(0.0) < g.bank_loans_total {
        ratio = f.amount / g.bank_loans_total;
    }
    let mut pct = to_long(ratio * Ext::from_f64(100.0));
    if 10 < pct {
        pct = 10;
        ratio = sf(0.1f32);
    }
    if 2 < pct && pct < 11 {
        action_flag = 0;
        for i in 1..=1600 {
            if !(i <= g.num_players || 85 < i) {
                continue;
            }
            if g.entity_status[i] != 1 || !(1 < g.credit_rating_num[i]) {
                continue;
            }
            if cast::<i32, _>(g.lending_bank[i]) != b || !(g.exchange_rate * Ext::from_f64(10.0) <= g.bank_debt[i]) {
                continue;
            }
            f.amount = ratio * g.bank_debt[i];
            if g.bank_largest_debtor[b] == i {
                f.amount = f.amount * Ext::from_f64(2.0);
            } else if g.cash[i] < f.amount * sf(0.2f32) && 6 < g.credit_rating_num[i] {
                continue;
            }
            g.bank_debt[i] = g.bank_debt[i] - f.amount;
            g.cash[i] = g.cash[i] - f.amount;
            g.cash[b] = g.cash[b] + f.amount;
            action_flag = 1;
        }
        if action_flag == 1 {
            g.news_msg = g.ent_symbol[b].clone()
                + __content.string(3010)
                + &str_s(cast::<Ext, _>(pct))
                + __content.string(3011);
            post_news(g, ui, b, 0);
        }
    }
    if Ext::from_f64(0.0) <= g.cash[b] {
        return;
    }

    // bond sales
    calc_entity_financials(g, ui, rng, b);
    if Ext::from_f64(0.0) < g.tmp_corp_bonds_value && g.tmp_gov_bonds_value == Ext::from_f64(0.0) {
        if g.tmp_bank_total_loans < g.total_assets * sf(0.1f32) {
            f.amount = forced_sell_bonds(g, ui, rng, b, 2, Ext::from_f64(0.0), Ext::from_f64(0.0));
        } else if 0 < g.hold_bond_id[(0, b)] {
            f.buyer_bank_id = g.hold_bond_id[(1, b)];
            if 0 < f.buyer_bank_id {
                g.tmp_ext2 = Ext::from_f64(2.0);
                g.tmp_ext1 = cast::<Ext, _>(g.hold_bond_face[(1, b)]) * sf(0.5f32);
                if g.tmp_ext1 < Ext::from_f64(100.0) {
                    g.tmp_ext1 = cast::<Ext, _>(g.hold_bond_face[(1, b)]);
                }
                sell_corp_bonds(g, ui, rng, b, f.buyer_bank_id);
            }
        }
    } else if Ext::from_f64(0.0) < g.tmp_gov_bonds_value && g.tmp_bank_total_loans < g.total_assets * sf(0.2f32) {
        f.amount = forced_sell_bonds(g, ui, rng, b, 1, Ext::from_f64(0.0), Ext::from_f64(0.0));
    }
    if Ext::from_f64(0.0) <= g.cash[b] {
        return;
    }

    // sell business loans to other banks ("Business")
    let mut iter = 0;
    while iter < 50 {
        if Ext::from_f64(0.0) <= g.cash[b] {
            break;
        }
        ratio = g.cash[b].abs() * sf(0.3f32);
        iter += 1;
        f.controller_id = rng.rnd_range(1600, 86);
        if iter == 15 {
            f.controller_id = 86;
        }
        f.debtor_cash = Ext::from_f64(0.0);
        f.debtor_id = f.controller_id;
        while f.debtor_id < 1601 {
            let d = f.debtor_id;
            if g.entity_status[d] == 1
                && cast::<i32, _>(g.lending_bank[d]) == b
                && !(g.bank_debt[d] < Ext::from_f64(1.0))
                && !(ratio * Ext::from_f64(2.0) < g.bank_debt[d])
            {
                calc_entity_financials(g, ui, rng, d);
                if g.credit_rating_num[d] != 0 {
                    f.amount = g.bank_debt[d];
                    let haircut_rate = cast::<Ext, _>(6 - g.credit_rating_num[d]) * sf(0.01f32);
                    ratio = haircut_rate * f.amount;
                    msg_prefix = str_s(cast::<Ext, _>(g.credit_rating_num[d] + 94));
                    if g.credit_rating_num[d] < 2 {
                        ratio = f.amount * sf(0.3f32);
                        msg_prefix = PbStr::from(" 70");
                    }
                    f.amount = f.amount - ratio;
                    f.debtor_cash = g.cash[d];
                    find_buyer_bank(g, rng, &mut f);
                    let buyer = f.buyer_bank_id;
                    if buyer != 0 {
                        g.cash[b] = g.cash[b] + f.amount;
                        if ratio != Ext::from_f64(0.0) {
                            if ratio <= Ext::from_f64(0.0) {
                                g.accrued_pl[b] = g.accrued_pl[b] + ratio.abs();
                                g.bad_debt_reserve[buyer] = g.bad_debt_reserve[buyer] - ratio.abs();
                            } else {
                                g.bad_debt_reserve[b] = g.bad_debt_reserve[b] - ratio;
                                g.accrued_pl[buyer] = g.accrued_pl[buyer] + ratio;
                            }
                            if g.bad_debt_reserve[b] < Ext::from_f64(0.0) {
                                g.accrued_pl[b] = g.accrued_pl[b] + g.bad_debt_reserve[b];
                                g.bad_debt_reserve[b] = g.exchange_rate * Ext::from_f64(5.0);
                                g.accrued_pl[b] = g.accrued_pl[b] - g.exchange_rate * Ext::from_f64(5.0);
                            }
                            if g.bad_debt_reserve[buyer] < Ext::from_f64(0.0) {
                                g.accrued_pl[buyer] = g.accrued_pl[buyer] + g.bad_debt_reserve[buyer];
                                g.bad_debt_reserve[buyer] = g.exchange_rate * Ext::from_f64(100.0);
                                g.accrued_pl[buyer] = g.accrued_pl[buyer] - g.exchange_rate * Ext::from_f64(100.0);
                            }
                        }
                        g.lending_bank[d] = cast::<i16, _>(buyer);
                        g.news_msg = g.ent_symbol[b].clone()
                            + __content.string(3012)
                            + &g.ent_symbol[d]
                            + __content.string(3013)
                            + &g.ent_symbol[buyer]
                            + " at"
                            + &msg_prefix
                            + __content.string(3014);
                        post_news(g, ui, 0, d);
                        if g.cash[d] != Ext::from_f64(0.0) {
                            calc_entity_financials(g, ui, rng, buyer);
                            calc_entity_financials(g, ui, rng, b);
                        }
                        if Ext::from_f64(0.0) <= g.cash[b] {
                            return;
                        }
                    }
                }
            }
            f.debtor_id += 1;
        }
    }
    if 49 < iter {
        bank_bailout_injection(g, ui, rng);
        bank_bailout_injection(g, ui, rng);
    }
    calc_entity_financials(g, ui, rng, b);
    if g.net_worth * sf(0.5f32) < g.tmp_subsid_stock_value
        && (f.controller_id == 0 || 10 < f.controller_id || g.tmp_bank_total_loans < g.total_assets * sf(0.1f32))
    {
        let _ = forced_sell_stocks(g, ui, rng, b, f.controller_id);
        g.news_msg = g.ent_symbol[b].clone() + __content.string(3015);
        if Ext::from_f64(0.0) <= g.cash[b] {
            return;
        }
        g.news_msg = replace(g.news_msg.as_bytes(), b"stock", __content.bytes(3016));
    }
    if g.cash[b] < Ext::from_f64(0.0) {
        g.credit_frozen[b] = 1;
    }
}
