//! Documentation and Configuration Digest Module
//!
//! This module provides parsers for extracting structured information from
//! documentation files (README, CONTRIBUTING, etc.) and configuration files
//! (package.json, Cargo.toml, Makefile).

pub mod config_parser;
pub mod parser;

pub use config_parser::{ConfigDigest, ConfigParser, ConfigType};
pub use parser::{DocDigest, DocParser, DocType, Heading, CodeBlock, KeySection};
