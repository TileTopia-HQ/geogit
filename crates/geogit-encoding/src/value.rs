use serde::{Deserialize, Serialize};

/// Column values stored in MessagePack features.
/// Follows Kart's serialization conventions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ColumnValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl ColumnValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ColumnValue::Integer(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            ColumnValue::Text(v) => Some(v.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ColumnValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ColumnValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            ColumnValue::Blob(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, ColumnValue::Null)
    }
}

impl std::fmt::Display for ColumnValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnValue::Null => write!(f, "NULL"),
            ColumnValue::Bool(v) => write!(f, "{v}"),
            ColumnValue::Integer(v) => write!(f, "{v}"),
            ColumnValue::Float(v) => write!(f, "{v}"),
            ColumnValue::Text(v) => write!(f, "{v}"),
            ColumnValue::Blob(v) => write!(f, "<{} bytes>", v.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_value_accessors() {
        assert_eq!(ColumnValue::Integer(42).as_i64(), Some(42));
        assert_eq!(ColumnValue::Text("hello".into()).as_str(), Some("hello"));
        assert_eq!(ColumnValue::Float(2.72).as_f64(), Some(2.72));
        assert_eq!(ColumnValue::Bool(true).as_bool(), Some(true));
        assert!(ColumnValue::Null.is_null());
    }
}
