//! CheckPriceAlerts / ExpirePriceAlerts / UpdateStreamingQuotes.
use wsr_core::pb::real::{Ext, cast};


use super::common::{cmp, names, orig_msgs, run, st, Rec};
use super::diff::{Original, Rand};
use wsr_core::logic::reports_lists::{check_price_alerts, expire_price_alerts, update_streaming_quotes};
use wsr_core::pb::numeric::format;
use wsr_core::pb::pbstr::PbStr;
use wsr_core::pb::rng::Rng;
use wsr_core::state::Globals;

const CHECK_PRICE_ALERTS: u32 = 0x6830fc;
const EXPIRE_PRICE_ALERTS: u32 = 0x684123;
const UPDATE_STREAMING_QUOTES: u32 = 0x5dc70b;

fn price_str(x: Ext) -> PbStr {
    let t = PbStr(format(x, b"* ###.00"));
    PbStr::from(t.to_display().trim())
}

/// A plausible market: players, active corps with caps/shares, commodity prices and rates, a watch list.
fn market(r: &mut Rand) -> Globals {
    let __content = wsr_core::content::current();
    let mut g = Globals::default();
    names(&mut g, r);
    g.num_players = cast::<i32, _>(r.range(1, 4));
    g.cur_player = cast::<i32, _>(r.range(1, cast::<i64, _>(g.num_players)));
    for p in 1..=5 {
        g.player_name[p] = PbStr::from(format!("{__wsr_text_13556}{p}",__wsr_text_13556=__content.string(13556)).as_str());
    }
    g.cur_player_name = g.player_name[g.cur_player].clone();
    g.currency_suffix = st(["", " USD", ""][cast::<usize, _>(r.range(0, 2))]);
    g.game_end_flag = st(if r.chance(cast::<f64, _>(Ext::from_f64(0.15))) { "Y" } else { "N" });
    g.game_in_progress = cast::<i32, _>(r.range(0, 1));
    g.sim_co_count = cast::<i32, _>(r.range(0, 1600));
    g.cur_year = cast::<i32, _>(r.range(2000, 2030));
    g.cur_quarter = cast::<i32, _>(r.range(1, 4));
    for i in 1..=1600 {
        g.entity_status[i] = if i <= g.num_players || r.chance(cast::<f64, _>(Ext::from_f64(0.9))) { 1 } else { cast::<i16, _>(r.range(0, 2)) };
        g.industry_num[i] = cast::<i16, _>(r.range(1, 70));
        g.shares_outstanding[i] = cast::<Ext, _>(r.range(1_000, 100_000)) / Ext::from_f64(10.0);
        g.market_cap[i] = cast::<Ext, _>(r.range(10, 1_000_000)) / Ext::from_f64(100.0);
    }
    g.oil_price = cast::<f32, _>(cast::<f32, _>(r.range(10, 200)) / Ext::from_f64(100.0));
    g.gold_price = cast::<f32, _>(cast::<f32, _>(r.range(300, 3000)));
    g.silver_price = cast::<f32, _>(cast::<f32, _>(r.range(500, 5000)) / Ext::from_f64(100.0));
    g.wheat_price = cast::<f32, _>(cast::<f32, _>(r.range(200, 900)) / Ext::from_f64(100.0));
    g.corn_price = cast::<f32, _>(cast::<f32, _>(r.range(200, 900)) / Ext::from_f64(100.0));
    g.bitcoin_price = cast::<i32, _>(r.range(1000, 90000));
    g.ethereum_price = cast::<i32, _>(r.range(100, 5000));
    g.stock_index = cast::<Ext, _>(r.range(100_000, 5_000_000)) / Ext::from_f64(100.0);
    g.prime_rate = cast::<f32, _>(cast::<f32, _>(r.range(100, 1500)) / Ext::from_f64(100.0));
    g.base_interest_rate = cast::<f32, _>(g.prime_rate - Ext::from_f64(0.25));
    g.long_bond_yield = cast::<f32, _>(cast::<f32, _>(r.range(100, 1200)) / Ext::from_f64(100.0));
    g.short_bond_yield = cast::<f32, _>(cast::<f32, _>(r.range(50, 1000)) / Ext::from_f64(100.0));
    g.gdp_growth = cast::<f32, _>(cast::<f32, _>(r.range(-500, 800)) / Ext::from_f64(100.0));
    g.sel_entity = if r.chance(cast::<f64, _>(Ext::from_f64(0.5))) { cast::<i32, _>(r.range(11, 1600)) } else { 0 };
    g
}

fn watch_list(g: &mut Globals, r: &mut Rand, near: &[i32]) {
    let p = g.cur_player;
    for slot in 1..=15 {
        if r.chance(cast::<f64, _>(Ext::from_f64(0.3))) {
            continue;
        }
        let c = if !near.is_empty() && r.chance(cast::<f64, _>(Ext::from_f64(0.6))) {
            (near[cast::<usize, _>(r.range(0, cast::<i64, _>(near.len()) - 1))] + cast::<i32, _>(r.range(-1, 1))).clamp(0, 1600)
        } else {
            cast::<i32, _>(r.range(0, 1600))
        };
        g.watch_list[(slot, p)] = c;
        if c >= 11 {
            let px = g.market_cap[c] / g.shares_outstanding[c];
            // mostly the current price (so the "unchanged" nudge path runs), sometimes another one
            let px = if r.chance(cast::<f64, _>(Ext::from_f64(0.7))) { px } else { px * (Ext::from_f64(0.9) + Ext::from_f64(0.2) * cast::<Ext, _>(r.range(0, 100)) / Ext::from_f64(100.0)) };
            g.stream_cur_price[(slot, p)] = price_str(px);
            g.stream_price_dir[(slot, p)] = cast::<i32, _>(r.range(-1, 1));
        }
    }
}

/// A random alert record; stock targets near the current cap so that both outcomes occur.
fn alert_rec(g: &Globals, r: &mut Rand, expire: bool) -> (PbStr, i32) {
    let kind = r.range(0, 9);
    let entity = match kind {
        0..=5 => cast::<i32, _>(r.range(11, 1600)),
        6 | 7 => cast::<i32, _>(r.range(3002, 3013)),
        8 => cast::<i32, _>(r.range(1, 10)),
        _ => [0, 1700, 3001, 3014][cast::<usize, _>(r.range(0, 3))],
    };
    let cur = match entity {
        11..=1600 => g.market_cap[entity],
        3002 => cast::<Ext, _>(g.oil_price) * Ext::from_f64(100.0),
        3003 => cast::<Ext, _>(g.gold_price),
        3004 => cast::<Ext, _>(g.silver_price),
        3005 => cast::<Ext, _>(g.wheat_price),
        3006 => cast::<Ext, _>(g.corn_price),
        3007 => cast::<Ext, _>(g.bitcoin_price),
        3008 => cast::<Ext, _>(g.ethereum_price),
        3009 => g.stock_index,
        3010 => cast::<Ext, _>(g.prime_rate),
        3011 => cast::<Ext, _>(g.long_bond_yield),
        3012 => cast::<Ext, _>(g.short_bond_yield),
        3013 => cast::<Ext, _>(g.gdp_growth),
        _ => Ext::from_f64(100.0),
    };
    let target = (cur * (Ext::from_f64(0.8) + Ext::from_f64(0.4) * cast::<Ext, _>(r.range(0, 1000)) / Ext::from_f64(1000.0)) * Ext::from_f64(100.0)).round() / Ext::from_f64(100.0);
    let owner = if r.chance(cast::<f64, _>(Ext::from_f64(0.8))) { g.cur_player } else { cast::<i32, _>(r.range(1, 5)) };
    let dir = ["UP", "DN", " UP ", "XX"][[0, 0, 0, 1, 1, 1, 2, 3][cast::<usize, _>(r.range(0, 7))]];
    let (q, y, t) = if expire && r.chance(cast::<f64, _>(Ext::from_f64(0.7))) {
        (r.range(1, cast::<i64, _>(g.cur_quarter)), r.range(cast::<i64, _>(g.cur_year) - 1, cast::<i64, _>(g.cur_year)), r.range(0, cast::<i64, _>(g.sim_co_count)))
    } else {
        (r.range(1, 4), r.range(cast::<i64, _>(g.cur_year) - 1, cast::<i64, _>(g.cur_year) + 1), r.range(0, 1600))
    };
    (PbStr::from(format!("{entity}|{owner}|{dir}|{target}|{q}|{y}|{t}").as_str()), entity)
}

/// MSGBOX texts (and ShowQuickMessage popups) from the original's log, in order.
fn orig_msg_texts(o: &Original) -> Vec<String> {
    o.log
        .iter()
        .filter(|l| l.starts_with("MSGBOX") || l.contains("CreateProcessA(0x0, 'qm.exe"))
        .cloned()
        .collect()
}

fn check_msgs(ui: &Rec, o: &Original) {
    let __content = wsr_core::content::current();
    let orig = orig_msg_texts(o);
    assert_eq!(ui.msgs.len(), orig_msgs(o), "messages: port {:#?}{__wsr_text_13990}{:#?}", ui.msgs, orig,__wsr_text_13990=__content.string(13990));
    for ((p, t), l) in ui.msgs.iter().zip(&ui.titles).zip(&orig) {
        if l.starts_with("MSGBOX") {
            // the log shows the text with CR as "\r"
            let want = format!("MSGBOX '{t}' '{}' type=0x2040 ", p.replace('\r', "\\r").replace('\n', "\\n"));
            assert!(l.starts_with(&want), "MSGBOX\n port: {want}\n orig: {l}");
        }
    }
}

#[test]
fn check_price_alerts_matches_original() {
    let mut shown = 0;
    for seed in 1u64..=12 {
        let mut r = Rand(seed * 2_654_435_761);
        let mut g = market(&mut r);
        let n = cast::<i32, _>(r.range(0, 30));
        let mut ents = Vec::new();
        for slot in 1..=n {
            if r.chance(cast::<f64, _>(Ext::from_f64(0.1))) {
                continue;
            }
            let (rec, e) = alert_rec(&g, &mut r, false);
            g.price_alerts[slot] = rec;
            ents.push(e);
        }
        g.price_alerts[0] = PbStr::from(format!(" {}", n + cast::<i32, _>(r.range(-2, 2))).as_str());
        if seed == 3 {
            g.price_alerts[0] = PbStr::new();
        }
        let stocks: Vec<i32> = ents.iter().copied().filter(|&e| (11..=1600).contains(&e)).collect();
        watch_list(&mut g, &mut r, &stocks);
        if seed % 4 == 0 {
            // a stock alert on a corp without shares (silently dropped by the trigger)
            if let Some(&e) = stocks.first() {
                g.shares_outstanding[e] = Ext::from_f64(0.0);
                for k in 1..=15 {
                    let w = g.watch_list[(k, g.cur_player)];
                    if (e - 1..=e + 1).contains(&w) {
                        g.watch_list[(k, g.cur_player)] = 0;
                    }
                }
                if (e - 1..=e + 1).contains(&g.sel_entity) {
                    g.sel_entity = 0;
                }
            }
        }
        let filter = match seed % 3 {
            0 => 0,
            1 => stocks.first().copied().unwrap_or(0),
            _ => cast::<i32, _>(r.range(0, 10)),
        };
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(CHECK_PRICE_ALERTS, &[&filter.to_string()], &g, rng0, &[]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        check_price_alerts(&mut port, &mut ui, &mut rng, filter);
        shown += ui.msgs.len();
        eprintln!("seed {seed}: {} alerts, filter {filter}, {} messages", n, ui.msgs.len());
        check_msgs(&ui, &o);
        assert_eq!(rng.state(), o.rng, "RND state");
        cmp().assert(&port, &o.globals);
    }
    assert!(shown > 0);
}

#[test]
fn expire_price_alerts_matches_original() {
    let mut shown = 0;
    for seed in 1u64..=10 {
        let mut r = Rand(seed * 40_503);
        let mut g = market(&mut r);
        let n = cast::<i32, _>(r.range(0, 40));
        let mut ents = Vec::new();
        for slot in 1..=100 {
            if slot > n && r.chance(cast::<f64, _>(Ext::from_f64(0.97))) {
                continue;
            }
            let (rec, e) = alert_rec(&g, &mut r, true);
            g.price_alerts[slot] = rec;
            ents.push(e);
        }
        g.price_alerts[0] = PbStr::from(format!(" {n}").as_str());
        let stocks: Vec<i32> = ents.iter().copied().filter(|&e| (11..=1600).contains(&e)).collect();
        watch_list(&mut g, &mut r, &stocks);
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(EXPIRE_PRICE_ALERTS, &[], &g, rng0, &[]) else { return };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        expire_price_alerts(&mut port, &mut ui, &mut rng);
        shown += ui.msgs.len();
        eprintln!("seed {seed}: {} messages", ui.msgs.len());
        check_msgs(&ui, &o);
        if let Some(Some(qm)) = o.files.get("TMP_QM.DAT") {
            let want = ui.msgs.last().unwrap().replace("\r\n\r\n", "@@").replace("\r\n", "##");
            let got = String::from_utf8_lossy(qm);
            assert_eq!(got.trim_end_matches("\r\n"), want, "TMP_QM.DAT");
        }
        assert_eq!(rng.state(), o.rng, "RND state");
        cmp().assert(&port, &o.globals);
    }
    assert!(shown > 0);
}

#[test]
fn update_streaming_quotes_matches_original() {
    for seed in 1u64..=8 {
        let mut r = Rand(seed * 7_368_787);
        let mut g = market(&mut r);
        if seed == 2 {
            g.is_computer_player[g.cur_player] = 1;
        }
        if seed == 3 {
            g.player_name[g.cur_player] = st("...OUT...");
        }
        let center = cast::<i32, _>(r.range(11, 1600));
        watch_list(&mut g, &mut r, &[center]);
        let (from, to) = if seed % 3 == 0 { (0, 2000) } else { (center - 1, center + 1) };
        let rng0 = cast::<u32, _>(r.next());
        let Some(o) = run(UPDATE_STREAMING_QUOTES, &[&from.to_string(), &to.to_string()], &g, rng0, &[]) else {
            return;
        };
        let mut port = g.clone();
        let mut ui = Rec::default();
        let mut rng = Rng::from_state(rng0);
        update_streaming_quotes(&mut port, &mut ui, &mut rng, from, to);
        assert_eq!(rng.state(), o.rng, "RND state");
        cmp().assert(&port, &o.globals);
    }
}

#[test]
fn check_price_alerts_unit() {
    let mut g = Globals::default();
    g.cur_player = 1;
    g.num_players = 1;
    g.cur_player_name = st("Ann");
    g.entity_status[20] = 1;
    g.ent_name[20] = st("Acme");
    g.ent_symbol[20] = st("ACM");
    g.market_cap[20] = Ext::from_f64(150.0);
    g.shares_outstanding[20] = Ext::from_f64(10.0);
    g.gold_price = cast::<f32, _>(Ext::from_f64(2000.0));
    g.price_alerts[0] = st("4");
    g.price_alerts[1] = st("20|1|UP|100|1|2000|5");
    g.price_alerts[2] = st("3003|1|DN|1500|1|2000|5");
    g.price_alerts[3] = st("3003|1|UP|1500|1|2000|5");
    g.price_alerts[4] = st("5|1|UP|1|1|2000|5");
    let mut ui = Rec::default();
    let mut rng = Rng::from_state(1);
    check_price_alerts(&mut g, &mut ui, &mut rng, 0);
    assert_eq!(ui.msgs.len(), 2);
    assert!(ui.msgs[0].contains("STOCK:  Acme (ACM)"));
    assert!(ui.msgs[0].contains("of 10.00, and is currently priced at 15.00 per share."));
    assert!(ui.msgs[1].contains("COMMODITY:  GOLD"));
    assert!(ui.msgs[1].contains("of $1,500.00, and is currently priced at $2,000.00 per ounce."));
    // the triggered alerts and the one on entity 5 are removed; the list is packed
    let left: Vec<String> = (1..=4).map(|k| g.price_alerts[k].to_display()).filter(|x| !x.is_empty()).collect();
    assert_eq!(left, ["3003|1|DN|1500|1|2000|5"]);
}
