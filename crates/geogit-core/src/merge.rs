use geogit_encoding::value::ColumnValue;
use std::collections::HashMap;

/// Result of a three-way merge between branches.
#[derive(Debug)]
pub struct MergeResult {
    /// Automatically resolved changes.
    pub resolved: Vec<ResolvedDelta>,
    /// Conflicts requiring manual resolution.
    pub conflicts: Vec<MergeConflict>,
}

impl MergeResult {
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

/// An automatically resolved merge delta.
#[derive(Debug, Clone)]
pub struct ResolvedDelta {
    pub pk: Vec<ColumnValue>,
    pub resolution: Resolution,
    pub values: HashMap<String, ColumnValue>,
}

/// How a merge delta was resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resolution {
    /// Only one side changed — take that side.
    FastForward,
    /// Both sides made identical changes.
    IdenticalChanges,
    /// Both sides changed different columns — merge column by column.
    ColumnMerge,
}

/// A merge conflict that requires manual resolution.
#[derive(Debug, Clone)]
pub struct MergeConflict {
    pub pk: Vec<ColumnValue>,
    pub ancestor: Option<HashMap<String, ColumnValue>>,
    pub ours: Option<HashMap<String, ColumnValue>>,
    pub theirs: Option<HashMap<String, ColumnValue>>,
    pub conflict_type: ConflictType,
}

/// Types of merge conflicts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictType {
    /// Same feature modified differently on both sides.
    BothModified,
    /// One side modified, the other deleted.
    ModifyDelete,
    /// Same PK inserted with different values on both sides.
    BothAdded,
}

/// Strategy for resolving conflicts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictStrategy {
    /// Accept our version.
    Ours,
    /// Accept their version.
    Theirs,
    /// Abort if conflicts exist.
    Abort,
}

/// Perform a three-way merge of feature deltas.
///
/// Given diffs from ancestor→ours and ancestor→theirs, produces
/// merged results and any unresolvable conflicts.
pub fn merge_deltas(
    ours_deltas: &[(Vec<ColumnValue>, DeltaKind)],
    theirs_deltas: &[(Vec<ColumnValue>, DeltaKind)],
) -> MergeResult {
    use std::collections::BTreeMap;

    // Index deltas by serialized PK
    let mut ours_map: BTreeMap<String, &DeltaKind> = BTreeMap::new();
    for (pk, delta) in ours_deltas {
        let key = pk_key(pk);
        ours_map.insert(key, delta);
    }

    let mut theirs_map: BTreeMap<String, &DeltaKind> = BTreeMap::new();
    for (pk, delta) in theirs_deltas {
        let key = pk_key(pk);
        theirs_map.insert(key, delta);
    }

    let mut resolved = Vec::new();
    let mut conflicts = Vec::new();

    // All PKs changed in either branch
    let all_keys: std::collections::BTreeSet<&String> =
        ours_map.keys().chain(theirs_map.keys()).collect();

    for key in all_keys {
        let ours = ours_map.get(key);
        let theirs = theirs_map.get(key);

        match (ours, theirs) {
            // Only one side changed — fast-forward
            (Some(delta), None) => {
                if let Some(vals) = delta.result_values() {
                    resolved.push(ResolvedDelta {
                        pk: delta.pk().to_vec(),
                        resolution: Resolution::FastForward,
                        values: vals,
                    });
                }
            }
            (None, Some(delta)) => {
                if let Some(vals) = delta.result_values() {
                    resolved.push(ResolvedDelta {
                        pk: delta.pk().to_vec(),
                        resolution: Resolution::FastForward,
                        values: vals,
                    });
                }
            }
            // Both sides changed the same feature
            (Some(ours_delta), Some(theirs_delta)) => {
                // TODO: column-level merge for compatible changes
                conflicts.push(MergeConflict {
                    pk: ours_delta.pk().to_vec(),
                    ancestor: ours_delta.old_values(),
                    ours: ours_delta.result_values(),
                    theirs: theirs_delta.result_values(),
                    conflict_type: classify_conflict(ours_delta, theirs_delta),
                });
            }
            (None, None) => unreachable!(),
        }
    }

    MergeResult {
        resolved,
        conflicts,
    }
}

/// A categorized delta for merge processing.
#[derive(Debug, Clone)]
pub enum DeltaKind {
    Insert {
        pk: Vec<ColumnValue>,
        values: HashMap<String, ColumnValue>,
    },
    Delete {
        pk: Vec<ColumnValue>,
        old_values: HashMap<String, ColumnValue>,
    },
    Update {
        pk: Vec<ColumnValue>,
        old_values: HashMap<String, ColumnValue>,
        new_values: HashMap<String, ColumnValue>,
    },
}

impl DeltaKind {
    fn pk(&self) -> &[ColumnValue] {
        match self {
            DeltaKind::Insert { pk, .. } => pk,
            DeltaKind::Delete { pk, .. } => pk,
            DeltaKind::Update { pk, .. } => pk,
        }
    }

    fn result_values(&self) -> Option<HashMap<String, ColumnValue>> {
        match self {
            DeltaKind::Insert { values, .. } => Some(values.clone()),
            DeltaKind::Delete { .. } => None,
            DeltaKind::Update { new_values, .. } => Some(new_values.clone()),
        }
    }

    fn old_values(&self) -> Option<HashMap<String, ColumnValue>> {
        match self {
            DeltaKind::Insert { .. } => None,
            DeltaKind::Delete { old_values, .. } => Some(old_values.clone()),
            DeltaKind::Update { old_values, .. } => Some(old_values.clone()),
        }
    }
}

fn classify_conflict(ours: &DeltaKind, theirs: &DeltaKind) -> ConflictType {
    match (ours, theirs) {
        (DeltaKind::Insert { .. }, DeltaKind::Insert { .. }) => ConflictType::BothAdded,
        (DeltaKind::Update { .. }, DeltaKind::Delete { .. })
        | (DeltaKind::Delete { .. }, DeltaKind::Update { .. }) => ConflictType::ModifyDelete,
        _ => ConflictType::BothModified,
    }
}

fn pk_key(pk: &[ColumnValue]) -> String {
    pk.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_fast_forward() {
        let ours = vec![(
            vec![ColumnValue::Integer(1)],
            DeltaKind::Update {
                pk: vec![ColumnValue::Integer(1)],
                old_values: HashMap::from([("name".into(), ColumnValue::Text("old".into()))]),
                new_values: HashMap::from([("name".into(), ColumnValue::Text("new".into()))]),
            },
        )];
        let theirs = vec![];

        let result = merge_deltas(&ours, &theirs);
        assert!(!result.has_conflicts());
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].resolution, Resolution::FastForward);
    }

    #[test]
    fn test_merge_conflict() {
        let pk = vec![ColumnValue::Integer(1)];
        let ours = vec![(
            pk.clone(),
            DeltaKind::Update {
                pk: pk.clone(),
                old_values: HashMap::from([("name".into(), ColumnValue::Text("original".into()))]),
                new_values: HashMap::from([("name".into(), ColumnValue::Text("ours".into()))]),
            },
        )];
        let theirs = vec![(
            pk.clone(),
            DeltaKind::Update {
                pk: pk.clone(),
                old_values: HashMap::from([("name".into(), ColumnValue::Text("original".into()))]),
                new_values: HashMap::from([("name".into(), ColumnValue::Text("theirs".into()))]),
            },
        )];

        let result = merge_deltas(&ours, &theirs);
        assert!(result.has_conflicts());
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(
            result.conflicts[0].conflict_type,
            ConflictType::BothModified
        );
    }
}
