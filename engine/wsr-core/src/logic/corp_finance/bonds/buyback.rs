//! `BondBuybackMenu` (0x5a0aa1).
use crate::pb::real::{Ext, cast};


use super::super::prelude::*;
use super::{cur_sym, quick, s, val_removed};
use crate::logic::trading::get_selected_player_etf;
use crate::logic::cashflow::msg_corp_only_function;
use crate::logic::earnings::{balance_cash_t_bills, calc_accrued_bond_interest, calc_projected_earnings};
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::describe_defunct_entity;
use crate::logic::reports_corp::calc_bond_call_price_pct;
use crate::logic::sim_entity::bonds::{bond_buyback, call_bonds, convert_or_redeem_convertibles};
use crate::logic::valuation::calc_stock_price;

/// Port of BondBuybackMenu (0x5a0aa1).
/// The player's "call / buy back bonds" action for `ActiveEntity` (or a managed ETF, see the PORT-TODO
/// of `GetSelectedPlayerEtf`); `must_retire_first = 1` when called from `IssueBondsMenu` (existing bonds
/// must be retired before a new issue; the explanations are prefixed). Convertibles whose conversion
/// value exceeds the call price may [runtime resource wording] (`ConvertOrRedeemConvertibles`). Requires
/// liquid funds, and a credit rating that allows it (rating 7+, or 5+/2+ without bank debt for
/// non-financials, or a deep discount under 80); otherwise explains the prohibition. YES calls a
/// percentage of the issue at the call price (`CallBonds`; refused below the market when the player's
/// companies hold the bonds, or with bank debt), NO buys back publicly traded bonds (`BondBuyback`
/// mode 0, forced) or, without public bonds, bonds of current holders (the player, uncontrolled or
/// computer-controlled companies; at market + 5 points from others): pays price and accrued interest,
/// books the holder's gain, borrows any shortfall from the lending bank, retires the face, re-values
/// the issuer and posts news (up to 5 headlines).
pub fn bond_buyback_menu(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, must_retire_first: i32, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut issuer_id = g.active_entity;
    let mut issuer_name = g.active_entity_name.clone();
    let mut not_callable = 0;
    let mut is_convertible = 0;
    let mut unavail_count = 0;
    let mut news_count = 0;
    let mut answer;
    let mut msg_text: PbStr;
    let mut note_text: PbStr;
    let mut title_text: PbStr;
    let etf = get_selected_player_etf(g, ui, rng);
    if 1580 < etf {
        let mut skip_etf = false;
        if g.bonds_issued[etf] == 0 {
            if g.cur_player != g.etf_no_bonds_warn_plr || g.cur_player == g.active_entity {
                g.etf_no_bonds_warn_plr = g.cur_player;
                quick(g, ui, g.ent_name[etf].clone() + __content.string(3149));
            }
            if g.active_entity < 11 {
                return;
            }
            skip_etf = true;
        }
        if !skip_etf {
            note_text = g.ent_name[etf].clone() + " (" + &g.ent_symbol[etf] + __content.string(3017)
                + __content.string(3018) + &g.ent_name[g.fund_adviser[etf]] + ". "
                + __content.string(3150) + "\r" + "\r"
                + __content.string(3020) + &g.ent_name[etf] + __content.string(3151) + "\r" + "\r"
                + __content.string(3022);
            if g.active_entity < 11 || g.bonds_issued[g.active_entity] <= 0 {
                note_text = note_text + __content.string(3152);
            } else {
                note_text = note_text + __content.string(3024) + &g.active_entity_name + __content.string(3153);
            }
            answer = ui.msgbox(&note_text, 8483, &s(__content.string(3154)));
            if answer == 2 {
                return;
            }
            if answer == 7 {
                if !(9 < g.active_entity && g.bonds_issued[g.active_entity] != 0) {
                    return;
                }
            } else {
                issuer_id = etf;
                issuer_name = g.ent_name[etf].clone();
            }
        }
    }
    // LAB_005a0ee4
    if issuer_id < 11 {
        msg_corp_only_function(g, ui);
        return;
    }
    if g.entity_status[issuer_id] == 2 {
        describe_defunct_entity(g, ui, issuer_id);
    }
    if g.entity_status[issuer_id] != 1 {
        return;
    }
    let id = issuer_id;
    if g.bonds_issued[id] <= 0 {
        quick(g, ui, s(__content.string(3155)) + &issuer_name + __content.string(3156));
        return;
    }
    calc_entity_financials(g, ui, rng, id);
    let mut coupon_text = right(g.bond_issue_info[id].as_bytes(), 5);
    if mid(coupon_text.as_bytes(), 1, 1) == "0" {
        coupon_text = right(coupon_text.as_bytes(), 4);
    }
    let maturity_year = to_long(val(mid(g.bond_issue_info[id].as_bytes(), 1, 4).as_bytes()));
    let years_to_maturity = maturity_year - g.cur_year;
    if 9 < years_to_maturity {
        not_callable = 1;
    }
    let call_price_pct = calc_bond_call_price_pct(years_to_maturity);
    let mut max_face: i64 = g.bonds_issued[id];
    let price_text = fmt_trim_cur(g.bond_price[id], b"###.00") + __content.string(3157);
    if must_retire_first == 1 {
        msg_text = issuer_name.clone() + __content.string(3158)
            + __content.string(3159) + &coupon_text + __content.string(3160) + &price_text + "\r" + "\r";
    } else {
        let trading_word = if 0 < g.bonds_public_held { s("trading") } else { s("priced") };
        msg_text = issuer_name.clone() + "'s " + &coupon_text + __content.string(3161) + &trading_word + " at "
            + &price_text + __content.string(3162) + &cur_sym(g)
            + &fmt_trim(cast::<Ext, _>(g.bonds_issued[id]), b"* ###,") + " " + &g.million_prefix_lc + "illion"
            + &g.currency_suffix + "." + "\r" + "\r";
    }
    if 0 < instr(1, g.bond_issue_info[id].as_bytes(), b"cv") {
        let conv_value = val(parse_any(g.bond_issue_info[id].as_bytes(), b"|", 2).as_bytes());
        if conv_value < g.market_cap[id] && sf(0.01f32) * (cast::<Ext, _>(call_price_pct)) < g.market_cap[id] / conv_value {
            let conv_shares = Ext::from_f64(1000.0) / (conv_value / g.shares_outstanding[id]);
            is_convertible = 1;
            if Ext::from_f64(1000.0) < g.bond_price[id].get() {
                not_callable = 0;
            }
            if not_callable == 0 {
                let mut conv_msg = PbStr::new();
                if must_retire_first == 1 {
                    conv_msg = msg_text.clone();
                }
                conv_msg = conv_msg + &issuer_name
                    + __content.string(3163) + __content.string(3164)
                    + &fmt_trim(conv_value / g.shares_outstanding[id], b"* ###,.00") + __content.string(3165)
                    + __content.string(3166)
                    + __content.string(3167) + &fmt_trim(conv_shares, b"* ###,.000") + " "
                    + __content.string(3168) + &cur_sym(g) + "1,000" + &g.currency_suffix + __content.string(3169) + "\r" + "\r"
                    + __content.string(3170)
                    + __content.string(3171) + "\r" + "\r"
                    + __content.string(3172)
                    + &str_s(cast::<Ext, _>(call_price_pct)) + __content.string(3173)
                    + "\r" + "\r" + __content.string(3174);
                answer = ui.msgbox(&conv_msg, 8484, &s(__content.string(3175)));
                if answer == 6 {
                    if convert_or_redeem_convertibles(g, ui, rng, id, 0, 0) == 1 {
                        msg_text = s(__content.string(3176)) + "\r" + "\r" + &issuer_name
                            + __content.string(3177)
                            + __content.string(3178)
                            + __content.string(3179) + &g.ent_symbol[id] + ".";
                        ui.msgbox(&msg_text, 8256, &s(__content.string(3180)));
                        return;
                    }
                    quick(g, ui, s(__content.string(3181)));
                }
                if answer == 2 {
                    return;
                }
                if answer == 7 {
                    let mut m = s(__content.string(3182))
                        + &trim(str_s(cast::<Ext, _>(call_price_pct)).as_bytes()) + __content.string(3183)
                        + __content.string(3184)
                        + __content.string(3185);
                    show_quick_message_ref(g, ui, &mut m);
                }
            }
        }
    }
    if g.liquid_funds <= Ext::from_f64(0.0) {
        note_text = s(__content.string(3186)) + __content.string(3187);
        if g.industry_num[id] == 71 {
            note_text = replace(note_text.as_bytes(), b"company", __content.bytes(3188));
        }
        if is_convertible == 0 {
            note_text = replace(note_text.as_bytes(), b"back", __content.bytes(3189));
        }
        if must_retire_first == 1 {
            msg_text = msg_text + &note_text;
        } else {
            msg_text = note_text;
        }
        ui.msgbox(&msg_text, 8208, &s(__content.string(1928)));
        return;
    }
    let rating = g.credit_rating_num[id];
    let ind = g.industry_num[id];
    let mut allowed = 6 < rating || (4 < rating && 2 < ind) || (1 < rating && 2 < ind && g.bank_debt[id] == Ext::from_f64(0.0));
    if !allowed && 2 < ind && g.bank_debt[id] == Ext::from_f64(0.0) && val(price_text.as_bytes()) < Ext::from_f64(80.0) {
        quick(g, ui,
            s(__content.string(3190))
                + __content.string(3191)
                + __content.string(3192),
        );
        allowed = true;
    }
    if !allowed {
        if g.credit_rating_str == "D" && Ext::from_f64(0.0) < g.bank_debt[id] && !(g.industry_num[id] < 2) {
            title_text = s(__content.string(3193));
            if g.credit_rating_num[id] == 0 {
                title_text = s(__content.string(3194));
            }
            note_text = s(__content.string(1927)) + &title_text + __content.string(3195)
                + __content.string(3196) + __content.string(3197);
        } else if g.industry_num[id] < 3 {
            title_text = s(__content.string(3198));
            if g.industry_num[id] == 2 {
                title_text = s(__content.string(3199));
            }
            note_text = s(__content.string(3200))
                + __content.string(3201) + "\r" + "\r"
                + __content.string(3202) + __content.string(3203) + &title_text
                + __content.string(3204);
        } else {
            note_text = s(__content.string(3205)) + &g.credit_rating_str + __content.string(3206)
                + __content.string(3207)
                + __content.string(3208)
                + __content.string(3209);
        }
        if must_retire_first == 1 {
            msg_text = msg_text + &note_text;
        } else {
            msg_text = note_text;
        }
        ui.msgbox(&msg_text, 8208, &s(__content.string(3210)));
        return;
    }
    // LAB_005a1fc3
    if g.bonds_public_held <= 0 {
        msg_text = msg_text + __content.string(3211) + __content.string(3212);
        if not_callable == 1 {
            msg_text = msg_text + __content.string(3213);
        }
        msg_text = msg_text + __content.string(3214) + &g.ent_symbol[id] + __content.string(3215);
        if not_callable == 0 && is_convertible == 0 {
            msg_text = msg_text + __content.string(3216)
                + &str_s(cast::<Ext, _>(call_price_pct)) + __content.string(3217);
        }
        msg_text = msg_text + __content.string(3218) + __content.string(3219) + "\r" + "\r";
    } else {
        msg_text = msg_text + __content.string(3220)
            + __content.string(3221);
        if not_callable == 0 && is_convertible == 0 {
            msg_text = msg_text + __content.string(3222)
                + &str_s(cast::<Ext, _>(call_price_pct)) + __content.string(3223);
        } else if not_callable == 1 {
            msg_text = msg_text + __content.string(3224) + &str_s(cast::<Ext, _>(maturity_year - 9));
            if is_convertible == 1 {
                msg_text = msg_text + __content.string(3225)
                    + __content.string(3226);
            }
        }
        msg_text = msg_text + "." + "\r" + "\r";
    }
    msg_text = msg_text + __content.string(3227);
    if not_callable == 0 && is_convertible == 0 {
        msg_text = msg_text + __content.string(3228);
    } else {
        msg_text = msg_text + __content.string(3229);
    }
    msg_text = msg_text + __content.string(3230);
    if is_convertible == 0 {
        msg_text = msg_text + "\r" + "\r" + __content.string(3231);
        if not_callable == 0 {
            msg_text = msg_text + __content.string(3232) + &str_s(cast::<Ext, _>(call_price_pct))
                + __content.string(3233) + "\r" + "\r" + __content.string(3234);
        }
        msg_text = msg_text + __content.string(3235);
        if g.bonds_public_held <= 0 {
            msg_text = msg_text + __content.string(3236);
        } else {
            msg_text = msg_text + __content.string(3237) + "\r" + "\r" + &g.currency_symbol
                + &fmt_trim(cast::<Ext, _>(g.bonds_public_held), b"##########") + " " + &g.million_prefix_uc
                + __content.string(3238);
        }
    }
    if g.liquid_funds <= Ext::from_f64(0.0) {
        msg_text = msg_text + __content.string(3239) + &g.ent_symbol[id] + __content.string(3240);
    }
    let title = s(__content.string(3241)) + &g.ent_name[id];
    answer = ui.msgbox(&msg_text, 8227, &title);
    if answer == 2 {
        return;
    }
    if answer == 7 {
        if not_callable == 1 || is_convertible == 1 {
            return;
        }
    } else if not_callable == 1 || is_convertible == 1 {
        answer = 7;
    }
    if answer == 6 && not_callable == 0 && is_convertible == 0 {
        let mut call_allowed = 1;
        if g.bond_price[id].get() < cast::<Ext, _>(call_price_pct) - Ext::from_f64(3.0) {
            'holders: for j in 1..=1583 {
                if (j <= g.num_players || 10 < j) && id != j && g.cur_player == g.corp_controller[j] {
                    for slot in 1..=15 {
                        if id == g.hold_bond_id[(slot, j)] {
                            call_allowed = 0;
                            if g.cur_player == j {
                                call_allowed = -1;
                                break;
                            }
                        }
                    }
                    if call_allowed < 1 {
                        break 'holders;
                    }
                }
            }
        }
        if call_allowed < 1 {
            note_text = s(__content.string(3242));
            if Ext::from_f64(0.0) < g.bank_debt[id] {
                note_text = s(__content.string(3243)) + __content.string(3244);
                call_allowed = -2;
            }
            title_text = s(__content.string(3245));
            note_text = note_text + "\r" + "\r";
            msg_text = s(__content.string(3246));
            if call_allowed == -2 {
                msg_text = msg_text + ".";
            }
            if call_allowed == 0 || call_allowed == -1 {
                msg_text = msg_text + __content.string(3247);
                if call_allowed == 0 {
                    msg_text = msg_text + __content.string(3248);
                }
                msg_text = msg_text + &g.cur_player_name + __content.string(3249) + &issuer_name + __content.string(3250)
                    + __content.string(3251)
                    + __content.string(3252);
                title_text = s(__content.string(3253));
            }
            ui.msgbox(&(note_text + &msg_text), 8208, &title_text);
        } else {
            let face_to_buy;
            loop {
                msg_text = issuer_name.clone() + " has " + &fmt_trim(cast::<Ext, _>(max_face), b"* ###,") + " " + &g.million_prefix_lc
                    + __content.string(3254) + &fmt_trim(g.liquid_funds, b"* ###,.00") + " "
                    + &g.million_prefix_lc + __content.string(3255) + __content.string(3256)
                    + &str_s(cast::<Ext, _>(call_price_pct)) + __content.string(3257)
                    + __content.string(3258);
                let title = s(__content.string(3259)) + &g.ent_symbol[id];
                let input_text = ui.inputbox(&msg_text, &title, &s("100"));
                let f = to_quad(val_removed(&input_text, b"$ ,"));
                if f <= 0 {
                    return;
                }
                if f < 101 {
                    face_to_buy = f;
                    break;
                }
            }
            g.tmp_ext1 = cast::<Ext, _>(face_to_buy);
            g.tmp_ext2 = Ext::from_f64(0.0);
            call_bonds(g, ui, rng, id);
        }
    } else if answer == 7 {
        if g.bonds_public_held <= 0 {
            for holder_id in 1..=1583 {
                if !((g.cur_player == holder_id || 10 < holder_id) && g.entity_status[holder_id] == 1 && id != holder_id) {
                    continue;
                }
                for j in 1..=15 {
                    if id != g.hold_bond_id[(j, holder_id)] {
                        continue;
                    }
                    let mut ctrl_id = 0;
                    if 10 < holder_id {
                        ctrl_id = g.corp_controller[holder_id];
                    }
                    if !(ctrl_id < 1 || 10 < ctrl_id || g.cur_player == ctrl_id) {
                        unavail_count = unavail_count + 1;
                        continue;
                    }
                    unavail_count = -999;
                    if g.cur_player == holder_id || g.cur_player == ctrl_id {
                        note_text = PbStr::new();
                    } else {
                        note_text = s(__content.string(3260));
                    }
                    let holder_face = g.hold_bond_face[(j, holder_id)];
                    if holder_face == 0 {
                        g.hold_bond_id[(j, holder_id)] = 0;
                        g.bond_hold_cost[(j, holder_id)] = Ext::from_f64(0.0);
                        continue;
                    }
                    msg_text = issuer_name.clone() + __content.string(3261) + &g.currency_symbol
                        + &fmt_trim(cast::<Ext, _>(holder_face), b"* ###,") + " " + &g.million_prefix_lc + __content.string(3262)
                        + &g.ent_name[holder_id] + ", at " + &note_text + __content.string(3263)
                        + &fmt_trim_cur(g.bond_price[id], b"* ###,.00") + "." + "\r" + "\r" + "Does " + &issuer_name
                        + __content.string(3264) + __content.string(3265) + &g.ent_name[holder_id] + "?";
                    answer = ui.msgbox(&msg_text, 8227, &s(__content.string(3266)));
                    if answer == 2 {
                        return;
                    }
                    if answer != 6 {
                        continue;
                    }
                    let mut cost = cast::<Ext, _>(holder_face) * (g.bond_price[id].get() * sf(0.01f32));
                    cost = cost + Ext::from_f64(1.0);
                    // (the 5-point premium is added for the player's own holdings here, see BUGS.md #302)
                    if g.cur_player == holder_id || g.cur_player == ctrl_id {
                        cost = cost + cast::<Ext, _>(holder_face) * sf(0.05f32);
                    }
                    if g.liquid_funds < cost {
                        max_face = to_quad(cast::<Ext, _>(holder_face) * (g.liquid_funds / cost));
                    } else {
                        max_face = holder_face;
                    }
                    msg_text = s(__content.string(3267)) + &g.ent_name[holder_id] + "? ("
                        + &issuer_name + __content.string(3268) + __content.string(3269) + &str_s(cast::<Ext, _>(max_face)) + " "
                        + &g.million_prefix_lc + "illion)";
                    let mut face_to_buy: i64 = 0;
                    while face_to_buy == 0 {
                        let title = s(__content.string(3270)) + &g.ent_symbol[id];
                        let input_text = ui.inputbox(&msg_text, &title, &str_s(cast::<Ext, _>(max_face)));
                        face_to_buy = to_quad(val_removed(&input_text, b"$ ,"));
                        if face_to_buy <= 0 {
                            return;
                        }
                        if max_face < face_to_buy {
                            face_to_buy = 0;
                        }
                    }
                    let mut cost = cast::<Ext, _>(face_to_buy) * (g.bond_price[id].get() * sf(0.01f32));
                    if 10 < holder_id && g.cur_player != ctrl_id {
                        cost = cost + cast::<Ext, _>(face_to_buy) * sf(0.05f32);
                    }
                    let seller_id = holder_id;
                    g.hold_bond_face[(j, seller_id)] = to_quad(cast::<Ext, _>(g.hold_bond_face[(j, holder_id)]) - cast::<Ext, _>(face_to_buy));
                    if g.hold_bond_face[(j, seller_id)] < 1 {
                        g.hold_bond_face[(j, seller_id)] = 0;
                        g.hold_bond_id[(j, seller_id)] = 0;
                    }
                    let mut sold_frac = Ext::from_f64(1.0);
                    if face_to_buy < holder_face {
                        sold_frac = cast::<Ext, _>(face_to_buy) / cast::<Ext, _>(holder_face);
                    }
                    let gain_on_sale = cost - sold_frac * g.bond_hold_cost[(j, holder_id)];
                    if holder_id < 11 {
                        g.player_cap_gains[seller_id] = g.player_cap_gains[seller_id] + gain_on_sale;
                    } else if holder_id < 86 {
                        g.accrued_pl[seller_id] = g.accrued_pl[seller_id] + gain_on_sale;
                    } else {
                        g.extraordinary_pl[seller_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[seller_id]) + gain_on_sale);
                    }
                    let cost_part = sold_frac * g.bond_hold_cost[(j, holder_id)];
                    g.bond_hold_cost[(j, holder_id)] = g.bond_hold_cost[(j, holder_id)] - cost_part;
                    if g.hold_bond_face[(j, holder_id)] == 0 {
                        g.bond_hold_cost[(j, holder_id)] = Ext::from_f64(0.0);
                    }
                    g.cash[holder_id] = g.cash[holder_id] + cost;
                    g.cash[id] = g.cash[id] - cost;
                    let accrued_int = calc_accrued_bond_interest(g, face_to_buy, id);
                    g.cash[seller_id] = g.cash[seller_id] + accrued_int;
                    g.cash[id] = g.cash[id] - accrued_int;
                    g.accrued_pl[seller_id] = g.accrued_pl[seller_id] + accrued_int;
                    g.accrued_pl[id] = g.accrued_pl[id] - accrued_int;
                    balance_cash_t_bills(g, id);
                    if g.cash[id] < Ext::from_f64(0.0) {
                        let borrowed = -g.cash[id];
                        g.bank_debt[id] = g.bank_debt[id] + borrowed;
                        g.cash[id] = Ext::from_f64(0.0);
                        let bank = cast::<i32, _>(g.lending_bank[id]);
                        g.cash[bank] = g.cash[bank] - borrowed;
                    }
                    g.extraordinary_pl[id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[id]) + (cast::<Ext, _>(face_to_buy) - cost));
                    g.bonds_issued[id] = to_quad(cast::<Ext, _>(g.bonds_issued[id]) - cast::<Ext, _>(face_to_buy));
                    if g.bonds_issued[id] < 1 {
                        g.bonds_issued[id] = 0;
                        g.bond_issue_info[id] = PbStr::new();
                        g.bond_price[id] = Default::default();
                    }
                    g.turn_action_count = g.turn_action_count + 1;
                    if 10 < id {
                        if g.industry_num[id] == 71 && 0 < g.fund_bonus_year[id] {
                            g.fund_bonus_year[id] = g.cur_year;
                        }
                        calc_projected_earnings(g, ui, rng, id);
                        g.valuation_mode = 3;
                        calc_stock_price(g, ui, rng, id, time_secs);
                        g.news_msg = g.ent_symbol[id].clone() + __content.string(3271) + &g.ent_symbol[holder_id]
                            + " for " + &cur_sym(g) + &fmt_trim(cost, b"* ###,") + " " + &g.million_prefix_lc + "illion";
                        quick(g, ui,
                            g.news_msg.clone() + __content.string(3272) + &cur_sym(g)
                                + &fmt_trim(accrued_int, b"* ###,.00") + " " + &g.million_prefix_lc + "illion"
                                + &g.currency_suffix,
                        );
                        if news_count < 5 {
                            post_news(g, ui, 0, 0);
                        }
                        if g.liquid_funds < Ext::from_f64(1.0) {
                            return;
                        }
                        news_count = news_count + 1;
                    }
                }
            }
            if 0 < unavail_count {
                quick(g, ui,
                    s(__content.string(3273)) + &issuer_name
                        + __content.string(3274),
                );
            }
        } else {
            let issued = g.bonds_issued[id];
            let mut cost = cast::<Ext, _>(issued) * (g.bond_price[id].get() * sf(0.01f32));
            cost = cost * sf(1.01f32);
            if 1 < g.credit_rating_num[id] {
                cost = cost + calc_accrued_bond_interest(g, issued, id);
            }
            if g.liquid_funds < cost {
                max_face = to_quad(cast::<Ext, _>(issued) * (g.liquid_funds / cost));
            } else {
                max_face = issued;
            }
            if max_face < 1 {
                quick(g, ui, issuer_name.clone() + __content.string(3275));
            } else {
                if g.bonds_public_held < max_face {
                    max_face = g.bonds_public_held;
                }
                msg_text = s(__content.string(3276)) + &issuer_name + __content.string(3268)
                    + __content.string(3269) + &str_s(cast::<Ext, _>(max_face)) + " " + &g.million_prefix_lc + "illion)";
                let mut face_to_buy: i64 = 0;
                while face_to_buy == 0 {
                    let title = s(__content.string(3270)) + &g.ent_symbol[id];
                    let input_text = ui.inputbox(&msg_text, &title, &str_s(cast::<Ext, _>(max_face)));
                    face_to_buy = to_quad(val_removed(&input_text, b"$ ,"));
                    if face_to_buy <= 0 {
                        return;
                    }
                    if max_face < face_to_buy {
                        face_to_buy = 0;
                    }
                }
                g.tmp_ext1 = cast::<Ext, _>(face_to_buy);
                g.tmp_ext2 = Ext::from_f64(0.0);
                g.tmp_ext3 = -Ext::from_f64(1.0);
                bond_buyback(g, ui, rng, id);
            }
        }
    }
}

/// `ShowQuickMessage` of a local string (BYREF: the local gets the `@@`/`##` encoding).
fn show_quick_message_ref(g: &mut Globals, ui: &mut dyn Ui, text: &mut PbStr) {
    crate::logic::report::show_quick_message(g, ui, text);
}

/// `RTRIM$(LTRIM$(FORMAT$(cur, mask)))` of a CURRENCY value (formatted from its exact 80-bit value).
fn fmt_trim_cur(x: crate::pb::numeric::Cur, mask: &[u8]) -> PbStr {
    trim(&crate::pb::numeric::format_ext(x.get_ext(), mask))
}
