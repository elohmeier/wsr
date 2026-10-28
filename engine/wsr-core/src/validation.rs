//! Guard stable browser load/save boundaries. Compatibility routines remain unchanged.
use crate::{
    pb::{array::Arr1, numfmt::val, pbstr::PbStr, string::parse_any},
    state::Globals,
};

fn range(name: &str, value: i32, lo: i32, hi: i32) -> Result<(), String> {
    if (lo..=hi).contains(&value) {
        Ok(())
    } else {
        Err(format!("{name} is {value}; expected {lo} to {hi}"))
    }
}

fn records(name: &str, values: &Arr1<PbStr>) -> Result<(), String> {
    let count = values[0].to_display();
    let count = count.trim();
    let count = if count.is_empty() {
        0
    } else {
        count
            .parse::<i32>()
            .map_err(|_| format!("{name}(0) is not an integer record count"))?
    };
    range(&format!("{name}(0)"), count, 0, values.ubound())
}

/// Validate finite storage and the references the loader/saver and immediate post-load routines use.
/// Negative cash, losses, short positions and missing optional advisers are valid game states.
pub fn validate_game(g: &Globals) -> Result<(), String> {
    if let Some(field) = g.first_nonfinite() {
        return Err(format!("{field} is not finite"));
    }
    range("NumPlayers", g.num_players, 1, 5)?;
    range("CurPlayer", g.cur_player, 1, g.num_players)?;
    range("ActiveEntity", g.active_entity, 1, 1600)?;
    range("SelEntity", g.sel_entity, 1, 1600)?;
    range("CurQuarter", g.cur_quarter, 1, 4)?;
    range("SimCoCount", g.sim_co_count, 0, 1600)?;
    range("StartYear", g.start_year, 1, 9999)?;
    range("CurYear", g.cur_year, g.start_year, 9999)?;
    range("GameEndYear", g.game_end_year, g.start_year, 9999)?;
    if g.exchange_rate <= 0.0 {
        return Err("ExchangeRate must be positive".into());
    }
    for player in 1..=g.num_players {
        range(
            &format!("PlayerTurnOrder({player})"),
            g.player_turn_order[player],
            1,
            g.num_players,
        )?;
    }
    for owner in 1..=1600 {
        range(
            &format!("EntityStatus({owner})"),
            g.entity_status[owner] as i32,
            0,
            2,
        )?;
        range(
            &format!("CorpController({owner})"),
            g.corp_controller[owner],
            0,
            1600,
        )?;
        let bank = g.lending_bank[owner] as i32;
        // The first bank borrows from slot 10: both NewGame_ParseBankCash and CreateBank
        // assign the preceding bank's slot. Other borrowers use actual bank slots.
        if bank != 0 {
            range(
                &format!("LendingBank({owner})"),
                bank,
                if owner == 11 { 10 } else { 11 },
                85,
            )?;
        }
        if owner >= 11
            && g.bonds_issued[owner] > 0
            && g.bond_issue_info[owner]
                .as_bytes()
                .windows(2)
                .any(|w| w == b"cv")
        {
            let price = val(parse_any(g.bond_issue_info[owner].as_bytes(), b"|", 2).as_bytes());
            if !price.is_finite() || price <= 0.0 {
                return Err(format!(
                    "BondIssueInfo({owner}) needs a finite, positive conversion value"
                ));
            }
        }
        if g.entity_status[owner] == 1 && owner >= 11 {
            range(
                &format!("IndustryNum({owner})"),
                g.industry_num[owner] as i32,
                1,
                71,
            )?;
            if g.shares_outstanding[owner] <= 0.0 {
                return Err(format!("SharesOutstanding({owner}) must be positive"));
            }
            if g.market_cap[owner] < 0.0 {
                return Err(format!("MarketCap({owner}) must not be negative"));
            }
        }
        range(
            &format!("HoldStockId(0, {owner})"),
            g.hold_stock_id[(0, owner)] as i32,
            0,
            15,
        )?;
        range(
            &format!("HoldBondId(0, {owner})"),
            g.hold_bond_id[(0, owner)],
            0,
            15,
        )?;
        for slot in 1..=15 {
            let stock = g.hold_stock_id[(slot, owner)] as i32;
            let bond = g.hold_bond_id[(slot, owner)];
            if stock != 0 {
                range(&format!("HoldStockId({slot}, {owner})"), stock, 11, 1600)?;
            }
            if bond != 0 {
                range(&format!("HoldBondId({slot}, {owner})"), bond, 11, 1600)?;
            }
        }
    }
    for (name, values) in [
        ("OptionRecs", &g.option_recs),
        ("FuturesRecs", &g.futures_recs),
        ("SwapContracts", &g.swap_contracts),
        ("PhysCommodRec", &g.phys_commod_rec),
        ("AssetSaleOffers", &g.asset_sale_offers),
    ] {
        records(name, values)?;
    }
    Ok(())
}
