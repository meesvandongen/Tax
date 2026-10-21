//! # sim-core
//!
//! A deterministic, agent-based city economy for comparing tax regimes.
//! Platform independent: compiled natively for headless experiments and to
//! WebAssembly for the interactive browser front end.
//!
//! Layout:
//! * [`world`] — all state; [`types`] — parcels, households, firms
//! * [`sim`] — the ordered monthly pipeline of systems
//! * [`tax`] — pluggable tax modules; [`presets`] — named policies
//! * [`econ`] — valuation / development feasibility used by every agent
//! * markets: [`labour`], [`housing`], [`land`], [`production`]
//! * [`government`], [`migration`], [`environment`], [`metrics`], [`snapshot`]

#![allow(clippy::needless_range_loop, clippy::unnecessary_map_or, clippy::manual_is_multiple_of)]

pub mod clock;
pub mod config;
pub mod development;
pub mod econ;
pub mod environment;
pub mod fields;
pub mod firms;
pub mod genesis;
pub mod government;
pub mod grid;
pub mod households;
pub mod housing;
pub mod inspect;
pub mod labour;
pub mod land;
pub mod metrics;
pub mod migration;
pub mod presets;
pub mod production;
pub mod property;
pub mod rng;
pub mod sim;
pub mod snapshot;
pub mod tax;
pub mod terrain;
pub mod types;
pub mod world;

pub use config::{EconParams, Policy, SimConfig, SpendingPolicy};
pub use metrics::Metrics;
pub use sim::Simulation;
