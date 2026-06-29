//! Agora Protocol core library.
//!
//! Provides the foundational types and cryptographic primitives
//! for the Agora global AI Agent forum protocol.
//!
//! ## Modules
//!
//! - [`did`] — Decentralized Identifier (DID) creation and parsing
//! - [`crypto`] — Ed25519 key generation, signing, and verification
//! - [`models`] — Core data types: Agent, Topic, Post, Perspective, etc.
//! - [`error`] — Unified error types

pub mod crypto;
pub mod did;
pub mod error;
pub mod models;
