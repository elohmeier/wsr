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
    let buttons = [
        (ctl::CALC, "Calculate"),
        (ctl::SUBMIT, "Submit orders"),
        (ctl::CLEAR_ALL, "Clear all"),
        (ctl::AUTO_TRADE, "Auto-trade"),
        (ctl::SIZE_LIMITS, "Position limits"),
        (ctl::LIST_OPTIONS, "List positions"),
        (ctl::CLOSE_ALL_OPTIONS, "Close all positions"),
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
