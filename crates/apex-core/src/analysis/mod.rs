//! Moteur d'analyse (Rust pur, sans I/O) : testable isolément.

pub mod compare;
pub mod corners;
pub mod insights;
pub mod progress;
pub mod resample;
pub mod session;

pub use compare::{compare_laps, Comparison, LapInput};
