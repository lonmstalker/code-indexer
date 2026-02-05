//! Session Dictionary Codec Module
//!
//! Provides token optimization through session-scoped dictionaries
//! that map long strings to short numeric IDs.

pub mod codec;
pub mod manager;

pub use codec::{DictEncoder, DictDecoder, DictDelta};
pub use manager::{SessionManager, Session};
