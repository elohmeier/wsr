//! JSON view of a loaded game for the demo page: date, players, market, largest companies, news.
use wsr_core::pb::real::{Ext, cast};


use wsr_core::pb::pbstr::PbStr;
use wsr_core::state::Globals;

/// JSON string literal (input is display text).
pub fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            c if (cast::<u32, _>(c)) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn js(s: &PbStr) -> String {
    json_str(&s.to_display())
}

fn num(x: impl Into<Ext>) -> String {
    let x = x.into().to_f64();
    if x.is_finite() {
        format!("{x}")
    } else {
        "null".into()
    }
}

/// Display calculations run on an isolated snapshot. The finance/date routines write scratch
/// globals (and may draw RNG), so rendering must never call them on the live session.
fn readouts_json(g: &Globals) -> String {
    use wsr_core::{
        logic::{earnings, finance},
        pb::rng::Rng,
        ui::Scripted,
    };
    let mut view = g.clone();
    let (month, day) = earnings::current_game_date(&mut view);
    let month = month.clamp(1, 12);
    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let date = format!(
        "{} {day}, {}",
        month_names[(month - 1) as usize],
        g.cur_year
    );
    let earnings = if (11..=1600).contains(&g.sel_entity) {
        earnings::get_next_earnings_date_str(&mut view, g.sel_entity)
            .to_display()
            .trim()
            .to_string()
    } else {
        String::new()
    };
    let player = g.cur_player;
    let balance = if (1..=g.num_players.min(5)).contains(&player) {
        finance::calc_entity_financials(
            &mut view,
            &mut Scripted::default(),
            &mut Rng::from_state(1),
            player,
        );
        format!(
            "{{\"cash\":{},\"otherAssets\":{},\"totalAssets\":{},\"debt\":{},\"netWorth\":{}}}",
            num(view.cash[player]),
            num(view.total_assets - view.cash[player]),
            num(view.total_assets),
            num(view.tmp_total_debt),
            num(view.net_worth)
        )
    } else {
        "null".into()
    };
    let selected = g.sel_entity;
    let controller = if (11..=1600).contains(&selected) {
        let id = g.corp_controller[selected];
        if id != selected && (1..=1600).contains(&id) {
            id
        } else {
            0
        }
    } else {
        0
    };
    let controller_name = if selected <= 10 {
        "N/A".into()
    } else if controller == 0 {
        "No one".into()
    } else {
        g.ent_name[controller].to_display()
    };
    format!("\"gameDate\":{},\"nextEarnings\":{},\"controllerId\":{controller},\"controllerName\":{},\"tickerText\":{},\"balance\":{balance},\"economy\":{{\"oil\":{},\"gold\":{},\"silver\":{},\"wheat\":{},\"corn\":{},\"longBondYield\":{},\"shortBondYield\":{}}}",
        json_str(&date), json_str(&earnings), json_str(&controller_name), js(&g.ticker_text),
        num(100.0*g.oil_price as f64),num(g.gold_price as f64),num(g.silver_price as f64),
        num(g.wheat_price as f64),num(g.corn_price as f64),num(g.long_bond_yield as f64),num(g.short_bond_yield as f64))
}

pub fn summary_json(g: &Globals) -> String {
    let readouts = readouts_json(g);
    // Use the runtime catalog and existing control definitions for the browser's captions.
    // Live date, price and balance values are supplied separately below.
    let captions = wsr_core::logic::shell::desk::desk_controls(
        g, &wsr_core::logic::newgame::LocalTime::HARNESS,
    ).iter().map(|control| format!("{}:{}", json_str(&control.id.to_string()), js(&control.text)))
        .collect::<Vec<_>>().join(",");
    let content = wsr_core::content::current();
    let headings = format!("[{},{}]", json_str(content.string(13898)), json_str(content.string(13766)));
    let disabled = wsr_core::logic::shell::desk::disabled_controls(g).iter()
        .map(i32::to_string).collect::<Vec<_>>().join(",");
    let selected_price =
        if (11..=1600).contains(&g.sel_entity) && g.shares_outstanding[g.sel_entity] > Ext::from_f64(0.0) {
            format!(
                "{:.2}",
                g.market_cap[g.sel_entity] / g.shares_outstanding[g.sel_entity]
            )
        } else {
            String::new()
        };
    let players: Vec<String> = (1..=g.num_players.clamp(0, 5))
        .map(|i| {
            format!(
                "{{\"id\":{i},\"name\":{},\"cash\":{},\"computer\":{}}}",
                js(&g.player_name[i]),
                num(g.cash[i]),
                g.is_computer_player[i] != 0
            )
        })
        .collect();
    let mut corps: Vec<i32> = (11..=1600).filter(|&i| g.entity_status[i] == 1).collect();
    corps.sort_by(|&a, &b| g.market_cap[b].total_cmp(&g.market_cap[a]));
    let active = corps.len();
    let top: Vec<String> = corps
        .iter()
        .take(25)
        .map(|&i| {
            let shares = g.shares_outstanding[i];
            let price = if shares != Ext::from_f64(0.0) { g.market_cap[i] / shares } else { Ext::from_f64(0.0) };
            format!(
                "{{\"id\":{i},\"name\":{},\"symbol\":{},\"industry\":{},\"marketCapM\":{},\"price\":{},\"cash\":{},\"controller\":{}}}",
                js(&g.ent_name[i]),
                js(&g.ent_symbol[i]),
                js(&g.ind_name[g.industry_num[i] as i32]),
                num(g.market_cap[i] * 100.0),
                num(price),
                num(g.cash[i]),
                g.corp_controller[i]
            )
        })
        .collect();
    let news: Vec<String> = (1..=60)
        .rev()
        .map(|i| js(&g.news_headline[i]))
        .filter(|s| s != "\"\"")
        .take(20)
        .collect();
    let industries = (1..=71)
        .map(|i| format!("{{\"id\":{i},\"name\":{}}}", js(&g.ind_name[i])))
        .collect::<Vec<_>>()
        .join(",");
    let watchlist = (1..=15).filter_map(|slot| {
        if !(1..=10).contains(&g.cur_player) { return None; }
        let id = g.watch_list[(slot, g.cur_player)];
        if !(11..=1600).contains(&id) || g.entity_status[id] != 1 { return None; }
        Some(format!("{{\"slot\":{slot},\"id\":{id},\"symbol\":{},\"name\":{},\"price\":{},\"direction\":{}}}",
            js(&g.ent_symbol[id]), js(&g.ent_name[id]), js(&g.stream_cur_price[(slot, g.cur_player)]), g.stream_price_dir[(slot, g.cur_player)]))
    }).collect::<Vec<_>>().join(",");
    let desk = format!("\"deskLabels\":{{{captions}}},\"newsHeading\":{headings},\"disabledControls\":[{disabled}],\"gameInProgress\":{},\"gameOver\":{},\"clock\":{},\"tickerSpeed\":{},\"transactionsLeft\":{},\
\"selectedEntity\":{},\"selectedName\":{},\"selectedSymbol\":{},\"selectedPrice\":{},\"selectedIndustry\":{},\
\"activeEntity\":{},\"activeName\":{},\"industries\":[{industries}],\"watchlist\":[{watchlist}],\"currencySymbol\":{},\"currencyCode\":{},\"moneyUnit\":{}",
        g.game_in_progress != 0, g.game_end_flag == "Y", js(&g.clock_state), g.ticker_speed,
        (5 - g.turn_action_count).max(0), g.sel_entity, js(&g.sel_corp_name), js(&g.ent_symbol[g.sel_entity]),
        json_str(&selected_price), g.sel_industry, g.active_entity, js(&g.active_entity_name),
        js(&g.currency_symbol), js(&g.currency_code), js(&g.million_prefix_uc));
    format!(
        "{{\"version\":{},\"year\":{},\"quarter\":{},\"startYear\":{},\"endYear\":{},\"currentPlayer\":{},\"players\":[{}],\
\"activeCorps\":{},\"stockIndex\":{},\"primeRate\":{},\"gdpGrowth\":{},\"topCorps\":[{}],\"news\":[{}],{desk},{readouts}}}",
        js(&g.version_str),
        g.cur_year,
        g.cur_quarter,
        g.start_year,
        g.game_end_year,
        g.cur_player,
        players.join(","),
        active,
        num(g.stock_index),
        num(g.prime_rate as f64),
        num(g.gdp_growth as f64),
        top.join(","),
        news.join(",")
    )
}

#[cfg(test)]
mod tests {
use wsr_core::pb::real::{Ext, cast};

    use super::*;
    #[test]
    fn desk_readouts_use_player_finances_without_changing_the_session() {
        crate::install_test_content();
        let mut g = Globals::default();
        g.num_players = 1;
        g.cur_player = 1;
        g.entity_status[1] = 1;
        g.lending_bank[1] = 11;
        g.cash[1] = Ext::from_f64(100.0);
        g.t_bills[1] = Ext::from_f64(25.0);
        g.bank_debt[1] = Ext::from_f64(10.0);
        g.net_worth = Ext::from_f64(98765.0);
        g.total_assets = Ext::from_f64(12345.0);
        g.cur_year = 2027;
        g.cur_quarter = 1;
        g.sim_co_count = 250;
        g.oil_price = cast::<f32, _>(Ext::from_f64(0.85));
        let before = g.to_json();
        let json = readouts_json(&g);
        assert!(json.contains("\"netWorth\":115"), "{json}");
        assert!(json.contains("\"totalAssets\":125"), "{json}");
        assert!(json.contains("\"gameDate\":\"January"), "{json}");
        assert!(json.contains("\"oil\":85"), "{json}");
        assert_eq!(g.to_json(), before);
    }

    #[test]
    fn selected_company_price_does_not_use_another_companys_scratch_price() {
        crate::install_test_content();
        let mut g = Globals::default();
        g.sel_entity = 11;
        g.market_cap[11] = Ext::from_f64(10.0);
        g.shares_outstanding[11] = Ext::from_f64(0.5);
        g.stock_price_str = PbStr::from("999.99");
        let json = summary_json(&g);
        assert!(json.contains("\"selectedPrice\":\"20.00\""), "{json}");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let content = wsr_core::content::current();
        assert_eq!(parsed["deskLabels"]["111"], content.string(10098));
        assert_eq!(parsed["newsHeading"][0], content.string(13898));
    }

    #[test]
    fn game_dates_match_month_ends_and_player_slots() {
        crate::install_test_content();
        for (year, quarter, progress, expected) in [
            (2027, 1, 499, "January 31, 2027"), (2027, 1, 500, "January 31, 2027"),
            (2027, 1, 501, "February 1, 2027"), (2027, 1, 1000, "February 28, 2027"),
            (2028, 1, 1000, "February 29, 2028"), (2027, 1, 1500, "March 31, 2027"),
            (2027, 1, 1600, "March 31, 2027"), (2027, 1, 1, "March 31, 2027"),
            (2027, 2, 500, "April 30, 2027"), (2027, 4, 1600, "December 31, 2027"),
        ] {
            let mut g = Globals::default();
            g.cur_year = year; g.cur_quarter = quarter; g.sim_co_count = progress; g.num_players = 2;
            g.tmp_ext1 = Ext::from_f64(123.0);
            let before = g.to_json();
            assert!(readouts_json(&g).contains(expected), "progress {progress}: {expected}");
            assert_eq!(g.to_json(), before);
        }
    }
}
