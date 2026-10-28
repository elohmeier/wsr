//! `ShowPersonalFinancialStatement` (0x43987e) with its GOSUB FmtNum (0x43ffcf). Its callees
//! `GetEntitySoleCommodity` (0x6358eb) and `CalcPlayerIncomeTax` (0x508797) are the ports in
//! `logic::reports_corp` / `logic::cashflow`.
use crate::pb::real::{Ext, cast};


use super::{fmt, fmt_trim, link_id, s, str15, str_trim};
use crate::logic::cashflow::calc_player_income_tax;
use crate::logic::control::{find_option_positions, update_corp_control};
use crate::logic::earnings::{balance_cash_t_bills, calc_ceo_pay};
use crate::logic::finance::{calc_capital_taxes, calc_entity_financials};
use crate::logic::futures::calc_phys_commod_storage_cost;
use crate::logic::holdings::{find_bond_holding_slot, find_stock_holding, pack_holding_arrays_with_news as pack_holding_arrays};
use crate::logic::options::{calc_option_price, calc_spread_factor};
use crate::logic::reports_corp::get_entity_sole_commodity;
use crate::logic::report::{describe_defunct_entity, show_text_report};
use crate::logic::swaps::{process_swap_payments, sum_swap_notional};
use crate::pb::numeric::val;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, ltrim, mid, parse_any, remove, remove_any, right, rtrim, string_fill, ucase, CaseMode};
use crate::pb::numeric::{to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// SINGLE pool constant 0.01 (`fmul dword [0x75e404]`).
const C001: f64 = 0.01f32 as f64;
/// SINGLE pool constant 0.04 (`fld dword [0x75e500]`).
const C004: f64 = 0.04f32 as f64;

/// GOSUB ShowPersonalFinancialStatement_FmtNum (0x43ffcf):
/// `fmtText = RIGHT$(STRING$(19, " ") + RTRIM$(LTRIM$(FORMAT$(fmtValue, numFmt))), 19)`.
fn fmt_num(x: Ext, num_fmt: &str) -> PbStr {
    let t = string_fill(19, b" ") + &fmt_trim(x, num_fmt);
    right(t.as_bytes(), 19)
}

/// `VAL(REMOVE$(FORMAT$(x, numFmt), ANY ","))`: rounds to what the statement shows.
fn reval(x: Ext, num_fmt: &str) -> Ext {
    let t = remove_any(fmt(x, num_fmt).as_bytes(), b",");
    val(t.as_bytes())
}

/// `RIGHT$(STRING$(12, " ") + RTRIM$(LTRIM$(FORMAT$(x, numFmt))), n)`.
fn col(x: Ext, num_fmt: &str, n: i32) -> PbStr {
    let t = string_fill(12, b" ") + &fmt_trim(x, num_fmt);
    right(t.as_bytes(), n)
}

/// The local `DIM lines(1 TO 1500)`.
struct Lines(Vec<PbStr>);

impl Lines {
    fn set(&mut self, i: i32, x: PbStr) {
        // Indices stay far below 1500 (error 9 / ON ERROR otherwise).
        self.0[cast::<usize, _>(i)] = x;
    }
    fn get(&self, i: i32) -> PbStr {
        self.0[cast::<usize, _>(i)].clone()
    }
}

/// Port of ShowPersonalFinancialStatement (0x43987e) with its GOSUB FmtNum (0x43ffcf).
/// The current player's " [runtime resource wording] " (balance sheet, credit info, employment, taxes,
/// projected annual portfolio cash flow, unrealized gain/loss on the positions in `LastSelEntity`, recent
/// headlines naming the player) or, with `view_mode = 1`, only the " [runtime resource wording] ".
///
/// State effects: [runtime resource wording] within the unused credit line is covered from the lending
/// bank, `PlayerCeoOf`/`PlayerCeoSinceYear` are maintained, `TmpCapitalTax`/`CorpSharesTaxDue` are
/// (re)computed, `LenderId`, `EntityPickMode = "ChangeCorp"` (not in mode 1), and the callees'
/// (CalcEntityFinancials, BalanceCashTBills, PackHoldingArrays, CalcCeoPay, UpdateCorpControl, …).
///
/// ON ERROR -> ShowRuntimeError (error 9 paths, e.g. `LenderId = 0`, are not ported).
pub fn show_personal_financial_statement(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, view_mode: i32) {
    let __content = crate::content::current();
    let mut lines = Lines(vec![PbStr::new(); 1501]);
    let dash_line = string_fill(70, b"-");
    // (UI) DIALOG END DlgWhoOwnsWhat
    let player_id = g.cur_player;
    if g.entity_status[g.cur_player] < 1 {
        return;
    }
    if g.entity_status[player_id] == 2 {
        describe_defunct_entity(g, ui, player_id);
        return;
    }
    calc_entity_financials(g, ui, rng, player_id);
    balance_cash_t_bills(g, player_id);
    if g.cash[player_id] < Ext::from_f64(0.0) && g.cash[player_id].abs() < g.unused_credit_line {
        let draw_amt = g.cash[player_id].abs();
        g.cash[player_id] = Ext::from_f64(0.0);
        let l = g.lender_id;
        g.cash[l] = g.cash[l] - draw_amt;
        g.bank_debt[player_id] = draw_amt + g.bank_debt[player_id];
    }
    calc_entity_financials(g, ui, rng, player_id);
    // UpdateMarketDisplay: the main window refresh, which also runs a market tick (random stock
    // revaluations, stock index, ticker, commodity prices). TIME$ seconds: see calc_stock_price.
    let seconds = ui.local_time().time_secs();
    crate::logic::market::update_market_display(g, ui, rng, seconds);
    calc_entity_financials(g, ui, rng, player_id);
    let player_name = g.ent_name[player_id].clone();
    pack_holding_arrays(g, ui);
    let mut num_fmt = "* #################,";
    if g.total_assets < Ext::from_f64(100000.0) {
        num_fmt = "* ##############.00";
    }
    let nf = num_fmt;
    let mut salary_bonus = Ext::from_f64(0.0);
    let mut val_text;
    let mut aux_text;
    let mut suffix_text;
    let mut aux_idx: i32;
    if view_mode != 1 {
        lines.set(2, s(__content.string(8634)) + &g.player_name[player_id] + ":");
        let work = lines.get(2);
        lines.set(3, s(" ") + &string_fill(work.len() - 1, b"-"));
        let mut work_text = g.currency_name.clone();
        if g.currency_code == "EUR" {
            work_text = s("Euro");
        }
        lines.set(4, s("(") + &work_text + __content.string(8635) + &g.million_prefix_lc + __content.string(8636));
        lines.set(6, s("ASSETS:"));
        lines.set(7, s(__content.string(8092)) + &fmt_num(g.cash[player_id], nf));
        lines.set(8, s(__content.string(8093)) + &fmt_num(g.t_bills[player_id], nf));
        lines.set(9, s(__content.string(8637)) + &fmt_num(g.tmp_subsid_stock_value, nf));
        aux_idx = cast::<i32, _>(g.hold_stock_id[(1, player_id)]);
        if g.hold_stock_id[(0, player_id)] == 1 && 10 < aux_idx && g.ent_name[aux_idx].len() < 24 {
            lines.set(9, s("  ") + &g.ent_name[aux_idx] + " Stock" + &string_fill(32, b" "));
            let ft = fmt_num(g.tmp_subsid_stock_value, nf);
            lines.set(9, mid(lines.get(9).as_bytes(), 1, 31) + &ft);
            let link_code = link_id(aux_idx);
            lines.set(9, lines.get(9) + &string_fill(85, b" ") + "@C" + &link_code);
        }
        lines.set(10, s(__content.string(8638)) + &fmt_num(g.player_short_margin_bal[player_id], nf));
        lines.set(11, s(__content.string(8097)) + &fmt_num(g.options_net_value, nf));
        lines.set(
            12,
            s(__content.string(8110)) + &fmt_num(g.futures_net_value + g.phys_commod_value, nf),
        );
        if g.futures_net_value == Ext::from_f64(0.0) || g.phys_commod_value != Ext::from_f64(0.0) {
            if g.futures_net_value == Ext::from_f64(0.0) && Ext::from_f64(0.0) < g.phys_commod_value {
                let ft = fmt_num(g.phys_commod_value, nf);
                lines.set(12, s(__content.string(8639)) + &ft);
                let commod_name = get_entity_sole_commodity(g, player_id, 2);
                if commod_name == "ETHEREUM" {
                    lines.set(12, s(__content.string(8640)) + &ft);
                } else if commod_name == "BITCOIN" {
                    lines.set(12, s(__content.string(8641)) + &ft);
                }
            }
        } else {
            lines.set(12, s(__content.string(8113)) + &fmt_num(g.futures_net_value, nf));
        }
        lines.set(13, s(__content.string(8642)) + &fmt_num(g.commod_margin_bal[player_id], nf));
        lines.set(14, s(__content.string(8643)) + &fmt_num(g.tmp_gov_bonds_value, nf));
        lines.set(15, s(__content.string(8644)) + &fmt_num(g.tmp_corp_bonds_value, nf));
        aux_idx = g.hold_bond_id[(1, player_id)];
        if g.hold_bond_id[(0, player_id)] == 1 && g.ent_name[aux_idx].len() < 24 {
            lines.set(15, s("  ") + &g.ent_name[aux_idx] + " Bonds" + &string_fill(32, b" "));
            let ft = fmt_num(g.tmp_corp_bonds_value, nf);
            lines.set(15, mid(lines.get(15).as_bytes(), 1, 31) + &ft);
            let link_code = link_id(aux_idx);
            lines.set(15, lines.get(15) + &string_fill(85, b" ") + "@C" + &link_code);
        }
        val_text = s(__content.string(8645));
        let mut pos_flag = 0;
        aux_idx = 0;
        for loop_idx in 11..=1600 {
            if g.entity_status[loop_idx] == 1 && g.player_advances[(player_id, loop_idx)] > Ext::from_f64(0.0) {
                pos_flag = pos_flag + 1;
                aux_idx = loop_idx;
            }
        }
        let mut link_code = PbStr::new();
        if pos_flag == 1 {
            val_text = g.ent_name[aux_idx].clone();
            if 15 < val_text.len() {
                val_text = g.ent_symbol[aux_idx].clone();
            }
            val_text = val_text + &string_fill(17, b" ");
            val_text = mid(val_text.as_bytes(), 1, 17);
            link_code = string_fill(85, b" ") + "@C" + &link_id(aux_idx);
        }
        lines.set(16, s(__content.string(8646)) + &val_text + &fmt_num(g.tmp_player_advances, nf) + &link_code);
        lines.set(17, s("                               -------------------"));
        lines.set(18, s(__content.string(8118)) + &fmt_num(g.total_assets, nf));
        lines.set(19, s("                               -------------------"));
        lines.set(21, s(__content.string(8647)));
        lines.set(22, s(__content.string(8129)) + &fmt_num(g.bank_debt[player_id], nf));
        let total_liabs = g.tmp_total_debt;
        lines.set(23, s("                               -------------------"));
        lines.set(24, s(__content.string(8136)) + &fmt_num(total_liabs, nf));
        lines.set(25, s("                               -------------------"));
        lines.set(26, s(__content.string(8648)) + &fmt_num(g.net_worth, nf));
        lines.set(27, s("                               ==================="));
        lines.set(30, s(__content.string(8649)));
        lines.set(31, dash_line.clone());
        lines.set(32, s(__content.string(8650)) + &g.credit_rating_str);
        let mut work_text = g.currency_symbol.clone()
            + &fmt_trim(g.unused_credit_line, "* ###########,.00")
            + " "
            + &g.million_prefix_uc
            + "illion";
        if g.unused_credit_line <= Ext::from_f64(0.0) {
            work_text = s("ZERO");
            if g.credit_frozen[g.lender_id] == 1 && player_id != g.corp_controller[g.lender_id] {
                work_text = s("FROZEN");
            }
        }
        lines.set(33, s(__content.string(8651)) + &work_text);
        lines.set(34, s(__content.string(8652)) + &fmt(cast::<Ext, _>(g.borrow_rate), "###.00") + "%");
        let bank = cast::<i32, _>(g.lending_bank[player_id]);
        lines.set(35, s(__content.string(8653)) + &g.ent_name[bank] + " (" + &g.ent_symbol[bank] + ")");
        g.lender_id = cast::<i32, _>(g.lending_bank[player_id]);
        g.entity_pick_mode = s(__content.string(6093));
        if 10 < g.lender_id {
            let v = link_id(g.lender_id);
            lines.set(35, lines.get(35) + &string_fill(45, b" ") + "@C" + &v);
        }
        val_text = g.currency_symbol.clone()
            + &fmt_trim(g.player_cap_gains[player_id].abs(), "* ###########,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        if g.player_cap_gains[player_id].abs() < C001 {
            val_text = s("NONE");
        }
        work_text = s("Gains");
        if g.player_cap_gains[player_id] < Ext::from_f64(0.0) {
            work_text = s("Losses");
        }
        lines.set(36, s(__content.string(8654)) + &work_text + ":  " + &val_text);
        let tax_balance = calc_player_income_tax(g, player_id);
        let mut taxable_income = g.tmp_ext1;
        g.tmp_ext1 = Ext::from_f64(0.0);
        if Ext::from_f64(0.0) < g.player_cap_gains[player_id] {
            taxable_income = taxable_income - g.player_cap_gains[player_id];
        }
        val_text = g.currency_symbol.clone()
            + &fmt_trim(taxable_income.abs(), "* ###########,")
            + " "
            + &g.million_prefix_lc
            + "illion";
        if val_text.len() + g.currency_suffix.len() < 25 {
            val_text = val_text + &g.currency_suffix;
        }
        work_text = s("Income");
        if taxable_income < Ext::from_f64(0.0) {
            work_text = s("Loss");
        }
        if taxable_income == Ext::from_f64(0.0) {
            val_text = s("NONE");
        }
        lines.set(37, s(__content.string(8655)) + &work_text + ": " + &val_text);
        val_text = g.currency_symbol.clone()
            + &fmt_trim(g.player_prepaid_tax[player_id], "* ####,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        if g.player_prepaid_tax[player_id] <= Ext::from_f64(0.0) {
            val_text = s("NONE");
        }
        lines.set(38, s(__content.string(8656)) + &val_text);
        val_text = g.currency_symbol.clone()
            + &fmt_trim(tax_balance, "* ###########,")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        if g.currency_suffix.len() + val_text.len() < 25 {
            val_text = val_text + &g.currency_suffix; // (suffix appended twice: quirk)
        }
        if tax_balance <= Ext::from_f64(0.0) {
            val_text = s("NONE");
        }
        lines.set(
            39,
            s(__content.string(8657)) + &str_trim(cast::<Ext, _>(1 + g.cur_year)) + "): " + &val_text,
        );
    }
    let mut work_text = PbStr::new();
    let ceo_of = g.player_ceo_of[player_id];
    if 0 < ceo_of && g.entity_status[ceo_of] != 1 {
        g.player_ceo_of[player_id] = 0;
        g.player_ceo_since_year[player_id] = 0;
    }
    if 10 < g.player_ceo_of[player_id] {
        let ceo = g.player_ceo_of[player_id];
        val_text = link_id(ceo);
        aux_text = string_fill(45, b" ") + "@C" + &val_text;
        if g.player_ceo_since_year[player_id] == 0 {
            g.player_ceo_since_year[player_id] = g.cur_year;
        }
        work_text = s(__content.string(8658))
            + &g.ent_name[ceo]
            + " (since"
            + &str15(cast::<Ext, _>(g.player_ceo_since_year[player_id]))
            + ")"
            + &aux_text;
        if 12 < g.ent_name[ceo].len() {
            work_text = remove(work_text.as_bytes(), __content.bytes(8659));
        }
        let ceo_salary = calc_ceo_pay(g, ui, rng, g.player_ceo_of[player_id], 0);
        lines.set(
            41,
            s(__content.string(8660))
                + &g.currency_symbol
                + &fmt_trim(ceo_salary, "* #########.00")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix,
        );
        update_corp_control(g, ui, rng, g.player_ceo_of[player_id]);
        val_text = s(__content.string(8661)) + &g.currency_symbol;
        aux_text = fmt_trim(g.ceo_bonus_multiple * ceo_salary, "* #########.00");
        suffix_text = s(")");
        salary_bonus = g.ceo_bonus_multiple * ceo_salary + ceo_salary;
        if 0 < g.public_pct && player_id == g.corp_controller[g.player_ceo_of[player_id]] {
            suffix_text = s(__content.string(8209));
        }
        val_text = val_text + &aux_text + " " + &g.million_prefix_lc + "illion" + &suffix_text;
        lines.set(42, val_text.clone());
        if val(aux_text.as_bytes()) == Ext::from_f64(0.0) && 0 < instr(1, val_text.as_bytes(), b"options") {
            lines.set(42, s(__content.string(8662)));
        }
    }
    let mut line_no: i32 = 42;
    if work_text.is_empty() {
        work_text = s(__content.string(8663));
        line_no = line_no - 1;
    } else {
        line_no = line_no + 1;
    }
    lines.set(40, s(__content.string(8664)) + &work_text);
    calc_entity_financials(g, ui, rng, player_id);
    let mut cap_taxes = calc_capital_taxes(g, player_id);
    if 0 < g.wealth_tax_rate && g.tmp_capital_tax == Ext::from_f64(0.0) && g.cur_quarter < 4 {
        let tmp_amt = g.player_prepaid_tax[player_id] + g.net_worth;
        g.tmp_capital_tax = (cast::<Ext, _>(g.wealth_tax_rate) * C001) * (tmp_amt - g.exchange_rate * Ext::from_f64(1000.0));
        if g.tmp_capital_tax <= g.exchange_rate * Ext::from_f64(1.0) {
            g.tmp_capital_tax = Ext::from_f64(0.0);
        }
        cap_taxes = g.tmp_capital_tax + cap_taxes;
    }
    if Ext::from_f64(1.0) < cap_taxes {
        if Ext::from_f64(0.0) < g.corp_shares_tax_due {
            val_text = s(__content.string(8483));
            aux_text = g.currency_symbol.clone() + &fmt_trim(g.corp_shares_tax_due, nf);
            lines.set(
                line_no,
                s("       ")
                    + &val_text
                    + __content.string(8665)
                    + &aux_text
                    + " "
                    + &g.million_prefix_uc
                    + "."
                    + &g.currency_suffix,
            );
            line_no = line_no + 1;
        }
        if Ext::from_f64(0.0) < g.tmp_capital_tax {
            val_text = s(__content.string(8666));
            aux_text = g.currency_symbol.clone() + &fmt_trim(g.tmp_capital_tax, nf);
            lines.set(
                line_no,
                s("       ")
                    + &val_text
                    + __content.string(8667)
                    + &aux_text
                    + " "
                    + &g.million_prefix_uc
                    + "."
                    + &g.currency_suffix,
            );
            line_no = line_no + 1;
        }
    }
    let cs = g.currency_symbol.clone();
    lines.set(
        line_no,
        s(__content.string(8668))
            + &rtrim(ltrim(cs.as_bytes()).as_bytes())
            + &fmt_trim(g.exchange_rate * Ext::from_f64(1.0), nf)
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix,
    );
    line_no = line_no + 1;
    let swap_notional = to_quad(sum_swap_notional(g, player_id, 0));
    if 0 < swap_notional {
        lines.set(
            line_no,
            s(__content.string(8669))
                + &rtrim(ltrim(cs.as_bytes()).as_bytes())
                + &fmt_trim(cast::<Ext, _>(swap_notional), "* ####,")
                + " "
                + &g.million_prefix_lc
                + "illion"
                + &g.currency_suffix,
        );
        line_no = line_no + 1;
        let swap_pl = to_quad(process_swap_payments(g, ui, rng, player_id, &s("R")));
        if 0 < swap_pl.abs() {
            val_text = s("Profits");
            if swap_pl < 0 {
                val_text = s("Loss");
            }
            lines.set(
                line_no,
                s(__content.string(8670))
                    + &val_text
                    + __content.string(8671)
                    + &rtrim(ltrim(cs.as_bytes()).as_bytes())
                    + &fmt_trim((cast::<Ext, _>(swap_pl)).abs(), "* ####,")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ".",
            );
            line_no = line_no + 1;
        }
    }
    lines.set(line_no, dash_line.clone());
    line_no = line_no + 2;
    if view_mode == 1 {
        for l in lines.0.iter_mut() {
            *l = PbStr::new();
        }
        line_no = 0;
    }
    let show_cash_flow = Ext::from_f64(0.0) < g.tmp_corp_bonds_value
        || Ext::from_f64(0.0) < g.tmp_gov_bonds_value
        || Ext::from_f64(10.0) < g.bank_debt[player_id]
        || Ext::from_f64(10.0) < g.cash[player_id] + g.t_bills[player_id]
        || Ext::from_f64(10.0) < g.tmp_subsid_stock_value
        || Ext::from_f64(10.0) < g.phys_commod_value;
    if show_cash_flow {
        line_no = cash_flow(g, &mut lines, line_no, nf, salary_bonus, swap_notional, &dash_line);
    } else {
        line_no = line_no + 2;
        lines.set(line_no, s(__content.string(2490)));
    }
    line_no = line_no + 2;
    if view_mode != 1 {
        if 10 < g.last_sel_entity {
            line_no = unrealized_gains(g, ui, rng, &mut lines, line_no, nf);
        }
        let aux_idx = line_no;
        line_no = line_no + 1;
        let mut found_news = 0;
        let up = ucase(player_name.as_bytes(), CaseMode::Ansi);
        let aux_text = s(" ") + &up + " ";
        let suffix_text = s(" ") + &up + ".";
        for loop_idx in 1..=60 {
            let val_text = s(" - ") + &g.news_headline[loop_idx] + " ";
            let num_text = ucase(val_text.as_bytes(), CaseMode::Ansi);
            if 0 < instr(1, num_text.as_bytes(), aux_text.as_bytes())
                || 0 < instr(1, num_text.as_bytes(), suffix_text.as_bytes())
            {
                found_news = 1;
                line_no = line_no + 1;
                lines.set(line_no, rtrim(val_text.as_bytes()));
            }
        }
        if found_news == 1 {
            lines.set(aux_idx, s(__content.string(8337)) + &up + ":");
            let n = lines.get(aux_idx).len() - 3;
            lines.set(aux_idx + 1, s("   ") + &string_fill(n, b"-"));
        }
    }
    // LAB_0043fe23
    lines.set(line_no + 4, s("@END"));
    for k in 1..=490 {
        g.text_lines[k] = lines.get(k);
        if lines.get(k) == "@END" {
            break;
        }
    }
    g.text_box_title =
        s(__content.string(8672)) + &g.ent_name[player_id] + " ===";
    if view_mode == 1 {
        g.text_box_title =
            s(__content.string(8673)) + &g.ent_name[player_id] + " ===";
    }
    g.text_has_links = 1;
    g.text_box_mode = s("50");
    show_text_report(g, ui, rng);
}

/// The " [runtime resource wording] " block (LAB_0043c286); returns the new `lineNo`.
#[allow(clippy::too_many_arguments)]
fn cash_flow(
    g: &mut Globals,
    lines: &mut Lines,
    mut line_no: i32,
    nf: &str,
    mut salary_bonus: Ext,
    swap_notional: i64,
    dash_line: &PbStr,
) -> i32 {
    let __content = crate::content::current();
    let player_id = g.cur_player;
    let mut tbill_interest = Ext::from_f64(0.0);
    let mut adv_interest = Ext::from_f64(0.0);
    let mut gov_bond_int = Ext::from_f64(0.0);
    let mut corp_bond_int = Ext::from_f64(0.0);
    let mut div_income = Ext::from_f64(0.0);
    let mut amort_total = Ext::from_f64(0.0);
    let mut amort_year;
    let mut maturing_face = Ext::from_f64(0.0);
    let mut skip_amort = 0;
    let mut val_text;
    line_no = line_no + 2;
    lines.set(
        line_no,
        s(__content.string(2457)) + &g.million_prefix_uc + __content.string(2458),
    );
    lines.set(line_no + 1, dash_line.clone());
    if Ext::from_f64(0.0) < g.t_bills[player_id] {
        let tbill_rate: Ext = cast::<Ext, _>(g.t_bill_rate);
        tbill_interest = (C001 * tbill_rate) * g.t_bills[player_id];
        if g.t_bills[player_id] <= Ext::from_f64(0.0) {
            tbill_interest = Ext::from_f64(0.0);
        }
        tbill_interest = reval(tbill_interest, nf);
        line_no = line_no + 1;
        val_text = col(tbill_interest, nf, 12);
        lines.set(line_no + 1, s(__content.string(2459)) + &val_text);
    }
    if 10 < g.player_ceo_of[player_id] {
        salary_bonus = reval(salary_bonus, nf);
        val_text = string_fill(12, b" ") + &fmt_trim(salary_bonus, nf);
        if Ext::from_f64(0.0) < val(val_text.as_bytes()) {
            line_no = line_no + 1;
            val_text = right(val_text.as_bytes(), 12);
            lines.set(line_no + 1, s(__content.string(2460)) + &val_text);
        }
    }
    if Ext::from_f64(0.0) < g.tmp_player_advances {
        adv_interest = (C001 * cast::<Ext, _>(g.prime_rate)) * g.tmp_player_advances;
        adv_interest = reval(adv_interest, nf);
        line_no = line_no + 1;
        val_text = col(adv_interest, nf, 12);
        lines.set(line_no + 1, s(__content.string(2461)) + &val_text);
    }
    if Ext::from_f64(0.0) < g.tmp_gov_bonds_value {
        if 0 < g.long_gov_bond_face[player_id] {
            let coupon_rate: Ext = val(right(g.long_bond_desc.as_bytes(), 5).as_bytes());
            gov_bond_int = (cast::<Ext, _>(g.long_gov_bond_face[player_id]) * C001) * coupon_rate;
            let maturity_year = to_long(val(mid(g.long_bond_desc.as_bytes(), 1, 4).as_bytes()));
            let mut qtrs = (maturity_year - g.cur_year) * 4 - (g.cur_quarter - 1);
            let discount_amt = cast::<Ext, _>(g.long_gov_bond_face[player_id]) - g.lt_gov_bond_cost[player_id];
            if qtrs < 1 {
                qtrs = 1;
            }
            amort_year = discount_amt / cast::<Ext, _>(qtrs);
            if 4 < qtrs {
                amort_year = amort_year * Ext::from_f64(4.0);
            }
            if maturity_year - g.cur_year < 2 {
                amort_year = discount_amt;
            }
            amort_total = amort_year;
        }
        if 0 < g.short_gov_bond_face[player_id] {
            let coupon_rate: Ext = val(right(g.short_bond_desc.as_bytes(), 5).as_bytes());
            gov_bond_int = (cast::<Ext, _>(g.short_gov_bond_face[player_id]) * C001) * coupon_rate + gov_bond_int;
            let maturity_year = to_long(val(mid(g.short_bond_desc.as_bytes(), 1, 4).as_bytes()));
            let mut qtrs = (maturity_year - g.cur_year) * 4 - (g.cur_quarter - 1);
            let discount_amt = cast::<Ext, _>(g.short_gov_bond_face[player_id]) - g.st_gov_bond_cost[player_id];
            if qtrs < 1 {
                qtrs = 1;
            }
            if qtrs < 5 {
                amort_year = discount_amt;
            } else {
                amort_year = (discount_amt / cast::<Ext, _>(qtrs)) * Ext::from_f64(4.0);
            }
            if maturity_year - g.cur_year < 2 {
                amort_year = discount_amt;
                if maturity_year - 1 == g.cur_year {
                    maturing_face = cast::<Ext, _>(g.short_gov_bond_face[player_id]);
                }
            }
            amort_total = amort_year + amort_total;
        }
        gov_bond_int = reval(gov_bond_int, nf);
        line_no = line_no + 1;
        val_text = col(gov_bond_int, nf, 12);
        lines.set(line_no + 1, s(__content.string(2462)) + &val_text);
    }
    if Ext::from_f64(0.0) < g.tmp_corp_bonds_value {
        for loop_idx in 1..=15 {
            let sec_id = g.hold_bond_id[(loop_idx, player_id)];
            if sec_id == 0 || g.credit_rating_num[sec_id] < 2 {
                continue;
            }
            let info = g.bond_issue_info[sec_id].clone();
            let coupon_rate: Ext = val(right(info.as_bytes(), 5).as_bytes());
            let mut bond_coupon = (cast::<Ext, _>(g.hold_bond_face[(loop_idx, player_id)]) * C001) * coupon_rate;
            corp_bond_int = bond_coupon + corp_bond_int;
            let maturity_year = to_long(val(mid(info.as_bytes(), 1, 4).as_bytes()));
            let mut qtrs = (maturity_year - g.cur_year) * 4;
            if 0 < qtrs {
                qtrs = qtrs - (g.cur_quarter - 1);
                if g.sim_co_count < sec_id {
                    qtrs = qtrs + 1;
                }
            }
            let discount_amt =
                cast::<Ext, _>(g.hold_bond_face[(loop_idx, player_id)]) - g.bond_hold_cost[(loop_idx, player_id)];
            skip_amort = 0;
            if discount_amt < Ext::from_f64(0.0) && 0 < instr(1, info.as_bytes(), b"cv") {
                skip_amort = 1;
            }
            let mut will_convert = 0;
            if Ext::from_f64(100.0) < g.bond_price[sec_id].get() && 0 < instr(1, info.as_bytes(), b"cv") {
                if maturity_year == g.cur_year {
                    will_convert = 1;
                } else if maturity_year - 1 == g.cur_year
                    && (cast::<Ext, _>((4 - g.cur_quarter) + 100)) < g.bond_price[sec_id].get()
                {
                    let conv_cap = val(parse_any(info.as_bytes(), b"|", 2).as_bytes());
                    if conv_cap < g.market_cap[sec_id] {
                        will_convert = 1;
                    }
                }
            }
            if skip_amort == 0 {
                if qtrs < 1 {
                    qtrs = 1;
                }
                if qtrs < 5 {
                    amort_year = discount_amt;
                } else {
                    amort_year = (discount_amt / cast::<Ext, _>(qtrs)) * Ext::from_f64(4.0);
                }
                amort_total = amort_year + amort_total;
            }
            if amort_total.abs() < Ext::from_f64(0.5) {
                amort_total = Ext::from_f64(0.0);
            } else {
                amort_total = reval(amort_total, nf);
            }
            if maturity_year > g.cur_year + 1 {
                continue;
            }
            let face = cast::<Ext, _>(g.hold_bond_face[(loop_idx, player_id)]);
            if g.cur_year + 1 == maturity_year {
                if g.cur_quarter == 1 && g.sim_co_count < sec_id {
                    continue;
                }
                if g.cur_quarter == 1 && will_convert == 0 {
                    maturing_face = face + maturing_face;
                    continue;
                }
            }
            // LAB_0043d16f
            if will_convert == 0 {
                maturing_face = face + maturing_face;
            }
            if g.cur_year == maturity_year {
                corp_bond_int = corp_bond_int - bond_coupon * Ext::from_f64(0.75);
            } else if g.cur_quarter != 1 {
                if g.cur_quarter == 2 {
                    if g.sim_co_count < sec_id {
                        continue;
                    }
                    bond_coupon = bond_coupon * Ext::from_f64(0.25);
                }
                if g.cur_quarter == 3 {
                    if g.sim_co_count < sec_id {
                        bond_coupon = bond_coupon * Ext::from_f64(0.25);
                    } else {
                        bond_coupon = bond_coupon * Ext::from_f64(0.5);
                    }
                }
                if g.cur_quarter == 4 {
                    if g.sim_co_count < sec_id {
                        bond_coupon = bond_coupon * Ext::from_f64(0.5);
                    } else {
                        bond_coupon = bond_coupon * Ext::from_f64(0.75);
                    }
                }
                corp_bond_int = corp_bond_int - bond_coupon;
            }
        }
        corp_bond_int = reval(corp_bond_int, nf);
        line_no = line_no + 1;
        val_text = col(corp_bond_int, nf, 12);
        lines.set(line_no + 1, s(__content.string(2463)) + &val_text);
    }
    if 0 < g.hold_stock_id[(0, player_id)] {
        for loop_idx in 1..=15 {
            let sec_id = cast::<i32, _>(g.hold_stock_id[(loop_idx, player_id)]);
            if sec_id == 0 {
                continue;
            }
            let pct = cast::<Ext, _>(g.hold_stock_pct[(loop_idx, player_id)]);
            if g.industry_num[sec_id] == 71 {
                let fund_income = val(parse_any(g.ent_misc_data[sec_id].as_bytes(), b"|", 12).as_bytes());
                div_income = (C004 * fund_income) * pct + div_income;
            } else if g.dividend_per_share[sec_id] != Ext::from_f64(0.0) {
                div_income = (g.dividend_per_share[sec_id] * pct) * g.shares_outstanding[sec_id] + div_income;
            }
        }
        div_income = reval(div_income, nf);
        line_no = line_no + 1;
        val_text = col(div_income, nf, 12);
        if div_income < Ext::from_f64(0.0) {
            val_text = val_text + __content.string(2464);
        }
        lines.set(line_no + 1, s(__content.string(2465)) + &val_text);
    }
    let total_income =
        ((((gov_bond_int + tbill_interest) + corp_bond_int) + div_income) + adv_interest) + salary_bonus;
    line_no = line_no + 1;
    lines.set(line_no + 1, s("                                ------------"));
    lines.set(line_no + 2, s(__content.string(2466)) + &col(total_income, nf, 12));
    line_no = line_no + 1;
    let mut storage_cost: Ext = calc_phys_commod_storage_cost(g, player_id);
    if storage_cost <= Ext::from_f64(0.0) {
        storage_cost = Ext::from_f64(0.0);
    } else {
        storage_cost = reval(storage_cost, nf);
        if storage_cost != Ext::from_f64(0.0) {
            lines.set(line_no + 2, s(__content.string(2467)) + &col(-storage_cost, nf, 12));
            line_no = line_no + 1;
        }
    }
    let mut interest_exp = (cast::<Ext, _>(g.borrow_rate) * g.bank_debt[player_id]) * C001;
    if interest_exp < Ext::from_f64(0.5) {
        interest_exp = Ext::from_f64(0.0);
    }
    interest_exp = reval(interest_exp, nf);
    line_no = line_no + 1;
    lines.set(line_no + 1, s(__content.string(2468)) + &col(-interest_exp, nf, 12));
    lines.set(line_no + 2, s("                                ------------"));
    let pre_tax_income = total_income - (storage_cost + interest_exp);
    lines.set(line_no + 3, s(__content.string(2469)) + &col(pre_tax_income, nf, 12));
    let taxable_income;
    if amort_total.abs() < Ext::from_f64(0.5) {
        amort_total = Ext::from_f64(0.0);
        taxable_income = pre_tax_income;
    } else {
        line_no = line_no + 1;
        val_text = col(amort_total, nf, 12);
        if amort_total <= Ext::from_f64(0.0) {
            lines.set(line_no + 3, s(__content.string(2471)) + &val_text);
        } else {
            lines.set(line_no + 3, s(__content.string(2470)) + &val_text);
        }
        line_no = line_no + 1;
        lines.set(line_no + 3, s("                                ------------"));
        line_no = line_no + 1;
        taxable_income = amort_total + pre_tax_income;
        lines.set(line_no + 3, s(__content.string(2472)) + &col(taxable_income, nf, 12));
    }
    let mut income_tax = (C001 * cast::<Ext, _>(5 + g.corp_tax_rate)) * taxable_income;
    if income_tax < Ext::from_f64(0.0) {
        income_tax = Ext::from_f64(0.0);
    }
    income_tax = reval(income_tax, nf);
    lines.set(line_no + 4, s(__content.string(2473)) + &col(-income_tax, nf, 12));
    let mut cap_taxes = calc_capital_taxes(g, player_id);
    if g.tmp_capital_tax == Ext::from_f64(0.0) && 0 < g.wealth_tax_rate {
        g.tmp_capital_tax = (cast::<Ext, _>(g.wealth_tax_rate) * C001)
            * ((g.player_prepaid_tax[player_id] + g.net_worth) - g.exchange_rate * Ext::from_f64(1000.0));
        if g.tmp_capital_tax <= Ext::from_f64(1.0) {
            g.tmp_capital_tax = Ext::from_f64(0.0);
        }
        g.corp_shares_tax_due = Ext::from_f64(4.0) * cap_taxes;
    } else if Ext::from_f64(0.0) < g.corp_shares_tax_due {
        g.corp_shares_tax_due = Ext::from_f64(4.0) * g.corp_shares_tax_due;
    }
    cap_taxes = g.tmp_capital_tax + g.corp_shares_tax_due;
    if Ext::from_f64(0.0) < cap_taxes {
        cap_taxes = reval(cap_taxes, nf);
        line_no = line_no + 1;
        val_text = col(-cap_taxes, nf, 12);
        lines.set(line_no + 4, s(__content.string(2474)) + &val_text);
        if g.tmp_capital_tax == Ext::from_f64(0.0) {
            lines.set(line_no + 4, s(__content.string(2475)) + &val_text);
        }
        if g.corp_shares_tax_due == Ext::from_f64(0.0) {
            lines.set(line_no + 4, s(__content.string(2476)) + &val_text);
        }
    }
    lines.set(line_no + 5, s("                                ------------"));
    let mut net_income = taxable_income - (income_tax + cap_taxes);
    val_text = col(net_income, nf, 12);
    lines.set(line_no + 6, s(__content.string(2477)) + &val_text);
    val_text = remove_any(val_text.as_bytes(), b",");
    net_income = val(val_text.as_bytes());
    line_no = line_no + 1;
    let tax = calc_player_income_tax(g, player_id);
    let mut tax_balance = g.player_prepaid_tax[player_id] - tax;
    g.tmp_ext1 = Ext::from_f64(0.0);
    val_text = col(tax_balance, nf, 12);
    let num_text = remove_any(val_text.as_bytes(), b",");
    tax_balance = val(num_text.as_bytes());
    if tax_balance != Ext::from_f64(0.0) {
        if Ext::from_f64(0.0) <= tax_balance {
            lines.set(line_no + 6, s(__content.string(2479)) + &val_text);
        } else {
            lines.set(line_no + 6, s(__content.string(2478)) + &val_text);
        }
        line_no = line_no + 1;
    }
    if amort_total != Ext::from_f64(0.0) {
        amort_total = -amort_total;
        let aux_text = col(amort_total, nf, 12);
        if Ext::from_f64(0.0) <= amort_total {
            lines.set(line_no + 6, s(__content.string(2481)) + &aux_text);
        } else {
            lines.set(line_no + 6, s(__content.string(2480)) + &aux_text);
        }
        line_no = line_no + 1;
    }
    if Ext::from_f64(0.0) < maturing_face {
        maturing_face = reval(maturing_face, nf);
        let suffix_text = col(maturing_face, nf, 12);
        lines.set(line_no + 6, s(__content.string(2482)) + &suffix_text);
        line_no = line_no + 1;
    }
    let living = reval(Ext::from(-g.exchange_rate), nf);
    val_text = col(living, nf, 10);
    lines.set(line_no + 6, s(__content.string(2483)) + &val_text);
    line_no = line_no + 1;
    lines.set(line_no + 6, s("                                ------------"));
    let tmp_amt = ((maturing_face + amort_total) + tax_balance) - g.exchange_rate * Ext::from_f64(1.0);
    net_income = tmp_amt + net_income;
    val_text = col(net_income, nf, 12);
    line_no = line_no + 1;
    lines.set(line_no + 6, s(__content.string(2484)) + &val_text);
    lines.set(line_no + 7, s("                                ============"));
    if 0 < swap_notional {
        line_no = line_no + 1;
        lines.set(line_no + 7, s(__content.string(2485)));
    }
    if amort_total == Ext::from_f64(0.0) {
        if skip_amort == 1 {
            line_no = line_no + 1;
            lines.set(line_no + 7, s(__content.string(2488)));
        }
    } else {
        line_no = line_no + 1;
        lines.set(line_no + 7, s(__content.string(2486)));
        line_no = line_no + 1;
        lines.set(line_no + 7, s(__content.string(2487)));
    }
    line_no = line_no + 1;
    lines.set(line_no + 7, s(__content.string(2489)));
    lines.set(line_no + 8, dash_line.clone());
    line_no + 8
}

/// " [runtime resource wording] <LastSelEntity>" block; returns the new `lineNo`.
fn unrealized_gains(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    lines: &mut Lines,
    mut line_no: i32,
    nf: &str,
) -> i32 {
    let __content = crate::content::current();
    let player_id = g.cur_player;
    let mut pos_flag = 0;
    let sec_id = g.last_sel_entity;
    if find_stock_holding(g, g.cur_player, sec_id) != 0 || find_option_positions(g, ui, player_id, sec_id, 0, 0) != 0 {
        pos_flag = 1;
    }
    let slot = find_bond_holding_slot(g, g.cur_player, sec_id);
    if 0 < slot && sec_id == g.hold_bond_id[(slot, player_id)] && 0 < instr(1, g.bond_issue_info[sec_id].as_bytes(), b"cv")
    {
        pos_flag = 2;
    }
    if pos_flag <= 0 {
        return line_no;
    }
    let sel = g.last_sel_entity;
    line_no = line_no + 1;
    lines.set(line_no, s(__content.string(8674)) + &g.ent_name[sel]);
    line_no = line_no + 1;
    lines.set(line_no, s("   --------------------------------------------"));
    let name_len = g.ent_name[sel].len();
    let aux_idx = name_len + 11;
    lines.set(line_no, lines.get(line_no) + &string_fill(name_len, b"-"));
    line_no = line_no + 1;
    let link = |sec_id: i32| string_fill(45, b" ") + "@C" + &link_id(sec_id);
    let mut total_gain: i64 = 0;
    let hold_idx = find_stock_holding(g, g.cur_player, sec_id);
    if hold_idx != 0 {
        let slot = to_long((cast::<Ext, _>(hold_idx)).abs());
        let cost_basis = to_quad(g.hold_stock_basis[(slot, player_id)]);
        let mut mkt_value = to_quad(g.market_cap[sec_id] * cast::<Ext, _>(g.hold_stock_pct[(slot, player_id)]));
        mkt_value = to_quad((cast::<Ext, _>(mkt_value)).abs());
        let mut x = cast::<Ext, _>(mkt_value) - cast::<Ext, _>(cost_basis);
        if hold_idx < 0 {
            x = cast::<Ext, _>(cost_basis) - cast::<Ext, _>(mkt_value);
        }
        let gain = to_quad(x);
        total_gain = gain;
        let val_text = s(" ") + &fmt_num(cast::<Ext, _>(gain), nf);
        let aux_text = g.ent_name[sec_id].clone() + " STOCK:" + &string_fill(20, b" ");
        let aux_text = mid(aux_text.as_bytes(), 1, aux_idx);
        lines.set(line_no, s("   ") + &aux_text + &val_text + &link(sec_id));
        line_no = line_no + 1;
    }
    if pos_flag == 2 {
        let slot = find_bond_holding_slot(g, g.cur_player, sec_id);
        let cost_basis = to_quad(g.bond_hold_cost[(slot, player_id)]);
        let mkt_value = to_quad((g.bond_price[sec_id].get() * cast::<Ext, _>(g.hold_bond_face[(slot, player_id)])) * C001);
        let gain = to_quad(cast::<Ext, _>(mkt_value) - cast::<Ext, _>(cost_basis));
        total_gain = to_quad(cast::<Ext, _>(gain) + cast::<Ext, _>(total_gain));
        let val_text = s(" ") + &fmt_num(cast::<Ext, _>(gain), nf);
        let aux_text = g.ent_name[sec_id].clone() + __content.string(8675) + &string_fill(20, b" ");
        let aux_text = mid(aux_text.as_bytes(), 1, aux_idx);
        lines.set(line_no, s("   ") + &aux_text + &val_text + &link(sec_id));
        line_no = line_no + 1;
    }
    if find_option_positions(g, ui, player_id, sec_id, 0, 0) != 0 {
        let mut gain: i64 = 0;
        for loop_idx in 1..=1000 {
            let rec = g.option_recs[loop_idx].clone();
            if rec.is_empty() {
                continue;
            }
            let f = |n: i32| val(parse_any(rec.as_bytes(), b"|", n).as_bytes());
            if f(1) != cast::<Ext, _>(sec_id) || f(3) != cast::<Ext, _>(g.cur_player) {
                continue;
            }
            let opt_type = to_long(f(2));
            let opt_qty = to_long(f(4));
            let opt_strike = f(5);
            let cost_basis = to_quad(f(8));
            let mut opt_price = calc_option_price(g, ui, rng, 0, Ext::from_f64(0.0), 0, 0, 0, loop_idx);
            if opt_qty < 0 {
                let spread = calc_spread_factor(opt_type, g.market_cap[sec_id], opt_strike);
                opt_price = spread * opt_price;
                // (tmpAmt = MAX(opt_price, 0.01) is not used)
            }
            let mkt_value = to_quad((cast::<Ext, _>(opt_qty)).abs() * opt_price);
            if opt_qty < 1 {
                gain = to_quad((cast::<Ext, _>(cost_basis) - cast::<Ext, _>(mkt_value)) + cast::<Ext, _>(gain));
            } else {
                gain = to_quad((cast::<Ext, _>(mkt_value) - cast::<Ext, _>(cost_basis)) + cast::<Ext, _>(gain));
            }
        }
        total_gain = to_quad(cast::<Ext, _>(gain) + cast::<Ext, _>(total_gain));
        let val_text = s(" ") + &fmt_num(cast::<Ext, _>(gain), nf);
        let aux_text = g.ent_name[sec_id].clone() + __content.string(8676) + &string_fill(20, b" ");
        let aux_text = mid(aux_text.as_bytes(), 1, aux_idx);
        lines.set(line_no, s("   ") + &aux_text + &val_text + &link(sec_id));
        line_no = line_no + 1;
    }
    lines.set(line_no, s("   ") + &string_fill(aux_idx, b" ") + &string_fill(20, b"-"));
    line_no = line_no + 1;
    let val_text = s(" ") + &fmt_num(cast::<Ext, _>(total_gain), nf);
    let aux_text = mid((s(__content.string(8677)) + &string_fill(20, b" ")).as_bytes(), 1, aux_idx);
    lines.set(line_no, s("   ") + &aux_text + &val_text);
    line_no = line_no + 1;
    lines.set(line_no, s("   ") + &string_fill(aux_idx, b" ") + &string_fill(20, b"="));
    line_no + 3
}
