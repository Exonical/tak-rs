//! Offline-first situational-awareness state.
//!
//! [`Store`] holds the current picture — contacts, map objects and chat — and
//! is fed [`TakEvent`]s from any source (network sessions, local sensors,
//! persistence). It knows nothing about XML, protobuf, sockets or UI; every
//! mutation returns the [`StoreChange`]s it caused so callers (UI, agents,
//! persistence) can react incrementally.
//!
//! Staleness is explicit: nothing is dropped until [`Store::sweep`] runs with
//! a clock the caller controls, which keeps behaviour deterministic and
//! testable and lets headless agents choose their own retention.

pub mod store;

pub use store::{RemovalReason, Store, StoreChange, StoreConfig, StoreStats};
pub use tak_core::TakEvent;
