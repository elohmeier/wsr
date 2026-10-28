//! Modal report controls must reach their native callbacks and retain the enclosing report.
use wsr_core::pb::real::Ext;

use wsr_core::{
    logic::{
        info_screens::chart::ChartSpec,
        newgame_helpers::init_month_names,
        report::{report_options, show_text_report},
    },
    pb::{pbstr::PbStr, rng::Rng},
    state::Globals,
    ui::{ReportAction, ReportOptions, Ui},
};
#[derive(Default)]
struct Host {
    actions: std::collections::VecDeque<ReportAction>,
    reports: usize,
    charts: Vec<i32>,
    prompts: Vec<String>,
    chart_trade: bool,
}
impl Ui for Host {
    fn msgbox(&mut self, _: &PbStr, _: i32, title: &PbStr) -> i32 {
        self.prompts.push(title.to_display());
        7
    }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr {
        PbStr::new()
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        panic!("report options were lost")
    }
    fn report_with_actions(
        &mut self,
        _: &PbStr,
        lines: &[PbStr],
        _: bool,
        _: &ReportOptions,
    ) -> ReportAction {
        self.reports += 1;
        assert_eq!(lines[0], PbStr::from("Report contents"));
        self.actions.pop_front().unwrap_or(ReportAction::Ok)
    }
    fn chart(&mut self, spec: &ChartSpec) -> bool {
        self.charts.push(spec.entity);
        self.chart_trade
    }
    fn news(&mut self, _: &PbStr) {}
}
fn world() -> Globals {
    let mut g = Globals::default();
    init_month_names(&mut g);
    g.game_in_progress = 1;
    g.game_end_flag = "N".into();
    g.cur_year = 2027;
    g.cur_quarter = 1;
    g.game_end_year = 2030;
    g.num_players = 2;
    g.cur_player = 1;
    g.active_entity = 1;
    g.cash[1] = Ext::from_f64(500.0);
    g.stock_index = Ext::from_f64(10000.0);
    g.entity_status[1] = 1;
    g.entity_status[131] = 1;
    g.ent_name[131] = "Test Corp".into();
    g.ent_symbol[131] = "TEST".into();
    g.market_cap[131] = Ext::from_f64(100.0);
    g.shares_outstanding[131] = Ext::from_f64(10.0);
    g.industry_num[131] = 3;
    g.cap_hist_close[(0, 131)] = Ext::from_f64(3.0);
    for m in 1..=60 {
        g.cap_hist_close[(m, 131)] = Ext::from_f64(100.0);
        g.price_hist_high[(m, 131)] = Ext::from_f64(110.0);
        g.price_hist_low[(m, 131)] = Ext::from_f64(90.0);
    }
    g.text_lines[1] = "Report contents".into();
    g.text_lines[2] = "@END".into();
    g
}
#[test]
fn earnings_chart_returns_to_the_same_report() {
    let __content = wsr_core::content::current();
    let mut g = world();
    g.text_box_title = __content.string(13975).into();
    g.chart_entity = 131;
    g.sim_co_count = 131;
    let mut host = Host {
        actions: [ReportAction::Chart, ReportAction::Ok].into(),
        ..Host::default()
    };
    assert!(report_options(&g).stock_chart);
    show_text_report(&mut g, &mut host, &mut Rng::from_state(1));
    assert_eq!(host.charts, vec![131]);
    assert_eq!(host.reports, 2);
    assert!(g.text_box_title.is_empty());
}
#[test]
fn trading_from_the_chart_does_not_resurrect_the_closed_report() {
    let __content = wsr_core::content::current();
    let mut g = world();
    g.text_box_title = __content.string(13975).into();
    g.chart_entity = 131;
    g.last_sel_entity = 131;
    g.sim_co_count = 131;
    let mut host = Host {
        actions: [ReportAction::Chart].into(),
        chart_trade: true,
        ..Host::default()
    };
    show_text_report(&mut g, &mut host, &mut Rng::from_state(1));
    assert_eq!(host.charts, vec![131]);
    assert_eq!(host.reports, 1);
    assert_eq!(g.dlg_open_news_report, 0);
}
#[test]
fn declined_rating_trade_closes_the_report_without_executing_a_trade() {
    let __content = wsr_core::content::current();
    let mut g = world();
    g.text_box_title = __content.string(13976).into();
    g.cur_analyst_rating = 2;
    g.research_report_corp = 131;
    // A report about a different company while the acting entity is selected invokes confirmation.
    g.sel_entity = 0;
    g.active_entity = 0;
    let mut host = Host {
        actions: [ReportAction::Rating].into(),
        ..Host::default()
    };
    let cash = g.cash[1];
    assert!(report_options(&g).rating_trade);
    show_text_report(&mut g, &mut host, &mut Rng::from_state(1));
    assert_eq!(host.reports, 1);
    assert!(
        host.prompts
            .iter()
            .any(|s| s.contains(__content.string(13899)))
    );
    assert_eq!(g.cash[1], cash);
    assert_eq!(g.research_report_corp, 0);
    assert_eq!(g.cur_analyst_rating, 0);
}
#[test]
fn report_controls_follow_native_conditions_and_block_finished_game_trades() {
    let __content = wsr_core::content::current();
    let mut g = world();
    g.text_box_title = __content.string(13975).into();
    g.chart_entity = 131;
    g.sim_co_count = 130;
    assert!(!report_options(&g).stock_chart);
    g.sim_co_count = 131;
    assert!(report_options(&g).stock_chart);
    g.sel_entity = 131;
    assert!(!report_options(&g).stock_chart);
    g.text_box_title = __content.string(13976).into();
    g.cur_analyst_rating = 3;
    assert!(!report_options(&g).rating_trade);
    g.cur_analyst_rating = 5;
    assert!(report_options(&g).rating_trade);
    g.game_end_flag = "Y".into();
    assert!(!report_options(&g).rating_trade);
}
