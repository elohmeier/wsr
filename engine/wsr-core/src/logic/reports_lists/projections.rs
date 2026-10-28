//! Industry projections: `ShowIndustryGrowthProjections` (0x45f1af) and `ShowIndustryProjection`
//! (0x460058) with its GOSUB `ShowIndustryProjection_Finish` (0x4615be).
use crate::pb::real::{Ext, cast};


use super::{fmt, fmt_trim, link_id, pb_array_sort, s, str_trim};
use crate::logic::control::update_corp_control;
use crate::logic::finance::get_industry_category;
use crate::logic::report::{show_quick_message, show_text_report};
use crate::logic::research::get_etf_description;
use crate::logic::valuation::{calc_dividend_yield, calc_industry_avg_net_margin, calc_industry_outlook, calc_stock_price};
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{compare, instr, mid, replace, right, space, string_fill};
use crate::pb::numeric::{int, to_long};
use crate::state::Globals;
use crate::ui::Ui;

/// Port of ShowIndustryGrowthProjections (0x45f1af).
/// Report " [runtime resource wording] ": for every industry 1..71 the current and long-term
/// demand growth, the average net margin (`CalcIndustryAvgNetMargin`) and the profit trend from
/// `CalcIndustryOutlook` (IMPROVING / DECLINING / STABLE; "N/A" for 1, 2, 70, 71), each line linked
/// `@I<id>`; the 71 lines are sorted alphabetically (ARRAY SORT). Clears `TmpExt1..3`.
pub fn show_industry_growth_projections(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = vec![PbStr::new(); 101]; // DIM lines(1 TO 100)
    let pct_fmt = "* ###";
    let dash_line = string_fill(72, b"-");
    lines[4] = s(__content.string(8747));
    lines[5] = dash_line.clone();
    lines[6] = s(__content.string(8748));
    lines[7] = s(__content.string(8749));
    lines[8] = s(__content.string(8750));
    lines[9] = dash_line.clone();
    let mut line_no: usize = 9;
    for ind_num in 1..=71i32 {
        line_no += 1;
        let industry = cast::<i16, _>(ind_num);
        if industry < 3 || 69 < industry {
            let row_text = g.ind_name[ind_num].clone() + &space(30);
            let cell_text = s("N/A");
            let row_text = mid(row_text.as_bytes(), 1, 29)
                + &cell_text
                + &space(9)
                + &cell_text
                + &space(10)
                + &cell_text
                + &space(9)
                + &cell_text
                + " *"
                + &space(60);
            lines[line_no] = row_text;
        } else {
            let avg_margin = calc_industry_avg_net_margin(g, rng, industry);
            let margin_text = fmt(cast::<Ext, _>(avg_margin), "* ######.0") + "%";
            let margin_text = right(margin_text.as_bytes(), 9);
            let row_text = g.ind_name[ind_num].clone() + &space(30);
            let cell_text = right(fmt(cast::<Ext, _>(g.ind_growth_rate[ind_num]), pct_fmt).as_bytes(), 3);
            let mut row_text = mid(row_text.as_bytes(), 1, 28) + &cell_text + "%        ";
            let cell_text = right(fmt(cast::<Ext, _>(g.ind_long_term_growth[ind_num]), pct_fmt).as_bytes(), 3);
            row_text = row_text + &cell_text + "%     " + &margin_text;
            calc_industry_outlook(g, rng, industry);
            let mut cell_text = s("IMPROVING");
            let ind = cast::<i32, _>(industry);
            let supply_ratio = g.ind_total_assets / cast::<Ext, _>(g.ind_demand[ind]);
            g.tmp_ext1 = (Ext::from_f64(1.0) - supply_ratio) * Ext::from_f64(100.0);
            let proj_demand =
                (cast::<Ext, _>(g.ind_growth_rate[ind]) * cast::<Ext, _>(0.005f32)) * cast::<Ext, _>(g.ind_demand[ind]) + cast::<Ext, _>(g.ind_demand[ind]);
            let proj_supply_ratio = g.tmp_ext3 / proj_demand;
            g.tmp_ext2 = (Ext::from_f64(1.0) - proj_supply_ratio) * Ext::from_f64(100.0);
            if g.tmp_ext1 > g.tmp_ext2 {
                cell_text = s("DECLINING");
            }
            if int(g.tmp_ext1) == int(g.tmp_ext2) {
                cell_text = s(__content.string(8751));
            }
            g.tmp_ext1 = Ext::from_f64(0.0);
            g.tmp_ext2 = Ext::from_f64(0.0);
            g.tmp_ext3 = Ext::from_f64(0.0);
            lines[line_no] = row_text + "     " + &cell_text + &space(60);
        }
        lines[line_no] = lines[line_no].clone() + "@I" + &link_id(ind_num);
    }
    // ARRAY SORT lines(10) FOR 71
    pb_array_sort::<PbStr, ()>(&mut lines[10..81], None, false, &|a, b| {
        compare(a.as_bytes(), b.as_bytes()) == std::cmp::Ordering::Less
    });
    line_no += 1;
    lines[line_no] = dash_line;
    line_no += 1;
    lines[line_no] = s(__content.string(8752));
    line_no += 3;
    lines[line_no] = s("@END");
    for (k, l) in lines.iter().enumerate().take(line_no + 1).skip(1) {
        g.text_lines[cast::<i32, _>(k)] = l.clone();
    }
    g.text_box_title = s(__content.string(8753));
    g.text_has_links = 1;
    g.text_box_mode = s("40");
    show_text_report(g, ui, rng);
}

/// Port of ShowIndustryProjection (0x460058).
/// Report "<industry>  [runtime resource wording] ": every active corp of the industry
/// (ids 131..1600) with price/net worth, credit rating (bond marks `*`/`**`), dividend yield, projected
/// pre-tax return, projected market share (`_P`/`_C`/`_H<n>` control marks), growth and discretionary
/// spending, sorted by market share (descending); then the industry demand growth and the legend.
/// Revalues each listed corp (`ValuationMode = 2`, `CalcStockPrice`; `time_secs` = the seconds of
/// `TIME$` it uses), clamps `GrowthRate` to -10..60 and runs `UpdateCorpControl`. Industries 1, 2 and
/// 70 (and ids < 1) get a "no projections" note; 71 (funds) the ETF strategies report.
pub fn show_industry_projection(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, industry_id: i32, time_secs: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = vec![PbStr::new(); 991]; // DIM lines(1 TO 990)
    let mut sort_keys = vec![0f32; 991]; // DIM sortKeys(1 TO 990) AS SINGLE
    let pct_fmt = "* ###.0";
    let dash_line = string_fill(83, b"-");
    let industry = cast::<i16, _>(industry_id);
    let mut line_no: i32;
    if industry < 3 || 69 < industry {
        if industry == 71 {
            show_etf_strategies_report(g, ui, rng);
            return;
        }
        if industry < 1 {
            lines[4] = s(__content.string(8604));
        } else {
            lines[4] = s(__content.string(8754));
            lines[6] = s("     ") + &g.ind_name[cast::<i32, _>(industry)];
            if industry < 3 {
                lines[6] = lines[6].clone() + __content.string(8755);
            }
        }
        line_no = 5;
        finish(g, ui, rng, &mut lines, line_no, industry);
        return;
    }
    let mut row_text = s(__content.string(8756)) + &g.ind_name[cast::<i32, _>(industry)] + __content.string(8757);
    let half_len = to_long(cast::<Ext, _>(0.5f32) * cast::<Ext, _>(row_text.len()));
    let pad_len = 42 - half_len;
    row_text = space(pad_len) + &row_text;
    lines[3] = row_text;
    lines[4] = dash_line;
    let category = get_industry_category(industry);
    let mut col_text = s(__content.string(8758));
    if category == 2 {
        col_text = s("MARKETING");
    }
    lines[5] = s(__content.string(8759));
    lines[6] = s(__content.string(8760));
    lines[7] = s(__content.string(8761)) + &col_text;
    lines[8] = s("--------  ---------  -------  -----  --------------  --------  --------  ----------");
    line_no = 8;
    calc_industry_outlook(g, rng, industry);
    let industry_outlook = cast::<f32, _>(g.tmp_ext2);
    let proj_capacity = g.tmp_ext3;
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    g.tmp_ext3 = Ext::from_f64(0.0);
    let mut market_share: f32 = cast::<f32, _>(Ext::from_f32(0.0f32));
    let mut row_count = 0;
    let mut has_bonds = 0;
    let mut has_conv_bonds = 0;
    let mut has_ctrl_marks = 0;
    let mut hostile_num = PbStr::new();
    let mut corp_id = 131;
    loop {
        if g.entity_status[corp_id] == 1 && g.industry_num[corp_id] == industry {
            if 60 < g.growth_rate[corp_id] {
                g.growth_rate[corp_id] = 60;
            }
            if g.growth_rate[corp_id] < -10 {
                g.growth_rate[corp_id] = -10;
            }
            let mut row_text = g.ent_symbol[corp_id].clone() + &space(10);
            row_text = mid(row_text.as_bytes(), 1, 10);
            g.valuation_mode = 2;
            calc_stock_price(g, ui, rng, corp_id, time_secs);
            let mut cell_text = PbStr::new();
            if Ext::from_f64(0.0) < g.net_worth {
                let ratio = crate::pb::numeric::double((Ext::from_f64(100.0) * g.market_cap[corp_id]) / g.net_worth); // stored as DOUBLE
                if Ext::from_f64(100.0) > ratio {
                    let spend_text = fmt(Ext::from_f64(100.0) * ratio, "* ####");
                    cell_text = s(" ") + &right(spend_text.as_bytes(), 4) + "% " + &space(5);
                } else {
                    cell_text = PbStr::new();
                }
            }
            if cell_text.is_empty() {
                cell_text = s("   NMF") + &space(6);
            }
            row_text = row_text + &cell_text;
            let mut col_text = s("    ") + &g.credit_rating_str;
            col_text = right(col_text.as_bytes(), 3);
            if g.bonds_public_held <= 0 {
                col_text = col_text + &space(4);
            } else if instr(1, g.bond_issue_info[corp_id].as_bytes(), b"cv") < 1 {
                col_text = col_text + " *  ";
                has_bonds = 1;
            } else {
                col_text = col_text + " ** ";
                has_conv_bonds = 1;
            }
            row_text = row_text + &col_text;
            let ratio = crate::pb::numeric::double(calc_dividend_yield(g, corp_id)); // stored as DOUBLE
            let yield_text = fmt(ratio, "* ###.0");
            row_text = row_text + &right(yield_text.as_bytes(), 5) + "%     ";
            let assets = g.business_assets[corp_id];
            let proj_assets = (cast::<Ext, _>(0.005f32) * assets) * cast::<Ext, _>(g.growth_rate[corp_id]) + assets;
            if Ext::from_f64(0.0) < proj_capacity {
                market_share = cast::<f32, _>((proj_assets / proj_capacity) * Ext::from_f64(100.0));
            }
            let mut proj_return = cast::<f32, _>(cast::<Ext, _>(market_share) * cast::<Ext, _>(0.2f32));
            proj_return = cast::<f32, _>(cast::<Ext, _>(proj_return) - Ext::from_f64(2.0));
            if Ext::from_f32(8.0f32) < proj_return {
                proj_return = 5.0f32;
            }
            proj_return = cast::<f32, _>((cast::<Ext, _>(industry_outlook) + cast::<Ext, _>(proj_return)) + cast::<Ext, _>(g.profit_margin[corp_id])
                - cast::<Ext, _>(g.disc_spend_pct[corp_id]));
            let mut cell_text = s("      ") + &fmt_trim(cast::<Ext, _>(proj_return), "#####0.0");
            cell_text = right(cell_text.as_bytes(), 7) + "%      ";
            cell_text = mid(cell_text.as_bytes(), 1, 13);
            let mut col_text = fmt(cast::<Ext, _>(market_share), pct_fmt);
            col_text = right(col_text.as_bytes(), 5) + "%       ";
            update_corp_control(g, ui, rng, corp_id);
            if g.cur_player == g.corp_controller[corp_id] {
                col_text = replace(col_text.as_bytes(), b"%  ", b"%_P");
                has_ctrl_marks = 1;
            }
            if 10 < g.corp_controller[corp_id] && corp_id != g.corp_controller[corp_id] {
                col_text = replace(col_text.as_bytes(), b"%  ", b"%_C");
                has_ctrl_marks = 1;
            }
            let ctrl = g.corp_controller[corp_id];
            if 0 < ctrl && ctrl <= g.num_players && g.cur_player != ctrl {
                hostile_num = str_trim(cast::<Ext, _>(ctrl));
                col_text = replace(col_text.as_bytes(), b"%  ", b"%_H");
                let with = s("%_H") + &hostile_num;
                col_text = replace(col_text.as_bytes(), b"%_H ", with.as_bytes());
                has_ctrl_marks = 1;
            }
            col_text = mid(col_text.as_bytes(), 1, 12);
            let mut growth_text = fmt(cast::<Ext, _>(g.growth_rate[corp_id]), "* ###");
            growth_text = right(growth_text.as_bytes(), 3) + "%      ";
            growth_text = mid(growth_text.as_bytes(), 1, 8);
            let spend_text = fmt(cast::<Ext, _>(g.disc_spend_pct[corp_id]), "* ###") + "%";
            line_no += 1;
            row_count += 1;
            lines[cast::<usize, _>(line_no)] = row_text + &cell_text + &col_text + &growth_text + &spend_text;
            if g.entity_pick_mode == __content.string(6093) {
                lines[cast::<usize, _>(line_no)] = lines[cast::<usize, _>(line_no)].clone() + &space(50) + "@C" + &link_id(corp_id);
            }
            sort_keys[cast::<usize, _>(line_no)] = market_share;
            if line_no == 984 {
                line_no = 985;
                lines[cast::<usize, _>(line_no)] = s(__content.string(8619));
            }
            if line_no == 975 {
                corp_id = 1600;
            }
        }
        corp_id += 1;
        if 1600 < corp_id {
            break;
        }
    }
    // ARRAY SORT sortKeys(9) FOR rowCount, TAGARRAY lines(), DESCEND
    let n = cast::<usize, _>(row_count);
    pb_array_sort(&mut sort_keys[9..9 + n], Some(&mut lines[9..9 + n]), true, &|a: &f32, b: &f32| a < b);
    line_no += 1;
    let l = cast::<usize, _>(line_no);
    lines[l] = lines[8].clone();
    let cell_text = right((fmt(cast::<Ext, _>(g.ind_growth_rate[cast::<i32, _>(industry)]), "* ###") + "%").as_bytes(), 4);
    lines[l + 1] = s(__content.string(8762)) + &cell_text;
    let col_text = right((fmt(cast::<Ext, _>(g.ind_long_term_growth[cast::<i32, _>(industry)]), "* ###") + "%").as_bytes(), 4);
    lines[l + 2] = s(__content.string(8763)) + &col_text;
    lines[l + 3] = string_fill(83, b"-");
    if has_bonds == 1 {
        lines[cast::<usize, _>(line_no) + 4] =
            space(10) + __content.string(8620);
        line_no += 1;
    }
    if has_conv_bonds == 1 {
        lines[cast::<usize, _>(line_no) + 4] = space(10) + __content.string(8621);
        line_no += 1;
    }
    if has_ctrl_marks == 1 {
        lines[cast::<usize, _>(line_no) + 4] = space(10) + __content.string(8622) + &g.cur_player_name;
        lines[cast::<usize, _>(line_no) + 5] = space(10) + __content.string(8623);
        if !hostile_num.is_empty() {
            line_no += 1;
            let hostile_id = to_long(val(hostile_num.as_bytes()));
            lines[cast::<usize, _>(line_no) + 5] = space(15)
                + "(_H"
                + &hostile_num
                + __content.string(8593)
                + &hostile_num
                + ", "
                + &g.player_name[hostile_id]
                + ")";
        }
        lines[cast::<usize, _>(line_no) + 6] = space(10) + __content.string(8624);
        line_no += 3;
    }
    line_no += 1;
    finish(g, ui, rng, &mut lines, line_no, industry);
}

/// Port of ShowEtfStrategiesReport (0x64aa96).
/// Lists the active ETFs and their investment strategies, with company links and the original fee
/// and distribution notice. Inactive fund slots are changed to industry 70. With no ETFs, shows the
/// explanation for the saved game's version instead of a report.
pub fn show_etf_strategies_report(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    let mut lines = vec![PbStr::new(); 101];
    lines[2] = s(__content.string(8764));
    lines[3] = s("               -----------------------------------------------");
    lines[5] = s(__content.string(8765));
    lines[6] = s("-------------------------  --------------------------------------------------------");
    let mut line_no = 6;
    for etf_id in 1581..=1600 {
        if g.entity_status[etf_id] != 1 {
            g.industry_num[etf_id] = 70;
        }
        if g.industry_num[etf_id] == 71 && g.entity_status[etf_id] == 1 {
            line_no += 1;
            let name = g.ent_name[etf_id].clone() + &space(20);
            lines[line_no] = mid(name.as_bytes(), 1, 27) + &get_etf_description(etf_id)
                + &space(50) + "@C" + &str_trim(cast::<Ext, _>(etf_id));
        }
    }
    if line_no == 6 {
        let mut message = s(__content.string(8766));
        if cast::<Ext, _>(6.3f32) <= val(g.saved_game_version.as_bytes()) {
            message = message + __content.string(8767)
                + __content.string(8768)
                + __content.string(8769)
                + __content.string(8770)
                + __content.string(8771);
        } else {
            message = message + __content.string(8772) + &g.saved_game_version
                + __content.string(8773)
                + __content.string(8774);
        }
        show_quick_message(g, ui, &mut message);
        return;
    }
    line_no += 1;
    lines[line_no] = lines[6].clone();
    g.entity_pick_mode = s(__content.string(6093));
    line_no += 2;
    for (offset, text) in [
        __content.string(8775),
        __content.string(8776),
        __content.string(8777),
        __content.string(8778),
        __content.string(8779),
        __content.string(8780),
    ].iter().enumerate() {
        lines[line_no + offset] = s(text);
    }
    lines[line_no + 8] = s("@END");
    for (i, line) in lines.iter().enumerate().skip(1) {
        g.text_lines[cast::<i32, _>(i)] = line.clone();
    }
    g.text_box_title = s(__content.string(8781));
    g.text_box_mode = s("78");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Port of ShowIndustryProjection_Finish (0x4615be), GOSUB of ShowIndustryProjection: ends the report
/// with `@END` (after 3 or 8 more lines) and shows it.
fn finish(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, lines: &mut [PbStr], mut line_no: i32, industry: i16) {
    let __content = crate::content::current();
    if 16 < line_no && line_no < 980 {
        line_no += 5;
    }
    line_no += 3;
    lines[cast::<usize, _>(line_no)] = s("@END");
    for k in 1..=line_no {
        g.text_lines[k] = lines[cast::<usize, _>(k)].clone();
    }
    g.text_box_title = s("=== ") + &g.ind_name[cast::<i32, _>(industry)] + __content.string(8782);
    g.text_box_mode = s("80");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}
