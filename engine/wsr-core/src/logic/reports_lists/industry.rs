//! Company lists: `ShowTopCompaniesList` (0x49e21f), `ShowIndustryReport` (0x47f17e) and
//! `ShowMyHeadlines` (0x49d62b), with their GOSUBs, plus the runtime's `ARRAY SORT … TAGARRAY …, DESCEND`
//! (`PB_ArraySort` 0x6bf6a9) they sort with.
use crate::pb::real::{Ext, cast};


use super::{fmt, fmt_trim, link_id, s, str15, str_trim};
use crate::logic::control::update_corp_control;
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::{show_quick_message, show_text_report};
use crate::logic::valuation::{
    calc_analyst_rating, calc_dividend_yield, calc_industry_outlook, calc_stock_price, price_pct_change_months,
};
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, mid, parse_any, replace, right, rtrim, space, string_fill, ucase, CaseMode};
use crate::pb::numeric::{to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// `ARRAY SORT keys(first) FOR count, TAGARRAY tags(), DESCEND` (`PB_ArraySort` 0x6bf6a9 with flag
/// 0x400): the runtime's recursive quicksort (middle element as pivot, Hoare partition; not stable, so
/// the order of equal keys is the runtime's). `keys` and `tags` are the elements `first ..` of the two
/// arrays (same subscript). `count = 0` sorts to the end of the array.
fn array_sort_desc<K: PartialOrd + Copy>(keys: &mut [K], tags: &mut [PbStr], first: usize, count: usize) {
    let n = if count == 0 { keys.len() - first } else { count };
    super::pb_array_sort(&mut keys[first..first + n], Some(&mut tags[first..first + n]), true, &|a: &K, b: &K| a < b);
}

/// "CONTROLLED BY" column of the top lists: `"NO ONE"`, the controlling corp's symbol, `"  _P  "` (current
/// player) or `"  _H<n> "` (another player; `hostile_num` keeps the number).
fn ctrl_column(g: &Globals, corp_id: i32, hostile_num: &mut PbStr) -> PbStr {
    let mut ctrl_col = s("NO ONE");
    if 0 < g.corp_controller[corp_id] {
        if g.num_players < g.corp_controller[corp_id] {
            if corp_id != g.corp_controller[corp_id] {
                let sym = &g.ent_symbol[g.corp_controller[corp_id]];
                ctrl_col = s("   ") + sym + "  ";
                if sym.len() == 4 {
                    ctrl_col = right(ctrl_col.as_bytes(), 7);
                } else {
                    ctrl_col = right(ctrl_col.as_bytes(), 6);
                }
            }
        } else if g.cur_player == g.corp_controller[corp_id] {
            ctrl_col = s("  _P  ");
        } else {
            *hostile_num = str_trim(cast::<Ext, _>(g.corp_controller[corp_id]));
            ctrl_col = s("  _H") + &*hostile_num + " ";
        }
    }
    ctrl_col
}

/// `"  " + CreditRatingStr`, right 3 characters.
fn rating3(g: &Globals) -> PbStr {
    let t = s("  ") + &g.credit_rating_str;
    right(t.as_bytes(), 3)
}

/// GOSUB TopList_AppendCorpLink (0x49ffbc): appends 60 blanks and the `@C<id>` link of `corp_id` to
/// `report_lines(line_num)` (the GOSUB also leaves the id text in `nameCol`, which is not read again).
fn top_list_append_corp_link(report_lines: &mut [PbStr], line_num: i32, corp_id: i32) {
    let name_col = link_id(corp_id);
    let l = &mut report_lines[cast::<usize, _>(line_num)];
    *l = l.clone() + &space(60) + "@C" + &name_col;
}

/// Port of ShowTopCompaniesList (0x49e21f).
/// Lists companies (links `@C<id>`, `EntityPickMode = "ChangeCorp"`): `list_type` 1 = companies with at
/// least 20% of their industry's business assets (industries 3..69, by share), 2 = largest tax-loss
/// carryovers, 3 = 100 largest cash + T-bill balances, 4 = 100 largest market caps; sorted descending with
/// the runtime's ARRAY SORT. Calls CalcIndustryOutlook (list 1), CalcEntityFinancials (credit rating) and
/// UpdateCorpControl (lists 1, 4) for every listed corp. Nothing found (or another `list_type`): popup
/// " [runtime resource wording] ".
///
/// Quirks: every sort covers `lineNum + 3` elements from line 4, i.e. six empty lines (key 0) past the
/// last entry, which mix with entries of key <= 0 (list 3 zero balances, list 4 non-positive caps): such
/// entries may be sorted past the end and dropped, and empty lines may appear in the list. The list-2
/// title counts the 3 header lines (`STR$(lineNum)`).
pub fn show_top_companies_list(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, list_type: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut report_lines = vec![PbStr::new(); 1601]; // DIM reportLines(0 TO 1600)
    let mut sort_keys = vec![Ext::ZERO; 1601]; // DIM sortKeys(0 TO 1600) AS EXT
    let mut hostile_num = PbStr::new();
    let mut name_col;
    let mut value_col;
    let mut ctrl_col = PbStr::new();
    let mut rating_col;
    let mut wide_cap = 0;
    let mut wide_cash = 0;
    let mut market_share = crate::pb::numeric::double(Ext::from_f64(0.0f64)); // DOUBLE
    let mut cap_rounded: i64;
    let mut found_any = 0;
    let mut line_num = 0;

    g.entity_pick_mode = s(__content.string(6093));
    g.text_box_mode = s("60");
    if list_type == 1 {
        g.text_box_mode = s("96");
        line_num = 3;
        for industry_id in 3i16..=69 {
            g.ind_total_assets = Ext::from_f64(0.0);
            let _outlook = calc_industry_outlook(g, rng, industry_id);
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            for corp_id in 131..=1600 {
                if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] != industry_id {
                    continue;
                }
                if g.business_assets[corp_id] < g.ind_total_assets * (cast::<Ext, _>(0.2f32)) {
                    continue;
                }
                if Ext::from_f64(0.0) < g.ind_total_assets {
                    market_share = crate::pb::numeric::double((g.business_assets[corp_id] / g.ind_total_assets) * Ext::from_f64(100.0));
                }
                line_num = line_num + 1;
                found_any = 1;
                sort_keys[cast::<usize, _>(line_num)] = market_share;
                name_col = g.ent_name[corp_id].clone() + &string_fill(27, b" ");
                name_col = mid(name_col.as_bytes(), 1, 27);
                value_col = g.ind_name[cast::<i32, _>(industry_id)].clone() + &string_fill(27, b" ");
                value_col = mid(value_col.as_bytes(), 1, 29);
                ctrl_col = fmt(market_share, "* ##") + "%";
                calc_entity_financials(g, ui, rng, corp_id);
                rating_col = rating3(g) + "   ";
                report_lines[cast::<usize, _>(line_num)] = name_col + &value_col + &rating_col + &ctrl_col + "      ";
                update_corp_control(g, ui, rng, corp_id);
                ctrl_col = ctrl_column(g, corp_id, &mut hostile_num);
                let l = &mut report_lines[cast::<usize, _>(line_num)];
                *l = l.clone() + &ctrl_col;
                top_list_append_corp_link(&mut report_lines, line_num, corp_id);
            }
        }
        array_sort_desc(&mut sort_keys, &mut report_lines, 4, cast::<usize, _>(line_num + 3));
        name_col = str15(cast::<Ext, _>(line_num - 3));
        if line_num < 3 {
            name_col = PbStr::new();
        }
        g.text_box_title = s("   === ") + &name_col + __content.string(8588);
        report_lines[1] = s(__content.string(8589));
        report_lines[2] = s(__content.string(8590));
        report_lines[3] = s("-------------------------  -------------------------  ------  -------  ----------");
        line_num = line_num + 1;
        report_lines[cast::<usize, _>(line_num)] =
            s("---------------------------------------------------------------------------------");
        line_num = line_num + 1;
        report_lines[cast::<usize, _>(line_num)] =
            s(__content.string(8591)) + &g.cur_player_name;
        line_num = line_num + 1;
        report_lines[cast::<usize, _>(line_num)] = s(__content.string(8592));
        if !hostile_num.is_empty() {
            line_num = line_num + 1;
            let hostile_player = to_long(val(hostile_num.as_bytes()));
            report_lines[cast::<usize, _>(line_num)] = string_fill(15, b" ")
                + "(_H"
                + &hostile_num
                + __content.string(8593)
                + &hostile_num
                + ", "
                + &g.player_name[hostile_player]
                + ")";
        }
    }
    if list_type == 2 || list_type == 3 || list_type == 4 {
        // column widths: with values over 12 characters the thousands separators are left out
        for corp_id in 11..=1600 {
            if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] == 71 {
                continue;
            }
            if list_type == 2 {
                name_col = fmt_trim(g.tax_loss_carry[corp_id].abs(), "* #########");
            } else {
                name_col = fmt_trim(g.cash[corp_id] + g.t_bills[corp_id], "* #########");
            }
            if 12 < name_col.len() {
                wide_cash = 1;
            }
            cap_rounded = to_quad(g.market_cap[corp_id] * Ext::from_f64(100.0));
            value_col = fmt_trim(cast::<Ext, _>(cap_rounded), "* ##########");
            if 12 < value_col.len() {
                wide_cap = 1;
            }
        }
    }
    if list_type == 2 || list_type == 3 {
        line_num = 3;
        for corp_id in 11..=1600 {
            if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] == 71 {
                continue;
            }
            let listed = if list_type == 2 {
                g.tax_loss_carry[corp_id] <= -Ext::from_f64(10.0)
            } else {
                Ext::from_f64(0.0) <= g.cash[corp_id] + g.t_bills[corp_id]
            };
            if !listed {
                continue;
            }
            line_num = line_num + 1;
            found_any = 1;
            calc_entity_financials(g, ui, rng, corp_id);
            rating_col = rating3(g);
            if list_type == 2 {
                sort_keys[cast::<usize, _>(line_num)] = g.tax_loss_carry[corp_id].abs();
            } else {
                sort_keys[cast::<usize, _>(line_num)] = g.cash[corp_id] + g.t_bills[corp_id];
            }
            name_col = g.ent_name[corp_id].clone() + &string_fill(27, b" ");
            value_col = fmt_trim(Ext::from(sort_keys[cast::<usize, _>(line_num)]), "* ##########");
            if wide_cash == 0 {
                value_col = fmt_trim(Ext::from(sort_keys[cast::<usize, _>(line_num)]), "* ##########,");
            }
            if 15 < value_col.len() {
                // Quirk: takes ctrlCol (the previous line's market cap) instead of valueCol.
                value_col = mid(ctrl_col.as_bytes(), 1, 15);
            }
            value_col = string_fill(15, b" ") + &value_col;
            value_col = right(value_col.as_bytes(), 15);
            cap_rounded = to_quad(g.market_cap[corp_id] * Ext::from_f64(100.0));
            ctrl_col = fmt_trim(cast::<Ext, _>(cap_rounded), "* ##########");
            if wide_cap == 0 {
                ctrl_col = fmt_trim(cast::<Ext, _>(cap_rounded), "* ##########,");
            }
            if 15 < ctrl_col.len() {
                ctrl_col = mid(ctrl_col.as_bytes(), 1, 15);
            }
            ctrl_col = string_fill(15, b" ") + &ctrl_col;
            ctrl_col = right(ctrl_col.as_bytes(), 15);
            report_lines[cast::<usize, _>(line_num)] = mid(name_col.as_bytes(), 1, 27)
                + &value_col
                + " "
                + &g.million_prefix_uc
                + ".    "
                + &rating_col
                + " "
                + &ctrl_col
                + " "
                + &g.million_prefix_uc
                + ".";
            top_list_append_corp_link(&mut report_lines, line_num, corp_id);
        }
        array_sort_desc(&mut sort_keys, &mut report_lines, 4, cast::<usize, _>(line_num + 3));
        if list_type == 2 {
            g.text_box_title =
                s("           === ") + &str15(cast::<Ext, _>(line_num)) + __content.string(8594);
            report_lines[1] = s(__content.string(8595));
            report_lines[2] = s(__content.string(8596));
            report_lines[3] = s("-------------------------  ------------------ -------- -------------------");
        }
    }
    if list_type == 3 {
        if 103 < line_num {
            line_num = 103;
        }
        g.text_box_title = s(__content.string(8597));
        report_lines[1] = s(__content.string(8598));
        report_lines[2] = s(__content.string(8599));
        report_lines[3] = s("-------------------------  ------------------ -------- -------------------");
    }
    if list_type == 4 {
        g.text_box_mode = s("40");
        line_num = 3;
        for corp_id in 11..=1600 {
            if g.entity_status[corp_id] != 1 || g.industry_num[corp_id] == 71 {
                continue;
            }
            line_num = line_num + 1;
            found_any = 1;
            calc_entity_financials(g, ui, rng, corp_id);
            rating_col = rating3(g);
            sort_keys[cast::<usize, _>(line_num)] = g.market_cap[corp_id] * Ext::from_f64(100.0);
            name_col = g.ent_name[corp_id].clone() + &string_fill(27, b" ");
            cap_rounded = to_quad(sort_keys[cast::<usize, _>(line_num)]);
            value_col = fmt_trim(cast::<Ext, _>(cap_rounded), "* ##########");
            if wide_cap == 0 {
                value_col = fmt_trim(cast::<Ext, _>(cap_rounded), "* ##########,");
            }
            if 15 < value_col.len() {
                value_col = mid(value_col.as_bytes(), 1, 15);
            }
            value_col = string_fill(15, b" ") + &value_col;
            value_col = right(value_col.as_bytes(), 15);
            update_corp_control(g, ui, rng, corp_id);
            ctrl_col = ctrl_column(g, corp_id, &mut hostile_num);
            report_lines[cast::<usize, _>(line_num)] = mid(name_col.as_bytes(), 1, 27)
                + &value_col
                + " "
                + &g.million_prefix_uc
                + ".      "
                + &rating_col
                + &string_fill(9, b" ")
                + &ctrl_col;
            top_list_append_corp_link(&mut report_lines, line_num, corp_id);
        }
        array_sort_desc(&mut sort_keys, &mut report_lines, 4, cast::<usize, _>(line_num + 3));
        if 103 < line_num {
            line_num = 103;
        }
        g.text_box_title = s(__content.string(8600));
        report_lines[1] = s(__content.string(8601));
        report_lines[2] = s(__content.string(8602));
        report_lines[3] = s("-------------------------  ------------------  ----------  ----------");
        line_num = line_num + 1;
        report_lines[cast::<usize, _>(line_num)] = s("---------------------------------------------------------------------");
        line_num = line_num + 1;
        report_lines[cast::<usize, _>(line_num)] =
            s(__content.string(8591)) + &g.cur_player_name;
        line_num = line_num + 1;
        report_lines[cast::<usize, _>(line_num)] = s(__content.string(8592));
        if !hostile_num.is_empty() {
            line_num = line_num + 1;
            let hostile_player = to_long(val(hostile_num.as_bytes()));
            report_lines[cast::<usize, _>(line_num)] = string_fill(15, b" ")
                + "(_H"
                + &hostile_num
                + __content.string(8593)
                + &hostile_num
                + ", "
                + &g.player_name[hostile_player]
                + ")";
        }
    }
    if found_any == 0 {
        let mut msg = s(__content.string(8603));
        show_quick_message(g, ui, &mut msg);
        return;
    }
    report_lines[cast::<usize, _>(line_num + 4)] = s("@END");
    let last_line = line_num + 4;
    for i in 1..=last_line {
        if line_num < i && i < line_num + 4 {
            report_lines[cast::<usize, _>(i)] = PbStr::new();
        }
        g.text_lines[i] = report_lines[cast::<usize, _>(i)].clone();
    }
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Port of ShowIndustryReport (0x47f17e).
/// "FINANCIAL SUMMARY FOR <industry> INDUSTRY": every active corp of industry `industry_id` (low word,
/// as INTEGER) with price to net worth, credit rating (`*`/`**` for public / convertible bonds), P/E,
/// return on equity (funds: one-year price performance, or N/A in the first year), dividend yield and
/// analyst rating, ranked descending by ROE (funds: performance, first year: dividend yield) with the
/// runtime's ARRAY SORT; `_P`/`_H<n>`/`_C` mark controlled companies. Each corp is valued first
/// (`ValuationMode = 2`, CalcStockPrice) and UpdateCorpControl / CalcDividendYield / CalcAnalystRating
/// run for it. With `EntityPickMode = "ChangeCorp"` lines get `@C<id>` links. Industry 0: popup
/// " [runtime resource wording] …".
pub fn show_industry_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, industry_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported; only reachable with ~980 corps in one
    // industry, where the footer lines run past lines(990))
    let mut lines = vec![PbStr::new(); 995]; // DIM lines(1 TO 990)
    let mut sort_keys = vec![0.0f32; 995]; // DIM sortKeys(1 TO 990) AS SINGLE
    let mut hostile_player_text = PbStr::new();
    let mut col_text;
    let mut fmt_text;
    let mut has_ctrl_marks = 0;
    let mut corp_count = 0;
    let mut has_conv_bonds = 0;
    let mut has_public_bonds = 0;

    let industry = cast::<i16, _>(industry_id);
    if industry == 0 {
        let mut msg = s(__content.string(8604));
        show_quick_message(g, ui, &mut msg);
        return;
    }
    let mut text_buf = g.ind_name[cast::<i32, _>(industry)].clone() + __content.string(8605);
    if industry == 71 {
        if g.start_year == g.cur_year {
            text_buf = replace(text_buf.as_bytes(), __content.bytes(8606), __content.bytes(8607));
        } else {
            text_buf = replace(text_buf.as_bytes(), __content.bytes(8606), __content.bytes(8608));
        }
    }
    let half_len = to_long(cast::<Ext, _>(text_buf.len()) * Ext::from_f64(0.5));
    text_buf = string_fill(42 - half_len, b" ") + &text_buf;
    lines[2] = text_buf;
    lines[3] = s("------------------------- ---------  -------  ------  ---------  ------  -----------");
    lines[4] = s(__content.string(8609));
    lines[5] = s(__content.string(8610));
    if industry == 71 {
        lines[4] = s(__content.string(8611));
        lines[5] = s(__content.string(8612));
    }
    lines[6] = lines[3].clone();
    let mut line_no: i32 = 6;
    let mut corp_id = 11;
    loop {
        if g.entity_status[corp_id] == 1 && g.industry_num[corp_id] == industry {
            let ln = |k: i32| cast::<usize, _>(k);
            line_no = line_no + 1;
            corp_count = corp_count + 1;
            text_buf = g.ent_name[corp_id].clone() + &space(26);
            lines[ln(line_no)] = mid(text_buf.as_bytes(), 1, 26);
            g.valuation_mode = 2;
            calc_stock_price(g, ui, rng, corp_id, 0);
            col_text = PbStr::new();
            if Ext::from_f64(0.0) < g.net_worth {
                let ratio = (g.market_cap[corp_id] * Ext::from_f64(100.0)) / g.net_worth;
                if ratio < Ext::from_f64(100.0) {
                    fmt_text = fmt(ratio * Ext::from_f64(100.0), "* ####");
                    col_text = s(" ") + &right(fmt_text.as_bytes(), 4) + " %" + &space(5);
                } else {
                    col_text = PbStr::new();
                }
            }
            update_corp_control(g, ui, rng, corp_id);
            if col_text.is_empty() {
                col_text = s("  NMF") + &space(7);
            }
            let ctrl = g.corp_controller[corp_id];
            if g.cur_player == ctrl {
                if instr(1, col_text.as_bytes(), b"NMF") < 1 {
                    col_text = replace(col_text.as_bytes(), b"%  ", b"%_P");
                } else {
                    col_text = replace(col_text.as_bytes(), b"NMF    ", b"NMF  _P");
                }
                has_ctrl_marks = 1;
            }
            if 10 < ctrl && corp_id != ctrl {
                if instr(1, col_text.as_bytes(), b"NMF") < 1 {
                    col_text = replace(col_text.as_bytes(), b"%  ", b"%_C");
                } else {
                    col_text = replace(col_text.as_bytes(), b"NMF    ", b"NMF  _C");
                }
                has_ctrl_marks = 1;
            }
            if 0 < ctrl && ctrl <= g.num_players && g.cur_player != ctrl {
                hostile_player_text = str_trim(cast::<Ext, _>(ctrl));
                if instr(1, col_text.as_bytes(), b"NMF") < 1 {
                    col_text = replace(col_text.as_bytes(), b"%  ", b"%_H");
                } else {
                    col_text = replace(col_text.as_bytes(), b"NMF    ", b"NMF  _H");
                }
                let with = s("_H") + &hostile_player_text;
                col_text = replace(col_text.as_bytes(), b"_H ", with.as_bytes());
                has_ctrl_marks = 1;
            }
            let t = s("    ") + &g.credit_rating_str;
            let mut rating_col = right(t.as_bytes(), 4);
            if g.bonds_public_held <= 0 {
                rating_col = rating_col + &space(4);
            } else if instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") < 1 {
                rating_col = rating_col + " *  ";
                has_public_bonds = 1;
            } else {
                rating_col = rating_col + " ** ";
                has_conv_bonds = 1;
            }
            let mut annual_earnings = g.earnings_ytd[corp_id];
            let mut qtrs_reported = g.cur_quarter;
            if g.sim_co_count < corp_id {
                qtrs_reported = g.cur_quarter - 1;
            }
            if qtrs_reported == 1 {
                annual_earnings = annual_earnings + g.earnings_y1[corp_id] * Ext::from_f64(0.75);
            }
            if qtrs_reported == 2 {
                annual_earnings = annual_earnings + g.earnings_y1[corp_id] * Ext::from_f64(0.5);
            }
            if qtrs_reported == 3 {
                annual_earnings = annual_earnings + g.earnings_y1[corp_id] * Ext::from_f64(0.25);
            }
            if qtrs_reported == 4 || qtrs_reported == 0 {
                annual_earnings = g.earnings_y1[corp_id];
            }
            let mut ratio = g.market_cap[corp_id] / (annual_earnings * (cast::<Ext, _>(0.01f32)));
            if Ext::from_f64(999.0) < ratio {
                ratio = Ext::from_f64(0.0);
            }
            fmt_text = fmt(ratio, "* ###.0");
            let mut pe_col = right(fmt_text.as_bytes(), 5) + &space(3);
            if industry == 71 || ratio <= Ext::from_f64(0.0) {
                pe_col = s(__content.string(8613));
            }
            if g.corp_prior_net_worth[corp_id] <= Ext::from_f64(0.0) {
                ratio = Ext::from_f64(0.0);
            } else {
                ratio = (g.earnings_y1[corp_id] / g.corp_prior_net_worth[corp_id]) * Ext::from_f64(100.0);
            }
            if Ext::from_f64(900.0) < ratio || ratio < -Ext::from_f64(99.0) {
                ratio = Ext::from_f64(0.0);
            }
            if industry != 71 {
                sort_keys[ln(line_no)] = cast::<f32, _>(ratio);
                if g.earnings_y1[corp_id] < Ext::from_f64(0.0) && ratio == Ext::from_f64(0.0) {
                    sort_keys[ln(line_no)] = -100.0f32;
                }
            }
            fmt_text = s("  ") + &fmt(ratio, "* ###.0");
            let mut roe_col = right(fmt_text.as_bytes(), 6) + "%  ";
            if ratio == Ext::from_f64(0.0) {
                roe_col = s(__content.string(8614));
            }
            if industry == 71 {
                if g.start_year == g.cur_year {
                    roe_col = s(__content.string(8615));
                } else {
                    let price_pct_change = price_pct_change_months(g, corp_id, 12);
                    if price_pct_change == 0 && g.cap_hist_close[(13, corp_id)] == Ext::from_f64(0.0) {
                        fmt_text = s(__content.string(8616));
                        roe_col = right(fmt_text.as_bytes(), 7) + "  ";
                    } else {
                        fmt_text = s("     ") + &str15(cast::<Ext, _>(price_pct_change));
                        roe_col = right(fmt_text.as_bytes(), 6) + "%  ";
                    }
                    sort_keys[ln(line_no)] = cast::<f32, _>(price_pct_change);
                }
            }
            ratio = calc_dividend_yield(g, corp_id);
            if industry == 71 && g.start_year == g.cur_year {
                sort_keys[ln(line_no)] = cast::<f32, _>(ratio);
            }
            if ratio < cast::<Ext, _>(0.05f32) {
                ratio = Ext::from_f64(0.0);
            }
            fmt_text = s("  ") + &fmt(ratio, "* ###.0");
            text_buf = right(fmt_text.as_bytes(), 6) + "%  ";
            lines[ln(line_no)] =
                lines[ln(line_no)].clone() + &col_text + &rating_col + &pe_col + &roe_col + &text_buf;
            let analyst_rating = calc_analyst_rating(g, ui, rng, corp_id);
            col_text = s(__content.string(8617));
            if analyst_rating == 2 {
                col_text = s(__content.string(6176));
            }
            if analyst_rating == 3 {
                col_text = s(__content.string(6177));
            }
            if analyst_rating == 4 {
                col_text = s(__content.string(6178));
            }
            if analyst_rating == 5 {
                col_text = s(__content.string(6179));
            }
            if g.public_pct == 0 {
                col_text = s(__content.string(8618));
            }
            lines[ln(line_no)] = lines[ln(line_no)].clone() + " " + &col_text;
            if g.entity_pick_mode == __content.string(6093) {
                col_text = link_id(corp_id);
                lines[ln(line_no)] = lines[ln(line_no)].clone() + &string_fill(50, b" ") + "@C" + &col_text;
            }
            if line_no == 985 {
                line_no = 986;
                lines[ln(line_no)] = s(__content.string(8619));
                corp_id = 1600;
            }
        }
        corp_id = corp_id + 1;
        if 1600 < corp_id {
            break;
        }
    }
    if line_no < 986 {
        array_sort_desc(&mut sort_keys, &mut lines, 7, cast::<usize, _>(corp_count));
        lines[cast::<usize, _>(line_no + 1)] = lines[3].clone();
        line_no = line_no + 1;
        if has_public_bonds == 1 {
            lines[cast::<usize, _>(line_no + 1)] =
                string_fill(10, b" ") + __content.string(8620);
            line_no = line_no + 1;
        }
        if has_conv_bonds == 1 {
            lines[cast::<usize, _>(line_no + 1)] =
                string_fill(10, b" ") + __content.string(8621);
            line_no = line_no + 1;
        }
        if has_ctrl_marks == 1 {
            lines[cast::<usize, _>(line_no + 1)] =
                string_fill(10, b" ") + __content.string(8622) + &g.cur_player_name;
            lines[cast::<usize, _>(line_no + 2)] = string_fill(10, b" ") + __content.string(8623);
            if !hostile_player_text.is_empty() {
                line_no = line_no + 1;
                let hostile_player = to_long(val(hostile_player_text.as_bytes()));
                lines[cast::<usize, _>(line_no + 2)] = string_fill(15, b" ")
                    + "(_H"
                    + &hostile_player_text
                    + __content.string(8593)
                    + &hostile_player_text
                    + ", "
                    + &g.player_name[hostile_player]
                    + ")";
            }
            lines[cast::<usize, _>(line_no + 3)] =
                string_fill(10, b" ") + __content.string(8624);
            line_no = line_no + 3;
        }
    }
    industry_report_display(g, ui, rng, &lines, line_no, industry);
}

/// GOSUB IndustryReport_Display (0x4808b8): pads short reports by 5 lines, ends the list with `@END`,
/// copies `lines(1..990)` to `TextLines` and shows the report (`TextBoxMode = "80"`, links on).
fn industry_report_display(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, lines: &[PbStr], mut line_no: i32, industry: i16) {
    let __content = crate::content::current();
    let mut lines = lines.to_vec();
    if 16 < line_no && line_no < 80 {
        line_no = line_no + 5;
    }
    lines[cast::<usize, _>(line_no + 1)] = s("@END");
    for k in 1..=990 {
        g.text_lines[k] = lines[cast::<usize, _>(k)].clone();
    }
    g.text_box_title = s(__content.string(8625)) + &g.ind_name[cast::<i32, _>(industry)] + __content.string(8626);
    g.text_box_mode = s("80");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Locals of ShowMyHeadlines shared with its GOSUB.
struct HeadlinesFrame {
    headlines: Vec<PbStr>,
    report_lines: Vec<PbStr>,
    found_any: i32,
    line_num: i32,
    match_id: i32,
}

/// GOSUB MyHeadlines_MatchEntity (0x49de6f): moves every remaining headline that mentions the symbol of
/// `match_id` (`" SYM "`, `"(SYM)"`, `" SYM."`, also upper-cased) into the report as `" - <headline>"`.
fn my_headlines_match_entity(g: &Globals, f: &mut HeadlinesFrame) {
    let sym = &g.ent_symbol[f.match_id];
    let sym_space = s(" ") + sym + " ";
    let sym_paren = s("(") + sym + ")";
    let sym_dot = s(" ") + sym + ".";
    for headline_idx in 1..=60usize {
        if f.headlines[headline_idx].is_empty() {
            continue;
        }
        let headline_text = s(" - ") + &f.headlines[headline_idx] + " ";
        let h = headline_text.as_bytes();
        let hit = instr(1, h, sym_space.as_bytes()) >= 1
            || instr(1, h, sym_paren.as_bytes()) >= 1
            || instr(1, h, ucase(sym_space.as_bytes(), CaseMode::Ansi).as_bytes()) > 0
            || instr(1, h, sym_dot.as_bytes()) >= 1
            || instr(1, h, ucase(sym_dot.as_bytes(), CaseMode::Ansi).as_bytes()) > 0;
        if !hit {
            continue;
        }
        f.found_any = 1;
        f.line_num = f.line_num + 1;
        f.set(f.line_num, rtrim(h));
        f.headlines[headline_idx] = PbStr::new();
    }
}

/// `reportLines(k) = x` for the local `DIM reportLines(1 TO 140)`; out-of-range writes (error 9, no ON
/// ERROR) go to element 0 (the dummy, never shown).
impl HeadlinesFrame {
    fn set(&mut self, k: i32, x: PbStr) {
        let k = if (1..=140).contains(&k) { cast::<usize, _>(k) } else { 0 };
        self.report_lines[k] = x;
    }
}

/// Closes a section: no matches -> the heading line is blanked (the dashes line is overwritten by the
/// next heading), else two lines of space.
fn end_section(f: &mut HeadlinesFrame, section_start: i32) {
    if f.line_num == section_start {
        f.set(f.line_num - 1, PbStr::new());
    } else {
        f.line_num = f.line_num + 2;
    }
}

/// Port of ShowMyHeadlines (0x49d62b).
/// " [runtime resource wording] ": the ticker headlines (`NewsHeadline(1..60)`) that mention the
/// current player's symbol, stocks the player holds directly, companies it controls, stocks those hold,
/// and stocks the player (or a company it controls) has options on — each headline listed once, in the
/// first matching section. None: popup " [runtime resource wording] ".
pub fn show_my_headlines(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut f = HeadlinesFrame {
        headlines: vec![PbStr::new(); 61],
        report_lines: vec![PbStr::new(); 141],
        found_any: 0,
        line_num: 2,
        match_id: 0,
    };
    for i in 1..=60 {
        f.headlines[cast::<usize, _>(i)] = g.news_headline[i].clone();
    }
    f.match_id = g.cur_player;
    f.set(2,
        s(__content.string(8627)) + &ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi),
    );
    f.line_num = f.line_num + 1;
    let dashes = s("            --------------------") + &string_fill(g.cur_player_name.len(), b"-");
    f.set(f.line_num, dashes);
    let mut section_start = f.line_num;
    my_headlines_match_entity(g, &mut f);
    end_section(&mut f, section_start);

    f.set(f.line_num, s(__content.string(8628)));
    f.line_num = f.line_num + 1;
    f.set(f.line_num, s("       -------------------------------------------------------"));
    section_start = f.line_num;
    for i in 1..=15 {
        f.match_id = cast::<i32, _>(g.hold_stock_id[(i, g.cur_player)]);
        if f.match_id != 0 {
            my_headlines_match_entity(g, &mut f);
        }
    }
    end_section(&mut f, section_start);

    f.set(f.line_num, s(__content.string(8629)));
    f.line_num = f.line_num + 1;
    f.set(f.line_num, s("            ---------------------------------------"));
    section_start = f.line_num;
    for i in 11..=1600 {
        if g.entity_status[i] == 1 && g.cur_player == g.corp_controller[i] {
            f.match_id = i;
            my_headlines_match_entity(g, &mut f);
        }
    }
    end_section(&mut f, section_start);

    f.set(f.line_num, s(__content.string(8630)));
    f.line_num = f.line_num + 1;
    f.set(f.line_num, s("            -----------------------------------------------------"));
    section_start = f.line_num;
    for i in 11..=1600 {
        if g.entity_status[i] == 1 && g.cur_player == g.corp_controller[i] {
            for slot in 1..=15 {
                f.match_id = 0;
                if 0 < g.hold_stock_id[(slot, i)] {
                    f.match_id = cast::<i32, _>(g.hold_stock_id[(slot, i)]);
                    if g.cur_player != f.match_id {
                        my_headlines_match_entity(g, &mut f);
                    }
                }
            }
        }
    }
    end_section(&mut f, section_start);

    f.set(f.line_num, s(__content.string(8631)));
    f.line_num = f.line_num + 1;
    f.set(f.line_num, s("            -----------------------------------------------------"));
    section_start = f.line_num;
    for i in 1..=1000 {
        if g.option_recs[i].is_empty() {
            continue;
        }
        f.match_id = 0;
        let opt_holder_id = to_long(val(parse_any(g.option_recs[i].as_bytes(), b"|", 3).as_bytes()));
        let opt_corp_id = to_long(val(parse_any(g.option_recs[i].as_bytes(), b"|", 1).as_bytes()));
        // CorpController(optHolderId) is read even for holder 0 (error 9 without ON ERROR: dummy element)
        if g.cur_player == opt_holder_id || g.cur_player == g.corp_controller[opt_holder_id] {
            f.match_id = opt_corp_id;
        }
        if 0 < f.match_id {
            my_headlines_match_entity(g, &mut f);
        }
    }
    if f.line_num == section_start {
        f.set(f.line_num, PbStr::new());
        f.set(f.line_num - 1, PbStr::new());
    }
    if f.found_any == 0 {
        let mut msg = s(__content.string(8632));
        show_quick_message(g, ui, &mut msg);
        return;
    }
    f.line_num = f.line_num + 3;
    f.set(f.line_num + 1, s("@END"));
    for line_num in 1..=90 {
        g.text_lines[line_num] = f.report_lines[cast::<usize, _>(line_num)].clone();
    }
    g.text_box_title = s(__content.string(8633));
    g.text_has_links = 1;
    g.text_box_mode = s("74");
    show_text_report(g, ui, rng);
}
