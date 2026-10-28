//! Generic dialog helpers of the original's DDT dialogs (window plumbing, no game state): frame sizing and
//! the `WM_NOTIFY` tooltip / tab helpers used by dialog procedures such as `DlgProcCapContribColors`.
//!
//! The original reads the Win32 `NMHDR` / `NMTTDISPINFO` structures through a pointer; the port models the
//! fields it touches as [`Nmhdr`].


#[allow(unused_imports)]
use super::prelude::*;

/// `TTN_GETDISPINFO` / `TTN_NEEDTEXT` notification code (-520).
pub const TTN_NEEDTEXT: i32 = -520;
/// `TCN_SELCHANGE` notification code (-551).
pub const TCN_SELCHANGE: i32 = -551;
/// `TTF_IDISHWND` flag of `NMTTDISPINFO.uFlags`.
pub const TTF_IDISHWND: u32 = 1;

/// The parts of a `WM_NOTIFY` header (`NMHDR`, extended to `NMTTDISPINFO` for tooltips) the helpers use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Nmhdr {
    /// `hwndFrom` (+4); for a tooltip with `TTF_IDISHWND` the tool's window handle.
    pub hwnd_from: i32,
    /// `code` (+8).
    pub code: i32,
    /// `NMTTDISPINFO.uFlags` (+100).
    pub flags: u32,
    /// `NMTTDISPINFO.szText` (+16, 80 bytes): the tooltip text set by [`set_tooltip_text`].
    pub text: PbStr,
}

/// Port of AdjustDialogFrameSize (0x409870).
/// Grows dialog `h_dlg` by the caption/menu height and the vertical scroll bar width implied by its style
/// (`GetSystemMetrics`). Pure window geometry: a no-op in the port.
pub fn adjust_dialog_frame_size(_h_dlg: i32, _dlg_style: u32, _has_menu: i32) {}

/// Port of IsTooltipNeedText (0x409991): the notification is a tooltip text request (`TTN_NEEDTEXT`).
pub fn is_tooltip_need_text(nm: &Nmhdr) -> bool {
    nm.code == TTN_NEEDTEXT
}

/// Port of GetTooltipCtrlId (0x4099f8).
/// For a tooltip request the id of the control the tooltip belongs to: `GetDlgCtrlID(hwndFrom)` when
/// `TTF_IDISHWND` is set (the port takes the control id from `ctrl_id_of`), else `hwndFrom` itself; 0
/// for other notifications.
pub fn get_tooltip_ctrl_id(nm: &Nmhdr, ctrl_id_of: impl Fn(i32) -> i32) -> i32 {
    let mut ctrl_id = 0;
    if nm.code == TTN_NEEDTEXT {
        if nm.flags & TTF_IDISHWND == 1 {
            ctrl_id = ctrl_id_of(nm.hwnd_from);
        } else {
            ctrl_id = nm.hwnd_from;
        }
    }
    ctrl_id
}

/// Port of SetTooltipText (0x409ad4): for a tooltip request, stores the first 79 characters of a
/// non-empty `tip_text` (plus the terminating NUL) as the tooltip text.
pub fn set_tooltip_text(nm: &mut Nmhdr, tip_text: &PbStr) {
    if nm.code == TTN_NEEDTEXT && !tip_text.is_empty() {
        nm.text = mid(tip_text.as_bytes(), 1, 79) + &s("\0");
    }
}

/// Port of IsTabSelChange (0x409b99): the notification is a tab selection change (`TCN_SELCHANGE`).
pub fn is_tab_sel_change(nm: &Nmhdr) -> bool {
    nm.code == TCN_SELCHANGE
}

/// Port of GetNotifyHwndFrom (0x409c00): `hwndFrom` of a tab selection change, else 0.
pub fn get_notify_hwnd_from(nm: &Nmhdr) -> i32 {
    if nm.code == TCN_SELCHANGE { nm.hwnd_from } else { 0 }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn tooltip_helpers() {
        let mut nm = Nmhdr { hwnd_from: 77, code: TTN_NEEDTEXT, flags: 0, text: PbStr::new() };
        assert!(is_tooltip_need_text(&nm) && !is_tab_sel_change(&nm));
        assert_eq!(get_tooltip_ctrl_id(&nm, |_| 5), 77);
        nm.flags = 3;
        assert_eq!(get_tooltip_ctrl_id(&nm, |h| h + 1), 78);
        set_tooltip_text(&mut nm, &s(&"x".repeat(100)));
        assert_eq!(nm.text.len(), 80);
        set_tooltip_text(&mut nm, &PbStr::new());
        assert_eq!(nm.text.len(), 80);
        assert_eq!(get_notify_hwnd_from(&nm), 0);
        let tab = Nmhdr { hwnd_from: 9, code: TCN_SELCHANGE, ..Nmhdr::default() };
        assert_eq!(get_notify_hwnd_from(&tab), 9);
        assert_eq!(get_tooltip_ctrl_id(&tab, |_| 1), 0);
    }
}
