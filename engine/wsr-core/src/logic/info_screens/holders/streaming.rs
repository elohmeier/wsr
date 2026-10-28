//! The streaming-quotes list buttons: fill the list with the player's holdings, clear it.
use crate::pb::real::{Ext, cast};


use crate::logic::control::find_option_positions;
use crate::logic::market::rebuild_streaming_quotes_panel;
use crate::logic::report::show_quick_message;
use crate::logic::reports_lists::{pb_array_sort, s};
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::valuation::calc_stock_price;
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, parse_any};
use crate::pb::numeric::{int, to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// Port of ClearStreamingQuotesList (0x5dea80), state effects only.
/// Empties the current player's streaming-quote list (`WatchList`, `StreamPrevPriceStr`,
/// `StreamCurPrice`, `StreamPriceDir` 1..15) after a YES/NO confirmation when it has at least 5 entries;
/// remembers a selected company (`SelEntity > 10`) as `LastAutoAddedQuote` (the previous one as
/// `PrevPickedEntity`) so it is not auto-added again at once. Nothing happens without a current player,
/// game, or after the game's end. The quote labels (`CONTROL KILL` 801..815 / 901..915) are UI.
pub fn clear_streaming_quotes_list(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    if g.cur_player == 0 || g.game_in_progress == 0 || g.game_end_flag == "Y" {
        return;
    }
    let p = g.cur_player;
    if 10 < g.watch_list[(5, p)] {
        let msg_text = s(__content.string(6212)) + __content.string(6213);
        let answer = ui.msgbox(&msg_text, 8227, &s(__content.string(6214)));
        if answer != 6 {
            return;
        }
    }
    for slot in 1..=15 {
        g.watch_list[(slot, p)] = 0;
        g.stream_prev_price_str[(slot, p)] = PbStr::new();
        g.stream_cur_price[(slot, p)] = PbStr::new();
        g.stream_price_dir[(slot, p)] = 0;
        // (UI) CONTROL KILL hDlgMain, slot + 800 / slot + 900
    }
    if 10 < g.sel_entity {
        g.prev_picked_entity = g.last_auto_added_quote;
        g.last_auto_added_quote = g.sel_entity;
    }
}

/// The locals of FillStreamingListWithHoldings shared with its GOSUBs.
#[derive(Default)]
struct FillFrame {
    corp_id: i32,
    not_listed: i32,
    added_count: i32,
    has_short: i32,
    ctrl_count: i32,
    corp_list: Vec<i32>,
    cap_keys: Vec<i64>,
}

/// Port of FillStreamingListWithHoldings (0x5dec84).
/// Adds to the current player's streaming-quote list (free slots, in this order): the stocks he holds
/// (long or short), the companies he controls (largest market cap first), the issuers of convertible
/// bonds he holds, the underlyings of his option positions and the funds (1581..1600) managed by a
/// company he controls; each new entry gets its price ([`calc_stock_price`] `ValuationMode = 0`) and a
/// random direction (`INT(RND * 3) - 1`). Runs [`pack_holding_arrays`] first. Without stocks, options
/// or convertible bonds only a popup is shown; a full list (slot 15 used) is cleared after a YES/NO
/// question ([`clear_streaming_quotes_list`]). Ends with [`rebuild_streaming_quotes_panel`] (mode 2)
/// and a popup ("... were added to the list" / "...  [runtime resource wording] ").
///
/// When the 15th slot gets filled the rest is skipped (the GOSUB jumps to the final message; the panel is
/// rebuilt twice then).
pub fn fill_streaming_list_with_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut f = FillFrame { corp_list: vec![0; 1601], cap_keys: vec![0; 1601], ..FillFrame::default() };
    if g.cur_player == 0 || g.game_in_progress == 0 || g.game_end_flag == "Y" {
        return fill_streaming_list_exit();
    }
    pack_holding_arrays(g, ui);
    if g.hold_stock_id[(0, g.cur_player)] == 0 && find_option_positions(g, ui, g.cur_player, 0, 0, 0) == 0 {
        let mut has_cv = false;
        for idx in 1..=15 {
            if g.hold_bond_id[(idx, g.cur_player)] != 0 {
                let scan_idx = g.hold_bond_id[(idx, g.cur_player)];
                if 0 < instr(1, g.bond_issue_info[scan_idx].as_bytes(), b"cv") {
                    has_cv = true;
                    break;
                }
            }
        }
        if !has_cv {
            show_quick_message(g, ui, &mut s(__content.string(6215)));
            return fill_streaming_list_exit();
        }
    }
    if 10 < g.watch_list[(15, g.cur_player)] {
        let msg_text = s(__content.string(6216));
        let answer = ui.msgbox(&msg_text, 8227, &s(__content.string(6214)));
        if answer != 6 {
            return fill_streaming_list_exit();
        }
        clear_streaming_quotes_list(g, ui);
        if 10 < g.watch_list[(15, g.cur_player)] {
            return fill_streaming_list_exit();
        }
    }
    let player_id = g.cur_player;
    for idx in 1..=15 {
        if 10 < g.hold_stock_id[(idx, player_id)] {
            f.corp_id = cast::<i32, _>(g.hold_stock_id[(idx, player_id)]);
            fill_streaming_list_is_listed(g, &mut f);
            if f.not_listed != 0 {
                if fill_streaming_list_add_entries(g, ui, rng, &mut f) {
                    return fill_streaming_list_final(g, ui, &f);
                }
                if g.hold_stock_pct[(idx, player_id)] < 0 {
                    f.has_short = 1;
                }
            }
        }
    }
    fill_streaming_list_collect_controlled(g, &mut f);
    // ARRAY SORT capKeys(1) FOR ctrlCount, TAGARRAY corpList(), DESCEND
    let n = cast::<usize, _>(f.ctrl_count.max(0));
    pb_array_sort(&mut f.cap_keys[1..1 + n], Some(&mut f.corp_list[1..1 + n]), true, &|a: &i64, b: &i64| a < b);
    let item_count = f.ctrl_count;
    for idx in 1..=item_count {
        f.corp_id = f.corp_list[cast::<usize, _>(idx)];
        fill_streaming_list_is_listed(g, &mut f);
        if f.not_listed != 0 && fill_streaming_list_add_entries(g, ui, rng, &mut f) {
            return fill_streaming_list_final(g, ui, &f);
        }
    }
    for idx in 1..=15 {
        if 10 < g.hold_bond_id[(idx, player_id)] {
            f.corp_id = g.hold_bond_id[(idx, player_id)];
            if instr(1, g.bond_issue_info[f.corp_id].as_bytes(), b"cv") != 0 {
                fill_streaming_list_is_listed(g, &mut f);
                if f.not_listed != 0 && fill_streaming_list_add_entries(g, ui, rng, &mut f) {
                    return fill_streaming_list_final(g, ui, &f);
                }
            }
        }
    }
    let item_count = to_long(val(g.option_recs[0].as_bytes()));
    for idx in 1..=item_count {
        let opt_rec = g.option_recs[idx].clone();
        if opt_rec == "" {
            continue;
        }
        if val(parse_any(opt_rec.as_bytes(), b"|", 3).as_bytes()) == cast::<Ext, _>(g.cur_player) {
            if val(parse_any(opt_rec.as_bytes(), b"|", 4).as_bytes()) < Ext::from_f64(0.0) {
                f.has_short = 1;
            }
            f.corp_id = to_long(val(parse_any(opt_rec.as_bytes(), b"|", 1).as_bytes()));
            fill_streaming_list_is_listed(g, &mut f);
            if f.not_listed != 0 && fill_streaming_list_add_entries(g, ui, rng, &mut f) {
                return fill_streaming_list_final(g, ui, &f);
            }
        }
    }
    for idx in 1581..=1600 {
        if g.entity_status[idx] == 1 && g.industry_num[idx] == 71 {
            let adviser_id = g.fund_adviser[idx];
            if 85 < adviser_id && adviser_id < 1601 && g.cur_player == g.corp_controller[adviser_id] {
                f.corp_id = idx;
                fill_streaming_list_is_listed(g, &mut f);
                if f.not_listed != 0 && fill_streaming_list_add_entries(g, ui, rng, &mut f) {
                    return fill_streaming_list_final(g, ui, &f);
                }
            }
        }
    }
    fill_streaming_list_final(g, ui, &f)
}

/// The closing message of [`fill_streaming_list_with_holdings`] (label 0x5df35e): rebuilds the panel
/// when something was added and shows the result popup.
fn fill_streaming_list_final(g: &mut Globals, ui: &mut dyn Ui, f: &FillFrame) {
    let __content = crate::content::current();
    let mut msg_text;
    if f.added_count < 1 {
        msg_text = s(__content.string(6217))
            + __content.string(6218)
            + __content.string(6219);
    } else {
        rebuild_streaming_quotes_panel(g, ui, 2);
        let mut short_text = PbStr::new();
        if f.has_short == 1 {
            short_text = s(__content.string(6220));
        }
        msg_text = s(__content.string(6221))
            + &short_text
            + __content.string(6222)
            + __content.string(6223)
            + __content.string(6224);
        if g.watch_list[(15, g.cur_player)] < 11 {
            msg_text = msg_text + ".";
        } else {
            msg_text = msg_text + __content.string(6225);
        }
    }
    show_quick_message(g, ui, &mut msg_text);
    fill_streaming_list_exit()
}

/// Port of FillStreamingList_IsListed (0x5df4ad), GOSUB of [`fill_streaming_list_with_holdings`]:
/// `notListed = 0` when `corpId` is already in `WatchList(1..15, CurPlayer)`, else 1.
fn fill_streaming_list_is_listed(g: &Globals, f: &mut FillFrame) {
    f.not_listed = 1;
    for scan_idx in 1..=15 {
        if f.corp_id == g.watch_list[(scan_idx, g.cur_player)] {
            f.not_listed = 0;
        }
    }
}

/// Port of FillStreamingList_AddEntries (0x5df502), GOSUB of [`fill_streaming_list_with_holdings`]:
/// puts `corpId` into the first free slot of the current player's list (price via [`calc_stock_price`]
/// in `ValuationMode = 0`, random direction -1..1). Filling slot 15 rebuilds the panel (mode 2) and
/// jumps to the routine's closing message: returns `true` then.
fn fill_streaming_list_add_entries(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, f: &mut FillFrame) -> bool {
    let p = g.cur_player;
    let mut free_slot = 1;
    while free_slot < 16 {
        if g.watch_list[(free_slot, p)] < 11 {
            g.valuation_mode = 0;
            calc_stock_price(g, ui, rng, f.corp_id, 0); // mode 0 does not read TIME$
            g.watch_list[(free_slot, p)] = f.corp_id;
            g.stream_prev_price_str[(free_slot, p)] = g.stock_price_str.clone();
            g.stream_cur_price[(free_slot, p)] = g.stock_price_str.clone();
            let rnd_val = to_quad(int(rng.rnd() * Ext::from_f64(3.0)) + -Ext::from_f64(1.0));
            g.stream_price_dir[(free_slot, p)] = cast::<i32, _>(rnd_val);
            f.added_count = f.added_count + 1;
            if free_slot == 15 {
                rebuild_streaming_quotes_panel(g, ui, 2);
                return true;
            }
            free_slot = 15;
        }
        free_slot = free_slot + 1;
    }
    false
}

/// Port of FillStreamingList_CollectControlled (0x5df646), GOSUB of
/// [`fill_streaming_list_with_holdings`]: collects the active companies the current player controls
/// (`corpList`) with their market cap as sort key (`capKeys`, QUAD).
fn fill_streaming_list_collect_controlled(g: &Globals, f: &mut FillFrame) {
    for scan_idx in 11..=1600 {
        if g.entity_status[scan_idx] == 1 && g.cur_player == g.corp_controller[scan_idx] {
            f.ctrl_count = f.ctrl_count + 1;
            f.corp_list[cast::<usize, _>(f.ctrl_count)] = scan_idx;
            f.cap_keys[cast::<usize, _>(f.ctrl_count)] = to_quad(g.market_cap[scan_idx]);
        }
    }
}

/// Port of FillStreamingList_Exit (0x5df708): the routine's exit label (frame cleanup only).
fn fill_streaming_list_exit() {}
