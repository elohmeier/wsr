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
