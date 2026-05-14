use crate::value::ColumnValue;

/// A stored feature: legend hash + column values (excluding PK, which is in the path).
///
/// Serialized as MessagePack: [legend_hash, [val1, val2, ...]]
#[derive(Debug, Clone)]
pub struct StoredFeature {
    /// SHA-256 hash of the legend used when this feature was written.
    pub legend_hash: String,
    /// Column values in legend order (excludes primary key columns).
    pub values: Vec<ColumnValue>,
}

impl StoredFeature {
    /// Encode to MessagePack bytes (Kart-compatible format).
    pub fn to_msgpack(&self) -> Vec<u8> {
        // Kart format: [legend_hash, [val1, val2, ...]]
        let tuple = (&self.legend_hash, &self.values);
        rmp_serde::to_vec(&tuple).expect("feature serialization should not fail")
    }

    /// Decode from MessagePack bytes.
    pub fn from_msgpack(data: &[u8]) -> Result<Self, FeatureError> {
        let (legend_hash, values): (String, Vec<ColumnValue>) =
            rmp_serde::from_slice(data).map_err(|e| FeatureError::Decode(e.to_string()))?;
        Ok(Self {
            legend_hash,
            values,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FeatureError {
    #[error("failed to decode feature: {0}")]
    Decode(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_roundtrip() {
        let feature = StoredFeature {
            legend_hash: "abc123".into(),
            values: vec![
                ColumnValue::Integer(42),
                ColumnValue::Text("hello".into()),
                ColumnValue::Float(2.72),
                ColumnValue::Bool(true),
                ColumnValue::Null,
            ],
        };

        let bytes = feature.to_msgpack();
        let decoded = StoredFeature::from_msgpack(&bytes).unwrap();
        assert_eq!(decoded.legend_hash, "abc123");
        assert_eq!(decoded.values.len(), 5);
        assert_eq!(decoded.values[0], ColumnValue::Integer(42));
        assert_eq!(decoded.values[1], ColumnValue::Text("hello".into()));
        assert_eq!(decoded.values[4], ColumnValue::Null);
    }
}
