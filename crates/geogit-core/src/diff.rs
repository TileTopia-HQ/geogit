use geogit_encoding::value::ColumnValue;
use std::collections::HashMap;

/// A single change to a feature row.
#[derive(Debug, Clone)]
pub enum FeatureDelta {
    /// A new feature was inserted.
    Insert {
        pk: Vec<ColumnValue>,
        new: HashMap<String, ColumnValue>,
    },
    /// A feature was deleted.
    Delete {
        pk: Vec<ColumnValue>,
        old: HashMap<String, ColumnValue>,
    },
    /// A feature was updated.
    Update {
        pk: Vec<ColumnValue>,
        old: HashMap<String, ColumnValue>,
        new: HashMap<String, ColumnValue>,
        changed_columns: Vec<String>,
    },
}

impl FeatureDelta {
    /// Get the primary key of the affected feature.
    pub fn pk(&self) -> &[ColumnValue] {
        match self {
            FeatureDelta::Insert { pk, .. } => pk,
            FeatureDelta::Delete { pk, .. } => pk,
            FeatureDelta::Update { pk, .. } => pk,
        }
    }

    /// Check if this is an insert.
    pub fn is_insert(&self) -> bool {
        matches!(self, FeatureDelta::Insert { .. })
    }

    /// Check if this is a delete.
    pub fn is_delete(&self) -> bool {
        matches!(self, FeatureDelta::Delete { .. })
    }

    /// Check if this is an update.
    pub fn is_update(&self) -> bool {
        matches!(self, FeatureDelta::Update { .. })
    }
}

/// Result of diffing two dataset versions.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// The dataset path within the repo.
    pub dataset_path: String,
    /// All changes between the two versions.
    pub deltas: Vec<FeatureDelta>,
}

impl DiffResult {
    pub fn inserts(&self) -> impl Iterator<Item = &FeatureDelta> {
        self.deltas.iter().filter(|d| d.is_insert())
    }

    pub fn deletes(&self) -> impl Iterator<Item = &FeatureDelta> {
        self.deltas.iter().filter(|d| d.is_delete())
    }

    pub fn updates(&self) -> impl Iterator<Item = &FeatureDelta> {
        self.deltas.iter().filter(|d| d.is_update())
    }

    /// Summary statistics.
    pub fn summary(&self) -> DiffSummary {
        DiffSummary {
            inserts: self.deltas.iter().filter(|d| d.is_insert()).count(),
            deletes: self.deltas.iter().filter(|d| d.is_delete()).count(),
            updates: self.deltas.iter().filter(|d| d.is_update()).count(),
        }
    }
}

/// Summary of diff statistics.
#[derive(Debug, Clone)]
pub struct DiffSummary {
    pub inserts: usize,
    pub deletes: usize,
    pub updates: usize,
}

impl std::fmt::Display for DiffSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} inserts, {} updates, {} deletes",
            self.inserts, self.updates, self.deletes
        )
    }
}
