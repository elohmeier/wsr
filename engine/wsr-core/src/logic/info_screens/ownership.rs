//! The stock holdings / ownership diagram (`ShowOwnershipDiagram` 0x54eb71): the selected entity in the
//! middle, its major shareholders above (up to 9, largest first) and the stocks it owns below (up to 15).
//! Clicking a shareholder or holding box selects that company (or, for the current player, makes the
//! player the acting entity) and redraws the diagram for it.
//!
//! The diagram's content is computed exactly by [`build_ownership_diagram`] as an [`OwnershipDiagram`]
//! (every control of the original's DDT dialog with id, text and position), which a UI can draw directly.
//! [`show_ownership_diagram`] presents it through [`Ui::ownership_diagram`]. Headless hosts use the
//! linked-report fallback; browser hosts render the boxes and connecting lines.

// Keep the original's statement shapes.
#![allow(clippy::assign_op_pattern, clippy::collapsible_if, clippy::manual_range_contains)]
use crate::pb::real::{Ext, cast};


use crate::logic::control::update_corp_control;
use crate::logic::economy::close_all_dialogs;
use crate::logic::market::select_target_corp;
use crate::logic::newgame::LocalTime;
use crate::logic::reports_lists::pb_array_sort;
use crate::logic::sim_entity::quarter_start::pack_holding_arrays;
use crate::logic::trading::select::{lookup_symbol, select_acting_entity};
use crate::logic::turn::ticker_sim_steps;
use crate::pb::numeric::str_num;
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{instr, len, ltrim, mcase, mid, parse_any, replace, right, rtrim, ucase, CaseMode};
use crate::state::Globals;
use crate::ui::{ReportAction, Ui};

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// `STR$(x)` of an INTEGER/LONG.
fn str_s(x: i32) -> PbStr {
    PbStr::from(str_num(cast::<Ext, _>(x), 15).as_str())
}

/// Kind of a control of the diagram dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramControlKind {
    /// A text box (`CONTROL ADD LABEL`).
    Label,
    /// A connecting line (`CONTROL ADD LINE`, no text).
    Line,
    /// The CLOSE button (id 1).
    Button,
}

/// One control of the diagram dialog, in dialog units, as the original adds it.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramControl {
    pub id: i32,
    pub kind: DiagramControlKind,
    pub text: PbStr,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// The label has a click callback (`OwnershipDiagram_OnClickBox<id>`): a shareholder or holding box.
    pub clickable: bool,
}

/// The computed diagram of [`build_ownership_diagram`].
#[derive(Clone, Debug, PartialEq)]
pub struct OwnershipDiagram {
    /// The dialog caption: `" [runtime resource wording] <name> (<symbol>)"`.
    pub title: PbStr,
    /// The entity shown (`SelEntity`).
    pub entity: i32,
    /// Dialog size (379 x `254 - heightAdj`).
    pub width: i32,
    pub height: i32,
    /// Shareholders found (`holderCount`, before the box limit of 9), as `(holder id, pct)`, in box order
    /// (sorted by pct, descending, for more than 2 holders).
    pub holders: Vec<(i32, i32)>,
    /// Number of stock holdings of the entity (`HoldStockId(0, entity)` after `PackHoldingArrays`).
    pub holding_count: i32,
    /// Every control in creation order.
    pub controls: Vec<DiagramControl>,
}

impl OwnershipDiagram {
    /// The control with id `id` (the last one added, as `GetDlgItem` would find the first; ids are unique
    /// in practice).
    pub fn control(&self, id: i32) -> Option<&DiagramControl> {
        self.controls.iter().find(|c| c.id == id && c.kind == DiagramControlKind::Label)
    }
}

/// The shareholder boxes (control id, x offset added to `xShift` or absolute, y) in the original's order.
/// `(id, x, shifted_x, y, shifted_y)`.
const HOLDER_BOXES: [(i32, i32, bool, i32, bool); 9] = [
    (450, 80, true, 74, true),
    (448, 230, true, 74, true),
    (446, 5, true, 74, true),
    (444, 305, false, 74, true),
    (442, 155, false, 54, false),
    (440, 80, false, 54, false),
    (438, 230, false, 54, false),
    (436, 5, false, 54, false),
    (434, 305, false, 54, false),
];

/// The holding boxes 2..15 (control id, x, x shifted by `xShift`, y before `- yShift`).
const HOLDING_BOXES: [(i32, i32, bool, i32); 14] = [
    (430, 80, true, 150),
    (428, 230, true, 150),
    (426, 5, true, 150),
    (424, 305, false, 150),
    (422, 155, false, 170),
    (420, 80, false, 170),
    (418, 230, false, 170),
    (416, 5, false, 170),
    (414, 305, false, 170),
    (403, 155, false, 190),
    (402, 80, false, 190),
    (408, 230, false, 190),
    (406, 5, false, 190),
    (404, 305, false, 190),
];

/// Port of OwnershipDiagram_FormatLabel (0x55075d), GOSUB of ShowOwnershipDiagram.
/// A box label: the symbol (longer than 11 characters: cut to 11, or 10 at 100%, and MCASE$d) with any
/// `":"` replaced by a blank, then `": " + STR$(pct) + "%"`.
pub fn format_label(sym_text: &PbStr, pct: i32) -> PbStr {
    let mut sym = sym_text.clone();
    let max_len = 11;
    if max_len < len(sym.as_bytes()) {
        sym = mid(sym.as_bytes(), 1, 11);
        if pct == 100 {
            sym = mid(sym.as_bytes(), 1, 10);
        }
        sym = mcase(sym.as_bytes(), CaseMode::Ansi);
    }
    if 0 < instr(1, sym.as_bytes(), b":") {
        sym = replace(sym.as_bytes(), b":", b" ");
    }
    sym + ": " + &str_s(pct) + "%"
}

/// The diagram part of Port of ShowOwnershipDiagram (0x54eb71): one pass of its loop up to `DIALOG SHOW
/// MODAL`. Sets `DlgOpen_HoldingsDiagram`; defaults `SelEntity` to the current player; sets `SelCorpName`,
/// `PublicPct = 100` and runs `UpdateCorpControl(SelEntity)`; collects the shareholders (every holder
/// 1..1600 with a slot 1..15 holding `SelEntity`, before `PackHoldingArrays`), sorts more than 2 of them
/// by percentage (`ARRAY SORT ... DESCEND`) and builds every control of the dialog.
///
/// `DIM holderIds(100), holderPcts(100)`: more than 100 holders of one stock would be a subscript error in
/// the original (impossible with 15 holding slots per entity in practice); the port keeps them all.
pub fn build_ownership_diagram(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> OwnershipDiagram {
    let __content = crate::content::current();
    // (UI) IF DlgOpen_HoldingsDiagram = 1 THEN DIALOG END DlgOwnership
    g.dlg_open_holdings_diagram = 1;
    let mut holder_ids: Vec<i32> = vec![0];
    let mut holder_pcts: Vec<i32> = vec![0];
    let mut holder_count = 0;
    let mut y_shift = 0;
    let mut height_adj = 0;
    let mut line_adj = 0;
    if g.sel_entity < 1 {
        g.sel_entity = g.cur_player;
        g.cur_player_name = g.ent_name[g.cur_player].clone();
        g.sel_corp_name = g.cur_player_name.clone();
    }
    let entity_id = g.sel_entity;
    g.sel_corp_name = g.ent_name[g.sel_entity].clone();
    g.public_pct = 100;
    update_corp_control(g, ui, rng, entity_id);
    for holder_id in 1..=1600 {
        for slot in 1..=15 {
            if cast::<i32, _>(g.hold_stock_id[(slot, holder_id)]) == entity_id {
                holder_count = holder_count + 1;
                holder_ids.push(cast::<i32, _>(holder_id));
                holder_pcts.push(cast::<i32, _>(g.hold_stock_pct[(slot, holder_id)]));
            }
        }
    }
    if holder_count < 5 {
        y_shift = 20;
    }
    pack_holding_arrays(g, ui);
    let holding_count = cast::<i32, _>(g.hold_stock_id[(0, entity_id)]);
    if holding_count < 11 {
        height_adj = 20;
    }
    if holding_count < 6 {
        height_adj = 40;
    }
    if holder_count < 5 {
        height_adj = height_adj + 20;
    }
    let mut title = s(__content.string(6584)) + &g.sel_corp_name;
    if 10 < g.sel_entity {
        title = title + " (" + &g.ent_symbol[g.sel_entity] + ")";
    }
    // (UI) AdjustDialogFrameSize; fonts
    let mut controls = Vec::new();
    let mut add = |id: i32, kind: DiagramControlKind, text: PbStr, x: i32, y: i32, w: i32, h: i32, clickable: bool| {
        controls.push(DiagramControl { id, kind, text, x, y, w, h, clickable });
    };
    use DiagramControlKind::{Button, Label, Line};
    add(454, Label, g.sel_corp_name.clone(), 109, 113 - y_shift, 162, 17, false);
    if holder_count == 1 || holder_count == 3 {
        line_adj = 10;
        add(397, Line, PbStr::new(), 153, 98 - y_shift, 77, 2, false);
        add(398, Line, PbStr::new(), 153, 89 - y_shift, 2, 10, false);
        add(399, Line, PbStr::new(), 228, 89 - y_shift, 2, 10, false);
    }
    add(394, Line, PbStr::new(), 189, (line_adj + 90) - y_shift, 2, 24 - line_adj, false);
    line_adj = 0;
    if holding_count == 2 || holding_count == 4 {
        line_adj = 9;
        add(400, Line, PbStr::new(), 153, 140 - y_shift, 77, 2, false);
        add(401, Line, PbStr::new(), 153, 140 - y_shift, 2, 10, false);
        add(402, Line, PbStr::new(), 230, 140 - y_shift, 2, 10, false);
    }
    add(396, Line, PbStr::new(), 189, 130 - y_shift, 2, 20 - line_adj, false);
    add(1, Button, s("CL&OSE"), 309, 114 - y_shift, 53, 15, false);
    add(412, Label, s(__content.string(6585)), 133, 17, 115, 17, false);
    add(410, Label, s(__content.string(6586)), 151, 223 - height_adj, 81, 17, false);
    let mut label_text = s("N/A");
    if 2 < holder_count {
        // ARRAY SORT holderPcts(1) FOR holderCount, TAGARRAY holderIds(), DESCEND
        let n = cast::<usize, _>(holder_count);
        pb_array_sort(&mut holder_pcts[1..1 + n], Some(&mut holder_ids[1..1 + n]), true, &|a: &i32, b: &i32| a < b);
    }
    let mut x_shift = 0;
    if holder_count == 1 || holder_count == 3 {
        x_shift = 38;
    }
    if 10 < entity_id {
        label_text = s(__content.string(6587)) + &str_s(cast::<i32, _>(g.public_pct)) + "%";
    }
    add(452, Label, label_text, x_shift + 155, 74 - y_shift, 69, 15, false);
    if 10 < entity_id && 0 < holder_count {
        for (k, &(id, x, sx, y, sy)) in HOLDER_BOXES.iter().enumerate() {
            if (cast::<i32, _>(k)) < holder_count {
                let sym_text = g.ent_symbol[holder_ids[k + 1]].clone();
                let text = format_label(&sym_text, holder_pcts[k + 1]);
                add(id, Label, text, if sx { x_shift + x } else { x }, if sy { y - y_shift } else { y }, 69, 15, true);
            }
        }
    }
    x_shift = 0;
    if holding_count == 2 || holding_count == 4 {
        x_shift = 38;
    }
    let mut label_text = s("NONE");
    if 0 < holding_count {
        label_text = g.ent_symbol[cast::<i32, _>(g.hold_stock_id[(1, entity_id)])].clone()
            + ": "
            + &str_s(cast::<i32, _>(g.hold_stock_pct[(1, entity_id)]))
            + "%";
    }
    add(432, Label, label_text, x_shift + 155, 150 - y_shift, 69, 15, true);
    for (k, &(id, x, sx, y)) in HOLDING_BOXES.iter().enumerate() {
        let slot = cast::<i32, _>(k) + 2;
        // the last box needs exactly 15 holdings (`holdingCount = 15`), the others `holdingCount > slot - 1`
        let shown = if slot == 15 { holding_count == 15 } else { slot - 1 < holding_count };
        if shown {
            let sym_text = g.ent_symbol[cast::<i32, _>(g.hold_stock_id[(slot, entity_id)])].clone();
            let pct = cast::<i32, _>(g.hold_stock_pct[(slot, entity_id)]);
            let text = format_label(&sym_text, pct);
            add(id, Label, text, if sx { x_shift + x } else { x }, y - y_shift, 69, 15, true);
        }
    }
    let holders = (1..=cast::<usize, _>(holder_count)).map(|k| (holder_ids[k], holder_pcts[k])).collect();
    OwnershipDiagram { title, entity: entity_id, width: 379, height: 254 - height_adj, holders, holding_count, controls }
}

/// Lines of the report presentation of `d`: the shareholder boxes (with the public float), the entity,
/// and the holding boxes; each clickable box carries `@C<control id>` (4 digits).
pub fn diagram_report_lines(d: &OwnershipDiagram) -> Vec<PbStr> {
    let __content = crate::content::current();
    let link = |c: &DiagramControl| -> PbStr {
        let mut t = s("  ") + &c.text;
        if c.clickable {
            t = t + &PbStr::from(" ".repeat(70).as_str()) + "@C" + &right(format!("000{}", c.id).as_bytes(), 4);
        }
        t
    };
    let mut lines = vec![s(__content.string(6585))];
    let holder_ids = [450, 448, 446, 444, 442, 440, 438, 436, 434, 452];
    for c in d.controls.iter().filter(|c| c.kind == DiagramControlKind::Label && holder_ids.contains(&c.id)) {
        lines.push(link(c));
    }
    lines.push(PbStr::new());
    if let Some(c) = d.control(454) {
        lines.push(c.text.clone());
    }
    lines.push(PbStr::new());
    lines.push(s(__content.string(6586)));
    let holding_ids = [432, 430, 428, 426, 424, 422, 420, 418, 416, 414, 403, 402, 408, 406, 404];
    for c in d.controls.iter().filter(|c| c.kind == DiagramControlKind::Label && holding_ids.contains(&c.id)) {
        lines.push(link(c));
    }
    lines
}

/// Port of OwnershipDiagram_SelectByLabel (0x552a65).
/// Selects the company named by a clicked box label (`"SYMBOL: n%"`; for the current player the player's
/// name): the current player (by name, or the first 9 characters of it) → `SelectActingEntity(1)`;
/// another active entity found by symbol (players 1..5 and companies 11..1600) → `LookupSymbol`,
/// `SelectTargetCorp`, remembering the previous selection in `LastSelEntity`. Returns 1 when the selection
/// changed (the diagram is then redrawn), else 0 (`"NONE"`/`"N/A"`, unknown or inactive symbol, other
/// player, already selected).
pub fn select_by_label(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, label_text: &PbStr) -> i32 {
    let prev_sel = g.sel_entity;
    let mut found_id = 0;
    let mut player_name: PbStr;
    let mut label = ucase(label_text.as_bytes(), CaseMode::Ansi);
    label = parse_any(label.as_bytes(), b":", 1);
    label = rtrim(ltrim(label.as_bytes()).as_bytes());
    if label == "NONE" || label == "N/A" {
        return 0;
    }
    g.target_corp = 0;
    if 4 < len(label.as_bytes()) {
        player_name = ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi);
        if label == player_name {
            found_id = g.cur_player;
        }
        if 9 <= len(label.as_bytes()) {
            label = mid(label.as_bytes(), 1, 9);
            if 9 < len(player_name.as_bytes()) {
                player_name = mid(player_name.as_bytes(), 1, 9);
            }
            if label == player_name {
                found_id = g.cur_player;
            }
        }
    }
    if found_id == 0 {
        let mut entity_id = 1;
        while entity_id < 1601 {
            let mut check_symbol = true;
            if g.cur_player == entity_id {
                player_name = ucase(g.cur_player_name.as_bytes(), CaseMode::Ansi);
                if label == player_name {
                    found_id = entity_id;
                    entity_id = 1600;
                    check_symbol = false;
                }
            }
            if check_symbol && (entity_id < 6 || 10 < entity_id) {
                if g.ent_symbol[entity_id] == label {
                    if g.entity_status[entity_id] != 1 {
                        return 0;
                    }
                    found_id = entity_id;
                    entity_id = 1600;
                }
            }
            entity_id = entity_id + 1;
        }
    }
    if found_id == 0 {
        return 0;
    }
    if g.num_players < found_id {
        let symbol_text = g.ent_symbol[found_id].clone();
        g.tmp_symbol = PbStr::new();
        lookup_symbol(g, ui, rng, &symbol_text);
        // (UI) CONTROL SET TEXT hDlgMain, 202, TmpSymbol
        if g.tmp_symbol.is_empty() || g.sel_entity == g.target_corp {
            0
        } else {
            select_target_corp(g, ui, rng, now.time_secs());
            if prev_sel != g.sel_entity && 10 < prev_sel {
                g.last_sel_entity = prev_sel;
            }
            1
        }
    } else if *label_text == g.ent_symbol[g.cur_player] || g.cur_player == found_id {
        select_acting_entity(g, ui, rng, 1, now);
        1
    } else {
        0
    }
}

/// Port of the click callbacks OwnershipDiagram_OnClickBox450 (0x5518a5), OwnershipDiagram_OnClickBox448
/// (0x551962), OwnershipDiagram_OnClickBox446 (0x551a1f), OwnershipDiagram_OnClickBox444 (0x551adc),
/// OwnershipDiagram_OnClickBox442 (0x551b99), OwnershipDiagram_OnClickBox440 (0x551c56),
/// OwnershipDiagram_OnClickBox438 (0x551d13), OwnershipDiagram_OnClickBox436 (0x551dd0),
/// OwnershipDiagram_OnClickBox434 (0x551e8d), OwnershipDiagram_OnClickBox432 (0x551f4a),
/// OwnershipDiagram_OnClickBox430 (0x552007), OwnershipDiagram_OnClickBox428 (0x5520c4),
/// OwnershipDiagram_OnClickBox426 (0x552181), OwnershipDiagram_OnClickBox424 (0x55223e),
/// OwnershipDiagram_OnClickBox422 (0x5522fb), OwnershipDiagram_OnClickBox420 (0x5523b8),
/// OwnershipDiagram_OnClickBox418 (0x552475), OwnershipDiagram_OnClickBox416 (0x552532),
/// OwnershipDiagram_OnClickBox414 (0x5525ef), OwnershipDiagram_OnClickBox403 (0x5526ac),
/// OwnershipDiagram_OnClickBox402 (0x552769), OwnershipDiagram_OnClickBox408 (0x552826),
/// OwnershipDiagram_OnClickBox406 (0x5528e3), OwnershipDiagram_OnClickBox404 (0x5529a0).
/// All 24 are the same code for their own control: a label containing `":"` is passed to
/// [`select_by_label`]; returns 1 when the dialog ends with result 1 (redraw), else 0 (stays open).
pub fn on_click_box(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime, label_text: &PbStr) -> i32 {
    if instr(1, label_text.as_bytes(), b":") != 0 && select_by_label(g, ui, rng, now, label_text) != 0 {
        // DIALOG END hDlg, 1
        return 1;
    }
    0
}

/// Port of OwnershipDiagram_OnClose (0x551839): the CLOSE button ends the dialog,
/// `DlgOpen_HoldingsDiagram = 0`, `CloseAllDialogs(2)`.
pub fn on_close(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // DIALOG END hDlg, 0
    g.dlg_open_holdings_diagram = 0;
    close_all_dialogs(g, ui, rng, 2);
}

/// Window messages of DlgProcOwnershipDiagram.
pub const WM_DESTROY: i32 = 2;

/// Port of DlgProcOwnershipDiagram (0x5508a6): `WM_DESTROY` clears `DlgOpen_HoldingsDiagram`; the rest is
/// colouring (see [`box_colors`]) and the CLOSE button's tooltip.
pub fn dlg_proc(g: &mut Globals, msg: i32) {
    if msg == WM_DESTROY {
        g.dlg_open_holdings_diagram = 0;
    }
}

/// The `WM_CTLCOLOR*` part of DlgProcOwnershipDiagram (0x5508a6): (text colour, background colour) as
/// `ColorTable()` indices of control `id` (the dialog background: text 15 on 0).
pub fn box_colors(id: i32) -> Option<(usize, usize)> {
    match id {
        402 | 403 | 404 | 406 | 408 | 410 | 414 | 416 | 418 | 420 | 422 | 424 | 426 | 428 | 430 | 432 => Some((0, 11)),
        412 | 434 | 436 | 438 | 440 | 442 | 444 | 446 | 448 | 450 | 452 => Some((0, 18)),
        454 => Some((15, 12)),
        _ => None,
    }
}

/// Port of OwnershipDiagram_ExitSub (0x55088a): the routine's empty exit block.
pub fn exit_sub() {}

/// Port of ShowOwnershipDiagram (0x54eb71).
/// Shows the ownership diagram of `SelEntity` ([`build_ownership_diagram`]) as a report
/// ([`diagram_report_lines`]); a clicked box link runs [`on_click_box`]: when it changes the selection the
/// diagram is redrawn for the new entity, after closing the transaction menu (`ActionMenuOpen = 0`) and,
/// unless the clock runs, one suppressed ticker step (`ActionCode = "SUPP"`, `ClockState = ""`,
/// `TickerOn = 0`, `TickerSimSteps(1)`, `ActionCode = ""`). OK = the CLOSE button ([`on_close`]); a window
/// closed otherwise only clears `DlgOpen_HoldingsDiagram` (`WM_DESTROY`).
pub fn show_ownership_diagram(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, now: &LocalTime) {
    loop {
        let d = build_ownership_diagram(g, ui, rng);
        let reopen_flag = loop {
            match ui.ownership_diagram(&d) {
                ReportAction::Link(code) => {
                    let id = cast::<i32, _>(crate::pb::numeric::val(mid(code.as_bytes(), 3, 4).as_bytes()));
                    let Some(c) = d.control(id).filter(|c| c.clickable) else { continue };
                    let text = c.text.clone();
                    if on_click_box(g, ui, rng, now, &text) == 1 {
                        dlg_proc(g, WM_DESTROY);
                        break 1;
                    }
                }
                ReportAction::Ok => {
                    on_close(g, ui, rng);
                    break 0;
                }
                ReportAction::No | ReportAction::Closed | ReportAction::Chart | ReportAction::Rating => {
                    dlg_proc(g, WM_DESTROY);
                    break 0;
                }
            }
        };
        if reopen_flag == 1 && 0 < g.action_menu_open {
            // (UI) DIALOG END hDlgTransMenu, 0
            g.action_menu_open = 0;
        }
        if reopen_flag == 1 {
            if !(g.clock_state == "RUN") {
                g.action_code = s("SUPP");
                g.clock_state = PbStr::new();
                g.ticker_on = 0;
                ticker_sim_steps(g, ui, rng, 1, now);
                g.action_code = PbStr::new();
            }
        }
        if reopen_flag != 1 {
            break;
        }
    }
    exit_sub();
}
