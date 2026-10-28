//! The bank asset allocation dialog: `ShowDlgBankAssetAlloc` (0x69f0a2), its dialog procedure
//! `DlgProcBankAssetAllocColors` (0x69ff56), the button callbacks and `ValidateBankAllocTotal` (0x6a0e30).
//!
//! The dialog's five percentage text boxes and the total are modelled as [`BankAllocDlg`]; a host UI can
//! drive it with [`bank_alloc_open`] + [`bank_alloc_dispatch`], or use [`show_dlg_bank_asset_alloc`],
//! which asks through [`Ui::choose`] / [`Ui::inputbox`] ([`bank_alloc_next_action`]).
use crate::pb::real::{Ext, cast};


use super::s;
use crate::logic::report::show_quick_message;
use crate::logic::restructure::dlg_util::{
    adjust_dialog_frame_size, get_notify_hwnd_from, get_tooltip_ctrl_id, is_tab_sel_change, is_tooltip_need_text,
    set_tooltip_text, Nmhdr,
};
use crate::logic::sim_entity::corp_ops::bank_allocate_assets;
use crate::pb::numeric::{str_num, val};
use crate::pb::pbstr::PbStr;
use crate::pb::rng::Rng;
use crate::pb::string::{ltrim, rtrim};
use crate::pb::numeric::{to_long, to_quad};
use crate::state::Globals;
use crate::ui::Ui;

/// Title of the dialog.
#[allow(non_snake_case)]
pub fn BANK_ALLOC_DLG_TITLE(__content: &crate::content::ContentPack) -> &str { __content.string(11228) }
/// Control ids of the five percentage text boxes (prime mortgages, sub-prime mortgages, consumer loans,
/// bonds, cash equivalents).
pub const BANK_ALLOC_FIELD_IDS: [i32; 5] = [1090, 1100, 1105, 1110, 1115];
/// Labels of the five percentage text boxes.
#[allow(non_snake_case)]
pub fn BANK_ALLOC_LABELS(__content: &crate::content::ContentPack) -> [&str; 5] { [__content.string(11229), __content.string(11230), __content.string(11231), "Bonds (Corp. & Govt.)", __content.string(11232)] }
/// Control ids: total text box, "Update Total %" label, APPLY, CANCEL, TURN OFF.
pub const BANK_ALLOC_TOTAL_ID: i32 = 1120;
pub const BANK_ALLOC_UPDATE_ID: i32 = 1055;
pub const BANK_ALLOC_APPLY_ID: i32 = 1;
pub const BANK_ALLOC_CANCEL_ID: i32 = 1123;
pub const BANK_ALLOC_TURN_OFF_ID: i32 = 1125;
/// The instruction text box (1095).
#[allow(non_snake_case)]
pub fn BANK_ALLOC_INSTRUCTIONS(__content: &crate::content::ContentPack) -> &str { __content.string(11233) }

/// The editable contents of the open dialog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BankAllocDlg {
    /// Texts of the five percentage boxes ([`BANK_ALLOC_FIELD_IDS`] order).
    pub fields: [PbStr; 5],
    /// Text of the total box (1120).
    pub total: PbStr,
    /// False once the dialog has ended.
    pub open: bool,
}

/// What the user does in the dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum BankAllocAction {
    /// Type `text` into percentage box `k` (0..4).
    SetField(usize, PbStr),
    /// Click "Update Total %".
    UpdateTotal,
    Apply,
    Cancel,
    TurnOff,
    /// Close the window (no callback).
    Close,
}

/// `RTRIM$(LTRIM$(STR$(x)))` with `digits` significant digits.
fn str_t(x: Ext, digits: u32) -> PbStr {
    rtrim(ltrim(str_num(x, digits).as_bytes()).as_bytes())
}

/// Port of ShowDlgBankAssetAlloc (0x69f0a2).
/// The " [runtime resource wording] " dialog for the bank `ActiveEntity`: target percentages for prime
/// mortgages, sub-prime mortgages, consumer loans, bonds and cash equivalents (APPLY / CANCEL / TURN
/// OFF). Before it: if automatic allocation is on, a one-time help message (`BankAllocHelpPlr`); if it
/// is off, " [runtime resource wording] " (NO → return, else `BankAllocOff = 0`). The user's actions
/// come from [`bank_alloc_next_action`]; see [`show_dlg_bank_asset_alloc_with`].
pub fn show_dlg_bank_asset_alloc(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, h_parent: i32) {
    let Some(mut dlg) = bank_alloc_open(g, ui, h_parent) else { return; };
    let content = crate::content::current();
    while dlg.open {
        use crate::ui::{Form, FormField};
        let form = Form {
            title: s(BANK_ALLOC_DLG_TITLE(&content)), help_topic: None, help_button: None,
            prompt: vec![s(BANK_ALLOC_INSTRUCTIONS(&content)), s(content.string(11256)) + &dlg.total],
            fields: (0..5).map(|k| FormField {
                id: BANK_ALLOC_FIELD_IDS[k], label: s(BANK_ALLOC_LABELS(&content)[k]) + " (%)",
                value: dlg.fields[k].clone(), numeric: true, ..Default::default()
            }).collect(),
            buttons: vec![(BANK_ALLOC_UPDATE_ID, s(content.string(11253))),
                (BANK_ALLOC_APPLY_ID, s("Apply")), (BANK_ALLOC_CANCEL_ID, s("Cancel")),
                (BANK_ALLOC_TURN_OFF_ID, s(content.string(11254)))],
        };
        let action = if let Some(response) = ui.form(&form) {
            if response.values.len() != 5 { BankAllocAction::Close } else {
                for (field, value) in dlg.fields.iter_mut().zip(response.values) { *field = value; }
                match response.button {
                    BANK_ALLOC_UPDATE_ID => BankAllocAction::UpdateTotal,
                    BANK_ALLOC_APPLY_ID => BankAllocAction::Apply,
                    BANK_ALLOC_CANCEL_ID => BankAllocAction::Cancel,
                    BANK_ALLOC_TURN_OFF_ID => BankAllocAction::TurnOff,
                    _ => BankAllocAction::Close,
                }
            }
        } else { bank_alloc_next_action(ui, &dlg) };
        bank_alloc_dispatch(g, ui, rng, &mut dlg, action);
    }
}

/// [`show_dlg_bank_asset_alloc`] with the user's actions taken from `next` (the modal loop of
/// `DIALOG SHOW MODAL`).
pub fn show_dlg_bank_asset_alloc_with(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    h_parent: i32,
    next: &mut dyn FnMut(&mut dyn Ui, &BankAllocDlg) -> BankAllocAction,
) {
    let Some(mut dlg) = bank_alloc_open(g, ui, h_parent) else {
        return;
    };
    while dlg.open {
        let action = next(ui, &dlg);
        bank_alloc_dispatch(g, ui, rng, &mut dlg, action);
    }
}

/// The part of ShowDlgBankAssetAlloc (0x69f0a2) before `DIALOG SHOW MODAL`: the help / resume questions
/// and the initial field texts (`STR$` of the bank's `BankAlloc*Pct` and their sum). `None` when the user
/// declines to resume automatic allocation.
pub fn bank_alloc_open(g: &mut Globals, ui: &mut dyn Ui, _h_parent: i32) -> Option<BankAllocDlg> {
    let __content = crate::content::current();
    let ae = g.active_entity;
    if g.bank_alloc_off[ae] == 0 {
        if g.bank_alloc_help_plr == 0 {
            let msg_text = s(__content.string(11234))
                + "\r"
                + "\r"
                + __content.string(11235)
                + __content.string(11236)
                + &g.active_entity_name
                + __content.string(11237)
                + __content.string(11238)
                + __content.string(11239)
                + "\r"
                + "\r"
                + __content.string(11240)
                + __content.string(11241)
                + "\r"
                + "\r"
                + __content.string(11242)
                + __content.string(11243)
                + "\r"
                + "\r"
                + __content.string(11244)
                + __content.string(11245)
                + __content.string(11246)
                + "\r"
                + "\r"
                + __content.string(11247);
            ui.msgbox(&msg_text, 8256, &(s(__content.string(11248)) + &g.active_entity_name));
            g.bank_alloc_help_plr = g.cur_player;
        }
    } else {
        let msg_text = s(__content.string(11249))
            + __content.string(11250)
            + &g.active_entity_name
            + "."
            + "\r"
            + "\r"
            + __content.string(11251)
            + &g.active_entity_name
            + "?";
        if ui.msgbox(&msg_text, 8228, &s(__content.string(11252))) == 7 {
            return None;
        }
        g.bank_alloc_off[ae] = 0;
    }
    adjust_dialog_frame_size(g.dlg_bank_asset_alloc, 0x80ca_0880, 0);
    // (UI) DIALOG NEW -> DlgBankAssetAlloc; labels, text boxes, buttons
    let pcts = [
        g.bank_alloc_prime_mort_pct[ae],
        g.bank_alloc_subprime_pct[ae],
        g.bank_alloc_consumer_pct[ae],
        g.bank_alloc_bonds_pct[ae],
        g.bank_alloc_cash_pct[ae],
    ];
    let total: i32 = pcts.iter().map(|&p| cast::<i32, _>(p)).sum();
    Some(BankAllocDlg {
        fields: pcts.map(|p| str_t(cast::<Ext, _>(p), 7)),
        total: str_t(cast::<Ext, _>(total), 15),
        open: true,
    })
}

/// One user action in the open dialog: the control callbacks (or closing the window).
pub fn bank_alloc_dispatch(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut BankAllocDlg, action: BankAllocAction) {
    match action {
        BankAllocAction::SetField(k, text) => {
            if k < 5 {
                dlg.fields[k] = text;
            }
        }
        BankAllocAction::UpdateTotal => dlg_bank_asset_alloc_update_total(g, ui, dlg),
        BankAllocAction::Apply => dlg_bank_asset_alloc_apply(g, ui, rng, dlg),
        BankAllocAction::Cancel => dlg_bank_asset_alloc_cancel(dlg),
        BankAllocAction::TurnOff => dlg_bank_asset_alloc_turn_off(g, ui, dlg),
        BankAllocAction::Close => dlg.open = false,
    }
}

/// Default way of asking for the next action: [`Ui::choose`] between editing one of the five fields
/// (then [`Ui::inputbox`] with the current text as default), "Update Total %", APPLY, CANCEL and TURN OFF;
/// cancelling the choice closes the window.
pub fn bank_alloc_next_action(ui: &mut dyn Ui, dlg: &BankAllocDlg) -> BankAllocAction {
    let __content = crate::content::current();
    let mut options: Vec<PbStr> =
        (0..5).map(|k| s(BANK_ALLOC_LABELS(&__content)[k]) + ": " + &dlg.fields[k] + "%").collect();
    options.extend([s(__content.string(11253)), s("APPLY"), s("CANCEL"), s(__content.string(11254))]);
    let prompt = [
        s(BANK_ALLOC_INSTRUCTIONS(&__content)),
        s(__content.string(11255)),
        s(__content.string(11256)) + &dlg.total,
    ];
    match ui.choose(&s(BANK_ALLOC_DLG_TITLE(&__content)), &prompt, &options) {
        Some(k) if k < 5 => {
            let text = ui.inputbox(&(s(BANK_ALLOC_LABELS(&__content)[k]) + " (%)"), &s(BANK_ALLOC_DLG_TITLE(&__content)), &dlg.fields[k]);
            BankAllocAction::SetField(k, text)
        }
        Some(5) => BankAllocAction::UpdateTotal,
        Some(6) => BankAllocAction::Apply,
        Some(7) => BankAllocAction::Cancel,
        Some(8) => BankAllocAction::TurnOff,
        _ => BankAllocAction::Close,
    }
}

/// Port of DlgBankAssetAlloc_UpdateTotal (0x6a0895): "Update Total %" puts
/// `ValidateBankAllocTotal(0)` into the total box.
pub fn dlg_bank_asset_alloc_update_total(g: &mut Globals, ui: &mut dyn Ui, dlg: &mut BankAllocDlg) {
    let total = validate_bank_alloc_total(g, ui, dlg, 0);
    dlg.total = str_t(cast::<Ext, _>(total), 15);
}

/// Port of DlgBankAssetAlloc_Apply (0x6a091b).
/// APPLY: `ValidateBankAllocTotal(1)` into the total box; if it is 100, stores the five fields
/// (`ROUND(VAL(text))` as INTEGER) in the bank's `BankAlloc*Pct`, ends the dialog, counts a turn action,
/// shows the "percentages … have been set" note and runs `BankAllocateAssets(ActiveEntity, 3, 0)`.
pub fn dlg_bank_asset_alloc_apply(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, dlg: &mut BankAllocDlg) {
    let __content = crate::content::current();
    let total = validate_bank_alloc_total(g, ui, dlg, 1);
    dlg.total = str_t(cast::<Ext, _>(total), 15);
    if total != 100 {
        return;
    }
    let ae = g.active_entity;
    let pct = |k: usize| cast::<i16, _>(to_quad(val(dlg.fields[k].as_bytes())));
    g.bank_alloc_prime_mort_pct[ae] = pct(0);
    g.bank_alloc_subprime_pct[ae] = pct(1);
    g.bank_alloc_consumer_pct[ae] = pct(2);
    g.bank_alloc_bonds_pct[ae] = pct(3);
    g.bank_alloc_cash_pct[ae] = pct(4);
    dlg.open = false;
    g.turn_action_count += 1;
    let mut m = s(__content.string(11257))
        + __content.string(11258)
        + "\r\n"
        + "\r\n"
        + __content.string(11259)
        + __content.string(11260)
        + __content.string(11261)
        + __content.string(11262);
    show_quick_message(g, ui, &mut m);
    bank_allocate_assets(g, ui, rng, ae, 3, 0);
}

/// Port of DlgBankAssetAlloc_Cancel (0x6a0c0d): ends the dialog.
pub fn dlg_bank_asset_alloc_cancel(dlg: &mut BankAllocDlg) {
    dlg.open = false;
}

/// Port of DlgBankAssetAlloc_TurnOff (0x6a0c5e): after "ARE YOU SURE …" (not NO) sets
/// `BankAllocOff(ActiveEntity) = 1`, explains the consequences and ends the dialog.
pub fn dlg_bank_asset_alloc_turn_off(g: &mut Globals, ui: &mut dyn Ui, dlg: &mut BankAllocDlg) {
    let __content = crate::content::current();
    let msg_text = s(__content.string(11263)) + &g.active_entity_name + "?";
    if ui.msgbox(&msg_text, 8228, &s(__content.string(11252))) == 7 {
        return;
    }
    let ae = g.active_entity;
    g.bank_alloc_off[ae] = 1;
    let mut m = s(__content.string(11264))
        + &g.active_entity_name
        + ". "
        + __content.string(11265)
        + __content.string(11266)
        + "\r\n"
        + "\r\n"
        + __content.string(11267)
        + "\r\n"
        + "\r\n"
        + __content.string(11268)
        + "\r\n"
        + __content.string(11269)
        + "\r\n"
        + __content.string(11270);
    show_quick_message(g, ui, &mut m);
    dlg.open = false;
}

/// Port of ValidateBankAllocTotal (0x6a0e30).
/// Sums `ROUND(VAL(text))` of the five fields. A cash-equivalents entry below 10 pops up an error
/// ("Resetting as 10 …", plus "the total … did not add up to 100%" if `show_errors = 1` and the sum so far
/// is not 100), sets that field to "10" and the total box to the corrected sum. Otherwise, with
/// `show_errors = 1` and a sum other than 100, a MSGBOX "Numbers Don't Add Up!". Returns the (corrected)
/// sum.
pub fn validate_bank_alloc_total(g: &mut Globals, ui: &mut dyn Ui, dlg: &mut BankAllocDlg, show_errors: i32) -> i32 {
    let __content = crate::content::current();
    let mut total_pct: i32 = 0;
    let mut pct = 0;
    let mut cash_low = false;
    for field in 1..=5 {
        pct = to_long(val(dlg.fields[field - 1].as_bytes()));
        total_pct = total_pct.wrapping_add(pct);
        if field == 5 && pct < 10 {
            cash_low = true;
        }
    }
    if !cash_low {
        if total_pct != 100 && show_errors == 1 {
            let msg_text = s(__content.string(11271))
                + &PbStr::from(str_num(cast::<Ext, _>(total_pct), 15).as_str())
                + __content.string(11272)
                + __content.string(11273);
            ui.msgbox(&msg_text, 8256, &s(__content.string(11274)));
        }
        return total_pct;
    }
    let mut msg_text = s(__content.string(11275))
        + "\r\n"
        + "\r\n"
        + __content.string(11276);
    if show_errors == 1 && total_pct != 100 {
        msg_text = msg_text + "\r\n" + "\r\n" + __content.string(11277);
    }
    show_quick_message(g, ui, &mut msg_text);
    dlg.fields[4] = s("10");
    total_pct = total_pct.wrapping_add(10 - pct);
    dlg.total = str_t(cast::<Ext, _>(total_pct), 15);
    total_pct
}

/// Port of DlgProcBankAssetAllocColors (0x69ff56).
/// Dialog procedure of the allocation dialog: control colours (`WM_CTLCOLOR*`: the dialog and the "%"
/// labels / note on colour 9, the "Update Total %" label on 14, the field labels on 0 — returns the
/// background brush `BrushHandles(n)`) and the tooltips of APPLY / CANCEL / TURN OFF (`WM_NOTIFY`). Pure
/// window plumbing; `ctrl_id` is `GetDlgCtrlID(lParam)` for the colour messages, `nm` the notification.
pub fn dlg_proc_bank_asset_alloc_colors(g: &Globals, msg_id: i32, is_dialog: bool, ctrl_id: i32, nm: Option<&mut Nmhdr>) -> i32 {
    let __content = crate::content::current();
    let mut brush = 0;
    match msg_id {
        310 => {
            if is_dialog {
                brush = g.brush_handles[9];
            }
        }
        306..=309 | 311 | 312 => {
            brush = match ctrl_id {
                1025 | 1030 | 1035 | 1040 | 1045 | 1050 | 1060 => g.brush_handles[9],
                1055 => g.brush_handles[14],
                1065 | 1070 | 1075 | 1080 | 1085 => g.brush_handles[0],
                _ => 0,
            };
        }
        78 => {
            if let Some(nm) = nm {
                if is_tooltip_need_text(nm) {
                    match get_tooltip_ctrl_id(nm, |h| h) {
                        1 => set_tooltip_text(nm, &s(__content.string(11278))),
                        1123 => set_tooltip_text(nm, &s(__content.string(11279))),
                        1125 => set_tooltip_text(nm, &s(__content.string(11280))),
                        _ => {}
                    }
                }
                if is_tab_sel_change(nm) {
                    get_notify_hwnd_from(nm);
                }
            }
        }
        _ => {}
    }
    brush
}
