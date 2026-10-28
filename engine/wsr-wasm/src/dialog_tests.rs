use wsr_core::{
    pb::{pbstr::PbStr, rng::Rng},
    state::Globals,
    ui::{Form, FormResponse, ReportAction, Ui},
};

struct Forms {
    calls: usize,
    edit: Box<dyn FnMut(usize, &Form) -> FormResponse>,
}
impl Ui for Forms {
    fn form(&mut self, form: &Form) -> Option<FormResponse> {
        self.calls += 1;
        Some((self.edit)(self.calls, form))
    }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr {
        panic!("A grouped form must not ask individual inputs")
    }
    fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 {
        7
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
}

#[derive(Default)]
struct SearchUi {
    calls: usize,
    topics: Vec<String>,
}
impl Ui for SearchUi {
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr {
        panic!("Database fields must stay in the search screen")
    }
    fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 {
        1
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
    fn help(&mut self, topic: &str) {
        self.topics.push(topic.into());
    }
    fn database_search(
        &mut self,
        view: &wsr_core::logic::info_screens::db_search::DbSearchView,
    ) -> Option<Vec<wsr_core::logic::info_screens::db_search::DbSearchAction>> {
        use wsr_core::logic::info_screens::db_search::{DbSearchAction as Action, ctl};
        self.calls += 1;
        assert_eq!(view.criteria.len(), 19);
        assert_eq!(view.criteria.iter().flat_map(|c| &c.fields).count(), 7);
        let pe = view
            .criteria
            .iter()
            .find(|c| c.id == ctl::CHK_LOW_PE)
            .unwrap();
        match self.calls {
            1 => Some(vec![
                Action::Edit(ctl::TXT_MAX_PE, PbStr::from("25")),
                Action::SetChecked(ctl::CHK_LOW_PE, true),
                Action::Click(ctl::BTN_MGMT_RATING),
            ]),
            2 => {
                assert!(pe.checked);
                assert_eq!(pe.fields[0].value, "25");
                let management = view
                    .criteria
                    .iter()
                    .find(|c| c.id == ctl::CHK_MGMT_RATING)
                    .unwrap();
                assert!(management.checked);
                assert_eq!(management.change.as_ref().unwrap().value, "Average");
                Some(vec![Action::Click(ctl::BTN_HELP)])
            }
            3 => {
                assert_eq!(self.topics, ["database search"]);
                assert!(pe.checked);
                assert_eq!(pe.fields[0].value, "25");
                Some(vec![Action::Click(ctl::BTN_CLEAR)])
            }
            4 => {
                assert!(view.criteria.iter().all(|c| !c.checked));
                Some(vec![Action::Close])
            }
            _ => panic!("Unexpected database dialog"),
        }
    }
}

#[test]
fn database_screen_uses_original_edit_checkbox_rating_help_and_clear_callbacks() {
    crate::install_test_content();
    let mut g = Globals::default();
    g.cur_player = 1;
    g.active_entity = 1;
    g.num_players = 2;
    let mut ui = SearchUi::default();
    wsr_core::logic::info_screens::db_search::show_db_search_dialog(
        &mut g,
        &mut ui,
        &mut Rng::from_state(1),
    );
    assert_eq!(ui.calls, 4);
    assert_eq!(g.dlg_open_db_search, 0);
    wsr_core::logic::trade_screens::ir_swaps::ir_swap_help_click(&g, &mut ui);
    assert_eq!(ui.topics, ["database search", "interest rate swaps"]);
}

#[derive(Default)]
struct ChoicesUi {
    selectors: Vec<(usize, String)>,
    commands: usize,
    disabled_seen: bool,
}
impl Ui for ChoicesUi {
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, _: &PbStr) -> PbStr {
        panic!("No transaction was confirmed")
    }
    fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 {
        1
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
    fn choose(&mut self, _: &PbStr, _: &[PbStr], _: &[PbStr]) -> Option<usize> {
        panic!("Use the original control type")
    }
    fn select(
        &mut self,
        _: &PbStr,
        _: &[PbStr],
        options: &[PbStr],
        _: Option<usize>,
        accept: &PbStr,
    ) -> Option<usize> {
        self.selectors.push((options.len(), accept.to_display()));
        None
    }
    fn choose_enabled(
        &mut self,
        _: &PbStr,
        _: &[PbStr],
        options: &[PbStr],
        enabled: &[bool],
    ) -> Option<usize> {
        self.commands += 1;
        assert_eq!(options.len(), 8);
        match self.commands {
            1 => Some(0), // Select oil.
            2 => Some(5), // Switch to physical trading.
            3 => {
                assert!(!enabled[2] && !enabled[3]);
                assert!(enabled[0] && enabled[1] && enabled[7]);
                self.disabled_seen = true;
                None
            }
            _ => panic!("Unexpected commodity desk command"),
        }
    }
}

#[test]
fn all_seven_selectors_require_confirmation_and_cancel_without_starting_a_transaction() {
    crate::install_test_content();
    use wsr_core::logic::{
        corp_finance, info_screens, newgame::LocalTime, restructure, shell, trade_screens,
    };
    let mut g = Globals::default();
    g.cur_player = 1;
    g.active_entity = 1;
    g.num_players = 2;
    let mut ui = ChoicesUi::default();
    let mut rng = Rng::from_state(1);
    let env = shell::Env::new(&LocalTime::HARNESS);
    shell::dialogs::show_govt_bond_dialog(&mut g, &mut ui, &mut rng, &env);
    shell::dialogs::show_who_owns_what_dialog(&mut g, &mut ui, &mut rng, &env);
    corp_finance::loans::show_loan_trade_type_dialog(&mut g, &mut ui, 0);
    restructure::capital::show_cap_contrib_dialog(&mut g, &mut ui, 0);
    trade_screens::option_station::strategy::show_option_strategy_picker(&mut g, &mut ui, 0);
    trade_screens::alerts_alloc::dlg_price_alert_type(&mut g, &mut ui, 0);
    info_screens::cheat::dlg_cheat_menu(&mut g, &mut ui, &mut rng, 0, 0);
    assert_eq!(
        ui.selectors,
        [
            (4, "Go".into()),
            (6, "Go".into()),
            (8, "Go".into()),
            (4, "Apply".into()),
            (10, "Go".into()),
            (13, "Apply".into()),
            (4, "Go".into())
        ]
    );
    assert_eq!(g.turn_action_count, 0);
}

#[test]
fn commodity_desk_keeps_short_controls_visible_and_disabled_in_physical_mode() {
    crate::install_test_content();
    let mut g = Globals::default();
    g.cur_player = 1;
    g.active_entity = 1;
    g.num_players = 2;
    g.start_year = 2026;
    g.cur_year = 2027;
    g.exchange_rate = 1.into();
    g.cash[1] = 5000.into();
    let mut ui = ChoicesUi::default();
    wsr_core::logic::trade_screens::commod_crypto::commod_desk::show_commodity_desk(
        &mut g,
        &mut ui,
        &mut Rng::from_state(1),
        0,
    );
    assert!(ui.disabled_seen);
    assert_eq!(g.turn_action_count, 0);
}

#[test]
fn startup_preserves_all_fields_on_invalid_input_and_declined_long_game() {
    crate::install_test_content();
    let mut g = Globals::default();
    g.num_players = 5;
    g.has_cpu_players = PbStr::from("Y");
    for k in [1, 3, 4, 5] {
        g.is_computer_player[k] = 1;
    }
    let mut ui = Forms {
        calls: 0,
        edit: Box::new(|call, form| {
            assert_eq!(form.fields.len(), 8);
            assert_eq!(form.fields[0].max_length, Some(20));
            let mut values: Vec<_> = form.fields.iter().map(|f| f.value.clone()).collect();
            match call {
                1 => {
                    for (i, value) in values[..5].iter_mut().enumerate() {
                        *value = PbStr::from(format!("Player {}", i + 1).as_str());
                    }
                    values[1] = PbStr::new();
                    values[5] = PbStr::from("800");
                    values[6] = PbStr::from("50");
                    values[7] = PbStr::from("4");
                }
                2 => {
                    assert_eq!(values[1], "");
                    assert_eq!(values[4], "Player 5");
                    assert_eq!(values[5], "800");
                    values[1] = PbStr::from("Human player");
                }
                3 => {
                    assert_eq!(values[6], "35");
                    assert_eq!(values[7], "4");
                }
                _ => panic!("Unexpected form retry"),
            }
            FormResponse { button: 1, values }
        }),
    };
    assert!(
        wsr_core::logic::newgame::show_startup_dialog(&mut g, &mut ui, &mut Rng::from_state(3))
            .is_some()
    );
    assert_eq!(ui.calls, 3);
    assert_eq!(g.start_net_worth, 800);
    assert_eq!(g.cfg_game_length_years, 35);
    assert_eq!(g.cfg_difficulty_raw, 4);
    assert_eq!(g.player_name[2], "Human player");
}

#[test]
fn startup_cancel_does_not_accept_defaults() {
    crate::install_test_content();
    let mut g = Globals::default();
    g.num_players = 2;
    let mut ui = Forms {
        calls: 0,
        edit: Box::new(|_, _| FormResponse {
            button: -1,
            values: vec![],
        }),
    };
    assert!(
        wsr_core::logic::newgame::show_startup_dialog(&mut g, &mut ui, &mut Rng::from_state(3))
            .is_none()
    );
    assert_eq!(g.start_net_worth, 0);
    assert_eq!(g.game_in_progress, 0);
}

#[test]
fn symbol_lookup_filters_active_banks_bond_issuers_and_controlled_companies() {
    let mut g = Globals::default();
    g.cur_player = 2;
    for (id, name, symbol) in [
        (11, "Zeta Bank", "ZB"),
        (12, "Defunct", "OLD"),
        (86, "Alpha Insurer", "AI"),
        (131, "Beta Corp", "BC"),
    ] {
        g.entity_status[id] = if id == 12 { 2 } else { 1 };
        g.ent_name[id] = PbStr::from(name);
        g.ent_symbol[id] = PbStr::from(symbol);
    }
    g.bonds_issued[131] = 1;
    g.corp_controller[86] = 2;
    let ids = |g: &Globals| {
        wsr_core::logic::trading::select::symbol_lookup_choices(g)
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&g), [86, 131, 11]);
    g.lookup_filter = PbStr::from("BANKS");
    assert_eq!(ids(&g), [11]);
    g.lookup_filter = PbStr::from("BONDS");
    assert_eq!(ids(&g), [131]);
    g.lookup_filter = PbStr::from("CTRL");
    assert_eq!(ids(&g), [86]);
}

#[test]
fn bank_allocation_applies_all_fields_in_one_submission_and_cancel_preserves_allocations() {
    crate::install_test_content();
    let mut g = Globals::default();
    g.active_entity = 11;
    g.bank_alloc_help_plr = 1;
    let mut ui = Forms {
        calls: 0,
        edit: Box::new(|call, form| {
            assert_eq!(form.fields.len(), 5);
            let values = ["20", "10", "30", "15", "25"].map(PbStr::from).to_vec();
            FormResponse {
                button: if call == 1 { 1055 } else { 1 },
                values,
            }
        }),
    };
    wsr_core::logic::trade_screens::alerts_alloc::show_dlg_bank_asset_alloc(
        &mut g,
        &mut ui,
        &mut Rng::from_state(1),
        0,
    );
    assert_eq!(ui.calls, 2);
    assert_eq!(g.bank_alloc_prime_mort_pct[11], 20);
    assert_eq!(g.bank_alloc_cash_pct[11], 25);
    ui.edit = Box::new(|_, form| FormResponse {
        button: -1,
        values: form.fields.iter().map(|_| PbStr::from("99")).collect(),
    });
    wsr_core::logic::trade_screens::alerts_alloc::show_dlg_bank_asset_alloc(
        &mut g,
        &mut ui,
        &mut Rng::from_state(1),
        0,
    );
    assert_eq!(g.bank_alloc_prime_mort_pct[11], 20);
    assert_eq!(g.bank_alloc_cash_pct[11], 25);
}

#[test]
fn swap_rate_refresh_preserves_notional_dates_and_position() {
    crate::install_test_content();
    let mut g = Globals::default();
    g.active_entity = 1;
    g.cur_player = 1;
    g.num_players = 2;
    g.exchange_rate = 1.into();
    g.cash[1] = 5000.into();
    g.prime_rate = 8.0;
    g.long_bond_yield = 6.0;
    let mut ui = Forms {
        calls: 0,
        edit: Box::new(|call, form| {
            assert_eq!(form.fields.len(), 7);
            assert!(form.fields[0].refresh_on_change);
            let values = ["L", "LONG", "1,234", "2", "2030", "4", "2032"]
                .map(PbStr::from)
                .to_vec();
            if call == 2 {
                assert_eq!(
                    form.fields
                        .iter()
                        .map(|f| f.value.clone())
                        .collect::<Vec<_>>(),
                    values
                );
            }
            FormResponse {
                button: if call == 1 { 0 } else { 998 },
                values,
            }
        }),
    };
    wsr_core::logic::trade_screens::ir_swaps::show_ir_swap_dialog(
        &mut g,
        &mut ui,
        &mut Rng::from_state(1),
        0,
    );
    assert_eq!(ui.calls, 2);
    assert_eq!(g.swap_rate_type, "L");
    assert_eq!(g.swap_position, "LONG");
    assert_eq!(g.dlg_open_swap_dlg, 0);
}
