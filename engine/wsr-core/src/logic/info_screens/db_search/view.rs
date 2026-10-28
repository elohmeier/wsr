//! Presentation of the database search controls; callbacks and matching stay in the engine.
use super::*;

#[derive(Clone, Debug)]
pub struct DbSearchField {
    pub id: i32,
    pub label: PbStr,
    pub value: PbStr,
}

#[derive(Clone, Debug)]
pub struct DbSearchChange {
    pub id: i32,
    pub label: PbStr,
    pub value: PbStr,
}

#[derive(Clone, Debug)]
pub struct DbSearchCriterion {
    pub id: i32,
    pub label: PbStr,
    pub checked: bool,
    pub fields: Vec<DbSearchField>,
    pub change: Option<DbSearchChange>,
}

#[derive(Clone, Debug)]
pub struct DbSearchView {
    pub title: PbStr,
    pub prompt: PbStr,
    pub count: PbStr,
    pub criteria: Vec<DbSearchCriterion>,
    pub buttons: Vec<(i32, PbStr)>,
    pub focus: Option<i32>,
}

pub fn db_search_view(dlg: &DbSearchDlg, focus: Option<i32>) -> DbSearchView {
    let content = crate::content::current();
    let boxes = VALUE_BOXES(&content);
    let criteria = CHECKBOXES(&content)
        .iter()
        .map(|&(id, caption)| {
            let field_ids: &[i32] = match id {
                ctl::CHK_HIGH_ROE => &[ctl::TXT_MIN_ROE],
                ctl::CHK_LOW_PE => &[ctl::TXT_MAX_PE],
                ctl::CHK_PRICE_VS_NW => &[ctl::TXT_PRICE_PCT_NW],
                ctl::CHK_DIV_YIELD => &[ctl::TXT_DIV_YIELD],
                ctl::CHK_MARKET_CAP => &[ctl::TXT_MIN_MKT_CAP, ctl::TXT_MAX_MKT_CAP],
                ctl::CHK_EARN_UP_NEXT_QTR => &[ctl::TXT_EARN_GROWTH],
                _ => &[],
            };
            let change = match id {
                ctl::CHK_EARN_UP_YEARS => Some((
                    ctl::BTN_EARN_YEARS,
                    ctl::TXT_EARN_YEARS,
                    "Change earnings-up years",
                )),
                ctl::CHK_MGMT_RATING => Some((
                    ctl::BTN_MGMT_RATING,
                    ctl::TXT_MGMT_RATING,
                    "Change management rating",
                )),
                ctl::CHK_CREDIT_RATING => Some((
                    ctl::BTN_CREDIT_RATING,
                    ctl::TXT_CREDIT_RATING,
                    "Change credit rating",
                )),
                ctl::CHK_ANALYST_RATING => Some((
                    ctl::BTN_ANALYST_RATING,
                    ctl::TXT_ANALYST_RATING,
                    "Change analyst rating",
                )),
                _ => None,
            }
            .map(|(id, value, label)| DbSearchChange {
                id,
                label: s(label),
                value: dlg.text(value),
            });
            DbSearchCriterion {
                id,
                label: if id == ctl::CHK_CONVERTIBLE_ONLY || id == ctl::CHK_ONE_INDUSTRY {
                    dlg.text(id)
                } else {
                    s(caption)
                },
                checked: dlg.check(id) != 0,
                fields: field_ids
                    .iter()
                    .map(|&id| DbSearchField {
                        id,
                        label: s(boxes.iter().find(|(i, _)| *i == id).unwrap().1),
                        value: dlg.text(id),
                    })
                    .collect(),
                change,
            }
        })
        .collect();
    DbSearchView {
        title: s(content.string(6071)),
        prompt: s(content.string(6070)),
        count: s(content.string(6068)) + &dlg.text(ctl::TXT_COUNT),
        criteria,
        focus,
        buttons: vec![
            (ctl::BTN_DISPLAY, s(content.string(6066))),
            (ctl::BTN_CLEAR, s(content.string(6067))),
            (ctl::BTN_HELP, s("Help")),
            (ctl::BTN_CLOSE, s("Close")),
        ],
    }
}
