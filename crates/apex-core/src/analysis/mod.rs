//! Moteur d'analyse (Rust pur, sans I/O) : testable isolément.

pub mod compare;
pub mod corners;
pub mod goals;
pub mod insights;
pub mod offtrack;
pub mod plan;
pub mod progress;
pub mod race;
pub mod resample;
pub mod risk;
pub mod session;
pub mod surface;

pub use compare::{compare_laps, Comparison, LapInput};
