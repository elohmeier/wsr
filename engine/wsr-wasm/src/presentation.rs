//! Browser presentation packets; the core owns every calculation and callback.
use wsr_core::pb::real::{Ext, cast};

use crate::summary::json_str;
use wsr_core::logic::{
    info_screens::ownership::{DiagramControlKind, OwnershipDiagram},
    trade_screens::option_station::{OptionStation, StationAction, ctl},
};
use wsr_core::pb::pbstr::PbStr;
fn js(s: &PbStr) -> String {
    json_str(&s.to_display())
}

pub fn db_search_json(view: &wsr_core::logic::info_screens::db_search::DbSearchView) -> String {
    serde_json::json!({
        "title": view.title.to_display(), "prompt": view.prompt.to_display(), "count": view.count.to_display(),
        "helpTopic": "database search", "focus": view.focus,
        "criteria": view.criteria.iter().map(|c| serde_json::json!({
            "id": c.id, "label": c.label.to_display(), "checked": c.checked,
            "fields": c.fields.iter().map(|f| serde_json::json!({"id": f.id, "label": f.label.to_display(), "value": f.value.to_display()})).collect::<Vec<_>>(),
            "change": c.change.as_ref().map(|b| serde_json::json!({"id": b.id, "label": b.label.to_display(), "value": b.value.to_display()}))
        })).collect::<Vec<_>>(),
        "buttons": view.buttons.iter().map(|(id, label)| serde_json::json!({"id": id, "label": label.to_display()})).collect::<Vec<_>>()
    }).to_string()
}

pub fn db_search_actions(bytes: &[u8], view: &wsr_core::logic::info_screens::db_search::DbSearchView)
    -> Vec<wsr_core::logic::info_screens::db_search::DbSearchAction> {
    use wsr_core::logic::info_screens::db_search::{DbSearchAction as Action, ctl};
    fn parse(bytes: &[u8], view: &wsr_core::logic::info_screens::db_search::DbSearchView) -> Option<Vec<Action>> {
        let button = i32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        if button == -1 { return Some(vec![Action::Close]); }
        let fields: Vec<_> = view.criteria.iter().flat_map(|c| &c.fields).collect();
        let is_field = fields.iter().any(|f| f.id == button);
        let checkbox = view.criteria.iter().any(|c| c.id == button);
        if !is_field && !checkbox && !view.buttons.iter().any(|(id, _)| *id == button)
            && !view.criteria.iter().any(|c| c.change.as_ref().is_some_and(|b| b.id == button)) { return None; }
        let mut p = 4;
        let mut values = Vec::new();
        for _ in 0..=fields.len() {
            let len = u32::from_le_bytes(bytes.get(p..p + 4)?.try_into().ok()?) as usize;
            p += 4;
            if len > 4096 { return None; }
            values.push(PbStr(bytes.get(p..p + len)?.to_vec()));
            p += len;
        }
        if p != bytes.len() { return None; }
        let checked = values.pop()?;
        let mut actions: Vec<_> = fields.iter().zip(values).filter(|(f, value)| f.value != *value)
            .map(|(f, value)| Action::Edit(f.id, value)).collect();
        if checkbox {
            if checked != "0" && checked != "1" { return None; }
            actions.push(Action::SetChecked(button, checked == "1"));
        } else if !is_field { actions.push(Action::Click(button)); }
        else if actions.is_empty() { actions.push(Action::EditChanged(button)); }
        if button == ctl::BTN_CLOSE { return Some(vec![Action::Close]); }
        Some(actions)
    }
    parse(bytes, view).unwrap_or_else(|| vec![Action::Close])
}

pub fn form_json(form: &wsr_core::ui::Form) -> String {
    serde_json::json!({
        "title": form.title.to_display(),
        "helpTopic": form.help_topic.as_ref().map(PbStr::to_display),
        "helpButton": form.help_button,
        "prompt": form.prompt.iter().map(PbStr::to_display).collect::<Vec<_>>(),
        "fields": form.fields.iter().map(|f| serde_json::json!({
            "id": f.id, "label": f.label.to_display(), "value": f.value.to_display(),
            "hint": f.hint.to_display(), "numeric": f.numeric, "min": f.min, "max": f.max,
            "minLength": f.min_length, "maxLength": f.max_length,
            "choices": f.choices.iter().map(|(value, label)| serde_json::json!({
                "value": value.to_display(), "label": label.to_display()
            })).collect::<Vec<_>>(), "refreshOnChange": f.refresh_on_change
        })).collect::<Vec<_>>(),
        "buttons": form.buttons.iter().map(|(id, label)| serde_json::json!({
            "id": id, "label": label.to_display()
        })).collect::<Vec<_>>()
    }).to_string()
}

/// i32 callback id followed by u32-length-prefixed CP1252 fields. Reject incomplete,
/// oversized or unexpected packets as a close; never execute a partial submission.
pub fn form_response(bytes: &[u8], form: &wsr_core::ui::Form) -> wsr_core::ui::FormResponse {
    fn parse(bytes: &[u8], form: &wsr_core::ui::Form) -> Option<wsr_core::ui::FormResponse> {
        let button = i32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        if button != 0 && !form.buttons.iter().any(|(id, _)| *id == button) { return None; }
        let mut p = 4;
        let mut values = Vec::new();
        for field in &form.fields {
            let len = u32::from_le_bytes(bytes.get(p..p + 4)?.try_into().ok()?) as usize;
            p += 4;
            if len > 4096 { return None; }
            let mut value = bytes.get(p..p + len)?.to_vec();
            p += len;
            if let Some(max) = field.max_length { value.truncate(max); }
            values.push(PbStr(value));
        }
        (p == bytes.len()).then_some(wsr_core::ui::FormResponse { button, values })
    }
    parse(bytes, form).unwrap_or_else(|| wsr_core::ui::FormResponse {
        button: -1, values: form.fields.iter().map(|f| f.value.clone()).collect()
    })
}
/// Undo the original chart's drawing-only scale and GDP translation.
/// Empty months and invalid numbers are omitted rather than plotted as zero.
pub fn chart_history_json(spec: &wsr_core::logic::info_screens::chart::ChartSpec) -> String {
    let multiplier = 1000_f64.powi(cast::<i32, _>(spec.thousands_shift));
    spec.highs
        .iter()
        .zip(&spec.lows)
        .zip(&spec.closes)
        .enumerate()
        .filter_map(|(i, ((&high, &low), &close))| {
            if high == Ext::from_f64(0.0) && low == Ext::from_f64(0.0) && close == Ext::from_f64(0.0) {
                return None;
            }
            let values = [high, low, close].map(|value| value * multiplier - spec.gdp_offset);
            if !values.iter().all(|value| value.is_finite()) {
                return None;
            }
            Some(format!(
                "{{\"month\":{},\"high\":{},\"low\":{},\"close\":{}}}",
                i + 1,
                values[0],
                values[1],
                values[2]
            ))
        })
        .collect::<Vec<_>>()
        .join(",")
}
pub fn ownership_json(spec: &OwnershipDiagram) -> String {
    let controls = spec.controls.iter().map(|c| {
        let kind = match c.kind { DiagramControlKind::Label => "label", DiagramControlKind::Line => "line", DiagramControlKind::Button => "button" };
        format!("{{\"id\":{},\"kind\":\"{kind}\",\"text\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"clickable\":{}}}",
            c.id, js(&c.text), c.x, c.y, c.w, c.h, c.clickable)
    }).collect::<Vec<_>>().join(",");
    format!(
        "{{\"title\":{},\"width\":{},\"height\":{},\"controls\":[{controls}]}}",
        js(&spec.title),
        spec.width,
        spec.height
    )
}
pub fn option_json(spec: &OptionStation) -> String {
    let rows = (0..8)
        .map(|k| {
            format!(
                "{{\"side\":{},\"strike\":{},\"expiry\":{},\"quantity\":{},\"price\":{}}}",
                js(&spec.grid.side[k]),
                js(&spec.grid.strike[k]),
                js(&spec.grid.exp[k]),
                js(&spec.grid.qty[k]),
                js(&spec.grid.price[k])
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let close_all_caption = spec.close_all_caption.to_display();
    let buttons = [
        (ctl::CALC, "Calculate"),
        (ctl::SUBMIT, "Submit orders"),
        (ctl::CLEAR_ALL, "Clear all"),
        (ctl::AUTO_TRADE, "Auto-trade"),
        (ctl::SIZE_LIMITS, "Position limits"),
        (ctl::LIST_OPTIONS, "List positions"),
        (ctl::CLOSE_ALL_OPTIONS, close_all_caption.as_str()),
        (ctl::HELP, "Help"),
        (ctl::CLOSE, "Close"),
    ];
    let buttons = buttons
        .iter()
        .map(|(id, text)| format!("{{\"id\":{id},\"label\":{}}}", json_str(text)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"title\":{},\"stock\":{},\"strikeHeader\":{},\"expiryHeader\":{},\"status\":{},\"rows\":[{rows}],\"buttons\":[{buttons}]}}",
        js(&spec.title),
        js(&spec.stock_label),
        js(&spec.strike_header),
        js(&spec.exp_header),
        js(&spec.status)
    )
}
/// i32 button, then 32 u32-length-prefixed CP1252 fields, in row-major order.
pub fn option_actions(bytes: &[u8], spec: &OptionStation) -> Vec<StationAction> {
    fn parse(bytes: &[u8], spec: &OptionStation) -> Option<Vec<StationAction>> {
        let button = i32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        if ![-1, 1, 1372, 1374, 1376, 1378, 1379, 1380, 1381, 1382].contains(&button) {
            return None;
        }
        let mut pos = 4;
        let mut out = Vec::new();
        for row in 1..=8 {
            for base in [ctl::SIDE, ctl::STRIKE, ctl::EXP, ctl::QTY] {
                let len = cast::<usize, _>(u32::from_le_bytes(bytes.get(pos..pos + 4)?.try_into().ok()?));
                pos += 4;
                if len > 4096 {
                    return None;
                }
                let text = PbStr(bytes.get(pos..pos + len)?.to_vec());
                pos += len;
                if text != spec.grid.text(base + row) {
                    out.push(StationAction::SetText(base + row, text));
                    if base == ctl::SIDE {
                        out.push(StationAction::KillFocus(base + row));
                    }
                }
            }
        }
        if pos != bytes.len() {
            return None;
        }
        out.push(if button == -1 {
            StationAction::Close
        } else {
            StationAction::Click(button)
        });
        Some(out)
    }
    parse(bytes, spec).unwrap_or_else(|| vec![StationAction::Close])
}
#[cfg(test)]
mod tests {
use wsr_core::pb::real::{Ext, cast};

    use super::*;
    #[test]
    fn database_packets_apply_pending_edits_before_checkbox_state_and_reject_truncation() {
        use wsr_core::logic::info_screens::db_search::{DbSearchDlg, DbSearchAction, db_search_view, ctl};
        crate::install_test_content();
        let mut dlg = DbSearchDlg::default();
        dlg.set_text(ctl::TXT_MAX_PE, PbStr::from("10"));
        let view = db_search_view(&dlg, None);
        let mut bytes = ctl::CHK_LOW_PE.to_le_bytes().to_vec();
        for field in view.criteria.iter().flat_map(|c| &c.fields) {
            let value = if field.id == ctl::TXT_MAX_PE { PbStr::from("25") } else { field.value.clone() };
            bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes.extend_from_slice(&1u32.to_le_bytes()); bytes.push(b'1');
        assert_eq!(db_search_actions(&bytes, &view), [DbSearchAction::Edit(ctl::TXT_MAX_PE, PbStr::from("25")), DbSearchAction::SetChecked(ctl::CHK_LOW_PE, true)]);
        bytes.pop();
        assert_eq!(db_search_actions(&bytes, &view), [DbSearchAction::Close]);
    }
    #[test]
    fn form_packets_preserve_empty_fields_and_reject_partial_submissions() {
        use wsr_core::ui::{Form, FormField};
        let form = Form {
            fields: vec![FormField { max_length: Some(20), ..Default::default() }, FormField::default()],
            buttons: vec![(1, PbStr::from("OK"))], ..Default::default()
        };
        let mut bytes = 1i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&21u32.to_le_bytes());
        bytes.extend_from_slice(b"123456789012345678901");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let response = form_response(&bytes, &form);
        assert_eq!(response.button, 1);
        assert_eq!(response.values, vec![PbStr::from("12345678901234567890"), PbStr::new()]);
        bytes.pop();
        assert_eq!(form_response(&bytes, &form).button, -1);
        bytes.extend_from_slice(&[0, 0]);
        assert_eq!(form_response(&bytes, &form).button, -1);
        bytes[..4].copy_from_slice(&999i32.to_le_bytes());
        assert_eq!(form_response(&bytes, &form).button, -1);
    }
    #[test]
    fn chart_history_restores_units_and_negative_growth_without_filling_empty_months() {
        crate::install_test_content();
        use wsr_core::logic::info_screens::chart::ChartSpec;
        let mut spec = ChartSpec {
            entity: 1604,
            size: 2,
            end_year: 2027,
            end_month: 1,
            title: PbStr::new(),
            width: 0,
            height: 0,
            controls: vec![],
            highs: vec![Ext::from(0.0), Ext::from(3.0), Ext::from(f64::NAN)],
            lows: vec![Ext::from(0.0), Ext::from(1.0), Ext::from(1.0)],
            closes: vec![Ext::from(0.0), Ext::from(2.0), Ext::from(1.0)],
            scale_max: 10,
            thousands_shift: 0,
            min_low: -Ext::from_f64(2.0),
            gdp_offset: Ext::from_f64(2.0),
            trade_label: None,
        };
        assert_eq!(
            chart_history_json(&spec),
            "{\"month\":2,\"high\":1,\"low\":-1,\"close\":0}"
        );
        spec.entity = 1;
        spec.gdp_offset = Ext::from_f64(0.0);
        spec.thousands_shift = 1;
        assert_eq!(
            chart_history_json(&spec),
            "{\"month\":2,\"high\":3000,\"low\":1000,\"close\":2000}"
        );
    }
    #[test]
    fn grid_edits_precede_submit_and_malformed_packets_cannot_submit() {
        crate::install_test_content();
        let mut bytes = ctl::SUBMIT.to_le_bytes().to_vec();
        for k in 0..32 {
            let text: &[u8] = match k {
                0 => b"B",
                1 => b"100",
                2 => b"12/2028",
                3 => b"5",
                _ => b"",
            };
            bytes.extend_from_slice(&(cast::<u32, _>(text.len())).to_le_bytes());
            bytes.extend_from_slice(text);
        }
        let spec = OptionStation::default();
        let actions = option_actions(&bytes, &spec);
        assert_eq!(
            actions.first(),
            Some(&StationAction::SetText(1321, PbStr::from("B")))
        );
        assert_eq!(actions.last(), Some(&StationAction::Click(ctl::SUBMIT)));
        assert_eq!(actions.len(), 6);
        bytes.pop();
        assert_eq!(option_actions(&bytes, &spec), vec![StationAction::Close]);
    }
}
