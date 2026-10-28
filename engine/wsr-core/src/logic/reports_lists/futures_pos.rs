//! Futures and physical-commodity position lists: `ShowFuturesPositions` (0x62cbaf),
//! `ListPhysCommodHoldings` (0x65c08f) (`ConsolidatePhysCommodRecs` is `logic::reports_corp`'s port).
//!
//! Records (see `logic::futures`): `FuturesRecs(i)` = `"COMMODITY|owner|qty|month|year|cost|price"`,
//! `PhysCommodRec(i)` = `"COMMODITY|owner|units|cost"`.

#![allow(clippy::assign_op_pattern, clippy::manual_range_contains, clippy::collapsible_if, clippy::neg_multiply)]
use crate::pb::real::{Ext, cast};


use super::{fmt, fmt_trim, s, str15, str_trim};
use crate::logic::earnings::calc_cur_month_of_year;
use crate::logic::finance::get_phys_commod_value;
use crate::logic::futures::{calc_futures_contract_value, count_futures_positions, sort_futures_recs};
use crate::logic::holdings::pack_holding_arrays_with_news as pack_holding_arrays;
use crate::logic::reports_corp::consolidate_phys_commod_recs;
use crate::logic::report::{describe_defunct_entity, show_quick_message, show_text_report};
use crate::pb::numeric::{val, val_ext, Ext80};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, lcase, ltrim, mcase, mid, parse_any, replace, right, rtrim, space, string_fill, CaseMode};
use crate::pb::numeric::{ceil, to_long, to_quad, Cux};
use crate::state::Globals;
use crate::ui::{mb, Ui};

/// `PARSE$(rec, ANY "|", n)`.
fn pf(rec: &PbStr, n: i32) -> PbStr {
    parse_any(rec.as_bytes(), b"|", n)
}

/// `VAL(PARSE$(rec, ANY "|", n))`.
fn pfv(rec: &PbStr, n: i32) -> Ext {
    val(pf(rec, n).as_bytes())
}

fn trim(x: &PbStr) -> PbStr {
    rtrim(ltrim(x.as_bytes()).as_bytes())
}

/// A local `DIM lines(0 TO hi) AS STRING`; an index past `hi` is error 9 (ON ERROR handler: ShowRuntimeError
/// and exit), reported as `Err`.
struct Lines(Vec<PbStr>);

struct OutOfRange;

impl Lines {
    fn new(hi: usize) -> Self {
        Lines(vec![PbStr::new(); hi + 1])
    }
    fn set(&mut self, i: i32, x: PbStr) -> Result<(), OutOfRange> {
        match self.0.get_mut(cast::<usize, _>(i)) {
            Some(e) if i >= 0 => {
                *e = x;
                Ok(())
            }
            _ => Err(OutOfRange),
        }
    }
    fn get(&self, i: i32) -> Result<PbStr, OutOfRange> {
        if i < 0 { Err(OutOfRange) } else { self.0.get(cast::<usize, _>(i)).cloned().ok_or(OutOfRange) }
    }
    /// `FOR i = 1 TO count: TextLines(i) = lines(i): NEXT`.
    fn copy_to_text_lines(&self, g: &mut Globals, count: i32) -> Result<(), OutOfRange> {
        for i in 1..=count {
            if i > 1050 {
                return Err(OutOfRange);
            }
            g.text_lines[i] = self.get(i)?;
        }
        Ok(())
    }
}

/// `cux = VAL(text)` (`PB_CuxStore`: the 80-bit VAL result times 100, rounded half-even). Done in 80-bit
/// precision: a decimal like "587.965" is just above the tie in 80 bits but below it as f64.
fn cux_from_text(text: &PbStr) -> Cux {
    let y = val_ext(text.as_bytes()).fmul(Ext80::from_i64(100));
    let r = y.into_ext();
    let mut c = Cux(cast::<i64, _>(r.round_ties_even()));
    if r.fract().abs() == Ext::from_f64(0.5) {
        // a tie in f64: decide by the exact 80-bit value
        let x = Ext80::from_ext(r);
        let key = |e: Ext80| (e.se & 0x7fff, e.mant);
        if key(y) != key(x) {
            let away = key(y) > key(x); // |y| > |r|
            c = Cux(if away { cast::<i64, _>(r + Ext::from_f64(0.5) * r.signum()) } else { cast::<i64, _>(r - Ext::from_f64(0.5) * r.signum()) });
        }
    }
    c
}

/// The currency words of the footnotes: `CurrencySuffix`, or `" " + CurrencyName` when the suffix is
/// empty or contains "SwFr"; "Pound"/"Dollar"/"Franc" get a plural "s".
fn currency_words(g: &Globals) -> PbStr {
    let mut t = g.currency_suffix.clone();
    if t.is_empty() || 0 < instr(1, t.as_bytes(), b"SwFr") {
        t = s(" ") + &g.currency_name;
    }
    if 0 < instr(1, t.as_bytes(), b"Pound")
        || 0 < instr(1, t.as_bytes(), b"Dollar")
        || 0 < instr(1, t.as_bytes(), b"Franc")
    {
        if !(right(t.as_bytes(), 1) == "s") {
            t = t + "s";
        }
    }
    t
}

/// Port of ShowFuturesPositions (0x62cbaf).
/// Report "List of … Futures Contracts for <entity>": one line per futures record of `entity_id` (mode 2:
/// long positions in `CurCommodity`, 3: short ones; modes 2..4 add `@C<rec>` link codes), totals and the
/// commodity margin balance. Without futures positions it offers the physical holdings (YES/NO/CANCEL:
/// `ListPhysCommodHoldings`) or, in mode 1, the current player's futures. Returns the record picked from
/// the list (`TargetCorp`), or in modes 2/3 the only matching record when there is exactly one; else 0.
///
/// Sorts `FuturesRecs` ten times first (`SortFuturesRecs`, which packs the holding arrays).
pub fn show_futures_positions(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32, list_mode: i32) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (an index error ends the routine, result 0)
    futures_positions(g, ui, rng, entity_id, list_mode).unwrap_or(0)
}

fn futures_positions(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    entity_id: i32,
    list_mode: i32,
) -> Result<i32, OutOfRange> {
    let __content = crate::content::current();
    let mut lines = Lines::new(1500);
    let mut result = 0;
    let mut showing_player = 0;
    let mut total_gain: i64 = 0;
    for _ in 0..5 {
        let events = sort_futures_recs(g);
        crate::logic::holdings::post_pack_events(g, ui, events);
        let events = sort_futures_recs(g);
        crate::logic::holdings::post_pack_events(g, ui, events);
    }
    let mut ent_id = entity_id;
    let mut mode = list_mode;
    if g.entity_status[ent_id] == 2 {
        describe_defunct_entity(g, ui, ent_id);
        return Ok(result);
    }
    let mut line_text;
    if count_futures_positions(g, ent_id, 0) == 0 {
        line_text = g.ent_name[ent_id].clone()
            + __content.string(8534)
            + __content.string(8535)
            + "\r"
            + "\r";
        if get_phys_commod_value(g, ent_id, &PbStr::new()) > Ext::from_f64(0.0) {
            line_text = line_text + __content.string(8536);
            if ent_id < 11 {
                line_text = replace(line_text.as_bytes(), __content.bytes(8537), __content.bytes(8538));
            }
            let answer = ui.msgbox(
                &line_text,
                mb::TASKMODAL | mb::ICONQUESTION | mb::YESNOCANCEL,
                &s(__content.string(8539)),
            );
            if answer == 6 {
                let phys_rec_idx = list_phys_commod_holdings(g, ui, rng, ent_id, 0);
                if 0 < phys_rec_idx
                    && cast::<Ext, _>(phys_rec_idx) <= val(g.phys_commod_rec[0].as_bytes())
                    && pfv(&g.phys_commod_rec[phys_rec_idx], 2) == cast::<Ext, _>(g.active_entity)
                {
                    line_text = g.phys_commod_rec[phys_rec_idx].clone();
                    // (UI) DIALOG END hDlgTransMenu
                    g.action_menu_open = 0;
                    g.cur_commodity = trim(&pf(&line_text, 1));
                    if 10 < g.active_entity {
                        let a = g.active_entity;
                        consolidate_phys_commod_recs(g, a);
                    }
                    let a = g.active_entity;
                    crate::logic::commod_ai::physical::sell_phys_commod(g, ui, rng, a, phys_rec_idx, 0, 0.0f32, 0);
                    // (UI) EnableMainControls
                }
            }
            return Ok(result);
        }
        if g.cur_player != ent_id {
            let cp = g.cur_player;
            if 0 < count_futures_positions(g, cp, 0) && mode == 1 {
                line_text = line_text + __content.string(8540);
                let answer = ui.msgbox(
                    &line_text,
                    mb::TASKMODAL | mb::ICONQUESTION | mb::YESNOCANCEL,
                    &s(__content.string(8539)),
                );
                if answer != 6 {
                    return Ok(result);
                }
                ent_id = g.cur_player;
                showing_player = 1;
            }
        }
        if showing_player == 0 {
            let mut msg = s(__content.string(8541))
                + "\r\n"
                + "\r\n"
                + &g.ent_name[ent_id]
                + __content.string(8542)
                + __content.string(8543);
            show_quick_message(g, ui, &mut msg);
            return Ok(result);
        }
    }
    if mode == 1 && g.active_entity == g.sel_entity && entity_id == ent_id {
        mode = 4;
    }
    if mode == 1 && g.cur_player == ent_id && entity_id != ent_id && g.active_entity == g.cur_player {
        mode = 4;
    }
    lines.set(2, s(__content.string(8544)))?;
    lines.set(3, s(__content.string(8545)))?;
    lines.set(4, s(__content.string(8546)))?;
    lines.set(5, s("--------- --------- ------- --------- -------------- -------------- ------------"))?;
    let mut line_idx = 5;
    let mut field_str;
    let mut line_tag;
    for rec_idx in 1..=1000 {
        let fut_rec = g.futures_recs[rec_idx].clone();
        if fut_rec.is_empty() {
            continue;
        }
        if pfv(&fut_rec, 2) != cast::<Ext, _>(ent_id) {
            continue;
        }
        let commod = pf(&fut_rec, 1);
        if 1 < mode && mode < 4 && !(commod == g.cur_commodity) {
            continue;
        }
        let qty = to_long(pfv(&fut_rec, 3));
        if (mode != 2 || -1 < qty) && (mode != 3 || qty < 1) {
            line_idx = line_idx + 1;
            line_text = commod.clone() + "        ";
            if commod == "STOCKS" {
                line_text = s(__content.string(8547)) + "       ";
            }
            line_text = mid(line_text.as_bytes(), 1, 10);
            let contract_price = cux_from_text(&pf(&fut_rec, 7));
            field_str = fmt_trim(contract_price.get(), "* ###.00");
            if 9 < field_str.len() {
                field_str = fmt_trim(contract_price.get(), "* ###");
            }
            field_str = space(7) + &field_str + " ";
            line_text = line_text + &right(field_str.as_bytes(), 10);
            let deliv_month = to_long(pfv(&fut_rec, 4));
            let deliv_year = to_long(pfv(&fut_rec, 5));
            let cost_basis = pfv(&fut_rec, 6);
            let mut notional = cost_basis;
            if qty < 0 {
                notional = cost_basis * -Ext::from_f64(1.0);
            }
            field_str = s("0") + &str_trim(cast::<Ext, _>(deliv_month)) + "/" + &str_trim(cast::<Ext, _>(deliv_year)) + " ";
            line_text = line_text + &right(field_str.as_bytes(), 8);
            let qty_str = str15(cast::<Ext, _>(qty));
            field_str = space(8) + &qty_str + " ";
            line_text = line_text + &right(field_str.as_bytes(), 10);
            field_str = space(14) + &fmt(notional, "* #####") + " ";
            line_text = line_text + &right(field_str.as_bytes(), 15);
            let cur_value = calc_futures_contract_value(g, ui, rng, &fut_rec);
            field_str = space(14) + &fmt(cur_value, "* #####") + " ";
            line_text = line_text + &right(field_str.as_bytes(), 15);
            let mut net_gain = Cux::default();
            net_gain.set(cur_value - cost_basis);
            if qty < 0 {
                net_gain.set(cost_basis - cur_value.abs());
            }
            total_gain = to_quad(net_gain.get() + cast::<Ext, _>(total_gain));
            field_str = space(3) + &fmt(net_gain.get(), "* ##########");
            line_text = line_text + &right(field_str.as_bytes(), 12);
            lines.set(line_idx, line_text.clone())?;
            line_tag = s("000") + &str_trim(cast::<Ext, _>(rec_idx));
            line_tag = s("@C") + &right(line_tag.as_bytes(), 4);
            if 1 < mode {
                lines.set(line_idx, lines.get(line_idx)? + &space(40) + &line_tag)?;
            }
        }
    }
    if line_idx == 5 {
        line_text = s(__content.string(8548))
            + &g.ent_name[ent_id]
            + ".";
        field_str = s("long");
        if mode == 3 {
            field_str = s("short");
        }
        line_tag = g.cur_commodity.clone();
        if line_tag == "STOCKS" {
            line_tag = s(__content.string(2499));
        }
        if mode == 2 || mode == 3 {
            line_text = g.ent_name[ent_id].clone()
                + __content.string(3737)
                + &field_str
                + __content.string(8549)
                + &lcase(line_tag.as_bytes(), CaseMode::Ansi)
                + __content.string(8550);
        }
        show_quick_message(g, ui, &mut line_text);
        return Ok(result);
    }
    if line_idx == 6 && 1 < mode && mode < 4 {
        result = to_long(val(right(lines.get(6)?.as_bytes(), 4).as_bytes()));
        return Ok(result);
    }
    line_idx = line_idx + 1;
    lines.set(line_idx, lines.get(5)?)?;
    line_idx = line_idx + 1;
    line_text = s(__content.string(8551));
    field_str = space(15) + &fmt(cast::<Ext, _>(total_gain), "* ##########");
    lines.set(line_idx, line_text + &right(field_str.as_bytes(), 12))?;
    line_idx = line_idx + 1;
    line_text = s(__content.string(8552));
    field_str = space(15) + &fmt(g.commod_margin_bal[ent_id], "* ##########");
    lines.set(line_idx, line_text + &right(field_str.as_bytes(), 12))?;
    line_idx = line_idx + 1;
    lines.set(line_idx, space(68) + &string_fill(12, b"-"))?;
    line_idx = line_idx + 1;
    line_text = s(__content.string(8553));
    field_str = space(15) + &fmt(cast::<Ext, _>(total_gain) + g.commod_margin_bal[ent_id], "* ##########");
    lines.set(line_idx, line_text + &right(field_str.as_bytes(), 12))?;
    line_idx = line_idx + 1;
    lines.set(line_idx, space(68) + &string_fill(12, b"="))?;
    line_idx = line_idx + 1;
    line_text = currency_words(g);
    lines.set(line_idx, s(__content.string(8554)))?;
    lines.set(line_idx + 1, s(__content.string(8555)))?;
    lines.set(line_idx + 2, s(__content.string(8556)))?;
    lines.set(line_idx + 3, s(__content.string(8557)))?;
    lines.set(line_idx + 4, s("     ") + &g.million_prefix_lc + __content.string(8089) + &line_text + ".)")?;
    line_idx = line_idx + 5;
    if mode < 2 || g.game_end_flag == "Y" {
        line_idx = line_idx - 2;
    } else {
        line_text = s(__content.string(8558));
        if mode == 3 {
            line_text = s(__content.string(8559));
        }
        if mode == 4 {
            line_text = s(__content.string(8560));
        }
        lines.set(line_idx + 2, s(__content.string(8561)) + &line_text + ")")?;
    }
    lines.set(line_idx + 4, s("@END"))?;
    let line_count = line_idx + 6;
    lines.copy_to_text_lines(g, line_count)?;
    field_str = mcase(g.cur_commodity.as_bytes(), CaseMode::Ansi) + " ";
    if g.cur_commodity == "STOCKS" {
        field_str = s(__content.string(8562));
    }
    if mode == 1 || mode == 4 {
        field_str = s(__content.string(8563));
    }
    let sim = g.sim_co_count;
    let cur_month = to_long(ceil(calc_cur_month_of_year(g, sim)));
    line_text = str_trim(cast::<Ext, _>(cur_month)) + "/" + &str_trim(cast::<Ext, _>(g.cur_year));
    g.text_box_title = s(__content.string(8564)) + &field_str + __content.string(8565) + &g.ent_name[ent_id] + " as of " + &line_text;
    if 1 < mode {
        g.entity_pick_mode = s("GetCorp");
        g.list_item_noun = s("item");
    }
    g.text_box_mode = s("74");
    g.target_corp = 0;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    if 0 < g.target_corp
        && cast::<Ext, _>(g.target_corp) <= val(g.futures_recs[0].as_bytes())
        && pfv(&g.futures_recs[g.target_corp], 2) == cast::<Ext, _>(g.active_entity)
    {
        if 0 < g.target_corp {
            result = g.target_corp;
        }
        g.target_corp = 0;
        return Ok(result);
    }
    g.target_corp = 0;
    result = 0;
    Ok(result)
}

/// Port of ListPhysCommodHoldings (0x65c08f) with its GOSUBs `ListPhysCommod_UnitFactor` (0x65d6e7),
/// `ListPhysCommod_CalcUnitCost` (0x65d7d1) and `ListPhysCommod_ValueLine` (0x65d7f6).
/// Report " [runtime resource wording] Owned and Stored by <entity>": units, cost per unit, spot price,
/// cost, value and unrealized gain per record (`@C<rec>` links when the list is selectable: `select_mode
/// = 1` or the entity is `ActiveEntity`). Without holdings it offers the futures list or (for a
/// company) the current player's holdings. Returns the record picked from the list (`TargetCorp`), else 0.
///
/// Quirk: each line's value is `GetPhysCommodValue(entity, name)`, i.e. the value of *all* the entity's
/// records of that commodity (only companies get their records merged by `ConsolidatePhysCommodRecs`).
pub fn list_phys_commod_holdings(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32, select_mode: i32) -> i32 {
    // ON ERROR GOTO -> ShowRuntimeError (an index error ends the routine, result 0)
    list_phys(g, ui, rng, entity_id, select_mode).unwrap_or(0)
}

fn list_phys(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32, select_mode: i32) -> Result<i32, OutOfRange> {
    let __content = crate::content::current();
    let mut lines = Lines::new(1000);
    let mut selected_rec = 0;
    let mut show_player = 0;
    let mut sum_cost: i64 = 0;
    let mut sum_gain: i64 = 0;
    let mut ent_id = entity_id;
    if g.entity_status[ent_id] == 2 {
        describe_defunct_entity(g, ui, ent_id);
        return Ok(selected_rec);
    }
    consolidate_phys_commod_recs(g, ent_id);
    let mut can_select = select_mode;
    if ent_id == g.active_entity {
        can_select = 1;
    }
    let fmt_decimal = "* ##############.00";
    let fmt_int = "* ###";
    pack_holding_arrays(g, ui);
    let mut total_value = get_phys_commod_value(g, ent_id, &PbStr::new());
    let mut line_text;
    if total_value == Ext::from_f64(0.0) {
        let cp = g.cur_player;
        total_value = get_phys_commod_value(g, cp, &PbStr::new());
        line_text = g.ent_name[ent_id].clone() + __content.string(8566) + "\r" + "\r";
        if 0 < count_futures_positions(g, ent_id, 0) {
            line_text = line_text + __content.string(8567);
            if ent_id < 11 {
                line_text = replace(line_text.as_bytes(), b"futures", __content.bytes(8568));
            }
            let answer = ui.msgbox(
                &line_text,
                mb::TASKMODAL | mb::ICONQUESTION | mb::YESNOCANCEL,
                &s(__content.string(8569)),
            );
            if answer == 6 {
                show_futures_positions(g, ui, rng, ent_id, 0);
            }
            return Ok(selected_rec);
        }
        if 10 < ent_id && Ext::from_f64(0.0) < total_value {
            line_text = line_text + __content.string(8570);
            let answer = ui.msgbox(
                &line_text,
                mb::TASKMODAL | mb::ICONQUESTION | mb::YESNOCANCEL,
                &s(__content.string(8569)),
            );
            if answer != 6 {
                return Ok(selected_rec);
            }
            // SelectActingEntity(1): the player becomes the acting entity (mode 1 doesn't read the clock)
            crate::logic::trading::select_acting_entity(g, ui, rng, 1, &crate::logic::newgame::LocalTime::default());
            ent_id = g.cur_player;
            can_select = 1;
            show_player = 1;
        }
        if show_player == 0 {
            let mut msg = s(__content.string(8541))
                + "\r\n"
                + "\r\n"
                + &g.ent_name[ent_id]
                + __content.string(2689);
            show_quick_message(g, ui, &mut msg);
            return Ok(selected_rec);
        }
    }
    lines.set(2, s(__content.string(8571)))?;
    lines.set(3, s(__content.string(8572)))?;
    lines.set(4, s(__content.string(8573)))?;
    lines.set(5, s("---------- --------- --------- --------- ------------ ------------- -----------"))?;
    let mut line_no = 5;
    let mut field;
    for rec_idx in 1..=1000 {
        let rec_text = g.phys_commod_rec[rec_idx].clone();
        if rec_text.is_empty() {
            continue;
        }
        if pfv(&rec_text, 2) != cast::<Ext, _>(ent_id) {
            continue;
        }
        line_no = line_no + 1;
        let commod_name = trim(&pf(&rec_text, 1));
        line_text = commod_name.clone() + "        ";
        line_text = mid(line_text.as_bytes(), 1, 11);
        let units = to_long(pfv(&rec_text, 3));
        let total_cost = pfv(&rec_text, 4);
        // GOSUB ListPhysCommod_UnitFactor (0x65d6e7): a DOUBLE local loaded from SINGLE constants
        let unit_factor: Ext = if commod_name == "GOLD" || commod_name == "BITCOIN" {
            cast::<Ext, _>(0.001f32)
        } else if commod_name == "OIL" || commod_name == "SILVER" || commod_name == "ETHEREUM" {
            cast::<Ext, _>(0.01f32)
        } else {
            cast::<Ext, _>(0.1f32)
        };
        // GOSUB ListPhysCommod_CalcUnitCost (0x65d7d1): unitCost AS CURRENCYX
        let mut unit_cost = Cux::default();
        unit_cost.set(total_cost / (cast::<Ext, _>(units) * unit_factor));
        // GOSUB ListPhysCommod_ValueLine (0x65d7f6)
        let cur_value: i64 = if 0 < instr(1, __content.bytes(14464), commod_name.as_bytes()) {
            to_quad(get_phys_commod_value(g, ent_id, &commod_name))
        } else {
            ui.msgbox(
                &(s(__content.string(8574)) + &commod_name),
                0,
                &PbStr::new(),
            );
            // The fallback prices by name can't apply: every listed name is a substring of the string
            // above, so the name test ends in the second message and the routine's exit.
            ui.msgbox(
                &(s(__content.string(8575)) + &commod_name),
                0,
                &PbStr::new(),
            );
            return Ok(selected_rec);
        };
        let units_str = str15(cast::<Ext, _>(units));
        field = space(8) + &units_str + " ";
        line_text = line_text + &right(field.as_bytes(), 10);
        field = space(10) + &fmt_trim(unit_cost.get() / g.exchange_rate, fmt_decimal) + " ";
        if Ext::from_f64(1000000.0) <= val(fmt(unit_cost.get() / g.exchange_rate, fmt_decimal).as_bytes()) {
            field = space(10) + &fmt_trim(unit_cost.get() / g.exchange_rate, fmt_int) + " ";
        }
        line_text = line_text + &right(field.as_bytes(), 10);
        g.stock_price_str = fmt_trim(Ext::from_f64(100.0) * cast::<Ext, _>(g.oil_price), fmt_decimal);
        if commod_name == "GOLD" {
            g.stock_price_str = fmt_trim(cast::<Ext, _>(g.gold_price), fmt_decimal);
        }
        if commod_name == "SILVER" {
            g.stock_price_str = fmt_trim(cast::<Ext, _>(g.silver_price), fmt_decimal);
        }
        if commod_name == "WHEAT" {
            g.stock_price_str = fmt_trim(cast::<Ext, _>(g.wheat_price), fmt_decimal);
        }
        if commod_name == "CORN" {
            g.stock_price_str = fmt_trim(cast::<Ext, _>(g.corn_price), fmt_decimal);
        }
        if commod_name == "BITCOIN" {
            g.stock_price_str = fmt_trim(cast::<Ext, _>(g.bitcoin_price), fmt_decimal);
            if Ext::from_f64(1000000.0) <= val(g.stock_price_str.as_bytes()) {
                g.stock_price_str = fmt_trim(cast::<Ext, _>(g.bitcoin_price), fmt_int);
            }
        }
        if commod_name == "ETHEREUM" {
            g.stock_price_str = fmt_trim(cast::<Ext, _>(g.ethereum_price), fmt_decimal);
        }
        field = space(10) + &g.stock_price_str + " ";
        line_text = line_text + &right(field.as_bytes(), 10);
        let cost_rounded = to_quad(total_cost);
        sum_cost = to_quad(cast::<Ext, _>(sum_cost + cost_rounded));
        field = space(13) + &str15(cast::<Ext, _>(cost_rounded)) + " ";
        line_text = line_text + &right(field.as_bytes(), 13);
        field = space(14) + &str15(cast::<Ext, _>(cur_value)) + " ";
        line_text = line_text + &right(field.as_bytes(), 14);
        let gain = to_quad(cast::<Ext, _>(cur_value - cost_rounded));
        sum_gain = to_quad(cast::<Ext, _>(sum_gain + gain));
        field = space(11) + &str15(cast::<Ext, _>(gain)) + " ";
        line_text = line_text + &right(field.as_bytes(), 12);
        lines.set(line_no, line_text.clone())?;
        let mut link_tag = s("000") + &str_trim(cast::<Ext, _>(rec_idx));
        link_tag = s("@C") + &right(link_tag.as_bytes(), 4);
        if can_select == 1 {
            lines.set(line_no, lines.get(line_no)? + &space(50) + &link_tag)?;
        }
    }
    line_no = line_no + 1;
    lines.set(line_no, lines.get(5)?)?;
    line_no = line_no + 1;
    line_text = space(31) + "TOTALS:" + &space(13) + &str15(cast::<Ext, _>(sum_cost)) + " ";
    line_text = right(line_text.as_bytes(), 54);
    let sum_value = to_quad(total_value);
    field = space(15) + &str15(cast::<Ext, _>(sum_value)) + " ";
    line_text = line_text + &right(field.as_bytes(), 14);
    field = space(13) + &str15(cast::<Ext, _>(sum_gain)) + " ";
    line_text = line_text + &right(field.as_bytes(), 12);
    lines.set(line_no, line_text)?;
    line_no = line_no + 1;
    lines.set(line_no, s("                                         ============ ============= ==========="))?;
    line_text = currency_words(g);
    line_no = line_no + 3;
    lines.set(line_no, s(__content.string(8576)))?;
    line_no = line_no + 1;
    lines.set(
        line_no,
        s(__content.string(8577)) + &g.million_prefix_lc + __content.string(8089) + &line_text + __content.string(8578),
    )?;
    if line_text.len() < 9 {
        lines.set(line_no, lines.get(line_no)? + __content.string(8579))?;
    } else {
        line_no = line_no + 1;
        lines.set(line_no, s(__content.string(8580)))?;
    }
    line_no = line_no + 2;
    lines.set(
        line_no,
        s(__content.string(8581)) + "\"" + "unit" + "\"" + __content.string(8582),
    )?;
    line_no = line_no + 1;
    lines.set(line_no, s(__content.string(8583)))?;
    lines.set(line_no + 1, s(__content.string(8584)))?;
    line_no = line_no + 3;
    if can_select == 1 && !(g.game_end_flag == "Y") {
        lines.set(line_no, s(__content.string(8585)))?;
    } else {
        line_no = line_no - 2;
    }
    lines.set(line_no + 4, s("@END"))?;
    let line_count = line_no + 6;
    lines.copy_to_text_lines(g, line_count)?;
    g.text_box_title = s(__content.string(8586)) + &g.ent_name[ent_id];
    if can_select == 1 {
        g.entity_pick_mode = s("GetCorp");
        g.list_item_noun = s(__content.string(8587));
    }
    g.text_box_mode = s("74");
    g.target_corp = 0;
    g.text_has_links = 1;
    show_text_report(g, ui, rng);
    if 0 < g.target_corp
        && cast::<Ext, _>(g.target_corp) <= val(g.phys_commod_rec[0].as_bytes())
        && pfv(&g.phys_commod_rec[g.target_corp], 2) == cast::<Ext, _>(g.active_entity)
    {
        if 0 < g.target_corp {
            selected_rec = g.target_corp;
        }
        g.target_corp = 0;
        return Ok(selected_rec);
    }
    g.target_corp = 0;
    selected_rec = 0;
    Ok(selected_rec)
}
