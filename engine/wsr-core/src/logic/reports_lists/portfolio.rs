//! Portfolio listings: `ShowPortfolioListing` (0x4422a4) with its GOSUB `Totals` (0x445f97), and
//! `ShowOptionPositions` (0x5f51fc) with its GOSUB `BuildRows` (0x5f69db).
use crate::pb::real::{Ext, cast};


use super::{fmt, fmt_trim, link_id, s, str_trim};
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::calc_cur_month_of_year;
use crate::logic::finance::calc_entity_financials;
use crate::logic::options::{calc_option_price, sort_option_recs};
use crate::logic::report::{describe_defunct_entity, show_quick_message, show_text_report};
use crate::logic::shorts::recalc_short_margin_balances;
use crate::logic::valuation::{calc_analyst_rating, calc_dividend_yield, check_stock_split};
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, mid, parse_any, replace, right, rtrim, string_fill, ucase, CaseMode};
use crate::pb::numeric::{ceil, to_long, Cux};
use crate::state::Globals;
use crate::ui::Ui;

/// SINGLE constant of the original's pool (`fmul dword [0x75e…]`).
fn sf(x: f32) -> Ext {
    cast::<Ext, _>(x)
}

fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// `STRING$(n, " ")`.
fn sp(n: i32) -> PbStr {
    string_fill(n, b" ")
}

fn rt(x: &PbStr, n: i32) -> PbStr {
    right(x.as_bytes(), n)
}

/// The currency label of the listings' headers: `CurrencySuffix`, or `" " + CurrencyName` if that is
/// empty or contains "SwFr"; pounds, dollars and francs get a plural "s".
fn currency_label(g: &Globals) -> PbStr {
    let mut t = g.currency_suffix.clone();
    if t.is_empty() || 0 < instr(1, t.as_bytes(), b"SwFr") {
        t = s(" ") + &g.currency_name;
    }
    if (0 < instr(1, t.as_bytes(), b"Pound")
        || 0 < instr(1, t.as_bytes(), b"Dollar")
        || 0 < instr(1, t.as_bytes(), b"Franc"))
        && !(rt(&t, 1) == "s")
    {
        t = t + "s";
    }
    t
}

/// Locals of ShowPortfolioListing shared with its GOSUB Totals.
struct PlFrame {
    lines: Vec<PbStr>,
    line_no: i32,
    listing_kind: i32,
    portfolio_kind: PbStr,
    stock_value_total: Ext,
    corp_bonds_value: Ext,
    gov_bonds_value: Ext,
    has_links: i32,
    owner_id: i32,
    entity_id: i32,
}

impl PlFrame {
    fn set(&mut self, i: i32, x: PbStr) {
        self.lines[cast::<usize, _>(i)] = x;
    }
    fn get(&self, i: i32) -> PbStr {
        self.lines[cast::<usize, _>(i)].clone()
    }
}

/// Port of ShowPortfolioListing (0x4422a4).
/// Report "=== <kind> PORTFOLIO LISTING FOR <NAME> ===": the stock holdings of `entity_id` (with price,
/// dividend yield, value and analyst rating — or tax basis when `TmpExt1 < 0` on entry) and/or its bond
/// holdings (government and corporate), with totals. The listing kind is `ROUND(ABS(TmpExt1))` (1 =
/// stocks only, 2 = bonds only, 3 = both with a grand total); `TmpExt1` is cleared. `bond_mode > 0` lists
/// corporate bonds only; `position_mode` 0 = long positions, 1 = long and short, -1 = short positions.
/// Without any stock/bond position it shows the option positions (`ShowOptionPositions`) after a
/// message, or a "No positions found" popup.
pub fn show_portfolio_listing(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    bond_mode: i32,
    position_mode: i32,
) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    // (UI) DIALOG END DlgWhoOwnsWhat
    let dash_line = string_fill(84, b"-");
    let stock_sep_line = s("---------------------------- ----- ---- -------- ----- -------------- --------------");
    let bond_sep_line =
        s("------------------------- -------------- ------- ----------- ----------- ----------- ---------");
    let mut fr = PlFrame {
        lines: vec![PbStr::new(); 1501],
        line_no: 0,
        listing_kind: to_long(g.tmp_ext1.abs()),
        portfolio_kind: PbStr::new(),
        stock_value_total: Ext::from_f64(0.0),
        corp_bonds_value: Ext::from_f64(0.0),
        gov_bonds_value: Ext::from_f64(0.0),
        has_links: 0,
        owner_id: 0,
        entity_id,
    };
    let mut display_mode = 0;
    if g.tmp_ext1 < Ext::from_f64(0.0) {
        display_mode = 3;
    }
    g.tmp_ext1 = Ext::from_f64(0.0);
    if g.entity_status[entity_id] < 1 {
        return;
    }
    if g.entity_status[entity_id] == 2 {
        describe_defunct_entity(g, ui, entity_id);
        return;
    }
    fr.owner_id = entity_id;
    let owner_id = entity_id;
    calc_entity_financials(g, ui, rng, entity_id);
    fr.portfolio_kind = s("STOCK");
    if g.options_net_value != Ext::from_f64(0.0) || Ext::from_f64(0.0) < g.long_options_value {
        fr.portfolio_kind = s("EQUITY");
    }
    if g.tmp_corp_bonds_value == Ext::from_f64(0.0) && g.tmp_gov_bonds_value == Ext::from_f64(0.0) && g.tmp_subsid_stock_value == Ext::from_f64(0.0) {
        if g.options_net_value == Ext::from_f64(0.0) && find_option_positions(g, ui, entity_id, 0, 0, 0) == 0 {
            let row_text = if owner_id < 11 { s("You do") } else { g.ent_name[owner_id].clone() + " does" };
            let mut msg = s(__content.string(8678)) + "\r\n" + "\r\n" + &row_text
                + __content.string(8679);
            show_quick_message(g, ui, &mut msg);
            return;
        }
        let text = s(__content.string(8680)) + &g.ent_name[owner_id] + ":"
            + "\r" + "\r" + __content.string(8681);
        ui.msgbox(&text, 8256, &s(__content.string(8682)));
        show_option_positions(g, ui, rng, owner_id, 0, 0);
        return;
    }
    let mut stock_basis_total = Ext::from_f64(0.0);
    let mut bond_basis_total = Ext::from_f64(0.0);
    let mut long_bond_value = Ext::from_f64(0.0);
    let mut has_excess_loss = 0;
    let mut stock_count = 0;
    let mut col_text;
    let mut field_text: PbStr;
    let mut row_text;
    let mut bond_kind_text = PbStr::new();
    if fr.listing_kind != 2 {
        col_text = if owner_id < 11 { PbStr::new() } else { s(" (") + &g.ent_symbol[owner_id] + ")" };
        field_text = currency_label(g);
        if position_mode == -1 {
            fr.portfolio_kind = s("SHORT");
        }
        fr.set(
            2,
            s("   ") + &fr.portfolio_kind + __content.string(8683) + &ucase(g.ent_name[owner_id].as_bytes(), CaseMode::Ansi)
                + &col_text + " (In" + &field_text + ")",
        );
        fr.set(3, dash_line.clone());
        fr.set(4, s(__content.string(8684)));
        fr.set(
            5,
            s(__content.string(8685)) + &g.million_prefix_uc
                + __content.string(8686),
        );
        if display_mode == 3 {
            fr.set(4, replace(fr.get(4).as_bytes(), __content.bytes(8687), __content.bytes(8688)));
            fr.set(5, replace(fr.get(5).as_bytes(), __content.bytes(8689), __content.bytes(8690)));
        }
        fr.set(6, s("---------------------------- ----- ---- -------- ----- -------------- --------------"));
        fr.line_no = 6;
        for slot in 1..=15 {
            if 0 < g.hold_stock_id[(slot, entity_id)] {
                if (cast::<Ext, _>(position_mode)).abs() == Ext::from_f64(1.0) || 0 < g.hold_stock_pct[(slot, entity_id)] {
                    stock_count = stock_count + 1;
                }
                if position_mode == -1 && 0 < g.hold_stock_pct[(slot, entity_id)] {
                    stock_count = stock_count - 1;
                }
            }
        }
        if g.options_net_value != Ext::from_f64(0.0) || Ext::from_f64(0.0) < g.long_options_value {
            stock_count = stock_count + 1;
            field_text = s(__content.string(8691));
        } else {
            field_text = PbStr::new();
        }
        if stock_count == 0 {
            fr.set(7, s("  ") + &g.ent_name[owner_id] + __content.string(8692) + &field_text + __content.string(8693));
            fr.set(8, stock_sep_line.clone());
            fr.line_no = 11;
        } else {
            fr.stock_value_total = Ext::from_f64(0.0);
            for slot in 1..=15 {
                if g.hold_stock_id[(slot, entity_id)] < 1 {
                    continue; // positionValue = 0
                }
                let pct_held = g.hold_stock_pct[(slot, entity_id)];
                if pct_held < 0 && position_mode == 0 {
                    continue;
                }
                if 0 < pct_held && position_mode == -1 {
                    continue;
                }
                fr.has_links = 1;
                fr.line_no = fr.line_no + 1;
                let corp_id = cast::<i32, _>(g.hold_stock_id[(slot, entity_id)]);
                row_text = g.ent_name[corp_id].clone() + &sp(35);
                row_text = mid(row_text.as_bytes(), 1, 29);
                col_text = g.ent_symbol[corp_id].clone() + "   ";
                col_text = mid(col_text.as_bytes(), 1, 4);
                row_text = row_text + &col_text + &fmt(cast::<Ext, _>(pct_held), "* ###") + "%";
                field_text = fmt(g.market_cap[corp_id] / g.shares_outstanding[corp_id], "* #####.00");
                field_text = rt(&field_text, 9);
                row_text = row_text + &field_text;
                let mut div_yield = cast::<f32, _>(calc_dividend_yield(g, corp_id));
                if 0.05f32 > div_yield {
                    div_yield = 0.0f32;
                }
                if pct_held < 0 {
                    div_yield = -div_yield; // fld1; fchs; fmul dword: exact
                }
                field_text = fmt(cast::<Ext, _>(div_yield), "* ###.0") + "%";
                field_text = rt(&field_text, 6);
                row_text = row_text + &field_text;
                let position_value = g.market_cap[corp_id] * cast::<Ext, _>(pct_held);
                field_text = fmt_trim(position_value, "* ###,");
                if 15 < field_text.len() {
                    field_text = fmt_trim(position_value, "* ###");
                }
                fr.stock_value_total = fr.stock_value_total + position_value;
                field_text = sp(15) + &field_text;
                field_text = rt(&field_text, 15);
                fr.set(fr.line_no, row_text + &field_text);
                if display_mode == 3 {
                    let mut cost_basis = g.hold_stock_basis[(slot, entity_id)];
                    if pct_held < 0 {
                        cost_basis = -cost_basis; // * -1
                    }
                    stock_basis_total = stock_basis_total + cost_basis;
                    col_text = fmt_trim(cost_basis, "* ######,");
                    if 14 < col_text.len() {
                        col_text = fmt_trim(cost_basis, "* ######");
                    }
                    col_text = sp(15) + &col_text;
                    col_text = rt(&col_text, 15);
                    if g.hold_stock_basis[(slot, entity_id)] < Ext::from_f64(0.0) {
                        has_excess_loss = 1;
                        col_text = col_text + "*";
                    }
                    fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
                } else {
                    let analyst_rating = calc_analyst_rating(g, ui, rng, corp_id);
                    col_text = s(__content.string(6175));
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
                    update_corp_control(g, ui, rng, corp_id);
                    if g.public_pct == 0 {
                        col_text = s(__content.string(6180));
                    }
                    fr.set(fr.line_no, fr.get(fr.line_no) + "   " + &col_text);
                }
                if g.entity_pick_mode == "GetCorp" || g.entity_pick_mode == __content.string(6093) {
                    fr.set(fr.line_no, fr.get(fr.line_no) + &sp(20) + "@C" + &link_id(corp_id));
                }
                fr.line_no = fr.line_no + 1;
                fr.set(fr.line_no, stock_sep_line.clone());
            }
            calc_entity_financials(g, ui, rng, owner_id);
            if (g.options_net_value != Ext::from_f64(0.0) || Ext::from_f64(0.0) < g.long_options_value) && position_mode != -1 {
                if display_mode == 3 && g.options_cost_basis != Ext::from_f64(0.0) {
                    col_text = fmt_trim(g.options_cost_basis, "* ###,");
                    if 15 < col_text.len() {
                        col_text = mid(fmt(g.options_cost_basis, "* ###").as_bytes(), 1, 15);
                    }
                    col_text = sp(15) + &col_text;
                    col_text = rt(&col_text, 15);
                } else {
                    col_text = s("       ----");
                }
                fr.line_no = fr.line_no + 1;
                row_text = fmt_trim(g.options_net_value, "* #############,");
                if 14 < row_text.len() {
                    row_text = mid(fmt(g.options_net_value, "* #############").as_bytes(), 1, 14);
                }
                row_text = sp(14) + &row_text;
                row_text = rt(&row_text, 14);
                fr.set(fr.line_no, s(__content.string(8694)) + &row_text + &col_text);
                fr.line_no = fr.line_no + 1;
                fr.set(fr.line_no, stock_sep_line.clone());
            }
            fr.line_no = fr.line_no + 1;
            col_text = g.currency_symbol.clone();
            let kind_text = fr.portfolio_kind.clone();
            row_text = s(__content.string(8695)) + &kind_text + __content.string(8696) + "(" + &col_text + &g.million_prefix_uc
                + "illion" + &g.currency_suffix + "):           ";
            row_text = mid(row_text.as_bytes(), 1, 54);
            let mut total_value = fr.stock_value_total + g.options_net_value;
            if g.cur_player == entity_id && position_mode == -1 {
                recalc_short_margin_balances(g);
                total_value = -g.player_short_margin_bal[g.cur_player]; // * -1
            }
            field_text = fmt_trim(total_value, "* ###,");
            if 15 < field_text.len() {
                field_text = mid(fmt_trim(total_value, "* ###").as_bytes(), 1, 15);
            }
            field_text = sp(15) + &field_text;
            let skip_total = bond_mode == 0
                && position_mode == 0
                && display_mode == 3
                && g.cur_player == entity_id
                && fr.listing_kind == 1;
            if !skip_total {
                field_text = rt(&field_text, 15);
                fr.set(fr.line_no, row_text.clone() + &field_text);
                col_text = PbStr::new();
                if stock_basis_total + g.options_cost_basis != Ext::from_f64(0.0) && display_mode == 3 {
                    total_value = stock_basis_total + g.options_cost_basis;
                    col_text = fmt_trim(total_value, "* ###,");
                    if 14 < col_text.len() {
                        col_text = fmt_trim(total_value, "* ###");
                    }
                    if 14 < col_text.len() {
                        col_text = s(__content.string(8697));
                    }
                    col_text = sp(15) + &col_text;
                    col_text = rt(&col_text, 15);
                    fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
                }
                fr.line_no = fr.line_no + 1;
                fr.set(fr.line_no, sp(55) + &string_fill(14, b"="));
                if !col_text.is_empty() {
                    fr.set(fr.line_no, fr.get(fr.line_no) + " " + &string_fill(14, b"="));
                }
                fr.line_no = fr.line_no + 1;
            }
            if 0 < has_excess_loss {
                fr.line_no = fr.line_no + 1;
                let n = fr.line_no;
                fr.set(n + 1, s(__content.string(8698)));
                fr.set(n + 2, s(__content.string(8699)));
                fr.set(n + 3, s(__content.string(8700)));
                fr.set(n + 4, s(__content.string(8701)));
                fr.line_no = fr.line_no + 6;
            }
        }
        if 1583 < owner_id || fr.listing_kind == 1 {
            totals(g, ui, rng, &mut fr);
            return;
        }
    }
    calc_entity_financials(g, ui, rng, owner_id);
    fr.line_no = fr.line_no + 2;
    col_text = if owner_id < 11 { PbStr::new() } else { s(" (") + &g.ent_symbol[owner_id] + ")" };
    field_text = currency_label(g);
    if 0 < bond_mode {
        bond_kind_text = s(__content.string(8702));
    }
    let n = fr.line_no;
    fr.set(
        n,
        s("   ") + &bond_kind_text + __content.string(8703) + &ucase(g.ent_name[owner_id].as_bytes(), CaseMode::Ansi)
            + &col_text + " (In" + &field_text + ")",
    );
    fr.set(n + 1, dash_line.clone() + &string_fill(9, b"-"));
    fr.set(n + 2, s(__content.string(8704)));
    fr.set(n + 3, s(__content.string(8705)));
    if 1 < display_mode {
        fr.set(n + 2, replace(fr.get(n + 2).as_bytes(), __content.bytes(8706), b"ADJUSTED"));
        fr.set(n + 3, replace(fr.get(n + 3).as_bytes(), __content.bytes(8707), __content.bytes(8688)));
    }
    fr.set(n + 4, bond_sep_line.clone());
    fr.line_no = fr.line_no + 5;
    if g.tmp_corp_bonds_value == Ext::from_f64(0.0) && g.tmp_gov_bonds_value == Ext::from_f64(0.0) {
        fr.set(fr.line_no, s("  ") + &g.ent_name[owner_id] + __content.string(8708));
        fr.set(fr.line_no + 1, dash_line.clone() + &string_fill(10, b"-"));
        totals(g, ui, rng, &mut fr);
        return;
    }
    if Ext::from_f64(0.0) < g.ci_gov_long_bond_value && bond_mode == 0 {
        row_text = s(__content.string(8709)) + &sp(15);
        row_text = mid(row_text.as_bytes(), 1, 26);
        col_text = rt(&g.long_bond_desc, 5);
        if mid(col_text.as_bytes(), 1, 1) == "0" {
            col_text = s(" ") + &rt(&col_text, 4);
        }
        field_text = mid(g.long_bond_desc.as_bytes(), 1, 4);
        row_text = row_text + &col_text + "% of " + &field_text;
        field_text = s("        ") + &fmt(cast::<Ext, _>(g.long_bond_price), "* ###.00");
        field_text = rt(&field_text, 8);
        row_text = row_text + &field_text;
        field_text = sp(12) + &fmt(cast::<Ext, _>(g.long_gov_bond_face[owner_id]), "* ###,");
        field_text = rt(&field_text, 12);
        row_text = row_text + &field_text;
        long_bond_value = cast::<Ext, _>(g.long_bond_price) * cast::<Ext, _>(g.long_gov_bond_face[owner_id]) * sf(0.01f32);
        fr.gov_bonds_value = g.short_bond_value + long_bond_value;
        field_text = sp(12) + &fmt(long_bond_value, "* ###,");
        field_text = rt(&field_text, 12);
        fr.set(fr.line_no, row_text.clone() + &field_text);
        if display_mode == 0 || 1583 < entity_id {
            col_text = sp(9) + &fmt(cast::<Ext, _>(g.long_bond_yield), "* ###.00") + "%";
            col_text = rt(&col_text, 9);
        } else {
            let cost_basis = g.lt_gov_bond_cost[owner_id];
            col_text = sp(12) + &fmt(cost_basis, "* ###,");
            col_text = rt(&col_text, 12);
            bond_basis_total = cost_basis;
        }
        fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
        col_text = s(__content.string(8710));
        if g.gdp_growth < -Ext::from_f32(10.0f32) {
            col_text = replace(col_text.as_bytes(), b"AAA", b" AA");
        }
        if display_mode != 0 {
            col_text = rt(&col_text, 7);
        }
        fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
        fr.line_no = fr.line_no + 1;
        fr.set(fr.line_no, bond_sep_line.clone());
        fr.line_no = fr.line_no + 1;
        if 1583 < owner_id {
            g.tmp_corp_bonds_value = Ext::from_f64(0.0);
        }
        if g.tmp_corp_bonds_value == Ext::from_f64(0.0) && g.short_bond_value == Ext::from_f64(0.0) {
            totals(g, ui, rng, &mut fr);
            return;
        }
    }
    if Ext::from_f64(0.0) < g.short_bond_value && bond_mode == 0 {
        row_text = s(__content.string(8711)) + &sp(14);
        row_text = mid(row_text.as_bytes(), 1, 26);
        col_text = rt(&g.short_bond_desc, 5);
        if mid(col_text.as_bytes(), 1, 1) == "0" {
            col_text = s(" ") + &rt(&col_text, 4);
        }
        field_text = mid(g.short_bond_desc.as_bytes(), 1, 4);
        row_text = row_text + &col_text + "% of " + &field_text;
        field_text = s("        ") + &fmt(cast::<Ext, _>(g.short_bond_price), "* ###.00");
        field_text = rt(&field_text, 8);
        row_text = row_text + &field_text;
        field_text = sp(12) + &fmt(cast::<Ext, _>(g.short_gov_bond_face[owner_id]), "* ###,");
        field_text = rt(&field_text, 12);
        row_text = row_text + &field_text;
        let short_bond_value = cast::<Ext, _>(g.short_bond_price) * cast::<Ext, _>(g.short_gov_bond_face[owner_id]) * sf(0.01f32);
        fr.gov_bonds_value = short_bond_value + long_bond_value;
        field_text = sp(12) + &fmt(short_bond_value, "* ###,");
        field_text = rt(&field_text, 12);
        fr.set(fr.line_no, row_text.clone() + &field_text);
        if display_mode == 0 || 1583 < entity_id {
            col_text = sp(9) + &fmt(cast::<Ext, _>(g.short_bond_yield), "* ###.00") + "%";
            col_text = rt(&col_text, 9);
        } else {
            let cost_basis = g.st_gov_bond_cost[owner_id];
            col_text = sp(12) + &fmt(cost_basis, "* ###,");
            col_text = rt(&col_text, 12);
            bond_basis_total = bond_basis_total + cost_basis;
        }
        fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
        col_text = s(__content.string(8710));
        if g.gdp_growth < -Ext::from_f32(10.0f32) {
            col_text = replace(col_text.as_bytes(), b"AAA", b" AA");
        }
        if display_mode != 0 {
            col_text = rt(&col_text, 7);
        }
        fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
        fr.line_no = fr.line_no + 1;
        fr.set(fr.line_no, bond_sep_line.clone());
        fr.line_no = fr.line_no + 1;
        if 1583 < owner_id {
            g.tmp_corp_bonds_value = Ext::from_f64(0.0);
        }
        if g.tmp_corp_bonds_value == Ext::from_f64(0.0) {
            totals(g, ui, rng, &mut fr);
            return;
        }
    }
    fr.line_no = fr.line_no - 1;
    fr.corp_bonds_value = Ext::from_f64(0.0);
    for slot in 1..=15 {
        if 0 < g.hold_bond_id[(slot, owner_id)] {
            fr.has_links = 1;
            fr.line_no = fr.line_no + 1;
            let issuer_id = g.hold_bond_id[(slot, owner_id)];
            calc_entity_financials(g, ui, rng, issuer_id);
            let position_value = sf(0.01f32) * g.bond_price[issuer_id].get() * cast::<Ext, _>(g.hold_bond_face[(slot, owner_id)]);
            fr.corp_bonds_value = fr.corp_bonds_value + position_value;
            row_text = g.ent_name[issuer_id].clone() + &sp(26);
            if instr(1, g.bond_issue_info[issuer_id].as_bytes(), b"cv") < 1 {
                row_text = mid(row_text.as_bytes(), 1, 26);
            } else {
                row_text = mid(row_text.as_bytes(), 1, 22) + " cv.";
            }
            col_text = rt(&g.bond_issue_info[issuer_id], 5);
            if mid(col_text.as_bytes(), 1, 1) == "0" {
                col_text = s(" ") + &rt(&col_text, 4);
            }
            field_text = mid(g.bond_issue_info[issuer_id].as_bytes(), 1, 4);
            row_text = row_text + &col_text + "% of " + &field_text;
            field_text = sp(8) + &fmt(g.bond_price[issuer_id].get(), "* ###.00");
            field_text = rt(&field_text, 8);
            row_text = row_text + &field_text;
            field_text = sp(12) + &fmt(cast::<Ext, _>(g.hold_bond_face[(slot, owner_id)]), "* ###,");
            field_text = rt(&field_text, 12);
            row_text = row_text + &field_text;
            field_text = sp(12) + &fmt(position_value, "* ###,");
            field_text = rt(&field_text, 12);
            fr.set(fr.line_no, row_text.clone() + &field_text);
            if display_mode == 0 {
                col_text = sp(9) + &fmt(cast::<Ext, _>(g.bond_ytm), "* ###.00") + "%";
                col_text = rt(&col_text, 9);
                if g.credit_rating_num[issuer_id] < 2 {
                    let need = sf(0.0025f32) * val(rt(&g.bond_issue_info[issuer_id], 5).as_bytes())
                        * cast::<Ext, _>(g.bonds_issued[issuer_id]);
                    let defaulted = if g.cash[issuer_id] + g.t_bills[issuer_id] >= need {
                        !(2 < g.industry_num[issuer_id]
                            && g.bank_debt[issuer_id] <= Ext::from_f64(0.0)
                            && g.credit_rating_num[issuer_id] != 0)
                    } else {
                        true
                    };
                    if defaulted {
                        col_text = s(__content.string(8712));
                    }
                }
            } else {
                let cost_basis = g.bond_hold_cost[(slot, entity_id)];
                col_text = sp(12) + &fmt(cost_basis, "* ###,");
                col_text = rt(&col_text, 12);
                bond_basis_total = bond_basis_total + cost_basis;
            }
            fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
            col_text = sp(10) + &g.credit_rating_str + "   ";
            col_text = rt(&col_text, 13);
            if display_mode != 0 || 0 < instr(1, fr.get(fr.line_no).as_bytes(), __content.bytes(8713)) {
                col_text = rt(&col_text, 10);
            }
            fr.set(fr.line_no, fr.get(fr.line_no) + &col_text);
            if g.entity_pick_mode == "GetCorp" || g.entity_pick_mode == __content.string(6093) {
                fr.set(fr.line_no, fr.get(fr.line_no) + &sp(60) + "@C" + &link_id(issuer_id));
            }
            fr.line_no = fr.line_no + 1;
            fr.set(fr.line_no, bond_sep_line.clone());
        }
    }
    calc_entity_financials(g, ui, rng, owner_id);
    col_text = g.currency_symbol.clone();
    row_text = s(__content.string(8714)) + "(" + &col_text + &g.million_prefix_uc + "illion" + &g.currency_suffix
        + "):";
    row_text = row_text + &sp(35);
    row_text = mid(row_text.as_bytes(), 1, 56);
    fr.line_no = fr.line_no + 1;
    col_text = sp(16) + &fmt(fr.corp_bonds_value + fr.gov_bonds_value, "* ###,");
    col_text = rt(&col_text, 16);
    if Ext::from_f64(0.0) < bond_basis_total {
        field_text = field_text + &sp(12) + &fmt(bond_basis_total, "* ###,");
        col_text = col_text + &rt(&field_text, 12);
    }
    fr.set(fr.line_no, row_text + &col_text);
    fr.set(fr.line_no + 1, sp(61) + &string_fill(11, b"="));
    if Ext::from_f64(0.0) < bond_basis_total {
        fr.set(fr.line_no + 1, fr.get(fr.line_no + 1) + " " + &string_fill(11, b"="));
    }
    totals(g, ui, rng, &mut fr);
}

/// GOSUB ShowPortfolioListing_Totals (0x445f97): grand total line (listing kind 3 with bonds), `@END`,
/// copies `lines(1..90)` to `TextLines`, sets the title and shows the report.
fn totals(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, fr: &mut PlFrame) {
    let __content = crate::content::current();
    if (g.tmp_corp_bonds_value != Ext::from_f64(0.0) || g.tmp_gov_bonds_value != Ext::from_f64(0.0)) && 2 < fr.listing_kind {
        let field_text = fr.portfolio_kind.clone();
        fr.line_no = fr.line_no + 3;
        let total = g.options_net_value + fr.stock_value_total + fr.corp_bonds_value + fr.gov_bonds_value;
        let col_text = g.currency_symbol.clone() + &fmt_trim(total, "* ######,");
        fr.set(
            fr.line_no,
            s(__content.string(8715)) + &field_text + __content.string(8716) + &col_text + " " + &g.million_prefix_uc
                + "illion" + &g.currency_suffix,
        );
        fr.line_no = fr.line_no + 1;
        let mut underline_len = 43;
        if fr.portfolio_kind == "EQUITY" {
            underline_len = 44;
        }
        fr.set(fr.line_no, sp(underline_len) + &string_fill(col_text.len(), b"="));
    }
    if fr.has_links == 0 {
        g.entity_pick_mode = PbStr::new();
    }
    fr.set(fr.line_no + 2, s("@END"));
    for line_no in 1..=90 {
        g.text_lines[line_no] = fr.get(line_no);
    }
    fr.line_no = 91;
    let mut field_text = fr.portfolio_kind.clone();
    if fr.listing_kind == 2 {
        field_text = s("BOND");
    }
    if fr.listing_kind == 3 {
        field_text = field_text + " & BOND";
    }
    g.text_box_title = s("===  ") + &field_text + __content.string(8717)
        + &ucase(g.ent_name[fr.owner_id].as_bytes(), CaseMode::Ansi);
    let col_text = if fr.entity_id < 11 { s(" ===") } else { s(" (") + &g.ent_symbol[fr.owner_id] + ") ===" };
    g.text_box_title = g.text_box_title.clone() + &col_text;
    g.text_box_mode = s("127");
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}

/// Locals of ShowOptionPositions shared with its GOSUB BuildRows.
struct OpFrame {
    rows: Vec<PbStr>,
    row: i32,
    holder_id: i32,
    filter_corp: i32,
    list_mode: i32,
    opt_type: i32,
    side_filter: i32,
    found_any: i32,
    has_grant: i32,
    fmt_str: &'static str,
}

/// ERROR 151 in BuildRows: the ON ERROR handler shows ShowRuntimeError and leaves the routine.
struct RuntimeError;

/// Port of ShowOptionPositions (0x5f51fc).
/// Report of the long and short call and put positions of `entity_id` in `OptionRecs()` (bid prices,
/// strike, expiry, percentage, tax basis, current value). `list_mode` 1..4 lists only long calls / long
/// puts / short calls / short puts on `corp_filter` (> 10) and makes the lines pickable (`@C<record>`);
/// mode 0 lists everything (`@C<corp>` links). With no positions (mode 0, no filter) it offers the
/// active entity's / current player's positions (MSGBOX YES/NO) or shows a "Nothing to display" popup.
/// Returns `TargetCorp` after the report (cleared), i.e. the picked link's target.
///
/// Side effects: `SortOptionRecs` (twice), `HasOptionsOutstanding` of every listed underlying, the
/// option pricing's side effects. ERROR 151 (`SharesOutstanding = 0` of a listed underlying) ends the
/// routine like its ON ERROR handler (ShowRuntimeError) without a report.
pub fn show_option_positions(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    list_mode: i32,
    corp_filter: i32,
) -> i32 {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError
    sort_option_recs(g, ui);
    sort_option_recs(g, ui);
    let mut holder_id = entity_id;
    if g.entity_status[entity_id] == 2 {
        describe_defunct_entity(g, ui, holder_id);
        return 0;
    }
    let mut ent_name = PbStr::new();
    let mut whose_str = PbStr::new();
    let mut switched = 0;
    if find_option_positions(g, ui, holder_id, 0, 0, 0) == 0 && list_mode == 0 && corp_filter == 0 {
        ent_name = g.ent_name[holder_id].clone();
        if g.cur_player != holder_id && g.active_entity != holder_id {
            let active = g.active_entity;
            if find_option_positions(g, ui, active, 0, 0, 0) == 0 {
                let cur = g.cur_player;
                if find_option_positions(g, ui, cur, 0, 0, 0) == 0 {
                    holder_id = 0;
                } else {
                    whose_str = s("your");
                }
            } else {
                whose_str = g.active_entity_name.clone() + "'s";
                if g.cur_player == g.active_entity {
                    whose_str = s("your");
                }
            }
            if 0 < holder_id {
                let msg_text = g.ent_name[holder_id].clone() + __content.string(8718) + "\r" + "\r"
                    + __content.string(8719) + &whose_str + __content.string(8720);
                let answer = ui.msgbox(&msg_text, 8228, &s(__content.string(8721)));
                if answer != 6 {
                    return 0;
                }
                holder_id = g.active_entity;
                if whose_str == "your" {
                    holder_id = g.cur_player;
                }
                switched = 1;
            }
        }
        if switched == 0 {
            let mut msg = s(__content.string(8541)) + "\r\n" + "\r\n" + &ent_name
                + __content.string(8722);
            show_quick_message(g, ui, &mut msg);
            return 0;
        }
    }
    let _ = (&ent_name, &whose_str);
    let mut _price_side = s("BID");
    if list_mode == 3 || list_mode == 4 {
        _price_side = s("ASK");
    }
    let mut fr = OpFrame {
        rows: vec![PbStr::new(); 1501],
        row: 0,
        holder_id,
        filter_corp: corp_filter,
        list_mode,
        opt_type: 0,
        side_filter: 0,
        found_any: 0,
        has_grant: 0,
        fmt_str: "* #############0.00",
    };
    let separator = string_fill(83, b"-");
    let hdr1 = s(__content.string(8723));
    let hdr2 = s(__content.string(8724));
    let mut hdr3 = s(__content.string(8725))
        + &g.million_prefix_uc + "IL.)";
    let hdr_underline = s("-------------------- -------- -------- ------- ---- ------- ---------- ----------");
    let currency_label = currency_label(g);
    fr.rows[1] = s(__content.string(8726));
    if 0 < list_mode && 10 < fr.filter_corp {
        let name = g.ent_name[fr.filter_corp].clone();
        match list_mode {
            1 => fr.rows[1] = s(__content.string(8727)) + &name + ":",
            2 => fr.rows[1] = s(__content.string(8728)) + &name + ":",
            3 => fr.rows[1] = s(__content.string(8729)) + &name + ":",
            4 => fr.rows[1] = s(__content.string(8730)) + &name + ":",
            _ => {}
        }
    }
    fr.rows[2] = s(__content.string(8731)) + &g.million_prefix_lc + __content.string(8089) + &currency_label
        + __content.string(8732);
    fr.rows[3] = separator.clone();
    fr.row = 4;
    let set = |fr: &mut OpFrame, i: i32, x: PbStr| fr.rows[cast::<usize, _>(i)] = x;
    let res = find_option_positions(g, ui, holder_id, 0, 1, 0);
    if list_mode < 2 && 0 < res {
        let r = fr.row;
        set(&mut fr, r + 1, s(__content.string(8733)));
        set(&mut fr, r + 2, s("          --------------------------------------------"));
        set(&mut fr, r + 3, PbStr::new());
        set(&mut fr, r + 4, hdr1.clone());
        set(&mut fr, r + 5, hdr2.clone());
        set(&mut fr, r + 6, hdr3.clone());
        set(&mut fr, r + 7, hdr_underline.clone());
        fr.row = fr.row + 8;
        fr.found_any = 0;
        fr.opt_type = 1;
        fr.side_filter = 1;
        if build_rows(g, ui, rng, &mut fr).is_err() {
            return 0;
        }
        let r = fr.row;
        set(&mut fr, r, hdr_underline.clone());
        if fr.has_grant == 1 {
            fr.row = fr.row + 1;
            let r = fr.row;
            set(&mut fr, r, s(__content.string(8734)));
            let t = fr.rows[cast::<usize, _>(r)].clone() + __content.string(8735);
            set(&mut fr, r, t);
        }
    }
    if list_mode == 0 || list_mode == 2 {
        let res = find_option_positions(g, ui, holder_id, 0, 2, 0);
        if 0 < res {
            fr.row = fr.row + 1;
            let r = fr.row;
            set(&mut fr, r + 1, s(__content.string(8736)));
            set(&mut fr, r + 2, s("          --------------------------------------------"));
            set(&mut fr, r + 3, PbStr::new());
            set(&mut fr, r + 4, hdr1.clone());
            set(&mut fr, r + 5, hdr2.clone());
            let msg_text = replace(hdr3.as_bytes(), b"OF CALL", b" OF PUT");
            set(&mut fr, r + 6, msg_text);
            set(&mut fr, r + 7, hdr_underline.clone());
            fr.row = fr.row + 8;
            fr.found_any = 0;
            fr.opt_type = 2;
            fr.side_filter = 1;
            if build_rows(g, ui, rng, &mut fr).is_err() {
                return 0;
            }
            let r = fr.row;
            set(&mut fr, r, hdr_underline.clone());
        }
    }
    hdr3 = replace(hdr3.as_bytes(), b"HELD", b"SOLD");
    if list_mode == 0 || list_mode == 3 {
        let res = find_option_positions(g, ui, holder_id, 0, 1, 0);
        if res == -11 || res == 12 {
            fr.row = fr.row + 1;
            let r = fr.row;
            set(&mut fr, r + 1, s(__content.string(8737)));
            set(&mut fr, r + 2, s("          --------------------------------------------"));
            set(&mut fr, r + 3, PbStr::new());
            set(&mut fr, r + 4, hdr1.clone());
            set(&mut fr, r + 5, hdr2.clone());
            set(&mut fr, r + 6, hdr3.clone());
            set(&mut fr, r + 7, hdr_underline.clone());
            fr.row = fr.row + 8;
            fr.found_any = 0;
            fr.opt_type = 1;
            fr.side_filter = 2;
            if build_rows(g, ui, rng, &mut fr).is_err() {
                return 0;
            }
            let r = fr.row;
            set(&mut fr, r, hdr_underline.clone());
        }
    }
    if list_mode == 0 || list_mode == 4 {
        let res = find_option_positions(g, ui, holder_id, 0, 2, 0);
        if res == -21 || res == 22 {
            fr.row = fr.row + 1;
            let r = fr.row;
            set(&mut fr, r + 1, s(__content.string(8738)));
            set(&mut fr, r + 2, s("          --------------------------------------------"));
            set(&mut fr, r + 3, PbStr::new());
            set(&mut fr, r + 4, hdr1.clone());
            set(&mut fr, r + 5, hdr2.clone());
            let msg_text = replace(hdr3.as_bytes(), b"OF CALL", b" OF PUT");
            set(&mut fr, r + 6, msg_text);
            set(&mut fr, r + 7, hdr_underline.clone());
            fr.row = fr.row + 8;
            fr.found_any = 0;
            fr.opt_type = 2;
            fr.side_filter = 2;
            if build_rows(g, ui, rng, &mut fr).is_err() {
                return 0;
            }
            let r = fr.row;
            set(&mut fr, r, hdr_underline.clone());
        }
    }
    let res = find_option_positions(g, ui, holder_id, 0, 0, 0);
    if res == -41 || res == 42 {
        let r = fr.row;
        set(&mut fr, r + 2, s(__content.string(8739)));
        set(&mut fr, r + 3, s(__content.string(8740)));
        set(&mut fr, r + 4, s(__content.string(8741)));
    }
    let r = fr.row;
    set(&mut fr, r + 10, separator.clone());
    set(&mut fr, r + 12, s("@END"));
    let line_count = fr.row + 12;
    fr.row = line_count;
    for rec_idx in 1..=line_count {
        g.text_lines[rec_idx] = fr.rows[cast::<usize, _>(rec_idx)].clone();
    }
    let mut owner_label = s(" My");
    if 10 < holder_id {
        owner_label = s(" ") + &g.ent_symbol[holder_id] + "'s";
    }
    let filter_name = g.ent_name[fr.filter_corp].clone();
    let msg_text = match list_mode {
        1 => owner_label + __content.string(8742) + &filter_name,
        2 => owner_label + __content.string(8743) + &filter_name,
        3 => owner_label + __content.string(8744) + &filter_name,
        4 => owner_label + __content.string(8745) + &filter_name,
        _ => {
            let t = s(__content.string(8746)) + &ucase(g.ent_name[holder_id].as_bytes(), CaseMode::Ansi) + " AS OF ";
            let progress = g.sim_co_count;
            let month = to_long(ceil(calc_cur_month_of_year(g, progress)));
            t + &str_trim(cast::<Ext, _>(month)) + "/" + &str_trim(cast::<Ext, _>(g.cur_year))
        }
    };
    g.text_box_title = msg_text;
    if list_mode < 1 {
        if list_mode == 0 {
            g.entity_pick_mode = s(__content.string(6093));
        }
    } else {
        g.entity_pick_mode = s("GetCorp");
        g.list_item_noun = s("option");
    }
    g.text_box_mode = s("80");
    g.target_corp = 0;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    if g.target_corp == 0 {
        return 0;
    }
    let result = g.target_corp;
    g.target_corp = 0;
    result
}

/// `VAL(PARSE$(OptionRecs(i), ANY "|", n))`.
fn opt_field(g: &Globals, i: i32, n: i32) -> Ext {
    val(parse_any(g.option_recs[i].as_bytes(), b"|", n).as_bytes())
}

/// `RTRIM$(LTRIM$(PARSE$(OptionRecs(i), ANY "|", n)))`.
fn opt_text(g: &Globals, i: i32, n: i32) -> PbStr {
    trim(&parse_any(g.option_recs[i].as_bytes(), b"|", n))
}

/// GOSUB ShowOptionPositions_BuildRows (0x5f69db): one report row per record of `OptionRecs(1..1000)`
/// held by the holder (on the filter corp, if any) of the section's type and side.
fn build_rows(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, fr: &mut OpFrame) -> Result<(), RuntimeError> {
    for rec_idx in 1..=1000 {
        if g.option_recs[rec_idx].is_empty() {
            continue;
        }
        if opt_field(g, rec_idx, 3) != cast::<Ext, _>(fr.holder_id) {
            continue;
        }
        if 10 < fr.filter_corp && opt_field(g, rec_idx, 1) != cast::<Ext, _>(fr.filter_corp) {
            continue;
        }
        if opt_field(g, rec_idx, 2) != cast::<Ext, _>(fr.opt_type) {
            continue;
        }
        if fr.side_filter == 1 && opt_field(g, rec_idx, 4) < Ext::from_f64(0.0) {
            continue;
        }
        if fr.side_filter == 2 && opt_field(g, rec_idx, 4) > Ext::from_f64(0.0) {
            continue;
        }
        fr.found_any = 1;
        let corp_id = to_long(opt_field(g, rec_idx, 1));
        g.has_options_outstanding[corp_id] = 1;
        let mut line_text = mid(g.ent_name[corp_id].as_bytes(), 1, 20) + &sp(21);
        line_text = mid(line_text.as_bytes(), 1, 21);
        if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
            return Err(RuntimeError); // ERROR 151
        }
        let mut msg_text = fmt(g.market_cap[corp_id] / g.shares_outstanding[corp_id], fr.fmt_str);
        line_text = line_text + &rt(&msg_text, 8) + " ";
        let strike_per_share = (opt_field(g, rec_idx, 5) / g.shares_outstanding[corp_id]).abs();
        msg_text = fmt(strike_per_share, fr.fmt_str);
        line_text = line_text + &rt(&msg_text, 8) + " ";
        let mut whose_str = s("0") + &opt_text(g, rec_idx, 6);
        whose_str = rt(&whose_str, 2) + "/";
        let ent_name = opt_text(g, rec_idx, 7);
        line_text = line_text + &whose_str + &ent_name + " ";
        let qty = to_long(opt_field(g, rec_idx, 4));
        msg_text = s("   ") + &str_trim(cast::<Ext, _>(qty)) + "%";
        line_text = line_text + &rt(&msg_text, 4) + " ";
        let opt_price: Ext = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, rec_idx);
        let mut price_per_share = Cux::default();
        price_per_share.set(opt_price);
        if g.shares_outstanding[corp_id] == Ext::from_f64(0.0) {
            g.shares_outstanding[corp_id] = Ext::from_f64(2.0);
            check_stock_split(g, corp_id);
            // errInfo = "VarType= PMult()"
            return Err(RuntimeError); // ERROR 151
        }
        price_per_share.set(price_per_share.get() / g.shares_outstanding[corp_id]);
        let bid_per_share = price_per_share;
        msg_text = fmt(bid_per_share.get().abs(), fr.fmt_str);
        line_text = line_text + &rt(&msg_text, 7) + " ";
        let mut tax_basis = Cux::default();
        tax_basis.set(opt_field(g, rec_idx, 8));
        if qty < 0 {
            tax_basis.set(-tax_basis.get()); // * -1
        }
        msg_text = fmt(tax_basis.get(), "* ################");
        msg_text = rt(&msg_text, 10);
        if fr.opt_type == 1 && fr.side_filter == 1 && opt_text(g, rec_idx, 8) == "GRANT" {
            msg_text = s("  *** 0.00");
            fr.has_grant = 1;
        }
        line_text = line_text + &msg_text + " ";
        let mut cur_value = Cux::default();
        cur_value.set(g.shares_outstanding[corp_id] * bid_per_share.get() * cast::<Ext, _>(qty));
        msg_text = fmt(cur_value.get(), "* ################");
        msg_text = rt(&msg_text, 10);
        line_text = line_text + &msg_text;
        let link = if fr.list_mode < 1 { link_id(corp_id) } else { link_id(rec_idx) };
        msg_text = s("@C") + &link;
        line_text = line_text + &sp(60) + &msg_text;
        fr.rows[cast::<usize, _>(fr.row)] = line_text;
        fr.row = fr.row + 1;
    }
    Ok(())
}
