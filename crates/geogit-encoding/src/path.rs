use base64::{Engine, engine::general_purpose::URL_SAFE};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::value::ColumnValue;

/// Path structure configuration (stored in meta/path-structure.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathStructure {
    pub scheme: PathScheme,
    pub branches: u32,
    pub levels: u32,
    pub encoding: PathEncoding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathScheme {
    Int,
    #[serde(rename = "msgpack/hash")]
    MsgpackHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PathEncoding {
    Base64,
    Hex,
}

impl Default for PathStructure {
    fn default() -> Self {
        Self {
            scheme: PathScheme::Int,
            branches: 64,
            levels: 4,
            encoding: PathEncoding::Base64,
        }
    }
}

/// URL-safe Base64 alphabet (A-Z, a-z, 0-9, -, _)
const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

impl PathStructure {
    /// Generate the full path for a feature given its primary key values.
    ///
    /// Returns e.g. "A/A/A/B/kU0=" for PK `[77]` with default int scheme.
    pub fn feature_path(&self, pk_values: &[ColumnValue]) -> String {
        let filename = self.pk_filename(pk_values);
        let dir = self.pk_directory(pk_values);
        format!("{dir}/{filename}")
    }

    /// Generate the filename component from PK values.
    /// This is urlsafe_b64(msgpack(pk_values)).
    pub fn pk_filename(&self, pk_values: &[ColumnValue]) -> String {
        let bytes = rmp_serde::to_vec(pk_values).expect("PK serialization should not fail");
        URL_SAFE.encode(&bytes)
    }

    /// Generate the directory path component from PK values.
    fn pk_directory(&self, pk_values: &[ColumnValue]) -> String {
        match self.scheme {
            PathScheme::Int => self.int_directory(pk_values),
            PathScheme::MsgpackHash => self.hash_directory(pk_values),
        }
    }

    /// Int scheme: use the integer PK value directly to generate the path.
    fn int_directory(&self, pk_values: &[ColumnValue]) -> String {
        let pk_int = pk_values[0]
            .as_i64()
            .expect("int scheme requires integer PK") as u64;

        match self.encoding {
            PathEncoding::Base64 => {
                // Convert integer to base-64 digits, pad to (levels + 1), drop last
                let mut digits = Vec::new();
                let mut val = pk_int;
                if val == 0 {
                    digits.push(0u8);
                } else {
                    while val > 0 {
                        digits.push((val % 64) as u8);
                        val /= 64;
                    }
                }
                digits.reverse();

                // Pad left to (levels + 1) characters
                let total = (self.levels + 1) as usize;
                while digits.len() < total {
                    digits.insert(0, 0);
                }

                // Drop the last character, take the last `levels` as path components
                digits.pop();
                let start = digits.len().saturating_sub(self.levels as usize);
                let path_digits = &digits[start..];

                path_digits
                    .iter()
                    .map(|&d| (B64_ALPHABET[d as usize] as char).to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            }
            PathEncoding::Hex => {
                let hex = format!("{pk_int:0width$x}", width = (self.levels + 1) as usize * 2);
                let chars: Vec<char> = hex.chars().collect();
                let total = self.levels as usize;
                let start = chars.len().saturating_sub(total + 1);
                (0..total)
                    .map(|i| chars[start + i].to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            }
        }
    }

    /// Hash scheme: SHA-256 of msgpack(pk_values), then encode for directory.
    fn hash_directory(&self, pk_values: &[ColumnValue]) -> String {
        let pk_bytes = rmp_serde::to_vec(pk_values).expect("PK serialization should not fail");
        let hash = Sha256::digest(&pk_bytes);

        match self.encoding {
            PathEncoding::Base64 => {
                // Take enough bits for `levels` base64 characters (6 bits each)
                let bits_needed = self.levels * 6;
                let bytes_needed = bits_needed.div_ceil(8) as usize;
                let hash_bytes = &hash[..bytes_needed.min(hash.len())];
                let b64 = URL_SAFE.encode(hash_bytes);
                let chars: Vec<char> = b64.chars().collect();
                (0..self.levels as usize)
                    .map(|i| chars.get(i).unwrap_or(&'A').to_string())
                    .collect::<Vec<_>>()
                    .join("/")
            }
            PathEncoding::Hex => {
                let hex_str = hex_encode(&hash);
                let chars: Vec<char> = hex_str.chars().collect();
                let chunk_size = match self.branches {
                    256 => 2,
                    16 => 1,
                    _ => 2,
                };
                (0..self.levels as usize)
                    .map(|i| {
                        let start = i * chunk_size;
                        chars[start..start + chunk_size].iter().collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("/")
            }
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_scheme_pk_77() {
        let ps = PathStructure::default(); // int, base64, 64 branches, 4 levels
        let pk = vec![ColumnValue::Integer(77)];
        let dir = ps.pk_directory(&pk);
        assert_eq!(dir, "A/A/A/B");
    }

    #[test]
    fn test_int_scheme_pk_large() {
        let ps = PathStructure::default();
        let pk = vec![ColumnValue::Integer(1_234_567_890)];
        let dir = ps.pk_directory(&pk);
        // 1234567890 in base64: BJlgLS -> drop last -> BJlgL -> last 4 = JlgL -> J/l/g/L
        assert_eq!(dir, "J/l/g/L");
    }

    #[test]
    fn test_pk_filename() {
        let ps = PathStructure::default();
        let pk = vec![ColumnValue::Integer(77)];
        let filename = ps.pk_filename(&pk);
        // [77] -> msgpack -> base64
        assert!(!filename.is_empty());
    }

    #[test]
    fn test_feature_path() {
        let ps = PathStructure::default();
        let pk = vec![ColumnValue::Integer(77)];
        let path = ps.feature_path(&pk);
        assert!(path.starts_with("A/A/A/B/"));
    }

    #[test]
    fn test_hash_scheme() {
        let ps = PathStructure {
            scheme: PathScheme::MsgpackHash,
            branches: 256,
            levels: 2,
            encoding: PathEncoding::Hex,
        };
        let pk = vec![ColumnValue::Integer(77)];
        let path = ps.feature_path(&pk);
        // Should have 2 directory levels + filename
        let parts: Vec<&str> = path.split('/').collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_path_structure_serde() {
        let ps = PathStructure::default();
        let json = serde_json::to_string(&ps).unwrap();
        let parsed: PathStructure = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.levels, 4);
        assert_eq!(parsed.branches, 64);
    }
}
