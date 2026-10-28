//! Search parameter validation, including the original's market-cap checkbox quirk.
use wsr_core::logic::info_screens::db_search::{ctl, db_search_read_params, DbSearchDlg};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::state::Globals;

#[test]
fn search_params_clamp_and_preserve_checkbox_quirk() {
    let mut g = Globals::default();
    let mut dlg = DbSearchDlg::default();
    g.srch_chk_high_roe = 1;
    g.srch_chk_low_pe = 1;
    g.srch_chk_market_cap = 1;
    dlg.set_check(ctl::CHK_HIGH_ROE, 1);
    dlg.set_check(ctl::CHK_LOW_PE, 1);
    dlg.set_check(ctl::CHK_MARKET_CAP, 1);
    for (id, value) in [(ctl::TXT_MIN_ROE, "1200"), (ctl::TXT_MAX_PE, "0"), (ctl::TXT_MAX_MKT_CAP, "0"), (ctl::TXT_MIN_MKT_CAP, "0")] {
        dlg.set_text(id, PbStr::from(value));
    }
    db_search_read_params(&mut g, &mut dlg);
    assert_eq!(g.srch_min_roe, 1000);
    assert_eq!(dlg.text(ctl::TXT_MIN_ROE), PbStr::from("1000%"));
    assert_eq!(g.srch_chk_low_pe, 0);
    assert_eq!(dlg.check(ctl::CHK_LOW_PE), 0);
    assert_eq!((g.srch_min_mkt_cap, g.srch_max_mkt_cap), (200, 1000));
    assert_eq!(g.srch_chk_market_cap, 0);
    assert_eq!(dlg.check(ctl::CHK_MARKET_CAP), 1);
}
