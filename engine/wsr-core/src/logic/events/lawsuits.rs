//! Malicious-prosecution countersuits: `ProcessMaliciousLawsuits` resolves the queued harassment suits
//! (`LawsuitRec` records `"2|defendant|plaintiff|timer|"`).
use crate::pb::real::{Ext, cast};


use super::prelude::*;
use crate::logic::finance::calc_entity_financials;
use crate::logic::report::show_quick_message;

fn s(x: &str) -> PbStr {
    PbStr::from(x)
}

/// Port of Lawsuits_ShiftQueue (0x4b1fbd), GOSUB of ProcessMaliciousLawsuits: drops `LawsuitRec(1)`
/// (shifts 2..10 down, clears 10).
fn lawsuits_shift_queue(g: &mut Globals) {
    for i in 1..=9 {
        g.lawsuit_rec[i] = g.lawsuit_rec[i + 1].clone();
    }
    g.lawsuit_rec[10] = PbStr::new();
}

/// Port of ProcessMaliciousLawsuits (0x4b145b) with its GOSUB Lawsuits_ShiftQueue (0x4b1fbd) and exit
/// block ProcessMaliciousLawsuits_Exit (0x4b20f6).
/// Runs at ticks 135, 422, 651, 922 and 1301. With no countersuit running, a queued harassment suit
/// (`"2|…"`, an empty first slot is shifted out first) is dismissed as frivolous and the harassed company
/// countersues (`MaliciousSuitDefendant`/`Plaintiff`, timer at least 2); otherwise the timer counts
/// down (0 when a party is gone). At timer 1 the countersuit is announced ("files"), at 0 it is dropped.
/// With probability `timer / 100` (rounded RND) the judge awards 100..2000 (plus 100 per notoriety point
/// of a controlling player, at most the defendant's net worth, at least 50..100) from the defendant
/// (cash, extraordinary P&L) to the plaintiff (cash, extraordinary items) and the suit ends.
pub fn process_malicious_lawsuits(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    let __content = crate::content::current();
    // ON ERROR GOTO -> ShowRuntimeError("Harass") (error path not ported)
    let tick = g.sim_co_count;
    if tick != 135 && tick != 422 && tick != 651 && tick != 922 && g.sim_co_count != 1301 {
        return;
    }
    if g.lawsuit_rec[1].is_empty() {
        lawsuits_shift_queue(g);
    }
    let mut detail_text = g.lawsuit_rec[1].clone();
    let mut part_text;
    if g.malicious_suit_timer < 1 {
        if mid(detail_text.as_bytes(), 1, 1) != "2" {
            return;
        }
        let field = |n: i32| val(parse_any(detail_text.as_bytes(), b"|", n).as_bytes());
        g.malicious_suit_defendant = to_long(field(2));
        g.malicious_suit_plaintiff = to_long(field(3));
        g.malicious_suit_timer = to_integer(field(4));
        lawsuits_shift_queue(g);
        if g.malicious_suit_timer < 2 {
            g.malicious_suit_timer = 2;
        }
        if g.malicious_suit_defendant < 1 || g.malicious_suit_plaintiff < 12 {
            g.malicious_suit_defendant = 0;
            g.malicious_suit_plaintiff = 0;
            g.malicious_suit_timer = 0;
            return;
        }
        g.news_msg = g.ent_symbol[g.malicious_suit_defendant].clone()
            + __content.string(5206)
            + &g.ent_symbol[g.malicious_suit_plaintiff]
            + __content.string(5207);
        let (d, p) = (g.malicious_suit_defendant, g.malicious_suit_plaintiff);
        post_news(g, ui, d, p);
    } else {
        g.malicious_suit_timer = g.malicious_suit_timer.wrapping_sub(1);
    }
    let defendant_id = g.malicious_suit_defendant;
    let plaintiff_id = g.malicious_suit_plaintiff;
    if !(g.entity_status[g.malicious_suit_defendant] == 1 && g.entity_status[plaintiff_id] == 1) {
        g.malicious_suit_timer = 0;
    }
    if g.malicious_suit_timer < 2 {
        part_text = s(" drops");
        detail_text = PbStr::new();
        if g.malicious_suit_timer == 1 {
            part_text = s(" files");
            detail_text = s(__content.string(5208))
                + &g.ent_symbol[defendant_id]
                + __content.string(5209)
                + __content.string(5210);
        }
        g.news_msg = g.ent_symbol[plaintiff_id].clone()
            + &part_text
            + __content.string(5211)
            + &g.ent_name[defendant_id];
        if g.malicious_suit_defendant < 11 {
            if right(g.news_msg.as_bytes(), 1) != "." {
                g.news_msg = g.news_msg.clone() + ".";
            }
        } else {
            g.news_msg = g.news_msg.clone() + " (" + &g.ent_symbol[defendant_id] + "). ";
        }
        let mut quick = g.news_msg.clone() + &detail_text;
        show_quick_message(g, ui, &mut quick);
        post_news(g, ui, 0, 0);
        if g.malicious_suit_timer == 0 {
            g.malicious_suit_defendant = 0;
            g.malicious_suit_plaintiff = 0;
            return;
        }
    }
    let mut damages = to_long(rng.rnd() * Ext::from_f64(100.0));
    if (cast::<i32, _>(g.malicious_suit_timer)) < damages {
        return;
    }
    damages = rng.rnd_range(20, 1);
    damages = damages.wrapping_mul(100);
    let def_controller = g.corp_controller[defendant_id];
    if 0 < def_controller && def_controller < 11 {
        damages =
            damages.wrapping_add((cast::<i32, _>(g.player_notoriety[def_controller])).wrapping_mul(100));
    }
    calc_entity_financials(g, ui, rng, defendant_id);
    if g.net_worth < cast::<Ext, _>(damages) {
        damages = to_long(g.net_worth);
    }
    if damages < 50 {
        damages = to_long(rng.rnd() * Ext::from_f64(50.0) + Ext::from_f64(50.0));
    }
    detail_text = fmt_trim(cast::<Ext, _>(damages), b"* ###,") + " " + &g.million_prefix_lc + "il. ";
    if 10 < defendant_id {
        part_text = g.ent_symbol[defendant_id].clone();
    } else {
        part_text = g.ent_name[defendant_id].clone();
    }
    let plaintiff_sym = g.ent_symbol[plaintiff_id].clone();
    g.malicious_suit_timer = 0;
    g.malicious_suit_defendant = 0;
    g.malicious_suit_plaintiff = 0;
    if !g.news_msg.is_empty() {
        post_news(g, ui, 0, 0);
    }
    g.news_msg = s(__content.string(5212))
        + &plaintiff_sym
        + " "
        + &trim(g.currency_symbol.as_bytes())
        + &detail_text
        + __content.string(5213);
    let min_len = 8;
    if len(part_text.as_bytes()) < min_len {
        g.news_msg = g.news_msg.clone() + " vs. " + &part_text;
    }
    let mut quick = g.news_msg.clone()
        + __content.string(5214)
        + &part_text
        + ".";
    show_quick_message(g, ui, &mut quick);
    post_news(g, ui, 0, 0);
    g.news_msg = part_text.clone()
        + __content.string(4728)
        + &trim(g.currency_symbol.as_bytes())
        + &detail_text
        + __content.string(5215);
    if len(part_text.as_bytes()) < min_len {
        g.news_msg = PbStr::new();
    } else {
        post_news(g, ui, 0, 0);
    }
    g.cash[defendant_id] = g.cash[defendant_id] - cast::<Ext, _>(damages);
    g.extraordinary_pl[defendant_id] =
        to_quad(cast::<Ext, _>(g.extraordinary_pl[defendant_id]) - cast::<Ext, _>(damages));
    g.cash[plaintiff_id] = g.cash[plaintiff_id] + cast::<Ext, _>(damages);
    g.extra_items_ytd[plaintiff_id] = g.extra_items_ytd[plaintiff_id] + cast::<Ext, _>(damages);
}
