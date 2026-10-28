//! Information screens: database search, ownership diagrams, holder lists, price charts, insider tips and the cheat menu.
//!
//! Submodules by screen: [`db_search`] (ShowDbSearchDialog / RunDbSearch and the search dialog
//! callbacks), [`ownership`] (ShowOwnershipDiagram and its box callbacks), [`holders`] (the "who owns
//! what" holder lists, the streaming-quotes fill and the report rating click), [`chart`] (ShowPriceChart,
//! DrawPriceChart, SynthesizePriceHistory, ChartAxisMaxForValue), [`insider`] (InsiderEarnTipEvent and
//! the cheat insider earnings tip), [`cheat`] (CheatMenu, its dialog, the merger tip and the antitrust cheat).

pub mod chart;
pub mod cheat;
pub mod db_search;
pub mod holders;
pub mod insider;
pub mod ownership;
