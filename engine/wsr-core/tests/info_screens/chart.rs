//! Checks chart scale boundaries and the synthetic stock history path.
use wsr_core::pb::real::{Ext, cast};

use wsr_core::logic::info_screens::chart::{chart_axis_max_for_value, synthesize_price_history};
use wsr_core::pb::ext80::Ext80;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

#[test]
fn axis_boundaries_follow_original_steps() {
    for (value, expected) in [(Ext::from_f64(0.0), 1), (Ext::from_f64(1.0), 3), (Ext::from_f64(300.0), 400), (Ext::from_f64(350.0), 400), (Ext::from_f64(400.0), 500), (Ext::from_f64(100000.0), 100000), (Ext::from_f64(100001.0), 0)] {
        assert_eq!(chart_axis_max_for_value(Ext80::from_ext(cast::<Ext, _>(value))), expected, "{value}");
    }
}

#[test]
fn synthesizing_stock_history_shifts_existing_months() {
    let mut g = Globals::default();
    let mut rng = Rng::default();
    let stock = 131;
    g.market_cap[stock] = Ext::from_f64(100.0);
    g.price_hist_high[(0, stock)] = -Ext::from_f64(1.0);
    g.cap_hist_close[(1, stock)] = Ext::from_f64(100.0);
    g.price_hist_high[(1, stock)] = Ext::from_f64(110.0);
    g.price_hist_low[(1, stock)] = Ext::from_f64(90.0);
    synthesize_price_history(&mut g, &mut rng, stock);
    assert_eq!(g.price_hist_high[(0, stock)], Ext::from_f64(0.0));
    assert_eq!(g.cap_hist_close[(0, stock)], Ext::from_f64(3.0));
    assert_eq!(g.cap_hist_close[(3, stock)], Ext::from_f64(100.0));
    assert_eq!(g.price_hist_high[(3, stock)], Ext::from_f64(110.0));
    assert_eq!(g.price_hist_low[(3, stock)], Ext::from_f64(90.0));
    assert!(g.price_hist_high[(1, stock)] >= g.price_hist_low[(2, stock)]);
    assert!(g.price_hist_low[(1, stock)] <= g.price_hist_high[(2, stock)]);
}

#[test]
fn crypto_chart_button_invokes_chart_flow() {
    let __content = wsr_core::content::current();
    use wsr_core::logic::newgame::LocalTime;
    use wsr_core::logic::trade_screens::commod_crypto::crypto::dlg_crypto_chart_bitcoin;
    use wsr_core::ui::Scripted;
    let mut g = Globals::default();
    let mut ui = Scripted::default();
    let mut rng = Rng::default();
    dlg_crypto_chart_bitcoin(&mut g, &mut ui, &mut rng, &LocalTime::HARNESS);
    assert_eq!(g.chart_entity, 1605);
    assert!(ui.log.iter().any(|entry| entry.contains(__content.string(13859))));
}
