//! `kiln-sim`: the simulation lab.
//!
//! M1 provided deck import and validation ([`deck`]). M2 adds the batch
//! runner skeleton ([`runner`]). M7 adds experiments ([`experiment`]): specs, metrics and reports.

pub mod deck;
pub mod experiment;
pub mod library;
pub mod live;
pub mod runner;

pub mod vanilla;
