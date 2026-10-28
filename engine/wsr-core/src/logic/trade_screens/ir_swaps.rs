//! The interest-rate swap dialog (" [runtime resource wording] …",
//! `ShowIRSwapDialog` 0x63b281) with its control callbacks, the swap creation (`IRSwap_CreateSwap`
//! 0x63efd6 with its GOSUBs and `CheckSwapPositionLimits` 0x63d93c) and the swap list / details screens
//! (`PickSwapFromList` 0x6437ed, `ShowSwapDetails` 0x644b6e).
//!
//! The dialog's state lives in the globals `SwapRateType` ("P"/"L"/"S", option buttons 980/982/984) and
//! `SwapPosition` ("LONG"/"SHORT", 986/988) plus the texts of its edit controls, modelled as
//! [`IrSwapForm`] (a UI can fill it and call the control callbacks directly). [`show_ir_swap_dialog`]
//! runs the dialog through [`Ui::choose`] (one option per control, see [`IrSwapControl`]) and
//! [`Ui::inputbox`] for the edit fields.

// Statements keep the original BASIC shape on purpose.
#![allow(clippy::assign_op_pattern, clippy::collapsible_else_if, clippy::collapsible_if)]
use crate::pb::real::{Ext, cast};


#[path = "ir_swaps/create.rs"]
mod create;
#[path = "ir_swaps/list.rs"]
mod list;

pub use create::{check_swap_position_limits, ir_swap_create_swap};
pub use list::{pick_swap_from_list, show_swap_details};

use crate::logic::finance::calc_entity_financials;
use crate::logic::restructure::dlg_util::{
    adjust_dialog_frame_size, get_notify_hwnd_from, get_tooltip_ctrl_id, is_tab_sel_change, is_tooltip_need_text,
    set_tooltip_text, Nmhdr,
};
use crate::logic::swaps::{count_entity_swaps, sum_swap_notional};
use crate::logic::trading::prelude::*;
use crate::pb::numeric::Cur;

/// The edit controls of the swap dialog (texts as typed; the original reads them with
/// `CONTROL GET TEXT` when OFFER is clicked and writes corrections back).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IrSwapForm {
    /// 989: notional amount in millions (e.g. `"12,345"` or `"Zero"`).
    pub notional: PbStr,
    /// 990: beginning quarter.
    pub start_qtr: PbStr,
    /// 991: beginning year.
    pub start_year: PbStr,
    /// 992: final quarter.
    pub end_qtr: PbStr,
    /// 993: final year.
    pub end_year: PbStr,
    /// 994 (read-only): the current fixed rate of the selected rate type, `FORMAT$(rate, "* ##.00")`.
    pub current_rate: PbStr,
    /// The dialog is still open (cleared by `DIALOG END DlgIRSwap`).
    pub open: bool,
}

/// The controls of the swap dialog a user can operate, in the order of the options [`show_ir_swap_dialog`]
/// offers through [`Ui::choose`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrSwapControl {
    /// 980 " [runtime resource wording] ".
    LongBondRate,
    /// 982 " [runtime resource wording] ".
    ShortBondRate,
    /// 984 " [runtime resource wording] ".
    PrimeRate,
    /// 986 " [runtime resource wording] ".
    Long,
    /// 988 " [runtime resource wording] ".
    Short,
    /// 989 notional amount (edit).
    Notional,
    /// 990 beginning quarter (edit).
    StartQtr,
    /// 991 beginning year (edit).
    StartYear,
    /// 992 final quarter (edit).
    EndQtr,
    /// 993 final year (edit).
    EndYear,
    /// 1 "OFFER".
    Offer,
    /// 958 "SWAP INFO".
    Info,
    /// 959 " [runtime resource wording] ".
    ViewSwaps,
    /// 960 "TERMINATE A CONTRACT".
    Terminate,
    /// 998 "EXIT".
    Exit,
}

impl IrSwapControl {
    /// Every control, in [`Ui::choose`] option order.
    pub const ALL: [IrSwapControl; 15] = [
        Self::LongBondRate,
        Self::ShortBondRate,
        Self::PrimeRate,
        Self::Long,
        Self::Short,
        Self::Notional,
        Self::StartQtr,
        Self::StartYear,
        Self::EndQtr,
        Self::EndYear,
        Self::Offer,
        Self::Info,
        Self::ViewSwaps,
        Self::Terminate,
        Self::Exit,
    ];

    /// The control id of the original dialog.
    pub fn id(self) -> i32 {
        match self {
            Self::LongBondRate => 980,
            Self::ShortBondRate => 982,
            Self::PrimeRate => 984,
            Self::Long => 986,
            Self::Short => 988,
            Self::Notional => 989,
            Self::StartQtr => 990,
            Self::StartYear => 991,
            Self::EndQtr => 992,
            Self::EndYear => 993,
            Self::Offer => 1,
            Self::Info => 958,
            Self::ViewSwaps => 959,
            Self::Terminate => 960,
            Self::Exit => 998,
        }
    }

    /// The control's caption (labels of the edit fields).
    pub fn label(self, g: &Globals) -> PbStr {
    let __content = crate::content::current();
        match self {
            Self::LongBondRate => s(__content.string(11410)),
            Self::ShortBondRate => s(__content.string(11411)),
            Self::PrimeRate => s(__content.string(11412)),
            Self::Long => s(__content.string(11413)),
            Self::Short => s(__content.string(11414)),
            Self::Notional => {
                s(__content.string(11415)) + &g.million_prefix_lc + __content.string(11416)
            }
            Self::StartQtr => s(__content.string(11417)),
            Self::StartYear => s("Beginning Year"),
            Self::EndQtr => s(__content.string(11418)),
            Self::EndYear => s("Final Year"),
            Self::Offer => s("OFFER"),
            Self::Info => s(__content.string(11419)),
            Self::ViewSwaps => s(__content.string(11420)),
            Self::Terminate => s(__content.string(11421)),
            Self::Exit => s("EXIT"),
        }
    }

    /// The edit field of `form` this control is (None for buttons).
    pub fn field(self, form: &mut IrSwapForm) -> Option<&mut PbStr> {
        match self {
            Self::Notional => Some(&mut form.notional),
            Self::StartQtr => Some(&mut form.start_qtr),
            Self::StartYear => Some(&mut form.start_year),
            Self::EndQtr => Some(&mut form.end_qtr),
            Self::EndYear => Some(&mut form.end_year),
            _ => None,
        }
    }
}

/// `FORMAT$(x, "* ##.00")` of a rate shown in control 994.
fn rate_text(x: Ext) -> PbStr {
    fmt(x, "* ##.00")
}

/// The title of the swap dialog.
pub fn ir_swap_dialog_title(g: &Globals) -> PbStr {
    let __content = crate::content::current();
    s(__content.string(11422)) + &g.active_entity_name
}

/// Port of ShowIRSwapDialog (0x63b281).
/// The modal swap dialog of the acting entity: [`ir_swap_dialog_init`] builds it, then the controls are
/// operated through [`Ui::choose`] (options = [`IrSwapControl::ALL`]; an edit field asks for its new
/// text with [`Ui::inputbox`]) until a callback ends the dialog or the user closes it (`None`, WM_CLOSE).
/// `DlgOpen_SwapDlg` is 1 while it is open.
pub fn show_ir_swap_dialog(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, _h_parent: i32) {
    let mut form = ir_swap_dialog_init(g, ui, rng);
    g.dlg_open_swap_dlg = 1;
    let title = ir_swap_dialog_title(g);
    while form.open {
        use crate::ui::{Form, FormField};
        let edits = [IrSwapControl::Notional, IrSwapControl::StartQtr, IrSwapControl::StartYear,
            IrSwapControl::EndQtr, IrSwapControl::EndYear];
        let mut fields = vec![
            FormField { id: 980, label: s("Rate type"), value: g.swap_rate_type.clone(),
                choices: vec![(s("L"), IrSwapControl::LongBondRate.label(g)),
                    (s("S"), IrSwapControl::ShortBondRate.label(g)), (s("P"), IrSwapControl::PrimeRate.label(g))],
                refresh_on_change: true, ..Default::default() },
            FormField { id: 986, label: s("Position"), value: g.swap_position.clone(),
                choices: vec![(s(""), s("Choose a position")), (s("LONG"), IrSwapControl::Long.label(g)),
                    (s("SHORT"), IrSwapControl::Short.label(g))], ..Default::default() },
        ];
        for ctrl in edits {
            let value = ctrl.field(&mut form).unwrap().clone();
            fields.push(FormField { id: ctrl.id(), label: ctrl.label(g), value, numeric: true, ..Default::default() });
        }
        let content = crate::content::current();
        let spec = Form {
            title: title.clone(), help_topic: Some(s("interest rate swaps")), help_button: Some(958),
            prompt: vec![s(content.string(11423)), s(content.string(11425)) + &form.current_rate,
                s(content.string(11428))], fields,
            buttons: [IrSwapControl::Offer, IrSwapControl::Info, IrSwapControl::ViewSwaps,
                IrSwapControl::Terminate, IrSwapControl::Exit].map(|ctrl| (ctrl.id(), ctrl.label(g))).to_vec(),
        };
        if let Some(response) = ui.form(&spec) {
            if response.button == -1 || response.values.len() != 7 { break; }
            let rate = match response.values[0].as_bytes() {
                b"L" => IrSwapControl::LongBondRate, b"S" => IrSwapControl::ShortBondRate,
                _ => IrSwapControl::PrimeRate,
            };
            ir_swap_dialog_click(g, ui, rng, &mut form, rate);
            if response.values[1] == "LONG" { ir_swap_sel_long(g); }
            else if response.values[1] == "SHORT" { ir_swap_sel_short(g); }
            else { g.swap_position = PbStr::new(); }
            for (ctrl, value) in edits.into_iter().zip(response.values.into_iter().skip(2)) {
                *ctrl.field(&mut form).unwrap() = value;
            }
            if let Some(ctrl) = IrSwapControl::ALL.into_iter().find(|c| c.id() == response.button) {
                ir_swap_dialog_click(g, ui, rng, &mut form, ctrl);
            }
            continue;
        }
        let prompt = ir_swap_dialog_prompt(g, &form);
        let options: Vec<PbStr> = IrSwapControl::ALL.iter().map(|c| c.label(g)).collect();
        let Some(k) = ui.choose(&title, &prompt, &options) else {
            // WM_CLOSE: DlgProcIRSwap leaves it to DDT, which ends the dialog
            form.open = false;
            break;
        };
        let ctrl = IrSwapControl::ALL[k];
        if let Some(field) = ctrl.field(&mut form) {
            let cur = field.clone();
            *field = ui.inputbox(&ctrl.label(g), &title, &cur);
            continue;
        }
        ir_swap_dialog_click(g, ui, rng, &mut form, ctrl);
    }
    g.dlg_open_swap_dlg = 0;
}

/// The dialog's static texts and current settings (prompt lines of [`show_ir_swap_dialog`]).
pub fn ir_swap_dialog_prompt(g: &Globals, form: &IrSwapForm) -> Vec<PbStr> {
    let __content = crate::content::current();
    let rate = if g.swap_rate_type == "L" {
        __content.string(11410)
    } else if g.swap_rate_type == "S" {
        __content.string(11411)
    } else if g.swap_rate_type == "P" {
        __content.string(11412)
    } else {
        "(none)"
    };
    let pos = if g.swap_position.is_empty() { s("(none)") } else { g.swap_position.clone() };
    vec![
        s(__content.string(11423)),
        s("Rate type to swap: ") + rate,
        s(__content.string(11424)) + &pos,
        s(__content.string(11425)) + &trim(form.current_rate.as_bytes()),
        IrSwapControl::Notional.label(g) + " " + &form.notional,
        s(__content.string(11426)) + &form.start_qtr + " / " + &form.start_year,
        s(__content.string(11427)) + &form.end_qtr + " / " + &form.end_year,
        s(__content.string(11428)),
    ]
}

/// The part of ShowIRSwapDialog (0x63b281) before `DIALOG SHOW MODAL`: defaults `SwapRateType` to "P",
/// clears `SwapPosition`, computes the acting entity's financials and fills the edit controls (notional
/// = the lesser of 10× net worth and 10,000,000 × `ExchangeRate`, "Zero" below a BBB rating or a net worth
/// under 1,000 × `ExchangeRate`; next quarter as beginning and final date; the current rate).
pub fn ir_swap_dialog_init(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) -> IrSwapForm {
    if g.swap_rate_type.is_empty() {
        g.swap_rate_type = s("P");
    }
    // currentRate AS CURRENCY
    let mut current_rate = Cur::from_f64(cast::<Ext, _>(g.prime_rate));
    if g.swap_rate_type == "L" {
        current_rate = Cur::from_f64(cast::<Ext, _>(g.long_bond_yield));
    }
    if g.swap_rate_type == "S" {
        current_rate = Cur::from_f64(cast::<Ext, _>(g.short_bond_yield));
    }
    g.swap_position = PbStr::new();
    // DlgIRSwap = DIALOG NEW (window handle, not modelled)
    adjust_dialog_frame_size(g.dlg_ir_swap, 0x80ca_0880, 0);
    let active = g.active_entity;
    calc_entity_financials(g, ui, rng, active);
    let mut max_notional = to_quad(g.net_worth * Ext::from_f64(10.0));
    if g.exchange_rate * Ext::from_f64(10000000.0) < cast::<Ext, _>(max_notional) {
        max_notional = to_quad(g.exchange_rate * Ext::from_f64(10000000.0));
    }
    let mut notional_text = fmt_trim(cast::<Ext, _>(max_notional), "* ###,");
    if g.credit_rating_num[g.active_entity] < 7 || g.net_worth < g.exchange_rate * Ext::from_f64(1000.0) {
        notional_text = s("Zero");
    }
    let mut start_qtr = g.cur_quarter + 1;
    if 4 < start_qtr {
        start_qtr = 1;
    }
    let mut start_year = g.cur_year;
    if g.cur_quarter == 4 {
        start_year = g.cur_year + 1;
    }
    // (UI) CONTROL SET OPTION for SwapPosition (always "" here) and SwapRateType
    IrSwapForm {
        notional: notional_text,
        start_qtr: str_trim(cast::<Ext, _>(start_qtr)),
        start_year: str_trim(cast::<Ext, _>(start_year)),
        end_qtr: str_trim(cast::<Ext, _>(start_qtr)),
        end_year: str_trim(cast::<Ext, _>(start_year)),
        current_rate: rate_text(current_rate.get()),
        open: true,
    }
}

/// A click on control `ctrl` of the swap dialog (the `CALL` callbacks of its buttons and option buttons;
/// edit fields are changed directly in `form`).
pub fn ir_swap_dialog_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, form: &mut IrSwapForm, ctrl: IrSwapControl) {
    match ctrl {
        IrSwapControl::LongBondRate => ir_swap_sel_long_bond_rate(g, form),
        IrSwapControl::ShortBondRate => ir_swap_sel_short_bond_rate(g, form),
        IrSwapControl::PrimeRate => ir_swap_sel_prime_rate(g, form),
        IrSwapControl::Long => ir_swap_sel_long(g),
        IrSwapControl::Short => ir_swap_sel_short(g),
        IrSwapControl::Offer => ir_swap_find_offer_click(g, ui, rng, form),
        IrSwapControl::Info => ir_swap_help_click(g, ui),
        IrSwapControl::ViewSwaps => ir_swap_view_swaps_click(g, ui, rng),
        IrSwapControl::Terminate => ir_swap_terminate_click(g, ui, rng, form),
        IrSwapControl::Exit => ir_swap_cancel_click(form),
        IrSwapControl::Notional
        | IrSwapControl::StartQtr
        | IrSwapControl::StartYear
        | IrSwapControl::EndQtr
        | IrSwapControl::EndYear => {}
    }
}

/// Port of DlgProcIRSwap (0x63c1fb).
/// Dialog procedure of the swap dialog: control colours (`WM_CTLCOLOR*`, returns the background brush
/// `BrushHandles(n)`; the dialog itself (`is_dialog`) in 0 on 17) and the tooltips of the buttons
/// (`WM_NOTIFY`). Pure window plumbing; `ctrl_id` is `GetDlgCtrlID(lParam)` for the colour messages,
/// `nm` the notification.
pub fn dlg_proc_ir_swap(g: &Globals, msg_id: i32, is_dialog: bool, ctrl_id: i32, nm: Option<&mut Nmhdr>) -> i32 {
    let __content = crate::content::current();
    let mut result = 0;
    match msg_id {
        275 | 276 | 277 | 5 | 16 | 2 | 274 | 15 => {}
        310 => {
            if is_dialog {
                result = g.brush_handles[17];
            }
        }
        306..=309 | 311 | 312 => {
            result = match ctrl_id {
                955 | 964 | 994 => g.brush_handles[14],
                962 => g.brush_handles[19],
                966 | 996 => g.brush_handles[0],
                968 | 972 | 976 | 980 | 982 | 984 | 986 | 988 => g.brush_handles[17],
                970 | 974 => g.brush_handles[9],
                978 => g.brush_handles[1],
                _ => 0,
            };
        }
        78 => {
            if let Some(nm) = nm {
                if is_tooltip_need_text(nm) {
                    let tip = match get_tooltip_ctrl_id(nm, |h| h) {
                        1 => __content.string(11429),
                        958 => __content.string(11430),
                        959 => __content.string(11431),
                        960 => __content.string(11432),
                        998 => __content.string(11433),
                        _ => "",
                    };
                    if !tip.is_empty() {
                        set_tooltip_text(nm, &s(tip));
                    }
                }
                if is_tab_sel_change(nm) {
                    get_notify_hwnd_from(nm);
                }
            }
        }
        _ => {}
    }
    result
}

/// Port of IRSwap_SelLongBondRate (0x63cd90): `SwapRateType = "L"`, shows the long bond yield in 994.
pub fn ir_swap_sel_long_bond_rate(g: &mut Globals, form: &mut IrSwapForm) {
    g.swap_rate_type = s("L");
    form.current_rate = rate_text(cast::<Ext, _>(g.long_bond_yield));
}

/// Port of IRSwap_SelShortBondRate (0x63ce2e): `SwapRateType = "S"`, shows the short bond yield in 994.
pub fn ir_swap_sel_short_bond_rate(g: &mut Globals, form: &mut IrSwapForm) {
    g.swap_rate_type = s("S");
    form.current_rate = rate_text(cast::<Ext, _>(g.short_bond_yield));
}

/// Port of IRSwap_SelPrimeRate (0x63cecc): `SwapRateType = "P"`, shows the prime rate in 994.
pub fn ir_swap_sel_prime_rate(g: &mut Globals, form: &mut IrSwapForm) {
    g.swap_rate_type = s("P");
    form.current_rate = rate_text(cast::<Ext, _>(g.prime_rate));
}

/// Port of IRSwap_SelLong (0x63cf66): `SwapPosition = "LONG"`.
pub fn ir_swap_sel_long(g: &mut Globals) {
    g.swap_position = s("LONG");
}

/// Port of IRSwap_SelShort (0x63cfba): `SwapPosition = "SHORT"`.
pub fn ir_swap_sel_short(g: &mut Globals) {
    g.swap_position = s("SHORT");
}

/// Port of IRSwap_FindOfferClick (0x63d016), the OFFER button.
/// Below a BBB rating or a net worth of 1,000 × `ExchangeRate` no counterparty is found (message, dialog
/// ends). Without a chosen position, an entity that already has swaps is asked to choose one. A company
/// (not a bank/insurer or brokerage) with swaps (`CountEntitySwaps` with the chosen side) is refused
/// (message, dialog ends); otherwise [`ir_swap_create_swap`] runs, and the dialog ends when it returns 0
/// with the turn's action count used up (`TurnActionCount > 4`, not solo mode).
pub fn ir_swap_find_offer_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, form: &mut IrSwapForm) {
    let __content = crate::content::current();
    let mut pos_type = 0;
    let active = g.active_entity;
    calc_entity_financials(g, ui, rng, active);
    if g.credit_rating_num[active] < 7 || g.net_worth < g.exchange_rate * Ext::from_f64(1000.0) {
        let mut msg_text = s(__content.string(11434));
        let msg_title;
        if g.credit_rating_num[active] < 7 {
            msg_text = msg_text
                + __content.string(11435)
                + __content.string(11436)
                + "\""
                + &g.credit_rating_str
                + "\""
                + ") of "
                + &g.active_entity_name
                + __content.string(11437)
                + "\""
                + "BBB"
                + "\""
                + __content.string(11438)
                + &g.active_entity_name
                + " will "
                + __content.string(11439);
            msg_title = s(__content.string(11440));
        } else {
            msg_text = msg_text + __content.string(11441) + &g.active_entity_name + __content.string(11442);
            if g.currency_code == "USA" {
                msg_text = msg_text + ". ";
            } else {
                msg_text = msg_text
                    + " -- or "
                    + &trim(g.currency_symbol.as_bytes())
                    + &fmt_trim(g.exchange_rate * Ext::from_f64(1000.0), "* ###.00")
                    + " "
                    + &g.million_prefix_lc
                    + "illion"
                    + &g.currency_suffix
                    + ". ";
            }
            msg_text = msg_text
                + __content.string(11443)
                + __content.string(11444);
            msg_title = s(__content.string(11445));
        }
        ui.msgbox(&msg_text, 8208, &msg_title);
        form.open = false;
        return;
    }
    if g.swap_position.is_empty() && 0 < count_entity_swaps(g, active, 0) {
        let mut m = s(__content.string(11446))
            + __content.string(11447);
        show_quick_message(g, ui, &mut m);
        return;
    }
    if g.swap_position == "LONG" {
        pos_type = 1;
    } else if g.swap_position == "SHORT" {
        pos_type = 2;
    }
    let swap_count = count_entity_swaps(g, active, pos_type);
    if active < 131 || g.industry_num[active] == 37 || swap_count < 1 {
        if ir_swap_create_swap(g, ui, rng, form) == 0 && 4 < g.turn_action_count && g.solo_human_mode == 0 {
            form.open = false;
        }
    } else {
        let pos_lc = lcase(g.swap_position.as_bytes(), CaseMode::Ansi);
        let mut msg_text = g.active_entity_name.clone()
            + __content.string(11448)
            + &pos_lc
            + __content.string(11449)
            + __content.string(11450)
            + &g.active_entity_name
            + __content.string(11451)
            + &pos_lc
            + __content.string(11452)
            + __content.string(11453)
            + "\r"
            + "\r";
        let mut other = s("LONG");
        if other == g.swap_position {
            other = s("SHORT");
        }
        if swap_count == 1 {
            msg_text = msg_text
                + __content.string(11454)
                + &g.active_entity_name
                + __content.string(11455)
                + &lcase(other.as_bytes(), CaseMode::Ansi)
                + __content.string(11456);
        }
        ui.msgbox(&msg_text, 8208, &s(__content.string(11457)));
        form.open = false;
    }
}

/// Port of IRSwap_HelpClick (0x63d6dc), the SWAP INFO button: runs `WSRHELP.EXE |SWAPS|` (the help
/// viewer, replaced by imported contextual documentation).
pub fn ir_swap_help_click(_g: &Globals, ui: &mut dyn Ui) {
    ui.help("interest rate swaps");
}

/// Port of IRSwap_ViewSwapsClick (0x63d767): the swap list ([`pick_swap_from_list`]) and the analysis of
/// each picked swap ([`show_swap_details`] mode 1) until nothing is picked.
pub fn ir_swap_view_swaps_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let mut swap_slot = 1;
    while 0 < swap_slot {
        swap_slot = pick_swap_from_list(g, ui, rng);
        if swap_slot == 0 {
            return;
        }
        show_swap_details(g, ui, rng, swap_slot, 1);
    }
}

/// Port of IRSwap_TerminateClick (0x63d7f5): the swap list and the termination question for each picked
/// swap ([`show_swap_details`] mode 2) until nothing is picked or the acting entity has no swaps left;
/// the dialog ends when the turn's action count is used up (`TurnActionCount > 4`, not solo mode).
pub fn ir_swap_terminate_click(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, form: &mut IrSwapForm) {
    let mut swap_slot = 1;
    loop {
        if swap_slot < 1 {
            return;
        }
        swap_slot = pick_swap_from_list(g, ui, rng);
        if swap_slot == 0 {
            return;
        }
        show_swap_details(g, ui, rng, swap_slot, 2);
        if 4 < g.turn_action_count && g.solo_human_mode == 0 {
            form.open = false;
            return;
        }
        if sum_swap_notional(g, g.active_entity, 0) == Ext::from_f64(0.0) {
            return;
        }
    }
}

/// Port of IRSwap_CancelClick (0x63d8e3), the EXIT button: ends the dialog.
pub fn ir_swap_cancel_click(form: &mut IrSwapForm) {
    form.open = false;
}
