//! Shared helpers of the ai_player tests: a world with a computer player to move, and running
//! AiComputerPlayerTurn (whole or single steps) in the port and the original.
#![allow(dead_code, unused_imports)]
use wsr_core::pb::real::{Ext, cast};


pub use super::diff::{Compare, Opts, Original, Rand, par_seeds, run_original_with};
pub use super::sim_common::{Rec, cmp, f32r, f64r, orig_msgs, run, st, word, world};
use wsr_core::logic::ai_player::ai_computer_player_turn_skipping;
use wsr_core::pb::rng::Rng;
use wsr_core::pb::numeric::Cur;
use wsr_core::state::Globals;

pub const TURN: u32 = 0x5c3172;

/// The top-level steps (GOSUB blocks) of AiComputerPlayerTurn, in call order.
#[allow(non_snake_case)]
pub fn STEPS(__content: &wsr_core::content::ContentPack) -> Vec<(u32, &str)> { vec![
    (0x5c3730, "SqueezeOutMinority"),
    (0x5c3c65, "AssessPosition"),
    (0x5c3fc7, "FreezeRivalCredit"),
    (0x5c4828, "AcquireHoldingCo"),
    (0x5c5a9d, "BuyCorpBonds"),
    (0x5c5e5c, "SellCorpBonds"),
    (0x5c603a, "RepayFrozenLoan"),
    (0x5c67af, "UseExcessCash"),
    (0x5c7240, "PickAntitrustTarget"),
    (0x5c7855, "BuyTargetStock"),
    (0x5c8c0b, "ReviewSubHoldings"),
    (0x5c8e00, "AdvanceFunds"),
    (0x5c9506, "EnterGrowthIndustry"),
    (0x5c9b05, "CallInAdvances"),
    (0x5c9f3f, "StartupCompany"),
    (0x5caa87, "InsurerBuyBonds"),
    (0x5cb017, __content.string(13554)),
    (0x5cb6ea, "FileLawsuit"),
    (0x5cc20c, "SpreadRumors"),
    (0x5cc888, "TuneControlledCorps"),
    (0x5cce66, "WriteOptions"),
    (0x5cd0cd, "FireCeos"),
    (0x5cd435, "MergeControlledCorps"),
    (0x5cdf0e, "HostileMerger"),
    (0x5ce762, "IssueStockOffering"),
    (0x5cebd9, "ImproveCreditRating"),
    (0x5cf5ba, "BuyoutPublicShares"),
    (0x5cfad1, "MergeTaxLossSub"),
    (0x5cfe21, "AntitrustScan"),
    (0x5d020b, "BuyMoreStock"),
    (0x5d0c41, "LiquidateIdleHoldingCo"),
    (0x5d0eb9, "ConsolidateTopHoldings"),
    (0x5d12ad, "SellStockRaiseCash"),
    (0x5d177e, "BuyEthereum"),
    (0x5d18dd, "EndGamePayDebt"),
] }

/// Callees skipped in the original (none: every callee of the turn is ported).
pub const CALLEE_TODO: &[(u32, u32)] = &[];

/// Callees from other modules whose calls are logged in the original (`CALL Name(...)`, coverage):
/// BuyStock, MergeCompanies, LiquidateSubIntoParent, LboTenderOffer.
pub const HOOKS: &[u32] = &[0x4efe60, 0x509293, 0x512aa9, 0x51f21c];

/// Skip list for a turn that runs only the steps in `only` (`None`: all steps).
pub fn turn_skips(only: Option<&[u32]>) -> Vec<(u32, u32)> {
    let __content = wsr_core::content::current();
    let mut v: Vec<(u32, u32)> = CALLEE_TODO.to_vec();
    if let Some(only) = only {
        for (a, _) in STEPS(&__content) {
            if !only.contains(&a) {
                v.push((a, 0));
            }
        }
    }
    v
}

/// The result of one turn in the port and the original.
pub struct TurnRun {
    pub before: Globals,
    pub port: Globals,
    pub rng: Rng,
    pub ui: Rec,
    pub orig: Original,
}

impl TurnRun {
    /// Names of the fields the original changed (for coverage assertions).
    pub fn changed(&self) -> Vec<String> {
        let d = cmp().rel_tol(Ext::from_f64(0.0)).abs_tol(Ext::from_f64(0.0)).diffs(&self.before, &self.orig.globals);
        let mut names: Vec<String> = d.iter().map(|l| l.split(['(', ':']).next().unwrap_or("").to_string()).collect();
        names.dedup();
        names
    }
    /// True if the original changed field `name` (original name, e.g. `Cash`).
    pub fn changed_field(&self, name: &str) -> bool {
        self.changed().iter().any(|n| n == name)
    }
    /// The original raised a PB runtime error (its ON ERROR handler ends the turn): not comparable.
    pub fn pberr(&self) -> bool {
        self.orig.log.iter().any(|l| l.starts_with("PBERR"))
    }
}

/// Runs AiComputerPlayerTurn on `g` with RND state `rng` in the port and the original, with only the
/// steps `only` (None: all) and the extra skips `extra` (both sides); MSGBOX `answers`.
pub fn run_turn(g: &Globals, rng: u32, only: Option<&[u32]>, extra: &[(u32, u32)], answers: &[&str]) -> Option<TurnRun> {
    let mut skips = turn_skips(only);
    skips.extend_from_slice(extra);
    let o = super::sim_common::run_hooked(TURN, &[], g, rng, answers, &skips, HOOKS)?;
    let mut port = g.clone();
    let mut r = Rng::from_state(rng);
    let mut ui = Rec::default();
    for a in answers {
        ui.answers.push_back(match *a {
            "IDOK" => 1,
            "IDCANCEL" => 2,
            "IDYES" => 6,
            "IDNO" => 7,
            x => x.parse().unwrap_or(1),
        });
    }
    let skip: Vec<u32> = skips.iter().map(|x| x.0).collect();
    ai_computer_player_turn_skipping(&mut port, &mut ui, &mut r, &wsr_core::logic::newgame::LocalTime::HARNESS, &skip);
    Some(TurnRun { before: g.clone(), port, rng: r, ui, orig: o })
}

/// Compares port and original (globals within `rel_tol`, RND state, message boxes); panics with the
/// differences. Returns false (after printing) if the original raised a runtime error.
#[track_caller]
pub fn check_turn(what: &str, t: &TurnRun, rel_tol: Ext) -> bool {
    if t.pberr() {
        let errs: Vec<&String> = t.orig.log.iter().filter(|l| l.starts_with("PBERR")).collect();
        eprintln!("{what}: original raised runtime errors {errs:?}; not compared");
        return false;
    }
    let d = cmp().rel_tol(rel_tol).abs_tol(Ext::from_f64(1e-9)).diffs(&t.port, &t.orig.globals);
    let log: Vec<&String> = t.orig.log.iter().filter(|l| !l.starts_with("API")).take(40).collect();
    assert!(
        d.is_empty(),
        "{what}: {} differences:\n{}\nlog: {log:#?}",
        d.len(),
        d.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
    assert_eq!(t.rng.state(), t.orig.rng, "{what}: RND state; log: {log:#?}");
    assert_eq!(t.ui.msgs.len(), orig_msgs(&t.orig), "{what}: message boxes {:?} vs log {log:#?}", t.ui.msgs);
    true
}

/// An active operating company (131..1580, industry < 70), or 131.
pub fn active_corp(g: &Globals, r: &mut Rand) -> i32 {
    for _ in 0..200 {
        let c = cast::<i32, _>(r.range(131, 1580));
        if g.entity_status[c] == 1 && g.industry_num[c] < 70 {
            return c;
        }
    }
    131
}

/// An active entity in `lo..=hi`, or `lo`.
pub fn active_in(g: &Globals, r: &mut Rand, lo: i32, hi: i32) -> i32 {
    for _ in 0..200 {
        let c = cast::<i32, _>(r.range(cast::<i64, _>(lo), cast::<i64, _>(hi)));
        if g.entity_status[c] == 1 {
            return c;
        }
    }
    lo
}

/// Recounts slot 0 (number of used slots) of `e`'s stock holdings.
pub fn recount(g: &mut Globals, e: i32) {
    g.hold_stock_id[(0, e)] = cast::<i16, _>((1..=15).filter(|&s| g.hold_stock_id[(s, e)] != 0).count());
}

/// Adds (or raises) `holder`'s holding of `corp` by `pct` (first free slot); false if no slot is free.
pub fn add_holding(g: &mut Globals, r: &mut Rand, holder: i32, corp: i32, pct: i32) -> bool {
    let slot = (1..=15)
        .find(|&s| cast::<i32, _>(g.hold_stock_id[(s, holder)]) == corp)
        .or_else(|| (1..=15).find(|&s| g.hold_stock_id[(s, holder)] == 0));
    let Some(s) = slot else { return false };
    g.hold_stock_id[(s, holder)] = cast::<i16, _>(corp);
    g.hold_stock_pct[(s, holder)] = cast::<i16, _>((cast::<i32, _>(g.hold_stock_pct[(s, holder)]) + pct).min(100));
    g.hold_stock_basis[(s, holder)] = g.market_cap[corp] * cast::<Ext, _>(pct) * Ext::from_f64(0.01) * f64r(r, Ext::from_f64(0.5), Ext::from_f64(1.5));
    recount(g, holder);
    true
}

/// A plausible mid-game world (sim_entity's `world`) where computer player `CurPlayer` is to move:
/// game-format bond descriptions; holdings rebuilt so that no company is more than ~90% held; the player
/// owns a main holding (`HoldStockId(1, player)`, usually controlled) and a few other stakes, and
/// some of its companies hold subsidiaries; `CorpController()` follows the holdings (UpdateCorpControl
/// of the port over all companies).
pub fn ai_world(seed: u64) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = world(seed);
    let mut r = Rand(seed.wrapping_mul(0xd1b5_4a32_d192_ed03) | 1);
    // bond descriptions in the game's formats (`YYYY@cc.cc`, `YYYYcv|cap|@cc.cc`)
    for e in 11..=1600 {
        if g.bonds_issued[e] == 0 {
            continue;
        }
        let mat = g.cur_year + cast::<i32, _>(r.range(0, 14));
        let coupon = cast::<Ext, _>(r.range(40, 300)) / Ext::from_f64(20.0);
        let rate = format!("{coupon:05.2}");
        if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
            let cap = g.market_cap[e] * f64r(&mut r, Ext::from_f64(0.05), Ext::from_f64(1.6));
            g.bond_issue_info[e] = st(&format!("{mat}{__wsr_text_13555}{cap:.2}|@{rate}",__wsr_text_13555=__content.string(13555)));
        } else {
            g.bond_issue_info[e] = st(&format!("{mat}@{rate}"));
        }
        g.bond_price[e] = Cur::from_f64(f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(115.0)));
    }
    g.lender_id = cast::<i32, _>(r.range(11, 85));
    g.bonds_public_held = r.range(0, 3000);
    g.ind_total_assets = f64r(&mut r, Ext::from_f64(1000.0), Ext::from_f64(50000.0));
    g.ceo_bonus_multiple = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(2.0));
    // players: 2..4, the one to move is a computer player
    g.num_players = cast::<i32, _>(r.range(2, 4));
    for p in 1..=5 {
        g.entity_status[p] = if p <= g.num_players { 1 } else { 0 };
        g.is_computer_player[p] = if p > 1 && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { 1 } else { 0 };
        g.ent_name[p] = st(&format!("{__wsr_text_13556}{p}{__wsr_text_13557}",__wsr_text_13556=__content.string(13556), __wsr_text_13557=__content.string(13557)));
        g.player_name[p] = g.ent_name[p].clone();
        g.ent_symbol[p] = st(&format!("PL{p}"));
        g.industry_num[p] = 0;
        g.player_ceo_of[p] = 0;
        g.cash[p] = match r.range(0, 4) {
            0 => f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(100.0)),
            1 => f64r(&mut r, Ext::from_f64(100.0), Ext::from_f64(2000.0)),
            2 => f64r(&mut r, Ext::from_f64(2000.0), Ext::from_f64(20000.0)),
            _ => f64r(&mut r, Ext::from_f64(20000.0), Ext::from_f64(200000.0)),
        } * g.exchange_rate;
        g.t_bills[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0)) } else { Ext::from_f64(0.0) };
        g.bank_debt[p] = if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) { f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(5000.0)) } else { Ext::from_f64(0.0) };
        g.player_notoriety[p] = cast::<i16, _>(r.range(0, 10));
        g.player_cap_gains[p] = f64r(&mut r, -Ext::from_f64(50.0), Ext::from_f64(500.0));
        g.player_prepaid_tax[p] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(50.0));
        g.player_startup_count[p] = cast::<i32, _>(r.range(0, 3));
    }
    let p = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player = p;
    g.is_computer_player[p] = 1;
    if g.start_year == g.cur_year && g.cur_quarter == 1 && r.chance(cast::<f64, _>(Ext::from_f64(0.8))) {
        g.sim_co_count = cast::<i32, _>(r.range(501, 1600));
    }
    if r.chance(cast::<f64, _>(Ext::from_f64(0.25))) {
        g.cur_year = g.game_end_year - cast::<i32, _>(r.range(0, 2));
        g.start_year = g.cur_year - cast::<i32, _>(r.range(3, 20));
    }
    // holdings: at most ~90% of any company held
    let mut held = vec![0i32; 1601];
    for e in 1..=1600 {
        for s in 0..=15 {
            g.hold_stock_id[(s, e)] = 0;
            g.hold_stock_pct[(s, e)] = 0;
            g.hold_stock_basis[(s, e)] = Ext::from_f64(0.0);
        }
    }
    for e in 11..=1600 {
        if g.entity_status[e] != 1 {
            continue;
        }
        let n = match r.range(0, 9) {
            0..=4 => 0,
            5..=7 => r.range(1, 3),
            _ => r.range(3, 14),
        };
        for _ in 0..n {
            let c = cast::<i32, _>(r.range(11, 1600));
            if c == e || g.entity_status[c] != 1 {
                continue;
            }
            let pct = cast::<i32, _>([r.range(1, 9), r.range(5, 25), r.range(20, 55), r.range(51, 90)][cast::<usize, _>(r.range(0, 3))]);
            if held[cast::<usize, _>(c)] + pct > 90 {
                continue;
            }
            if add_holding(&mut g, &mut r, e, c, pct) {
                held[cast::<usize, _>(c)] += pct;
            }
        }
    }
    // the players' stakes; the mover's first holding is its main company
    for q in 1..=g.num_players {
        let n = if q == p { r.range(0, 9) } else { r.range(0, 5) };
        for k in 0..n {
            let c = if q == p && k == 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                active_in(&g, &mut r, 11, 130)
            } else {
                active_corp(&g, &mut r)
            };
            let room = 95 - held[cast::<usize, _>(c)];
            if room < 1 {
                continue;
            }
            let pct = if q == p && k == 0 {
                [room.min(100), room.min(cast::<i32, _>(r.range(20, 70))), room.min(cast::<i32, _>(r.range(5, 25)))][cast::<usize, _>(r.range(0, 2))]
            } else {
                room.min(cast::<i32, _>(r.range(1, 60)))
            };
            if add_holding(&mut g, &mut r, q, c, pct) {
                held[cast::<usize, _>(c)] += pct;
            }
        }
    }
    // control follows the holdings (twice: chains)
    let mut ui = Rec::default();
    let mut rr = Rng::from_state(1);
    g.ctrl_change_msg_mode = 1;
    for _ in 0..2 {
        for c in 11..=1600 {
            if g.entity_status[c] == 1 {
                wsr_core::logic::control::update_corp_control(&mut g, &mut ui, &mut rr, c);
            }
        }
    }
    g.ctrl_change_msg_mode = 0;
    g.news_msg = if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { st("Earlier news item") } else { st("") };
    g.auto_buyout_year = [0, g.cur_year, g.cur_year - 1, g.cur_year - 3][cast::<usize, _>(r.range(0, 3))];
    g.ai_adv_call_quarter = cast::<i32, _>(r.range(0, 4));
    if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
        let t = cast::<i32, _>(g.hold_stock_id[(1, p)]);
        if t > 10 && g.corp_controller[t] == p {
            g.player_ceo_of[p] = t;
        }
    }
    g
}
