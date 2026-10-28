//! Differential tests of logic::corp_finance::loans (borrowing, repaying, the sweep setting, changing
//! the lending bank, tax prepayment, bank loan trading and its type dialog).
//!
//! As in the lending tests, a scripted Ui ([`Drv`]) answers every MSGBOX / INPUTBOX / pick list / dialog
//! and logs the equivalent harness answers in call order (`IDYES`, the entered text, `dblclick:301:N` on
//! the report list box, `text:310:SYM` + `click:1` in the symbol-entry dialog, `click:499..506` +
//! `click:1` in the loan-type dialog); the original is then run with that log as its answer queue. A
//! pick in the loan list is followed by the report dialog (`FollowReportLink`).
#![allow(unused_imports, clippy::type_complexity, clippy::too_many_arguments)]
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use wsr_core::logic::corp_finance::loans::*;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const REPAY_BANK_LOAN: u32 = 0x46feda;
const BORROW_FROM_BANK: u32 = 0x46ef51;
const MENU_TOGGLE_SWEEP_CASH: u32 = 0x40d9d5;
const CHANGE_LENDING_BANK: u32 = 0x59bd8a;
const PREPAY_INCOME_TAX: u32 = 0x5c1974;
const BUY_SELL_LOANS: u32 = 0x58a220;
const SHOW_LOAN_TRADE_TYPE_DIALOG: u32 = 0x5b856f;

/// Scripted Ui that logs the harness answers of everything it answers.
#[derive(Default)]
struct Drv {
    /// MSGBOX answers (default YES / OK).
    boxes: VecDeque<i32>,
    /// INPUTBOX / symbol-entry answers (default "" = cancel).
    inputs: VecDeque<String>,
    /// Pick-list choices: `Some(k)` = the k-th (mod count) link line, `None` = no pick (OK).
    picks: VecDeque<Option<usize>>,
    /// Loan-type dialog choices (`None` = CANCEL).
    choices: VecDeque<Option<usize>>,
    /// Harness answer queue for the original.
    log: Vec<String>,
    msgs: Vec<String>,
    news: Vec<String>,
    /// (link id, list index) of every pick made.
    picked: Vec<(i32, usize)>,
}

impl Ui for Drv {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(text.to_display());
        if style == 0 && title.is_empty() {
            return 1; // ShowQuickMessage (qm.exe in the original)
        }
        let yes_no = style & 0xf == 3 || style & 0xf == 4;
        let a = self.boxes.pop_front().unwrap_or(if yes_no { 6 } else { 1 });
        self.log.push(match a {
            1 => "IDOK".into(),
            2 => "IDCANCEL".into(),
            6 => "IDYES".into(),
            7 => "IDNO".into(),
            n => n.to_string(),
        });
        a
    }
    fn inputbox(&mut self, prompt: &PbStr, _title: &PbStr, _default: &PbStr) -> PbStr {
    let __content = wsr_core::content::current();
        let a = self.inputs.pop_front().unwrap_or_default();
        if prompt.to_display().ends_with(__content.string(6717)) {
            // InBox(3): the symbol-entry dialog (edit 310, OK 1, Cancel 2)
            if a.is_empty() {
                self.log.push("click:2".into());
            } else {
                self.log.push(format!("text:310:{a}"));
                self.log.push("click:1".into());
            }
        } else {
            self.log.push(a.clone());
        }
        PbStr::from(a.as_str())
    }
    fn report(&mut self, _title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
    let __content = wsr_core::content::current();
        let links: Vec<(usize, i32)> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let t = l.to_display();
                let p = t.rfind("@C")?;
                let id: i32 = t[p + 2..].trim().get(..4)?.parse().ok()?;
                Some((i, id))
            })
            .collect();
        if links.is_empty() {
            return ReportAction::Ok;
        }
        if let Some(Some(k)) = self.picks.pop_front() {
            let (idx, id) = links[k % links.len()];
            self.log.push(format!("dblclick:301:{idx}"));
            self.picked.push((id, idx));
            return ReportAction::Link(PbStr::from(format!("{__wsr_text_13538}{id:04}",__wsr_text_13538=__content.string(13538)).as_str()));
        }
        ReportAction::Ok
    }
    fn choose(&mut self, _title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        match self.choices.pop_front().flatten() {
            Some(k) => {
                let k = k % options.len();
                self.log.push(format!("click:{}", 499 + k));
                self.log.push("click:1".into());
                Some(k)
            }
            None => {
                self.log.push("click:508".into());
                None
            }
        }
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
}


fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x5eed_10a9
}

/// Runs the original with the driver's answer log and compares state, RND and message-box count.
#[track_caller]
fn compare(
    addr: u32,
    args: &[&str],
    g: &Globals,
    port: &Globals,
    rng: &Rng,
    ui: &Drv,
    seed: u64,
    what: &str,
) -> Option<Original> {
    let __content = wsr_core::content::current();
    let answers: Vec<&str> = ui.log.iter().map(|s| s.as_str()).collect();
    let o = run(addr, args, g, rng0(seed), &answers, &[])?;
    cmp()
        .ignore("TextLines")
        .ignore("TextBoxTitle")
        .ignore("hLoanTradeDlg")
        .ignore("DlgSymbolEntry")
        .abs_tol(Ext::from_f64(1e-9))
        .assert(port, &o.globals);
    assert_eq!(rng.state(), o.rng, "RND state after {what} (seed {seed})");
    assert_eq!(
        ui.msgs.len(),
        orig_msgs(&o),
        "message boxes of {what} (seed {seed}): {:?}\n{:?}",
        ui.msgs,
        o.log
    );
    for (id, idx) in &ui.picked {
        let hit = o.dialogs.iter().any(|d| {
            d.control(301).is_some_and(|c| {
                c.items
                    .get(*idx)
                    .is_some_and(|t| t.to_display().contains(&format!("{__wsr_text_13538}{id:04}",__wsr_text_13538=__content.string(13538))))
            })
        });
        assert!(
            hit,
            "pick of {id} at list index {idx} not found in the original's list boxes ({what}, seed {seed})"
        );
    }
    Some(o)
}

/// A world with the borrower/lender relations the loan actions look at; the player's ETF 1581 is advised
/// by an insurer he controls.
fn world_l(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0x2545_f491_4f6c_dd1d) | 1);
    g.sel_entity = 0;
    g.last_sel_entity = 0;
    g.cur_player = 1;
    g.is_computer_player[1] = 0;
    g.borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.entity_borrow_rate = cast::<f32, _>(f32r(&mut r, Ext::from_f64(3.0), Ext::from_f64(14.0)));
    g.currency_suffix = PbStr::new();
    g.app_title = st(__content.string(13661));
    g.central_bank_name = st(__content.string(4373));
    g.turn_action_count = cast::<i32, _>(r.range(0, 3));
    g.solo_human_mode = cast::<i32, _>(r.range(0, 1));
    g.subprime_crisis_state = cast::<i16, _>(r.range(-2, 2));
    for e in 1..=1600 {
        g.credit_frozen[e] = if r.chance(Ext::from_f64(0.15)) { 1 } else { 0 };
    }
    for e in 11..=85 {
        g.bank_largest_debtor[e] = if r.chance(Ext::from_f64(0.3)) {
            cast::<i32, _>(r.range(86, 1600))
        } else {
            0
        };
        g.subprime_secs[e] = if r.chance(Ext::from_f64(0.6)) { r.range(0, 8000) } else { 0 };
    }
    for p in 1..=5 {
        g.player_loan_offers_avail[p] = cast::<i32, _>(r.range(0, 4));
        g.sweep_cash_to_loan[p] = cast::<i32, _>(r.range(0, 1));
    }
    // the player's fund 1581 and its adviser
    let adviser = cast::<i32, _>(r.range(86, 130));
    g.entity_status[adviser] = 1;
    g.industry_num[adviser] = 2;
    g.corp_controller[adviser] = 1;
    g.entity_status[1581] = 1;
    g.fund_adviser[1581] = adviser;
    g.ctrl_since_year[1581] = g.cur_year - 4;
    g
}

/// A corp the player controls (or the player himself) as the acting entity.
fn act_as(g: &mut Globals, e: i32) {
    g.entity_status[e] = 1;
    if e > 10 {
        g.corp_controller[e] = 1;
    }
    g.active_entity = e;
    g.active_entity_name = g.ent_name[e].clone();
}

// --------------------------------------------------------------------------------- sweep, prepay tax

#[test]
fn diff_menu_toggle_sweep_cash() {
    let __content = wsr_core::content::current();
    par_seeds(1..=6, |seed| {
        let mut g = world_l(seed);
        g.cur_player = if seed % 3 == 0 { 0 } else { 1 };
        g.cur_sweep_cash = cast::<i32, _>(seed % 2);
        act_as(&mut g, 1);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        let cfg = menu_toggle_sweep_cash(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = compare(
            MENU_TOGGLE_SWEEP_CASH,
            &[],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            "MenuToggleSweepCash",
        ) else {
            return;
        };
        let orig = o
            .files
            .get("CFIG.WSR")
            .cloned()
            .flatten()
            .expect(__content.string(13719));
        assert_eq!(
            String::from_utf8_lossy(&cfg),
            String::from_utf8_lossy(&orig),
            "CFIG.WSR (seed {seed})"
        );
    });
}

#[test]
fn diff_prepay_income_tax() {
    par_seeds(1..=12, |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x77);
        act_as(&mut g, 1);
        if seed % 4 == 0 {
            g.cash[1] = Ext::from_f64(0.0);
            g.t_bills[1] = Ext::from_f64(0.0);
        }
        if seed == 3 {
            g.active_entity = 2; // not the current player: nothing happens
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        let amt = match seed % 4 {
            1 => f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(200.0)),
            2 => f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(5000.0)),
            _ => f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(3000.0)),
        };
        ui.inputs = [format!("{amt:.2}"), "1".into(), "".into()]
            .into_iter()
            .collect();
        ui.boxes = [6, 7][cast::<usize, _>(seed % 2)..].iter().copied().collect();
        prepay_income_tax(&mut port, &mut ui, &mut rng, 0);
        compare(
            PREPAY_INCOME_TAX,
            &[],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            "PrepayIncomeTax",
        );
    });
}

// ------------------------------------------------------------------------------------ borrow, repay

#[test]
fn diff_borrow_from_bank() {
    let __content = wsr_core::content::current();
    par_seeds(1..=16, |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x99);
        let ae = match seed % 4 {
            0 => 1,
            1 => cast::<i32, _>(r.range(131, 1580)),
            2 => cast::<i32, _>(r.range(86, 130)),
            _ => cast::<i32, _>(r.range(11, 85)),
        };
        act_as(&mut g, ae);
        g.sel_entity = match seed % 5 {
            0 => 1581,
            1 => cast::<i32, _>(r.range(131, 1580)),
            _ => ae,
        };
        g.credit_rating_num[ae] = cast::<i32, _>(r.range(0, 10));
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.boxes = match seed % 3 {
            0 => [6, 6].into(),
            1 => [7, 6].into(),
            _ => [6, 2].into(),
        };
        let amt = if seed % 6 == 0 {
            Ext::from_f64(3.0)
        } else {
            f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(400.0))
        };
        ui.inputs = ["999999".into(), format!("{amt:.2}")].into_iter().collect();
        let cfg = borrow_from_bank(&mut port, &mut ui, &mut rng, 0);
        let Some(o) = compare(
            BORROW_FROM_BANK,
            &[],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            "BorrowFromBank",
        ) else {
            return;
        };
        if let Some(cfg) = cfg {
            let orig = o
                .files
                .get("CFIG.WSR")
                .cloned()
                .flatten()
                .expect(__content.string(13719));
            assert_eq!(
                String::from_utf8_lossy(&cfg),
                String::from_utf8_lossy(&orig),
                "CFIG.WSR (seed {seed})"
            );
        }
    });
}

#[test]
fn diff_repay_bank_loan() {
    par_seeds(1..=24, |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x55);
        let ae = match seed % 4 {
            0 => 1,
            1 | 2 => cast::<i32, _>(r.range(131, 1580)),
            _ => cast::<i32, _>(r.range(86, 130)),
        };
        act_as(&mut g, ae);
        g.sel_entity = match seed % 7 {
            0 => 1581,
            1 => cast::<i32, _>(r.range(131, 1580)),
            _ => ae,
        };
        if seed % 3 == 0 {
            g.difficulty_level = 3;
        }
        if ae > 10 {
            for p in 1..=g.num_players {
                if r.chance(Ext::from_f64(0.6)) {
                    g.player_advances[(p, ae)] = f64r(&mut r, Ext::from_f64(0.5), Ext::from_f64(300.0));
                }
            }
            g.credit_rating_num[ae] = cast::<i32, _>(r.range(3, 10));
        }
        if seed % 5 == 0 {
            g.bank_debt[ae] = Ext::from_f64(0.0);
        }
        let mode = cast::<i32, _>(seed % 3);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.boxes = [[6, 6], [7, 6], [6, 7]][cast::<usize, _>(seed % 3)].into();
        ui.inputs = (0..4)
            .map(|_| format!("{:.2}", f64r(&mut r, Ext::from(-Ext::from_f64(5.0)), Ext::from(Ext::from_f64(600.0)))))
            .collect();
        repay_bank_loan(&mut port, &mut ui, &mut rng, 0, mode);
        compare(
            REPAY_BANK_LOAN,
            &[&mode.to_string()],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            &format!("RepayBankLoan({mode})"),
        );
    });
}

// ---------------------------------------------------------------------------------- change the bank

#[test]
fn diff_change_lending_bank() {
    par_seeds(1..=20, |seed| {
        let mut g = world_l(seed);
        let mut r = Rand(seed ^ 0x33);
        let ae = match seed % 3 {
            0 => 1,
            _ => cast::<i32, _>(r.range(131, 1580)),
        };
        act_as(&mut g, ae);
        if seed % 4 == 1 {
            g.bank_debt[ae] = Ext::from_f64(0.0);
        }
        if seed % 2 == 0 {
            g.credit_frozen[ae] = 0;
            let lender = cast::<i32, _>(g.lending_bank[ae]);
            g.corp_controller[lender] = cast::<i32, _>(r.range(131, 1600));
        }
        if seed % 6 == 0 {
            g.cash[ae] = Ext::from_f64(50_000.0);
        }
        let new_bank = cast::<i32, _>(r.range(11, 85));
        g.entity_status[new_bank] = 1;
        let entry = match seed % 5 {
            0 => String::new(),
            1 => g.ent_symbol[new_bank].to_display(),
            2 => r.range(131, 1580).to_string(),
            _ => new_bank.to_string(),
        };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.inputs = [entry].into();
        change_lending_bank(&mut port, &mut ui, &mut rng);
        compare(
            CHANGE_LENDING_BANK,
            &[],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            "ChangeLendingBank",
        );
    });
}

// ------------------------------------------------------------------------------------- loan trading

/// A bank of the player as the acting entity, with business loans concentrated in a few banks (so that
/// some hold more than 10) controlled by computer corps.
fn world_bank(seed: u64) -> Globals {
    let mut g = world_l(seed);
    let mut r = Rand(seed ^ 0x1234_5678);
    let b = cast::<i32, _>(r.range(11, 85));
    act_as(&mut g, b);
    g.cash[b] = f64r(&mut r, Ext::from_f64(500.0), Ext::from_f64(5000.0));
    g.bank_c_ds[b] = f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(60000.0));
    g.bank_demand_deposits[b] = f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(60000.0));
    g.player_loan_offers_avail[1] = cast::<i32, _>(r.range(1, 4));
    for e in 86..=1580 {
        if r.chance(Ext::from_f64(0.7)) {
            g.lending_bank[e] = cast::<i16, _>(r.range(11, 25));
        }
        if cast::<i32, _>(g.lending_bank[e]) == b && r.chance(Ext::from_f64(0.5)) {
            g.bank_debt[e] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(300.0));
        }
    }
    for k in 11..=25 {
        if k != b && r.chance(Ext::from_f64(0.8)) {
            g.entity_status[k] = 1;
            g.corp_controller[k] = cast::<i32, _>(r.range(131, 1600));
        }
    }
    g
}

#[test]
fn diff_show_loan_trade_type_dialog() {
    par_seeds(1..=3, |seed| {
        let g = world_l(seed);
        let mut port = g.clone();
        let rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.choices = [[Some(2), None, Some(7)][cast::<usize, _>(seed - 1)]].into();
        show_loan_trade_type_dialog(&mut port, &mut ui, 0);
        compare(
            SHOW_LOAN_TRADE_TYPE_DIALOG,
            &["v:0"],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            "ShowLoanTradeTypeDialog",
        );
    });
}

/// Types 2..4 and 6..8 (buying / selling loan pools), some through the dialog.
#[test]
fn diff_buy_sell_loan_pools() {
    par_seeds(1..=18, |seed| {
        let mut g = world_bank(seed);
        let mut r = Rand(seed ^ 0x4242);
        let tt = [2, 3, 4, 6, 7, 8][cast::<usize, _>(seed % 6)];
        let requested = if seed % 4 == 0 { 0 } else { tt };
        if seed % 5 == 0 {
            g.bank_consumer_loans[g.active_entity] = 0;
        }
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        ui.choices = [Some(cast::<usize, _>(tt - 1))].into();
        ui.boxes = [if seed % 3 == 0 { 7 } else { 6 }].into();
        let amt = match seed % 3 {
            0 => f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(500.0)),
            1 => Ext::from_f64(99_999_999.0),
            _ => f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(3000.0)),
        };
        ui.inputs = [
            format!("{amt:.0}"),
            format!("{:.0}", f64r(&mut r, Ext::from(Ext::from_f64(1.0)), Ext::from(Ext::from_f64(400.0)))),
        ]
        .into();
        buy_sell_loans(&mut port, &mut ui, &mut rng, requested);
        compare(
            BUY_SELL_LOANS,
            &[&requested.to_string()],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            &format!("BuySellLoans({requested}) type {tt}"),
        );
    });
}

/// Type 1 (buy a business loan) by search and from the list, type 5 (sell one) from the list.
#[test]
fn diff_buy_sell_business_loans() {
    let listed = std::sync::atomic::AtomicUsize::new(0);
    par_seeds(1..=24, |seed| {
        let g = world_bank(seed);
        let mut r = Rand(seed ^ 0x5151);
        let tt = if seed % 3 == 0 { 5 } else { 1 };
        let mut port = g.clone();
        let mut rng = Rng::from_state(rng0(seed));
        let mut ui = Drv::default();
        // type 1: list? (YES on even seeds), sort by rating?, buy another?
        ui.boxes = if tt == 1 {
            [
                if seed % 2 == 0 { 6 } else { 7 },
                if seed % 4 == 0 { 6 } else { 7 },
                6,
                7,
            ]
            .into()
        } else {
            [6, 7].into()
        };
        ui.picks = [
            Some(cast::<usize, _>(r.range(0, 500))),
            Some(cast::<usize, _>(r.range(0, 500))),
        ]
        .into();
        ui.inputs = (0..4)
            .map(|_| format!("{:.0}", f64r(&mut r, Ext::from(Ext::from_f64(5.0)), Ext::from(Ext::from_f64(400.0)))))
            .collect();
        buy_sell_loans(&mut port, &mut ui, &mut rng, tt);
        if !ui.picked.is_empty() {
            listed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        compare(
            BUY_SELL_LOANS,
            &[&tt.to_string()],
            &g,
            &port,
            &rng,
            &ui,
            seed,
            &format!("BuySellLoans({tt})"),
        );
    });
    assert!(listed.into_inner() > 0, "no loan was picked from a list");
}
