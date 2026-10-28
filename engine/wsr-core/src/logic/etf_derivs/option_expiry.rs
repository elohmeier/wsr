//! Option expiry: `ExerciseOption` (0x5f993e) with its GOSUB `ExerciseOption_TransferShares`
//! (0x5fc2c4), the month-end processing `ProcessExpiringOptions` (0x5f2963) with its GOSUB
//! `ProcessExpiringOptions_ChargeSettlement` (0x5f355e), and the player reminder
//! `OptionExpiryReminder` (0x5ffdbc).
//!
//! Option records (`OptionRecs()`): `corp|type (1 call, 2 put)|holder|pct (negative = written)|strike
//! (MarketCap units)|exp month|exp year|premium` (executive options carry `GRANT`/`VESTED` in field 8).
//!
//! EXT locals retain x87 precision; SINGLE constants widen with `sf(..)` and DOUBLE stores are explicit.

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::too_many_arguments,
    clippy::if_same_then_else,
    clippy::needless_late_init,
    clippy::neg_multiply,
    clippy::manual_clamp
)]
use crate::pb::real::{Ext, cast};


use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::control::update_corp_control;
use crate::logic::earnings::{
    balance_cash_t_bills, calc_cur_month_of_year, calc_projected_earnings,
};
use crate::logic::events::antitrust::antitrust_review_acquisition;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::find_holding_slot;
use crate::logic::holdings::find_stock_holding;
use crate::logic::news::is_in_recent_headlines;
use crate::logic::options::{add_or_remove_option_rec, settle_option_rec};
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::update_control_chain;
use crate::logic::sim_entity::prelude::*;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::pb::string::{CaseMode, lcase, mcase};
use crate::pb::numeric::{Cux, ceil};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn fval(rec: &PbStr, n: i32) -> Ext {
    val(field(rec, n).as_bytes())
}

/// `PlayerExerciseOptions(id)` (`DIM (1 TO 5)`); `None` = error 9 (out of range).
fn player_exercise_options(g: &Globals, id: i32) -> Option<i32> {
    if (1..=5).contains(&id) {
        Some(g.player_exercise_options[id])
    } else {
        None
    }
}

/// `IsComputerPlayer(id)` (`DIM (1 TO 5)`); `None` = error 9.
fn is_computer_player(g: &Globals, id: i32) -> Option<i32> {
    if (1..=5).contains(&id) {
        Some(g.is_computer_player[id])
    } else {
        None
    }
}

/// The ON ERROR handler of ExerciseOption (0x5fc3bc) for error 9 (out of range):
/// `ShowRuntimeError(ERR, "ExerciseOption", "Player (Plr&) # =" + STR$(holderId), corpId)` (0x46ea48), whose
/// error-9 text is shown with ShowQuickMessage. (Error 151, division by zero, shows nothing unless
/// `DataPath` contains "PBWIN"; it is not modelled.)
fn exercise_option_error9(g: &mut Globals, ui: &mut dyn Ui, holder_id: i32, corp_id: i32) {
    let __content = crate::content::current();
    let diag = s(__content.string(4885)) + &str_s(cast::<Ext, _>(holder_id));
    let mut diag_info = PbStr::new();
    if !diag.is_empty() {
        diag_info = s(__content.string(4886)) + &diag + &s(")");
    }
    let mut corp_info = PbStr::new();
    if 0 < corp_id {
        corp_info =
            s(__content.string(4887)) + &str_s(cast::<Ext, _>(corp_id)) + &s(" (") + &g.ent_name[corp_id] + &s(")");
    }
    let err_text = s(__content.string(4888)) + &str_s(Ext::from_f64(9.0)) + &s(" ") + &s(__content.string(4889));
    let msg_text = err_text
        + &s(__content.string(4890))
        + &s(__content.string(4891))
        + &s(__content.string(4892))
        + &s("....")
        + &diag_info
        + &corp_info;
    let mut m = msg_text + &s(__content.string(4893));
    show_quick_message(g, ui, &mut m);
}

/// Port of ExerciseOption (0x5f993e) with its GOSUB ExerciseOption_TransferShares (0x5fc2c4) and the
/// exit block ExerciseOption_Exit (0x5fc431).
///
/// Exercises (part of) option record `rec_idx`: a long put or a written call (`direction` 1) sells
/// stock of the holder at the strike (99% proceeds; capital gain to `PlayerCapGains` / `ExtraordinaryPL`),
/// a long call or a written put (`direction` 2) buys stock from the public float at the strike (+1%
/// unless VESTED), limited by the float, by antitrust law (a stake that would bring control is checked
/// by `AntitrustReviewAcquisition`) and by the buyer's funds and credit (bank loan from `LenderId`).
/// AI holders only exercise in the money and according to the players' exercise settings
/// (`PlayerExerciseOptions`); `is_manual = 1` is the player's own exercise of `pct_requested` %.
/// Posts news, rewrites the record with the remaining quantity. Returns 1 fully exercised, 2 partially,
/// 3 partially as requested (manual), 0 not exercised.
///
/// Error 9 (`PlayerExerciseOptions(controllerId)` of an uncontrolled holder, id 0) goes to the ON ERROR
/// handler: runtime-error popup, result 0 (BUGS.md #171). Error 151 (`SharesOutstanding = 0`, a record
/// with quantity 0) leaves the routine silently with result 0.
pub fn exercise_option(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    rec_idx_ref: i32,
    is_manual: i32,
    pct_requested: i32,
) -> i32 {
    let __content = crate::content::current();
    let rec_idx = rec_idx_ref;
    let fmt_str: &[u8] = b"* ##############.00";
    let mut msg_text: PbStr; // = OptionRecs(recIdx) in the original (not read)
    let corp_id = to_long(fval(&g.option_recs[rec_idx], 1));
    let opt_type = to_long(fval(&g.option_recs[rec_idx], 2));
    let holder_id = to_long(fval(&g.option_recs[rec_idx], 3));
    let qty = to_long(fval(&g.option_recs[rec_idx], 4));
    // frame locals (zeroed at entry)
    let mut held_pct: i16 = 0;
    let mut adviser_ctrl: i32 = 0;
    let mut cap_pct: i16 = 0;
    let mut is_vested = 0;
    let mut ratio: Ext = Ext::from_f64(0.0); // DOUBLE, [ebp-0xe8]
    let premium_part: Ext;
    let mut allowed_pct: i32;

    if corp_id == holder_id {
        return 0;
    }
    if 85 < holder_id && holder_id < 130 && opt_type == 2 && qty < 0 {
        return 0;
    }
    let mut direction = 1;
    if opt_type == 1 && 0 < qty {
        direction = 2;
    }
    if opt_type == 2 && qty < 0 {
        direction = 2;
    }
    let slot = find_holding_slot(g, holder_id, corp_id);
    let mut controller_id = g.corp_controller[holder_id];
    if holder_id < 11 && is_manual == 0 {
        let Some(peo) = player_exercise_options(g, holder_id) else {
            exercise_option_error9(g, ui, holder_id, corp_id);
            return 0;
        };
        if peo == 0 {
            return 0;
        }
        if 0 < holder_id {
            let Some(icp) = is_computer_player(g, holder_id) else {
                exercise_option_error9(g, ui, holder_id, corp_id);
                return 0;
            };
            if icp == 1
                && cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) != corp_id
                && opt_type == 1
                && 0 < qty
            {
                return 0;
            }
        }
    } else if 10 < holder_id && is_manual == 0 {
        if g.industry_num[holder_id] == 71
            && 0 < get_etf_adviser_human_status(g, ui, rng, holder_id)
        {
            let adviser = g.fund_adviser[holder_id];
            adviser_ctrl = g.corp_controller[adviser];
            let Some(peo) = player_exercise_options(g, adviser_ctrl) else {
                exercise_option_error9(g, ui, holder_id, corp_id);
                return 0;
            };
            if peo == 0 {
                return 0;
            }
            if direction != 1 {
                if 0 < slot {
                    held_pct = g.hold_stock_pct[(slot, holder_id)];
                }
                if !(held_pct < 10) {
                    return 0;
                }
                if (cast::<Ext, _>(qty)).abs() + cast::<Ext, _>(held_pct) > Ext::from_f64(10.0) {
                    cap_pct = 10 - held_pct;
                    if qty < 0 {
                        cap_pct = cap_pct.wrapping_mul(-1);
                    }
                }
            }
        }
        if !(0 < adviser_ctrl) {
            let mut go = controller_id >= 1 && controller_id <= 10;
            if !go {
                if 0 < find_stock_holding(g, holder_id, corp_id) && opt_type == 2 && qty < 0 {
                    go = true;
                } else if g.industry_num[holder_id] == 71
                    && opt_type == 2
                    && qty < 0
                    && cast::<Ext, _>(g.bonds_issued[holder_id]) == Ext::from_f64(0.0)
                {
                    go = true;
                } else if 0 < find_stock_holding(g, holder_id, corp_id)
                    && g.industry_num[holder_id] == 71
                    && opt_type == 1
                    && qty < 0
                {
                    go = true;
                } else if g.industry_num[holder_id] == 71 && opt_type == 1 && 0 < qty {
                    go = true;
                } else if 0 < find_stock_holding(g, holder_id, corp_id) && opt_type == 1 && 0 < qty
                {
                    go = true;
                } else {
                    return 0;
                }
            }
            if go && controller_id < 6 {
                // 0x5fa03c
                let Some(peo) = player_exercise_options(g, controller_id) else {
                    exercise_option_error9(g, ui, holder_id, corp_id);
                    return 0;
                };
                if peo == 0 {
                    let Some(icp) = is_computer_player(g, controller_id) else {
                        exercise_option_error9(g, ui, holder_id, corp_id);
                        return 0;
                    };
                    if icp == 0 {
                        return 0;
                    }
                    let alt_slot = find_holding_slot(g, holder_id, corp_id);
                    if alt_slot == 0 && opt_type == 1 && 0 < qty {
                        return 0;
                    }
                    if cast::<i32, _>(g.hold_stock_id[(alt_slot, holder_id)]) != corp_id
                        && opt_type == 1
                        && 0 < qty
                    {
                        return 0;
                    }
                }
            }
        }
    }

    // 0x5fa155
    let mut exercise_pct = qty;
    if is_manual == 1 && 0 < pct_requested {
        exercise_pct = pct_requested;
    }
    if cap_pct != 0 && 0 < adviser_ctrl {
        exercise_pct = cast::<i32, _>(cap_pct);
    }
    if 100 < exercise_pct {
        exercise_pct = 100;
    }
    if exercise_pct < -100 {
        exercise_pct = -100;
    }
    let strike = fval(&g.option_recs[rec_idx], 5).abs();
    let exp_month = to_long(fval(&g.option_recs[rec_idx], 6));
    let exp_year = to_long(fval(&g.option_recs[rec_idx], 7));
    let premium = fval(&g.option_recs[rec_idx], 8);
    if 0 < instr(1, field(&g.option_recs[rec_idx], 8).as_bytes(), b"VESTED") {
        is_vested = 1;
    }
    if opt_type == 1 && is_manual == 0 && !(strike <= g.market_cap[corp_id]) {
        return 0;
    }
    if opt_type == 2 && is_manual == 0 && !(strike >= g.market_cap[corp_id]) {
        return 0;
    }
    let mut strike_abs = Cux::default();
    strike_abs.set(strike.abs());
    if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
        return 0; // ERROR 151 -> ON ERROR handler (silent)
    }
    let strike_str = fmt_trim(strike_abs.get() / g.shares_outstanding[corp_id], fmt_str);
    if direction == 2 && 10 < holder_id && g.industry_num[holder_id] == 1 {
        if is_manual == 1 {
            let mut m = s(
                __content.string(4894),
            );
            show_quick_message(g, ui, &mut m);
        }
        return 0;
    }

    let mut premium_part_v: Ext = Ext::from_f64(0.0);
    if direction == 1 {
        if slot == 0 {
            held_pct = -1;
        } else {
            held_pct = g.hold_stock_pct[(slot, holder_id)];
        }
        if held_pct < 1 {
            if is_manual == 1 {
                let mut m = s(__content.string(4895))
                    + &g.active_entity_name
                    + &s(__content.string(3737))
                    + &g.ent_name[corp_id]
                    + &s(__content.string(4896));
                show_quick_message(g, ui, &mut m);
            }
            return 0;
        }
        if (cast::<i32, _>(held_pct)) < exercise_pct {
            exercise_pct = cast::<i32, _>(held_pct);
        }
        if exercise_pct < 0 && (cast::<Ext, _>(exercise_pct)).abs() > cast::<Ext, _>(held_pct) {
            exercise_pct = -(cast::<i32, _>(held_pct));
        }
        let proceeds = strike * (cast::<Ext, _>(exercise_pct)).abs() * sf(0.99f32);
        let mut gain_loss: Ext;
        if (cast::<Ext, _>(exercise_pct)).abs() == cast::<Ext, _>(held_pct) {
            if is_manual == 1 && (cast::<Ext, _>(exercise_pct)).abs() != cast::<Ext, _>(pct_requested) {
                let mut m = s(__content.string(4897))
                    + &PbStr::from(str_num(cast::<Ext, _>(held_pct), 7).as_str())
                    + &s(__content.string(4898));
                show_quick_message(g, ui, &mut m);
            }
            g.hold_stock_id[(slot, holder_id)] = 0;
            g.hold_stock_id[(0, holder_id)] = g.hold_stock_id[(0, holder_id)].wrapping_sub(1);
            g.hold_stock_pct[(slot, holder_id)] = 0;
            gain_loss = proceeds - g.hold_stock_basis[(slot, holder_id)];
            g.hold_stock_basis[(slot, holder_id)] = Ext::from_f64(0.0);
        } else {
            if 0 < held_pct {
                ratio = crate::pb::numeric::double((cast::<Ext, _>(exercise_pct)).abs() / cast::<Ext, _>(held_pct));
            }
            let basis_part = g.hold_stock_basis[(slot, holder_id)] * ratio;
            gain_loss = proceeds - basis_part;
            let p =
                to_quad(cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]) - (cast::<Ext, _>(exercise_pct)).abs());
            g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(p);
            g.hold_stock_basis[(slot, holder_id)] =
                g.hold_stock_basis[(slot, holder_id)] - basis_part;
        }
        if qty == 0 {
            return 0; // ERROR 151 -> ON ERROR handler (silent)
        }
        if qty == exercise_pct {
            premium_part_v = premium;
        } else {
            ratio = crate::pb::numeric::double(cast::<Ext, _>(exercise_pct) / cast::<Ext, _>(qty));
            premium_part_v = premium * ratio;
        }
        if opt_type == 1 {
            gain_loss = gain_loss + premium_part_v.abs();
        } else {
            gain_loss = gain_loss - premium_part_v.abs();
        }
        if holder_id < 11 {
            g.player_cap_gains[holder_id] = g.player_cap_gains[holder_id] + gain_loss;
        } else {
            g.extraordinary_pl[holder_id] =
                to_quad(cast::<Ext, _>(g.extraordinary_pl[holder_id]) + gain_loss);
        }
        g.cash[holder_id] = g.cash[holder_id] + proceeds;
        if opt_type == 1 {
            g.news_msg = g.ent_symbol[corp_id].clone()
                + &s(__content.string(1564))
                + &g.ent_symbol[holder_id]
                + &s(" (")
                + &trim(str_s((cast::<Ext, _>(exercise_pct)).abs()).as_bytes())
                + &s(__content.string(4899))
                + &trim(g.currency_symbol.as_bytes())
                + &strike_str;
        } else {
            g.news_msg = g.ent_symbol[holder_id].clone()
                + &s(__content.string(4900))
                + &str_s((cast::<Ext, _>(exercise_pct)).abs())
                + &s("% of ")
                + &g.ent_symbol[corp_id]
                + &s(" at ")
                + &trim(g.currency_symbol.as_bytes())
                + &strike_str;
        }
    }
    if direction == 2 {
        if slot == 0 {
            if is_manual == 1 {
                let mut m = s(__content.string(4901))
                    + &g.active_entity_name
                    + &s(__content.string(4902));
                show_quick_message(g, ui, &mut m);
            } else if g.cur_player == holder_id
                || g.active_entity == holder_id
                || g.sel_entity == holder_id
            {
                let mut m = g.ent_name[holder_id].clone()
                    + &s(__content.string(4903))
                    + &g.ent_name[corp_id]
                    + &s(__content.string(4904));
                show_quick_message(g, ui, &mut m);
            }
            return 0;
        }
        held_pct = g.hold_stock_pct[(slot, holder_id)];
        update_corp_control(g, ui, rng, corp_id);
        g.public_pct = g.public_pct.wrapping_sub(1);
        if g.public_pct < 1 || held_pct < 0 {
            if is_manual == 1 {
                if g.public_pct < 1 {
                    msg_text =
                        s(__content.string(4905))
                            + &s(__content.string(4906))
                            + &s(__content.string(4907));
                } else {
                    msg_text = s(__content.string(4908))
                        + &g.ent_name[corp_id]
                        + &s(__content.string(4909));
                }
                ui.msgbox(&msg_text, 8208, &s(__content.string(4910)));
            }
            return 0;
        }
        if (cast::<i32, _>(g.public_pct)) < exercise_pct {
            exercise_pct = cast::<i32, _>(g.public_pct);
            if is_manual == 1 {
                let mut m = s(
                    __content.string(4911),
                );
                show_quick_message(g, ui, &mut m);
            }
        }
        if exercise_pct < 0 && (cast::<Ext, _>(exercise_pct)).abs() > cast::<Ext, _>(g.public_pct) {
            exercise_pct = -(cast::<i32, _>(g.public_pct));
        }
        if holder_id < 11 {
            controller_id = holder_id;
        } else {
            controller_id = g.corp_controller[holder_id];
        }
        if controller_id != g.corp_controller[corp_id]
            && !(1580 < holder_id && g.industry_num[holder_id] == 71)
        {
            if g.hold_stock_id[(slot, holder_id)] == 0 {
                g.hold_stock_id[(slot, holder_id)] = cast::<i16, _>(corp_id);
            }
            // GOSUB ExerciseOption_TransferShares
            allowed_pct = transfer_shares(
                g,
                ui,
                rng,
                corp_id,
                holder_id,
                slot,
                held_pct,
                controller_id,
                exercise_pct,
            );
            let safe_pct = if exercise_pct < 0 {
                -allowed_pct
            } else {
                allowed_pct
            };
            if held_pct == 0 {
                g.hold_stock_id[(slot, holder_id)] = 0;
                g.hold_stock_pct[(slot, holder_id)] = 0;
            } else {
                g.hold_stock_pct[(slot, holder_id)] = held_pct;
            }
            g.ctrl_change_msg_mode = 2;
            update_control_chain(g, ui, rng, corp_id);
            allowed_pct = 0;
            if !((cast::<Ext, _>(safe_pct)).abs() >= (cast::<Ext, _>(exercise_pct)).abs()) {
                let _industry = g.industry_num[corp_id];
                g.tmp_ext1 = cast::<Ext, _>(holder_id);
                g.tmp_ext2 = cast::<Ext, _>(corp_id);
                g.tmp_ext3 = Ext::from_f64(2.0);
                if 0 < controller_id && controller_id < 11 {
                    g.tmp_ext3 = Ext::from_f64(1.0);
                }
                if is_manual == 1 {
                    g.tmp_ext3 = Ext::from_f64(0.0);
                }
                allowed_pct = antitrust_review_acquisition(g, ui, rng, 3);
                if allowed_pct < 0 {
                    exercise_pct = safe_pct;
                }
            }
            if exercise_pct == 0 {
                if is_manual == 1 {
                    let mut m = s(
                        __content.string(4912),
                    ) + &g.ent_name[corp_id]
                        + &s(
                            __content.string(4913),
                        );
                    show_quick_message(g, ui, &mut m);
                } else {
                    g.news_msg = s(__content.string(4914))
                        + &g.ent_symbol[corp_id]
                        + &s(__content.string(4915));
                    post_news(g, ui, corp_id, 0);
                    let sym_len = len(g.ent_symbol[holder_id].as_bytes());
                    if sym_len < 17 {
                        msg_text = s("Govt.");
                        if sym_len < 12 {
                            msg_text = s(__content.string(2879));
                        }
                        g.news_msg = msg_text.clone()
                            + &s(__content.string(4916))
                            + &g.ent_symbol[holder_id]
                            + &s(__content.string(4917))
                            + &g.ent_symbol[corp_id];
                    }
                    let industry = cast::<i32, _>(g.industry_num[corp_id]);
                    let mut m = s(__content.string(4918))
                        + &g.ent_name[holder_id]
                        + &s(__content.string(4919))
                        + &g.ent_name[corp_id]
                        + &s(
                            __content.string(4920),
                        )
                        + &s("the ")
                        + &g.ind_name[industry]
                        + &s(__content.string(1547));
                    show_quick_message(g, ui, &mut m);
                    post_news(g, ui, 0, 0);
                }
                return 0;
            }
            if allowed_pct < 0 {
                if is_manual == 1 {
                    // BUGS.md #170: the industry name is indexed with ABS(allowedPct).
                    let ind = cast::<i32, _>(to_quad((cast::<Ext, _>(allowed_pct)).abs()));
                    let mut m = s(__content.string(4921))
                        + &g.ent_symbol[corp_id]
                        + &s(__content.string(4922))
                        + &s(__content.string(4923))
                        + &g.ind_name[ind]
                        + &s(__content.string(1547));
                    show_quick_message(g, ui, &mut m);
                } else {
                    msg_text = s("calls");
                    if opt_type == 2 {
                        msg_text = s("puts");
                    }
                    let mut msg_part = PbStr::new();
                    if 10 < holder_id {
                        msg_part = s(" by ") + &g.ent_symbol[holder_id];
                    }
                    g.news_msg = s(__content.string(4924))
                        + &msg_text
                        + &s(" on ")
                        + &g.ent_symbol[corp_id]
                        + &msg_part
                        + &s(__content.string(4925));
                    post_news(g, ui, holder_id, corp_id);
                }
            }
        }
        // 0x5fb707
        let mut total_cost = strike * (cast::<Ext, _>(exercise_pct)).abs();
        if is_vested == 0 {
            total_cost = sf(1.01f32) * total_cost;
        }
        let put_intrinsic = if exercise_pct < 0 {
            (g.market_cap[corp_id] - strike).abs()
        } else {
            Ext::from_f64(0.0)
        };
        loop {
            let tmp_credit = (cast::<Ext, _>(exercise_pct)).abs() * put_intrinsic;
            g.cash[holder_id] = g.cash[holder_id] + tmp_credit;
            calc_entity_financials(g, ui, rng, holder_id);
            g.cash[holder_id] = g.cash[holder_id] - tmp_credit;
            let shortfall = total_cost - g.liquid_funds;
            if !(shortfall > Ext::from_f64(0.0)) {
                break;
            }
            if 0 < exercise_pct {
                exercise_pct -= 1;
            } else {
                exercise_pct += 1;
            }
            total_cost = strike * (cast::<Ext, _>(exercise_pct)).abs();
            if !((cast::<Ext, _>(exercise_pct)).abs() > Ext::from_f64(0.0)) {
                break;
            }
        }
        if exercise_pct == 0 {
            if is_manual == 1 {
                let mut m = s(
                    __content.string(4926),
                );
                show_quick_message(g, ui, &mut m);
            }
            return 0;
        }
        if held_pct == 0 {
            g.hold_stock_id[(slot, holder_id)] = cast::<i16, _>(corp_id);
        }
        let p = to_quad((cast::<Ext, _>(exercise_pct)).abs() + cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]));
        g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(p);
        if qty == 0 {
            return 0; // ERROR 151 -> ON ERROR handler (silent)
        }
        if qty == exercise_pct {
            premium_part_v = premium;
        } else {
            ratio = crate::pb::numeric::double(cast::<Ext, _>(exercise_pct) / cast::<Ext, _>(qty));
            premium_part_v = premium * ratio;
        }
        g.hold_stock_basis[(slot, holder_id)] = g.hold_stock_basis[(slot, holder_id)] + total_cost;
        if opt_type == 1 {
            g.hold_stock_basis[(slot, holder_id)] =
                g.hold_stock_basis[(slot, holder_id)] + premium_part_v;
        } else {
            g.hold_stock_basis[(slot, holder_id)] =
                g.hold_stock_basis[(slot, holder_id)] - premium_part_v;
        }
        g.cash[holder_id] = g.cash[holder_id] - total_cost;
        balance_cash_t_bills(g, holder_id);
        if g.cash[holder_id] < Ext::from_f64(0.0) {
            let shortfall = g.cash[holder_id] * -Ext::from_f64(1.0);
            if g.unused_credit_line > shortfall {
                g.cash[holder_id] = Ext::from_f64(0.0);
                g.bank_debt[holder_id] = g.bank_debt[holder_id] + shortfall;
                let l = g.lender_id;
                g.cash[l] = g.cash[l] - shortfall;
            } else {
                g.cash[holder_id] = g.cash[holder_id] + g.unused_credit_line;
                g.bank_debt[holder_id] = g.bank_debt[holder_id] + g.unused_credit_line;
                let l = g.lender_id;
                g.cash[l] = g.cash[l] - g.unused_credit_line;
            }
        }
        if opt_type == 1 {
            g.news_msg = g.ent_symbol[holder_id].clone()
                + &s(__content.string(4927))
                + &str_s(cast::<Ext, _>(exercise_pct))
                + &s("% of ")
                + &g.ent_symbol[corp_id]
                + &s(" at ")
                + &trim(g.currency_symbol.as_bytes())
                + &strike_str
                + &s(".");
            if is_vested == 1 {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(4928), __content.bytes(4929));
                let msg_part = g.news_msg.clone();
                g.news_msg = g.ent_symbol[corp_id].clone()
                    + &s(__content.string(4930))
                    + &g.ent_symbol[holder_id]
                    + &s(__content.string(4931));
                post_news(g, ui, 0, 0);
                g.news_msg = msg_part;
                if is_manual == 1 {
                    let comp_per_unit = g.market_cap[corp_id] - strike;
                    let comp = cast::<Ext, _>(exercise_pct) * comp_per_unit;
                    g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - comp;
                    g.cash[corp_id] = g.cash[corp_id] - comp;
                    calc_projected_earnings(g, ui, rng, corp_id);
                }
            }
        } else {
            g.news_msg = trim(str_s((cast::<Ext, _>(exercise_pct)).abs()).as_bytes())
                + &s(__content.string(2163))
                + &g.ent_symbol[corp_id]
                + &s(__content.string(4932))
                + &g.ent_symbol[holder_id]
                + &s(" at ")
                + &trim(g.currency_symbol.as_bytes())
                + &strike_str;
            if len(g.ent_symbol[holder_id].as_bytes()) < 16 {
                g.news_msg = g.news_msg.clone() + &s(" price.");
            }
        }
    }
    premium_part = premium_part_v;
    if !g.news_msg.is_empty() {
        post_news(g, ui, holder_id, corp_id);
    }
    update_corp_control(g, ui, rng, corp_id);
    let remaining_qty = cast::<i16, _>(qty.wrapping_sub(exercise_pct));
    if remaining_qty == 0 {
        return 1;
    }
    msg_text = str_s(cast::<Ext, _>(corp_id))
        + &s("|")
        + &str_s(cast::<Ext, _>(opt_type))
        + &s("|")
        + &str_s(cast::<Ext, _>(holder_id))
        + &s("|")
        + &PbStr::from(str_num(cast::<Ext, _>(remaining_qty), 7).as_str())
        + &s("|");
    msg_text = msg_text
        + &str_s(strike)
        + &s("|")
        + &str_s(cast::<Ext, _>(exp_month))
        + &s("|")
        + &str_s(cast::<Ext, _>(exp_year))
        + &s("|")
        + &str_s(premium - premium_part);
    g.option_recs[rec_idx] = msg_text;
    if is_manual == 1 && (cast::<Ext, _>(exercise_pct)).abs() == cast::<Ext, _>(pct_requested) {
        return 3;
    }
    2
}

/// Port of ExerciseOption_TransferShares (0x5fc2c4), a GOSUB of [`exercise_option`].
/// Raises the holder's stake step by step (1 .. |exercisePct|) and recomputes control of `corp_id`;
/// stops (restoring `CorpController(corp)`) at the first step that would give `controller_id` control.
/// Returns `allowedPct`, the largest step that does not.
fn transfer_shares(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    corp_id: i32,
    holder_id: i32,
    slot: i32,
    held_pct: i16,
    controller_id: i32,
    exercise_pct: i32,
) -> i32 {
    let mut allowed_pct = 0;
    let orig_controller = g.corp_controller[corp_id];
    let n = to_long((cast::<Ext, _>(exercise_pct)).abs());
    let mut step_pct = 1;
    while step_pct <= n {
        g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(step_pct.wrapping_add(cast::<i32, _>(held_pct)));
        g.ctrl_change_msg_mode = 1;
        update_corp_control(g, ui, rng, corp_id);
        if controller_id == g.voting_controller {
            g.corp_controller[corp_id] = orig_controller;
            break;
        }
        allowed_pct = step_pct;
        step_pct += 1;
    }
    allowed_pct
}

/// Port of ProcessExpiringOptions (0x5f2963) with its GOSUB ProcessExpiringOptions_ChargeSettlement
/// (0x5f355e) and exit block ProcessExpiringOptions_Exit (0x5f367d).
///
/// Month-end expiry of option records 1..1000 whose expiry (`month * 0.01 + year`, SINGLE) is at or
/// before the current month (`CEIL(CalcCurMonthOfYear(SimCoCount))`): a VESTED executive option in the
/// money first charges its intrinsic value to the company (`AccruedPL`, `Cash`); then the option is
/// exercised ([`exercise_option`]); a fully exercised record is removed, the rest is settled at the
/// current price (`SettleOptionRec`) with an "expired"/"settled" headline. Packs the holdings before
/// and after.
///
/// The ON ERROR handler (ShowRuntimeError with the record index) is not modelled.
pub fn process_expiring_options(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    pack_holding_arrays(g, ui);
    let cur_month = to_long(ceil(calc_cur_month_of_year(g, g.sim_co_count)));
    let now = cast::<f32, _>(cast::<Ext, _>(cur_month) * sf(0.01f32) + cast::<Ext, _>(g.cur_year));
    let mut rec_idx = 1;
    loop {
        let mut worthless = false;
        let mut is_vested = false;
        let opt_rec = g.option_recs[rec_idx].clone();
        if !opt_rec.is_empty() {
            let exp = cast::<f32, _>(cast::<Ext, _>(to_long(fval(&opt_rec, 6))) * sf(0.01f32)
                + cast::<Ext, _>(to_long(fval(&opt_rec, 7))));
            if now >= exp {
                let corp_id = to_long(fval(&opt_rec, 1));
                let opt_type = to_long(fval(&opt_rec, 2));
                let holder = to_long(fval(&opt_rec, 3));
                let qty = to_long(fval(&opt_rec, 4));
                let strike = fval(&opt_rec, 5).abs();
                let mut headline = rtrim(opt_rec.as_bytes());
                if right(headline.as_bytes(), 6) == "VESTED" {
                    is_vested = true;
                }
                if is_vested && g.market_cap[corp_id] > strike {
                    // GOSUB ProcessExpiringOptions_ChargeSettlement
                    let intrinsic_val = g.market_cap[corp_id] - strike;
                    let settle_cost = cast::<Ext, _>(qty) * intrinsic_val;
                    g.accrued_pl[corp_id] = g.accrued_pl[corp_id] - settle_cost;
                    g.cash[corp_id] = g.cash[corp_id] - settle_cost;
                }
                if exercise_option(g, ui, rng, rec_idx, 0, 0) == 1 {
                    add_or_remove_option_rec(g, ui, &PbStr::new(), rec_idx);
                } else if !g.option_recs[rec_idx].is_empty() {
                    let mut opt_kind = s("call");
                    if opt_type == 2 {
                        opt_kind = s("put");
                    }
                    if opt_type == 1 && g.market_cap[corp_id] <= strike {
                        worthless = true;
                    }
                    if opt_type == 2 && strike <= g.market_cap[corp_id] {
                        worthless = true;
                    }
                    let mut short_tag = PbStr::new();
                    if qty < 0 {
                        short_tag = s(__content.string(4933));
                    }
                    if len(g.ent_name[holder].as_bytes()) > 6 {
                        // BUGS.md #172: reversed test, clears the tag for nearly every holder
                        short_tag = PbStr::new();
                    }
                    let settle = settle_option_rec(g, ui, rec_idx);
                    let mut result_str = s("a gain");
                    if settle == 2 {
                        result_str = s("a loss");
                    }
                    if settle == 3 {
                        result_str = s("no gain");
                    }
                    let mut sym_tag = PbStr::new();
                    if 10 < holder {
                        sym_tag = s(" (") + &g.ent_symbol[holder] + &s(")");
                    }
                    let month_year = PbStr::from(str_num(cast::<Ext, _>(cur_month), 15).as_str())
                        + &s("/")
                        + &trim(str_s(cast::<Ext, _>(g.cur_year)).as_bytes());
                    if worthless {
                        result_str = s(__content.string(4934));
                        if qty < 0 || is_vested {
                            result_str = PbStr::new();
                            if len(g.ent_name[holder].as_bytes()) < 21 {
                                short_tag = s(__content.string(4933));
                            }
                        }
                        if is_vested {
                            headline = g.ent_name[holder].clone() + &s(__content.string(4935));
                        } else {
                            headline = g.ent_name[holder].clone() + &sym_tag;
                            if !result_str.is_empty() && len(g.ent_name[holder].as_bytes()) > 17 {
                                headline = g.ent_symbol[holder].clone();
                            }
                            headline = headline + &month_year + &short_tag + &s(" ") + &opt_kind;
                        }
                        g.news_msg = headline.clone()
                            + &s("s on ")
                            + &g.ent_symbol[corp_id]
                            + &s(__content.string(4936))
                            + &result_str;
                    } else {
                        g.news_msg = g.ent_name[holder].clone()
                            + &sym_tag
                            + &month_year
                            + &short_tag
                            + &s(" ")
                            + &opt_kind
                            + &s("s on ")
                            + &g.ent_symbol[corp_id]
                            + &s(__content.string(4937))
                            + &result_str
                            + &s(".");
                        if is_vested {
                            g.news_msg = g.ent_symbol[corp_id].clone()
                                + &s(__content.string(4938))
                                + &g.ent_name[holder];
                        }
                    }
                    if g.corp_controller[holder] < 11 || g.corp_controller[corp_id] < 11 {
                        post_news(g, ui, 0, 0);
                    }
                    if !g.news_msg.is_empty()
                        && is_in_recent_headlines(g, __content.bytes(4939)) == 0
                        && is_in_recent_headlines(g, __content.bytes(4940)) == 0
                    {
                        post_news(g, ui, 0, 0);
                    }
                    if !g.news_msg.is_empty() && g.cur_player == g.corp_controller[holder] {
                        post_news(g, ui, 0, 0);
                    }
                }
            }
        }
        rec_idx += 1;
        if 1000 < rec_idx {
            pack_holding_arrays(g, ui);
            return;
        }
    }
}

/// Port of OptionExpiryReminder (0x5ffdbc).
///
/// Pop-up for the current player (player 2 while `ActionCode = "AUTO"`) about own or controlled
/// companies' option and [runtime resource wording] , with the current exercise /
/// physical-delivery settings. In the last quarter of the game's last year (late in the quarter) it
/// instead suggests realising tax losses when the player has capital gains. Nothing when there are no
/// option records (`OptionRecs(0) = ""`).
pub fn option_expiry_reminder(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // locals (zeroed at entry)
    let mut msg_text = PbStr::new();
    let mut fut_kind = PbStr::new();
    let mut has_long_commod = 0;
    let mut player_has_fut = 0;
    let mut holder_corp = 0;
    let mut other_corp = 0;
    let mut first_corp = 0;
    let mut exp_count = 0;
    let mut corp_id = 0;

    if g.option_recs[0].is_empty() {
        return;
    }
    if 1400 < g.sim_co_count && g.cur_quarter == 4 && g.game_end_year - 1 == g.cur_year {
        msg_text = s(
            __content.string(4941),
        );
        let cp = g.cur_player;
        calc_entity_financials(g, ui, rng, cp);
        if g.futures_net_value != Ext::from_f64(0.0) || g.phys_commod_value != Ext::from_f64(0.0) {
            msg_text = msg_text + &s(__content.string(4942));
        }
        if g.player_cap_gains[g.cur_player] > Ext::from_f64(0.0) {
            let mut m = s(__content.string(4943))
                + &s("\r\n")
                + &s("\r\n")
                + &msg_text;
            show_quick_message(g, ui, &mut m);
        }
        return;
    }
    let mut scan_limit = to_long(val(g.option_recs[0].as_bytes()) + Ext::from_f64(100.0));
    if 1000 < scan_limit {
        scan_limit = 1000;
    }
    let mut player_id = g.cur_player;
    if g.action_code == "AUTO" {
        player_id = 2;
    }
    let mut next_year = g.cur_year;
    let cur_month = calc_cur_month_of_year(g, g.sim_co_count);
    let mut next_month = to_long(ceil(cur_month) + Ext::from_f64(1.0));
    if 12 < next_month {
        next_month = 1;
        next_year += 1;
    }
    for rec_idx in 1..=scan_limit {
        if g.option_recs[rec_idx].is_empty() {
            continue;
        }
        let rec_text = g.option_recs[rec_idx].clone();
        if fval(&rec_text, 6) != cast::<Ext, _>(next_month) {
            continue;
        }
        let holder_id = to_long(fval(&rec_text, 3));
        if player_id != holder_id && player_id != g.corp_controller[holder_id] {
            continue;
        }
        if fval(&rec_text, 7) != cast::<Ext, _>(next_year) {
            continue;
        }
        holder_corp = holder_id;
        corp_id = to_long(fval(&rec_text, 1));
        if first_corp == 0 {
            first_corp = corp_id;
        } else if first_corp != corp_id {
            other_corp = corp_id;
        }
        exp_count += 1;
    }
    if 0 < exp_count {
        msg_text = s(__content.string(4944))
            + &g.ent_name[corp_id]
            + &s(__content.string(4945))
            + &g.month_name[next_month];
        if 10 < holder_corp {
            let with = g.ent_name[holder_corp].clone() + &s(" has");
            msg_text = replace(msg_text.as_bytes(), __content.bytes(1506), with.as_bytes());
        }
        if 10 < other_corp {
            msg_text = msg_text
                + &s(
                    __content.string(4946),
                );
        }
        msg_text = msg_text + &s(".");
        msg_text = msg_text + &s(__content.string(4947));
        let mut setting_str = s("NO.");
        if g.player_exercise_options[player_id] == 1 {
            setting_str = s("YES.");
        }
        msg_text = msg_text + &setting_str;
    }
    if val(g.futures_recs[0].as_bytes()) > Ext::from_f64(0.0) {
        exp_count = 0;
        holder_corp = 0;
        scan_limit = to_long(val(g.futures_recs[0].as_bytes()) + Ext::from_f64(100.0));
        if 1000 < scan_limit {
            scan_limit = 1000;
        }
        for rec_idx in 1..=scan_limit {
            if g.futures_recs[rec_idx].is_empty() {
                continue;
            }
            let rec_text = g.futures_recs[rec_idx].clone();
            if fval(&rec_text, 4) != cast::<Ext, _>(next_month) {
                continue;
            }
            let holder_id = to_long(fval(&rec_text, 2));
            if player_id != holder_id && player_id != g.corp_controller[holder_id] {
                continue;
            }
            if fval(&rec_text, 5) != cast::<Ext, _>(next_year) {
                continue;
            }
            if 10 < holder_id {
                holder_corp = holder_id;
            } else {
                player_has_fut = 1;
            }
            fut_kind = field(&rec_text, 1);
            if fut_kind != "STOCKS" && fval(&rec_text, 3) > Ext::from_f64(0.0) {
                has_long_commod = 1;
            }
            exp_count += 1;
        }
        if 0 < exp_count {
            if fut_kind == "STOCKS" {
                fut_kind = s(__content.string(2547));
            }
            fut_kind = lcase(fut_kind.as_bytes(), CaseMode::Ansi) + &s(" ");
            if fut_kind == __content.string(4948) || fut_kind == __content.string(4949) {
                fut_kind = mcase(fut_kind.as_bytes(), CaseMode::Ansi);
            }
            if 1 < exp_count {
                fut_kind = PbStr::new();
            }
            let mut fut_msg = s(__content.string(4950))
                + &fut_kind
                + &s(__content.string(4951))
                + &g.month_name[next_month]
                + &s(". ");
            if 10 < holder_corp {
                let with = g.ent_name[holder_corp].clone() + &s(" has");
                fut_msg = replace(fut_msg.as_bytes(), __content.bytes(1506), with.as_bytes());
                if player_has_fut == 1 {
                    fut_msg = s(
                        __content.string(4952),
                    );
                }
            }
            if instr(1, fut_kind.as_bytes(), b"index") == 0 && has_long_commod == 1 {
                fut_msg = fut_msg
                    + &s(
                        __content.string(4953),
                    );
                if g.player_take_delivery[player_id] == 1 {
                    fut_msg = replace(fut_msg.as_bytes(), b": NO", b": YES");
                }
            }
            if !msg_text.is_empty() {
                msg_text =
                    msg_text + &s("\r\n") + &s("\r\n") + &s(__content.string(4954)) + &s("\r\n") + &s("\r\n");
            }
            msg_text = msg_text + &fut_msg;
        }
    }
    if !msg_text.is_empty() {
        show_quick_message(g, ui, &mut msg_text);
    }
}
