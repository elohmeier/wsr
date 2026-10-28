//! `PickConvertiblePrice` (0x688014) and `StockSplit` (0x5bc3ff).
use crate::pb::real::{Ext, cast};


use super::super::prelude::*;
use super::{cur_sym, quick, s, val_removed};
use crate::logic::report::show_quick_message;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::{Cur, Cux};

/// Port of PickConvertiblePrice (0x688014).
/// Issue of `bond_amount` convertible bonds by `ActiveEntity` at a conversion price the player picks
/// within 10..25% above the share price (`CURRENCYX`, accepted 0.99 x min .. 1.01 x max): the coupon is
/// `base_rate x (0.65 + premium% - 10%)` rounded to 1/20 (less 2 x rating/20 for ratings 7+ above 2.5%),
/// at least 1.5%; maturity `req_maturity` (at most `CurYear + 10`). `TmpExt1` (bank-loan paydown) and
/// `TmpExt2` (prepayment penalty flag) only feed the texts and are cleared. On YES: cash and
/// `BondsIssued` = the amount, `BondIssueInfo = "<year>cv|<conversion cap>|@<coupon>"`, public notice;
/// returns 1. Cancel / declining returns 0.
pub fn pick_convertible_price(g: &mut Globals, ui: &mut dyn Ui, bond_amount: i64, base_rate: Ext, req_maturity: i32) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let corp_id = g.active_entity;
    let loan_paydown = g.tmp_ext1;
    let prepay_penalty = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let mut maturity_year = req_maturity;
    if g.cur_year + 10 < maturity_year {
        maturity_year = g.cur_year + 10;
    }
    let mut share_price = Cux(0);
    share_price.set(g.market_cap[corp_id] / g.shares_outstanding[corp_id]);
    let price_text = fmt_trim(share_price.get(), b"* ###.00");
    let mut max_price = Cux(0);
    max_price.set_ext(share_price.get_ext().fmul(Ext80::from_f32(1.25f32)));
    let mut min_price = Cux(0);
    min_price.set_ext(share_price.get_ext().fmul(Ext80::from_f32(1.1f32)));
    let mut msg_text = s(__content.string(3277)) + &g.active_entity_name + __content.string(3278)
        + "the " + &g.ent_symbol[corp_id] + __content.string(3279)
        + &price_text + __content.string(3280) + &fmt_trim(min_price.get(), b"* ###.00") + " and "
        + &fmt_trim(max_price.get(), b"* ###.00") + "." + "\r" + "\r"
        + __content.string(3281)
        + __content.string(3282)
        + __content.string(3283);
    ui.msgbox(&msg_text, 8256, &s(__content.string(3284)));
    loop {
        let mut coupon = Cur(0);
        let mut conv_price = Cux(0);
        loop {
            coupon.set(base_rate);
            msg_text = s(__content.string(3285))
                + &fmt_trim(min_price.get(), b"* ###.00") + " and " + &fmt_trim(max_price.get(), b"* ###.00")
                + __content.string(3286);
            let input_text = ui.inputbox(&msg_text, &s(__content.string(3287)), &PbStr::new());
            if input_text.is_empty() {
                return 0;
            }
            conv_price.set(val_removed(&input_text, b"$ ,"));
            let ok = conv_price.get_ext().cmp_val(max_price.get_ext().fmul(Ext80::from_f32(1.01f32))).is_le()
                && !conv_price.get_ext().cmp_val(min_price.get_ext().fmul(Ext80::from_f32(0.99f32))).is_lt();
            if ok {
                break;
            }
            quick(g, ui,
                s(__content.string(3288))
                    + __content.string(3289),
            );
        }
        let conv_price_text = fmt_trim(conv_price.get(), b"* ###.00");
        let premium_ratio = conv_price.get_ext().fdiv(share_price.get_ext()).fsub(Ext80::from_i64(1)).into_ext();
        let premium_pct = to_long(premium_ratio * Ext::from_f64(100.0) - Ext::from_f64(10.0));
        let premium_frac = Ext80::from_i64(cast::<i64, _>(premium_pct)).fmul(Ext80::from_f32(0.01f32)).into_ext();
        coupon.set_ext(Ext80::from_ext(premium_frac).fadd(Ext80::from_f32(0.65f32)).fmul(coupon.get_ext()));
        let mut rate_x20 = coupon.get_ext().fmul(Ext80::from_i64(20)).to_i32();
        if 6 < g.credit_rating_num[corp_id] && 50 < rate_x20 {
            rate_x20 = rate_x20 - g.credit_rating_num[corp_id] * 2;
        }
        coupon.set_ext(Ext80::from_i64(cast::<i64, _>(rate_x20)).fdiv(Ext80::from_i64(20)));
        if coupon.get() < Ext::from_f64(1.5) {
            coupon.set(Ext::from_f64(1.5));
        }
        let mut shares_per_bond = Cux(0);
        shares_per_bond.set_ext(Ext80::from_i64(1000).fdiv(conv_price.get_ext()));
        msg_text = g.active_entity_name.clone() + __content.string(3290) + &str_s(cast::<Ext, _>(maturity_year - g.cur_year))
            + __content.string(3291) + __content.string(3292) + &conv_price_text
            + __content.string(3293) + &fmt_trim(coupon.get(), b"##.00") + __content.string(3294) + "each "
            + &cur_sym(g) + "1,000" + &g.currency_suffix + __content.string(3295)
            + &fmt_trim(shares_per_bond.get(), b"* ###,.000") + __content.string(3296) + &g.ent_symbol[corp_id] + __content.string(3297)
            + &fmt_trim(shares_per_bond.get() * Ext::from_f64(1000.0), b"* ###,") + __content.string(3298) + &cur_sym(g) + __content.string(3299)
            + &g.currency_suffix + __content.string(3300) + "\r" + "\r";
        if Ext::from_f64(0.0) < loan_paydown {
            msg_text = msg_text + &cur_sym(g) + &fmt_trim(loan_paydown, b"* ###,.00") + " " + &g.million_prefix_lc
                + "illion" + &g.currency_suffix + __content.string(3301);
            if prepay_penalty == 1 {
                msg_text = msg_text + __content.string(3099);
            }
            msg_text = msg_text + "\r" + "\r";
        }
        msg_text = msg_text + __content.string(3302);
        let answer = ui.msgbox(&msg_text, 8227, &s(__content.string(3102)));
        if answer == 2 {
            return 0;
        }
        if answer != 7 {
            g.cash[corp_id] = g.cash[corp_id] + cast::<Ext, _>(bond_amount);
            g.bonds_issued[corp_id] = bond_amount;
            let coupon_text = fmt_trim(coupon.get(), b"0#.00");
            let cap = conv_price.get_ext().fmul(Ext80::from_ext(g.shares_outstanding[corp_id])).into_ext();
            g.bond_issue_info[corp_id] = trim(str_s(cast::<Ext, _>(maturity_year)).as_bytes()) + "cv|"
                + &fmt_trim(Ext::from(cap), b"* ###.00") + "|@" + &coupon_text;
            msg_text = s(__content.string(3105)) + "\r" + "\r" + &g.active_entity_name
                + __content.string(3106) + &cur_sym(g) + &fmt_trim(cast::<Ext, _>(bond_amount), b"* ###,") + " "
                + &g.million_prefix_lc + "illion" + &g.currency_suffix + __content.string(3107) + &fmt_trim(coupon.get(), b"##.00")
                + __content.string(3303) + &str_s(cast::<Ext, _>(maturity_year)) + __content.string(3109) + "\"" + &g.credit_rating_str
                + "." + "\"" + " (";
            let mut aux_text = s("Junk)");
            if 6 < g.credit_rating_num[g.active_entity] {
                aux_text = s(__content.string(3110));
            }
            msg_text = msg_text + &aux_text + "\r" + "\r" + "Each " + &cur_sym(g) + __content.string(3304)
                + &fmt_trim(shares_per_bond.get(), b"* ###,.000") + __content.string(3305) + "of " + &g.ent_symbol[corp_id]
                + __content.string(3306);
            if Ext::from_f64(0.0) < loan_paydown {
                aux_text = g.ent_name[cast::<i32, _>(g.lending_bank[corp_id])].clone();
                msg_text = msg_text + "\r" + "\r" + __content.string(3307)
                    + &aux_text + ", in " + __content.string(3308) + &cur_sym(g) + &fmt_trim(loan_paydown, b"* ###,.00") + " "
                    + &g.million_prefix_lc + "illion" + &g.currency_suffix + ". ";
                if prepay_penalty == 1 {
                    msg_text = msg_text + __content.string(3112);
                }
            }
            ui.msgbox(&msg_text, 8256, &s(__content.string(3113)));
            return 1;
        }
        msg_text = s(__content.string(3309));
        let answer = ui.msgbox(&msg_text, 8227, &s(__content.string(3310)));
        if answer != 6 {
            return 0;
        }
    }
}

/// Port of StockSplit (0x5bc3ff).
/// Stock split (`split_type = 1`, price at least 10) or reverse split (2, price at most 100) of
/// `ActiveEntity` (asks first when another company is selected): the player enters the factor (1..10);
/// the new price must stay within 3..1000 (only the bound in the split's direction is checked). Scales
/// `SharesOutstanding` and `DividendPerShare` and shows the new price.
pub fn stock_split(g: &mut Globals, ui: &mut dyn Ui, split_type: i32) {
    let __content = crate::content::current();
    if !(10 < g.active_entity && g.entity_status[g.active_entity] == 1) {
        return;
    }
    let i = g.active_entity;
    let mut default_text = PbStr::new();
    if split_type == 2 {
        default_text = s(__content.string(3311));
    }
    if g.active_entity != g.sel_entity && 10 < g.sel_entity {
        let msg_text = s(__content.string(3312)) + &g.ent_name[g.sel_entity] + "...!" + "\r" + "\r" + __content.string(3313)
            + &g.active_entity_name + __content.string(3314) + &default_text + __content.string(3315);
        if ui.msgbox(&msg_text, 8227, &g.app_title.clone()) != 6 {
            return;
        }
    }
    if g.shares_outstanding[i] <= Ext::from_f64(0.0) {
        g.shares_outstanding[i] = Ext::from_f64(1.0);
    }
    let shares = g.shares_outstanding[i];
    let mut msg_text;
    if split_type == 1 {
        if g.market_cap[i] / shares < Ext::from_f64(10.0) {
            let mut m = s(__content.string(3316)) + &g.active_entity_name + __content.string(3317);
            show_quick_message(g, ui, &mut m);
            return;
        }
        msg_text = s(__content.string(3318))
            + __content.string(3319);
        msg_text = msg_text + __content.string(3320);
    } else {
        if Ext::from_f64(100.0) < g.market_cap[i] / shares {
            let mut m = s(__content.string(3316)) + &g.active_entity_name + __content.string(3321);
            show_quick_message(g, ui, &mut m);
            return;
        }
        msg_text = s(__content.string(3322))
            + __content.string(3323);
        msg_text = msg_text + __content.string(3324);
    }
    default_text = s("2");
    if split_type == 2 {
        default_text = s("10");
    }
    let new_shares;
    loop {
        let mut title_prefix = PbStr::new();
        if split_type == 2 {
            title_prefix = s(__content.string(3325));
        }
        let title = title_prefix + __content.string(3326) + &g.active_entity_name;
        let answer = ui.inputbox(&msg_text, &title, &default_text);
        let factor = val_removed(&answer, b"$ ,%");
        if factor <= Ext::from_f64(0.0) {
            return;
        }
        let mut ns = factor * shares;
        if split_type == 2 {
            ns = shares / factor;
        }
        let x = g.market_cap[i] / ns;
        let mut err_text = PbStr::new();
        if factor < Ext::from_f64(1.0) || Ext::from_f64(10.0) < factor {
            err_text = s(__content.string(3327));
        }
        if err_text.is_empty() {
            if !(Ext::from_f64(3.0) <= x) && split_type == 1 {
                err_text = s("ERR");
            } else if Ext::from_f64(1000.0) < x && split_type == 2 {
                err_text = s("ERR");
            }
            if err_text == "ERR" {
                err_text = s(__content.string(3328)) + &fmt_trim(x, b"* ###,.00")
                    + __content.string(3329);
            }
        }
        if err_text.is_empty() {
            new_shares = ns;
            break;
        }
        show_quick_message(g, ui, &mut err_text);
    }
    let so = g.shares_outstanding[i];
    if so <= new_shares {
        if so < new_shares && Ext::from_f64(0.0) < so {
            g.dividend_per_share[i] = g.dividend_per_share[i] * (so / new_shares);
        }
    } else if Ext::from_f64(0.0) < new_shares {
        g.dividend_per_share[i] = g.dividend_per_share[i] * (so / new_shares);
    }
    g.shares_outstanding[i] = new_shares;
    quick(g, ui,
        s(__content.string(3330)) + &g.ent_symbol[i] + " is: " + &cur_sym(g)
            + &fmt_trim(g.market_cap[i] / new_shares, b"* ###,.00") + &g.currency_suffix + __content.string(1071),
    );
}
