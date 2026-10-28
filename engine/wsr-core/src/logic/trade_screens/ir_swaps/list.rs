//! The acting entity's swap list (`PickSwapFromList` 0x6437ed) and the analysis / termination of one swap
//! (`ShowSwapDetails` 0x644b6e).
use crate::pb::real::{Ext, cast};


use crate::logic::earnings::calc_projected_earnings;
use crate::logic::finance::calc_entity_financials;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::swaps::{calc_swap_est_qtr_profit, calc_swap_termination_fee, sort_swap_contracts};
use crate::logic::trading::borrow::borrow_to_cover_purchase;
use crate::logic::trading::prelude::*;
use crate::pb::numeric::Cur;

/// `CurQuarter * 0.1! + CurYear` (exact in f64, see `create::year_qtr`).
fn year_qtr(q: i32, y: i32) -> Ext {
    cast::<Ext, _>(q) * sf(0.1f32) + cast::<Ext, _>(y)
}

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// Port of PickSwapFromList (0x6437ed) with its GOSUBs PickSwapFromList_EstProfit (0x644a6a) and
/// PickSwapFromList_Exit (0x644b25, the common exit).
///
/// Sorts the swaps (`SortSwapContracts`) and lists those of the acting entity (counterparty, position,
/// begin date or "ACTIVE", end date, rate type, fixed rate, notional, estimated quarterly profit of active
/// swaps and their total, the current rates) as a pick list ("GetCorp" mode; each line links `@C<slot>`).
/// Returns the picked slot if it belongs to the acting entity, else 0. Without swaps: popup, 0.
pub fn pick_swap_from_list(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not reachable: at most 1,000 swaps, 1,019 lines)
    let mut result = 0;
    let mut lines: Vec<PbStr> = vec![PbStr::new(); 1501];
    let mut found = 0;
    let mut total_profit: i64 = 0;
    sort_swap_contracts(g, ui);
    let cur_date = year_qtr(g.cur_quarter, g.cur_year);
    lines[2] = s(__content.string(11537));
    lines[3] = s(__content.string(11538));
    lines[4] = s(__content.string(11539)) + &g.million_prefix_uc + __content.string(11540);
    lines[5] = s("------------------------- ----- ------ ------ ---------- ------ ---------- ---------");
    if 10 < g.active_entity {
        // the header's "YOUR" is in line 2, so this never matches (BUGS.md #372)
        lines[4] = replace(lines[4].as_bytes(), b"YOU ", g.ent_symbol[g.active_entity].as_bytes());
    }
    if g.million_prefix_uc == "B" {
        lines[4] = replace(lines[4].as_bytes(), b"MIL.", (g.million_prefix_uc.clone() + "IL.").as_bytes());
    }
    let mut line_no: usize = 5;
    let loop_end = to_long(val(g.swap_contracts[0].as_bytes()));
    let active = cast::<Ext, _>(g.active_entity);
    for slot in 1..=loop_end {
        let swap_rec = g.swap_contracts[slot].clone();
        if val(field(&swap_rec, 1).as_bytes()) != active && val(field(&swap_rec, 2).as_bytes()) != active {
            continue;
        }
        if swap_rec.is_empty() {
            continue;
        }
        line_no += 1;
        found = 1;
        let mut cp_id = to_long(val(field(&swap_rec, 1).as_bytes()));
        let position;
        if g.active_entity == cp_id {
            position = s("LONG");
            cp_id = to_long(val(field(&swap_rec, 2).as_bytes()));
        } else {
            position = s("SHORT");
        }
        let cp_name = g.ent_name[cp_id].clone() + &space(25);
        lines[line_no] = mid(cp_name.as_bytes(), 1, 26);
        let mut field_text = position.clone() + "  ";
        field_text = mid(field_text.as_bytes(), 1, 6);
        lines[line_no] = lines[line_no].clone() + &field_text;
        field_text = trim(field(&swap_rec, 5).as_bytes());
        if val(field_text.as_bytes()) <= cur_date {
            field_text = s("ACTIVE");
        }
        lines[line_no] = lines[line_no].clone() + &field_text + " " + &trim(field(&swap_rec, 6).as_bytes()) + " ";
        let rate_type = trim(field(&swap_rec, 4).as_bytes());
        field_text = s(__content.string(6151));
        if rate_type == "P" {
            field_text = s(__content.string(6152));
        } else if rate_type == "L" {
            field_text = s(__content.string(6153));
        }
        lines[line_no] = lines[line_no].clone() + &field_text + " ";
        let mut rate_text = trim(field(&swap_rec, 3).as_bytes());
        if instr(1, rate_text.as_bytes(), b".") < 3 {
            rate_text = s(" ") + &rate_text;
        }
        if instr(1, rate_text.as_bytes(), b".") < 3 {
            rate_text = s(" ") + &rate_text;
        }
        field_text = right(rate_text.as_bytes(), 3);
        if !(mid(field_text.as_bytes(), 1, 1) == ".") {
            rate_text = rate_text + "0";
        }
        field_text = right(rate_text.as_bytes(), 3);
        if !(mid(field_text.as_bytes(), 1, 1) == ".") {
            rate_text = rate_text + "0";
        }
        lines[line_no] = lines[line_no].clone() + &rate_text + "% ";
        let notional = to_quad(val(field(&swap_rec, 7).as_bytes()));
        field_text = space(13) + &fmt_trim(cast::<Ext, _>(notional), "* ###,") + " ";
        lines[line_no] = lines[line_no].clone() + &right(field_text.as_bytes(), 10);
        field_text = trim(field(&swap_rec, 5).as_bytes());
        if val(field_text.as_bytes()) <= cur_date {
            let fixed_rate = Cur::from_f64(val(trim(field(&swap_rec, 3).as_bytes()).as_bytes()));
            // PickSwapFromList_EstProfit (0x644a6a)
            let _ = calc_swap_est_qtr_profit(g, &rate_type, fixed_rate, &position, notional);
            let est_profit = to_quad(g.tmp_ext1);
            g.tmp_ext1 = Ext::from_f64(0.0);
            total_profit = to_quad(cast::<Ext, _>(total_profit) + cast::<Ext, _>(est_profit));
            field_text = space(13) + &fmt_trim(cast::<Ext, _>(est_profit), "* ###,");
        } else {
            field_text = s(__content.string(11541));
        }
        lines[line_no] = lines[line_no].clone() + &right(field_text.as_bytes(), 9);
        lines[line_no] = lines[line_no].clone() + &space(50) + "@C" + &link_id(slot);
    }
    if found == 0 {
        let mut m = g.active_entity_name.clone() + __content.string(11542);
        show_quick_message(g, ui, &mut m);
        // (UI) DIALOG END hDlgTransMenu
        return result;
    }
    line_no += 3;
    let mut field_text = space(12) + &fmt_trim(cast::<Ext, _>(total_profit), "* ###,");
    field_text = right(field_text.as_bytes(), 10);
    lines[line_no] = s(__content.string(11543))
        + &g.million_prefix_uc
        + "IL.): "
        + &field_text;
    lines[line_no + 1] = space(75) + "=========";
    lines[line_no + 3] = s(__content.string(11544));
    lines[line_no + 4] = s("  ---------------------------------------------------------------");
    lines[line_no + 5] = s(__content.string(11545)) + &fmt(cast::<Ext, _>(g.prime_rate), "* ##.00") + "%";
    lines[line_no + 6] = s(__content.string(11546)) + &fmt(cast::<Ext, _>(g.long_bond_yield), "* ##.00") + "%";
    lines[line_no + 7] = s(__content.string(11547)) + &fmt(cast::<Ext, _>(g.short_bond_yield), "* ##.00") + "%";
    lines[line_no + 8] = lines[line_no + 4].clone();
    line_no += 11;
    lines[line_no] = s("@END");
    g.text_box_title = s(__content.string(11548))
        + &g.active_entity_name
        + __content.string(11549)
        + &str_trim(cast::<Ext, _>(g.cur_quarter))
        + " OF"
        + &str_s(cast::<Ext, _>(g.cur_year))
        + ":";
    for (slot, line) in lines.iter().enumerate().take(line_no + 1).skip(1) {
        g.text_lines[cast::<i32, _>(slot)] = line.clone();
    }
    g.entity_pick_mode = s("GetCorp");
    g.list_item_noun = s("swap");
    g.text_box_mode = s("84");
    g.target_corp = 0;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    let tc = g.target_corp;
    if 0 < tc && cast::<Ext, _>(tc) <= val(g.swap_contracts[0].as_bytes()) {
        let rec = g.swap_contracts[tc].clone();
        if val(field(&rec, 1).as_bytes()) != active && val(field(&rec, 2).as_bytes()) != active {
            g.target_corp = 0;
        }
    } else {
        g.target_corp = 0;
    }
    if 0 < g.target_corp {
        result = g.target_corp;
    }
    g.target_corp = 0;
    result
}

/// Port of ShowSwapDetails (0x644b6e) with its GOSUB ShowSwapDetails_RateName (0x645ac5) and the exit
/// label EmptyStub_645ce6 (0x645ce6).
///
/// The terms of swap `slot` from the acting entity's side, its estimated quarterly cost/benefit
/// (`CalcSwapEstQtrProfit`, for swaps already in effect) and the termination fee (`CalcSwapTerminationFee`,
/// a quarter of it when the swap earns more than 4 a quarter; nothing when the player controls the
/// counterparty). Mode 1: shown (MSGBOX). Mode 2: asks whether to terminate (YES/NO). A swap with the
/// player's own counterparty is just canceled on YES (popup); otherwise anything but NO pays the fee (cash
/// and T-bills, else financed by `BorrowToCoverPurchase`, NO there ends it): `Cash` and `AccruedPL` move
/// from the acting entity to the counterparty, the swap is removed (`PackHoldingArrays`,
/// `CalcProjectedEarnings` of the counterparty), popup + news, `TurnActionCount` + 1.
pub fn show_swap_details(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, slot: i32, mode: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let slot_idx = slot;
    let swap_rec = g.swap_contracts[slot_idx].clone();
    let long_party = to_long(val(field(&swap_rec, 1).as_bytes()));
    let short_party = to_long(val(field(&swap_rec, 2).as_bytes()));
    let mut cp_name = g.ent_name[short_party].clone();
    let mut position = s("LONG");
    let mut i = short_party;
    if g.active_entity != long_party {
        cp_name = g.ent_name[long_party].clone();
        position = s("SHORT");
        i = long_party;
    }
    let notional = to_quad(val(field(&swap_rec, 7).as_bytes()));
    let cur_date = year_qtr(g.cur_quarter, g.cur_year);
    let start_date = val(field(&swap_rec, 5).as_bytes());
    let _end_date = val(field(&swap_rec, 6).as_bytes());
    let fixed_rate = Cur::from_f64(val(field(&swap_rec, 3).as_bytes()));
    // ShowSwapDetails_RateName (0x645ac5)
    let rate_sel = field(&swap_rec, 4);
    let mut rate_name = PbStr::new();
    if rate_sel == "P" {
        rate_name = s(__content.string(11412));
    } else if rate_sel == "L" {
        rate_name = s(__content.string(11550));
    } else if rate_sel == "S" {
        rate_name = s(__content.string(11551));
    }
    let float_rate_text = trim(calc_swap_est_qtr_profit(g, &rate_sel, fixed_rate, &position, notional).as_bytes());
    let est_qtr_profit = g.tmp_ext1;
    g.tmp_ext1 = Ext::from_f64(0.0);

    let mut msg_text = s(__content.string(11552)) + "\r" + "\r";
    if g.active_entity == long_party {
        msg_text = msg_text
            + &g.ent_name[long_party]
            + __content.string(11553)
            + &field(&swap_rec, 3)
            + __content.string(11554)
            + &g.ent_symbol[short_party]
            + "\r"
            + &g.ent_name[long_party]
            + ": Pays "
            + &float_rate_text
            + __content.string(11555)
            + &g.ent_symbol[short_party]
            + "\r"
            + "\r";
    } else {
        msg_text = msg_text
            + &g.active_entity_name
            + __content.string(11556)
            + &float_rate_text
            + __content.string(11557)
            + &g.ent_symbol[i]
            + "\r"
            + &g.active_entity_name
            + ": Pays "
            + &field(&swap_rec, 3)
            + __content.string(11558)
            + &g.ent_symbol[i]
            + "\r";
    }
    msg_text = msg_text
        + __content.string(11559)
        + &field(&swap_rec, 6)
        + "\r"
        + __content.string(11560)
        + &rate_name
        + "\r"
        + __content.string(11561)
        + &fmt_trim(cast::<Ext, _>(notional), "* ####,")
        + " "
        + &g.million_prefix_lc
        + "illion"
        + "\r"
        + "\r";
    msg_text = msg_text + __content.string(11562) + &g.active_entity_name + ":" + "\r" + "\r";
    if start_date <= cur_date {
        let cost_word = if est_qtr_profit < Ext::from_f64(0.0) { s("cost") } else { s("benefit") };
        msg_text = msg_text
            + "Net "
            + &cost_word
            + __content.string(11563)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(est_qtr_profit.abs(), "* ####,.00")
            + " "
            + &g.million_prefix_lc
            + "illion.";
    } else {
        let start = field(&swap_rec, 5);
        msg_text = msg_text
            + __content.string(11564)
            + &right(start.as_bytes(), 1)
            + " of "
            + &mid(start.as_bytes(), 1, 4);
    }
    msg_text = msg_text + "\r" + "\r";
    let active = g.active_entity;
    let mut term_fee = calc_swap_termination_fee(g, active, slot_idx);
    if Ext::from_f64(4.0) < est_qtr_profit {
        term_fee = term_fee * sf(0.25f32);
    }
    let player_controls = g.cur_player == i || g.cur_player == g.corp_controller[i];
    if player_controls {
        msg_text = msg_text + __content.string(11565) + "\r" + "\r";
    } else {
        msg_text = msg_text
            + __content.string(11566)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(term_fee, "* #####,")
            + " "
            + &g.million_prefix_lc
            + "illion."
            + "\r"
            + "\r";
    }
    if mode == 1 {
        ui.msgbox(&msg_text, 8256, &(s(__content.string(11567)) + &cp_name));
        return;
    }
    if player_controls {
        msg_text = msg_text + __content.string(11568);
    } else {
        msg_text = msg_text + __content.string(11569) + __content.string(11570) + &cp_name + " NOW?";
    }
    let answer = ui.msgbox(&msg_text, 8484, &s(__content.string(11213)));
    let player_controls = g.cur_player == i || g.cur_player == g.corp_controller[i];
    if player_controls && answer == 6 {
        g.swap_contracts[slot_idx] = PbStr::new();
        pack_holding_arrays(g, ui);
        calc_projected_earnings(g, ui, rng, i);
        let mut m = s(__content.string(11571))
            + &g.ent_name[i]
            + " and "
            + &g.active_entity_name
            + __content.string(11572);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if answer == 7 {
        return;
    }
    if g.cash[active] + g.t_bills[active] < term_fee {
        let mut deficit = term_fee - (g.cash[active] + g.t_bills[active]);
        calc_entity_financials(g, ui, rng, active);
        if borrow_to_cover_purchase(g, ui, &mut deficit, term_fee, 0) == 7 {
            return;
        }
    }
    let active = g.active_entity;
    g.cash[active] = g.cash[active] - term_fee;
    g.accrued_pl[active] = g.accrued_pl[active] - term_fee;
    g.cash[i] = g.cash[i] + term_fee;
    g.accrued_pl[i] = g.accrued_pl[i] + term_fee;
    g.swap_contracts[slot_idx] = PbStr::new();
    pack_holding_arrays(g, ui);
    calc_projected_earnings(g, ui, rng, i);
    g.news_msg = g.ent_symbol[active].clone()
        + " pays "
        + &g.ent_symbol[i]
        + " "
        + &trim(g.currency_symbol.as_bytes())
        + &fmt_trim(term_fee, "####,")
        + " "
        + &g.million_prefix_lc
        + __content.string(11573);
    let mut m = g.news_msg.clone();
    show_quick_message(g, ui, &mut m);
    g.news_msg = m;
    post_news(g, ui, 0, 0);
    g.turn_action_count = g.turn_action_count + 1;
}
