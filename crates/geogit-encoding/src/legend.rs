use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::value::ColumnValue;

/// A legend maps stored feature values back to column IDs.
///
/// When a feature is written, the legend records which columns existed
/// at the time of writing. When reading the feature later (after schema
/// changes), the legend enables mapping values to the correct columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Legend {
    /// Ordered list of column IDs at the time of writing.
    pub column_ids: Vec<Uuid>,
}

impl Legend {
    /// Create a new legend from column IDs.
    pub fn new(column_ids: Vec<Uuid>) -> Self {
        Self { column_ids }
    }

    /// Compute the SHA-256 hash of the legend (used as filename).
    pub fn hash(&self) -> String {
        let bytes = self.to_msgpack();
        let digest = Sha256::digest(&bytes);
        hex::encode(digest)
    }

    /// Serialize to MessagePack (Kart-compatible).
    pub fn to_msgpack(&self) -> Vec<u8> {
        // Kart stores legends as msgpack arrays of UUID strings
        let ids: Vec<String> = self.column_ids.iter().map(|id| id.to_string()).collect();
        rmp_serde::to_vec(&ids).expect("legend serialization should not fail")
    }

    /// Deserialize from MessagePack.
    pub fn from_msgpack(data: &[u8]) -> Result<Self, LegendError> {
        let ids: Vec<String> =
            rmp_serde::from_slice(data).map_err(|e| LegendError::Decode(e.to_string()))?;
        let column_ids: Result<Vec<Uuid>, _> = ids.iter().map(|s| Uuid::parse_str(s)).collect();
        Ok(Self {
            column_ids: column_ids.map_err(|e| LegendError::Decode(e.to_string()))?,
        })
    }

    /// Decode a stored feature's values using this legend and the current schema.
    ///
    /// Returns a map of column_name -> value for the current schema.
    /// - Columns in the legend but not in the current schema are dropped.
    /// - Columns in the current schema but not in the legend get NULL.
    pub fn decode_values(
        &self,
        values: &[ColumnValue],
        current_schema: &crate::schema::Schema,
    ) -> std::collections::HashMap<String, ColumnValue> {
        use std::collections::HashMap;

        // Build id -> value map from legend + stored values
        let id_values: HashMap<&Uuid, &ColumnValue> =
            self.column_ids.iter().zip(values.iter()).collect();

        // Map to current schema
        let mut row = HashMap::new();
        for col in &current_schema.0 {
            let val = id_values
                .get(&col.id)
                .map(|v| (*v).clone())
                .unwrap_or(ColumnValue::Null);
            row.insert(col.name.clone(), val);
        }
        row
    }

    /// Get column names in legend order, using the current schema for name lookups.
    pub fn column_names(&self, schema: &crate::schema::Schema) -> Vec<String> {
        self.column_ids
            .iter()
            .map(|id| {
                schema
                    .0
                    .iter()
                    .find(|c| c.id == *id)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| id.to_string())
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LegendError {
    #[error("failed to decode legend: {0}")]
    Decode(String),
}

// Hex encoding helper (avoid pulling in another crate)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legend_roundtrip() {
        let ids = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let legend = Legend::new(ids.clone());
        let bytes = legend.to_msgpack();
        let decoded = Legend::from_msgpack(&bytes).unwrap();
        assert_eq!(decoded.column_ids, ids);
    }

    #[test]
    fn test_legend_hash_deterministic() {
        let ids = vec![Uuid::new_v4(), Uuid::new_v4()];
        let legend = Legend::new(ids);
        let h1 = legend.hash();
        let h2 = legend.hash();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex = 64 chars
    }
}
