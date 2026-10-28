//! Browser safety guards, including cases the compatible original would loop on or serialize badly.
use wsr_core::pb::real::{Ext, cast};

use std::{sync::mpsc, time::Duration};
use wsr_core::{
    logic::{
        newgame::LocalTime,
        newgame_helpers,
        shell::{self, Env, Files},
    },
    pb::{pbstr::PbStr, rng::Rng},
    save,
    state::Globals,
    ui::{ReportAction, Ui},
    validation::validate_game,
};

#[derive(Default)]
struct Host {
    errors: Vec<String>,
    answer: Option<PbStr>,
}
impl Ui for Host {
    fn safety_checks(&self) -> bool {
        true
    }
    fn game_error(&mut self, message: &PbStr, _: &PbStr) {
        self.errors.push(message.to_display());
    }
    fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 {
        1
    }
    fn inputbox(&mut self, _: &PbStr, _: &PbStr, default: &PbStr) -> PbStr {
        self.answer.take().unwrap_or_else(|| default.clone())
    }
    fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
        ReportAction::Ok
    }
    fn news(&mut self, _: &PbStr) {}
}
fn install_content() {
    use wsr_core::content::{ContentPack, TextId};
    let entries = TextId::identifiers().enumerate().map(|(i, id)| (id, id.binary_length().map(|n| PbStr(vec![0; n])).unwrap_or_else(|| PbStr::from(format!("Synthetic resource {i:06}                         ").as_str()))));
    wsr_core::content::install(std::sync::Arc::new(ContentPack::from_entries(entries).unwrap()));
}
fn world() -> Globals {
    install_content();
    let mut g = Globals::default();
    g.version_str = "9.85".into();
    g.game_number = 1;
    g.game_in_progress = 1;
    g.num_players = 2;
    g.cur_player = 1;
    g.active_entity = 1;
    g.sel_entity = 1;
    g.player_turn_order[1] = 1;
    g.player_turn_order[2] = 2;
    g.entity_status[1] = 1;
    g.entity_status[2] = 1;
    g.start_year = 2024;
    g.cur_year = 2025;
    g.game_end_year = 2030;
    g.cur_quarter = 1;
    g.exchange_rate = cast::<f64, _>(Ext::from_f64(1.0));
    g.game_end_flag = "N".into();
    g.sim_co_count = 100;
    // A used ETF slot means this current-version tiny world needs no legacy ETF migration.
    g.entity_status[1581] = 2;
    g
}
fn bounded<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        install_content();
        let _ = send.send(work());
    });
    receive
        .recv_timeout(Duration::from_secs(5))
        .expect("bounded operation did not finish")
}

#[test]
fn finite_validation_names_scalar_and_column_major_array_cells() {
    let mut g = world();
    validate_game(&g).unwrap();
    g.bank_loans_total = Ext::INFINITY;
    assert!(validate_game(&g).unwrap_err().contains("BankLoansTotal"));
    g.bank_loans_total = Ext::from_f64(0.0);
    g.hold_stock_basis[(3, 200)] = Ext::NAN;
    assert!(
        validate_game(&g)
            .unwrap_err()
            .contains("HoldStockBasis(3, 200)")
    );
}

#[test]
fn references_counts_and_required_share_counts_are_checked() {
    let base = world();
    let mut g = base.clone();
    g.cur_player = 3;
    assert!(validate_game(&g).unwrap_err().contains("CurPlayer"));
    let mut g = base.clone();
    g.hold_stock_id[(1, 1)] = 2000;
    assert!(validate_game(&g).unwrap_err().contains("HoldStockId(1, 1)"));
    let mut g = base.clone();
    g.futures_recs[0] = "999999".into();
    assert!(validate_game(&g).unwrap_err().contains("FuturesRecs(0)"));
    let mut g = base.clone();
    g.option_recs[0] = "not a count".into();
    assert!(validate_game(&g).unwrap_err().contains("OptionRecs(0)"));
    let mut g = base;
    g.entity_status[131] = 1;
    g.industry_num[131] = 5;
    assert!(
        validate_game(&g)
            .unwrap_err()
            .contains("SharesOutstanding(131)")
    );
}

#[test]
fn first_bank_can_use_the_original_preceding_bank_lender() {
    let mut g = world();
    g.lending_bank[11] = 10;
    validate_game(&g).unwrap();
    g.lending_bank[131] = 10;
    assert!(validate_game(&g).unwrap_err().contains("LendingBank(131)"));
}

#[test]
fn convertible_bonds_require_a_usable_conversion_value() {
    let mut g = world();
    g.bonds_issued[131] = 100;
    for terms in ["2030 cv 04.00", "2030cv|0|@04.00", "2030cv|1e999|@04.00"] {
        g.bond_issue_info[131] = terms.into();
        assert!(
            validate_game(&g)
                .unwrap_err()
                .contains("BondIssueInfo(131)")
        );
    }
    g.bond_issue_info[131] = "2030cv|150.00|@04.00".into();
    validate_game(&g).unwrap();
    g.bond_issue_info[131] = "2030@00.00".into();
    validate_game(&g).unwrap(); // A non-convertible zero-coupon bond is valid.
}

#[test]
fn manual_save_save_as_and_autosave_keep_previous_files_on_invalid_state() {
    for mode in [0, 1, 2] {
        let mut g = world();
        g.cash[1] = Ext::NAN;
        let before = g.to_json();
        let mut rng = Rng::from_state(123456);
        let mut files = Files::default();
        files
            .files
            .insert("GAME01.DAT".into(), b"last good save".to_vec());
        let env = Env::new(&LocalTime::HARNESS).with_files(files);
        let mut ui = Host::default();
        assert!(shell::app::save_game_mode(&mut g, &mut ui, &mut rng, &env, mode).is_none());
        assert_eq!(g.to_json(), before);
        assert_eq!(rng.state(), 123456);
        assert!(env.files.borrow().written.is_empty());
        assert_eq!(
            env.files.borrow().read(b"GAME01.DAT").unwrap(),
            b"last good save"
        );
        assert!(ui.errors[0].contains("Cash(1)"));
    }
}

#[test]
fn save_rejects_overflow_created_during_financial_recalculation() {
    let mut g = world();
    g.entity_status[11] = 1;
    g.industry_num[11] = 1;
    g.shares_outstanding[11] = Ext::from_f64(1.0);
    g.market_cap[11] = Ext::from_f64(10.0);
    g.bank_demand_deposits[11] = Ext::MAX;
    g.bank_c_ds[11] = Ext::MAX;
    validate_game(&g).unwrap();
    let before = g.to_json();
    let mut rng = Rng::from_state(123456);
    let env = Env::skipping(&LocalTime::HARNESS, &[0x41ef3f]);
    let mut ui = Host::default();
    assert!(shell::app::save_game(&mut g, &mut ui, &mut rng, &env).is_none());
    assert!(env.files.borrow().written.is_empty());
    assert_eq!(g.to_json(), before);
    assert_eq!(rng.state(), 123456);
    assert!(ui.errors[0].contains("not finite"));
}

#[test]
fn rejected_load_keeps_game_rng_and_virtual_files() {
    let mut bad = world();
    bad.exchange_rate = f64::INFINITY;
    bad.name_list_dirty = 1;
    let bytes = save::write_save_file(
        &mut bad,
        &mut wsr_core::ui::Scripted::default(),
        &mut Rng::default(),
    );
    let mut g = world();
    g.cash[1] = Ext::from_f64(500.0);
    let before = g.to_json();
    let mut rng = Rng::from_state(123456);
    let env = Env::new(&LocalTime::HARNESS);
    env.files
        .borrow_mut()
        .write(b"LASTGAME.DAT", b"7\r\n".to_vec());
    let written = env.files.borrow().written.clone();
    assert!(
        shell::app::load_game(&mut g, &mut Host::default(), &mut rng, &env, &bytes, false).is_err()
    );
    assert_eq!(g.to_json(), before);
    assert_eq!(rng.state(), 123456);
    assert_eq!(env.files.borrow().written, written);
}

#[test]
fn rejected_menu_load_keeps_the_current_save_slot() {
    for data in [None, Some(b"not a saved game".to_vec())] {
        let mut g = world();
        let before = g.to_json();
        let mut rng = Rng::from_state(123456);
        let env = Env::new(&LocalTime::HARNESS);
        if let Some(data) = data {
            env.files
                .borrow_mut()
                .files
                .insert("GAME07.DAT".into(), data);
        }
        let mut ui = Host {
            answer: Some("7".into()),
            ..Host::default()
        };
        shell::app::menu_open_saved_game(&mut g, &mut ui, &mut rng, &env);
        assert_eq!(g.to_json(), before);
        assert_eq!(rng.state(), 123456);
        assert_eq!(ui.errors.len(), 1);
        assert!(ui.errors[0].contains("GAME07.DAT"));
        assert!(env.files.borrow().written.is_empty());
    }
}

#[test]
fn etf_picker_counts_inactive_draws_toward_its_limit() {
    bounded(|| {
        let mut g = Globals::default();
        let mut rng = Rng::from_state(123456);
        assert_eq!(
            newgame_helpers::pick_etf_holding_stock(
                &mut g,
                &mut Host::default(),
                &mut rng,
                1593,
                0
            ),
            0
        );
        let mut expected = Rng::from_state(123456);
        for _ in 0..2000 {
            expected.rnd_range(1580, 11);
        }
        assert_eq!(rng.state(), expected.state());
    });
}

#[test]
fn etf_setup_rejects_empty_and_insufficient_country_pools_without_mutation() {
    for candidates in [0, 14] {
        bounded(move || {
            let mut g = world();
            for fund in 1581..=1600 {
                g.entity_status[fund] = 2;
            }
            g.entity_status[1593] = 0;
            for company in 131..131 + candidates {
                g.entity_status[company] = 1;
                g.industry_num[company] = 5;
                g.country_idx[company] = 8;
                g.shares_outstanding[company] = Ext::from_f64(1.0);
            }
            let before = g.to_json();
            let mut rng = Rng::from_state(123456);
            let mut ui = Host::default();
            assert!(!newgame_helpers::init_etfs(&mut g, &mut ui, &mut rng));
            assert_eq!(rng.state(), 123456);
            assert_eq!(g.to_json(), before);
            assert!(ui.errors[0].contains(&format!("only {candidates} are available")));
        });
    }
}

#[test]
fn etf_setup_accepts_fifteen_distinct_eligible_holdings() {
    bounded(|| {
        let mut g = world();
        for fund in 1581..=1600 {
            g.entity_status[fund] = 2;
        }
        g.entity_status[1593] = 0;
        for company in 131..146 {
            g.entity_status[company] = 1;
            g.industry_num[company] = 5;
            g.country_idx[company] = 8;
            g.shares_outstanding[company] = Ext::from_f64(1.0);
            g.market_cap[company] = Ext::from_f64(10.0);
        }
        let mut ui = Host::default();
        assert!(
            newgame_helpers::init_etfs(&mut g, &mut ui, &mut Rng::from_state(123456)),
            "{:?}",
            ui.errors
        );
        let holdings: std::collections::BTreeSet<_> =
            (1..=15).map(|slot| g.hold_stock_id[(slot, 1593)]).collect();
        assert_eq!(holdings.len(), 15);
        assert!(!holdings.contains(&0));
    });
}
