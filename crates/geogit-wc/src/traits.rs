use anyhow::Result;
use geogit_core::dataset::DatasetMeta;
use geogit_core::diff::FeatureDelta;
use geogit_encoding::value::ColumnValue;
use std::collections::HashMap;

/// Trait for working copy implementations.
///
/// A working copy is a database (GeoPackage, PostGIS, etc.) where
/// GIS software can directly edit features. Changes are tracked
/// and can be committed back to the repository.
pub trait WorkingCopy {
    /// Write dataset features to the working copy.
    fn checkout(
        &mut self,
        dataset_path: &str,
        meta: &DatasetMeta,
        features: &[(Vec<ColumnValue>, HashMap<String, ColumnValue>)],
    ) -> Result<()>;

    /// Detect changes made in the working copy since last checkout.
    fn status(&self, dataset_path: &str) -> Result<Vec<FeatureDelta>>;

    /// Reset working copy to match the given features (discard changes).
    fn reset(
        &mut self,
        dataset_path: &str,
        meta: &DatasetMeta,
        features: &[(Vec<ColumnValue>, HashMap<String, ColumnValue>)],
    ) -> Result<()>;

    /// List all datasets in the working copy.
    fn list_datasets(&self) -> Result<Vec<String>>;
}
