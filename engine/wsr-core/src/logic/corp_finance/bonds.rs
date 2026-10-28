//! Bond issues and buybacks of the player's companies (and ETFs): `IssueBondsMenu` (0x5a40a2) with its
//! GOSUBs, `BondBuybackMenu` (0x5a0aa1, [`buyback`]), `PickConvertiblePrice` (0x688014) and `StockSplit`
//! (0x5bc3ff, [`convertible`]), and the menu GOSUB `MenuCmd_PlaceBondsWithBank` (0x556858).
//!
//! The GOSUB blocks of `IssueBondsMenu` run on its stack frame: they are private fns over [`IssueFrame`].
//! A GOSUB that jumps to the routine's exit (`CorpAction_ExitStub`) returns `true` here and the caller
//! returns at once.
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::{calc_credit_rating, calc_entity_financials, get_controller_type};
use crate::logic::holdings::find_bond_holding_slot;
use crate::logic::sim_entity::bonds::issue_bonds;
use crate::logic::report::show_quick_message;
use crate::logic::sim_entity::fin_inst::buy_corp_bonds;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::trading::get_selected_player_etf;
use crate::pb::string::remove_any;

mod buyback;
mod convertible;

pub use buyback::bond_buyback_menu;
pub use convertible::{pick_convertible_price, stock_split};

/// A string literal.
pub(crate) fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `RTRIM$(LTRIM$(CurrencySymbol))`.
pub(crate) fn cur_sym(g: &Globals) -> PbStr {
    trim(g.currency_symbol.as_bytes())
}

/// `VAL(REMOVE$(text, ANY chars))`.
pub(crate) fn val_removed(text: &PbStr, chars: &[u8]) -> Ext {
    val(remove_any(text.as_bytes(), chars).as_bytes())
}

/// `ShowQuickMessage` of a temporary string.
pub(crate) fn quick(g: &mut Globals, ui: &mut dyn Ui, text: PbStr) {
    let mut t = text;
    show_quick_message(g, ui, &mut t);
}

/// Port of CorpAction_ExitStub (0x5a734d).
/// The shared exit (frame release and `RET`) of `IssueBondsMenu`; its GOSUBs jump here to leave the
/// whole routine. In the port: a `return` (GOSUBs return `true`). No game state.
pub fn corp_action_exit_stub() {}

/// Port of ExecuteMenuCommand_ExitSub (0x557b6f).
/// The exit (frame release and `RET 4`) of `ExecuteMenuCommand`; [`menu_cmd_place_bonds_with_bank`]
/// jumps here, which the port reports by returning `true`. No game state.
pub fn execute_menu_command_exit_sub() {}

/// The locals of `IssueBondsMenu` (0x5a40a2) shared with its GOSUBs (zeroed at entry).
#[derive(Default)]
struct IssueFrame {
    etf_issuer: i32,
    issuer_name: PbStr,
    issuer_id: i32,
    conv_notice_shown: i32,
    base_rate: Ext,
    conv_issued: i32,
    prepay_penalty_flag: i64,
    coupon_rate: Ext,
    term_years: i32,
    answer: i32,
    payoff_amt: i64,
    placement_pct: i64,
    issue_amount: i64,
    institution_id: i32,
    attempts: i32,
    payoff_request: Ext,
    input_text: PbStr,
    aux_text: PbStr,
    msg_text: PbStr,
    default_years: i32,
    max_years: i32,
    quoted_rate: Ext,
    /// -0x12c (unnamed): years entered in the rate quote.
    years_entry: i32,
}

/// Port of IssueBondsMenu (0x5a40a2).
/// The player's "issue bonds" action for `ActiveEntity` (or a managed ETF, see the PORT-TODO): existing
/// bonds must first be retired ([`bond_buyback_menu`] mode 1); refuses poor credit, new companies, more
/// than 6 recent issues of the player, SuperFund/asbestos companies, an over-leveraged ETF and base rates
/// above 12.5%. Asks how much bank debt to pay off (2% prepayment penalty against a non-controlled bank
/// from difficulty 3) and the amount (up to 4x net worth less bank loans, 1.5x for banks, `MaxBondIssue`;
/// at least 50 x `ExchangeRate`), re-rates the credit with the new debt, quotes the coupon (BorrowRate + 1,
/// rounded to 1/20, less for good ratings; fund adjustments) and the maturity
/// ([`issue_bonds_menu_rate_quote`]), offers convertibles when the share price is high enough (else a
/// reverse split, [`stock_split`]) via [`pick_convertible_price`], and issues straight bonds with
/// `IssueBonds`. Pays down the bank loan, charges the penalty, posts the notice, counts the action, and
/// on odd `RandSeed` places part of the new issue with up to 50 random banks/insurers
/// ([`issue_bonds_menu_place_with_institution`]).
pub fn issue_bonds_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut f = IssueFrame::default();
    if g.entity_status[g.active_entity] != 1 {
        return;
    }
    f.issuer_id = g.active_entity;
    f.issuer_name = g.active_entity_name.clone();
    f.etf_issuer = get_selected_player_etf(g, ui, rng);
    if f.etf_issuer >= 1581 {
        let etf = f.etf_issuer;
        f.aux_text = g.ent_name[etf].clone() + " (" + &g.ent_symbol[etf] + __content.string(3017)
            + __content.string(3018) + &g.ent_name[g.fund_adviser[etf]] + ". "
            + __content.string(3019) + "\r" + "\r" + __content.string(3020)
            + &g.ent_name[etf] + __content.string(3021) + "\r" + "\r" + __content.string(3022);
        if g.active_entity < 11 || g.bonds_issued[g.active_entity] != 0 {
            f.aux_text = f.aux_text.clone() + __content.string(3023);
        } else {
            f.aux_text = f.aux_text.clone() + __content.string(3024) + &g.active_entity_name + __content.string(3025);
        }
        f.answer = ui.msgbox(&f.aux_text, 8483, &s(__content.string(3026)));
        if f.answer == 2 {
            return;
        }
        if f.answer == 7 {
            if 10 < g.active_entity && g.bonds_issued[g.active_entity] == 0 {
                f.etf_issuer = 0;
            } else {
                return;
            }
        } else {
            f.issuer_id = etf;
            f.issuer_name = g.ent_name[etf].clone();
            if g.bonds_issued[f.issuer_id] <= 0 {
                if etf < 1584 {
                    if Ext::from_f32(5.0f32) < g.base_interest_rate {
                        quick(g, ui,
                            s(__content.string(3027))
                                + __content.string(3028),
                        );
                    }
                } else if etf < 1586 {
                    quick(g, ui,
                        s(__content.string(3029))
                            + __content.string(3030)
                            + __content.string(3031)
                            + __content.string(3032)
                            + __content.string(3033),
                    );
                }
            }
        }
    }
    // LAB_005a4546
    if f.issuer_id < 11 {
        quick(g, ui, s(__content.string(3034)) + &g.active_entity_name + __content.string(3035));
        return;
    }
    if 0 < g.bonds_issued[f.issuer_id] {
        if 1580 < f.etf_issuer {
            f.aux_text = f.issuer_name.clone() + __content.string(3036)
                + __content.string(3037) + &f.issuer_name + __content.string(2611);
            show_quick_message(g, ui, &mut f.aux_text);
            return;
        }
        bond_buyback_menu(g, ui, rng, 1, time_secs);
        if 0 < g.bonds_issued[f.issuer_id] {
            return;
        }
    }
    let id = f.issuer_id;
    let max_bond_issue = g.max_bond_issue;
    loop {
        let mut max_issue: i64;
        let mut prev_rating;
        loop {
            calc_entity_financials(g, ui, rng, id);
            prev_rating = g.credit_rating_str.clone();
            let mut credit_ok: i32 = cast::<i32, _>(Ext::from_f64(0.0) <= g.net_worth);
            max_issue = to_quad(g.net_worth * Ext::from_f64(4.0) - g.tmp_bank_loan);
            if g.industry_num[id] == 1 {
                max_issue = to_quad(g.net_worth * sf(1.5f32));
            }
            if 1580 < f.etf_issuer {
                max_issue = to_quad(g.net_worth * sf(0.333f32) - g.tmp_bank_loan);
            }
            if max_bond_issue < max_issue {
                max_issue = max_bond_issue;
            }
            if max_issue <= 0 {
                credit_ok = 0;
            }
            if f.etf_issuer == 0 && g.earnings_yr4_ago[id] == Ext::from_f64(0.0) && g.earnings_yr3_ago[id] == Ext::from_f64(0.0) {
                credit_ok = -1;
            }
            if 6 < g.player_recent_bond_issues[g.cur_player] && f.etf_issuer == 0 {
                credit_ok = -2;
            }
            if cast::<i32, _>(g.superfund_corp) == id || g.asbestos_corp.wrapping_abs() == id {
                credit_ok = -3;
            }
            if 1580 < f.etf_issuer && g.net_worth * sf(0.333f32) < g.tmp_bank_loan {
                credit_ok = -4;
            }
            if Ext::from_f32(12.5f32) < g.base_interest_rate {
                credit_ok = -5;
            }
            if credit_ok < 1 {
                f.msg_text = f.issuer_name.clone() + __content.string(3038);
                if credit_ok == -1 {
                    f.msg_text = f.issuer_name.clone() + __content.string(3039)
                        + __content.string(3040);
                }
                if credit_ok == -2 {
                    f.msg_text = s(__content.string(3041))
                        + __content.string(3042)
                        + __content.string(3043)
                        + __content.string(3044);
                }
                if credit_ok == -3 {
                    f.aux_text = s(__content.string(3045));
                    if cast::<i32, _>(g.superfund_corp) == id {
                        f.aux_text = s(__content.string(3046));
                    }
                    f.msg_text = s(__content.string(3047))
                        + __content.string(3048) + &f.issuer_name + ", due "
                        + __content.string(3049) + __content.string(3050)
                        + &f.aux_text + __content.string(3051);
                }
                if credit_ok == -4 {
                    f.msg_text = f.issuer_name.clone() + __content.string(3052)
                        + __content.string(3053)
                        + __content.string(3054);
                }
                if credit_ok == -5 {
                    f.msg_text = s(__content.string(3055))
                        + __content.string(3056);
                }
                ui.msgbox(&f.msg_text, 8208, &s(__content.string(3057)));
                return;
            }
            if Ext::from_f64(0.0) < g.bank_debt[id] && 85 < id && f.payoff_request == Ext::from_f64(0.0) {
                if g.cur_player != g.corp_controller[cast::<i32, _>(g.lending_bank[id])]
                    && 2 < g.difficulty_level
                    && f.etf_issuer == 0
                {
                    f.prepay_penalty_flag = 1;
                }
                let mut max_payoff = g.bank_debt[id];
                if (cast::<Ext, _>(max_bond_issue)) < max_payoff {
                    max_payoff = cast::<Ext, _>(max_bond_issue);
                }
                f.aux_text = fmt_trim(max_payoff, b"* ###,.00");
                f.msg_text = s(__content.string(3058)) + &f.aux_text + " " + &g.million_prefix_lc
                    + __content.string(3059) + &f.issuer_name + __content.string(3060);
                if f.prepay_penalty_flag == 1 {
                    f.msg_text = f.msg_text.clone() + "\r" + "\r" + __content.string(3061);
                }
                while f.payoff_request == Ext::from_f64(0.0) {
                    f.input_text = ui.inputbox(&f.msg_text, &s(__content.string(3062)), &f.aux_text);
                    if f.input_text.is_empty() {
                        return;
                    }
                    f.payoff_request = val_removed(&f.input_text, b"$ ,");
                    if f.payoff_request <= Ext::from_f64(0.0) {
                        f.payoff_request = -sf(0.0001f32);
                    }
                    if max_payoff + Ext::from_f64(1.0) < f.payoff_request {
                        quick(g, ui,
                            s(__content.string(3063)) + &f.issuer_name + __content.string(3064)
                                + __content.string(3065)
                                + __content.string(3066),
                        );
                        f.payoff_request = max_payoff;
                    }
                }
            }
            if f.payoff_request <= Ext::from_f64(0.0) {
                f.prepay_penalty_flag = 0;
            }
            if Ext::from_f64(0.0) < f.payoff_request {
                max_issue = to_quad(cast::<Ext, _>(max_issue) + f.payoff_request);
            }
            if max_bond_issue < max_issue {
                max_issue = max_bond_issue;
            }
            f.aux_text = fmt_trim(cast::<Ext, _>(max_issue), b"* ###,");
            f.msg_text = f.issuer_name.clone() + __content.string(3067) + &cur_sym(g) + &f.aux_text + " "
                + &g.million_prefix_lc + __content.string(3068) + "\r\n" + "\r\n" + __content.string(3069)
                + &f.issuer_name + __content.string(3070) + &g.million_prefix_lc + __content.string(3071);
            loop {
                f.input_text = ui.inputbox(&f.msg_text, &s(__content.string(3072)), &f.aux_text);
                f.issue_amount = to_quad(val_removed(&f.input_text, b"$ ,"));
                if f.issue_amount < 1 {
                    return;
                }
                if !(max_issue < f.issue_amount) {
                    break;
                }
            }
            f.input_text = trim(str_s(cast::<Ext, _>(f.issue_amount)).as_bytes());
            if (cast::<Ext, _>(f.issue_amount)) < g.exchange_rate * Ext::from_f64(50.0) {
                quick(g, ui, s(__content.string(3073)));
                continue;
            }
            break;
        }
        if f.payoff_request < Ext::from_f64(0.0) {
            f.payoff_request = Ext::from_f64(0.0);
        }
        f.payoff_amt = to_quad(f.payoff_request);
        if f.issue_amount < f.payoff_amt {
            f.payoff_amt = f.issue_amount;
        }
        let net_new = cast::<Ext, _>(f.issue_amount) - cast::<Ext, _>(f.payoff_amt);
        g.tmp_total_debt = g.tmp_total_debt + net_new;
        if g.tmp_total_debt < Ext::from_f64(0.0) {
            g.tmp_total_debt = Ext::from_f64(0.0);
        }
        g.total_assets = g.total_assets + net_new;
        if g.total_assets < Ext::from_f64(1.0) {
            g.total_assets = Ext::from_f64(1.0);
        }
        calc_credit_rating(g, id);
        f.coupon_rate = Ext::from_f64(1.0) + cast::<Ext, _>(g.borrow_rate);
        if g.industry_num[id] == 1 {
            f.coupon_rate = sf(1.25f32) + cast::<Ext, _>(g.borrow_rate);
        }
        f.placement_pct = to_quad(Ext::from_f64(20.0) * f.coupon_rate);
        f.coupon_rate = cast::<Ext, _>(f.placement_pct) / Ext::from_f64(20.0);
        if g.credit_rating_num[id] < 8 {
            f.coupon_rate = f.coupon_rate - Ext::from_f64(0.25);
        }
        if g.credit_rating_num[id] < 6 {
            f.coupon_rate = f.coupon_rate - Ext::from_f64(0.25);
        }
        if f.coupon_rate < Ext::from_f64(2.5) {
            f.coupon_rate = Ext::from_f64(2.5);
        }
        f.term_years = 10;
        if g.industry_num[id] == 71 {
            if id == 1600 {
                f.coupon_rate = f.coupon_rate + Ext::from_f64(0.25);
            } else if id < 1586 || 1599 < id {
                if id < 1584 {
                    f.coupon_rate = f.coupon_rate - Ext::from_f64(0.5);
                    if id == 1581 {
                        f.coupon_rate = f.coupon_rate - Ext::from_f64(0.25);
                    }
                } else if id == 1584 || id == 1585 {
                    f.coupon_rate = f.coupon_rate + Ext::from_f64(1.25);
                    f.term_years = 5;
                }
            } else {
                f.coupon_rate = f.coupon_rate - Ext::from_f64(0.25);
            }
        }
        if issue_bonds_menu_rate_quote(g, ui, &mut f) {
            return;
        }
        if f.etf_issuer == 0 {
            let price_ok = |g: &Globals| cast::<Ext, _>(g.cur_quarter) + Ext::from_f64(16.0) <= g.market_cap[id] / g.shares_outstanding[id];
            let mut offer_conv = price_ok(g);
            if !offer_conv && f.conv_notice_shown != 1 {
                f.conv_notice_shown = 1;
                f.msg_text = s(__content.string(3074))
                    + __content.string(3075) + &f.issuer_name + __content.string(3076)
                    + __content.string(3077) + "\r\n" + "\r\n"
                    + __content.string(3078)
                    + __content.string(3079) + "\r" + "\r" + __content.string(3080)
                    + &f.issuer_name + "?";
                f.answer = ui.msgbox(&f.msg_text, 8483, &s(__content.string(3081)));
                if f.answer == 2 {
                    return;
                }
                f.msg_text = PbStr::new();
                if f.answer != 7 {
                    stock_split(g, ui, 2);
                    if price_ok(g) {
                        f.conv_notice_shown = 0;
                        offer_conv = true;
                    } else {
                        quick(g, ui, s(__content.string(3082)));
                    }
                }
            }
            if offer_conv {
                // LAB_005a564a
                f.msg_text = s(__content.string(3083)) + &f.issuer_name + __content.string(3084)
                    + &cur_sym(g) + &fmt_trim(cast::<Ext, _>(f.issue_amount), b"* ###,") + " " + &g.million_prefix_lc
                    + __content.string(3085) + &str_s(cast::<Ext, _>(f.term_years + g.cur_year))
                    + __content.string(3086) + &fmt_trim(f.coupon_rate, b"##.00") + "%." + "\r" + "\r"
                    + __content.string(3087);
                if f.term_years < 11 {
                    f.msg_text = f.msg_text.clone() + ".";
                } else {
                    f.msg_text = f.msg_text.clone() + __content.string(3088);
                }
                f.msg_text = f.msg_text.clone() + "\r" + "\r" + __content.string(3089) + &f.issuer_name
                    + __content.string(3090);
                f.answer = ui.msgbox(&f.msg_text, 8484, &s(__content.string(3091)));
                if f.answer == 6 {
                    g.tmp_ext1 = f.payoff_request;
                    if (cast::<Ext, _>(f.issue_amount)) < f.payoff_request {
                        g.tmp_ext1 = cast::<Ext, _>(f.issue_amount);
                    }
                    g.tmp_ext2 = cast::<Ext, _>(f.prepay_penalty_flag);
                    let mut conv_term = f.term_years;
                    let mut conv_rate = f.coupon_rate;
                    if 10 < f.term_years {
                        conv_term = 10;
                        conv_rate = f.base_rate;
                    }
                    f.conv_issued = pick_convertible_price(g, ui, f.issue_amount, conv_rate, conv_term + g.cur_year);
                    if f.conv_issued == 0 {
                        f.msg_text = s(__content.string(3092));
                        f.answer = ui.msgbox(&f.msg_text, 8227, &s(__content.string(3093)));
                        if f.answer == 7 || f.answer == 2 {
                            return;
                        }
                    }
                }
            }
        }
        if f.conv_issued != 0 {
            break;
        }
        f.msg_text = s(__content.string(3094)) + &f.issuer_name
            + __content.string(3095) + &cur_sym(g) + &fmt_trim(cast::<Ext, _>(f.issue_amount), b"* ###,") + " "
            + &g.million_prefix_lc + "illion" + &g.currency_suffix + __content.string(2801) + &fmt_trim(f.coupon_rate, b"##.00")
            + __content.string(3096) + &str_s(cast::<Ext, _>(f.term_years + g.cur_year))
            + __content.string(3097) + &g.credit_rating_str + "." + "\r" + "\r";
        if Ext::from_f64(0.0) < f.payoff_request {
            let bank_debt = g.bank_debt[id];
            if bank_debt < f.payoff_request || (cast::<Ext, _>(f.issue_amount)) < f.payoff_request {
                if (cast::<Ext, _>(f.issue_amount)) <= f.payoff_request {
                    f.msg_text = f.msg_text.clone() + "All " + &cur_sym(g) + &fmt_trim(cast::<Ext, _>(f.issue_amount), b"* ###,.00");
                } else {
                    f.msg_text = f.msg_text.clone() + &cur_sym(g) + &fmt_trim(g.bank_debt[id], b"* ###,.00");
                }
            } else {
                f.msg_text = f.msg_text.clone() + &cur_sym(g) + &fmt_trim(f.payoff_request, b"* ###,.00");
            }
            f.msg_text = f.msg_text.clone() + " " + &g.million_prefix_lc + "illion" + &g.currency_suffix
                + __content.string(3098) + &g.ent_name[cast::<i32, _>(g.lending_bank[id])]
                + ". ";
            if f.prepay_penalty_flag == 1 {
                f.msg_text = f.msg_text.clone() + __content.string(3099);
            }
            f.msg_text = f.msg_text.clone() + "\r" + "\r";
        }
        f.msg_text = f.msg_text.clone() + __content.string(3100) + "\r" + "\r";
        if g.credit_rating_str != prev_rating {
            f.msg_text = f.msg_text.clone() + __content.string(3101);
        }
        f.answer = ui.msgbox(&f.msg_text, 8227, &s(__content.string(3102)));
        if f.answer == 2 {
            return;
        }
        if f.answer != 7 {
            break;
        }
    }
    if Ext::from_f64(0.0) < f.payoff_request {
        if (cast::<Ext, _>(f.issue_amount)) < f.payoff_request {
            f.payoff_request = cast::<Ext, _>(f.issue_amount);
        }
        g.bank_debt[id] = g.bank_debt[id] - f.payoff_request;
        if g.bank_debt[id] < Ext::from_f64(1.0) && Ext::from_f64(0.0) < f.payoff_request {
            g.bank_debt[id] = Ext::from_f64(0.0);
        }
        g.cash[id] = g.cash[id] - f.payoff_request;
        let lender = g.lender_id;
        g.cash[lender] = g.cash[lender] + f.payoff_request;
        if Ext::from_f64(1.0) < f.payoff_request {
            g.news_msg = g.ent_symbol[id].clone() + __content.string(3103) + &g.ent_name[lender] + " (" + &g.ent_symbol[lender]
                + __content.string(3104) + &cur_sym(g) + &fmt_trim(f.payoff_request, b"############") + " " + &g.million_prefix_lc
                + "il.";
            post_news(g, ui, 0, 0);
        }
    }
    if f.conv_issued == 0 {
        g.tmp_ext1 = cast::<Ext, _>(f.issue_amount);
        issue_bonds(g, ui, rng, id, f.coupon_rate, f.term_years + g.cur_year);
    }
    if f.prepay_penalty_flag == 1 {
        let penalty = sf(0.02f32) * f.payoff_request;
        let lender = g.lender_id;
        g.cash[id] = g.cash[id] - penalty;
        g.cash[lender] = g.cash[lender] + penalty;
        g.accrued_pl[id] = g.accrued_pl[id] - penalty;
        g.accrued_pl[lender] = g.accrued_pl[lender] + penalty;
    }
    if f.etf_issuer == 0 {
        calc_projected_earnings(g, ui, rng, id);
    } else {
        calc_entity_financials(g, ui, rng, id);
    }
    if f.conv_issued == 0 {
        f.msg_text = s(__content.string(3105)) + "\r" + "\r" + &f.issuer_name + __content.string(3106)
            + &cur_sym(g) + &fmt_trim(cast::<Ext, _>(f.issue_amount), b"* ###,") + " " + &g.million_prefix_lc + "illion"
            + &g.currency_suffix + __content.string(3107) + &fmt_trim(f.coupon_rate, b"##.00") + __content.string(3108)
            + &str_s(cast::<Ext, _>(f.term_years + g.cur_year)) + __content.string(3109) + "\"" + &g.credit_rating_str + "." + "\"" + " (";
        f.aux_text = s("Junk)");
        if 6 < g.credit_rating_num[id] {
            f.aux_text = s(__content.string(3110));
        }
        f.msg_text = f.msg_text.clone() + &f.aux_text;
        if Ext::from_f64(0.0) < f.payoff_request {
            f.msg_text = f.msg_text.clone() + "\r" + "\r"
                + __content.string(3111) + "of "
                + &cur_sym(g) + &fmt_trim(f.payoff_request, b"* ###,.00") + " " + &g.million_prefix_lc + "illion"
                + &g.currency_suffix + ". ";
            if f.prepay_penalty_flag == 1 {
                f.msg_text = f.msg_text.clone() + __content.string(3112);
            }
        }
        ui.msgbox(&f.msg_text, 8256, &s(__content.string(3113)));
        if 1580 < f.etf_issuer && 0 < g.fund_bonus_year[id] {
            g.fund_bonus_year[id] = g.cur_year;
        }
    }
    g.turn_action_count = g.turn_action_count + 1;
    if f.etf_issuer == 0 {
        g.player_recent_bond_issues[g.cur_player] = g.player_recent_bond_issues[g.cur_player] + 1;
    }
    // stageLabel = "PostTrans"
    pack_holding_arrays(g, ui);
    if f.etf_issuer == 0 {
        g.player_recent_corp = cast::<i16, _>(id);
    }
    if g.rand_seed % 2 == 1 || 1580 < f.etf_issuer {
        return;
    }
    loop {
        if 49 < f.attempts {
            return;
        }
        f.attempts = f.attempts + 1;
        f.institution_id = rng.rnd_range(130, 11);
        if f.attempts < 30 {
            f.placement_pct = cast::<i64, _>(rng.rnd_range(41, 21));
        } else {
            f.placement_pct = f.placement_pct - 1;
        }
        if f.placement_pct < 5 {
            return;
        }
        if g.entity_status[f.institution_id] == 1 && get_controller_type(g, f.institution_id) != 1 {
            if issue_bonds_menu_place_with_institution(g, ui, rng, &mut f) {
                return;
            }
        }
    }
}

/// Port of IssueBondsMenu_RateQuote (0x5a6962).
/// GOSUB of `IssueBondsMenu`: shows the straight-bond coupon for the default maturity (10 years, 5 for
/// the leveraged index funds 1584/1585) and lets the player try other terms (3..20 years, 3..5 for those
/// funds): -0.05 per year under 10 (-0.05 more under 7, -0.1 more under 5 above 5%), +0.05 per year over
/// 10 (+0.05 over 15, +0.1 over 17 below 12%); fund terms 4/3: -0.1/-0.25. Sets `couponRate` and
/// `termYears`; `baseRate` keeps the default quote. Returns `true` when the player cancels (exit of
/// `IssueBondsMenu`).
fn issue_bonds_menu_rate_quote(g: &mut Globals, ui: &mut dyn Ui, f: &mut IssueFrame) -> bool {
    let __content = crate::content::current();
    f.default_years = 10;
    if f.etf_issuer == 1584 || f.etf_issuer == 1585 {
        f.default_years = 5;
    }
    f.base_rate = f.coupon_rate;
    f.quoted_rate = f.coupon_rate;
    f.max_years = 20;
    if f.etf_issuer == 1584 || f.etf_issuer == 1585 {
        f.max_years = 5;
    }
    f.aux_text = fmt_trim(cast::<Ext, _>(f.issue_amount), b"* ###,");
    f.msg_text = s("If ") + &f.issuer_name + __content.string(3114) + &cur_sym(g) + &f.aux_text + " " + &g.million_prefix_lc
        + __content.string(3115) + __content.string(3116) + &str_s(cast::<Ext, _>(f.default_years))
        + __content.string(3117) + &fmt_trim(f.base_rate, b"##.00")
        + __content.string(3118)
        + __content.string(3119) + "in"
        + &str_s(cast::<Ext, _>(3 + g.cur_year)) + ") to" + &str_s(cast::<Ext, _>(f.max_years)) + __content.string(3120)
        + &str_s(cast::<Ext, _>(f.max_years + g.cur_year)) + ")." + "\r" + "\r";
    if f.etf_issuer == 0 {
        if f.conv_notice_shown != 1 {
            f.msg_text = f.msg_text.clone()
                + __content.string(3121)
                + __content.string(3122) + "\r"
                + "\r";
        }
    } else if f.max_years == 5 {
        f.msg_text = f.msg_text.clone() + __content.string(3123) + &f.issuer_name
            + __content.string(3124) + __content.string(3125)
            + __content.string(3126) + "\r" + "\r";
    }
    f.msg_text = f.msg_text.clone() + __content.string(3127) + "THAN"
        + &str_s(cast::<Ext, _>(f.default_years)) + __content.string(3128);
    let title = s(__content.string(3129)) + &str_s(cast::<Ext, _>(f.default_years + g.cur_year)) + "?";
    f.answer = ui.msgbox(&f.msg_text, 8228, &title);
    if f.answer == 7 {
        f.term_years = f.default_years;
        f.coupon_rate = f.base_rate;
        return false;
    }
    loop {
        f.msg_text = s(__content.string(3130)) + &str_s(cast::<Ext, _>(f.max_years)) + __content.string(3131);
        f.years_entry = 0;
        while f.years_entry < 3 || f.max_years < f.years_entry {
            let default = trim(str_s(cast::<Ext, _>(f.default_years)).as_bytes()) + __content.string(3132);
            f.input_text = ui.inputbox(&f.msg_text, &s(__content.string(3133)), &default);
            if f.input_text.is_empty() || val(f.input_text.as_bytes()) <= Ext::from_f64(0.0) {
                f.term_years = f.default_years;
                return false;
            }
            f.years_entry = to_long(val(f.input_text.as_bytes()));
            if 2 < f.years_entry && f.years_entry <= f.max_years {
                break;
            }
            quick(g, ui,
                s(__content.string(3134)) + &str_s(cast::<Ext, _>(f.max_years)),
            );
        }
        let y = f.years_entry;
        if f.etf_issuer == 1584 || f.etf_issuer == 1585 {
            if y == 5 {
                quick(g, ui, s(__content.string(3135)));
                return false;
            }
            if y == 4 {
                f.quoted_rate = f.base_rate - sf(0.1f32);
            }
            if y == 3 {
                f.quoted_rate = f.base_rate - Ext::from_f64(0.25);
            }
        } else if y < 10 {
            f.quoted_rate = f.base_rate - cast::<Ext, _>(10 - y) * sf(0.05f32);
            if y < 7 {
                f.quoted_rate = f.quoted_rate - sf(0.05f32);
            }
            if y < 5 && Ext::from_f64(5.0) < f.quoted_rate {
                f.quoted_rate = f.quoted_rate - sf(0.1f32);
            }
        } else {
            if y < 11 {
                quick(g, ui, s(__content.string(3135)));
                return false;
            }
            f.quoted_rate = cast::<Ext, _>(y - 10) * sf(0.05f32) + f.base_rate;
            if 15 < y {
                f.quoted_rate = f.quoted_rate + sf(0.05f32);
            }
            if 17 < y && f.quoted_rate < Ext::from_f64(12.0) {
                f.quoted_rate = f.quoted_rate + sf(0.1f32);
            }
        }
        f.msg_text = s(__content.string(3136)) + &f.issuer_name + __content.string(3137) + &str_s(cast::<Ext, _>(y))
            + __content.string(3138) + __content.string(3139)
            + &fmt_trim(f.quoted_rate, b"##.00") + "%." + "\r" + "\r" + "Is the " + &fmt_trim(f.quoted_rate, b"##.00")
            + __content.string(3140) + &str_s(cast::<Ext, _>(y)) + __content.string(3141) + __content.string(3142);
        if f.etf_issuer == 0 {
            f.msg_text = f.msg_text.clone() + __content.string(3143);
        }
        f.msg_text = f.msg_text.clone() + "." + "\r" + "\r"
            + __content.string(3144)
            + __content.string(3145);
        f.answer = ui.msgbox(&f.msg_text, 8227, &s(__content.string(3146)));
        if f.answer == 2 {
            return true;
        }
        if f.answer != 7 {
            f.term_years = y;
            f.coupon_rate = f.quoted_rate;
            return false;
        }
    }
}

/// Port of IssueBondsMenu_PlaceWithInstitution (0x5a67e3).
/// GOSUB of `IssueBondsMenu`: while the whole new issue is still held by the public, bank/insurer
/// `institutionId` buys `placementPct`% of it (`BuyCorpBonds`, mode 2) if its liquid funds exceed
/// `placementPct x 1.3%` of the public bonds and it holds fewer than 5 bond issues; posts the news.
/// Returns `true` (exit of `IssueBondsMenu`) when the issue is no longer entirely held by the public,
/// i.e. on the call after a placement.
fn issue_bonds_menu_place_with_institution(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut IssueFrame) -> bool {
    let __content = crate::content::current();
    calc_entity_financials(g, ui, rng, f.issuer_id);
    if g.bonds_issued[f.issuer_id] != g.bonds_public_held {
        return true;
    }
    f.payoff_amt = to_quad(cast::<Ext, _>(g.bonds_public_held) * (cast::<Ext, _>(f.placement_pct) * sf(0.013f32)));
    calc_entity_financials(g, ui, rng, f.institution_id);
    if (cast::<Ext, _>(f.payoff_amt)) < g.liquid_funds && g.hold_bond_id[(0, f.institution_id)] < 5 {
        g.tmp_ext1 = cast::<Ext, _>(f.placement_pct) * (cast::<Ext, _>(g.bonds_issued[f.issuer_id]) * sf(0.01f32));
        g.tmp_ext2 = Ext::from_f64(2.0);
        buy_corp_bonds(g, ui, rng, f.institution_id, f.issuer_id);
        g.news_msg = s(__content.string(3147)) + &g.ent_symbol[f.issuer_id] + __content.string(3148)
            + &g.ent_symbol[f.institution_id];
        post_news(g, ui, f.institution_id, f.issuer_id);
    }
    false
}

/// Port of MenuCmd_PlaceBondsWithBank (0x556858).
/// GOSUB of `ExecuteMenuCommand` after the player traded bonds of `PlayerRecentCorp`: half of the time
/// (and only half of those for a company with negative net worth) a random bank/insurer (11..130)
/// controlled by a company, that already holds the bonds and has liquid funds of 35% of the public
/// bonds (13% after 120 tries), buys up to 90% of its liquid funds of the publicly held bonds
/// (`BuyCorpBonds`, mode 2). Returns `true` when no buyer was found after 150 tries: the original then
/// jumps to the exit of `ExecuteMenuCommand` (`ExecuteMenuCommand_ExitSub`), so the caller must return
/// (BUGS.md #303).
pub fn menu_cmd_place_bonds_with_bank(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> bool {
    if Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
        return false;
    }
    let bond_issuer_id = cast::<i32, _>(g.player_recent_corp);
    calc_entity_financials(g, ui, rng, bond_issuer_id);
    if 0 < g.bonds_public_held {
        if g.net_worth < Ext::from_f64(0.0) && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(2.0) {
            return false;
        }
        let mut bond_amount = g.bonds_public_held;
        let mut buyer_id = 0;
        let mut count = 0;
        while buyer_id == 0 {
            count = count + 1;
            buyer_id = to_long(rng.rnd() * Ext::from_f64(119.0) + Ext::from_f64(11.0));
            if g.corp_controller[buyer_id] < 11 || g.entity_status[buyer_id] != 1 {
                buyer_id = 0;
            } else if find_bond_holding_slot(g, buyer_id, bond_issuer_id) == 0 {
                buyer_id = 0;
            } else {
                calc_entity_financials(g, ui, rng, buyer_id);
                if g.liquid_funds < cast::<Ext, _>(bond_amount) * sf(0.35f32) && count < 121 {
                    buyer_id = 0;
                }
                if 120 < count && g.liquid_funds < cast::<Ext, _>(bond_amount) * sf(0.13f32) {
                    buyer_id = 0;
                }
            }
            if 150 < count && buyer_id == 0 {
                buyer_id = -1;
            }
        }
        if buyer_id < 11 {
            execute_menu_command_exit_sub();
            return true;
        }
        if (cast::<Ext, _>(bond_amount)) > g.liquid_funds * sf(0.9f32) {
            bond_amount = to_quad(g.liquid_funds * sf(0.9f32));
        }
        g.tmp_ext1 = cast::<Ext, _>(bond_amount);
        g.tmp_ext2 = Ext::from_f64(2.0);
        buy_corp_bonds(g, ui, rng, buyer_id, bond_issuer_id);
    }
    false
}
