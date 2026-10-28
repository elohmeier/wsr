//! Differential tests of the ownership diagram (ShowOwnershipDiagram 0x54eb71 and its callbacks).
use wsr_core::pb::real::{Ext, cast};


use super::common::*;
use super::diff::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use wsr_core::logic::info_screens::ownership::{
    build_ownership_diagram, diagram_report_lines, format_label, show_ownership_diagram, DiagramControlKind,
};
use wsr_core::logic::newgame::LocalTime;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;
use wsr_core::ui::{ReportAction, Ui};

/// Answers reports with the queued link codes (then OK) and records message boxes / reports.
#[derive(Default)]
struct Clicks {
    q: VecDeque<String>,
    msgs: usize,
    reports: Vec<(String, Vec<PbStr>)>,
}

impl Ui for Clicks {
    fn msgbox(&mut self, _text: &PbStr, style: i32, _title: &PbStr) -> i32 {
        self.msgs += 1;
        if matches!(style & 0xf, 3 | 4) { 6 } else { 1 }
    }
    fn inputbox(&mut self, _p: &PbStr, _t: &PbStr, default: &PbStr) -> PbStr {
        default.clone()
    }
    fn report(&mut self, title: &PbStr, lines: &[PbStr], _yes_no: bool) -> ReportAction {
        self.reports.push((title.to_display(), lines.to_vec()));
        match self.q.pop_front() {
            Some(c) if c == "close" => ReportAction::Closed,
            Some(c) => ReportAction::Link(PbStr::from(c.as_str())),
            None => ReportAction::Ok,
        }
    }
    fn news(&mut self, _h: &PbStr) {}
}

#[test]
fn format_label_cases() {
    assert_eq!(format_label(&st("ABC"), 12), st("ABC:  12%"));
    assert_eq!(format_label(&st("PLAYER NUMBER 1"), 40), st("Player Numb:  40%"));
    assert_eq!(format_label(&st("PLAYER NUMBER 1"), 100), st("Player Num:  100%"));
    assert_eq!(format_label(&st("A:B"), 5), st("A B:  5%"));
    assert_eq!(format_label(&st("LONGNAME:WITHCOLON"), 7), st("Longname Wi:  7%"));
}

/// A world with the diagram's entity, holders and holdings varied.
fn diagram_world(seed: u64) -> (Globals, Rand) {
    let mut g = world(seed);
    let mut r = Rand(seed ^ 0x0a11_0ce5);
    g.ticker_stop_req = st("Y");
    g.clock_state = if r.chance(Ext::from_f64(0.5)) { st("RUN") } else { PbStr::new() };
    g.ticker_on = cast::<i32, _>(r.range(0, 1));
    g.action_menu_open = cast::<i32, _>(r.range(0, 1));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    g.cur_player_name = g.ent_name[g.cur_player].clone();
    // players' symbols: sometimes their names (long labels, selection by name)
    for p in 1..=g.num_players {
        if r.chance(Ext::from_f64(0.5)) {
            g.ent_symbol[p] = g.player_name[p].clone();
            g.ent_name[p] = g.player_name[p].clone();
        }
    }
    g.cur_player_name = g.ent_name[g.cur_player].clone();
    let actives: Vec<i32> = (11..=1600).filter(|&e| g.entity_status[e] == 1).collect();
    let sel = match r.range(0, 9) {
        0 => 0,
        1 => g.cur_player,
        _ => actives[cast::<usize, _>(r.range(0, cast::<i64, _>(actives.len()) - 1))],
    };
    g.sel_entity = sel;
    let target = if sel < 1 { g.cur_player } else { sel };
    // extra shareholders of the target (incl. the current player sometimes)
    let extra = [0, 1, 2, 3, 6, 12][cast::<usize, _>(r.range(0, 5))];
    for k in 0..extra {
        let h = if k == 0 && r.chance(Ext::from_f64(0.5)) { g.cur_player } else { cast::<i32, _>(r.range(11, 1600)) };
        let slot = cast::<i32, _>(r.range(1, 15));
        if g.hold_stock_id[(slot, h)] == 0 {
            g.hold_stock_id[(0, h)] += 1;
        }
        g.hold_stock_id[(slot, h)] = cast::<i16, _>(target);
        g.hold_stock_pct[(slot, h)] = cast::<i16, _>(r.range(1, 60));
    }
    // holdings of the target: sometimes all 15 slots
    if r.chance(Ext::from_f64(0.3)) {
        let n = cast::<i32, _>(r.range(1, 15));
        for slot in 1..=15 {
            g.hold_stock_id[(slot, target)] = if slot <= n { cast::<i16, _>(actives[cast::<usize, _>(r.range(0, cast::<i64, _>(actives.len()) - 1))]) } else { 0 };
            g.hold_stock_pct[(slot, target)] = if slot <= n { cast::<i16, _>(r.range(1, 100)) } else { 0 };
        }
        g.hold_stock_id[(0, target)] = cast::<i16, _>(n);
    }
    // some long / colon / inactive symbols
    for _ in 0..40 {
        let e = cast::<i32, _>(r.range(11, 1600));
        match r.range(0, 2) {
            0 => g.ent_symbol[e] = st(&format!("{}LONGSYMBOL", word(&mut r, 4).to_display())),
            1 => g.ent_symbol[e] = st(&format!("{}:{}", word(&mut r, 3).to_display(), word(&mut r, 3).to_display())),
            _ => g.entity_status[e] = 2,
        }
    }
    (g, r)
}

fn orig_dialogs(o: &Original) -> Vec<&Dialog> {
    let __content = wsr_core::content::current();
    o.dialogs.iter().filter(|d| d.title.to_display().starts_with(__content.string(6584))).collect()
}

static REDRAWN: AtomicUsize = AtomicUsize::new(0);
static STAYED: AtomicUsize = AtomicUsize::new(0);
static SORTED: AtomicUsize = AtomicUsize::new(0);
static FULL: AtomicUsize = AtomicUsize::new(0);
static PLAYER_SEL: AtomicUsize = AtomicUsize::new(0);
static CORP_SEL: AtomicUsize = AtomicUsize::new(0);
static CLOSED_X: AtomicUsize = AtomicUsize::new(0);

#[test]
fn show_ownership_diagram_vs_original() {
    let __content = wsr_core::content::current();
    if let Some(why) = unavailable() {
        eprintln!("skipped: {why}");
        return;
    }
    par_seeds(1..=60, |seed| {
        let (mut g0, mut r) = diagram_world(seed);
        if seed == 2 {
            // Guarantee company navigation as well as player navigation; random labels with colons
            // or truncation may intentionally fail symbol lookup.
            let player = g0.cur_player;
            g0.sel_entity = player;
            g0.clock_state = st("RUN");
            g0.entity_status[131] = 1;
            g0.industry_num[131] = 37;
            g0.ent_symbol[131] = st("PORT131");
            g0.hold_stock_id[(1, player)] = 131;
            g0.hold_stock_pct[(1, player)] = 10;
            g0.hold_stock_id[(0, player)] = cast::<i16, _>((1..=15).filter(|&slot| g0.hold_stock_id[(slot, player)] != 0).count());
        }
        let rng0 = cast::<u32, _>(r.range(0, 0x7fff_ffff));
        // the first diagram, computed on a copy
        let mut gd = g0.clone();
        let d = build_ownership_diagram(&mut gd, &mut Clicks::default(), &mut Rng::from_state(rng0));
        if d.holders.len() > 2 {
            SORTED.fetch_add(1, Ordering::Relaxed);
        }
        if d.holding_count == 15 {
            FULL.fetch_add(1, Ordering::Relaxed);
        }
        let boxes: Vec<i32> = d.controls.iter().filter(|c| c.clickable).map(|c| c.id).collect();
        // seeds 1..=10: close at once (OK / window close); else click a box (player box preferred sometimes)
        let mut port_q = VecDeque::new();
        let mut orig_ans: Vec<String> = Vec::new();
        if seed % 6 == 0 {
            port_q.push_back("close".to_string());
            orig_ans.push("close".to_string());
        } else if seed % 6 != 1 {
            let player_box = d
                .controls
                .iter()
                .find(|c| c.clickable && c.text.to_display().to_uppercase().starts_with(&g0.ent_symbol[g0.cur_player].to_display().to_uppercase()[..cast::<usize, _>(g0.ent_symbol[g0.cur_player].len().min(9))]))
                .map(|c| c.id);
            let id = if seed == 2 {
                d.controls.iter().find(|c| c.clickable && c.text.to_display().starts_with("PORT131:")).unwrap().id
            } else { match player_box {
                Some(id) if r.chance(Ext::from_f64(0.7)) => id,
                _ => boxes[cast::<usize, _>(r.range(0, cast::<i64, _>(boxes.len()) - 1))],
            }};
            port_q.push_back(format!("{__wsr_text_13538}{id:04}",__wsr_text_13538=__content.string(13538)));
            orig_ans.push(format!("click:{id}"));
        }
        orig_ans.push("click:1".to_string());
        orig_ans.push("click:1".to_string());

        let mut g = g0.clone();
        let mut rng = Rng::from_state(rng0);
        let mut ui = Clicks { q: port_q, ..Default::default() };
        show_ownership_diagram(&mut g, &mut ui, &mut rng, &LocalTime::HARNESS);
        if seed == 2 { assert_eq!(g.sel_entity, 131, "the company holding box selects its company"); }

        let ans: Vec<&str> = orig_ans.iter().map(|s| s.as_str()).collect();
        let Some(o) = run(0x54eb71, &["0"], &g0, rng0, &ans, &[]) else { return };
        cmp().ignore("DlgOwnership").assert(&g, &o.globals);
        assert_eq!(rng.state(), o.rng, "seed {seed}: rng");
        assert_eq!(ui.msgs, orig_msgs(&o), "seed {seed}: message boxes");
        let od = orig_dialogs(&o);
        // the first diagram control by control
        let mut want: Vec<(i32, String)> =
            od[0].controls.iter().filter(|c| !c.text.is_empty()).map(|c| (c.id, c.text.to_display())).collect();
        let mut got: Vec<(i32, String)> = d
            .controls
            .iter()
            .filter(|c| c.kind != DiagramControlKind::Line && !c.text.is_empty())
            .map(|c| (c.id, c.text.to_display()))
            .collect();
        want.sort();
        got.sort();
        assert_eq!(got, want, "seed {seed}{__wsr_text_13907}",__wsr_text_13907=__content.string(13907));
        assert_eq!(d.title, od[0].title, "seed {seed}: title");
        // every diagram shown (a redraw shows a second one)
        let titles: Vec<String> = od.iter().map(|d| d.title.to_display()).collect();
        let port_titles: Vec<String> = {
            let mut t: Vec<String> = ui.reports.iter().map(|r| r.0.clone()).collect();
            // The report adapter asks again when a click leaves the same modal dialog open. A real
            // redraw can also have the same title (e.g. selecting the player again), so preserve those.
            if titles.len() == 1 { t.dedup(); }
            t
        };
        assert_eq!(port_titles, titles, "seed {seed}: diagrams");
        if od.len() > 1 {
            REDRAWN.fetch_add(1, Ordering::Relaxed);
            if g.sel_entity == g.cur_player {
                PLAYER_SEL.fetch_add(1, Ordering::Relaxed);
            } else {
                CORP_SEL.fetch_add(1, Ordering::Relaxed);
            }
            assert_eq!(ui.reports.last().unwrap().1, diagram_report_lines(&build_ownership_diagram(&mut g.clone(), &mut Clicks::default(), &mut Rng::from_state(0))), "seed {seed}");
        } else if seed % 6 > 1 {
            STAYED.fetch_add(1, Ordering::Relaxed);
        }
        if seed % 6 == 0 {
            CLOSED_X.fetch_add(1, Ordering::Relaxed);
        }
    });
    if std::env::var("WSR_ONLY_SEED").is_err() {
        let c = |a: &AtomicUsize| a.load(Ordering::Relaxed);
        eprintln!(
            "redrawn {}{__wsr_text_13908}{}, corp {}), stayed {}, sorted {}, full {}{__wsr_text_13909}{}",
            c(&REDRAWN), c(&PLAYER_SEL), c(&CORP_SEL), c(&STAYED), c(&SORTED), c(&FULL), c(&CLOSED_X)
        ,__wsr_text_13908=__content.string(13908), __wsr_text_13909=__content.string(13909));
        assert!(c(&PLAYER_SEL) > 0 && c(&CORP_SEL) > 0 && c(&STAYED) > 0 && c(&SORTED) > 3 && c(&FULL) > 0);
    }
}
