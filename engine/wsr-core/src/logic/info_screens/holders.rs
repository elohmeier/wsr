//! The "who owns what" holder lists (futures / physical commodities, interest rate swaps, options, stock
//! cross-holdings; called from DlgWhoOwnsWhat_Go), the streaming-quotes list fill / clear buttons and
//! the research report's rating click.
//!
//! The lists are text reports with `@C<id>` links in `"ChangeCorp"` pick mode (a click makes the holder
//! the active entity, see [`crate::logic::trading::follow_report_link`]).

// Statements keep the original BASIC shape on purpose.
#![allow(
    clippy::assign_op_pattern,
    clippy::collapsible_if,
    clippy::collapsible_else_if,
    clippy::needless_late_init,
    clippy::neg_cmp_op_on_partial_ord
)]
use crate::pb::real::{Ext, cast};


mod rating_click;
mod streaming;

pub use rating_click::*;
pub use streaming::*;

use crate::logic::control::update_corp_control;
use crate::logic::report::{show_quick_message, show_text_report};
use crate::logic::reports_lists::{fmt_trim, link_id, pb_array_sort, s, str15, str_trim};
use crate::logic::valuation::calc_analyst_rating;
use crate::pb::ext80::Ext80;
use crate::pb::numeric::format_ext;
use crate::pb::numeric::{val, val_ext};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{chr, compare, instr, ltrim, mcase, mid, parse_any, replace, right, rtrim, string_fill, CaseMode};
use crate::pb::numeric::{to_integer, to_long, to_quad, Cux};
use crate::state::Globals;
use crate::ui::Ui;

/// `RTRIM$(LTRIM$(x))`.
fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// `PARSE$(rec, ANY "|", n)`.
fn field(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `ARRAY SORT lines(start) FOR count` (string elements, binary compare).
fn sort_lines(lines: &mut [PbStr], start: i32, count: i32) {
    let (a, n) = (cast::<usize, _>(start), cast::<usize, _>(count.max(0)));
    pb_array_sort::<PbStr, ()>(&mut lines[a..a + n], None::<&mut [()]>, false, &|x, y| {
        compare(x.as_bytes(), y.as_bytes()) == std::cmp::Ordering::Less
    });
}

/// `" [runtime resource wording] <what> AS "ACTIVE ENTITY")"` with `indent` leading blanks.
fn click_line(indent: i32, what: &str) -> PbStr {
    let __content = crate::content::current();
    string_fill(indent, b" ") + __content.string(6111) + what + " AS " + &chr(&[34]) + __content.string(6112) + &chr(&[34]) + ")"
}

/// The locals of ListFuturesOrPhysHolders shared with its GOSUBs.
struct FutFrame {
    lines: Vec<PbStr>,
    separator: PbStr,
    rec_text: PbStr,
    commod_name: PbStr,
    qty: i32,
    unit_price: Cux,
}

/// Port of ListFuturesOrPhysHolders (0x670040).
/// "Who owns what" list of the futures positions (`list_type = 1`, `FuturesRecs()`) or physical commodity
/// holdings (`list_type = 2`, `PhysCommodRec()`): asks (YES/NO/CANCEL) whether to list only the
/// positions of the current player and the companies he controls, then shows holder, commodity,
/// contracts (long/short) or units and the contract price / cost per unit (in U.S. $), sorted by name,
/// with `@C` links to the holders. Without records, or without any matching position, only a popup is
/// shown.
pub fn list_futures_or_phys_holders(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, list_type: i32) {
    let __content = crate::content::current();
    let mut f = FutFrame {
        lines: vec![PbStr::new(); 1501],
        separator: PbStr::new(),
        rec_text: PbStr::new(),
        commod_name: PbStr::new(),
        qty: 0,
        unit_price: Cux(0),
    };
    let pos_desc;
    let pos_title;
    if list_type == 1 {
        if val(g.futures_recs[0].as_bytes()) == Ext::from_f64(0.0) {
            show_quick_message(g, ui, &mut s(__content.string(6113)));
            return list_futures_or_phys_holders_exit();
        }
        pos_desc = s(__content.string(6114));
        pos_title = s("Futures");
    } else {
        if val(g.phys_commod_rec[0].as_bytes()) == Ext::from_f64(0.0) {
            show_quick_message(g, ui, &mut s(__content.string(6115)));
            return list_futures_or_phys_holders_exit();
        }
        pos_desc = s(__content.string(6116));
        pos_title = s(__content.string(6117));
    }
    let msg_text = s(__content.string(6118))
        + &pos_desc
        + __content.string(6119)
        + "\r"
        + "\r"
        + __content.string(6120)
        + &pos_desc
        + __content.string(6121);
    let answer = ui.msgbox(&msg_text, 8483, &(s(__content.string(6122)) + &pos_title + __content.string(6123)));
    if answer == 2 {
        return list_futures_or_phys_holders_exit();
    }
    let mut only_mine = PbStr::new();
    if answer == 6 {
        only_mine = s("Y");
    }
    let fmt_money = "* ##############.00";
    list_futures_or_phys_header(&mut f, list_type);
    let mut line_no = 3;
    let mut any_found = 0;
    for rec_idx in 1..=1000 {
        if list_type == 1 {
            if g.futures_recs[rec_idx] == "" {
                continue;
            }
            f.rec_text = g.futures_recs[rec_idx].clone();
        }
        if list_type == 2 {
            if g.phys_commod_rec[rec_idx] == "" {
                continue;
            }
            f.rec_text = g.phys_commod_rec[rec_idx].clone();
        }
        let holder_id = to_long(val(field(&f.rec_text, 2).as_bytes()));
        if holder_id == 0 || g.entity_status[holder_id] != 1 {
            continue;
        }
        if 10 < holder_id && g.cur_player != g.corp_controller[holder_id] && only_mine == "Y" {
            continue;
        }
        if holder_id < 11 && g.cur_player != holder_id && only_mine == "Y" {
            continue;
        }
        let name_tmp = g.ent_name[holder_id].clone() + &string_fill(27, b" ");
        let mut line_text = mid(name_tmp.as_bytes(), 1, 27);
        f.commod_name = trim(&field(&f.rec_text, 1));
        let mut fld = f.commod_name.clone();
        if fld == "STOCKS" {
            fld = s(__content.string(2499));
        }
        let field_tmp = string_fill(11, b" ") + &mcase(fld.as_bytes(), CaseMode::Ansi);
        fld = right(field_tmp.as_bytes(), 11) + "      ";
        line_text = line_text + &fld;
        f.qty = to_long(val(field(&f.rec_text, 3).as_bytes()));
        let mut pos_type = s(__content.string(6124));
        if f.qty < 0 {
            pos_type = s(__content.string(6125));
        }
        if list_type == 2 {
            pos_type = s("  ");
        }
        f.qty = to_long((cast::<Ext, _>(f.qty)).abs());
        let qty_str = string_fill(9, b" ") + &fmt_trim(cast::<Ext, _>(f.qty), "* ###,");
        line_text = line_text + &right(qty_str.as_bytes(), 9) + &pos_type;
        if list_type == 2 {
            list_futures_or_phys_unit_cost(g, &mut f);
        } else {
            f.unit_price.set_ext(val_ext(field(&f.rec_text, 7).as_bytes()));
        }
        let price = PbStr(format_ext(f.unit_price.get_ext(), fmt_money.as_bytes()));
        fld = string_fill(13, b" ") + &trim(&price) + " ";
        line_text = line_text + &right(fld.as_bytes(), 13);
        line_no = line_no + 1;
        any_found = 1;
        let link_tag = link_id(holder_id);
        f.lines[cast::<usize, _>(line_no)] = line_text + &string_fill(65, b" ") + "@C" + &link_tag;
    }
    if any_found == 0 {
        let mut line_text = s(__content.string(6126));
        if only_mine == "Y" {
            line_text = s(__content.string(6127));
        }
        show_quick_message(g, ui, &mut (line_text + __content.string(6128)));
        return list_futures_or_phys_holders_exit();
    }
    sort_lines(&mut f.lines, 4, line_no - 3);
    line_no = line_no + 1;
    f.lines[cast::<usize, _>(line_no)] = f.separator.clone();
    line_no = line_no + 2;
    if list_type == 2 {
        f.lines[cast::<usize, _>(line_no)] = s(__content.string(6129))
            + &chr(&[34])
            + "unit"
            + &chr(&[34])
            + __content.string(6130);
        line_no = line_no + 1;
        f.lines[cast::<usize, _>(line_no)] = s(__content.string(6131));
        f.lines[cast::<usize, _>(line_no + 1)] = s(__content.string(6132));
        line_no = line_no + 3;
    }
    f.lines[cast::<usize, _>(line_no)] = click_line(7, "COMPANY");
    let line_count = line_no + 2;
    line_no = line_count;
    for k in 1..=line_count {
        g.text_lines[k] = f.lines[cast::<usize, _>(k)].clone();
    }
    g.text_lines[line_no + 3] = s("@END");
    let mut line_text = s(__content.string(6133));
    g.text_box_mode = s("20");
    if list_type == 2 {
        line_text = s(__content.string(6134));
    } else {
        g.text_box_mode = s("64");
    }
    g.text_box_title = s("     ") + __content.string(6135) + &line_text;
    g.entity_pick_mode = s(__content.string(6093));
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    list_futures_or_phys_holders_exit()
}

/// Port of ListFuturesOrPhys_Header (0x670c76), GOSUB of [`list_futures_or_phys_holders`]: the column
/// headings (`lines(1..3)`) and the separator line for futures (`list_type = 1`) or physical holdings.
fn list_futures_or_phys_header(f: &mut FutFrame, list_type: i32) {
    let __content = crate::content::current();
    f.lines[2] = s(__content.string(6136));
    f.separator = s("-------------------------  ---------------  ---------");
    if list_type == 1 {
        f.lines[1] = string_fill(68, b" ") + "CONTRACT";
        f.lines[2] = f.lines[2].clone() + __content.string(6137);
        f.lines[2] = replace(f.lines[2].as_bytes(), __content.bytes(6138), __content.bytes(6139));
        f.separator = f.separator.clone() + "  --------  ----------------";
    } else {
        f.lines[1] = string_fill(53, b" ") + __content.string(6140);
        f.lines[2] = f.lines[2].clone() + __content.string(6141);
        f.lines[2] = replace(f.lines[2].as_bytes(), b"CONTRACTS", __content.bytes(6142));
        f.separator = f.separator.clone() + "  ---------------";
    }
    f.lines[3] = f.separator.clone();
}

/// Port of ListFuturesOrPhys_UnitCost (0x670e97), GOSUB of [`list_futures_or_phys_holders`]: the cost per
/// unit of a physical holding, `totalCost / (qty * unitFactor) / ExchangeRate` (CURRENCYX). Units are
/// contract sizes: 0.001 (SINGLE) for gold and bitcoin, 0.01 for oil, silver and ethereum, else 0.1.
fn list_futures_or_phys_unit_cost(g: &Globals, f: &mut FutFrame) {
    let total_cost = val_ext(field(&f.rec_text, 4).as_bytes());
    let unit_factor: Ext = if f.commod_name == "GOLD" || f.commod_name == "BITCOIN" {
        cast::<Ext, _>(0.001f32)
    } else if f.commod_name == "OIL" || f.commod_name == "SILVER" || f.commod_name == "ETHEREUM" {
        cast::<Ext, _>(0.01f32)
    } else {
        cast::<Ext, _>(0.1f32)
    };
    let denom = Ext80::from_i64(cast::<i64, _>(f.qty)).fmul(Ext80::from_ext(unit_factor));
    f.unit_price.set_ext(total_cost.fdiv(denom));
    f.unit_price.set_ext(f.unit_price.get_ext().fdiv(Ext80::from_ext(g.exchange_rate)));
}

/// Port of ListFuturesOrPhysHolders_Exit (0x670fc5): the routine's exit label (frame cleanup only).
fn list_futures_or_phys_holders_exit() {}

/// Port of ListSwapHolders (0x671001).
/// "Who owns what" list of the interest rate swaps (`SwapContracts()`: long party, short party, fixed
/// rate, rate type P/L/S, begin and end date, notional): asks (YES/NO/CANCEL) whether to list only the
/// swaps in which the current player or a company he controls is a party, then shows long party, short
/// party's symbol, dates ("ACTIVE" once begun), rate type, fixed rate and notional, sorted by the long
/// party's name, with `@C` links to the long parties.
///
/// The sort covers `lines(5..)`, i.e. also the dashed separator line (stays first for names that sort
/// after `-`).
pub fn list_swap_holders(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut lines = vec![PbStr::new(); 1501];
    if val(g.swap_contracts[0].as_bytes()) == Ext::from_f64(0.0) {
        show_quick_message(g, ui, &mut s(__content.string(6143)));
        return;
    }
    let field_text = s(__content.string(6144))
        + __content.string(6145)
        + "\r"
        + "\r"
        + __content.string(6146);
    let answer = ui.msgbox(&field_text, 8483, &s(__content.string(6147)));
    if answer == 2 {
        return;
    }
    let mut only_mine = PbStr::new();
    if answer == 6 {
        only_mine = s("Y");
    }
    // curDate = CurQuarter * 0.1! + CurYear (80-bit, stored as DOUBLE)
    let cur_date = Ext80::from_i64(cast::<i64, _>(g.cur_quarter))
        .fmul(Ext80::from_f32(0.1f32))
        .fadd(Ext80::from_i64(cast::<i64, _>(g.cur_year)))
        .into_ext();
    lines[2] = s(__content.string(6148));
    lines[3] = s(__content.string(6149));
    lines[4] = s(__content.string(6150));
    lines[5] = s("------------------------- -------- ------ ------ ---------- ------ -------------");
    let mut line_no: i32 = 5;
    let mut any_found = 0;
    let rec_count = to_long(val(g.swap_contracts[0].as_bytes()));
    for rec_idx in 1..=rec_count {
        let rec_text = g.swap_contracts[rec_idx].clone();
        if rec_text == "" {
            continue;
        }
        let long_party = to_long(val(field(&rec_text, 1).as_bytes()));
        let short_party = to_long(val(field(&rec_text, 2).as_bytes()));
        let cp = g.cur_player;
        if !(only_mine == "Y")
            || cp == long_party
            || cp == g.corp_controller[long_party]
            || cp == short_party
            || cp == g.corp_controller[short_party]
        {
            any_found = 1;
            line_no = line_no + 1;
            let ln = cast::<usize, _>(line_no);
            let name_field = g.ent_name[long_party].clone() + &string_fill(25, b" ");
            lines[ln] = mid(name_field.as_bytes(), 1, 26);
            let mut field_text = s("  ") + &g.ent_symbol[short_party] + "      ";
            field_text = mid(field_text.as_bytes(), 1, 9);
            if short_party < 11 && 6 < cast::<i32, _>(g.ent_symbol[short_party].len()) {
                field_text = s(" PLAYER") + &str_trim(cast::<Ext, _>(short_party)) + " ";
            }
            lines[ln] = lines[ln].clone() + &field_text;
            field_text = trim(&field(&rec_text, 5));
            if val_ext(field_text.as_bytes()).cmp_val(Ext80::from_ext(cur_date)) != std::cmp::Ordering::Greater {
                field_text = s("ACTIVE");
            }
            lines[ln] = lines[ln].clone() + &field_text + " " + &trim(&field(&rec_text, 6)) + " ";
            let rate_type = trim(&field(&rec_text, 4));
            field_text = s(__content.string(6151));
            if rate_type == "P" {
                field_text = s(__content.string(6152));
            } else if rate_type == "L" {
                field_text = s(__content.string(6153));
            }
            lines[ln] = lines[ln].clone() + &field_text + " ";
            let mut fixed_rate = trim(&field(&rec_text, 3));
            for _ in 0..2 {
                if instr(1, fixed_rate.as_bytes(), b".") < 3 {
                    fixed_rate = s(" ") + &fixed_rate;
                }
            }
            for _ in 0..2 {
                let t = right(fixed_rate.as_bytes(), 3);
                if !(mid(t.as_bytes(), 1, 1) == ".") {
                    fixed_rate = fixed_rate + "0";
                }
            }
            lines[ln] = lines[ln].clone() + &fixed_rate + "% ";
            let notional = to_quad(val(field(&rec_text, 7).as_bytes()));
            field_text = string_fill(13, b" ") + &fmt_trim(cast::<Ext, _>(notional), "* ###,");
            lines[ln] = lines[ln].clone() + &right(field_text.as_bytes(), 11);
            let link_tag = s("@C") + &link_id(long_party);
            lines[ln] = lines[ln].clone() + &string_fill(45, b" ") + &link_tag;
        }
    }
    if any_found == 0 {
        let mut field_text = s(__content.string(6126));
        if only_mine == "Y" {
            field_text = s(__content.string(6127));
        }
        show_quick_message(g, ui, &mut (field_text + __content.string(6154)));
        return;
    }
    sort_lines(&mut lines, 5, line_no - 4);
    line_no = line_no + 1;
    lines[cast::<usize, _>(line_no)] = lines[5].clone();
    line_no = line_no + 2;
    lines[cast::<usize, _>(line_no)] = click_line(6, __content.string(6155));
    line_no = line_no + 2;
    lines[cast::<usize, _>(line_no)] = s("@END");
    g.text_box_title = s(__content.string(6156));
    for k in 1..=line_no {
        g.text_lines[k] = lines[cast::<usize, _>(k)].clone();
    }
    g.entity_pick_mode = s(__content.string(6093));
    g.list_item_noun = PbStr::new();
    g.text_box_mode = s("64");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Port of ListOptionHolders (0x671d38).
/// "Who owns what" list of the option positions (`OptionRecs()`: underlying, 1 call / 2 put, holder,
/// percent, negative = written). A player holding stocks is asked (YES/NO/CANCEL) whether to list only
/// his own positions and those of the companies he controls. Lists companies and the current player
/// (other players are left out) with type, long/short, underlying and percent, sorted by name, with `@C`
/// links; also flags `HasOptionsOutstanding` of every underlying.
///
/// The "only mine" filter tests `CorpController(holder) = CurPlayer`, also for the player's own
/// records. The sort covers `lines(4..)` including the separator.
pub fn list_option_holders(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut lines = vec![PbStr::new(); 1501];
    if val(g.option_recs[0].as_bytes()) == Ext::from_f64(0.0) {
        show_quick_message(g, ui, &mut s(__content.string(6157)));
        return;
    }
    let mut only_mine = PbStr::new();
    if 0 < g.hold_stock_id[(0, g.cur_player)] {
        let line_text = s(__content.string(6158))
            + __content.string(6159)
            + "\r"
            + "\r"
            + __content.string(6160);
        let answer = ui.msgbox(&line_text, 8483, &s(__content.string(6161)));
        if answer == 2 {
            return;
        }
        if answer == 6 {
            only_mine = s("Y");
        }
    }
    lines[2] = s(__content.string(6162));
    lines[3] = s(__content.string(6163));
    lines[4] = s("--------------------------  -------  -----  -------------------------  ------");
    let mut line_no: i32 = 4;
    let mut any_found = 0;
    let rec_count = to_long(val(g.option_recs[0].as_bytes()));
    for rec_idx in 1..=rec_count {
        let rec_text = g.option_recs[rec_idx].clone();
        if rec_text == "" {
            continue;
        }
        let underlying_id = to_long(val(field(&rec_text, 1).as_bytes()));
        g.has_options_outstanding[underlying_id] = 1;
        let holder_id = to_long(val(field(&rec_text, 3).as_bytes()));
        if !(10 < holder_id || g.cur_player == holder_id) {
            continue;
        }
        if only_mine == "Y" && g.cur_player != g.corp_controller[holder_id] {
            continue;
        }
        any_found = 1;
        let mut line_text = g.ent_name[holder_id].clone() + &string_fill(28, b" ");
        line_text = mid(line_text.as_bytes(), 1, 28);
        let mut opt_type = s(__content.string(6164));
        if val(field(&rec_text, 2).as_bytes()) == Ext::from_f64(2.0) {
            opt_type = s(__content.string(6165));
        }
        line_text = line_text + &opt_type;
        let pct = to_integer(val(field(&rec_text, 4).as_bytes()));
        let mut long_short = s("LONG   ");
        if pct < 0 {
            long_short = s("SHORT  ");
        }
        line_text = line_text + &long_short;
        let stock_corp_id = to_long(val(field(&rec_text, 1).as_bytes()));
        let mut fld = g.ent_name[stock_corp_id].clone() + &string_fill(27, b" ");
        fld = mid(fld.as_bytes(), 1, 27);
        line_text = line_text + &fld;
        fld = s("   ") + &str15(cast::<Ext, _>(pct)) + "% ";
        fld = right(fld.as_bytes(), 6);
        line_no = line_no + 1;
        let link_tag = s("@C") + &link_id(holder_id);
        lines[cast::<usize, _>(line_no)] = line_text + &fld + &string_fill(40, b" ") + &link_tag;
    }
    if any_found == 0 {
        let mut line_text = s(__content.string(6126));
        if only_mine == "Y" {
            line_text = s(__content.string(6166));
        }
        show_quick_message(g, ui, &mut (line_text + __content.string(6167)));
        return;
    }
    sort_lines(&mut lines, 4, line_no - 3);
    line_no = line_no + 1;
    lines[cast::<usize, _>(line_no)] = lines[4].clone();
    line_no = line_no + 2;
    lines[cast::<usize, _>(line_no)] = click_line(7, "COMPANY");
    line_no = line_no + 2;
    lines[cast::<usize, _>(line_no)] = s("@END");
    g.text_box_title = s(__content.string(6168));
    for k in 1..=line_no {
        g.text_lines[k] = lines[cast::<usize, _>(k)].clone();
    }
    g.entity_pick_mode = s(__content.string(6093));
    g.list_item_noun = PbStr::new();
    g.text_box_mode = s("64");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Port of ListStockCrossHoldings (0x672677).
/// "Who owns what" list of the stocks held by companies (not funds, industry 71): a player holding stocks
/// is asked (YES/NO/CANCEL) whether to list only the companies he controls. Each holding shows parent,
/// stock, percent and the analyst's rating of the stock ([`calc_analyst_rating`]); a stock without
/// public float ("NOT TRADED") is detected through `PublicPct` of [`update_corp_control`] (run once per
/// held stock and cached; a 100% holding counts as untraded without the check). Sorted by parent name,
/// with `@C` links to the parents. Companies are only scanned while fewer than 1971 lines are used.
pub fn list_stock_cross_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut lines = vec![PbStr::new(); 2051];
    let mut public_pct_cache = vec![0i32; 1611];
    let mut only_mine = PbStr::new();
    if 0 < g.hold_stock_id[(0, g.cur_player)] {
        let line_text = s(__content.string(6169))
            + __content.string(6170)
            + "\r"
            + "\r"
            + __content.string(6171);
        let answer = ui.msgbox(&line_text, 8483, &s(__content.string(6172)));
        if answer == 2 {
            return;
        }
        if answer == 6 {
            only_mine = s("Y");
        }
    }
    for corp_id in 11..=1600 {
        public_pct_cache[corp_id] = -1;
    }
    lines[1] = s(__content.string(6173));
    lines[2] = s(__content.string(6174));
    lines[3] = s("-------------------------  -------------------------  ------- -----------");
    let mut line_no: i32 = 3;
    let mut any_found = 0;
    for corp_id in 11..=1600 {
        if g.entity_status[corp_id] != 1
            || g.hold_stock_id[(0, corp_id)] == 0
            || g.industry_num[corp_id] == 71
            || 1971 <= line_no
        {
            continue;
        }
        if only_mine == "Y" && g.cur_player != g.corp_controller[corp_id] {
            continue;
        }
        for slot in 1..=15 {
            if !(0 < g.hold_stock_id[(slot, corp_id)]) {
                continue;
            }
            any_found = 1;
            let mut line_text = g.ent_name[corp_id].clone() + &string_fill(27, b" ");
            line_text = mid(line_text.as_bytes(), 1, 27);
            let held_corp_id = cast::<i32, _>(g.hold_stock_id[(slot, corp_id)]);
            let mut held_name = g.ent_name[held_corp_id].clone() + &string_fill(27, b" ");
            held_name = mid(held_name.as_bytes(), 1, 27);
            let pct_owned = cast::<i32, _>(g.hold_stock_pct[(slot, corp_id)]);
            let mut pct_str = s("   ") + &str15(cast::<Ext, _>(pct_owned)) + "%  ";
            pct_str = right(pct_str.as_bytes(), 7);
            line_no = line_no + 1;
            let rating = calc_analyst_rating(g, ui, rng, held_corp_id);
            let mut rating_str = s(__content.string(6175));
            if rating == 2 {
                rating_str = s(__content.string(6176));
            }
            if rating == 3 {
                rating_str = s(__content.string(6177));
            }
            if rating == 4 {
                rating_str = s(__content.string(6178));
            }
            if rating == 5 {
                rating_str = s(__content.string(6179));
            }
            if public_pct_cache[cast::<usize, _>(held_corp_id)] == -1 {
                if pct_owned == 100 {
                    g.public_pct = 0;
                } else {
                    update_corp_control(g, ui, rng, held_corp_id);
                    public_pct_cache[cast::<usize, _>(held_corp_id)] = cast::<i32, _>(g.public_pct);
                }
            } else {
                g.public_pct = cast::<i16, _>(public_pct_cache[cast::<usize, _>(held_corp_id)]);
            }
            if g.public_pct == 0 {
                rating_str = s(__content.string(6180));
            }
            rating_str = s(" ") + &rating_str;
            let link_tag = s("@C") + &link_id(corp_id);
            lines[cast::<usize, _>(line_no)] =
                line_text + &held_name + &pct_str + &rating_str + &string_fill(40, b" ") + &link_tag;
        }
    }
    if any_found == 0 {
        let mut line_text = s(__content.string(6126));
        if only_mine == "Y" {
            line_text = s(__content.string(6166));
        }
        show_quick_message(g, ui, &mut (line_text + __content.string(6181)));
        return;
    }
    sort_lines(&mut lines, 3, line_no - 2);
    line_no = line_no + 1;
    lines[cast::<usize, _>(line_no)] = lines[3].clone();
    line_no = line_no + 2;
    lines[cast::<usize, _>(line_no)] = click_line(7, "COMPANY");
    line_no = line_no + 2;
    lines[cast::<usize, _>(line_no)] = s("@END");
    g.text_box_title = s(__content.string(6182));
    for k in 1..=line_no {
        g.text_lines[k] = lines[cast::<usize, _>(k)].clone();
    }
    g.entity_pick_mode = s(__content.string(6093));
    g.list_item_noun = PbStr::new();
    g.text_box_mode = s("40");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Port of ListEtfAdvisers (0x673009), including its fee-formatting GOSUB (0x673701).
/// Lists active ETFs, their advisers, fees and assets, with links to select each fund.
pub fn list_etf_advisers(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    use crate::logic::finance::calc_entity_financials;
    use crate::logic::reports_corp::assign_etf_adviser;
    let mut lines = vec![PbStr::new(); 40];
    lines[2] = s(__content.string(6183));
    lines[3] = s(__content.string(6184))
        + &g.million_prefix_uc + __content.string(6185);
    lines[4] = s("-------------------------  -------------------------  -------------  --------------");
    let mut line_no = 4usize;
    let mut has_bonus_note = false;
    for etf in 1581..=1600 {
        if g.entity_status[etf] != 1 || g.industry_num[etf] != 71 { continue; }
        if g.fund_fee_pct[etf] < Ext::from_f32(0.2f32) || g.fund_fee_pct[etf] > Ext::from_f32(1.0f32) || g.fund_adviser[etf] == 0 {
            assign_etf_adviser(g, ui, rng, etf, 0, 0);
        }
        let mut fee_pct = s("0") + &trim(&PbStr::from(crate::pb::numeric::str_num(cast::<Ext, _>(g.fund_fee_pct[etf]), 7).as_str()));
        if cast::<Ext, _>(g.fund_fee_pct[etf]) > cast::<Ext, _>(0.95_f32) { fee_pct = s("1.0"); }
        let adviser = g.fund_adviser[etf];
        let fund_name = mid((g.ent_name[etf].clone() + &string_fill(27, b" ")).as_bytes(), 1, 27);
        let adviser_name = mid((g.ent_name[adviser].clone() + &string_fill(27, b" ")).as_bytes(), 1, 27);
        let mut fee = s("     ") + &fee_pct + "%    ";
        if g.fund_bonus_year[etf] > 0 && etf > 1585 && g.cur_year % 2 == etf % 2 {
            fee = replace(fee.as_bytes(), b"%  ", b"% *");
            has_bonus_note = true;
        }
        calc_entity_financials(g, ui, rng, etf);
        let assets = right((string_fill(15, b" ") + &fmt_trim(g.total_assets, "* ###,")).as_bytes(), 15);
        line_no += 1;
        lines[line_no] = fund_name + &adviser_name + &fee + &assets + &string_fill(40, b" ") + "@C" + &link_id(etf);
    }
    if line_no == 4 {
        show_quick_message(g, ui, &mut s(__content.string(6186)));
        return;
    }
    sort_lines(&mut lines, 5, cast::<i32, _>(line_no - 4));
    line_no += 1;
    lines[line_no] = lines[4].clone();
    line_no += 1;
    if has_bonus_note {
        lines[line_no] = s(__content.string(6187));
        line_no += 2;
    }
    lines[line_no] = click_line(7, "ETF");
    line_no += 2;
    lines[line_no] = s("@END");
    g.text_box_title = s(__content.string(6188));
    for i in 1..=line_no { g.text_lines[cast::<i32, _>(i)] = lines[i].clone(); }
    g.entity_pick_mode = s(__content.string(6093));
    g.list_item_noun = PbStr::new();
    g.text_box_mode = s("75");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}
