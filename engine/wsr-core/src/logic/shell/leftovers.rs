//! Leftovers: `ConvertOldFuturesUnits` (0x63663c, the save-game upgrade of old futures units),
//! `DeliverPhysCommodForShortFutures` (0x664ce0, physical delivery against an expiring short futures
//! contract) and the register of GOSUB blocks, exit stubs and dialog callbacks whose code is part of
//! routines ported elsewhere ([`INLINE_PORTS`]).
use crate::pb::real::{Ext, cast};


use super::prelude::*;

/// Port of ConvertOldFuturesUnits (0x63663c).
/// Save-game upgrade (called by `LoadGame` for files older than version 7.9): shows a quick message,
/// then multiplies the unit count (field 3) of every futures record (`FuturesRecs(1..1000)`) and every
/// physical-commodity record (`PhysCommodRec(1..1000)`) by 10 — silver by 100 — except oil, bitcoin and
/// ethereum. The records are rebuilt with the trimmed commodity name; the count is `ROUND` to a QUAD
/// and written with `STR$` (trimmed). Futures records keep fields 2 and 4..7, physical records fields 2
/// and 4 (later fields are dropped).
pub fn convert_old_futures_units(g: &mut Globals, ui: &mut dyn Ui) {
    let __content = crate::content::current();
    let mut m = s(OLD_FUTURES_UNITS_NOTICE(&__content));
    show_quick_message(g, ui, &mut m);
    convert_old_futures_units_state(g);
}

/// The notice shown before upgrading the units in a save older than version 7.9.
#[allow(non_snake_case)]
pub(crate) fn OLD_FUTURES_UNITS_NOTICE(__content: &crate::content::ContentPack) -> &str { __content.string(10150) }

/// The conversion without presentation; `LoadGame` returns its notices to the host.
pub(crate) fn convert_old_futures_units_state(g: &mut Globals) {
    let new_qty = |rec: &PbStr, commod: &PbStr| -> PbStr {
        let mut q = to_quad(val(parse_any(rec.as_bytes(), b"|", 3).as_bytes()) * Ext::from_f64(10.0));
        if *commod == "SILVER" {
            q = to_quad(cast::<Ext, _>(q) * Ext::from_f64(10.0));
        }
        str_trim(cast::<Ext, _>(q)) + "|"
    };
    let skip = |c: &PbStr| *c == "OIL" || *c == "BITCOIN" || *c == "ETHEREUM";
    for rec_idx in 1..=1000 {
        let rec = g.futures_recs[rec_idx].clone();
        if rec.is_empty() {
            continue;
        }
        let commod = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes());
        if skip(&commod) {
            continue;
        }
        let owner_fld = parse_any(rec.as_bytes(), b"|", 2) + "|";
        let qty_fld = new_qty(&rec, &commod);
        let month_fld = parse_any(rec.as_bytes(), b"|", 4) + "|";
        let year_fld = parse_any(rec.as_bytes(), b"|", 5) + "|";
        let cost_fld = parse_any(rec.as_bytes(), b"|", 6) + "|";
        let price_fld = parse_any(rec.as_bytes(), b"|", 7);
        g.futures_recs[rec_idx] =
            commod + "|" + &owner_fld + &qty_fld + &month_fld + &year_fld + &cost_fld + &price_fld;
    }
    for rec_idx in 1..=1000 {
        let rec = g.phys_commod_rec[rec_idx].clone();
        if rec.is_empty() {
            continue;
        }
        let commod = trim(parse_any(rec.as_bytes(), b"|", 1).as_bytes());
        if skip(&commod) {
            continue;
        }
        let owner_fld = parse_any(rec.as_bytes(), b"|", 2) + "|";
        let qty_fld = new_qty(&rec, &commod);
        let cost_fld = parse_any(rec.as_bytes(), b"|", 4);
        g.phys_commod_rec[rec_idx] = commod + "|" + &owner_fld + &qty_fld + &cost_fld;
    }
}

/// Port of DeliverPhysCommodForShortFutures (0x664ce0).
/// Physical delivery against an expiring short futures contract (`futures_rec` = the futures record
/// text "commodity|holder|contracts|…"; called by `SettleFuturesContract` when the holder makes
/// delivery): if the (active) holder owns at least 0.5 worth of that commodity (`GetPhysCommodValue`
/// with the commodity as filter) and has a physical record with units, it sells `contracts / units`
/// percent of it (100% when it owns no more units than the contracts) as a silent delivery
/// (`SellPhysCommod` mode 2; delivery mode 1, or -1 when it owns fewer units than the contracts) with
/// `CurCommodity` set to the commodity. Returns 1 when it delivered, else 0.
pub fn deliver_phys_commod_for_short_futures(
    g: &mut Globals,
    ui: &mut dyn Ui,
    rng: &mut Rng,
    env: &Env,
    futures_rec: &PbStr,
) -> i32 {
    crate::logic::commod_ai::physical::deliver_phys_commod_for_short_futures_skipping(
        g, ui, rng, futures_rec, |addr| env.skips(addr),
    )
}

/// GOSUB blocks, exit stubs and dialog callbacks of this module's list whose code is part of routines
/// ported elsewhere (address, name, the port that contains it). Exit stubs are the `EXIT SUB` /
/// `RETURN` targets of their routine (a `return` in the port); the slot GOSUBs are inlined.
///
/// * MarginCall_LiquidateLoop (0x632df4): the closing loop of `commodity_margin_call` (sim_entity::mgmt).
/// * MarginCall_LiquidateLoop2 (0x632e2e): the same loop entered after the commodity name was parsed (sim_entity::mgmt).
/// * MarginCall_ForceLiquidate (0x632f15): the forced liquidation branch of `commodity_margin_call` (sim_entity::mgmt).
/// * MarginCall_Exit (0x633084): its exit (sim_entity::mgmt).
/// * ProcessExpiringOptions_ChargeSettlement (0x5f355e): inlined in `process_expiring_options` (etf_derivs::option_expiry).
/// * ExecAssetPurchase_BlendMargin (0x66c5ad): inlined in `execute_business_asset_purchase` (sim_entity::asset_sales).
/// * CapContribDlg_OptBonds (0x5dc5de): option 4 of the capital-contribution `choose` (restructure::capital).
/// * LoanTradeDlg_OptBuyBusiness (0x5b91ae): option 1 of the loan-trade `choose` (corp_finance::loans::buy_sell).
/// * ToggleStreamingQuote_RemoveSlot (0x5de4ea): inlined in `toggle_streaming_quote` (market); its label kills are UI.
/// * UpdateStreamingQuotes_ClearSlot (0x5dcd6c): inlined in `update_streaming_quotes` (reports_lists::alerts).
/// * UpdateStreamingQuotes_KillSlotCtrls (0x5dce1f): `CONTROL KILL` of the slot labels, UI only (reports_lists::alerts).
/// * RebuildStreamingQuotes_ClearSlot (0x5dd942): inlined in `rebuild_streaming_quotes_panel` (market).
/// * RebuildStreamingQuotes_CompactList (0x5dd6f8): inlined in `rebuild_streaming_quotes_panel` (market).
/// * RebuildStreamingQuotes_KillSlotCtrls (0x5dd913): `CONTROL KILL` of the slot labels, UI only (market).
/// * SimEntity_CoverMarginDeficit (0x4d41e2): inlined in SimEntity_WorkingCapitalCash (sim_entity::accounting).
/// * UpdateCorpControl_OnError (0x4214c5): the ON ERROR handler of `update_corp_control` (control): ShowRuntimeError; the port raises no PB runtime errors.
/// * UpdateStreamingQuotes_Exit (0x5dceb3): exit of `update_streaming_quotes` (reports_lists::alerts).
/// * RebuildStreamingQuotesPanel_Exit (0x5dda55): exit of `rebuild_streaming_quotes_panel` (market).
/// * ToggleStreamingQuote_Exit (0x5de62b): exit of `toggle_streaming_quote` (market).
/// * ProjEarn_Exit (0x4989f3): exit of CalcProjectedEarnings (earnings).
/// * StartNewCorp_ExitGosub (0x4f41bf): exit of StartNewCorpOrExpand.
/// * UpdateAntitrustReserves_ExitGosub (0x5472c8): exit of UpdateAntitrustReserves.
/// * FindShortPositions_Exit (0x5dfe18): exit of FindShortPositions (shorts).
/// * ShowOptionPositions_Exit (0x5f7656): exit of ShowOptionPositions.
/// * CalcOptionPrice_Exit (0x605f93): exit of CalcOptionPrice (options).
/// * GetPlrNakedLiab_Exit (0x608a05): exit of GetPlrNakedLiab (options).
/// * LiquidateFutures_Exit (0x62f44d): exit of LiquidateFuturesPositions (futures).
/// * AiCorpHedging_Exit (0x633866): exit of EstimateCorpHedgeGains.
/// * CalcFuturesBasis_Exit (0x6365fa): exit of CalcFuturesBasis (futures).
/// * GetPhysCommodValue_Exit (0x65be60): exit of GetPhysCommodValue (finance).
/// * ListPhysCommod_Exit (0x65db01): exit of ListPhysCommodHoldings.
/// * CalcStorageCost_Exit (0x661ba3): exit of CalcPhysCommodStorageCost.
/// * CoverCashDeficit_Exit (0x66307c): exit of CoverCashDeficit.
/// * LookupSymbol_Exit (0x42314f): exit of LookupSymbol (trading::select).
/// * AiSetDividend_Exit (0x49d5f6): exit of AiSetDividend.
/// * PickEtfHoldingStock_Exit (0x649fac): exit of PickEtfHoldingStock.
/// * CalcCorpBondPrice_Exit (0x42ad41): exit of CalcCorpBondPrice (finance).
/// * ShowBalanceSheet_Exit (0x439826): exit of ShowBalanceSheet (reports_corp).
/// * ShowPersonalFinancialStatement_Exit (0x4400a3): exit of ShowPersonalFinancialStatement (reports_lists::pfs).
/// * ShowPortfolioListing_Exit (0x446440): exit of ShowPortfolioListing (reports_lists::portfolio).
/// * ShowIndustryProjection_Exit (0x46175a): exit of ShowIndustryProjection (reports_lists::projections).
/// * ShowEarningsReport_Exit (0x46bf6a): exit of ShowEarningsReport (reports_corp).
/// * IndustryReport_Exit (0x480a48): exit of ShowIndustryReport (reports_lists::industry).
/// * ShowTopCompaniesList_Exit (0x4a00f2): exit of ShowTopCompaniesList (reports_lists::industry).
/// * Bankruptcy_ExitSub (0x4ed90f): exit of BankruptcyLiquidation (sim_entity::liquidation).
/// * Merger_ExitSub (0x510ac0): exit of MergeCompanies (restructure::merge).
/// * LiquidateSub_ExitSub (0x51b220): exit of LiquidateSubIntoParent (restructure::liquidate_sub).
/// * LiquidateCorp_ExitSub (0x51f1f4): exit of LiquidateCorporation (sim_entity::liquidation).
/// * MiscNews_ExitSub (0x53ec97): exit of ShowMiscNewsStory (economy).
/// * EmptyStub_61ce57 (0x61ce57): an empty SUB without callers but its thunk.
/// * thunk_EmptyStub_61ce57 (0x61ce05): thunk to the empty SUB 0x61ce57.
/// * ConsolidatePhysCommod_Exit (0x665fc3): exit of ConsolidatePhysCommodRecs (reports_corp).
/// * ExecuteBusinessAssetPurchase_Exit (0x66c776): exit of ExecuteBusinessAssetPurchase (sim_entity::asset_sales).
/// * CheckPriceAlerts_Exit (0x6840ff): exit of CheckPriceAlerts (reports_lists::alerts).
/// * RunScenarioEvent_Exit_69af5e (0x69af5e): exit of RunScenarioEvent_689361 (turn::scenario_event).
/// * LoadConfigAndCheckFiles_Exit (0x46c9d2): exit of LoadConfigAndCheckFiles (newgame_helpers).
/// * NewGameSetup_Exit (0x47c2ab): exit of NewGameSetup (newgame).
/// * TechBubbleCycle_Exit (0x4835a6): exit of TechBubbleCycle (economy::crises).
/// * RunScenarioEvent_Exit (0x48fe23): exit of RunScenarioEvent (turn::scenario).
/// * ShowMyHeadlines_Exit (0x49e12b): exit of ShowMyHeadlines (reports_lists::industry).
/// * UpdateInterestRates_Exit (0x4afc78): exit of UpdateInterestRates (economy::macro_update).
/// * PlanMarketCycle_Exit (0x4c1ba1): exit of PlanMarketCycle (economy::cycle).
/// * SimProcessNextEntity_Exit (0x4dfeeb): exit of SimProcessNextEntity (sim_entity).
/// * UpdateEconomy_ExitSub (0x52d761): exit of UpdateEconomy (economy).
/// * Pandemic_Stub (0x53e1b4): exit of PandemicUpdate (economy::crises2).
/// * ContributeCapital_ExitSub (0x59bd58): exit of ContributeCapitalToSub (restructure::capital).
/// * EmptyStub_62cb79 (0x62cb79): empty exit stub of UpdateCommodityPrices (market::commodities).
/// * EmptyStub_649368 (0x649368): empty exit stub of InitETFs (newgame_helpers).
/// * EmptyStub_64a5e4 (0x64a5e4): empty exit stub of UpgradeSaveAddExtraETFs (newgame_helpers).
/// * EmptyStub_64d4e7 (0x64d4e7): empty exit stub of AssignEtfAdviser (reports_corp).
/// * EmptyStub_6358ca (0x6358ca): empty exit stub of RebaseCommodityTargetsFromHistory (economy::commodities).
/// * AiOfferAssets_Exit (0x66bd96): exit of AiOfferAssetsForSale (sim_entity::asset_sales).
/// * TradeTBills_Exit (0x69bb37): exit of TradeTBills (trading::bonds).
/// * InsurerGovBondTrading_Exit (0x4e6342): exit of InsurerGovBondTrading (sim_entity::fin_inst).
/// * ForceCoverShorts_Exit (0x5e0ac7): exit of ForceCoverShorts (sim_entity::mgmt).
/// * AiCorpGrowthAndDividends_Exit (0x6b433c): exit of AiCorpGrowthAndDividends (sim_entity::dividends).
/// * CalcGovBondYields_Exit (0x428bb6): exit of CalcGovBondYields (finance).
pub const INLINE_PORTS: &[u32] = &[
    0x632df4, 0x632e2e, 0x632f15, 0x633084, 0x5f355e, 0x66c5ad, 0x5dc5de, 0x5b91ae, 0x5de4ea, 0x5dcd6c, 0x5dce1f,
    0x5dd942, 0x5dd6f8, 0x5dd913, 0x4d41e2, 0x4214c5, 0x5dceb3, 0x5dda55, 0x5de62b, 0x4989f3, 0x4f41bf, 0x5472c8,
    0x5dfe18, 0x5f7656, 0x605f93, 0x608a05, 0x62f44d, 0x633866, 0x6365fa, 0x65be60, 0x65db01, 0x661ba3, 0x66307c,
    0x42314f, 0x49d5f6, 0x649fac, 0x42ad41, 0x439826, 0x4400a3, 0x446440, 0x46175a, 0x46bf6a, 0x480a48, 0x4a00f2,
    0x4ed90f, 0x510ac0, 0x51b220, 0x51f1f4, 0x53ec97, 0x61ce57, 0x61ce05, 0x665fc3, 0x66c776, 0x6840ff, 0x69af5e,
    0x46c9d2, 0x47c2ab, 0x4835a6, 0x48fe23, 0x49e12b, 0x4afc78, 0x4c1ba1, 0x4dfeeb, 0x52d761, 0x53e1b4, 0x59bd58,
    0x62cb79, 0x649368, 0x64a5e4, 0x64d4e7, 0x6358ca, 0x66bd96, 0x69bb37, 0x4e6342, 0x5e0ac7, 0x6b433c, 0x428bb6,
];
