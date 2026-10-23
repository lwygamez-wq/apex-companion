//! # apex-core
//!
//! Cœur d'Apex Companion, indépendant de l'interface :
//! - [`model`] : modèle de données unifié multi-simulateurs
//! - [`connectors`] : un connecteur par simulateur, capacités déclarées honnêtement
//! - [`ingest`] : parseurs de fichiers de télémétrie (iRacing `.ibt`)
//! - [`analysis`] : alignement par distance, delta, virages, conseils
//! - [`storage`] : persistance SQLite locale (WAL, transactions, anti-doublon)
//! - [`detect`] : détection des processus et machine à états de session
//! - [`mock`] : données de démonstration déterministes

pub mod analysis;
pub mod codec;
pub mod connectors;
pub mod detect;
pub mod garage61_api;
pub mod ingest;
pub mod live_coach;
pub mod mock;
pub mod service;
pub mod model;
pub mod storage;
pub mod tracks;
pub mod tts;
