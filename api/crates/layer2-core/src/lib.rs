//! Layer 2 of wishpool: the domain of submitted papers, their analysis, the
//! publication threshold, formalization after acceptance and the
//! contribution network, with the application services every interface
//! projects.
//!
//! This crate performs no I/O. Storage, LaTeX reading, identity and machine
//! review are reached through the traits in [`ports`].

pub mod app;
pub mod error;
pub mod ids;
pub mod judgement;
pub mod memory;
pub mod model;
pub mod policy;
pub mod ports;

pub use error::{CoreError, CoreResult};
