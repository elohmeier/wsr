//! The player's corporate bond trades `TradeCorpBonds` (0x5b6dbe) and T-bill trades `TradeTBills`
//! (0x69af92).

// Keep the original's statement shapes.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use super::select::in_box_symbol;
use crate::logic::corp_finance::bonds::bond_buyback_menu;
use crate::logic::earnings::{balance_cash_t_bills, calc_accrued_bond_interest};
use crate::logic::finance::{calc_entity_financials, get_managing_player};
use crate::logic::holdings::find_bond_holding_slot;
use crate::logic::reports_lists::show_portfolio_listing;
use crate::logic::research::bank_regulator_lending_check;
use crate::logic::sim_entity::bonds::bond_buyback;
use crate::logic::sim_entity::fin_inst::{buy_corp_bonds, sell_corp_bonds};
use crate::pb::string::mcase;

/// Port of MsgNonFinancialNoBondTrading (0x5b6cfe): the " [runtime resource wording]
/// institutions …" message box.
pub fn msg_non_financial_no_bond_trading(g: &Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let msg_text = s(__content.string(11795))
        + __content.string(11796)
        + __content.string(11797)
        + __content.string(11798)
        + __content.string(11799);
    ui.msgbox(&msg_text, 8256, &g.app_title);
}

/// Locals of TradeCorpBonds used by its GOSUB TradeCorpBonds_CalcCost.
struct BondFrame {
    issuer_id: i32,
    buying_power: Ext,
    max_face: i64,
}

/// Port of TradeCorpBonds (0x5b6dbe) with its GOSUBs TradeCorpBonds_CalcCost (0x5b8309) and
/// TradeCorpBonds_Exit (0x5b84f6).
///
/// `trade_mode` 1 = a bank or insurer (the acting entity, else refused) trades corporate bonds,
/// `buy_sell` 1 = buy / 2 = sell; `trade_mode` 2 = the acting entity buys back its own bonds (a player:
/// any bonds, asked whether to buy or sell by its holdings). Buying needs liquid funds of 0.5 (the
/// buying power `LiquidFunds`), passes the bank regulators (`BankRegulatorLendingCheck`), and a "D"
/// rated issuer with bank debt may not buy back its bonds. The issuer is entered in the symbol dialog
/// (`InBox(3)`, `LookupFilter = "BONDS"`) or, to sell, picked from the bond portfolio
/// (`ShowPortfolioListing(ActiveEntity, 1, 0)`, then the dialog). For a buy the affordable face
/// (CalcCost: price with market impact, accrued interest, 1.6x/1x margin) and the public bonds are
/// shown; the face is asked again while over the public bonds; then `BondBuyback` (own bonds, forced)
/// or `BuyCorpBonds`. Without public bonds an issuer's own buyback goes to `BondBuybackMenu`. For a sale
/// the face (up to the holding) is asked and `SellCorpBonds` runs. A company the current player controls
/// becomes `PlayerRecentCorp`. `time_secs` is passed to `BondBuybackMenu`.
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn trade_corp_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, trade_mode: i32, buy_sell: i32, time_secs: i32) {
    let __content = crate::content::current();
    // DIM listBuf(...) (unused)
    let ae0 = g.active_entity;
    calc_entity_financials(g, ui, rng, ae0);
    let mut f = BondFrame { issuer_id: 0, buying_power: g.liquid_funds, max_face: 0 };
    let mut is_sell = 0;
    let mut prompt_text;
    let mut aux_text = PbStr::new();
    let mut answer;
    if trade_mode == 1 && 130 < g.active_entity {
        msg_non_financial_no_bond_trading(g, ui);
        return;
    }
    if buy_sell == 1 && f.buying_power < Ext::from_f64(0.5) {
        let m = g.ent_name[g.active_entity].clone()
            + __content.string(11800)
            + __content.string(11801);
        ui.msgbox(&m, 8208, &s(__content.string(11802)));
        return;
    }
    if trade_mode == 1 && buy_sell == 1 {
        let ae = g.active_entity;
        if bank_regulator_lending_check(g, ui, ae, 0) == 1 {
            let m = s(__content.string(11803))
                + &g.active_entity_name
                + __content.string(11804)
                + __content.string(11805)
                + __content.string(11806)
                + __content.string(11807);
            ui.msgbox(&m, 8208, &s(__content.string(11808)));
            return;
        }
    }
    let mut subject_text = s("do you");
    if 10 < g.active_entity {
        subject_text = s("does ") + &g.ent_symbol[g.active_entity];
    }
    if trade_mode == 2 {
        f.issuer_id = g.active_entity;
    }
    if f.issuer_id == g.active_entity
        && g.credit_rating_str == "D"
        && Ext::from_f64(0.0) < g.bank_debt[g.active_entity]
        && 1 < g.industry_num[g.active_entity]
    {
        prompt_text = s(__content.string(11809)) + __content.string(11810);
        ui.msgbox(&prompt_text, 8208, &s(__content.string(11811)));
        return;
    }
    if f.issuer_id != g.active_entity {
        let ae = g.active_entity;
        for slot in 1..=15 {
            if 0 < g.hold_bond_id[(slot, ae)] && 0 < g.hold_bond_face[(slot, ae)] {
                is_sell = 1;
            }
        }
        if trade_mode == 1 && buy_sell == 2 && is_sell == 0 {
            let m = g.active_entity_name.clone() + __content.string(11812);
            ui.msgbox(&m, 8208, &s(__content.string(11813)));
            return;
        }
        if trade_mode == 1 && buy_sell == 1 {
            is_sell = 0;
        }
        if trade_mode == 1 && buy_sell == 2 {
            is_sell = 1;
        }
        g.tmp_symbol = PbStr::new();
        g.action_code = s("BUYSELLJUNK");
        if is_sell == 0 {
            aux_text = s("buy");
            if 10 < ae && 0 < g.bonds_issued[ae] {
                aux_text = aux_text + __content.string(11814);
            }
        } else {
            aux_text = s("sell");
        }
        prompt_text = s(__content.string(11815));
        prompt_text =
            prompt_text + __content.string(11816) + &g.active_entity_name + __content.string(2915) + &aux_text + ":" + "\r\n" + "\r\n";
        g.input_dlg_label = PbStr::new();
        g.input_dlg_prompt = prompt_text;
    }
    if is_sell == 0 {
        if f.issuer_id != g.active_entity {
            g.lookup_filter = s("BONDS");
            g.input_dlg_title = s(__content.string(11817)) + &g.active_entity_name;
            in_box_symbol(g, ui, rng);
            g.lookup_filter = PbStr::new();
            g.action_code = PbStr::new();
            if g.tmp_symbol.is_empty() || g.target_corp < 11 {
                return;
            }
            f.issuer_id = g.target_corp;
            g.target_corp = 0;
        }
        if trade_mode == 1 && buy_sell == 1 && find_bond_holding_slot(g, g.active_entity, f.issuer_id) == 0 {
            prompt_text = s("You are");
            if 10 < g.active_entity {
                prompt_text = g.active_entity_name.clone() + " is";
            }
            let m = s(__content.string(11818))
                + &prompt_text
                + __content.string(11819);
            let title = g.app_title.clone();
            ui.msgbox(&m, 8208, &title);
            return;
        }
        let issuer = f.issuer_id;
        calc_entity_financials(g, ui, rng, issuer);
        if g.bonds_issued[issuer] == 0 || g.bonds_public_held <= 0 {
            if 0 < g.bonds_issued[issuer] && issuer == g.active_entity {
                bond_buyback_menu(g, ui, rng, 0, time_secs);
                return;
            }
            prompt_text = s(__content.string(2167))
                + &g.ent_name[issuer]
                + __content.string(11820)
                + __content.string(11821);
            ui.msgbox(&prompt_text, 8208, &s(__content.string(11822)));
            return;
        }
        let public_bonds: i64 = g.bonds_public_held;
        f.max_face = public_bonds;
        trade_corp_bonds_calc_cost(g, &mut f);
        if f.max_face < public_bonds && 0 < f.max_face {
            if f.max_face <= 500 {
                f.max_face = to_quad(cast::<Ext, _>(f.max_face) - Ext::from_f64(1.0));
            } else {
                f.max_face = to_quad(cast::<Ext, _>(f.max_face) - Ext::from_f64(2.0));
            }
        }
        loop {
            let mut coupon_text = right(g.bond_issue_info[issuer].as_bytes(), 5);
            if val(coupon_text.as_bytes()) < Ext::from_f64(10.0) {
                coupon_text = right(coupon_text.as_bytes(), 4);
            }
            coupon_text = coupon_text + "%";
            aux_text = g.credit_rating_str.clone() + "-rated ";
            answer = PbStr::new();
            if issuer == g.active_entity {
                answer = s(" back");
            }
            prompt_text = g.ent_name[issuer].clone()
                + " has "
                + &trim(g.currency_symbol.as_bytes())
                + &fmt_trim(cast::<Ext, _>(public_bonds), "* ###,")
                + " "
                + &g.million_prefix_lc
                + __content.string(9420)
                + __content.string(11823)
                + &aux_text
                + &coupon_text
                + __content.string(11824)
                + &mid(g.bond_issue_info[issuer].as_bytes(), 1, 4)
                + __content.string(11825)
                + &fmt_trim(g.bond_price[issuer].get(), "* ###.00")
                + __content.string(11826)
                + &g.million_prefix_lc
                + "illion "
                + __content.string(11827)
                + &subject_text
                + __content.string(11828)
                + &answer
                + "?  ";
            if f.max_face < public_bonds {
                prompt_text = prompt_text
                    + __content.string(11829)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(cast::<Ext, _>(f.max_face), "* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + __content.string(11830);
            } else if (cast::<Ext, _>(public_bonds)) < g.exchange_rate * Ext::from_f64(500.0) {
                prompt_text = prompt_text + __content.string(11831);
            }
            let mut default_amt = str_s(cast::<Ext, _>(public_bonds));
            if f.max_face < public_bonds {
                default_amt = str_s(cast::<Ext, _>(f.max_face));
            }
            let title = g.ent_symbol[g.active_entity].clone() + __content.string(11832);
            answer = ui.inputbox(&prompt_text, &title, &trim(default_amt.as_bytes()));
            g.tmp_ext1 = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
            if g.tmp_ext1 <= Ext::from_f64(0.0) {
                g.tmp_ext1 = Ext::from_f64(0.0);
                return;
            }
            if (cast::<Ext, _>(public_bonds)) < g.tmp_ext1 {
                prompt_text = s(__content.string(11833))
                    + &g.ent_symbol[issuer]
                    + __content.string(4149)
                    + &g.currency_symbol
                    + &str_s(cast::<Ext, _>(public_bonds))
                    + " "
                    + &g.million_prefix_uc
                    + __content.string(11834);
                show_quick_message(g, ui, &mut prompt_text);
            }
            if !((cast::<Ext, _>(public_bonds)) < g.tmp_ext1) {
                break;
            }
        }
        g.tmp_ext2 = Ext::from_f64(0.0);
        if g.cur_player == g.corp_controller[issuer] {
            g.player_recent_corp = cast::<i16, _>(issuer);
        }
        if g.active_entity == issuer {
            g.tmp_ext3 = -Ext::from_f64(1.0);
            let ae = g.active_entity;
            bond_buyback(g, ui, rng, ae);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
        } else {
            let ae = g.active_entity;
            buy_corp_bonds(g, ui, rng, ae, issuer);
        }
    } else if is_sell == 1 {
        f.issuer_id = 0;
        g.tmp_symbol = PbStr::new();
        while f.issuer_id == 0 {
            if is_sell == 1 {
                g.tmp_ext1 = -Ext::from_f64(2.0);
                g.entity_pick_mode = s("GetCorp");
                g.target_corp = 0;
                let ae = g.active_entity;
                show_portfolio_listing(g, ui, rng, ae, 1, 0);
                if g.target_corp < 11 {
                    g.target_corp = 0;
                } else {
                    g.tmp_symbol = g.ent_symbol[g.target_corp].clone();
                }
            }
            if g.target_corp == 0 {
                g.lookup_filter = s("BONDS");
                g.input_dlg_title = s(__content.string(11817)) + &g.active_entity_name;
                in_box_symbol(g, ui, rng);
                g.lookup_filter = PbStr::new();
            }
            g.action_code = PbStr::new();
            if g.tmp_symbol.is_empty() {
                return;
            }
            f.issuer_id = g.target_corp;
            g.target_corp = 0;
            is_sell = 0;
            let ae = g.active_entity;
            for slot in 1..=15 {
                if f.issuer_id == g.hold_bond_id[(slot, ae)] {
                    is_sell = 1;
                    f.max_face = g.hold_bond_face[(slot, ae)];
                }
            }
            if is_sell == 0 {
                prompt_text = s("ERROR...")
                    + &g.active_entity_name
                    + __content.string(11835)
                    + __content.string(11836)
                    + &g.ent_name[f.issuer_id]
                    + "!";
                show_quick_message(g, ui, &mut prompt_text);
                f.issuer_id = 0;
            }
        }
        let issuer = f.issuer_id;
        calc_entity_financials(g, ui, rng, issuer);
        let price = g.bond_price[issuer];
        prompt_text = s(__content.string(11837))
            + &g.ent_name[issuer]
            + __content.string(11838)
            + &fmt_trim(price.get(), "* ###.00")
            + __content.string(11839)
            + &g.currency_symbol
            + &g.million_prefix_lc
            + "illion "
            + __content.string(11840)
            + &subject_text
            + __content.string(11841)
            + "owned:"
            + &str_s(cast::<Ext, _>(f.max_face))
            + " "
            + &g.million_prefix_lc
            + __content.string(11842);
        if (cast::<Ext, _>(g.bonds_public_held)) < g.exchange_rate * Ext::from_f64(500.0) {
            prompt_text = prompt_text + __content.string(11843);
        }
        loop {
            let title = g.ent_symbol[g.active_entity].clone() + __content.string(11844);
            answer = ui.inputbox(&prompt_text, &title, &fmt_trim(cast::<Ext, _>(f.max_face), "* ###,"));
            g.tmp_ext1 = val(remove_any(answer.as_bytes(), b"$ ,%").as_bytes());
            if g.tmp_ext1 <= Ext::from_f64(0.0) {
                return;
            }
            if (cast::<Ext, _>(f.max_face)) < g.tmp_ext1 {
                aux_text = s(__content.string(11833))
                    + &ucase(g.active_entity_name.as_bytes(), CaseMode::Ansi)
                    + __content.string(4171)
                    + &g.currency_symbol
                    + &str_s(cast::<Ext, _>(f.max_face))
                    + " "
                    + &g.million_prefix_uc
                    + __content.string(11845)
                    + __content.string(11846)
                    + &g.ent_name[issuer]
                    + ".";
                show_quick_message(g, ui, &mut aux_text);
            }
            if !((cast::<Ext, _>(f.max_face)) < g.tmp_ext1) {
                break;
            }
        }
        g.tmp_ext2 = Ext::from_f64(0.0);
        let ae = g.active_entity;
        sell_corp_bonds(g, ui, rng, ae, issuer);
        if g.cur_player == g.corp_controller[issuer] {
            g.player_recent_corp = cast::<i16, _>(issuer);
        }
    }
}

/// Port of TradeCorpBonds_CalcCost (0x5b8309), GOSUB of TradeCorpBonds: limits `max_face` to what the
/// buying power pays at the bond price, then lowers it (by a third while the cost exceeds 1.6x the
/// funds left after the accrued interest, else by 1) until the cost — price x face, for another issuer
/// with the market impact `0.03 * face / (issued + public) * 0.25 * DifficultyLevel` and 0.5%, for the
/// own bonds price + 1 point — fits. Without a lower bound this loops forever when the buying power is
/// below the accrued interest (BUGS.md #253; reproduced).
fn trade_corp_bonds_calc_cost(g: &Globals, f: &mut BondFrame) {
    let price = g.bond_price[f.issuer_id].get();
    let affordable = to_quad(f.buying_power / (sf(0.01f32) * price));
    if affordable < f.max_face {
        f.max_face = affordable;
    }
    loop {
        let mut cost: Ext;
        if g.active_entity != f.issuer_id {
            // both stored as DOUBLE
            let impact: Ext =
                cast::<Ext, _>(f.max_face) / (cast::<Ext, _>(g.bonds_issued[f.issuer_id]) + cast::<Ext, _>(g.bonds_public_held)) * sf(0.03f32);
            let diff_adj: Ext = cast::<Ext, _>(g.difficulty_level) * sf(0.25f32);
            cost = sf(0.01f32) * price * cast::<Ext, _>(f.max_face) * (impact * diff_adj + Ext::from_f64(1.0));
            cost = sf(1.005f32) * cost;
        } else {
            cost = sf(0.01f32) * (Ext::from_f64(1.0) + price) * cast::<Ext, _>(f.max_face);
        }
        let accrued = calc_accrued_bond_interest(g, f.max_face, f.issuer_id);
        if (f.buying_power - accrued) * sf(1.6f32) < cost {
            f.max_face = to_quad(sf(0.67f32) * cast::<Ext, _>(f.max_face));
        } else if f.buying_power - accrued < cost {
            f.max_face = to_quad(cast::<Ext, _>(f.max_face) - Ext::from_f64(1.0));
        }
        if !(f.buying_power - accrued < cost) {
            break;
        }
    }
}

/// Port of TradeTBills (0x69af92) with its GOSUBs TradeTBills_Confirm (0x69b8c1) and TradeTBills_Exit
/// (0x69bb37).
///
/// The acting entity moves cash into T-bills or back (`BalanceCashTBills` first). The explanation (with
/// a note on AutoPilot for unmanaged companies) is shown only until a player has seen it
/// (`TBillHelpShownPlr`). Without cash and T-bills: message box. With only one of them the direction is
/// given (an info box unless the current player has seen it), with both the player is asked (YES = buy,
/// NO = sell, CANCEL = back). The amount is asked until it is at most the available amount (+ 0.005;
/// nothing or a non-positive amount cancels); then cash and T-bills are swapped (T-bills not below 0) and
/// confirmed (with an AutoPilot note when the company now holds over 4x its cash in T-bills).
// PB locals start zeroed; the port keeps them initialised like the original.
#[allow(unused_assignments)]
pub fn trade_t_bills(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let entity_id = g.active_entity;
    balance_cash_t_bills(g, entity_id);
    let mut msg_body = PbStr::new();
    let mut missing_asset = PbStr::new();
    let mut action;
    let max_amount;
    let mut help_text = s(__content.string(11847))
        + &g.active_entity_name
        + __content.string(11848)
        + __content.string(11849)
        + __content.string(11850)
        + __content.string(11851)
        + &fmt_trim(cast::<Ext, _>(g.t_bill_rate), "###.00")
        + "%,"
        + __content.string(11852)
        + "\r"
        + "\r";
    let small = sf(0.005f32);
    if 10 < g.active_entity {
        let ae = g.active_entity;
        if get_managing_player(g, ae) == 0 && (small < g.t_bills[entity_id] || small < g.cash[entity_id]) {
            help_text = help_text
                + __content.string(11853)
                + "since "
                + &g.active_entity_name
                + __content.string(11854)
                + __content.string(11855)
                + __content.string(11856)
                + __content.string(11857)
                + "\r"
                + "\r";
        }
    }
    if 0 < g.t_bill_help_shown_plr {
        help_text = PbStr::new();
    }
    let has_t_bills = small < g.t_bills[entity_id];
    let has_cash = small < g.cash[entity_id];
    if !has_t_bills && !has_cash {
        action = g.active_entity_name.clone() + " does";
        if g.cur_player == g.active_entity {
            action = s("You do");
        }
        msg_body = action
            + __content.string(11858)
            + __content.string(11859)
            + "\r"
            + "\r"
            + __content.string(11860);
        ui.msgbox(&(help_text + &msg_body), 8208, &s(__content.string(11861)));
        return;
    }
    if has_t_bills != has_cash {
        if has_t_bills {
            max_amount = g.t_bills[entity_id];
            action = s("sell");
            missing_asset = s("cash");
        } else {
            max_amount = g.cash[entity_id];
            action = s("buy");
            missing_asset = s("T-bills");
        }
        let enter_prompt = s(__content.string(11862)) + &action + ".";
        if g.cur_player != g.t_bill_help_shown_plr {
            let title = mcase(action.as_bytes(), CaseMode::Ansi) + __content.string(11863);
            ui.msgbox(&(help_text.clone() + &msg_body + &enter_prompt), 8256, &title);
        }
        help_text = g.active_entity_name.clone() + __content.string(3737) + &missing_asset + ". ";
    } else {
        msg_body = msg_body + __content.string(11864) + "\r" + "\r" + __content.string(11865);
        let answer = ui.msgbox(&(help_text.clone() + &msg_body), 8227, &s(__content.string(11866)));
        if answer == 2 {
            return;
        }
        action = s("buy");
        let mut m = g.cash[entity_id];
        if answer == 7 {
            action = s("sell");
            m = g.t_bills[entity_id];
        }
        max_amount = m;
        help_text = PbStr::new();
    }
    help_text = help_text
        + __content.string(11867)
        + &action
        + __content.string(11868)
        + &g.million_prefix_lc
        + __content.string(11869);
    g.t_bill_help_shown_plr = g.cur_player;
    let amount = loop {
        let title = mcase(action.as_bytes(), CaseMode::Ansi) + __content.string(11863);
        let input_text = ui.inputbox(&help_text, &title, &fmt_trim(max_amount, "* ###,.00"));
        let amount = val(remove_any(input_text.as_bytes(), b"$ ,").as_bytes());
        if amount <= Ext::from_f64(0.0) {
            return;
        }
        if amount <= max_amount + small {
            break amount;
        }
        // (`amount > maxAmount` always holds here: the original's fall-through into the GOSUB is dead.)
    };
    if action == "buy" {
        g.cash[entity_id] = g.cash[entity_id] - amount;
        g.t_bills[entity_id] = amount + g.t_bills[entity_id];
    } else {
        g.cash[entity_id] = amount + g.cash[entity_id];
        g.t_bills[entity_id] = g.t_bills[entity_id] - amount;
        if g.t_bills[entity_id] < Ext::from_f64(0.0) {
            g.t_bills[entity_id] = Ext::from_f64(0.0);
        }
    }
    trade_t_bills_confirm(g, ui, entity_id, &mut action, amount);
}

/// Port of TradeTBills_Confirm (0x69b8c1), GOSUB of TradeTBills: the "Confirmation" box.
fn trade_t_bills_confirm(g: &Globals, ui: &mut dyn Ui, entity_id: i32, action: &mut PbStr, amount: Ext) {
    let __content = crate::content::current();
    if *action == "buy" {
        *action = s("bought");
    } else {
        *action = s("sold");
    }
    let mut help_text = s(__content.string(3176))
        + "\r"
        + "\r"
        + &g.active_entity_name
        + " has "
        + &*action
        + __content.string(11870)
        + __content.string(9358)
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(amount, "* ###,.00")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + &g.currency_suffix
        + ".";
    if 10 < entity_id && g.cash[entity_id] * Ext::from_f64(4.0) < g.t_bills[entity_id] && g.auto_pilot[entity_id] == 1 {
        help_text = help_text
            + "\r"
            + "\r"
            + __content.string(11871)
            + &g.ent_name[entity_id]
            + __content.string(11872)
            + __content.string(11873)
            + __content.string(11874);
    }
    ui.msgbox(&help_text, 8256, &s(__content.string(11875)));
}
