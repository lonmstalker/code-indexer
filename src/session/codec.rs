//! Dictionary Encoder/Decoder
//!
//! Compresses repeated strings (file paths, symbol kinds, modules) to short IDs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Dictionary delta - new entries added in a response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DictDelta {
    /// File path mappings (short_id -> full_path)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub files: HashMap<u32, String>,
    /// Symbol kind mappings (short_id -> kind_name)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub kinds: HashMap<u8, String>,
    /// Module mappings (short_id -> module_path)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub modules: HashMap<u16, String>,
}

impl DictDelta {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.kinds.is_empty() && self.modules.is_empty()
    }
}

/// Encoder that maps strings to short IDs
#[derive(Debug, Clone, Default)]
pub struct DictEncoder {
    files: HashMap<String, u32>,
    kinds: HashMap<String, u8>,
    modules: HashMap<String, u16>,
    next_file_id: u32,
    next_kind_id: u8,
    next_module_id: u16,
}

impl DictEncoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from existing session dictionary
    pub fn from_session(
        files: HashMap<String, u32>,
        kinds: HashMap<String, u8>,
        modules: HashMap<String, u16>,
    ) -> Self {
        let next_file_id = files.values().copied().max().unwrap_or(0) + 1;
        let next_kind_id = kinds.values().copied().max().unwrap_or(0).saturating_add(1);
        let next_module_id = modules.values().copied().max().unwrap_or(0) + 1;

        Self {
            files,
            kinds,
            modules,
            next_file_id,
            next_kind_id,
            next_module_id,
        }
    }

    /// Encode a file path, returning the short ID
    /// Also tracks if this is a new entry
    pub fn encode_file(&mut self, path: &str) -> (u32, bool) {
        if let Some(&id) = self.files.get(path) {
            (id, false)
        } else {
            let id = self.next_file_id;
            self.files.insert(path.to_string(), id);
            self.next_file_id += 1;
            (id, true)
        }
    }

    /// Encode a symbol kind
    pub fn encode_kind(&mut self, kind: &str) -> (u8, bool) {
        if let Some(&id) = self.kinds.get(kind) {
            (id, false)
        } else {
            let id = self.next_kind_id;
            self.kinds.insert(kind.to_string(), id);
            self.next_kind_id = self.next_kind_id.saturating_add(1);
            (id, true)
        }
    }

    /// Encode a module path
    pub fn encode_module(&mut self, module: &str) -> (u16, bool) {
        if let Some(&id) = self.modules.get(module) {
            (id, false)
        } else {
            let id = self.next_module_id;
            self.modules.insert(module.to_string(), id);
            self.next_module_id += 1;
            (id, true)
        }
    }

    /// Get all dictionaries (for session persistence)
    pub fn get_dictionaries(&self) -> (HashMap<String, u32>, HashMap<String, u8>, HashMap<String, u16>) {
        (self.files.clone(), self.kinds.clone(), self.modules.clone())
    }

    /// Get the inverted dictionaries for the DictDelta
    pub fn get_delta(&self) -> DictDelta {
        DictDelta {
            files: self.files.iter().map(|(k, v)| (*v, k.clone())).collect(),
            kinds: self.kinds.iter().map(|(k, v)| (*v, k.clone())).collect(),
            modules: self.modules.iter().map(|(k, v)| (*v, k.clone())).collect(),
        }
    }
}

/// Decoder that maps short IDs back to strings
#[derive(Debug, Clone, Default)]
pub struct DictDecoder {
    files: HashMap<u32, String>,
    kinds: HashMap<u8, String>,
    modules: HashMap<u16, String>,
}

impl DictDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from a DictDelta
    pub fn from_delta(delta: &DictDelta) -> Self {
        Self {
            files: delta.files.clone(),
            kinds: delta.kinds.clone(),
            modules: delta.modules.clone(),
        }
    }

    /// Merge a new delta into this decoder
    pub fn merge(&mut self, delta: &DictDelta) {
        self.files.extend(delta.files.iter().map(|(k, v)| (*k, v.clone())));
        self.kinds.extend(delta.kinds.iter().map(|(k, v)| (*k, v.clone())));
        self.modules.extend(delta.modules.iter().map(|(k, v)| (*k, v.clone())));
    }

    /// Decode a file ID
    pub fn decode_file(&self, id: u32) -> Option<&str> {
        self.files.get(&id).map(|s| s.as_str())
    }

    /// Decode a kind ID
    pub fn decode_kind(&self, id: u8) -> Option<&str> {
        self.kinds.get(&id).map(|s| s.as_str())
    }

    /// Decode a module ID
    pub fn decode_module(&self, id: u16) -> Option<&str> {
        self.modules.get(&id).map(|s| s.as_str())
    }
}

/// A compact symbol representation using dictionary IDs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactEncodedSymbol {
    /// Symbol name
    pub n: String,
    /// Symbol kind (encoded)
    pub k: u8,
    /// File path (encoded)
    pub f: u32,
    /// Line number
    pub l: u32,
    /// Signature (optional, not encoded as it varies too much)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sig: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoder_new_entries() {
        let mut encoder = DictEncoder::new();

        let (id1, is_new1) = encoder.encode_file("src/main.rs");
        assert!(is_new1);
        assert_eq!(id1, 0);

        let (id2, is_new2) = encoder.encode_file("src/lib.rs");
        assert!(is_new2);
        assert_eq!(id2, 1);

        let (id3, is_new3) = encoder.encode_file("src/main.rs");
        assert!(!is_new3);
        assert_eq!(id3, 0);
    }

    #[test]
    fn test_encoder_decoder_roundtrip() {
        let mut encoder = DictEncoder::new();

        encoder.encode_file("src/main.rs");
        encoder.encode_file("src/lib.rs");
        encoder.encode_kind("function");
        encoder.encode_kind("struct");

        let delta = encoder.get_delta();
        let decoder = DictDecoder::from_delta(&delta);

        assert_eq!(decoder.decode_file(0), Some("src/main.rs"));
        assert_eq!(decoder.decode_file(1), Some("src/lib.rs"));
        assert_eq!(decoder.decode_kind(0), Some("function"));
        assert_eq!(decoder.decode_kind(1), Some("struct"));
    }

    #[test]
    fn test_delta_merge() {
        let mut encoder = DictEncoder::new();
        encoder.encode_file("file1.rs");
        let delta1 = encoder.get_delta();

        encoder.encode_file("file2.rs");
        let delta2 = encoder.get_delta();

        let mut decoder = DictDecoder::from_delta(&delta1);
        decoder.merge(&delta2);

        assert_eq!(decoder.decode_file(0), Some("file1.rs"));
        assert_eq!(decoder.decode_file(1), Some("file2.rs"));
    }
}
