//! `MergeCompanies` (0x509293) with its GOSUB blocks (`Merger_ChargeCosts` 0x50fed0,
//! `Merger_TrimHoldingsTo100Pct` 0x51012b, `Merger_SumHolderBases` 0x510572), `AdjustOptionsForMerger`
//! (0x5f861f), `DisableMainControls` (0x4242c6) and the runtime-error pop-up `ShowRuntimeError` (0x46ea48).
//!
//! Error handling: MergeCompanies' ON ERROR handler (0x510a27) calls
//! `ShowRuntimeError(ERR, "ExecuteMerger", "  ACQUIRER = # " + STR$(acquirerId) + " TGT = # " + STR$(targetId))`
//! and leaves the routine. The one reachable error is error 9 in `Merger_TrimHoldingsTo100Pct`
//! (BUGS.md #220): the port shows the same pop-up and leaves at the same point.
use crate::pb::real::{Ext, cast};


use super::liquidate_sub::liquidate_sub_into_parent;
use super::prelude::*;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::corp_ai::spinoff::spin_off_subsidiary_menu;
use crate::logic::events::antitrust::antitrust_review_acquisition;
use crate::logic::finance::calc_entity_financials;
use crate::logic::holdings::{find_holding_slot, find_stock_holding};
use crate::logic::options::close_option_positions;
use crate::logic::report::{number_to_words, show_quick_message};
use crate::logic::reports_corp::{check_cross_ownership, divest_stock};
use crate::logic::reports_lists::update_control_chain;
use crate::logic::shorts::find_short_positions;
use crate::logic::sim_entity::ai_corp::capital_infusion;
use crate::logic::sim_entity::asset_sales::calc_group_stake_pct;
use crate::logic::sim_entity::bonds::{adjust_conversion_price, adjust_price_alerts_for_stock};
use crate::logic::sim_entity::mgmt::force_cover_shorts;
use crate::logic::sim_entity::quarter_start::recapture_excess_loss;
use crate::logic::turn::enable_main_controls;
use crate::logic::valuation::{
    adjust_index_divisor, calc_stock_price, get_mgmt_margin_vs_industry, price_pct_change_months,
};
use crate::pb::ext80::Ext80;
use crate::pb::numeric::{str_num_ext, val_ext};
use crate::pb::string::{CaseMode, mcase, ucase};

/// LiquidateSubIntoParent (0x512aa9): may be left out with [`merge_companies_skipping`].
pub const LIQUIDATE_SUB_INTO_PARENT: u32 = 0x512aa9;
/// SpinOffSubsidiaryMenu (0x5bfb37): may be left out with [`merge_companies_skipping`].
pub const SPIN_OFF_SUBSIDIARY_MENU: u32 = 0x5bfb37;

/// Port of ShowRuntimeError (0x46ea48).
/// The ON ERROR pop-up: "Error number n (description) [runtime resource wording] function `proc_name`....",
/// the diagnostic text and the company, as a quick message ("  Continuing momentarily...."). Error 151
/// (division by zero) is a MSGBOX instead, and only when `DataPath` contains "PBWIN" (else nothing).
pub fn show_runtime_error(
    g: &mut Globals,
    ui: &mut dyn Ui,
    err_num: i32,
    proc_name: &PbStr,
    diag_text: &PbStr,
    corp_id: i32,
) {
    let __content = crate::content::current();
    let mut diag_info = PbStr::new();
    if !diag_text.is_empty() {
        diag_info = s(__content.string(4886)) + diag_text + ")";
    }
    let mut corp_info = PbStr::new();
    if 0 < corp_id {
        corp_info = s(__content.string(4887)) + &str_s(cast::<Ext, _>(corp_id)) + " (" + &g.ent_name[corp_id] + ")";
    }
    let err_text = s(__content.string(4888)) + &str_s(cast::<Ext, _>(err_num));
    let err_desc = match err_num {
        5 => s(__content.string(9668)),
        7 => s(__content.string(9669)),
        9 => s(__content.string(4889)),
        52 => s(__content.string(9670)),
        53 => s(__content.string(9671)),
        55 => s(__content.string(9672)),
        61 => s(__content.string(9673)),
        62 => s(__content.string(9674)),
        70 => s(__content.string(9675)),
        72 => s(__content.string(9676)),
        76 => s(__content.string(9677)),
        151 => {
            if instr(1, ucase(g.data_path.as_bytes(), CaseMode::Ansi).as_bytes(), b"PBWIN") == 0 {
                return;
            }
            s(__content.string(9678))
        }
        _ => s(__content.string(9679)) + "#",
    };
    let msg_text = err_text
        + " "
        + &err_desc
        + __content.string(4890)
        + __content.string(4891)
        + proc_name
        + "...."
        + &diag_info
        + &corp_info;
    if err_num == 151 {
        ui.msgbox(&msg_text, 0, &PbStr::new());
    } else {
        let mut m = msg_text + __content.string(4893);
        show_quick_message(g, ui, &mut m);
    }
}

/// Port of DisableMainControls (0x4242c6): disables the main window's buttons while a dialog is up (UI
/// only; no game state).
pub fn disable_main_controls(g: &mut Globals) {
    let _ = g;
}

/// Port of AdjustOptionsForMerger (0x5f861f).
/// After [runtime resource wording] in which `corp_id`'s shares are rescaled by the exchange ratio
/// `exch_ratio` (old shares per new share), every option record on `corp_id` (`OptionRecs`,
/// `"corp|holder|type|qty|strike|..."`, 8 fields) gets quantity `ROUND(ratio * qty)` (at least 1 / -1)
/// and strike `|strike / ratio|`; the other fields are trimmed. Headline if the stock has options
/// outstanding. A zero ratio is error 151 (the ON ERROR handler's ShowRuntimeError("OptionMergeAdj")
/// is silent unless `DataPath` contains "PBWIN").
pub fn adjust_options_for_merger(g: &mut Globals, ui: &mut dyn Ui, corp_id: i32, exch_ratio: Ext) {
    let __content = crate::content::current();
    let ratio = exch_ratio;
    if ratio == Ext::from_f64(0.0) || ratio.is_nan() {
        // ERROR 151 -> ON ERROR handler (0x5f8a77)
        let diag = s("Z9# =") + &str_s(ratio);
        show_runtime_error(g, ui, 151, &s(__content.string(9680)), &diag, corp_id);
        return;
    }
    find_option_positions(g, ui, 0, corp_id, 0, 0);
    if g.has_options_outstanding[corp_id] == 0 {
        return;
    }
    for rec_idx in 1..=1000 {
        if g.option_recs[rec_idx].is_empty() {
            continue;
        }
        let opt_rec = g.option_recs[rec_idx].clone();
        let field = |n: i32| parse_any(opt_rec.as_bytes(), b"|", n);
        if !(val(field(1).as_bytes()) == cast::<Ext, _>(corp_id)) {
            continue;
        }
        // newStrike (EXT) = ABS(VAL(field 5) / ratio), kept in 80 bits for its STR$
        let new_strike = val_ext(field(5).as_bytes()).fdiv(Ext80::from_ext(ratio)).abs();
        let mut new_qty = to_long(val(field(4).as_bytes()));
        new_qty = Ext80::from_ext(ratio).fmul(Ext80::from_i64(cast::<i64, _>(new_qty))).to_i32();
        if new_qty == 0 {
            new_qty = 1;
            if val(field(4).as_bytes()) < Ext::from_f64(0.0) {
                new_qty = -1;
            }
        }
        let mut new_rec = PbStr::new();
        for f in 1..=8 {
            if f < 4 || 5 < f {
                new_rec = new_rec + &trim(field(f).as_bytes());
            } else if f == 4 {
                new_rec = new_rec + &str_s(cast::<Ext, _>(new_qty));
            } else {
                new_rec = new_rec + &PbStr::from(str_num_ext(new_strike, 15).as_str());
            }
            if f < 8 {
                new_rec = new_rec + "|";
            }
        }
        g.option_recs[rec_idx] = new_rec;
    }
    g.news_msg = s(__content.string(9681)) + &g.ent_symbol[corp_id] + __content.string(9682);
    post_news(g, ui, 0, 0);
}

/// The locals of MergeCompanies that its GOSUB blocks use.
#[derive(Default)]
struct Frame {
    acquirer_id: i32,
    target_id: i32,
    merge_mode: i32,
    /// EXT
    merger_cost: Ext,
    reversed: PbStr,
    attempt_suffix: PbStr,
    acq_slot: i32,
    hold_value: Ext,
    // Merger_SumHolderBases
    acq_basis_total: Ext,
    tgt_basis_total: Ext,
    acq_excess_loss: Ext,
    tgt_excess_loss: Ext,
    acq_excess_holder: i32,
    tgt_excess_holder: i32,
    acq_hold_pct: i16,
    acq_hold_basis: Ext,
    tgt_hold_pct: i16,
    tgt_hold_basis: Ext,
}

/// GOSUB Merger_ChargeCosts (0x50fed0).
/// The acquirer (the target after a reverse merger) pays `mergerCost` from cash and books it as a loss;
/// from 1 ($M) on a headline (and, for the player, a quick message). In the computer modes the headline
/// is only posted for a completed merger (`attemptSuffix` empty).
fn charge_costs(f: &Frame, g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let mut cost_payer_id = f.acquirer_id;
    let mut other_corp_id = f.target_id;
    if f.reversed == "Y" {
        std::mem::swap(&mut cost_payer_id, &mut other_corp_id);
    }
    g.cash[cost_payer_id] = g.cash[cost_payer_id] - f.merger_cost;
    g.accrued_pl[cost_payer_id] = g.accrued_pl[cost_payer_id] - f.merger_cost;
    if Ext::from_f64(1.0) <= f.merger_cost {
        g.news_msg = g.ent_symbol[cost_payer_id].clone()
            + __content.string(9683)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(f.merger_cost, b"############.0")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + " in "
            + &g.ent_symbol[other_corp_id]
            + " merger"
            + &f.attempt_suffix
            + ".";
        if f.merge_mode == 0 {
            crate::logic::report::show_quick_news(g, ui);
        }
        if f.merge_mode != 0 && !f.attempt_suffix.is_empty() {
            return;
        }
        post_news(g, ui, 0, 0);
    }
}

/// GOSUB Merger_SumHolderBases (0x510572).
/// Sums the tax bases of all (active) holders of the acquirer's and the target's stock (a negative basis
/// of a holder other than the other party is the excess-loss account: amount and holder kept instead),
/// plus the public float at market cap; brings both companies' control up to date
/// (`CtrlChangeMsgMode = 1` if the target's control is decided by the vote) and reads the cross holdings.
fn sum_holder_bases(f: &mut Frame, g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    for holder_id in 1..=1600 {
        if g.entity_status[holder_id] != 1 {
            continue;
        }
        for slot_idx in 1..=15 {
            let held_id = cast::<i32, _>(g.hold_stock_id[(slot_idx, holder_id)]);
            if f.acquirer_id == held_id && 0 < g.hold_stock_pct[(slot_idx, holder_id)] {
                let basis = g.hold_stock_basis[(slot_idx, holder_id)];
                if basis >= Ext::from_f64(0.0) || f.target_id == holder_id {
                    f.acq_basis_total = f.acq_basis_total + basis;
                } else {
                    f.acq_excess_loss = basis.abs();
                    f.acq_excess_holder = holder_id;
                }
            }
            if f.target_id == held_id && 0 < g.hold_stock_pct[(slot_idx, holder_id)] {
                let basis = g.hold_stock_basis[(slot_idx, holder_id)];
                if basis >= Ext::from_f64(0.0) || f.acquirer_id == holder_id {
                    f.tgt_basis_total = f.tgt_basis_total + basis;
                } else {
                    f.tgt_excess_loss = basis.abs();
                    f.tgt_excess_holder = holder_id;
                }
            }
        }
    }
    let prev_controller = g.corp_controller[f.target_id];
    for _ in 0..5 {
        update_corp_control(g, ui, rng, f.acquirer_id);
    }
    if 0 < g.public_pct {
        f.acq_basis_total = f.acq_basis_total + g.market_cap[f.acquirer_id] * cast::<Ext, _>(g.public_pct);
    }
    if g.corp_controller[f.target_id] == g.voting_controller || prev_controller == g.voting_controller {
        g.ctrl_change_msg_mode = 1;
    }
    update_corp_control(g, ui, rng, f.target_id);
    if 0 < g.public_pct {
        f.tgt_basis_total = f.tgt_basis_total + g.market_cap[f.target_id] * cast::<Ext, _>(g.public_pct);
    }
    f.acq_slot = find_stock_holding(g, f.acquirer_id, f.target_id);
    if 0 < f.acq_slot {
        f.acq_hold_pct = g.hold_stock_pct[(f.acq_slot, f.acquirer_id)];
        f.acq_hold_basis = g.hold_stock_basis[(f.acq_slot, f.acquirer_id)];
    }
    f.acq_slot = find_stock_holding(g, f.target_id, f.acquirer_id);
    if 0 < f.acq_slot {
        f.tgt_hold_pct = g.hold_stock_pct[(f.acq_slot, f.target_id)];
        f.tgt_hold_basis = g.hold_stock_basis[(f.acq_slot, f.target_id)];
    }
    f.acq_slot = 0;
}

/// GOSUB Merger_TrimHoldingsTo100Pct (0x51012b).
/// While the (rounded) holdings of the merged company add up to more than 100%, the largest holder gives
/// up one point. When that holding reaches 0 the holder is paid the full market cap (`holdValue`; a
/// capital gain for a player, extraordinary P/L for a company) and the basis is to be cleared, but the
/// statement clears `HoldStockBasis(slot, holderId)` with `holderId` = 1601 left by the scan loop
/// (BUGS.md #220): error 9. Returns `Err(())` there (the caller's ON ERROR handler takes over; the
/// basis stays, the debug MSGBOX " [runtime resource wording] …" after it is never reached).
fn trim_holdings_to_100_pct(f: &mut Frame, g: &mut Globals) -> Result<(), ()> {
    loop {
        let mut total_pct: i32 = 0;
        let mut max_pct: i32 = 0;
        f.acq_slot = 0;
        let mut max_holder_id: i32 = 0;
        for holder_id in 1..=1600 {
            for slot in 1..=15 {
                if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == f.acquirer_id && 0 < g.hold_stock_pct[(slot, holder_id)]
                {
                    total_pct += cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
                    if max_pct < cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]) {
                        max_pct = cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]);
                        max_holder_id = holder_id;
                    }
                }
            }
        }
        if total_pct < 101 {
            return Ok(());
        }
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, max_holder_id)]) == f.acquirer_id
                && 0 < g.hold_stock_pct[(slot, max_holder_id)]
            {
                g.hold_stock_pct[(slot, max_holder_id)] = g.hold_stock_pct[(slot, max_holder_id)].wrapping_sub(1);
                if g.hold_stock_pct[(slot, max_holder_id)] == 0 {
                    g.hold_stock_id[(slot, max_holder_id)] = 0;
                    f.hold_value = g.market_cap[f.acquirer_id];
                    g.cash[max_holder_id] = g.cash[max_holder_id] + f.hold_value;
                    let gain = f.hold_value - g.hold_stock_basis[(slot, max_holder_id)];
                    if g.num_players < max_holder_id {
                        g.extraordinary_pl[max_holder_id] = to_quad(cast::<Ext, _>(g.extraordinary_pl[max_holder_id]) + gain);
                    } else {
                        g.player_cap_gains[max_holder_id] = g.player_cap_gains[max_holder_id] + gain;
                    }
                    // HoldStockBasis(slot, holderId) = 0 with holderId = 1601: error 9 (BUGS.md #220)
                    return Err(());
                }
                break;
            }
        }
    }
}

/// Port of MergeCompanies (0x509293) with its GOSUB blocks.
///
/// Tax-free [runtime resource wording] of `target` into `acquirer` (ids 11..1600, no funds) at a premium of
/// `premium` % over the target's market price. `mode` 0: the player's merger (all dialogs: options
/// warning, banking commission, votes, reverse merger, government rulings, divestitures, cost messages);
/// 1: a computer player's buy-out (premium `premium`, or 32/62% with a 3% bump if < 1); 2: a corporate
/// merger (premium `premium` if > 0, else random 15..50, less 10 above 32 when more than 85% is public).
///
/// Steps: holders' tax bases ([`sum_holder_bases`]); merger cost (60% of the target's market cap, or 30%
/// of both if the target is larger, at most ~100 x `ExchangeRate`; reduced by the acquirer's stake; the
/// computer modes give up if it exceeds half the net worth); banking commission (bank mergers); the
/// target's market cap gets the premium; shareholder vote (holders of the target: players and the
/// player's companies by MSGBOX in mode 2, others by premium/credit/size rules; the public by a "no"
/// share from the target's country, sizes, premium, the acquirer's record and difficulty) — a failed
/// vote restores the market cap and charges 40% of the cost; antitrust review
/// (`AntitrustReviewAcquisition(1)`, 60%); reverse merger if the target owns more of the acquirer than
/// vice versa; holding slot for the target (else a government block, 70%, or — for the player — a forced
/// divestiture of a holding by sale or spin-off, 80% if refused); a forced divestiture of a 20..50%
/// holder controlled like the acquirer in the same industry (80% if refused); short covering and options
/// settlement; costs. Then the combination: the acquirer's share count, market cap (random 80..105% for
/// self-dealing / mode 1), earnings history, prior net worth and price history are rescaled by the
/// exchange ratio; options and convertible/alert prices adjusted; every holding of the target becomes a
/// holding of the acquirer (rounded; holders rounded to 0 are cashed out), holdings of the acquirer are
/// rescaled, double holdings merged; the acquirer holds 100% of the target (basis capped at the market
/// value); excess-loss recapture; index divisor; holdings trimmed to 100% ([`trim_holdings_to_100_pct`],
/// BUGS.md #220); finally for operating companies a capital infusion (large tax losses) and the
/// liquidation of the target into the acquirer (`LiquidateSubIntoParent`), cross-ownership checks.
///
/// `time_secs` is `VAL(RIGHT$(TIME$, 2))` for CalcStockPrice / SpinOffSubsidiaryMenu /
/// LiquidateSubIntoParent.
pub fn merge_companies(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    acquirer: i32,
    target: i32,
    mode: i32,
    premium: i16,
    time_secs: i32,
) {
    merge_companies_skipping(g, ui, rng, acquirer, target, mode, premium, time_secs, &[]);
}

/// [`merge_companies`] that does not call the routines in `skip` ([`LIQUIDATE_SUB_INTO_PARENT`],
/// [`SPIN_OFF_SUBSIDIARY_MENU`]), as when the differential tests skip them in the original.
pub fn merge_companies_skipping(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    acquirer: i32,
    target: i32,
    mode: i32,
    premium: i16,
    time_secs: i32,
    skip: &[u32],
) {
    let __content = crate::content::current();
    // ON ERROR GOTO 0x510a27 (ShowRuntimeError "ExecuteMerger", exit)
    let mut f = Frame::default();
    if merge_main(&mut f, g, ui, rng, acquirer, target, mode, premium, time_secs, skip).is_err() {
        let diag = s(__content.string(9684)) + &str_s(cast::<Ext, _>(f.acquirer_id)) + __content.string(9685) + &str_s(cast::<Ext, _>(f.target_id));
        show_runtime_error(g, ui, 9, &s(__content.string(9686)), &diag, 0);
    }
}

/// The body of MergeCompanies; `Err` = error 9 (to the ON ERROR handler).
fn merge_main(
    f: &mut Frame,
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    acquirer_arg: i32,
    target_arg: i32,
    mode_arg: i32,
    premium_arg: i16,
    time_secs: i32,
    skip: &[u32],
) -> Result<(), ()> {
    let __content = crate::content::current();
    let mut cashed_out_list = [0i32; 101]; // DIM (0 TO 100) AS LONG
    let mut acq_holder_list = [0i32; 101];
    let mut msg_text: PbStr;
    let mut msg_part: PbStr;
    let mut msg_word: PbStr;
    let mut adverb: PbStr;
    let mut asked_reverse = PbStr::new();
    let mut premium_pct: i16;
    let mut largest_holder_id: i32 = 0;
    let mut self_deal: i32 = 0;
    let mut owned_pct: i32 = 0;
    let mut answer: i32;
    let mut approved: i32;
    let mut no_vote_pct: i32;
    let mut combined_cap: Ext;

    if !(11 <= acquirer_arg && acquirer_arg <= 1600 && 11 <= target_arg && target_arg <= 1600) {
        return Ok(());
    }
    f.acquirer_id = acquirer_arg;
    f.target_id = target_arg;
    let mut target_mkt_cap = g.market_cap[f.target_id];
    g.valuation_mode = 0;
    calc_stock_price(g, ui, rng, f.target_id, time_secs);
    let target_price_str = g.stock_price_str.clone();
    f.merge_mode = mode_arg;
    if 10 < f.target_id && g.industry_num[f.target_id] == 71 {
        return Ok(());
    }
    if f.merge_mode == 0 {
        premium_pct = premium_arg;
    } else if f.merge_mode == 2 && 0 < premium_arg {
        premium_pct = premium_arg;
    } else {
        premium_pct = to_integer(rng.rnd() * Ext::from_f64(35.0) + Ext::from_f64(15.0));
        if 32 < premium_pct && f.merge_mode == 2 {
            update_corp_control(g, ui, rng, f.target_id);
            if 85 < g.public_pct {
                premium_pct = premium_pct.wrapping_sub(10);
            }
        }
        if f.merge_mode == 1 {
            premium_pct = premium_arg;
        }
    }
    let mut acquirer_name = g.ent_name[f.acquirer_id].clone();
    if f.acquirer_id == f.target_id {
        return Ok(());
    }
    let mut target_name = g.ent_name[f.target_id].clone();

    if f.merge_mode == 0 {
        msg_text = PbStr::new();
        if find_option_positions(g, ui, f.acquirer_id, f.target_id, 0, 0) != 0 {
            msg_text = s("X");
        }
        if find_option_positions(g, ui, g.cur_player, f.target_id, 0, 0) != 0 {
            msg_text = target_name.clone();
        }
        if !msg_text.is_empty() {
            if msg_text == "X" {
                msg_text = acquirer_name.clone()
                    + __content.string(9687)
                    + &target_name
                    + __content.string(9289)
                    + "\r"
                    + "\r"
                    + __content.string(9688)
                    + __content.string(9689)
                    + __content.string(9690)
                    + &target_name
                    + "."
                    + "\r"
                    + "\r";
            } else {
                msg_text = s(__content.string(9691))
                    + __content.string(2146)
                    + &msg_text
                    + "."
                    + "\r"
                    + "\r"
                    + __content.string(9692)
                    + __content.string(9689)
                    + __content.string(9690)
                    + &target_name
                    + "."
                    + "\r"
                    + "\r";
            }
            msg_text = msg_text + __content.string(9693);
            answer = ui.msgbox(&msg_text, 8228, &s(__content.string(9694)));
            if answer != 6 {
                show_quick_message(g, ui, &mut s(__content.string(5028)));
                return Ok(());
            }
        }
    }
    if f.acquirer_id < 11 {
        return Ok(());
    }
    sum_holder_bases(f, g, ui, rng);

    // ---- merger cost ----
    f.merger_cost = g.market_cap[f.target_id] * sf(0.6f32);
    if g.market_cap[f.acquirer_id] < g.market_cap[f.target_id] {
        f.merger_cost = (g.market_cap[f.target_id] + g.market_cap[f.acquirer_id]) * sf(0.3f32);
    }
    if g.exchange_rate * Ext::from_f64(100.0) < f.merger_cost {
        f.merger_cost = g.exchange_rate * Ext::from_f64(20.0) * rng.rnd() + g.exchange_rate * Ext::from_f64(90.0);
    }
    for slot_idx in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot_idx, f.acquirer_id)]) == f.target_id {
            combined_cap = Ext::from_f64(1.0) - sf(0.01f32) * cast::<Ext, _>(g.hold_stock_pct[(slot_idx, f.acquirer_id)]);
            f.merger_cost = f.merger_cost * combined_cap;
        }
    }
    f.attempt_suffix = s(__content.string(9695));
    calc_entity_financials(g, ui, rng, f.acquirer_id);
    if 0 < f.merge_mode && g.net_worth * sf(0.5f32) < f.merger_cost {
        return Ok(());
    }

    // ---- banking commission ----
    approved = 1;
    if g.industry_num[f.acquirer_id] == 1
        && g.net_worth * sf(0.5f32) < g.market_cap[f.target_id] * Ext::from_f64(100.0) + g.tmp_subsid_stock_value
        && f.merge_mode != 1
    {
        approved = 0;
    }
    if g.industry_num[f.target_id] == 1
        && g.industry_num[f.acquirer_id] == 1
        && (f.target_id + g.cur_year + g.cur_quarter + f.acquirer_id) % 6 != 1
        && f.merge_mode != 1
    {
        approved = -1;
    }
    if f.merge_mode == 0
        && g.industry_num[f.target_id] == 1
        && g.industry_num[f.acquirer_id] == 1
        && g.cur_player == g.bank_merger_player
    {
        approved = -1;
    }
    if g.industry_num[f.acquirer_id] == 1 && g.industry_num[f.target_id] == 1 && approved < 1 {
        if g.corp_controller[f.acquirer_id] != g.corp_controller[f.target_id] || approved == 0 {
            if f.merge_mode == 0 {
                msg_text = s(__content.string(9696))
                    + &acquirer_name
                    + " AND "
                    + &target_name
                    + __content.string(9697)
                    + "\r"
                    + "\r";
                if approved == 0 {
                    msg_text = msg_text + __content.string(9698);
                    if g.tmp_subsid_stock_value == Ext::from_f64(0.0) {
                        msg_text = msg_text + __content.string(9699);
                    } else {
                        msg_text = msg_text + __content.string(9700);
                    }
                    msg_text = msg_text + __content.string(9701);
                }
                ui.msgbox(&msg_text, 8192, &s(__content.string(9702)));
            } else {
                g.news_msg = s(__content.string(9703))
                    + &g.ent_symbol[f.acquirer_id]
                    + " and "
                    + &g.ent_symbol[f.target_id];
                if mid(g.news_headline[60].as_bytes(), 1, 7) != s("Banking") {
                    post_news(g, ui, f.acquirer_id, f.target_id);
                }
            }
            f.merger_cost = f.merger_cost * sf(0.2f32);
            charge_costs(f, g, ui);
            return Ok(());
        }
    }
    approved = 0;
    let _ = approved;
    if f.merge_mode == 0
        && g.cur_player == g.corp_controller[f.acquirer_id]
        && g.cur_player == g.corp_controller[f.target_id]
    {
        self_deal = 1;
    }

    // ---- premium on the target's market cap ----
    if f.merge_mode == 1 && premium_pct < 1 {
        g.market_cap[f.target_id] = sf(1.03f32) * g.market_cap[f.target_id];
        premium_pct = 32;
        if (cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(f.acquirer_id) || cast::<i32, _>(g.superfund_corp) == f.acquirer_id {
            premium_pct = 62;
        }
    } else {
        g.market_cap[f.target_id] = g.market_cap[f.target_id] * (sf(0.01f32) * (cast::<Ext, _>(premium_pct) + Ext::from_f64(100.0)));
    }

    // ---- shareholder vote ----
    let mut yes_pct: i16 = 100;
    g.public_pct = 0;
    g.rand_seed = g.rand_seed.wrapping_add(1);
    update_corp_control(g, ui, rng, f.target_id);
    if cast::<i32, _>(g.largest_holder) != f.acquirer_id {
        largest_holder_id = cast::<i32, _>(g.largest_holder);
    }
    let target_public_pct = cast::<i32, _>(g.public_pct);
    update_corp_control(g, ui, rng, f.acquirer_id);
    if f.merge_mode < 2 {
        for holder_id in 1..=1600 {
            let mut hold_slot = 1;
            while hold_slot < 16 {
                if cast::<i32, _>(g.hold_stock_id[(hold_slot, holder_id)]) == f.target_id
                    && 1 <= g.hold_stock_pct[(hold_slot, holder_id)]
                    && g.cur_player != holder_id
                    && !(1580 < holder_id && g.industry_num[holder_id] == 71)
                {
                    let pct = g.hold_stock_pct[(hold_slot, holder_id)];
                    if holder_id < 11 {
                        yes_pct = yes_pct.wrapping_sub(pct);
                    } else {
                        let voter_corp_id = holder_id;
                        update_corp_control(g, ui, rng, voter_corp_id);
                        if g.cur_player != g.corp_controller[voter_corp_id] {
                            let ctrl = g.corp_controller[voter_corp_id];
                            if 0 < ctrl && ctrl < 11 {
                                yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                            } else {
                                let k = (cast::<i32, _>(yes_pct))
                                    .wrapping_add(
                                        g.cur_quarter.wrapping_add(g.cur_year).wrapping_add(g.turn_action_count),
                                    )
                                    .wrapping_add(f.target_id)
                                    .wrapping_add(holder_id);
                                if (cast::<i32, _>(premium_pct)) < k % 55 + 20 {
                                    yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                                } else if !(2 < g.credit_rating_num[f.acquirer_id]
                                    && g.market_cap[f.target_id] * sf(0.2f32) <= g.market_cap[f.acquirer_id])
                                {
                                    yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                                }
                                hold_slot = 15;
                            }
                        }
                    }
                }
                hold_slot += 1;
            }
        }
    } else {
        for holder_id in 1..=1600 {
            let mut hold_slot = 1;
            while hold_slot < 16 {
                if !(cast::<i32, _>(g.hold_stock_id[(hold_slot, holder_id)]) == f.target_id
                    && 1 <= g.hold_stock_pct[(hold_slot, holder_id)])
                {
                    hold_slot += 1;
                    continue;
                }
                if 0 < holder_id && holder_id < 11 && g.cur_player != holder_id {
                    yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                    hold_slot = 16;
                    continue;
                }
                let voter_corp_id = holder_id;
                if g.cur_player == voter_corp_id {
                    g.corp_controller[holder_id] = g.cur_player;
                } else {
                    update_corp_control(g, ui, rng, voter_corp_id);
                }
                answer = 0;
                if g.cur_player == g.corp_controller[voter_corp_id] {
                    disable_main_controls(g);
                    msg_word = s("its");
                    if holder_id < 11 {
                        msg_word = s("his/her");
                    }
                    msg_text = acquirer_name.clone()
                        + " ("
                        + &g.ent_symbol[f.acquirer_id]
                        + __content.string(9704)
                        + __content.string(2146)
                        + &target_name
                        + " ("
                        + &g.ent_symbol[f.target_id]
                        + __content.string(9705)
                        + "at a"
                        + &PbStr::from(str_num(cast::<Ext, _>(premium_pct), 7).as_str())
                        + __content.string(9706)
                        + &target_price_str
                        + __content.string(1071)
                        + "\r"
                        + "\r"
                        + &acquirer_name
                        + __content.string(9707)
                        + &g.credit_rating_str
                        + ".' ";
                    if !(Ext::from_f64(0.0) < g.market_cap[f.target_id]) {
                        msg_part = s("\r") + "\r";
                    } else {
                        combined_cap = g.market_cap[f.acquirer_id] / target_mkt_cap;
                        if Ext::from_f64(10.0) < combined_cap {
                            combined_cap = combined_cap.floor();
                        }
                        msg_part = fmt_trim(combined_cap, b"##############")
                            + __content.string(9708)
                            + &g.ent_symbol[f.target_id]
                            + "'s";
                        if Ext::from_f64(1.0) < combined_cap && combined_cap < Ext::from_f64(2.0) {
                            msg_part = fmt_trim((combined_cap - Ext::from_f64(1.0)) * Ext::from_f64(100.0), b"####")
                                + __content.string(9709)
                                + &g.ent_symbol[f.target_id]
                                + "'s";
                        }
                        if combined_cap < Ext::from_f64(1.0) {
                            msg_part = fmt_trim(combined_cap * Ext::from_f64(100.0), b"####")
                                + __content.string(9710)
                                + &g.ent_symbol[f.target_id]
                                + "'s";
                        }
                        if (combined_cap - Ext::from_f64(1.0)).abs() < sf(0.1f32) {
                            msg_part = s(__content.string(9711)) + &g.ent_symbol[f.target_id];
                        }
                        msg_part = s(__content.string(9712)) + &msg_part;
                        let pct_change = price_pct_change_months(g, f.acquirer_id, 12);
                        if Ext::from_f64(10.0) < (cast::<Ext, _>(pct_change)).abs() {
                            msg_part = msg_part
                                + __content.string(9713)
                                + &number_to_words(pct_change, 0, 1)
                                + __content.string(1347);
                            if pct_change < 0 {
                                msg_part = replace(msg_part.as_bytes(), __content.bytes(9714), b"is down");
                            }
                        }
                        msg_part = msg_part + "." + "\r" + "\r";
                    }
                    if g.tax_loss_carry[f.acquirer_id] < -Ext::from_f64(300.0) * g.exchange_rate
                        && -Ext::from_f64(100.0) * g.exchange_rate < g.tax_loss_carry[f.target_id]
                    {
                        msg_part = msg_part
                            + __content.string(9715)
                            + &trim(g.currency_symbol.as_bytes())
                            + &fmt_trim(g.tax_loss_carry[f.acquirer_id].abs(), b"################")
                            + " "
                            + &g.million_prefix_lc
                            + "illion"
                            + &g.currency_suffix
                            + __content.string(9716)
                            + __content.string(9717)
                            + "\r"
                            + "\r";
                    }
                    msg_text = msg_text + &msg_part;
                    msg_part = g.ent_symbol[f.acquirer_id].clone() + " is";
                    if g.industry_num[f.acquirer_id] == g.industry_num[f.target_id]
                        && g.industry_num[f.acquirer_id] != 70
                    {
                        msg_part = s(__content.string(9718));
                    }
                    if g.industry_num[f.acquirer_id] == 70 {
                        msg_part = acquirer_name.clone() + __content.string(9719);
                    } else {
                        msg_part =
                            msg_part + __content.string(1110) + &g.ind_name[cast::<i32, _>(g.industry_num[f.acquirer_id])] + __content.string(1547);
                    }
                    msg_text = msg_text + &msg_part + "\r" + "\r";
                    msg_part = s("You own");
                    if 10 < holder_id {
                        msg_part = g.ent_name[holder_id].clone() + " owns";
                    }
                    msg_text =
                        msg_text + &msg_part + __content.string(9720) + &target_name + "." + "\r" + "\r";
                    msg_part = PbStr::new();
                    if Ext::from_f64(0.0) < g.net_worth {
                        msg_part = s(__content.string(9721));
                        let price_to_nw: Ext = (g.market_cap[f.acquirer_id] * Ext::from_f64(100.0)) / g.net_worth;
                        if price_to_nw < Ext::from_f64(100.0) {
                            msg_part = msg_part + &fmt_trim(price_to_nw * Ext::from_f64(100.0), b"#####") + __content.string(9722);
                        } else {
                            msg_part = msg_part + __content.string(9723);
                        }
                        msg_part = msg_part + __content.string(1071);
                    }
                    if !msg_part.is_empty() {
                        msg_text = msg_text + &msg_part + "\r" + "\r";
                    }
                    let mgmt = get_mgmt_margin_vs_industry(g, f.acquirer_id, 0);
                    if mgmt < 9 && -7 < mgmt {
                        adverb = PbStr::new();
                        msg_part = s(__content.string(9106));
                        if mgmt < 0 {
                            msg_part = s(__content.string(9107));
                        }
                    } else {
                        adverb = s("very ");
                        msg_part = s("capable");
                        if mgmt < -6 {
                            msg_part = s(__content.string(9108));
                        }
                        if mgmt < -10 {
                            adverb = s(__content.string(9109));
                        }
                    }
                    msg_part = g.ent_symbol[f.acquirer_id].clone()
                        + __content.string(9724)
                        + &adverb
                        + &msg_part
                        + ".";
                    if g.industry_num[f.acquirer_id] == 70 {
                        msg_part = PbStr::new();
                    }
                    if !msg_part.is_empty() {
                        msg_text = msg_text + &msg_part + "\r" + "\r";
                    }
                    msg_text = msg_text
                        + "Does "
                        + &g.ent_name[holder_id]
                        + __content.string(9725)
                        + &msg_word
                        + __content.string(4833)
                        + &target_name
                        + __content.string(9726)
                        + &trim(str_num(cast::<Ext, _>(g.hold_stock_pct[(hold_slot, holder_id)]), 7).as_bytes())
                        + "% ";
                    msg_text = msg_text + "of " + &g.ent_symbol[f.target_id] + __content.string(9727) + "\r" + "\r";
                    if (cast::<i32, _>(yes_pct)) - (cast::<i32, _>(g.hold_stock_pct[(hold_slot, holder_id)])) < 51 {
                        msg_text = msg_text + __content.string(9728);
                    }
                    answer = ui.msgbox(&msg_text, 8228, &s(__content.string(9729)));
                    enable_main_controls(g);
                }
                if answer != 6 {
                    if answer == 7 {
                        yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                    } else if !(1580 < holder_id && g.industry_num[holder_id] == 71) {
                        let ctrl = g.corp_controller[holder_id];
                        if 0 < ctrl && ctrl < 11 && g.corp_controller[f.acquirer_id] != g.corp_controller[holder_id] {
                            yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                            hold_slot = 15;
                        } else {
                            if g.corp_controller[f.acquirer_id] != g.corp_controller[voter_corp_id]
                                && f.acquirer_id != holder_id
                            {
                                let k = (cast::<i32, _>(yes_pct))
                                    .wrapping_add(
                                        g.cur_quarter.wrapping_add(g.cur_year).wrapping_add(g.turn_action_count),
                                    )
                                    .wrapping_add(f.target_id)
                                    .wrapping_add(holder_id);
                                if (cast::<i32, _>(premium_pct)) < k % 15 + 20 {
                                    yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                                } else if !(1 < g.credit_rating_num[f.acquirer_id]
                                    && g.market_cap[f.target_id] * sf(0.2f32) <= g.market_cap[f.acquirer_id])
                                    && -Ext::from_f64(500.0) * g.exchange_rate <= g.tax_loss_carry[f.acquirer_id]
                                {
                                    yes_pct = yes_pct.wrapping_sub(g.hold_stock_pct[(hold_slot, holder_id)]);
                                }
                            }
                            hold_slot = 15;
                        }
                    }
                }
                hold_slot += 1;
            }
        }
    }

    // ---- the public's "no" share ----
    no_vote_pct = (g.cur_quarter.wrapping_add(g.cur_year).wrapping_add(f.target_id)) % 30 + 10;
    if 1 < g.difficulty_level {
        no_vote_pct += g.difficulty_level * 2;
    }
    g.public_pct = cast::<i16, _>(target_public_pct);
    if g.market_cap[f.acquirer_id] < g.market_cap[f.target_id] * sf(0.45f32) {
        no_vote_pct += 55;
    }
    if g.market_cap[f.acquirer_id] < g.market_cap[f.target_id] * sf(0.3f32) {
        no_vote_pct += 10;
    }
    if g.market_cap[f.acquirer_id] < g.market_cap[f.target_id] * sf(0.15f32) {
        no_vote_pct += 15;
    }
    let sel = g.country_idx[f.target_id];
    if !(sel == 12
        || sel == 16
        || sel == 18
        || sel == 23
        || (30..=32).contains(&sel)
        || sel == 35
        || sel == 40
        || sel == 42
        || sel == 45
        || sel == 46)
    {
        if sel == 0 || sel == 2 || sel == 6 || sel == 50 {
            no_vote_pct = to_long(sf(0.8f32) * cast::<Ext, _>(no_vote_pct));
        } else {
            no_vote_pct += 5;
            if g.country_idx[f.acquirer_id] != g.country_idx[f.target_id] {
                no_vote_pct += 10;
            }
        }
    }
    if no_vote_pct < 50 && g.earnings_yr2_ago[f.acquirer_id] == Ext::from_f64(0.0) {
        no_vote_pct += 30;
    }
    let p = cast::<i32, _>(premium_pct);
    if (0..=4).contains(&p) {
        no_vote_pct += 75;
    } else if (5..=24).contains(&p) {
        no_vote_pct = (50 - p + no_vote_pct)
            + (g.cur_quarter
                .wrapping_add(g.turn_action_count)
                .wrapping_add(g.cur_year)
                .wrapping_add(f.target_id)
                .wrapping_add(f.acquirer_id))
                % 15;
    } else if (25..=40).contains(&p) {
        no_vote_pct = to_long(cast::<Ext, _>(45 - p) / Ext::from_f64(2.0) + cast::<Ext, _>(no_vote_pct));
    } else if (41..=100).contains(&p) {
        no_vote_pct += (g
            .turn_action_count
            .wrapping_add(g.cur_quarter)
            .wrapping_add(f.target_id)
            .wrapping_add(g.rand_seed))
            % 12;
    }
    if no_vote_pct < 5 {
        no_vote_pct = rng.rnd_range(5, 1);
    }
    if self_deal == 1 || cast::<i32, _>(g.merger_tip_corp) == f.target_id {
        no_vote_pct = to_long(sf(0.6f32) * cast::<Ext, _>(no_vote_pct));
    }
    calc_entity_financials(g, ui, rng, f.acquirer_id);
    if g.credit_rating_str == "D" {
        no_vote_pct = rng.rnd_range(96, 90);
    }
    if 99 < no_vote_pct {
        no_vote_pct = 99;
    }
    if f.merge_mode == 1
        && ((cast::<Ext, _>(g.asbestos_corp)).abs() == cast::<Ext, _>(f.acquirer_id) || cast::<i32, _>(g.superfund_corp) == f.acquirer_id)
        && g.rand_seed % 8 + 10 < no_vote_pct
    {
        no_vote_pct = g.rand_seed % 8 + 10;
    }
    g.public_pct = to_integer(cast::<Ext, _>(no_vote_pct) * sf(0.01f32) * cast::<Ext, _>(g.public_pct));
    yes_pct = yes_pct.wrapping_sub(g.public_pct);

    if yes_pct < 51 {
        if f.merge_mode == 0 {
            msg_text = s(__content.string(9730))
                + &PbStr::from(str_num(cast::<Ext, _>(yes_pct), 7).as_str())
                + "% OF "
                + &target_name
                + __content.string(9731);
            if yes_pct == 0 {
                msg_text = target_name.clone() + __content.string(9732);
            }
            if premium_pct < 15 && self_deal != 1 {
                msg_text = msg_text
                    + "\r"
                    + "\r"
                    + &target_name
                    + __content.string(9733)
                    + &g.ent_name[f.acquirer_id]
                    + ".";
            } else {
                msg_word = msg_text.clone()
                    + "\r"
                    + "\r"
                    + &target_name
                    + __content.string(9734);
                if g.credit_rating_str == "D" {
                    msg_text = msg_word + __content.string(9735);
                } else if !(target_mkt_cap * sf(0.75f32) <= g.market_cap[f.acquirer_id]) {
                    msg_text = msg_word + __content.string(9736);
                } else if g.earnings_yr2_ago[f.acquirer_id] == Ext::from_f64(0.0) {
                    msg_text = msg_word + __content.string(9737);
                }
            }
            ui.msgbox(
                &(msg_text.clone() + "\r" + "\r" + __content.string(9738)),
                8192,
                &s(__content.string(9739)),
            );
        }
        msg_text = g.ent_symbol[f.acquirer_id].clone()
            + __content.string(5271)
            + &target_name
            + __content.string(9740)
            + &g.ent_symbol[f.target_id]
            + __content.string(9741);
        if 68 < msg_text.len() {
            g.news_msg = g.ent_symbol[f.target_id].clone()
                + __content.string(9742)
                + &PbStr::from(str_num(cast::<Ext, _>(premium_pct), 7).as_str())
                + __content.string(9743)
                + &g.ent_symbol[f.acquirer_id];
            if rng.rnd() * Ext::from_f64(3.0) < Ext::from_f64(1.0) || premium_pct < 10 {
                g.news_msg = g.ent_symbol[f.target_id].clone()
                    + __content.string(9744)
                    + &g.ent_symbol[f.acquirer_id];
            }
        } else {
            g.news_msg = msg_text.clone();
        }
        // (modes 0 and 1: the headline is never posted, BUGS.md #222)
        if f.merge_mode == 2 {
            g.news_msg =
                g.ent_symbol[f.target_id].clone() + __content.string(9745) + &g.ent_symbol[f.acquirer_id] + " merger";
            if 0 < yes_pct {
                g.news_msg = g.news_msg.clone()
                    + __content.string(9746)
                    + &PbStr::from(str_num(cast::<Ext, _>(yes_pct), 7).as_str())
                    + __content.string(9747);
            }
            post_news(g, ui, f.acquirer_id, f.target_id);
        }
        g.market_cap[f.target_id] = target_mkt_cap;
        if f.merge_mode < 2 {
            g.turn_action_count = g.turn_action_count.wrapping_add(1);
        }
        f.merger_cost = f.merger_cost * sf(0.4f32);
        charge_costs(f, g, ui);
        return Ok(());
    }
    if f.merge_mode == 0 {
        msg_text = s(__content.string(9748))
            + &PbStr::from(str_num(cast::<Ext, _>(yes_pct), 7).as_str())
            + __content.string(9749)
            + &target_name;
        msg_text = msg_text + "\r" + "\r" + __content.string(9750);
        ui.msgbox(
            &(msg_text.clone() + "\r" + "\r" + __content.string(9751)),
            8192,
            &g.app_title.clone(),
        );
    } else {
        g.news_msg = g.ent_symbol[f.target_id].clone()
            + __content.string(9752)
            + &g.ent_symbol[f.acquirer_id]
            + " by"
            + &PbStr::from(str_num(cast::<Ext, _>(yes_pct), 7).as_str())
            + "% vote.";
        post_news(g, ui, f.acquirer_id, f.target_id);
    }
    // (targetIndustry = IndustryNum(targetId): not used)

    // ---- antitrust ----
    g.tmp_ext1 = cast::<Ext, _>(f.acquirer_id);
    g.tmp_ext2 = cast::<Ext, _>(f.target_id);
    g.tmp_ext3 = cast::<Ext, _>(f.merge_mode);
    approved = antitrust_review_acquisition(g, ui, rng, 1);
    if approved != 1 {
        // (the target's market cap keeps the premium: BUGS.md #221)
        f.merger_cost = f.merger_cost * sf(0.6f32);
        charge_costs(f, g, ui);
        return Ok(());
    }

    // ---- reverse merger? ----
    for slot in 1..=15 {
        f.acq_slot = find_holding_slot(g, f.acquirer_id, f.target_id);
        if 0 < f.acq_slot {
            owned_pct = cast::<i32, _>(g.hold_stock_pct[(f.acq_slot, f.acquirer_id)]);
        }
        if cast::<i32, _>(g.hold_stock_id[(slot, f.target_id)]) == f.acquirer_id {
            if g.hold_stock_pct[(slot, f.target_id)] == 100 {
                if f.merge_mode == 0 {
                    show_quick_message(g, ui,
                        &mut (acquirer_name.clone() + " and " + &target_name + __content.string(9753)),
                    );
                }
                return Ok(());
            }
            if owned_pct < cast::<i32, _>(g.hold_stock_pct[(slot, f.target_id)]) {
                if f.merge_mode == 0 {
                    if asked_reverse == "Y" {
                        answer = 7;
                    } else {
                        answer = ui.msgbox(
                            &(s(__content.string(3089))
                                + &target_name
                                + __content.string(9754)
                                + __content.string(9755)
                                + __content.string(9756)
                                + &acquirer_name
                                + "?"),
                            8228,
                            &s(__content.string(9757)),
                        );
                        asked_reverse = s("Y");
                    }
                } else {
                    answer = 6;
                }
                if answer == 6 {
                    // (acqHoldPct / acqHoldBasis are not swapped)
                    largest_holder_id = 0;
                    std::mem::swap(&mut f.acquirer_id, &mut f.target_id);
                    std::mem::swap(&mut acquirer_name, &mut target_name);
                    std::mem::swap(&mut f.tgt_basis_total, &mut f.acq_basis_total);
                    std::mem::swap(&mut f.tgt_excess_loss, &mut f.acq_excess_loss);
                    std::mem::swap(&mut f.acq_excess_holder, &mut f.tgt_excess_holder);
                    target_mkt_cap = g.market_cap[f.target_id];
                    f.reversed = s("Y");
                    owned_pct = 0;
                    f.acq_slot = find_holding_slot(g, f.acquirer_id, f.target_id);
                    if 0 < f.acq_slot {
                        owned_pct = cast::<i32, _>(g.hold_stock_pct[(f.acq_slot, f.acquirer_id)]);
                    }
                }
            }
        }
    }
    if owned_pct == 100 {
        if f.merge_mode == 0 {
            ui.msgbox(
                &(s("...")
                    + &acquirer_name
                    + __content.string(9758)
                    + "\r"
                    + "\r"
                    + "OF "
                    + &target_name
                    + __content.string(9759)),
                8208,
                &g.app_title.clone(),
            );
        }
        return Ok(());
    }

    // ---- government approval ----
    if 0 < f.acq_slot {
        if f.merge_mode == 0 {
            ui.msgbox(
                &(s(__content.string(9760))
                    + &acquirer_name
                    + " AND "
                    + &target_name
                    + __content.string(9761)),
                8256,
                &g.app_title.clone(),
            );
        }
        g.news_msg = acquirer_name.clone()
            + " ("
            + &g.ent_symbol[f.acquirer_id]
            + __content.string(9762)
            + &g.ent_symbol[f.target_id];
        post_news(g, ui, 0, 0);
        g.ctrl_change_msg_mode = 2;
        update_control_chain(g, ui, rng, f.target_id);
        update_control_chain(g, ui, rng, f.acquirer_id);
    } else if 0 < f.merge_mode || 1 < (f.target_id + g.cur_year + f.acquirer_id) % 3 {
        if f.merge_mode != 0 {
            msg_text = g.country_name[g.country_idx[f.target_id]].clone();
            if g.country_idx[f.target_id] == 0 {
                msg_text = s("U.S.");
            }
            // (headline set, never posted: BUGS.md #222)
            g.news_msg = msg_text
                + __content.string(9763)
                + &g.ent_symbol[f.acquirer_id]
                + " and "
                + &g.ent_symbol[f.target_id];
        } else {
            // (msgWord = "": every branch below sets it)
            msg_text = s(__content.string(9764))
                + &acquirer_name
                + " AND "
                + &target_name
                + __content.string(9765);
            let k = f.target_id + f.acquirer_id + g.cur_year;
            if g.country_idx[f.acquirer_id] == 0 || g.country_idx[f.target_id] == 0 {
                msg_word = s(__content.string(9766));
                if k % 4 == 1 {
                    msg_word = s(__content.string(9767));
                }
                if k % 4 == 2 {
                    msg_word = s(__content.string(9768));
                }
                if k % 4 == 3 {
                    msg_word = s(__content.string(9769));
                }
            } else {
                msg_word =
                    s(__content.string(9770)) + &g.country_name[g.country_idx[f.target_id]] + __content.string(9771);
            }
            if !msg_word.is_empty() {
                msg_text = msg_text + "\r" + "\r" + &msg_word;
            }
            ui.msgbox(&msg_text, 8256, &s(__content.string(2894)));
        }
        f.merger_cost = f.merger_cost * sf(0.7f32);
        charge_costs(f, g, ui);
        return Ok(());
    }

    // ---- forced divestiture to free a holding slot (player) ----
    approved = 0;
    if f.acq_slot == 0 && f.merge_mode == 0 {
        f.acq_slot = g.cur_year % 12 + 2;
        if cast::<i32, _>(g.hold_stock_id[(f.acq_slot, f.acquirer_id)]) == f.target_id {
            f.acq_slot += 1;
        }
        let divest_id = cast::<i32, _>(g.hold_stock_id[(f.acq_slot, f.acquirer_id)]);
        let divest_pct = cast::<i32, _>(g.hold_stock_pct[(f.acq_slot, f.acquirer_id)]);
        let gain_amt: i64 = to_quad(
            g.market_cap[divest_id] * cast::<Ext, _>(divest_pct) * sf(0.9f32) - g.hold_stock_basis[(f.acq_slot, f.acquirer_id)],
        );
        msg_word = s("gain");
        msg_part = s("taxable");
        if gain_amt < 0 {
            msg_word = s("loss");
        }
        if 79 < divest_pct {
            msg_part = s(__content.string(9772));
        }
        msg_text = s(__content.string(9773))
            + &acquirer_name
            + __content.string(9774)
            + &str_s(cast::<Ext, _>(divest_pct))
            + __content.string(9775)
            + &g.ent_name[divest_id]
            + __content.string(9776)
            + &msg_part
            + __content.string(9777)
            + "\r"
            + "\r";
        if 0 < gain_amt.wrapping_abs() {
            msg_text = msg_text
                + __content.string(9778)
                + &msg_word
                + " of "
                + &trim(g.currency_symbol.as_bytes())
                + &trim(g.currency_suffix.as_bytes())
                + &fmt_trim((cast::<Ext, _>(gain_amt)).abs(), b"* ###,")
                + " "
                + &g.million_prefix_lc
                + __content.string(9779)
                + "\r"
                + "\r";
        }
        msg_text = msg_text + __content.string(9780);
        answer = ui.msgbox(&msg_text, 8228, &s(__content.string(5053)));
        if answer != 6 {
            g.news_msg = s(__content.string(9781))
                + &g.ent_symbol[f.acquirer_id]
                + " and "
                + &g.ent_symbol[f.target_id]
                + __content.string(9782);
            post_news(g, ui, 0, 0);
            f.merger_cost = f.merger_cost * sf(0.8f32);
            charge_costs(f, g, ui);
            return Ok(());
        }
        g.news_msg = s(__content.string(9783))
            + &g.ent_symbol[f.acquirer_id]
            + __content.string(9784)
            + &g.ent_symbol[divest_id]
            + " in "
            + &g.ent_symbol[f.target_id]
            + __content.string(9785);
        post_news(g, ui, 0, 0);
        let mut spin_off_result = 0;
        if divest_id != f.acquirer_id {
            msg_text = s(__content.string(9786))
                + &g.ent_symbol[f.acquirer_id]
                + __content.string(9787)
                + &g.ent_symbol[divest_id]
                + __content.string(9788)
                + __content.string(9789);
            if 0 <= gain_amt {
                msg_word = s(__content.string(9790));
            } else {
                msg_word = s(__content.string(9791));
            }
            msg_text = msg_text
                + "\r"
                + "\r"
                + &msg_word
                + __content.string(9792)
                + __content.string(9793)
                + &g.ent_symbol[f.acquirer_id]
                + __content.string(9794)
                + __content.string(9795);
            if divest_pct < 80 {
                msg_text = msg_text + __content.string(9796);
            }
            answer = ui.msgbox(
                &msg_text,
                8484,
                &(s(__content.string(9797)) + &g.ent_symbol[divest_id] + "?"),
            );
            if answer == 6 {
                if !skip.contains(&SPIN_OFF_SUBSIDIARY_MENU) {
                    spin_off_subsidiary_menu(g, ui, rng, f.acquirer_id, divest_id, time_secs);
                }
                let k = find_holding_slot(g, f.acquirer_id, divest_id);
                if 0 < k && cast::<i32, _>(g.hold_stock_id[(k, f.acquirer_id)]) == divest_id {
                    msg_text = s(__content.string(9798))
                        + &g.ent_symbol[divest_id]
                        + __content.string(9799);
                    answer = ui.msgbox(
                        &(s(__content.string(9800))
                            + &g.ent_symbol[divest_id]
                            + __content.string(9801)),
                        8484,
                        &s(__content.string(9802)),
                    );
                    if answer == 6 {
                        msg_text = s(__content.string(9803))
                            + &g.ent_symbol[divest_id]
                            + __content.string(9804);
                        spin_off_result = -99;
                    }
                    show_quick_message(g, ui, &mut msg_text);
                } else {
                    spin_off_result = -1;
                }
            }
            if spin_off_result == -99 {
                g.news_msg = s(__content.string(9781))
                    + &g.ent_symbol[f.acquirer_id]
                    + " and "
                    + &g.ent_symbol[f.target_id]
                    + __content.string(9782);
                post_news(g, ui, 0, 0);
                f.merger_cost = f.merger_cost * sf(0.8f32);
                charge_costs(f, g, ui);
                return Ok(());
            }
        }
        if spin_off_result == 0 {
            divest_stock(g, ui, rng, divest_id, f.acquirer_id, 0);
        }
        approved = 1;
        if !g.news_msg.is_empty() {
            post_news(g, ui, 0, 0);
        }
    }

    // ---- forced divestiture by a blocking holder of the acquirer ----
    if approved == 0 {
        for holder_id in 11..=1600 {
            if (f.target_id + g.cur_year + f.acquirer_id) % 10 != 1 {
                continue;
            }
            let mut holder_slot = 1;
            while holder_slot < 16 {
                if cast::<i32, _>(g.hold_stock_id[(holder_slot, holder_id)]) != f.acquirer_id {
                    holder_slot += 1;
                    continue;
                }
                if g.hold_stock_pct[(holder_slot, holder_id)] < 20 || 50 < g.hold_stock_pct[(holder_slot, holder_id)] {
                    holder_slot = 16;
                    continue;
                }
                if g.corp_controller[f.acquirer_id] != g.corp_controller[holder_id] {
                    holder_slot += 1;
                    continue;
                }
                let blocker_id = holder_id;
                if g.rand_seed % 2 == 0 {
                    msg_word = s(__content.string(9805));
                } else {
                    msg_word = s(__content.string(9806));
                }
                if 0 < g.country_idx[f.acquirer_id] {
                    msg_text = g.country_name[g.country_idx[f.acquirer_id]].clone();
                    if msg_text.len() < 16 {
                        msg_word = s(__content.string(9807)) + &mcase(msg_text.as_bytes(), CaseMode::Ansi);
                    } else {
                        msg_word = s(__content.string(9808));
                    }
                }
                if f.target_id == blocker_id {
                    // (blockerId = 0)
                    holder_slot += 1;
                    continue;
                }
                if g.industry_num[blocker_id] != g.industry_num[f.target_id]
                    && g.industry_num[blocker_id] != g.industry_num[f.acquirer_id]
                {
                    holder_slot += 1;
                    continue;
                }
                if f.merge_mode == 0 {
                    answer = ui.msgbox(
                        &(s(__content.string(9809))
                            + &ucase(msg_word.as_bytes(), CaseMode::Ansi)
                            + " "
                            + __content.string(9810)
                            + &g.ent_symbol[blocker_id]
                            + __content.string(9811)
                            + &acquirer_name
                            + " "
                            + __content.string(9812)
                            + &acquirer_name
                            + " AND "
                            + &target_name
                            + "."
                            + "\r"
                            + "\r"
                            + __content.string(5052)),
                        8228,
                        &s(__content.string(5053)),
                    );
                } else if rng.rnd() * Ext::from_f64(5.0) <= Ext::from_f64(1.0) || f.merge_mode != 2 {
                    answer = 7;
                } else {
                    answer = 6;
                }
                if answer != 6 {
                    g.news_msg = s(__content.string(9781))
                        + &g.ent_symbol[f.acquirer_id]
                        + " and "
                        + &g.ent_symbol[f.target_id]
                        + __content.string(9782);
                    post_news(g, ui, 0, 0);
                    f.merger_cost = f.merger_cost * sf(0.8f32);
                    charge_costs(f, g, ui);
                    return Ok(());
                }
                g.news_msg =
                    s("The ") + &msg_word + __content.string(9813) + &g.ent_symbol[f.acquirer_id] + __content.string(9785);
                post_news(g, ui, 0, 0);
                divest_stock(g, ui, rng, f.acquirer_id, blocker_id, 0);
                holder_slot = 16;
            }
        }
    }

    // ---- shorts and options on the target ----
    if find_short_positions(g, ui, rng, f.target_id, 0) == 2 {
        force_cover_shorts(g, ui, rng, 1, 0, f.target_id, 0);
    }
    let num_players = g.num_players;
    for slot in 1..=num_players {
        if g.player_name[slot] == "...OUT..." {
            continue;
        }
        if 0 < find_stock_holding(g, slot, f.target_id) {
            let k = find_stock_holding(g, slot, f.acquirer_id);
            if k < 0 {
                let k = to_long((cast::<Ext, _>(k)).abs());
                let _other_corp_id = to_long((cast::<Ext, _>(g.hold_stock_pct[(k, slot)])).abs());
                force_cover_shorts(g, ui, rng, 1, slot, f.acquirer_id, 0);
            }
        }
    }
    if close_option_positions(g, ui, rng, 0, f.target_id, 1, 0) == 1 {
        g.news_msg = s(__content.string(2782))
            + &g.ent_symbol[f.target_id]
            + __content.string(9814)
            + &g.ent_symbol[f.acquirer_id]
            + " merger";
        calc_group_stake_pct(g, g.cur_player, f.target_id);
        if Ext::from_f64(0.0) < g.tmp_ext1 {
            show_quick_message(g, ui,
                &mut (g.news_msg.clone() + __content.string(9815)),
            );
        }
        g.tmp_ext1 = Ext::from_f64(0.0);
        post_news(g, ui, 0, 0);
    }
    f.attempt_suffix = PbStr::new();
    charge_costs(f, g, ui);
    if f.merge_mode == 0
        && g.industry_num[f.acquirer_id] == 1
        && g.industry_num[f.target_id] == 1
        && g.cur_player == g.corp_controller[f.acquirer_id]
        && -1 < g.bank_merger_player
    {
        g.bank_merger_player = g.cur_player;
    }

    // ---- the combination ----
    for slot in 1..=15 {
        if cast::<i32, _>(g.hold_stock_id[(slot, f.acquirer_id)]) == f.target_id {
            g.hold_stock_id[(slot, f.acquirer_id)] = 0;
            g.hold_stock_pct[(slot, f.acquirer_id)] = 0;
        }
    }
    if owned_pct == 100 {
        owned_pct = 99;
    }
    combined_cap = cast::<Ext, _>(100 - owned_pct) * g.market_cap[f.target_id] * sf(0.01f32) + g.market_cap[f.acquirer_id];
    let share_ratio: Ext = g.market_cap[f.acquirer_id] / combined_cap;
    adjust_options_for_merger(g, ui, f.acquirer_id, share_ratio);
    g.shares_outstanding[f.acquirer_id] = g.shares_outstanding[f.acquirer_id] / share_ratio;
    let new_holders_frac: Ext = Ext::from_f64(1.0) - share_ratio;
    if !(share_ratio == Ext::from_f64(0.0) || share_ratio.is_nan()) {
        // 100 * (1 / ratio) - 100 in 80 bits, stored as DOUBLE
        let hundred = Ext80::from_i64(100);
        let price_adj_pct = hundred
            .fmul(Ext80::from_i64(1).fdiv(Ext80::from_ext(share_ratio)))
            .fsub(hundred)
            .into_ext();
        adjust_conversion_price(g, f.acquirer_id, Ext::from_f64(0.0), Ext::from(price_adj_pct));
        adjust_price_alerts_for_stock(g, f.acquirer_id, Ext::from_f64(0.0), Ext::from(price_adj_pct));
    }
    if self_deal == 0 && f.merge_mode != 1 {
        g.market_cap[f.acquirer_id] = combined_cap;
    } else {
        g.market_cap[f.acquirer_id] = combined_cap * (rng.rnd() * sf(0.25f32) + sf(0.8f32));
    }
    let keep = cast::<Ext, _>(100 - owned_pct);
    g.earnings_y1[f.acquirer_id] = g.earnings_y1[f.acquirer_id] + keep * g.earnings_y1[f.target_id] * sf(0.01f32);
    g.earnings_yr2_ago[f.acquirer_id] =
        g.earnings_yr2_ago[f.acquirer_id] + keep * g.earnings_yr2_ago[f.target_id] * sf(0.01f32);
    g.earnings_yr3_ago[f.acquirer_id] =
        g.earnings_yr3_ago[f.acquirer_id] + keep * g.earnings_yr3_ago[f.target_id] * sf(0.01f32);
    g.earnings_yr4_ago[f.acquirer_id] =
        g.earnings_yr4_ago[f.acquirer_id] + keep * g.earnings_yr4_ago[f.target_id] * sf(0.01f32);
    g.earnings_ytd[f.acquirer_id] = g.earnings_ytd[f.acquirer_id] + keep * g.earnings_ytd[f.target_id] * sf(0.01f32);
    if Ext::from_f64(0.0) < g.corp_prior_net_worth[f.acquirer_id] {
        g.corp_prior_net_worth[f.acquirer_id] = g.corp_prior_net_worth[f.acquirer_id] / share_ratio;
    }
    let pct_scale: Ext = new_holders_frac / (Ext::from_f64(1.0) - cast::<Ext, _>(owned_pct) * sf(0.01f32));

    // holdings of the target become holdings of the acquirer; holdings of the acquirer are rescaled
    for holder_id in 1..=1600 {
        f.hold_value = Ext::from_f64(0.0);
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == f.target_id {
                g.hold_stock_id[(slot, holder_id)] = cast::<i16, _>(f.acquirer_id);
                f.hold_value = cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]) * g.market_cap[f.target_id];
                let new_pct = Ext80::from_i64(cast::<i64, _>(g.hold_stock_pct[(slot, holder_id)]))
                    .fmul(Ext80::from_ext(pct_scale))
                    .to_i64();
                g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(new_pct);
                if g.hold_stock_pct[(slot, holder_id)] == 0 {
                    g.cash[holder_id] = g.cash[holder_id] + f.hold_value;
                    g.cash[f.acquirer_id] = g.cash[f.acquirer_id] - f.hold_value;
                    g.hold_stock_id[(slot, holder_id)] = 0;
                    if g.num_players < holder_id {
                        g.extraordinary_pl[holder_id] = to_quad(
                            cast::<Ext, _>(g.extraordinary_pl[holder_id])
                                + (f.hold_value - g.hold_stock_basis[(slot, holder_id)]),
                        );
                    } else {
                        if f.hold_value < Ext::from_f64(0.0) {
                            g.hold_stock_basis[(slot, holder_id)] = -g.hold_stock_basis[(slot, holder_id)];
                        }
                        g.player_cap_gains[holder_id] =
                            g.player_cap_gains[holder_id] + (f.hold_value - g.hold_stock_basis[(slot, holder_id)]);
                    }
                    g.hold_stock_basis[(slot, holder_id)] = Ext::from_f64(0.0);
                } else {
                    f.hold_value = Ext::from_f64(0.0);
                    if let Some(e) = cashed_out_list[1..=100].iter_mut().find(|e| **e == 0) {
                        *e = holder_id;
                    }
                }
            } else if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == f.acquirer_id {
                f.hold_value = cast::<Ext, _>(g.hold_stock_pct[(slot, holder_id)]) * g.market_cap[f.acquirer_id] * share_ratio;
                let new_pct = Ext80::from_i64(cast::<i64, _>(g.hold_stock_pct[(slot, holder_id)]))
                    .fmul(Ext80::from_ext(share_ratio))
                    .to_i64();
                g.hold_stock_pct[(slot, holder_id)] = cast::<i16, _>(new_pct);
                if g.hold_stock_pct[(slot, holder_id)] == 0 {
                    g.cash[holder_id] = g.cash[holder_id] + f.hold_value;
                    g.cash[f.acquirer_id] = g.cash[f.acquirer_id] - f.hold_value;
                    g.hold_stock_id[(slot, holder_id)] = 0;
                    if g.num_players < holder_id {
                        g.extraordinary_pl[holder_id] = to_quad(
                            cast::<Ext, _>(g.extraordinary_pl[holder_id])
                                + (f.hold_value - g.hold_stock_basis[(slot, holder_id)]),
                        );
                    } else {
                        g.player_cap_gains[holder_id] =
                            g.player_cap_gains[holder_id] + (f.hold_value - g.hold_stock_basis[(slot, holder_id)]);
                    }
                    g.hold_stock_basis[(slot, holder_id)] = Ext::from_f64(0.0);
                } else {
                    f.hold_value = Ext::from_f64(0.0);
                    if let Some(e) = acq_holder_list[1..=100].iter_mut().find(|e| **e == 0) {
                        *e = holder_id;
                    }
                }
            }
        }
        if Ext::from_f64(0.0) < f.hold_value {
            g.news_msg = g.ent_symbol[holder_id].clone()
                + __content.string(9816)
                + &g.ent_symbol[f.acquirer_id]
                + __content.string(5271)
                + &g.ent_symbol[f.target_id];
            let gain_amt = to_quad(f.hold_value);
            if 0 < gain_amt {
                g.news_msg = g.ent_symbol[holder_id].clone()
                    + __content.string(9817)
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(cast::<Ext, _>(gain_amt), b"* ###,")
                    + " "
                    + &g.million_prefix_lc
                    + "il. in "
                    + &g.ent_symbol[f.acquirer_id]
                    + __content.string(5271)
                    + &g.ent_symbol[f.target_id];
            }
            post_news(g, ui, holder_id, 0);
        }
    }
    // holders of both stocks: the two holdings of the acquirer are merged into the second slot
    for &holder in &acq_holder_list[1..=100] {
        if holder == 0 {
            continue;
        }
        for &cashed_out in &cashed_out_list[1..=100] {
            if holder != cashed_out {
                continue;
            }
            let mut merged_pct: i32 = 0;
            let mut merged_basis: Ext = Ext::from_f64(0.0);
            let mut slot_idx = 1;
            while slot_idx < 16 {
                if cast::<i32, _>(g.hold_stock_id[(slot_idx, holder)]) == f.acquirer_id {
                    if merged_pct == 0 {
                        g.hold_stock_id[(slot_idx, holder)] = 0;
                        merged_pct = cast::<i32, _>(g.hold_stock_pct[(slot_idx, holder)]);
                        g.hold_stock_pct[(slot_idx, holder)] = 0;
                        merged_basis = g.hold_stock_basis[(slot_idx, holder)];
                        g.hold_stock_basis[(slot_idx, holder)] = Ext::from_f64(0.0);
                    } else {
                        g.hold_stock_pct[(slot_idx, holder)] =
                            cast::<i16, _>(merged_pct + cast::<i32, _>(g.hold_stock_pct[(slot_idx, holder)]));
                        g.hold_stock_basis[(slot_idx, holder)] = merged_basis + g.hold_stock_basis[(slot_idx, holder)];
                        merged_basis = Ext::from_f64(0.0);
                        slot_idx = 15;
                    }
                }
                slot_idx += 1;
            }
            let _ = merged_basis;
        }
    }
    if 0 < largest_holder_id {
        let k = find_holding_slot(g, largest_holder_id, f.acquirer_id);
        if cast::<i32, _>(g.hold_stock_id[(k, largest_holder_id)]) == f.acquirer_id {
            g.news_msg = g.ent_symbol[largest_holder_id].clone()
                + __content.string(9818)
                + &g.ent_symbol[f.acquirer_id]
                + __content.string(9819)
                + &g.ent_symbol[f.acquirer_id]
                + " and "
                + &g.ent_symbol[f.target_id];
            if 20 < g.ent_symbol[largest_holder_id].len() {
                g.news_msg = replace(g.news_msg.as_bytes(), __content.bytes(9820), b"gets");
            }
            post_news(g, ui, 0, 0);
        }
    }
    // the acquirer holds 100% of the target
    g.hold_stock_id[(f.acq_slot, f.acquirer_id)] = cast::<i16, _>(f.target_id);
    g.hold_stock_pct[(f.acq_slot, f.acquirer_id)] = 100;
    if f.acq_hold_pct == 0 {
        if g.market_cap[f.target_id] * Ext::from_f64(100.0) < f.tgt_basis_total {
            f.tgt_basis_total = g.market_cap[f.target_id] * Ext::from_f64(100.0);
        }
    } else {
        f.acq_hold_basis = g.market_cap[f.target_id] * cast::<Ext, _>(100 - cast::<i32, _>(f.acq_hold_pct)) + f.acq_hold_basis;
        if f.acq_hold_basis < f.tgt_basis_total {
            f.tgt_basis_total = f.acq_hold_basis;
        }
    }
    g.hold_stock_basis[(f.acq_slot, f.acquirer_id)] = f.tgt_basis_total;
    if Ext::from_f64(0.0) < f.tgt_excess_loss && 10 < f.tgt_excess_holder {
        let h = f.tgt_excess_holder;
        for i in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(i, h)]) == f.acquirer_id && g.hold_stock_pct[(i, h)] < 80 {
                g.accrued_pl[h] = g.accrued_pl[h] + f.tgt_excess_loss;
                g.hold_stock_basis[(i, h)] = Ext::from_f64(0.0);
                g.news_msg = g.ent_symbol[h].clone()
                    + __content.string(9821)
                    + &g.ent_symbol[f.target_id]
                    + " stock.";
                post_news(g, ui, f.target_id, h);
            }
        }
    }
    g.market_cap[f.target_id] = target_mkt_cap;
    adjust_index_divisor(g, f.target_id, 100 - owned_pct);
    for slot_idx in 1..=60 {
        g.cap_hist_close[(slot_idx, f.acquirer_id)] = g.cap_hist_close[(slot_idx, f.acquirer_id)] / share_ratio;
        g.price_hist_high[(slot_idx, f.acquirer_id)] = g.price_hist_high[(slot_idx, f.acquirer_id)] / share_ratio;
        g.price_hist_low[(slot_idx, f.acquirer_id)] = g.price_hist_low[(slot_idx, f.acquirer_id)] / share_ratio;
    }
    g.valuation_mode = 2;
    calc_stock_price(g, ui, rng, f.target_id, time_secs);
    trim_holdings_to_100_pct(f, g)?;
    if f.merge_mode < 2 {
        g.turn_action_count = g.turn_action_count.wrapping_add(1);
    }

    // ---- operating companies: tax losses and liquidation of the target ----
    let ind_a = g.industry_num[f.acquirer_id];
    let ind_t = g.industry_num[f.target_id];
    if ind_a < 3 || ind_t < 3 {
        return Ok(());
    }
    if ind_t != ind_a {
        if ind_a != 70 && ind_t != 70 {
            return Ok(());
        }
        if f.merge_mode == 2 && ind_a == 70 && g.rand_seed % 10 != 7 {
            return Ok(());
        }
        if g.tax_loss_carry[f.target_id] < -Ext::from_f64(10.0) {
            return Ok(());
        }
    }
    if 0 < f.merge_mode && g.tax_loss_carry[f.target_id] < -Ext::from_f64(10.0) && Ext::from_f64(1.0) < rng.rnd() * Ext::from_f64(10.0) {
        if g.industry_num[f.acquirer_id] == 70 || g.industry_num[f.target_id] == 70 {
            return Ok(());
        }
    }
    if 0 < f.merge_mode
        && g.tax_loss_carry[f.target_id] < -Ext::from_f64(1000.0) * g.exchange_rate
        && -Ext::from_f64(500.0) * g.exchange_rate < g.tax_loss_carry[f.acquirer_id]
    {
        capital_infusion(g, ui, rng, f.acquirer_id, f.target_id);
    }
    g.tmp_ext1 = cast::<Ext, _>(f.merge_mode);
    if 130 < f.target_id && !skip.contains(&LIQUIDATE_SUB_INTO_PARENT) {
        liquidate_sub_into_parent(g, ui, rng, f.acquirer_id, f.target_id, time_secs);
    }
    recapture_excess_loss(g, ui, f.acquirer_id, 0);
    if 130 < f.target_id {
        check_cross_ownership(g, ui, rng, f.target_id, 1);
    }
    check_cross_ownership(g, ui, rng, f.acquirer_id, 1);
    Ok(())
}
