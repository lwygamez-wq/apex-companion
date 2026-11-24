//! Moteur d'analyse (Rust pur, sans I/O) : testable isolément.

pub mod attack;
pub mod braking;
pub mod compare;
pub mod corners;
pub mod debrief;
pub mod goals;
pub mod insights;
pub mod offtrack;
pub mod phases;
pub mod plan;
pub mod progress;
pub mod race;
pub mod race_options;
pub mod references;
pub mod regularity;
pub mod resample;
pub mod stint_drift;
pub mod risk;
pub mod session;
pub mod setup_diag;
pub mod setup_memory;
pub mod setup_trial;
pub mod strategy;
pub mod surface;

pub use compare::{compare_laps, Comparison, LapInput};
