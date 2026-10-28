//! Differential tests of `logic::restructure::capital`: ContributeCapitalToSub (0x596b50) with its dialog
//! (driven through `Ui::choose` / the harness's dialog actions) and UpdateCtrlSinceInMiscData (0x5288b9).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::{BTreeSet, VecDeque};
use wsr_core::logic::report::show_report_dialog;
use wsr_core::logic::restructure::{contribute_capital_to_sub, update_ctrl_since_in_misc_data};
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

const CONTRIBUTE_CAPITAL: u32 = 0x596b50;
const UPDATE_CTRL_SINCE: u32 = 0x5288b9;

fn rng0(seed: u64) -> u32 {
    (cast::<u32, _>(seed)).wrapping_mul(2_654_435_761) ^ 0x2c1b_3c6d
}

/// One scripted user action, given to both the port's Ui and the original's harness.
#[derive(Clone, Debug)]
enum Step {
    /// A MSGBOX answer ("IDYES", "IDNO", "IDCANCEL").
    Msg(&'static str),
    /// INPUTBOX text.
    Input(String),
    /// The asset-type dialog: option index 0..3 then Apply, or `None` = Cancel.
    Choose(Option<usize>),
    /// The asset-type dialog closed with Apply without picking an option.
    ApplyNone,
    /// Pick the line linking entity `id` in the next report.
    Pick(i32),
}

/// Port-side Ui following the script; records message boxes, news and coverage markers.
#[derive(Default)]
struct StepUi {
    steps: VecDeque<Step>,
    msgs: Vec<String>,
    news: Vec<String>,
    /// List index of each picked report line (for the original's `select:301:N`).
    picked_rows: Vec<usize>,
}

impl Ui for StepUi {
    fn msgbox(&mut self, text: &PbStr, style: i32, title: &PbStr) -> i32 {
        self.msgs.push(format!("[{}] {}", title.to_display(), text.to_display()));
        let quick = style == 0 && title.is_empty();
        let default = if style & 0xf == 3 || style & 0xf == 4 { 6 } else { 1 };
        if quick {
            return default;
        }
        if let Some(Step::Msg(a)) = self.steps.front() {
            let a = *a;
            self.steps.pop_front();
            return match a {
                "IDYES" => 6,
                "IDNO" => 7,
                "IDCANCEL" => 2,
                _ => 1,
            };
        }
        default
    }
    fn inputbox(&mut self, _prompt: &PbStr, _title: &PbStr, default: &PbStr) -> PbStr {
        if let Some(Step::Input(t)) = self.steps.front() {
            let t = PbStr::from(t.as_str());
            self.steps.pop_front();
            return t;
        }
        // the harness answers an unscripted INPUTBOX$ with Cancel ("")
        let _ = default;
        PbStr::new()
    }
    fn report(&mut self, _title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        if let Some(Step::Pick(id)) = self.steps.front() {
            let code = format!("@C{:04}", id);
            self.steps.pop_front();
            if let Some(k) = lines.iter().position(|l| l.to_display().ends_with(&code)) {
                self.picked_rows.push(k);
                return ReportAction::Link(PbStr::from(code.as_str()));
            }
            self.picked_rows.push(usize::MAX);
        }
        ReportAction::Ok
    }
    fn news(&mut self, headline: &PbStr) {
        self.news.push(headline.to_display());
    }
    fn choose(&mut self, _title: &PbStr, _prompt: &[PbStr], options: &[PbStr]) -> Option<usize> {
        assert_eq!(options.len(), 4);
        match self.steps.front() {
            Some(Step::Choose(c)) => {
                let c = *c;
                self.steps.pop_front();
                c
            }
            Some(Step::ApplyNone) => {
                self.steps.pop_front();
                None
            }
            _ => None,
        }
    }
}

/// The original's answer list for a script (`rows`: the port's picked list rows, in order).
fn harness_answers(steps: &[Step], rows: &[usize]) -> Vec<String> {
    let mut out = Vec::new();
    let mut rows = rows.iter();
    for st in steps {
        match st {
            Step::Msg(a) => out.push(a.to_string()),
            Step::Input(t) => out.push(t.clone()),
            Step::Choose(Some(k)) => {
                out.push(format!("click:{}", [513, 515, 517, 518][*k]));
                out.push("click:1".into());
            }
            Step::Choose(None) => out.push("click:2".into()),
            Step::ApplyNone => out.push("click:1".into()),
            Step::Pick(_) => {
                if let Some(&r) = rows.next()
                    && r != usize::MAX
                {
                    out.push(format!("select:301:{r}"));
                    out.push("click:1".into());
                }
            }
        }
    }
    out
}

/// Whether the port's report dialog follows a clicked `@C` link (FollowReportLink 0x46c9f4 wired).
fn links_followed() -> bool {
    struct Clicker;
    impl Ui for Clicker {
        fn msgbox(&mut self, _: &PbStr, _: i32, _: &PbStr) -> i32 {
            1
        }
        fn inputbox(&mut self, _: &PbStr, _: &PbStr, d: &PbStr) -> PbStr {
            d.clone()
        }
        fn report(&mut self, _: &PbStr, _: &[PbStr], _: bool) -> ReportAction {
            ReportAction::Link(PbStr::from("@C0150"))
        }
        fn news(&mut self, _: &PbStr) {}
    }
    let mut g = Globals::default();
    g.entity_pick_mode = st("GetCorp");
    g.text_lines[1] = st("X                @C0150");
    g.text_lines[2] = st("@END");
    show_report_dialog(&mut g, &mut Clicker, &mut Rng::from_state(1));
    g.target_corp == 150
}

/// Scenario kinds of [`cc_world`].
const KINDS: u64 = 20;

/// A world with an active entity (parent) and a scripted contribution scenario `seed % KINDS`.
/// Returns the world, the script and whether it needs a report pick.
fn cc_world(seed: u64) -> (Globals, Vec<Step>, bool) {
    let kind = seed % KINDS;
    let mut g = world(seed + 7100);
    let mut r = Rand(seed * 977 + 13);
    g.sel_entity = 0;
    g.cur_player = 1;
    g.num_players = g.num_players.max(2);
    g.player_name[1] = st("Player One");
    // parent: a player, a bank/insurer (bonds), or an operating corp
    let parent: i32 = match kind {
        12..=15 => {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.4))) {
                1
            } else {
                cast::<i32, _>(r.range(11, 130))
            }
        }
        9 | 10 | 11 | 19 => cast::<i32, _>(r.range(131, 1580)),
        _ => match r.range(0, 2) {
            0 => 1,
            1 => cast::<i32, _>(r.range(11, 130)),
            _ => cast::<i32, _>(r.range(131, 1580)),
        },
    };
    g.entity_status[parent] = 1;
    g.active_entity = parent;
    g.active_entity_name = if parent < 11 { g.player_name[parent].clone() } else { g.ent_name[parent].clone() };
    g.corp_controller[parent] = if parent < 11 { 0 } else { 1 };
    // subsidiaries
    let sub_range: (i64, i64) = match kind {
        12..=14 => (11, 130),
        9..=11 | 15 | 17 | 19 => (131, 1580),
        _ => {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                (131, 1580)
            } else {
                (11, 130)
            }
        }
    };
    let n_subs = match kind {
        0 => 0,
        1..=3 => cast::<i32, _>(r.range(2, 3)),
        _ => 1,
    };
    for slot in 0..=15 {
        g.hold_stock_id[(slot, parent)] = 0;
        g.hold_stock_pct[(slot, parent)] = 0;
        g.hold_stock_basis[(slot, parent)] = Ext::from_f64(0.0);
    }
    let mut subs = Vec::new();
    let mut slot = 0;
    while (cast::<i32, _>(subs.len())) < n_subs {
        let sid = cast::<i32, _>(r.range(sub_range.0, sub_range.1));
        if sid == parent || subs.contains(&sid) {
            continue;
        }
        slot += 1;
        g.entity_status[sid] = 1;
        g.hold_stock_id[(slot, parent)] = cast::<i16, _>(sid);
        g.hold_stock_pct[(slot, parent)] = 100;
        g.hold_stock_basis[(slot, parent)] = f64r(&mut r, Ext::from_f64(10.0), Ext::from_f64(800.0));
        g.corp_controller[sid] = if parent < 11 { parent } else { g.corp_controller[parent] };
        subs.push(sid);
    }
    // other stock holdings (kind 0 and stock transfers); kind 18: none
    let mut stocks = Vec::new();
    let n_other = if kind == 18 { 0 } else { r.range(0, 4) };
    for _ in 0..n_other {
        let c = cast::<i32, _>(r.range(131, 1580));
        if subs.contains(&c) || c == parent || stocks.contains(&c) {
            continue;
        }
        slot += 1;
        g.entity_status[c] = 1;
        g.hold_stock_id[(slot, parent)] = cast::<i16, _>(c);
        g.hold_stock_pct[(slot, parent)] = if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) { -(cast::<i16, _>(r.range(1, 10))) } else { cast::<i16, _>(r.range(1, 99)) };
        g.hold_stock_basis[(slot, parent)] = f64r(&mut r, -Ext::from_f64(40.0), Ext::from_f64(600.0));
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            g.corp_controller[c] = 1;
        }
        stocks.push(c);
    }
    g.hold_stock_id[(0, parent)] = cast::<i16, _>(slot);
    let sub = subs.first().copied().unwrap_or(0);
    // bonds of the parent
    if parent <= 130 {
        for s in 0..=15 {
            g.hold_bond_id[(s, parent)] = 0;
            g.hold_bond_face[(s, parent)] = 0;
            g.bond_hold_cost[(s, parent)] = Ext::from_f64(0.0);
        }
        g.long_gov_bond_face[parent] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { r.range(1, 900) } else { 0 };
        g.short_gov_bond_face[parent] = if r.chance(cast::<f64, _>(Ext::from_f64(0.6))) { r.range(1, 900) } else { 0 };
        g.lt_gov_bond_cost[parent] = cast::<Ext, _>(g.long_gov_bond_face[parent]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        g.st_gov_bond_cost[parent] = cast::<Ext, _>(g.short_gov_bond_face[parent]) * f64r(&mut r, Ext::from_f64(0.8), Ext::from_f64(1.1));
        let mut issuers = Vec::new();
        match kind {
            14 => issuers.push(cast::<i32, _>(r.range(131, 1580))),
            15 => issuers.push(sub),
            _ => {
                for _ in 0..r.range(0, 3) {
                    issuers.push(cast::<i32, _>(r.range(131, 1580)));
                }
            }
        }
        let mut n = 0;
        for &b in &issuers {
            if b == parent || (n > 0 && issuers[..n].contains(&b)) {
                continue;
            }
            n += 1;
            g.entity_status[b] = 1;
            if g.bonds_issued[b] <= 0 {
                g.bonds_issued[b] = r.range(100, 1500);
                g.bond_issue_info[b] = st(&format!("{} {}  {:5.2}", g.cur_year + 5, if r.chance(Ext::from_f64(0.2)) { "cv" } else { "" }, Ext::from_f64(7.5)));
                g.bond_price[b].set(f64r(&mut r, Ext::from_f64(60.0), Ext::from_f64(120.0)));
            }
            g.hold_bond_id[(cast::<i32, _>(n), parent)] = b;
            g.hold_bond_face[(cast::<i32, _>(n), parent)] = r.range(1, g.bonds_issued[b].min(400));
            g.bond_hold_cost[(cast::<i32, _>(n), parent)] = cast::<Ext, _>(g.hold_bond_face[(cast::<i32, _>(n), parent)]) * f64r(&mut r, Ext::from_f64(0.7), Ext::from_f64(1.2));
        }
        g.hold_bond_id[(0, parent)] = cast::<i32, _>(n);
    }
    if kind == 17 {
        // the parent does not hold the subsidiary's bonds
        for s in 1..=15 {
            if g.hold_bond_id[(s, parent)] == sub {
                g.hold_bond_id[(s, parent)] = 0;
            }
        }
        if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
            g.bonds_issued[sub] = 0;
        }
    }
    // business assets of the parent
    if (9..=11).contains(&kind) && sub > 0 {
        g.business_assets[parent] = f64r(&mut r, Ext::from_f64(50.0), Ext::from_f64(3000.0));
        g.industry_num[parent] = if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { 37 } else { cast::<i16, _>(r.range(3, 68)) };
        match kind {
            9 => g.industry_num[sub] = g.industry_num[parent],
            10 => {
                g.industry_num[sub] = 70;
                g.business_assets[sub] = Ext::from_f64(0.0);
                if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
                    g.tax_loss_carry[sub] = -f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(400.0));
                }
                if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.tax_credits[sub] = f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(60.0));
                }
            }
            _ => {
                if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.industry_num[sub] = 70;
                } else {
                    g.industry_num[sub] = cast::<i16, _>(r.range(3, 68));
                }
            }
        }
    }
    if kind == 19 {
        g.business_assets[parent] = Ext::from_f64(0.0);
    }
    // script
    let mut steps = Vec::new();
    let mut pick = false;
    let amount_input = |r: &mut Rand, max: Ext| -> Option<String> {
        match r.range(0, 4) {
            0 => None,
            1 => Some(format!("{}", (max * f64r(r, Ext::from(Ext::from_f64(0.1)), Ext::from(Ext::from_f64(0.9)))).floor())),
            2 => Some("0".into()),
            3 => Some(format!("{:.2}", max * f64r(r, Ext::from(Ext::from_f64(0.2)), Ext::from(Ext::from_f64(0.8))))),
            _ => Some(format!("${}", max.floor())),
        }
    };
    match kind {
        0 => {}
        1 => steps.push(Step::Msg("IDNO")),
        2 => steps.push(Step::Msg("IDYES")),
        3 => {
            steps.push(Step::Msg("IDYES"));
            steps.push(Step::Pick(subs[cast::<usize, _>(r.range(0, cast::<i64, _>(subs.len()) - 1))]));
            steps.push(Step::Choose(Some(0)));
            pick = true;
        }
        4 => steps.push(Step::Choose(None)),
        5 => steps.push(Step::ApplyNone),
        6..=8 => {
            if kind == 7 && parent > 10 && parent <= 85 {
                // a bank with a large cash position: the regulators' contribution limit
                g.cash[parent] = f64r(&mut r, Ext::from_f64(20_000.0), Ext::from_f64(80_000.0));
                g.bank_demand_deposits[parent] = g.cash[parent] * f64r(&mut r, Ext::from_f64(0.0), Ext::from_f64(0.3));
                g.bank_c_ds[parent] = Ext::from_f64(0.0);
                g.market_cap[sub] = f64r(&mut r, Ext::from_f64(1.0), Ext::from_f64(50.0));
            }
            steps.push(Step::Choose(Some(0)));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                steps.push(Step::Input("99999999".into()));
            }
            if let Some(a) = amount_input(&mut r, Ext::from_f64(1500.0)) {
                steps.push(Step::Input(a));
            }
        }
        9..=11 | 19 => {
            steps.push(Step::Choose(Some(1)));
            let asks = g.industry_num[sub] == 70
                && !(-Ext::from_f64(100.0) * g.exchange_rate <= g.tax_loss_carry[sub] && g.tax_credits[sub] <= Ext::from_f64(25.0));
            if asks {
                steps.push(Step::Msg(if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) { "IDNO" } else { "IDYES" }));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                steps.push(Step::Input("99999999".into()));
            }
            let max = g.business_assets[parent];
            match r.range(0, 3) {
                0 => {}
                1 => steps.push(Step::Input(format!("{:.2}", max * f64r(&mut r, Ext::from(Ext::from_f64(0.1)), Ext::from(Ext::from_f64(0.95)))))),
                2 => steps.push(Step::Input("0".into())),
                _ => steps.push(Step::Input(format!("{:.2}", max))),
            }
        }
        12 | 13 => {
            steps.push(Step::Choose(Some(3)));
            steps.push(Step::Msg(["IDYES", "IDYES", "IDNO", "IDCANCEL"][cast::<usize, _>(r.range(0, 3))]));
            let max = cast::<Ext, _>(g.long_gov_bond_face[parent].max(g.short_gov_bond_face[parent]));
            if r.chance(cast::<f64, _>(Ext::from_f64(0.2))) {
                steps.push(Step::Input("99999999".into()));
            }
            if let Some(a) = amount_input(&mut r, max) {
                steps.push(Step::Input(a));
            }
            if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                steps.push(Step::Input(format!("{}", r.range(0, 50))));
            }
        }
        14 | 15 | 17 => {
            steps.push(Step::Choose(Some(3)));
            if kind != 17 && (g.long_gov_bond_face[parent] > 0 || g.short_gov_bond_face[parent] > 0) {
                steps.push(Step::Msg("IDNO"));
            }
            if kind == 15 {
                steps.push(Step::Msg(if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { "IDYES" } else { "IDNO" }));
            }
            if let Some(a) = amount_input(&mut r, Ext::from_f64(400.0)) {
                steps.push(Step::Input(a));
            }
        }
        16 | 18 => {
            steps.push(Step::Choose(Some(2)));
            if !stocks.is_empty() && kind == 16 {
                let c = stocks[cast::<usize, _>(r.range(0, cast::<i64, _>(stocks.len()) - 1))];
                steps.push(Step::Pick(c));
                pick = true;
                // an insider hedge: a long put of the parent on a stock the player controls
                let slot = (1..=15).find(|&k| cast::<i32, _>(g.hold_stock_id[(k, parent)]) == c).unwrap();
                let held = g.hold_stock_pct[(slot, parent)];
                if held > 0 && r.chance(cast::<f64, _>(Ext::from_f64(0.5))) {
                    g.corp_controller[c] = 1;
                    let qty = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i64, _>(held) } else { r.range(1, cast::<i64, _>(held)) };
                    g.option_recs[0] = st("1");
                    g.option_recs[1] = st(&format!("{c}|2|{parent}|{qty}|{:.2}|7|{}|12.5", g.market_cap[c], g.cur_year + 1));
                    steps.push(Step::Msg(if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { "IDYES" } else { "IDNO" }));
                } else if g.corp_controller[c] == 1 {
                    g.corp_controller[c] = 2;
                }
                if let Some(a) = amount_input(&mut r, Ext::from_f64(99.0)) {
                    steps.push(Step::Input(a));
                }
            }
        }
        _ => unreachable!(),
    }
    (g, steps, pick)
}

/// Coverage markers of a port run.
fn markers(ui: &StepUi, port: &Globals, before: &Globals) -> Vec<String> {
    let __content = wsr_core::content::current();
    let mut m = Vec::new();
    for t in &ui.msgs {
        let key = t.split(['.', ':', ',']).next().unwrap_or("").to_string();
        m.push(key.chars().take(90).collect());
        if t.contains(__content.string(14016)) {
            m.push(__content.string(14016).into());
        }
    }
    if port.turn_action_count != before.turn_action_count {
        m.push("TRANSFER".into());
    }
    m
}

#[test]
fn diff_contribute_capital_to_sub() {
    let __content = wsr_core::content::current();
    let follow = links_followed();
    if !follow {
        eprintln!("NOTE: report links are not followed by the port yet (FollowReportLink): pick scenarios skipped");
    }
    let seen = std::sync::Mutex::new(BTreeSet::new());
    par_seeds(1..=300, |seed| {
        if std::env::var("SEED").is_ok_and(|s| s.parse::<u64>().ok() != Some(seed)) {
            return;
        }
        let (g, steps, pick) = cc_world(seed);
        if pick && !follow {
            return;
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = StepUi { steps: steps.iter().cloned().collect(), ..StepUi::default() };
        contribute_capital_to_sub(&mut port, &mut ui, &mut rng, 0);
        let answers = harness_answers(&steps, &ui.picked_rows);
        let answers: Vec<&str> = answers.iter().map(|s| s.as_str()).collect();
        let what = format!("seed {seed}{__wsr_text_13623}{}{__wsr_text_14017}{steps:?}", seed % KINDS,__wsr_text_13623=__content.string(13623), __wsr_text_14017=__content.string(14017));
        let Some(o) = run(CONTRIBUTE_CAPITAL, &[], &g, rs, &answers, &[]) else {
            return;
        };
        if std::env::var("DBG").is_ok() {
            for l in o.log.iter().filter(|l| !l.starts_with("API")) {
                eprintln!("{what}: {l}");
            }
            eprintln!("{what}: port msgs {:?}", ui.msgs);
        }
        let d = cmp().ignore("hCapContribDlg").rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).diffs(&port, &o.globals);
        assert!(d.is_empty(), "{what}: {} differences (port vs original):\n{}", d.len(), d.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
        assert_eq!(rng.state(), o.rng, "{what}: RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "{what}: message boxes {:?} vs {:?}", ui.msgs, o.log);
        let mut s = seen.lock().unwrap();
        for k in markers(&ui, &port, &g) {
            s.insert(k);
        }
    });
    if unavailable().is_some() {
        return;
    }
    let s = seen.into_inner().unwrap();
    if std::env::var("COV").is_ok() {
        eprintln!("{s:#?}");
    }
    let need = [
        "TRANSFER",
        __content.string(14018),
        __content.string(9427),
        __content.string(14019),
        __content.string(14020),
        __content.string(14021),
        __content.string(14022),
        __content.string(14023),
        __content.string(14024),
        __content.string(14025),
        __content.string(14026),
        __content.string(14027),
        __content.string(14028),
        "No corporate bond selected",
        __content.string(14029),
        __content.string(14030),
        __content.string(14031),
        __content.string(14032),
        __content.string(14033),
        __content.string(14034),
        "[] Because",
        "[] Next",
        __content.string(14035),
        "[] Proceed",
        __content.string(14036),
        __content.string(14016),
    ];
    for n in need {
        assert!(s.iter().any(|k| k.contains(n)), "branch not covered: {n}\n{s:#?}");
    }
}

#[test]
fn diff_update_ctrl_since_in_misc_data() {
    let rewritten = std::sync::atomic::AtomicU32::new(0);
    par_seeds(1..=24, |seed| {
        let mut g = world(seed + 7300);
        let mut r = Rand(seed * 41 + 5);
        let c = cast::<i32, _>(r.range(131, 1580));
        g.entity_status[c] = 1;
        g.ctrl_since_year[c] = match seed % 4 {
            0 => 0,
            1 => g.cur_year,
            _ => g.cur_year - cast::<i32, _>(r.range(1, 8)),
        };
        if seed % 6 == 5 {
            g.ent_misc_data[c] = st("1|2|3|");
        }
        // an 80%+ corporate owner for half of them
        if seed % 2 == 0 {
            let p = cast::<i32, _>(r.range(131, 1580));
            if p != c {
                g.entity_status[p] = 1;
                g.hold_stock_id[(1, p)] = cast::<i16, _>(c);
                g.hold_stock_pct[(1, p)] = cast::<i16, _>(r.range(80, 100));
            }
        }
        let rs = rng0(seed);
        let mut port = g.clone();
        let mut rng = Rng::from_state(rs);
        let mut ui = StepUi::default();
        update_ctrl_since_in_misc_data(&mut port, &mut ui, &mut rng, c);
        let Some(o) = run(UPDATE_CTRL_SINCE, &[&c.to_string()], &g, rs, &[], &[]) else {
            return;
        };
        if port.ent_misc_data[c] != g.ent_misc_data[c] {
            rewritten.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let what = format!("seed {seed} corp {c}");
        cmp().rel_tol(Ext::from_f64(1e-9)).abs_tol(Ext::from_f64(1e-9)).assert(&port, &o.globals);
        assert_eq!(rng.state(), o.rng, "{what}: RND state");
        assert_eq!(ui.msgs.len(), orig_msgs(&o), "{what}: message boxes");
    });
    if unavailable().is_none() {
        assert!(rewritten.into_inner() > 0, "no EntMiscData rewrite covered");
    }
}
