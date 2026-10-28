//! ETF management, executive/corporate options, swaps and derivatives housekeeping.
//!
//! Submodules:
//! - [`option_expiry`]: `ExerciseOption`, `ProcessExpiringOptions`, `OptionExpiryReminder`.
//! - [`corp_options`]: executive options (`GrantExecutiveOptions`, `VestExecutiveOptions`), corporate
//!   option trading and forced closes (`AiCorpOptionsTrading`, `ForceInsiderShortCover`,
//!   `ForceCloseDerivativesBadCredit`, `ForceCloseExcessNakedOptions`).
//! - [`swaps_ai`]: `CalcSwapFixedRate`, `AiCreateSwaps`, `SwapRateShockEvent`.
//! - [`etf_mgmt`]: `EtfComplianceCheck`, `EtfAdviserReview`, `CoverNegativeCash`, `AiEtfInvestCash`,
//!   `EtfLiquidateBondsOnDisqualify`, `ReviveDefunctEtfs`.
//! - [`etf_ops`]: `BondEtfManagePortfolio`, `EtfPayPerformanceBonus`, `IndexEtfMaintenance`,
//!   `PromptReduceEtfFee`, `RemindEtfManagerDuties`, `EtfUpdateBonusEligibility`,
//!   `EtfSellIneligibleHolding`.

pub mod corp_options;
pub mod etf_mgmt;
pub mod etf_ops;
pub mod option_expiry;
pub mod swaps_ai;
