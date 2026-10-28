//! `ShowPlayerStandings` (0x446484), `ShowBankLoanCustomers` (0x461796), `ShowMyAdvances` (0x499f8e) and
//! their callees `UpdateControlChain` (0x5220c5), `SetMenusGameEnded` (0x46e4cb, state part).
use crate::pb::real::{Ext, cast};


use super::{fmt, fmt_trim, link_id, s, str15};
use crate::logic::control::update_corp_control;
use crate::logic::finance::{calc_entity_financials, get_market_interest_rate};
use crate::logic::report::show_text_report;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{len, ltrim, mid, right, rtrim, string_fill, ucase, CaseMode};
use crate::pb::numeric::to_quad;
use crate::state::Globals;
use crate::ui::Ui;

/// A local `DIM lines(1 TO hi) AS STRING` (index 0 unused).
struct Lines(Vec<PbStr>);

impl Lines {
    fn new(hi: usize) -> Self {
        Lines(vec![PbStr::new(); hi + 1])
    }
    fn set(&mut self, i: i32, x: PbStr) {
        // Out of range is error 9 -> ON ERROR handler (ShowRuntimeError, exit); not reachable with
        // valid states (at most 5 players / 1600 entities).
        self.0[cast::<usize, _>(i)] = x;
    }
    fn get(&self, i: i32) -> PbStr {
        self.0[cast::<usize, _>(i)].clone()
    }
    /// `ARRAY SORT lines(start) FOR count` (ascending, binary string comparison).
    fn sort(&mut self, start: i32, count: i32) {
        if count <= 1 {
            return;
        }
        let a = cast::<usize, _>(start);
        let b = cast::<usize, _>(start + count);
        self.0[a..b].sort_by(|x, y| x.as_bytes().cmp(y.as_bytes()));
    }
}

fn ltrim_rtrim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

fn uc(x: &PbStr) -> PbStr {
    ucase(x.as_bytes(), CaseMode::Ansi)
}

/// Port of SetMenusGameEnded (0x46e4cb).
/// Menu changes for a finished game (UI) and `AutoAddStreamList = 0`.
pub fn set_menus_game_ended(g: &mut Globals) {
    // (UI) MENU SET TEXT 605 "&Restart W$R"; EnableMenuItem 605 on, 615/620/667/676 off
    g.auto_add_stream_list = 0;
}

/// Port of UpdateControlChain (0x5220c5).
/// For an active corporation (id > 10): `UpdateCorpControl` on it, up to 100 times on its
/// `LargestHolder` (if that is another corp and the corp is controlled by someone else), and on every
/// held company and their held companies. `CtrlChangeMsgMode` is set to 1 if it was 2, else 0, before
/// each call.
pub fn update_control_chain(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) {
    // ON ERROR -> ShowRuntimeError (error path not ported)
    if !(0 < entity_id) {
        return;
    }
    if g.entity_status[entity_id] != 1 || !(10 < entity_id) {
        return;
    }
    let mut msg_mode = 0;
    if g.ctrl_change_msg_mode == 2 {
        msg_mode = 1;
    }
    g.ctrl_change_msg_mode = msg_mode;
    update_corp_control(g, ui, rng, entity_id);
    if 0 < g.corp_controller[entity_id] && entity_id != g.corp_controller[entity_id] && 0 < g.largest_holder {
        let mut iter = 100;
        loop {
            let holder_id = cast::<i32, _>(g.largest_holder);
            if 10 < holder_id {
                g.ctrl_change_msg_mode = msg_mode;
                update_corp_control(g, ui, rng, holder_id);
            }
            iter = iter - 1;
            if iter == 0 {
                break;
            }
        }
    }
    for slot in 1..=15 {
        let sub_id = cast::<i32, _>(g.hold_stock_id[(slot, entity_id)]);
        if 0 < sub_id {
            g.ctrl_change_msg_mode = msg_mode;
            update_corp_control(g, ui, rng, sub_id);
            for sub_slot in 1..=15 {
                let sub_sub_id = cast::<i32, _>(g.hold_stock_id[(sub_slot, sub_id)]);
                if 0 < sub_sub_id {
                    g.ctrl_change_msg_mode = msg_mode;
                    update_corp_control(g, ui, rng, sub_sub_id);
                }
            }
        }
    }
}

/// Right-aligned cell of the net-worth table: `RIGHT$(STRING$(w, " ") + v, w) + tail`, or `over` (and
/// `hasOverflow = 1`) when `v` is longer than `w`.
fn cell(v: PbStr, w: i32, tail: &str, over: &str, has_overflow: &mut i32) -> PbStr {
    if w < len(v.as_bytes()) {
        *has_overflow = 1;
        s(over)
    } else {
        let t = string_fill(w, b" ") + &v;
        right(t.as_bytes(), w) + tail
    }
}

/// Port of ShowPlayerStandings (0x446484).
/// Report "Game Standings" / "FINAL STANDINGS": the leader (highest net worth among players without the
/// add-cash cheat), the players, a net-worth table and key information per player (bank, rating,
/// largest controlled company, companies controlled, stocks, employment). At game end also handles the
/// high score (`HiScore` DLL, MSGBOXes) and `SetMenusGameEnded`. Updates `PlayerCorpsControlled`, clears
/// control of companies held by bankrupt players and stale CEO posts.
pub fn show_player_standings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = Lines::new(900);
    let dash_line = string_fill(80, b"-");
    let num_fmt = "* ###########";
    let mut has_overflow = 0;
    let mut disqual_note = PbStr::new();
    let mut link_text;
    let mut best_value: Ext = Ext::from_f64(0.0);
    let mut best_id: i32 = 0;
    let mut score_usd: i64;
    let mut has_links = 0;
    let mut leader_name = PbStr::new();
    let mut msg_text = PbStr::new();
    let mut value_text;
    let mut row_text;
    let mut line_len;
    let out = |g: &Globals, p: i32| g.player_name[p] == "...OUT...";

    let player_count = g.num_players;
    let mut player_id = 1;
    while player_id <= player_count && (g.long_game_marker < 6 || 35 < g.game_end_year.wrapping_sub(g.start_year)) {
        if !out(g, player_id) {
            calc_entity_financials(g, ui, rng, player_id);
            if best_value < g.net_worth || best_value == Ext::from_f64(0.0) {
                if g.player_cheat_code[player_id] < 6 {
                    best_value = g.net_worth;
                    best_id = player_id;
                } else {
                    disqual_note = s(__content.string(8783));
                }
            }
        }
        player_id = player_id + 1;
    }
    if best_id < 1 {
        lines.set(2, s(__content.string(8784)));
        lines.set(3, s(__content.string(8785)));
        if g.game_end_flag == "Y" {
            leader_name = s(__content.string(8786));
        }
    } else {
        leader_name = g.ent_name[best_id].clone();
        lines.set(2, s(__content.string(8787)) + &leader_name + &disqual_note);
    }
    if g.game_end_flag == "Y" {
        set_menus_game_ended(g);
        lines.set(2, s(__content.string(8788)) + &leader_name + &disqual_note);
        score_usd = to_quad(best_value);
        if !(g.currency_code == "USA") && Ext::from_f64(0.0) < g.exchange_rate {
            score_usd = to_quad(best_value / g.exchange_rate);
        }
        let mut score_eligible = 0;
        if 0 < best_id && g.is_computer_player[best_id] == 0 {
            score_eligible = 1;
        }
        if 5 < g.long_game_marker || g.antitrust_cheat_used == 1 {
            score_eligible = 0;
        }
        if (cast::<Ext, _>(score_usd)) <= g.exchange_rate * Ext::from_f64(100.0) || score_eligible != 1 {
            if best_id == 0 && g.hi_score_done == 0 {
                msg_text = s(__content.string(8789))
                    + __content.string(8790);
                ui.msgbox(&msg_text, 8256, &s(__content.string(8791)));
                g.hi_score_done = 1;
            }
        } else {
            if 0 < best_id && g.hi_score_done == 0 {
                let hi_score_result = ui.high_score(&g.data_path, &leader_name, score_usd);
                if hi_score_result == 2 {
                    msg_text = s(__content.string(8792))
                        + &uc(&leader_name)
                        + __content.string(8793)
                        + __content.string(8794)
                        + &g.currency_symbol
                        + &fmt_trim(best_value, "* ###,")
                        + " "
                        + &g.million_prefix_lc
                        + "illion"
                        + &g.currency_suffix;
                    if g.currency_code == "USA" {
                        msg_text = msg_text + "!";
                    } else {
                        msg_text = msg_text + " ($" + &fmt_trim(cast::<Ext, _>(score_usd), "* ###,") + __content.string(8795);
                    }
                }
                if 0 < g.hi_score_done {
                    msg_text = PbStr::new();
                }
                g.hi_score_done = 1;
            }
            if 10 < len(msg_text.as_bytes()) {
                let title = g.app_title.clone();
                ui.msgbox(&msg_text, 8256, &title);
            }
        }
    }
    let fmt_c = format!("{num_fmt},");
    if 0 < best_id {
        row_text = s(__content.string(8796))
            + &g.currency_symbol
            + &fmt_trim(best_value, &fmt_c)
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix;
        if !(g.currency_code == "USA") {
            row_text = row_text + "  ($" + &fmt_trim(best_value / g.exchange_rate, &fmt_c) + __content.string(8797);
        }
        lines.set(3, row_text.clone());
        line_len = len(lines.get(3).as_bytes());
        lines.set(4, string_fill(line_len, b"-"));
    }
    line_len = len(lines.get(2).as_bytes());
    if line_len < len(lines.get(3).as_bytes()) {
        line_len = len(lines.get(3).as_bytes());
    }
    lines.set(4, string_fill(line_len, b"-"));
    msg_text = str15(cast::<Ext, _>(g.difficulty_level));
    if g.difficulty_level == 3
        && (g.wealth_tax_rate != -1
            || g.carbon_tax_rate != -1
            || g.oil_windfall_tax_rate != -1
            || g.holding_co_tax_rate != -1
            || g.corp_shares_tax_country != -1
            || g.health_care_tax_rate != -1)
    {
        msg_text = s(" 4");
    }
    lines.set(6, s(__content.string(8798)) + &msg_text + ")");
    lines.set(7, s("-----------------------------------------------------------"));
    let mut line_no = 8;
    for player_id in 1..=g.num_players {
        line_no = line_no + 1;
        msg_text = PbStr::new();
        if out(g, player_id) {
            msg_text = s(__content.string(8799));
        } else if 5 < g.player_cheat_code[player_id] {
            msg_text = s(__content.string(8800));
        }
        lines.set(line_no, s("PLAYER") + &str15(cast::<Ext, _>(player_id)) + " = " + &g.ent_name[player_id] + &msg_text);
    }
    line_no = line_no + 2;
    msg_text = g.currency_symbol.clone();
    lines.set(
        line_no + 1,
        s(__content.string(8801))
            + &msg_text
            + &g.million_prefix_uc
            + "illion"
            + &g.currency_suffix
            + ")",
    );
    lines.set(line_no + 2, dash_line.clone());
    lines.set(line_no + 3, s(__content.string(8802)));
    lines.set(line_no + 4, s(__content.string(8803)));
    lines.set(line_no + 5, s("-------------------- ----------- ----------- ----------- ----------- -----------"));
    line_no = line_no + 5;
    for player_id in 1..=g.num_players {
        calc_entity_financials(g, ui, rng, player_id);
        line_no = line_no + 1;
        msg_text = s("PLAYER") + &str15(cast::<Ext, _>(player_id));
        if len(g.ent_name[player_id].as_bytes()) < 21 {
            msg_text = uc(&g.ent_name[player_id]);
        }
        msg_text = msg_text + &string_fill(20, b" ");
        msg_text = mid(msg_text.as_bytes(), 1, 20) + " ";
        let t_bills = g.t_bills[player_id];
        let cash = g.cash[player_id];
        value_text = cell(fmt_trim(cash + t_bills, num_fmt), 11, " ", "    ****    ", &mut has_overflow);
        row_text = msg_text.clone() + &value_text;
        let t_bills = g.t_bills[player_id];
        let other = g.total_assets - (g.cash[player_id] + t_bills);
        value_text = cell(fmt_trim(other, num_fmt), 11, " ", "    ****    ", &mut has_overflow);
        row_text = row_text + &value_text;
        value_text = cell(fmt_trim(g.total_assets, num_fmt), 11, "", "    ****   ", &mut has_overflow);
        row_text = row_text + &value_text;
        value_text = cell(fmt_trim(-g.tmp_bank_loan, num_fmt), 12, " ", "   ****    ", &mut has_overflow);
        row_text = row_text + &value_text;
        value_text = cell(fmt_trim(g.net_worth, num_fmt), 11, "", "    ****    ", &mut has_overflow);
        row_text = row_text + &value_text;
        if out(g, player_id) {
            row_text = uc(&g.ent_name[player_id]) + __content.string(8804);
        }
        lines.set(line_no, row_text.clone());
    }
    lines.set(line_no + 1, dash_line.clone());
    if has_overflow == 1 {
        line_no = line_no + 3;
        lines.set(line_no, s(__content.string(8805)));
    }
    line_no = line_no + 3;
    lines.set(line_no, s(__content.string(8806)));
    lines.set(line_no + 1, s("           --------------------------------------"));
    line_no = line_no + 2;
    for player_id in 1..=g.num_players {
        line_no = line_no + 1;
        calc_entity_financials(g, ui, rng, player_id);
        row_text = g.ent_name[player_id].clone() + ":";
        if g.is_computer_player[player_id] == 1 {
            row_text = row_text + __content.string(8807);
        }
        line_len = len(row_text.as_bytes());
        lines.set(line_no, row_text.clone());
        lines.set(line_no + 1, string_fill(line_len, b"-"));
        value_text = ltrim_rtrim(&g.currency_symbol)
            + &fmt_trim(g.net_worth, "* ###,")
            + " "
            + &g.million_prefix_uc
            + "ILLION"
            + &uc(&g.currency_suffix);
        if out(g, player_id) {
            value_text = s("NIL");
        }
        lines.set(line_no + 2, s(__content.string(8808)) + &value_text);
        let bank = cast::<i32, _>(g.lending_bank[player_id]);
        msg_text = g.ent_name[bank].clone() + __content.string(8809) + &g.ent_symbol[bank] + ")";
        value_text = g.credit_rating_str.clone();
        if out(g, player_id) {
            msg_text = s("(NONE)");
            value_text = msg_text.clone();
            link_text = PbStr::new();
        } else {
            link_text = string_fill(95, b" ") + "@C" + &link_id(bank);
            has_links = 1;
        }
        lines.set(line_no + 3, s(__content.string(8810)) + &msg_text + &link_text);
        if out(g, player_id) {
            value_text = s(__content.string(8811));
            let k = player_id.wrapping_add(g.rand_seed) % 5;
            if k == 0 {
                value_text = value_text + __content.string(8812);
            }
            if k == 1 {
                value_text = value_text + __content.string(8813);
            }
            if k == 2 {
                value_text = value_text + __content.string(8814);
            }
            if k == 3 {
                value_text = value_text + "BEGGAR)";
            }
            if k == 4 {
                value_text = value_text + __content.string(8815);
            }
        }
        lines.set(line_no + 4, s(__content.string(8816)) + &value_text);
        update_control_chain(g, ui, rng, player_id);
        best_value = Ext::from_f64(0.0);
        best_id = 0;
        let mut ctrl_count = 0;
        for j in 11..=1600 {
            if g.entity_status[j] != 1 {
                continue;
            }
            if player_id == g.corp_controller[j] && out(g, player_id) {
                g.corp_controller[j] = 0;
                update_corp_control(g, ui, rng, j);
            }
            if player_id == g.corp_controller[j] {
                if best_value < g.market_cap[j] {
                    best_value = g.market_cap[j];
                    best_id = j;
                }
                ctrl_count = ctrl_count + 1;
            }
        }
        row_text = s(__content.string(8817));
        msg_text = s("(NONE)");
        if best_id < 1 {
            link_text = PbStr::new();
        } else {
            msg_text = g.ent_name[best_id].clone() + __content.string(8809) + &g.ent_symbol[best_id] + ")";
            link_text = string_fill(95, b" ") + "@C" + &link_id(best_id);
            has_links = 1;
        }
        lines.set(line_no + 5, row_text.clone() + &msg_text + &link_text);
        lines.set(line_no + 6, s(__content.string(8818)) + &str15(cast::<Ext, _>(ctrl_count)));
        g.player_corps_controlled[player_id] = ctrl_count;
        row_text = s(__content.string(8819));
        score_usd = 0;
        for j in 1..=15 {
            if score_usd < 15 && 0 < g.hold_stock_id[(j, player_id)] && 0 < g.hold_stock_pct[(j, player_id)] {
                score_usd = score_usd + 1;
                let held = cast::<i32, _>(g.hold_stock_id[(j, player_id)]);
                row_text = row_text + &g.ent_symbol[held];
                if len(row_text.as_bytes()) < 66 {
                    row_text = row_text + ", ";
                } else {
                    score_usd = 15;
                }
            }
        }
        if score_usd == 0 {
            row_text = row_text + "(NONE)";
        } else {
            row_text = rtrim(row_text.as_bytes());
            line_len = len(row_text.as_bytes());
            if right(row_text.as_bytes(), 1) == "," {
                row_text = mid(row_text.as_bytes(), 1, line_len - 1);
            }
            if score_usd == 1 && g.hold_stock_id[(0, player_id)] == 1 {
                let id = cast::<i32, _>(g.hold_stock_id[(1, player_id)]);
                row_text = row_text + &string_fill(85, b" ") + "@C" + &link_id(id);
                has_links = 1;
            }
        }
        lines.set(line_no + 7, row_text.clone());
        row_text = s(__content.string(8820));
        if out(g, player_id) {
            g.player_ceo_of[player_id] = 0;
            g.player_ceo_since_year[player_id] = 0;
        }
        let ceo_corp_id = g.player_ceo_of[player_id];
        if 10 < ceo_corp_id && g.entity_status[ceo_corp_id] != 1 {
            g.player_ceo_of[player_id] = 0;
            g.player_ceo_since_year[player_id] = 0;
        }
        if g.player_ceo_of[player_id] < 11 {
            msg_text = s(__content.string(8821));
            if out(g, player_id) {
                match player_id {
                    1 | 2 => {
                        msg_text = s(__content.string(8822));
                        if g.currency_code == "UK" {
                            msg_text = msg_text + __content.string(8823);
                        }
                    }
                    3 => msg_text = s(__content.string(8824)),
                    4 => msg_text = s(__content.string(8825)),
                    5 => msg_text = s(__content.string(8826)),
                    _ => {}
                }
            }
        } else {
            let ceo = g.player_ceo_of[player_id];
            msg_text = s(__content.string(8658)) + &g.ent_name[ceo];
            link_text = string_fill(95, b" ") + "@C" + &link_id(ceo);
            msg_text = msg_text + &link_text;
            has_links = 1;
        }
        lines.set(line_no + 8, row_text.clone() + &msg_text);
        line_no = line_no + 9;
    }
    lines.set(line_no + 2, s("@END"));
    if has_links == 1 {
        g.entity_pick_mode = s(__content.string(6093));
    }
    for line_no in 1..=900 {
        g.text_lines[line_no] = lines.get(line_no);
    }
    g.text_box_title = s(__content.string(8827));
    if g.game_end_flag == "Y" {
        g.text_box_title = s(__content.string(8828));
    }
    g.text_has_links = 1;
    g.text_box_mode = s("87");
    show_text_report(g, ui, rng);
}

/// `pctOfTotal = (amount / totalLoans) * 100` stored in a SINGLE.
fn pct_single(amount: Ext, total: Ext) -> Ext {
    cast::<Ext, _>(cast::<f32, _>((amount / total) * Ext::from_f64(100.0)))
}

/// Port of ShowBankLoanCustomers (0x461796).
/// Report "BANK LOAN PORTFOLIO": the business borrowers of the selected bank (`list_mode = 0`) or of
/// `ActiveEntity` (`list_mode` 1 = freeze/unfreeze picker, 2 = call-a-loan picker) with amount, rating,
/// share and rate, sorted by name; for `list_mode < 1` also the [runtime resource wording] and
/// interbank loans. With `list_mode = 0` and no bank selected only a MSGBOX is shown.
///
/// Quirk (BUGS.md): for `list_mode = 0` with fewer than 21 lines the padding loop overwrites `lines(1..3)`
/// with `" "` (the report heading in line 2 is lost) instead of appending lines.
pub fn show_bank_loan_customers(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, list_mode: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = Lines::new(1500);
    let dash_line = string_fill(66, b"-");
    if (g.sel_entity < 11 || 85 < g.sel_entity) && list_mode == 0 {
        let title = g.app_title.clone();
        ui.msgbox(&s(__content.string(8829)), 8208, &title);
        return;
    }
    let mut bank_id = g.sel_entity;
    if 0 < list_mode {
        bank_id = g.active_entity;
    }
    let mut footnote = PbStr::new();
    let bank_name = g.ent_name[bank_id].clone();
    lines.set(2, s(__content.string(8830)) + &bank_name + ":");
    if list_mode == 1 {
        lines.set(2, s(__content.string(8831)));
    }
    if list_mode == 1 {
        lines.set(3, s(__content.string(8832)));
    }
    if list_mode == 2 {
        lines.set(2, s(__content.string(8833)));
    }
    let mut cell_text = g.currency_name.clone();
    if g.currency_code == "EUR" {
        cell_text = s("Euro");
    }
    lines.set(4, s("        (") + &cell_text + __content.string(8834) + &g.million_prefix_lc + __content.string(8636));
    lines.set(5, dash_line.clone());
    lines.set(6, s(__content.string(8835)));
    lines.set(7, s(__content.string(8836)));
    lines.set(8, s("---------------------------     -------------- ------ ----- ------"));
    let mut line_no = 8;
    calc_entity_financials(g, ui, rng, bank_id);
    let total_loans = g.tmp_bank_total_loans;
    let mut row_text;
    let mut rate_text;
    for borrower_id in 1..=1600 {
        if !(borrower_id <= g.num_players || 85 < borrower_id) {
            continue;
        }
        if g.entity_status[borrower_id] != 1 || cast::<i32, _>(g.lending_bank[borrower_id]) != bank_id {
            continue;
        }
        if g.bank_debt[borrower_id] < Ext::from_f64(0.001) && list_mode == 2 {
            continue;
        }
        line_no = line_no + 1;
        calc_entity_financials(g, ui, rng, borrower_id);
        row_text = g.ent_name[borrower_id].clone();
        if 10 < borrower_id {
            row_text = row_text + " (" + &g.ent_symbol[borrower_id] + ")";
        }
        row_text = row_text + &string_fill(32, b" ");
        let debt = fmt(g.bank_debt[borrower_id], "* ###########.0");
        row_text = mid(row_text.as_bytes(), 1, 32) + &right(debt.as_bytes(), 13);
        row_text = row_text + "   " + &g.credit_rating_str + &string_fill(25, b" ");
        row_text = mid(row_text.as_bytes(), 1, 51);
        let pct_of_total = pct_single(g.bank_debt[borrower_id], total_loans);
        cell_text = fmt(pct_of_total, "* ###.0") + "%";
        cell_text = right(cell_text.as_bytes(), 6);
        row_text = row_text + "  " + &cell_text + " ";
        rate_text = s("   ") + &fmt(cast::<Ext, _>(g.borrow_rate), "* ##.00") + "%";
        rate_text = right(rate_text.as_bytes(), 6);
        lines.set(line_no, row_text.clone() + &rate_text);
        if g.credit_frozen[borrower_id] == 1 {
            lines.set(line_no, lines.get(line_no) + " FROZ.");
        } else if 0 < list_mode
            && g.corp_controller[borrower_id] < 11
            && 0 < g.corp_controller[borrower_id]
            && g.cur_player != g.corp_controller[borrower_id]
        {
            lines.set(line_no, lines.get(line_no) + "  *");
            footnote = s(__content.string(8837));
        }
        if borrower_id < 11 && g.cur_player != borrower_id && !(g.entity_pick_mode == "GetCorp") {
            if g.entity_pick_mode == __content.string(6093) {
                cell_text = s("0000");
            }
            // else: cellText keeps the percentage cell (as in the original)
        } else {
            cell_text = link_id(borrower_id);
        }
        lines.set(line_no, lines.get(line_no) + &string_fill(60, b" ") + "@C" + &cell_text);
    }
    lines.sort(9, line_no - 8);
    if !footnote.is_empty() {
        line_no = line_no + 2;
        lines.set(line_no, s("  ") + &footnote);
    }
    if 0 < list_mode {
        line_no = line_no + 2;
        lines.set(line_no, s(__content.string(8838)));
    }
    if list_mode < 1 {
        let borrower_id = bank_id;
        let portfolio = |g: &mut Globals, rng: &mut Rng, label: &str, amount: Ext, rate_type: i32| -> PbStr {
            let a = fmt(amount, "* ###########.0");
            let row_text = s(label) + &right(a.as_bytes(), 13) + __content.string(8839);
            let pct_of_total = pct_single(amount, total_loans);
            let mut cell_text = fmt(pct_of_total, "* ###.0") + "% ";
            cell_text = right(cell_text.as_bytes(), 7);
            let loan_rate = crate::pb::numeric::double(get_market_interest_rate(g, rng, rate_type)); // stored in a DOUBLE
            let mut rate_text = fmt(loan_rate, "* ##.00") + "%";
            rate_text = right(rate_text.as_bytes(), 6);
            row_text + &cell_text + &rate_text
        };
        let amount = cast::<Ext, _>(g.bank_consumer_loans[borrower_id]);
        let row = portfolio(g, rng, __content.string(8840), amount, 1);
        lines.set(line_no + 2, row);
        line_no = line_no + 2;
        let amount = cast::<Ext, _>(g.bank_mortgage_loans[borrower_id]);
        let row = portfolio(g, rng, __content.string(8841), amount, 2);
        lines.set(line_no + 2, row);
        line_no = line_no + 2;
        let amount = cast::<Ext, _>(g.subprime_secs[borrower_id]);
        let row = portfolio(g, rng, __content.string(8842), amount, 3);
        lines.set(line_no + 2, row);
        line_no = line_no + 2;
        if borrower_id < 85 && Ext::from_f64(0.0) < g.bank_debt[borrower_id + 1] {
            row_text = s(__content.string(8843)) + &g.ent_symbol[borrower_id + 1] + &string_fill(30, b" ");
            row_text = mid(row_text.as_bytes(), 1, 32);
            calc_entity_financials(g, ui, rng, borrower_id + 1);
            let debt = fmt(g.bank_debt[borrower_id + 1], "* ###########.0");
            row_text = row_text + &right(debt.as_bytes(), 13) + "   " + &g.credit_rating_str + "      ";
            row_text = mid(row_text.as_bytes(), 1, 53);
            let pct_of_total = pct_single(g.bank_debt[borrower_id + 1], total_loans);
            cell_text = fmt(pct_of_total, "* ###.0") + "% ";
            cell_text = right(cell_text.as_bytes(), 7);
            rate_text = fmt(cast::<Ext, _>(g.entity_borrow_rate), "* ##.00") + "%";
            rate_text = right(rate_text.as_bytes(), 6);
            row_text = row_text + &cell_text + &rate_text;
            if g.entity_pick_mode == __content.string(6093) {
                row_text = row_text + &string_fill(60, b" ") + "@C" + &link_id(1 + borrower_id);
            }
            lines.set(line_no + 2, row_text);
            line_no = line_no + 2;
        }
        if list_mode == 0 && line_no < 21 {
            // Quirk: pads lines(1..3) (overwriting the heading) instead of lines(lineNo).
            for borrower_id in 1..=3 {
                line_no = line_no + 1;
                lines.set(borrower_id, s(" "));
            }
        }
    }
    if line_no < 995 {
        lines.set(line_no + 1, s("  "));
        lines.set(line_no + 2, s("  "));
        line_no = line_no + 2;
    }
    line_no = line_no + 1;
    lines.set(line_no, s("@END"));
    for borrower_id in 1..=line_no {
        g.text_lines[borrower_id] = lines.get(borrower_id);
    }
    g.text_box_title =
        s(__content.string(8844)) + &bank_name + " (" + &g.ent_symbol[g.sel_entity] + ") ===";
    g.text_has_links = 1;
    g.text_box_mode = s("20");
    show_text_report(g, ui, rng);
}

/// Port of ShowMyAdvances (0x499f8e).
/// Report " [runtime resource wording] <player>": the player's advances (demand loans) to corporations
/// with the borrower's credit rating, sorted by name, and the total; a MSGBOX if there are none.
/// Lines carry `@C` links when `EntityPickMode` is "ChangeCorp" or "GetCorp".
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn show_my_advances(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, player_id: i32) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError (error path not ported)
    let mut lines = Lines::new(1050);
    let separator = string_fill(59, b"-");
    if g.num_players < player_id {
        return;
    }
    calc_entity_financials(g, ui, rng, player_id);
    let total_advances = g.tmp_player_advances;
    if !(Ext::from_f64(0.0) < g.tmp_player_advances) {
        let field = s(__content.string(8845)) + __content.string(8846);
        let title = g.app_title.clone();
        ui.msgbox(&field, 8208, &title);
        return;
    }
    lines.set(2, s(__content.string(8847)));
    if g.entity_pick_mode == "GetCorp" {
        lines.set(2, s(__content.string(8848)));
    }
    let mut currency_note = PbStr::new();
    if !g.currency_suffix.is_empty() {
        currency_note = s("of") + &g.currency_suffix;
    }
    lines.set(
        4,
        s(__content.string(8849))
            + &g.currency_symbol
            + &g.million_prefix_uc
            + __content.string(8850)
            + &currency_note
            + ")",
    );
    lines.set(6, separator.clone());
    lines.set(7, s(__content.string(8851)));
    lines.set(8, s(__content.string(8852)));
    lines.set(9, s("---------------------------     -----------------    ------"));
    let mut line_count = 9;
    let mut corp_id = 11;
    loop {
        if g.entity_status[corp_id] == 1 && Ext::from_f64(0.0) < g.player_advances[(player_id, corp_id)] {
            line_count = line_count + 1;
            let add_link = 1;
            let mut line_text = g.ent_name[corp_id].clone() + &string_fill(35, b" ");
            line_text = mid(line_text.as_bytes(), 1, 33);
            let mut field =
                string_fill(30, b" ") + &fmt(g.player_advances[(player_id, corp_id)], "##############.00") + "      ";
            field = right(field.as_bytes(), 21);
            line_text = line_text + &field;
            calc_entity_financials(g, ui, rng, corp_id);
            field = s("  ") + &g.credit_rating_str;
            field = right(field.as_bytes(), 3);
            lines.set(line_count, line_text + &field);
            if (g.entity_pick_mode == __content.string(6093) || g.entity_pick_mode == "GetCorp") && add_link == 1 {
                lines.set(line_count, lines.get(line_count) + &string_fill(60, b" ") + "@C" + &link_id(corp_id));
            }
            if line_count == 995 {
                corp_id = 1600;
            }
        }
        corp_id = corp_id + 1;
        if !(corp_id < 1601) {
            break;
        }
    }
    lines.sort(10, line_count - 9);
    if line_count == 995 {
        line_count = 996;
        lines.set(line_count, s(__content.string(8853)));
    }
    line_count = line_count + 1;
    lines.set(line_count, separator.clone());
    line_count = line_count + 1;
    lines.set(line_count, s("  "));
    line_count = line_count + 1;
    let field = string_fill(30, b" ") + &fmt(total_advances, "###############.00");
    lines.set(line_count, s(__content.string(8854)) + &right(field.as_bytes(), 16));
    line_count = line_count + 1;
    lines.set(line_count, s("                                ================="));
    line_count = line_count + 1;
    lines.set(line_count, s("@END"));
    for corp_id in 1..=line_count {
        g.text_lines[corp_id] = lines.get(corp_id);
    }
    g.text_box_title = s(__content.string(8855)) + &uc(&g.ent_name[player_id]) + " ===";
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
}
