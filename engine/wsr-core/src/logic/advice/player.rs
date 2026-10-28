//! Mode 1 with a player selected: financial situation, performance and taxes.
use crate::pb::real::{Ext, cast};

use super::prelude::*;
use super::End;
use crate::logic::{finance, holdings, swaps, valuation};
use crate::logic::report::number_to_words;

fn words(n: i32, percent: bool) -> PbStr { number_to_words(n, 0, i32::from(percent)) }
fn push(dst: &mut PbStr, text: &PbStr) { *dst = dst.clone() + text; }

pub(super) fn mode1_player(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> Result<End, Abort> {
    let __content = crate::content::current();
    let e = c.entity_id;
    let p = g.cur_player;
    let mut paragraph = s(__content.string(1687));
    finance::calc_entity_financials(g, ui, rng, e);
    let streak = valuation::price_trend_streak(g, e);
    let mut change = valuation::price_pct_change_months(g, e, 12);
    let two_year = valuation::price_pct_change_months(g, e, 24);
    if streak.abs() > 5 || change.abs() > 9 {
        if streak == 0 {
            if g.net_worth >= Ext::from_f64(0.0) {
                paragraph = paragraph + __content.string(1688) + if change < 0 { "de" } else { "in" } + __content.string(1689) + &words(change, true) + __content.string(1347);
                let trend = valuation::price_trend_direction(g, e, 2, 1);
                if change > 0 && trend < 0 { paragraph = paragraph + __content.string(1690); }
                if change < 0 && trend > 0 { paragraph = paragraph + __content.string(1691); }
                paragraph = paragraph + ". ";
            } else { paragraph = paragraph + __content.string(1692); }
        } else {
            paragraph = paragraph + __content.string(1688) + if streak < 0 { "de" } else { "in" } + __content.string(1693) + &words(streak, false) + __content.string(1694);
            change = valuation::price_pct_change_months(g, e, streak.abs());
            if change != 0 { paragraph = paragraph + __content.string(1695) + if change < 0 { __content.string(1696) } else { "grown" } + &words(change, true) + __content.string(1697); }
            paragraph = paragraph + ". ";
            if change < -15 { paragraph = paragraph + __content.string(1698); }
        }
    } else if g.net_worth < Ext::from_f64(0.0) {
        paragraph = paragraph + __content.string(1692);
    } else if two_year < -20 && two_year > -99 {
        paragraph = paragraph + __content.string(1699) + &words(two_year, true) + __content.string(1700);
    } else {
        let five = valuation::price_pct_change_months(g, e, 59);
        if five < 0 {
            paragraph = s(__content.string(1701)) + &words(five, true) + ". ";
        } else if five > 50 && g.player_cheat_code[p] < 6 {
            paragraph = s(__content.string(1702)) + &words(five, true) + __content.string(1700);
            if (g.long_game_marker > 5 && g.game_end_year - g.start_year < 36) || g.antitrust_cheat_used == 1 { paragraph = PbStr::new(); }
        }
    }
    c.under_investigation = i32::from(p == g.insider_probe_player || g.tipster_state[p] == 4);
    if c.under_investigation == 1 {
        let indicted = p == g.insider_probe_player && g.tipster_state[p] == -1;
        paragraph = paragraph + CRLF2 + if indicted { __content.string(1703) } else { "A major" } + __content.string(1704) + if indicted { "DICTMENT" } else { "VESTIGATION" } + __content.string(1705) + if indicted { __content.string(1706) } else { __content.string(1707) } + __content.string(1708);
        if g.nice_day_shown == 0 { paragraph = paragraph + CRLF2 + __content.string(1709) + CRLF2; g.nice_day_shown = 1; }
    }
    if g.long_game_marker > 5 && g.net_worth > g.exchange_rate * Ext::from_f64(100.0) && g.game_end_year - g.start_year <= 35 {
        paragraph = paragraph + CRLF2 + __content.string(14400) + CRLF2;
    } else if g.net_worth > g.exchange_rate * Ext::from_f64(100.0) {
        if g.player_cheat_code[p] >= 6 { paragraph = paragraph + CRLF2 + __content.string(14401) + CRLF2; }
        else if g.antitrust_cheat_used == 1 { paragraph = paragraph + CRLF2 + __content.string(14402) + CRLF2; }
    }
    push(&mut g.help_text, &paragraph);
    let mut para = PbStr::new();
    let mut phrase = PbStr::new();
    let mut clause = s(". ");
    let rating = g.credit_rating_num[e];
    if rating < 2 {
        if g.bank_debt[e] <= Ext::from_f64(0.0) { phrase = s(__content.string(1710)); }
        else { para = s(__content.string(1711)) + &g.lender_bank_name + __content.string(1712) + CRLF2 + __content.string(1713); }
    } else if rating < 7 {
        phrase = s(if rating < 4 { __content.string(1714) } else { " poor" });
        clause = s(__content.string(1715));
        if g.t_bills[e] + g.cash[e] < g.tmp_bank_loan * sf(0.01f32) { clause = s(__content.string(1716)); }
    } else {
        phrase = s(" decent"); clause = s(__content.string(1717));
        if rating > 7 {
            phrase = s(__content.string(1718));
            if g.unused_credit_line > Ext::from_f64(0.0) && g.cash[e] >= Ext::from_f64(0.0) {
                clause = s(__content.string(1719));
                if g.commod_margin_bal[e] != Ext::from_f64(0.0) || g.futures_net_value != Ext::from_f64(0.0) { clause = clause + CRLF2 + __content.string(1720); }
            } else if g.cash[e] >= Ext::from_f64(0.0) { clause = clause + __content.string(1721); }
        }
        if g.cash[e] < Ext::from_f64(0.0) {
            clause = s(__content.string(1722));
            if g.unused_credit_line > Ext::from_f64(0.0) { clause = clause + __content.string(1723); }
            clause = clause + __content.string(1724);
        }
        if c.under_investigation == 1 { clause = s(". "); }
    }
    if para.is_empty() { para = s(__content.string(1725)) + &phrase + __content.string(1726) + &g.credit_rating_str + "\")" + &clause; }
    if g.total_assets * Ext::from_f64(0.3) < g.cash[e] && g.exchange_rate * Ext::from_f64(300.0) < g.net_worth && rating > 4 {
        para = para + CRLF2 + __content.string(1727);
        if g.total_assets * Ext::from_f64(0.2) < g.t_bills[e] { para = para + __content.string(1728); }
        if g.base_interest_rate > Ext::from_f32(8.0f32) && g.monetary_policy < 2 {
            para = para + __content.string(1729);
            if swaps::sum_swap_notional(g, e, 0) == Ext::from_f64(0.0) { para = para + __content.string(1730); }
        }
    }
    let largest = holdings::get_largest_holding_id(g, e);
    if largest > 10 {
        let mut months = valuation::price_trend_streak(g, largest);
        let mut pct = valuation::price_pct_change_months(g, largest, 6);
        if months == 0 { months = 6; } else {
            let pct2 = valuation::price_pct_change_months(g, largest, months.abs());
            if pct.abs() < pct2.abs() { pct = pct2; }
        }
        if pct.abs() > 10 {
            let desc = if pct >= 0 { __content.string(1731) } else if pct.abs() > 19 { "plunged" } else { __content.string(1057) };
            para = para + CRLF2 + __content.string(1732) + &g.ent_name[largest] + __content.string(1733) + desc + &words(pct, true) + __content.string(1094) + &words(months, false) + __content.string(1095);
        }
    }
    c.swap_payment = to_quad(swaps::process_swap_payments(g, ui, rng, e, &s("R")));
    if g.exchange_rate * Ext::from_f64(25.0) < (cast::<Ext, _>(c.swap_payment)).abs() {
        if para.len() > 40 { para = para + CRLF2; }
        let action = if c.swap_payment < 0 { "lose" } else if g.cur_quarter > 2 { "make" } else { "earn" };
        para = para + __content.string(1734) + action + " " + &c.currency_prefix + &fmt_trim((cast::<Ext, _>(c.swap_payment)).abs(), "* #########,") + " " + &g.million_prefix_lc + "illion" + &c.currency_suffix + __content.string(1735);
        if c.swap_payment < 0 {
            para = para + if (cast::<Ext, _>(c.swap_payment)) < g.exchange_rate * -Ext::from_f64(300.0) { __content.string(1736) } else { "So far," } + __content.string(1737);
            if g.net_worth < (cast::<Ext, _>(c.swap_payment)).abs() && g.net_worth > Ext::from_f64(0.0) { para = para + __content.string(1738); }
        } else if c.swap_payment > 0 { para = para + __content.string(1739) + if cast::<Ext, _>(c.swap_payment) > g.exchange_rate * Ext::from_f64(300.0) { __content.string(1740) } else { "correct" } + ". "; }
    }
    g.help_text = g.help_text.clone() + &para + CRLF2;
    if g.game_end_flag == "Y" { return game_end(g, c); }
    taxes(g, ui, rng, c)?;
    Ok(End::Cleanup)
}

fn game_end(g: &mut Globals, c: &mut Ctx) -> Result<End, Abort> {
    let __content = crate::content::current();
    let e = c.entity_id;
    let p = g.cur_player;
    let mut para = PbStr::new();
    let mut ratio = Ext::from_f64(0.0);
    g.help_text = PbStr::new();
    if g.start_net_worth == 0 { g.start_net_worth = to_long(g.exchange_rate * Ext::from_f64(1000.0)); }
    if g.start_net_worth == 0 {
        if g.net_worth <= g.exchange_rate * Ext::from_f64(1000.0) { para = s(__content.string(1741)); }
        else {
            let phrase = if g.net_worth > g.exchange_rate * Ext::from_f64(5000.0) { __content.string(1742) } else if g.net_worth > g.exchange_rate * Ext::from_f64(100000.0) { __content.string(1743) } else { "" };
            para = s(__content.string(1744)) + phrase + __content.string(1745);
        }
    } else {
        ratio = g.net_worth / (g.exchange_rate * cast::<Ext, _>(g.start_net_worth));
        let pct = to_long(ratio * Ext::from_f64(100.0) - Ext::from_f64(100.0));
        let years = g.cur_year - g.start_year;
        let mut rate = if ratio > Ext::from_f64(0.0) { super::calc_required_annual_return(g, g.net_worth, years) } else { Ext::from_f64(0.0) };
        if rate > Ext::from_f64(1000.0) { rate = -Ext::from_f64(1.0); }
        if rate != -Ext::from_f64(1.0) {
            if rate < Ext::from_f64(8.0) {
                if pct < 2 {
                    if pct.abs() < 2 { para = s(__content.string(1746)); }
                    else if pct < 0 && g.net_worth > Ext::from_f64(0.0) { para = s(__content.string(1747)) + &words(pct, true) + __content.string(1748); }
                    else if years < 10 {
                        let duration = if years == 0 { s(__content.string(1749)) } else { s("only") + &words(years, false) + if years > 1 { " years" } else { " year" } };
                        para = s(__content.string(1750)) + &duration + ". ";
                    } else { para = s(__content.string(1751)) + &words(years, false) + __content.string(1752); }
                } else {
                    para = s(__content.string(1753)) + &words(pct, true) + __content.string(1754);
                    if pct > 19 { para = para + __content.string(1755); }
                }
            } else if rate < Ext::from_f64(20.0) {
                para = s(__content.string(1756)) + &words(pct, true) + " in this game, in" + &words(years, false) + if years == 1 { " year" } else { " years" } + __content.string(1757) + if g.is_computer_player[e] == 0 { __content.string(1758) } else { __content.string(1759) };
            } else {
                para = s(__content.string(1760));
                if ratio > Ext::from_f64(3.0) {
                    let n = to_long(fix(ratio));
                    let phrase = if n > 1000 { fmt_trim(cast::<Ext, _>(n), "* ###########,") } else { trim(&words(n, false)) };
                    para = para + __content.string(1761) + &phrase + __content.string(1762);
                }
                if g.long_game_marker < 6 && g.player_cheat_code[p] < 6 && g.antitrust_cheat_used == 0 { para = para + __content.string(1763); }
            }
        }
        if rate != Ext::from_f64(0.0) {
            para = para + __content.string(1764);
            if rate == -Ext::from_f64(1.0) { para = para + __content.string(1765); }
            else {
                para = para + &fmt_trim(rate, "* ###.0") + "%";
                if years > 1 { para = para + __content.string(1766) + &words(years, false) + " years"; }
                para = para + ". ";
            }
        }
    }
    if ratio > Ext::from_f64(1.5) && (g.long_game_marker >= 6 || g.player_cheat_code[p] >= 6 || g.game_end_year - g.start_year >= 36 || g.antitrust_cheat_used == 1) {
        para = para + __content.string(1767);
        if g.long_game_marker >= 6 && g.game_end_year - g.start_year <= 35 { para = para + __content.string(1768); }
        else if g.player_cheat_code[p] >= 6 { para = para + __content.string(1769); }
        else if g.antitrust_cheat_used == 1 { para = para + __content.string(1770); }
        else if g.game_end_year - g.start_year > 35 { para = para + __content.string(1771); }
    }
    if g.start_net_worth > 0 { para = para + CRLF2 + __content.string(1772) + &money(g, c, g.exchange_rate * cast::<Ext, _>(g.start_net_worth), "* #########,") + ".) "; }
    g.help_text = s(__content.string(1773)) + &ucase(g.ent_name[p].as_bytes(), CaseMode::Ansi) + "):" + CRLF2 + &para;
    if (ratio != Ext::from_f64(0.0) && ratio <= Ext::from_f64(0.1)) || g.cur_player_name == "...OUT..." { g.help_text = g.help_text.clone() + CRLF2 + __content.string(1774); }
    Ok(End::Exit)
}
fn money(g: &Globals, c: &Ctx, x: Ext, mask: &str) -> PbStr {
    c.currency_prefix.clone() + &fmt_trim(x, mask) + " " + &g.million_prefix_lc + "illion" + &c.currency_suffix
}

fn taxes(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, c: &mut Ctx) -> R {
    let __content = crate::content::current();
    let e = c.entity_id;
    g.help_text = g.help_text.clone() + __content.string(1775);
    let mut gains = g.player_cap_gains[e].max(Ext::from_f64(0.0));
    let ordinary = g.accrued_pl[e] + cast::<Ext, _>(g.extraordinary_pl[e]) + g.div_recvd1[e] + g.div_recvd2[e];
    let tax = |ordinary: Ext, gains: Ext, g: &Globals| -> Ext {
        if ordinary < Ext::from_f64(0.0) { ((ordinary + gains).max(Ext::from_f64(0.0)) * sf(0.01f32) * cast::<Ext, _>(g.cap_gains_tax_rate)).max(Ext::from_f64(0.0)) }
        else { (ordinary * cast::<Ext, _>(5 + g.corp_tax_rate) * sf(0.01f32) + gains * sf(0.01f32) * cast::<Ext, _>(g.cap_gains_tax_rate)).max(Ext::from_f64(0.0)) }
    };
    let accrued = tax(ordinary, gains, g);
    g.help_text = g.help_text.clone() + &money(g, c, accrued, "* ###,.00") + ". ";
    let ceo_corp = g.player_ceo_of[e];
    let ceo = crate::logic::earnings::calc_ceo_pay(g, ui, rng, ceo_corp, 0);
    let pay = cast::<Ext, _>(g.ceo_bonus_multiple) * ceo + ceo * cast::<Ext, _>(5 - g.cur_quarter) * Ext::from_f64(0.25);
    let mut projected = ordinary + pay.max(Ext::from_f64(0.0));
    let mut neg_div = Ext::from_f64(0.0);
    for slot in 1..16 {
        let stock = cast::<i32, _>(g.hold_stock_id[(slot, e)]);
        if stock != 0 {
            let value = g.shares_outstanding[stock] * g.dividend_per_share[stock] * cast::<Ext, _>(g.hold_stock_pct[(slot, e)]) * Ext::from_f64(0.25);
            let periods = cast::<Ext, _>(5 - g.cur_quarter - i32::from(stock < g.sim_co_count));
            if value <= Ext::from_f64(0.0) { neg_div += periods * value; } else { projected += periods * value; }
        }
        let bond = g.hold_bond_id[(slot, e)];
        if bond != 0 {
            let coupon = val(right(g.bond_issue_info[bond].as_bytes(), 5).as_bytes());
            let periods = cast::<Ext, _>(5 - g.cur_quarter - i32::from(bond < g.sim_co_count));
            projected += periods * cast::<Ext, _>(g.hold_bond_face[(slot, e)]) * coupon * sf(0.0025f32);
        }
    }
    let periods = cast::<Ext, _>(5 - g.cur_quarter);
    let long_rate = val(right(g.long_bond_desc.as_bytes(), 5).as_bytes());
    let short_rate = val(right(g.short_bond_desc.as_bytes(), 5).as_bytes());
    projected += periods * (cast::<Ext, _>(g.long_gov_bond_face[e]) * long_rate * sf(0.0025f32) + cast::<Ext, _>(g.short_gov_bond_face[e]) * short_rate * sf(0.0025f32));
    crate::logic::earnings::balance_cash_t_bills(g, e);
    projected += periods * g.t_bills[e] * sf(0.0025f32) * cast::<Ext, _>(g.t_bill_rate) + periods * g.tmp_player_advances * sf(0.0025f32) * cast::<Ext, _>(g.prime_rate);
    if g.bank_debt[e] > Ext::from_f64(0.0) { projected -= periods * g.bank_debt[e] * sf(0.0025f32) * cast::<Ext, _>(g.borrow_rate); }
    gains = (g.player_cap_gains[e] + neg_div).max(Ext::from_f64(0.0));
    let mut income_tax = tax(projected, gains, g);
    let mut phrase = s(__content.string(1776)) + &money(g, c, income_tax, "#############,.00") + ". " + CRLF2;
    finance::calc_entity_financials(g, ui, rng, e);
    let mut other = finance::calc_capital_taxes(g, e);
    if g.wealth_tax_rate > 0 {
        other -= g.tmp_capital_tax;
        g.tmp_capital_tax = ((g.net_worth - g.exchange_rate * Ext::from_f64(1000.0)) * cast::<Ext, _>(g.wealth_tax_rate) * sf(0.01f32) + cast::<Ext, _>(g.wealth_tax_rate) * sf(0.01f32) * g.player_prepaid_tax[e]).max(Ext::from_f64(0.0));
        other += g.tmp_capital_tax;
    }
    other += cast::<Ext, _>(4 - g.cur_quarter) * g.corp_shares_tax_due;
    if g.player_name[e] == "...OUT..." { other = Ext::from_f64(0.0); }
    let current_tax = crate::logic::cashflow::calc_player_income_tax(g, g.cur_player);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_accrued_inc_tax = income_tax.max(current_tax);
    if g.player_name[e] == "...OUT..." { g.tmp_accrued_inc_tax = Ext::from_f64(0.0); }
    let adjusted = g.net_worth + if g.cur_quarter == 4 && c.swap_payment >= 0 { cast::<Ext, _>(c.swap_payment) } else { Ext::from_f64(0.0) };
    if adjusted < g.tmp_accrued_inc_tax - g.player_prepaid_tax[e] && g.tmp_accrued_inc_tax - g.player_prepaid_tax[e] > Ext::from_f64(0.0) && g.net_worth > Ext::from_f64(0.0) {
        income_tax = g.tmp_accrued_inc_tax;
        g.help_text = s(__content.string(1777)) + CRLF2 + __content.string(1778) + CRLF2 + &phrase;
        phrase = s(__content.string(1779)) + &money(g, c, g.net_worth, "#############,.00") + ". ";
        phrase = phrase + if g.currency_code == "USA" { __content.string(1780) } else if g.currency_code == "UK" { __content.string(1781) } else { __content.string(1782) };
        if g.cur_quarter < 4 { phrase = phrase + __content.string(1783); }
        else if g.sim_co_count < 750 { phrase = phrase + __content.string(14403); }
        else { phrase = phrase + __content.string(1784); }
        phrase = phrase + __content.string(1785);
        if g.credit_rating_num[e] > 6 { phrase = phrase + CRLF2 + __content.string(1786); }
        phrase = phrase + CRLF2;
        c.tax_crisis = 1;
    }
    let prepaid = g.player_prepaid_tax[e];
    if prepaid <= Ext::from_f64(0.0) {
        if income_tax > Ext::from_f64(0.0) { phrase = phrase + __content.string(1787); }
    } else {
        phrase = phrase + __content.string(1788) + &money(g, c, prepaid, "#############,.00") + ". ";
        if prepaid < income_tax {
            phrase = phrase + __content.string(1789) + &money(g, c, income_tax - prepaid, "###############,.00") + __content.string(1790) + if other > Ext::from_f64(0.01) { "INCOME" } else { "income" } + __content.string(1791);
            if income_tax < g.net_worth || g.liquid_funds < income_tax {
                let available = g.t_bills[e] + g.unused_credit_line + g.cash[e];
                if g.net_worth <= available { phrase = phrase + __content.string(1792) + &money(g, c, g.net_worth, "#############,.00") + ". "; }
                else { phrase = phrase + __content.string(1793) + &money(g, c, available, "#############,.00") + ". "; }
            }
        } else if prepaid > income_tax { phrase = phrase + __content.string(1794) + &money(g, c, prepaid - income_tax, "###############,.00") + ". "; }
    }
    if other > Ext::from_f64(0.01) {
        if right(phrase.as_bytes(), 2) != "\r\n" { phrase = phrase + CRLF2; }
        // This sentence omits the currency suffix in the original.
        phrase = phrase + __content.string(1795) + &c.currency_prefix + &fmt_trim(other, "###############,.00") + " " + &g.million_prefix_lc + "illion ";
        phrase = phrase + if g.tmp_capital_tax == Ext::from_f64(0.0) { __content.string(1796) } else if g.corp_shares_tax_due == Ext::from_f64(0.0) { __content.string(1797) } else { __content.string(1798) };
    }
    let owed = income_tax + other - prepaid;
    if g.liquid_funds < owed && owed > Ext::from_f64(10.0) && c.tax_crisis == 0 && income_tax < g.net_worth && g.net_worth > Ext::from_f64(0.0) {
        phrase = phrase + CRLF2 + __content.string(1799) + if other == Ext::from_f64(0.0) { __content.string(1800) } else { __content.string(1801) } + __content.string(1802);
    }
    if right(phrase.as_bytes(), 2) != "\r\n" { phrase = phrase + CRLF2; }
    g.help_text = g.help_text.clone() + &phrase;
    Ok(())
}
