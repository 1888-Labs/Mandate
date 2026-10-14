//! Mandate HTTP API: create, delegate, revoke and check authority, with
//! shared state on Arkiv. All authorization semantics come from
//! `mandate-core`; this crate only moves documents and decisions.

pub mod adapter;
pub mod api;
pub mod backend;
pub mod config;
pub mod demo;
pub mod routes;
pub mod snapshot;
mod web;

pub use routes::{router, AppState, Clock};
