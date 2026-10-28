//! Bond trading and liquidity of the financial institutions (banks 11..85, insurers 86..130):
//! `AiFinInstBondTrading` (0x4dff05), `AiRandSellJunkBonds` (0x4f721b), `BuyCorpBonds` (0x4f59c5),
//! `SellCorpBonds` (0x4f670d), `InsurerGovBondTrading` (0x4e5b4b), `TradeGovBonds` (0x6a16ad),
//! `SubprimeSecsWriteDown` (0x508952), `BankSystemLiquidity` (0x4e635d) and `BankAllocateAssets` (0x69c127).
use crate::pb::real::{Ext, cast};


pub use super::corp_ops::{bank_allocate_assets, trade_gov_bonds};
#[allow(unused_imports)]
use super::prelude::*;
use crate::logic::cashflow::get_etf_adviser_human_status;
use crate::logic::earnings::{balance_cash_t_bills, calc_accrued_bond_interest};
use crate::logic::finance::{calc_entity_financials, get_controller_type, get_managing_player};
use crate::logic::holdings::find_bond_holding_slot;
use crate::logic::newgame_helpers::create_new_bank_in_free_slot;
use crate::logic::news::is_in_recent_headlines;
use crate::pb::numeric::Cur;

/// `RTRIM$(LTRIM$(FORMAT$(x, mask)))`.
fn fmt_trim(x: Ext, mask: &str) -> PbStr {
    trim(&format(x, mask.as_bytes()))
}

/// Port of AiFinInstBondTrading (0x4dff05).
/// Corporate-bond trading of an AI bank or insurer `entity_id` (11..130, active; not a human-controlled
/// bank, no managing player): a held bond that is junk (rating < 3), very expensive (price above
/// 150..325) or — with more than 4 holdings — picked at random (1 in 3) triggers
/// [`ai_rand_sell_junk_bonds`] (for `SimCoCount`!) and ends the routine. Otherwise, 3 times in 8, it may
/// sell junk instead (rates under 6..7.5 and liquid funds under 2000, 1 in 3), stops for corp-controlled
/// holders of more than 3 issues or when short of cash, and else spends 20% of its cash + T-bills
/// (at least 250) on the bonds of a random issuer (up to 50 tries: over 1500 issued, at least 800
/// publicly held, priced 50..60 to 97 — or up to 120 if it is `PlayerRecentCorp` —, convertibles half
/// the time only if `PlayerRecentCorp`) via [`buy_corp_bonds`] (mode 1 for computer-player controlled
/// holders, else 2). Clears `PlayerRecentCorp` when it is the candidate.
pub fn ai_fin_inst_bond_trading(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, entity_id: i32) {
    // ON ERROR GOTO: ShowRuntimeError("AiFinInstBondTrading ...") unless DataPath contains a marker —
    // error path not ported.
    let inst_id = entity_id;
    if !(g.entity_status[inst_id] == 1 && 11 <= inst_id && inst_id <= 130) {
        return;
    }
    if get_controller_type(g, inst_id) == 1 && g.industry_num[inst_id] == 1 {
        return;
    }
    if get_managing_player(g, inst_id) > 0 {
        return;
    }
    for slot in 1..=15 {
        if 0 < g.hold_bond_id[(slot, inst_id)] {
            let issuer = g.hold_bond_id[(slot, inst_id)];
            let mut keep = true;
            if 4 < g.hold_bond_id[(0, inst_id)] && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0) {
                keep = false;
            }
            if g.credit_rating_num[issuer] < 3 {
                keep = false;
            } else {
                let x = Ext::from_f64(175.0) * rng.rnd() + Ext::from_f64(150.0);
                if g.bond_price[issuer].get() > x {
                    keep = false;
                }
            }
            if !keep {
                ai_rand_sell_junk_bonds(g, ui, rng);
                return;
            }
        }
    }
    if Ext::from_f64(4.0) * rng.rnd() > sf(1.5f32) {
        return;
    }
    if (cast::<Ext, _>(g.base_interest_rate)) < sf(1.5f32) * rng.rnd() + Ext::from_f64(6.0)
        && g.t_bills[inst_id] + g.cash[inst_id] < g.exchange_rate * Ext::from_f64(2000.0)
        && Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(1.0)
    {
        ai_rand_sell_junk_bonds(g, ui, rng);
        return;
    }
    if 4 <= g.hold_bond_id[(0, inst_id)] && 10 < g.corp_controller[inst_id] {
        return;
    }
    calc_entity_financials(g, ui, rng, inst_id);
    let liquid = g.t_bills[inst_id] + g.cash[inst_id];
    if liquid < Ext::from_f64(1000.0) * rng.rnd() + Ext::from_f64(5000.0) {
        let liquid = g.t_bills[inst_id] + g.cash[inst_id];
        if g.total_assets * sf(0.05f32) > liquid {
            return;
        }
        if g.tmp_corp_bonds_value > g.tmp_gov_bonds_value * cast::<Ext, _>(g.industry_num[inst_id]) * Ext::from_f64(2.0) {
            return;
        }
    }
    let mut buy_amt = to_quad((g.t_bills[inst_id] + g.cash[inst_id]) * sf(0.2f32));
    if (cast::<Ext, _>(buy_amt)) < Ext::from_f64(250.0) {
        return;
    }
    let mut tries = 0;
    let mut target: i32 = 0;
    while target < 11 {
        tries += 1;
        target = rng.rnd_range(1600, 11);
        if Ext::from_f64(3.0) * rng.rnd() < Ext::from_f64(1.0) && 10 < g.player_recent_corp {
            target = cast::<i32, _>(g.player_recent_corp);
        }
        if g.entity_status[target] != 1 {
            target = 0;
        } else if !(cast::<Ext, _>(g.bonds_issued[target]) > Ext::from_f64(1500.0)) {
            target = 0;
        } else {
            calc_entity_financials(g, ui, rng, target);
            if !(cast::<Ext, _>(g.bonds_public_held) >= Ext::from_f64(800.0)) {
                target = 0;
            } else {
                let x = Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(50.0);
                let price = g.bond_price[target].get();
                if !(price < x) && price <= Ext::from_f64(97.0) {
                    if Ext::from_f64(2.0) * rng.rnd() > Ext::from_f64(1.0)
                        && 0 < instr(1, g.bond_issue_info[target].as_bytes(), b"cv")
                        && cast::<i32, _>(g.player_recent_corp) != target
                    {
                        target = 0;
                    }
                } else if g.bond_price[target].get() > Ext::from_f64(120.0)
                    || cast::<i32, _>(g.player_recent_corp) != target
                {
                    target = 0;
                }
            }
        }
        if target == inst_id {
            target = 0;
        }
        if cast::<i32, _>(g.player_recent_corp) == target {
            g.player_recent_corp = 0;
        }
        if tries == 50 {
            return;
        }
    }
    if cast::<Ext, _>(buy_amt) > cast::<Ext, _>(g.bonds_public_held) {
        buy_amt = g.bonds_public_held;
    }
    g.tmp_ext1 = cast::<Ext, _>(buy_amt);
    g.tmp_ext2 = Ext::from_f64(2.0);
    if get_controller_type(g, inst_id) == 2 {
        g.tmp_ext2 = Ext::from_f64(1.0);
    }
    buy_corp_bonds(g, ui, rng, inst_id, target);
}

/// Port of AiRandSellJunkBonds (0x4f721b).
/// The institution `SimCoCount` (11..130, active; not a human-controlled bank, no managing player) sells
/// corporate bonds: the first held issue rated below 2..4 or priced above 175..225 (else a random slot,
/// or slot 3 when holding more than 3 issues), unless the issuer's latest quarterly earnings
/// (`EntMiscData` field 1) fell from field 3 but are positive and below the projection — overridden for
/// small positions (face under 0..200). Large positions not picked as junk are sold only 45%. Calls
/// [`sell_corp_bonds`] in mode 2.
pub fn ai_rand_sell_junk_bonds(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    // procTag = "RandSellJunk"; ON ERROR -> ShowRuntimeError (error path not ported)
    let holder_id = g.sim_co_count;
    if holder_id < 11 || 130 < holder_id {
        return;
    }
    if g.entity_status[holder_id] != 1 {
        return;
    }
    if get_controller_type(g, holder_id) == 1 && holder_id < 86 {
        return;
    }
    if get_managing_player(g, holder_id) > 0 {
        return;
    }
    let mut forced_sell = 0;
    let mut slot = rng.rnd_range(15, 1);
    if 3 < g.hold_bond_id[(0, holder_id)] {
        slot = 3;
        forced_sell = 1;
    }
    let count = g.hold_bond_id[(0, holder_id)];
    let mut i = 1;
    while i <= count {
        if 0 < g.hold_bond_id[(i, holder_id)] {
            let bond_issuer = g.hold_bond_id[(i, holder_id)];
            let r = rng.rnd_range(3, 1) + 1;
            let mut sell = r > g.credit_rating_num[bond_issuer];
            if !sell {
                let x = Ext::from_f64(50.0) * rng.rnd() + Ext::from_f64(175.0);
                sell = g.bond_price[bond_issuer].get() > x;
            }
            if sell {
                slot = i;
                forced_sell = 1;
                break;
            }
        }
        i += 1;
    }
    if g.hold_bond_id[(slot, holder_id)] == 0 {
        return;
    }
    let issuer_id = g.hold_bond_id[(slot, holder_id)];
    let mut do_sell = 1;
    if 20 < len(g.ent_misc_data[issuer_id].as_bytes()) {
        let field = parse_any(g.ent_misc_data[issuer_id].as_bytes(), b"|", 3);
        let prev_earnings = val(field.as_bytes());
        let field = parse_any(g.ent_misc_data[issuer_id].as_bytes(), b"|", 1);
        let last_earnings = val(field.as_bytes());
        if prev_earnings > last_earnings
            && g.proj_qtr_earnings[issuer_id] > last_earnings
            && last_earnings > Ext::from_f64(0.0)
        {
            do_sell = 0;
        }
    }
    let mut face_amt = g.hold_bond_face[(slot, holder_id)];
    let x = Ext::from_f64(200.0) * rng.rnd();
    if (cast::<Ext, _>(face_amt)) < x {
        do_sell = 1;
    }
    if do_sell == 0 {
        return;
    }
    let x = Ext::from_f64(1000.0) * rng.rnd() + Ext::from_f64(1000.0);
    if cast::<Ext, _>(face_amt) > x && forced_sell == 0 {
        face_amt = to_quad(cast::<Ext, _>(face_amt) * sf(0.45f32));
    }
    g.tmp_ext1 = cast::<Ext, _>(face_amt);
    g.tmp_ext2 = Ext::from_f64(2.0);
    sell_corp_bonds(g, ui, rng, holder_id, issuer_id);
}

/// Port of BuyCorpBonds (0x4f59c5).
/// `buyer_id` buys `TmpExt1` (face) of `issuer_id`'s bonds; `TmpExt2` = mode (0 = player order with
/// confirmation boxes, > 0 = AI; both cleared). Needs a bond slot ([`find_bond_holding_slot`]); the face is
/// capped at the publicly held bonds (AI: at least 100). Price = bond price x face x (1 + premium for the
/// demand: 3% x face / (held + issued) x 0.05, players 0.25 x difficulty) x 1.002, plus accrued interest;
/// trimmed by up to 1 to fit the buyer's liquid funds, else nothing (player: error box). A cash shortfall is
/// borrowed from the lending bank (players: `BorrowToCoverPurchase`; NO cancels). Books cash, accrued P&L
/// (the bought accrued interest), face and cost; news (AI: only 1 in 10 into a free `NewsMsg`, unless the
/// buyer is a fund advised by the current player).
pub fn buy_corp_bonds(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    buyer_id: i32,
    issuer_id: i32,
) {
    let __content = crate::content::current();
    // ON ERROR -> ShowRuntimeError (error path not ported)
    let fmt_money = "* ###,";
    let issuer = issuer_id;
    let mut face_amt = to_quad(g.tmp_ext1);
    let mode = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let slot = find_bond_holding_slot(g, buyer_id, issuer);
    if slot == 0 {
        return;
    }
    if mode != 0 {
        calc_entity_financials(g, ui, rng, issuer);
    }
    let demand_factor =
        cast::<Ext, _>(face_amt) / (cast::<Ext, _>(g.bonds_issued[issuer]) + cast::<Ext, _>(g.bonds_public_held)) * sf(0.03f32);
    if cast::<Ext, _>(face_amt) > cast::<Ext, _>(g.bonds_public_held) {
        face_amt = g.bonds_public_held;
        if !(cast::<Ext, _>(face_amt) >= Ext::from_f64(100.0)) && mode > 0 {
            return;
        }
    }
    if !(cast::<Ext, _>(face_amt) > Ext::from_f64(0.0)) {
        return;
    }
    let mut premium_scale = cast::<f32, _>(cast::<Ext, _>(g.difficulty_level) * sf(0.25f32));
    if mode > 0 {
        premium_scale = 0.05f32;
    }
    let accrued_int = calc_accrued_bond_interest(g, face_amt, issuer);
    let mut total_cost = sf(0.01f32)
        * g.bond_price[issuer].get()
        * cast::<Ext, _>(face_amt)
        * (demand_factor * cast::<Ext, _>(premium_scale) + Ext::from_f64(1.0));
    total_cost = sf(1.002f32) * total_cost;
    total_cost = accrued_int + total_cost;
    let premium = total_cost - sf(0.01f32) * g.bond_price[issuer].get() * cast::<Ext, _>(face_amt);
    calc_entity_financials(g, ui, rng, buyer_id);
    while g.liquid_funds < total_cost && total_cost - g.liquid_funds < Ext::from_f64(1.0) {
        total_cost = total_cost - sf(0.1f32);
    }
    if g.liquid_funds < total_cost {
        if mode == 0 {
            let msg = PbStr::from(__content.string(4148))
                + &g.ent_name[buyer_id]
                + __content.string(10842)
                + __content.string(10843)
                + __content.string(10844)
                + &g.currency_symbol
                + &fmt_trim(total_cost - g.liquid_funds, fmt_money)
                + " "
                + &g.million_prefix_uc
                + "ILLION.";
            ui.msgbox(&msg, 0x2010, &PbStr::from(__content.string(1928)));
        }
        return;
    }
    let mut shortfall = total_cost - (g.cash[buyer_id] + g.t_bills[buyer_id]);
    if shortfall > Ext::from_f64(0.0) {
        if mode == 0 {
            if crate::logic::trading::borrow_to_cover_purchase(g, ui, &mut shortfall, premium, 0) == 7 {
                return;
            }
        } else {
            g.cash[buyer_id] = shortfall + g.cash[buyer_id];
            let lb = cast::<i32, _>(g.lending_bank[buyer_id]);
            g.cash[lb] = g.cash[lb] - shortfall;
            g.bank_debt[buyer_id] = shortfall + g.bank_debt[buyer_id];
        }
    }
    g.cash[buyer_id] = g.cash[buyer_id] - total_cost;
    g.accrued_pl[buyer_id] = g.accrued_pl[buyer_id] - accrued_int;
    balance_cash_t_bills(g, buyer_id);
    g.hold_bond_id[(slot, buyer_id)] = issuer;
    g.hold_bond_face[(slot, buyer_id)] =
        to_quad(cast::<Ext, _>(face_amt) + cast::<Ext, _>(g.hold_bond_face[(slot, buyer_id)]));
    g.bond_hold_cost[(slot, buyer_id)] =
        (total_cost - accrued_int) + g.bond_hold_cost[(slot, buyer_id)];
    if mode == 0 {
        let maturity_year = mid(g.bond_issue_info[issuer].as_bytes(), 1, 4);
        let mut coupon_text = right(g.bond_issue_info[issuer].as_bytes(), 5);
        if val(coupon_text.as_bytes()) < Ext::from_f64(10.0) {
            coupon_text = right(coupon_text.as_bytes(), 4);
        }
        coupon_text = coupon_text + "% ";
        if 0 < instr(1, g.bond_issue_info[issuer].as_bytes(), b"cv") {
            coupon_text = coupon_text + __content.string(10845);
        }
        let msg = PbStr::from(__content.string(10846))
            + "\r"
            + "\r"
            + &g.ent_name[buyer_id]
            + __content.string(10847)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(face_amt), "* ###,")
            + " "
            + &g.million_prefix_lc
            + "illion "
            + __content.string(10848)
            + &g.ent_name[issuer]
            + " "
            + &coupon_text
            + __content.string(10849)
            + &maturity_year
            + ", "
            + __content.string(10850)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(total_cost - accrued_int, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + ", "
            + __content.string(10851)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(accrued_int, "* ###,.00")
            + " "
            + &g.million_prefix_lc
            + "illion."
            + "\r"
            + "\r";
        ui.msgbox(&msg, 0x2000, &PbStr::from(__content.string(10729)));
        g.turn_action_count += 1;
        if g.cash[buyer_id].abs() < sf(0.015f32) && total_cost > Ext::from_f64(50.0) {
            g.cash[buyer_id] = Ext::from_f64(0.0);
        }
    }
    let mut msg_text = right(g.bond_issue_info[issuer].as_bytes(), 5);
    if val(msg_text.as_bytes()) < Ext::from_f64(10.0) {
        msg_text = right(msg_text.as_bytes(), 4);
    }
    msg_text = PbStr::from(" ") + &msg_text + "%";
    let news_text = g.ent_symbol[buyer_id].clone()
        + __content.string(2062)
        + &g.ent_symbol[issuer]
        + &msg_text
        + __content.string(10852)
        + &mid(g.bond_issue_info[issuer].as_bytes(), 1, 4)
        + ".";
    if mode != 0 && get_etf_adviser_human_status(g, ui, rng, buyer_id) != 2 {
        if g.news_msg.is_empty() && Ext::from_f64(10.0) * rng.rnd() < Ext::from_f64(1.0) {
            g.news_msg = news_text;
        }
        return;
    }
    g.news_msg = news_text;
    post_news(g, ui, 0, 0);
}

/// Port of SellCorpBonds (0x4f670d).
/// `seller_id` (<= 1583) sells up to `TmpExt1` (face, capped at its holding) of `issuer_id`'s bonds;
/// `TmpExt2` = mode (0 = player with confirmation box, > 0 = AI; both cleared). Proceeds = price before
/// the sale x face x (1 - discount) x 0.998 (discount 5% for the AI, else 0.15 x difficulty x the market
/// impact); the bond price drops by the market impact (5% of face / (held + issued), at most 2.5%). Gain
/// over the proportional cost goes to `PlayerCapGains` (players) or `ExtraordinaryPL`; the accrued
/// interest is received. A fully sold slot is cleared. News: posted for players; for the AI it is left in
/// `NewsMsg` (cleared again if a similar headline is recent and the seller is not the current player's).
pub fn sell_corp_bonds(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    seller_id: i32,
    issuer_id: i32,
) {
    let __content = crate::content::current();
    // procTag = "SellJunk"; ON ERROR -> ShowRuntimeError (error path not ported)
    let mut face_to_sell = to_quad(g.tmp_ext1);
    let mode = to_long(g.tmp_ext2);
    g.tmp_ext1 = Ext::from_f64(0.0);
    g.tmp_ext2 = Ext::from_f64(0.0);
    let issuer = issuer_id;
    if g.entity_status[issuer] != 1 || !(cast::<Ext, _>(face_to_sell) > Ext::from_f64(0.0)) || 1583 < seller_id {
        return;
    }
    calc_entity_financials(g, ui, rng, issuer);
    let bond_price = g.bond_price[issuer];
    let mut held_slot = 0;
    let mut held_face: i64 = 0;
    for i in 1..=15 {
        if issuer == g.hold_bond_id[(i, seller_id)] {
            held_slot = i;
            held_face = g.hold_bond_face[(i, seller_id)];
            if cast::<Ext, _>(face_to_sell) > cast::<Ext, _>(held_face) {
                face_to_sell = held_face;
            }
        }
    }
    if held_slot == 0
        || cast::<Ext, _>(face_to_sell) == Ext::from_f64(0.0)
        || cast::<Ext, _>(held_face) == Ext::from_f64(0.0)
        || cast::<Ext, _>(g.bonds_issued[issuer]) == Ext::from_f64(0.0)
    {
        return;
    }
    let mut market_impact =
        cast::<f32, _>(cast::<Ext, _>(face_to_sell) / (cast::<Ext, _>(g.bonds_issued[issuer]) + cast::<Ext, _>(g.bonds_public_held)));
    if Ext::from_f32(0.5f32) < market_impact || market_impact == Ext::from_f32(0.0f32) {
        market_impact = 0.5f32;
    }
    market_impact = cast::<f32, _>(sf(0.05f32) * cast::<Ext, _>(market_impact));
    let mut fraction: Ext = cast::<Ext, _>(market_impact) * (cast::<Ext, _>(g.difficulty_level) * sf(0.15f32));
    if mode > 0 {
        fraction = sf(0.05f32);
    }
    let mut proceeds = sf(0.01f32) * cast::<Ext, _>(face_to_sell) * ((Ext::from_f64(1.0) - fraction) * bond_price.get());
    proceeds = sf(0.998f32) * proceeds;
    let p = (Ext::from_f64(1.0) - cast::<Ext, _>(market_impact)) * g.bond_price[issuer].get();
    g.bond_price[issuer] = Cur::from_f64(p);
    let accrued_int = calc_accrued_bond_interest(g, face_to_sell, issuer);
    fraction = cast::<Ext, _>(face_to_sell) / cast::<Ext, _>(held_face);
    let cost_sold = fraction * g.bond_hold_cost[(held_slot, seller_id)];
    if g.num_players < seller_id {
        g.extraordinary_pl[seller_id] =
            to_quad((proceeds - cost_sold) + cast::<Ext, _>(g.extraordinary_pl[seller_id]));
    } else {
        g.player_cap_gains[seller_id] = (proceeds - cost_sold) + g.player_cap_gains[seller_id];
    }
    g.accrued_pl[seller_id] = accrued_int + g.accrued_pl[seller_id];
    g.cash[seller_id] = (proceeds + g.cash[seller_id]) + accrued_int;
    g.bond_hold_cost[(held_slot, seller_id)] = g.bond_hold_cost[(held_slot, seller_id)]
        - g.bond_hold_cost[(held_slot, seller_id)] * fraction;
    g.hold_bond_face[(held_slot, seller_id)] =
        to_quad(cast::<Ext, _>(g.hold_bond_face[(held_slot, seller_id)]) - cast::<Ext, _>(face_to_sell));
    if fraction == Ext::from_f64(1.0) {
        g.bond_hold_cost[(held_slot, seller_id)] = Ext::from_f64(0.0);
        g.hold_bond_id[(held_slot, seller_id)] = 0;
        g.hold_bond_face[(held_slot, seller_id)] = 0;
        if 0 < g.hold_bond_id[(0, seller_id)] {
            g.hold_bond_id[(0, seller_id)] -= 1;
        }
    }
    if g.news_msg.is_empty() || mode == 0 || g.corp_controller[issuer] <= g.num_players {
        let mut bond_kind = PbStr::from(__content.string(10853));
        if g.credit_rating_num[issuer] < 7 {
            bond_kind = PbStr::from(" junk");
        }
        g.news_msg = g.ent_symbol[seller_id].clone()
            + " sells "
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(cast::<Ext, _>(face_to_sell), "* ###,")
            + " "
            + &g.million_prefix_lc
            + __content.string(10854)
            + &g.ent_symbol[issuer]
            + &bond_kind
            + " bonds.";
        if mode == 0 {
            post_news(g, ui, 0, 0);
        } else if g.cur_player != g.corp_controller[seller_id]
            && is_in_recent_headlines(g, __content.bytes(10855)) > 0
        {
            g.news_msg = PbStr::new();
        }
    }
    if mode == 0 {
        let msg = PbStr::from(__content.string(10856))
            + "\r"
            + "\r"
            + &g.ent_symbol[seller_id]
            + __content.string(3012)
            + &trim(g.currency_symbol.as_bytes())
            + &trim(str_num(cast::<Ext, _>(face_to_sell), 15).as_bytes())
            + " "
            + &g.million_prefix_lc
            + __content.string(10857)
            + &g.ent_name[issuer]
            + __content.string(10858)
            + __content.string(10859)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(proceeds, "* ###,.0")
            + " "
            + &g.million_prefix_lc
            + "illion"
            + &g.currency_suffix
            + __content.string(3272)
            + &trim(g.currency_symbol.as_bytes())
            + &fmt_trim(accrued_int, "* ###,.0")
            + " "
            + &g.million_prefix_lc
            + "illion";
        ui.msgbox(&msg, 0x2000, &PbStr::from(__content.string(10729)));
        g.turn_action_count += 1;
    }
}

/// Port of InsurerGovBondTrading (0x4e5b4b) with its GOSUB InsurerBonds_BuyGovBonds (0x4e6121).
/// Government-bond trading of an AI insurer (`insurer_id` > 10, active, industry 2, no managing player):
/// short of cash (cash + T-bills under 2500 and at most 10% of its gov bonds, and not merely a high-rate
/// quarter with positive cash) it sells 1000 (face) of its long bonds — or the short ones when the long
/// holding is under 1000 or, 1 in 3, smaller than the short one — with a news line (1 in 11, or for the
/// current player's insurer). With ample cash (above 2500..5000 plus capital and income taxes) and rates
/// above 6..8 it buys 1000..2000 (face) of short notes (long bonds over 60% of the shorts and rates under 8)
/// or long bonds, news 1 in 5 (`LastTickerNewsKind = 2`).
pub fn insurer_gov_bond_trading(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, insurer_id: i32) {
    let __content = crate::content::current();
    // ON ERROR -> ShowRuntimeError (error path not ported)
    let ins = insurer_id;
    if ins < 11 {
        return;
    }
    if g.entity_status[ins] != 1 || g.industry_num[ins] != 2 {
        return;
    }
    if get_managing_player(g, ins) > 0 {
        return;
    }
    if !(g.cash[ins] > Ext::from_f64(0.0)) {
        balance_cash_t_bills(g, ins);
    }
    calc_entity_financials(g, ui, rng, ins);
    let liquid = g.t_bills[ins] + g.cash[ins];
    if g.exchange_rate * Ext::from_f64(2500.0) > liquid
        && !(g.t_bills[ins] + g.cash[ins] > g.tmp_gov_bonds_value * sf(0.1f32))
    {
        let x = sf(1.2f32) * rng.rnd() + Ext::from_f64(6.0);
        if x < cast::<Ext, _>(g.base_interest_rate) && g.cash[ins] > Ext::from_f64(0.0) {
            return;
        }
        if !(cast::<Ext, _>(g.long_gov_bond_face[ins]) >= Ext::from_f64(1000.0))
            && (cast::<Ext, _>(g.short_gov_bond_face[ins])) < Ext::from_f64(1000.0)
        {
            return;
        }
        let mut bond_term = 1;
        let mut held = g.long_gov_bond_face[ins];
        if (cast::<Ext, _>(held)) < Ext::from_f64(1000.0) {
            bond_term = 2;
        }
        if Ext::from_f64(3.0) * rng.rnd() > Ext::from_f64(2.0)
            && cast::<Ext, _>(g.short_gov_bond_face[ins]) > cast::<Ext, _>(g.long_gov_bond_face[ins])
        {
            bond_term = 2;
        }
        if bond_term == 1 {
            if !((cast::<Ext, _>(held)) < Ext::from_f64(1000.0)) {
                trade_gov_bonds(g, ins, 1000, 1, 2);
            }
        } else {
            held = g.short_gov_bond_face[ins];
            if !((cast::<Ext, _>(held)) < Ext::from_f64(1000.0)) {
                trade_gov_bonds(g, ins, 1000, 2, 2);
            }
        }
        if g.last_ticker_news_kind != 2
            && (g.rand_seed % 11 == 3 || g.cur_player == g.corp_controller[ins])
        {
            g.news_msg = g.ent_symbol[ins].clone() + __content.string(10860);
        }
        return;
    }
    let liquid = g.t_bills[ins] + g.cash[ins];
    let x = (Ext::from_f64(2500.0) * rng.rnd() + Ext::from_f64(2500.0) + g.tmp_accrued_inc_tax) + g.capital_taxes_total;
    if x < liquid {
        let y = Ext::from_f64(2.0) * rng.rnd() + Ext::from_f64(6.0);
        if y < cast::<Ext, _>(g.base_interest_rate) && g.industry_num[ins] == 2 {
            // GOSUB InsurerBonds_BuyGovBonds
            let mut buy_face = to_quad(Ext::from_f64(10.0) * rng.rnd() + Ext::from_f64(10.0));
            buy_face = to_quad(Ext::from_f64(100.0) * cast::<Ext, _>(buy_face));
            if cast::<Ext, _>(g.long_gov_bond_face[ins]) > cast::<Ext, _>(g.short_gov_bond_face[ins]) * sf(0.6f32)
                && Ext::from_f64(8.0) > cast::<Ext, _>(g.base_interest_rate)
            {
                trade_gov_bonds(g, ins, buy_face, 2, 1);
            } else {
                trade_gov_bonds(g, ins, buy_face, 1, 1);
            }
            if g.last_ticker_news_kind != 2 && g.news_msg.is_empty() && g.rand_seed % 5 == 4 {
                g.news_msg = g.ent_symbol[ins].clone() + __content.string(10861);
                g.last_ticker_news_kind = 2;
            }
        }
    }
}

/// Port of SubprimeSecsWriteDown (0x508952).
/// Quarterly loss on the subprime securities of `entity_id`, returned (EXT) and charged to its bad-debt
/// reserve (banks < 86) or accrued P&L. In its write-down quarter (`(CurYear + id) MOD 4 + 1`) with weak
/// growth (GDP growth under 1..3) and no crisis: (0.7..1.3) x ((6 - growth) x 0.1 + 0.3) percent (min 0.2
/// below 0.35; x0.8 / x1.2 for margins above 5 / below -5) plus 3 - difficulty minus the crisis state
/// (halved when it is -1), banks also 0.2% of their mortgages. During a crisis 2..9% a quarter; otherwise
/// (odd `CurYear + CurQuarter`) a small amortisation of `CurQuarter` x 0.05%.
pub fn subprime_secs_write_down(g: &mut Globals, rng: &mut Rng, entity_id: i32) -> Ext {
    // ON ERROR -> ShowRuntimeError (error path not ported)
    let i = entity_id;
    let mut loss_amt: Ext = Ext::from_f64(0.0);
    let mut done = false;
    if g.cur_quarter == (g.cur_year + i) % 4 + 1 {
        let x = Ext::from_f64(3.0) - Ext::from_f64(2.0) * rng.rnd();
        if x > cast::<Ext, _>(g.gdp_growth) && g.subprime_crisis_state <= 0 {
            let f = (Ext::from_f64(6.0) - cast::<Ext, _>(g.gdp_growth)) * sf(0.1f32) + sf(0.3f32);
            let r = sf(0.6f32) * rng.rnd();
            loss_amt = f * r + f * sf(0.7f32);
            if loss_amt < sf(0.35f32) {
                loss_amt = sf(0.2f32);
            }
            if g.profit_margin[i] > 5 {
                loss_amt = sf(0.8f32) * loss_amt;
            }
            if g.profit_margin[i] < -5 {
                loss_amt = sf(1.2f32) * loss_amt;
            }
            loss_amt = (Ext::from_f64(3.0) - cast::<Ext, _>(g.difficulty_level)) + loss_amt;
            loss_amt = loss_amt - cast::<Ext, _>(g.subprime_crisis_state);
            let mut write_down = loss_amt * sf(0.01f32) * cast::<Ext, _>(g.subprime_secs[i]);
            if g.subprime_crisis_state == -1 {
                write_down = sf(0.5f32) * write_down;
            }
            g.subprime_secs[i] = to_quad(cast::<Ext, _>(g.subprime_secs[i]) - write_down);
            loss_amt = write_down;
            if i < 86 {
                let m = cast::<Ext, _>(g.bank_mortgage_loans[i]) * sf(0.002f32);
                g.bank_mortgage_loans[i] = to_quad(cast::<Ext, _>(g.bank_mortgage_loans[i]) - m);
                loss_amt = m + loss_amt;
            }
            done = true;
        }
    }
    if !done {
        if g.subprime_crisis_state > 0 {
            let t = (Ext::from_f64(5.0) + cast::<Ext, _>(g.subprime_crisis_state)) - Ext::from_f64(5.0) * rng.rnd();
            loss_amt = Ext::from_f64(15.0) - t;
            if loss_amt < Ext::from_f64(2.0) {
                loss_amt = Ext::from_f64(2.0);
            }
            if loss_amt > sf(9.5f32) {
                loss_amt = Ext::from_f64(9.0);
            }
            let w = loss_amt * sf(0.01f32) * cast::<Ext, _>(g.subprime_secs[i]);
            g.subprime_secs[i] = to_quad(cast::<Ext, _>(g.subprime_secs[i]) - w);
            loss_amt = w;
        } else if cast::<Ext, _>(g.subprime_secs[i]) > Ext::from_f64(0.0) && (g.cur_year + g.cur_quarter) % 2 == 1 {
            let w = cast::<Ext, _>(g.cur_quarter) * sf(0.0005f32) * cast::<Ext, _>(g.subprime_secs[i]);
            g.subprime_secs[i] = to_quad(cast::<Ext, _>(g.subprime_secs[i]) - w);
            loss_amt = w;
        }
    }
    if loss_amt > Ext::from_f64(0.0) {
        if i < 86 {
            g.bad_debt_reserve[i] = g.bad_debt_reserve[i] - loss_amt;
        } else {
            g.accrued_pl[i] = g.accrued_pl[i] - loss_amt;
        }
    }
    loss_amt
}

/// Port of BankSystemLiquidity (0x4e635d).
/// Interbank liquidity for every bank (`bank_id` <= 10) or only `bank_id`: a bank (not 85) takes over the
/// part of the next bank's debt above 2000 (cash moves from the next bank); a bank with cash repays its
/// own debt to its lending bank; for a single bank its assets are reallocated (`BankAllocateAssets(bank,
/// 3, 0)`); after T-bill balancing a cash deficit (plus 0..50) is borrowed from the lending bank up to a
/// debt of 2000 (`BankLiquidityAndCapitalCheck` when still negative). Banks left with debt above 1000
/// count as strained: more than 20 (1 in 7) found a new bank in slot 75 if it is free, else more than 10
/// in Q4 late in the game (`SimCoCount` > 1450, 1 in 3) one in slot 80
/// ([`create_new_bank_in_free_slot`] — note it uses the first free slot 61..85).
pub fn bank_system_liquidity(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng, bank_id: i32) {
    bank_system_liquidity_by(g, ui, rng, &|_| bank_id);
}

/// [`bank_system_liquidity`] called as `BankSystemLiquidity(LenderId)`: the BYREF argument is the global
/// `LenderId`, which `BankAllocateAssets` (through `CalcEntityFinancials`) sets to each processed bank's
/// lending bank, so the run goes on along the chain of lending banks with higher ids.
pub fn bank_system_liquidity_lender(g: &mut Globals, ui: &mut dyn Ui, rng: &mut Rng) {
    bank_system_liquidity_by(g, ui, rng, &|g| g.lender_id);
}

/// The body of BankSystemLiquidity; `bank_id` reads the (BYREF) argument at each use.
fn bank_system_liquidity_by(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    bank_id: &dyn Fn(&Globals) -> i32,
) {
    // procTag = "BankLiq"; ON ERROR -> ShowRuntimeError (error path not ported)
    let mut strained_count = 0;
    let mut bank = 11;
    while bank <= 85 {
        if !(bank_id(g) <= 10 || bank == bank_id(g)) || g.entity_status[bank] != 1 {
            bank += 1;
            continue;
        }
        balance_cash_t_bills(g, bank);
        if bank != 85 && g.bank_debt[bank + 1] > Ext::from_f64(2000.0) {
            let amount = g.bank_debt[bank + 1] - Ext::from_f64(2000.0);
            g.cash[bank + 1] = g.cash[bank + 1] - amount;
            g.bank_debt[bank + 1] = g.bank_debt[bank + 1] - amount;
            g.cash[bank] = amount + g.cash[bank];
        }
        let mut lender = cast::<i32, _>(g.lending_bank[bank]);
        let mut amount: Ext;
        if g.bank_debt[bank] > Ext::from_f64(0.0) && g.cash[bank] > Ext::from_f64(0.0) {
            amount = g.cash[bank];
            if !(g.cash[bank] < g.bank_debt[bank]) {
                amount = g.bank_debt[bank];
            }
            g.cash[bank] = g.cash[bank] - amount;
            g.bank_debt[bank] = g.bank_debt[bank] - amount;
            if 10 < lender {
                g.cash[lender] = amount + g.cash[lender];
            }
        }
        if 10 < bank_id(g) {
            bank_allocate_assets(g, ui, rng, bank, 3, 0);
        }
        balance_cash_t_bills(g, bank);
        if g.cash[bank] < Ext::from_f64(0.0) {
            lender = cast::<i32, _>(g.lending_bank[bank]);
            amount = Ext::from_f64(50.0) * rng.rnd() + -g.cash[bank]; // -1 * Cash (exact)
            if g.bank_debt[bank] < Ext::from_f64(2000.0) {
                let credit_room = Ext::from_f64(2000.0) - g.bank_debt[bank];
                if credit_room < amount {
                    amount = credit_room;
                }
                g.cash[bank] = amount + g.cash[bank];
                g.bank_debt[bank] = amount + g.bank_debt[bank];
                if 10 < lender {
                    g.cash[lender] = g.cash[lender] - amount;
                }
            }
            if g.t_bills[bank] + g.cash[bank] < Ext::from_f64(0.0) && g.bank_debt[bank] >= Ext::from_f64(2000.0) {
                crate::logic::corp_finance::bank::bank_liquidity_and_capital_check(g, ui, rng, bank);
            }
            if g.bank_debt[bank] > Ext::from_f64(1000.0) {
                strained_count += 1;
            }
        }
        bank += 1;
    }
    if strained_count > 20 && g.rand_seed % 7 == 3 {
        if g.entity_status[75] != 1 {
            create_new_bank_in_free_slot(g, ui, rng);
        }
    } else if g.cur_quarter == 4
        && g.start_year + 10 < g.cur_year
        && g.sim_co_count > 1450
        && g.rand_seed % 3 == 1
        && strained_count > 10
        && g.entity_status[80] != 1
    {
        create_new_bank_in_free_slot(g, ui, rng);
    }
}
