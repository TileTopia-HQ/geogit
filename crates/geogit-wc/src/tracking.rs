use rusqlite::Connection;

/// Change tracking table management for GeoPackage working copies.
///
/// GeoGit installs triggers on each dataset table that record
/// INSERT/UPDATE/DELETE operations in a tracking table. This allows
/// efficient detection of changes without scanning all rows.
pub struct ChangeTracker {
    tracking_table: String,
}

impl Default for ChangeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeTracker {
    pub fn new() -> Self {
        Self {
            tracking_table: "_geogit_track".to_string(),
        }
    }

    /// Create the change tracking table if it doesn't exist.
    pub fn init(&self, conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                table_name TEXT NOT NULL,
                pk TEXT NOT NULL,
                change_type TEXT NOT NULL CHECK(change_type IN ('I', 'U', 'D')),
                changed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            self.tracking_table
        ))?;
        Ok(())
    }

    /// Install INSERT/UPDATE/DELETE triggers on a table.
    pub fn install_triggers(
        &self,
        conn: &Connection,
        table_name: &str,
        pk_column: &str,
    ) -> rusqlite::Result<()> {
        // INSERT trigger
        conn.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS _geogit_ins_{table_name}
             AFTER INSERT ON \"{table_name}\"
             BEGIN
                 INSERT INTO {track} (table_name, pk, change_type)
                 VALUES ('{table_name}', CAST(NEW.\"{pk_column}\" AS TEXT), 'I');
             END",
            track = self.tracking_table,
        ))?;

        // UPDATE trigger
        conn.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS _geogit_upd_{table_name}
             AFTER UPDATE ON \"{table_name}\"
             BEGIN
                 INSERT INTO {track} (table_name, pk, change_type)
                 VALUES ('{table_name}', CAST(NEW.\"{pk_column}\" AS TEXT), 'U');
             END",
            track = self.tracking_table,
        ))?;

        // DELETE trigger
        conn.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS _geogit_del_{table_name}
             AFTER DELETE ON \"{table_name}\"
             BEGIN
                 INSERT INTO {track} (table_name, pk, change_type)
                 VALUES ('{table_name}', CAST(OLD.\"{pk_column}\" AS TEXT), 'D');
             END",
            track = self.tracking_table,
        ))?;

        Ok(())
    }

    /// Get all tracked changes for a table.
    pub fn get_changes(
        &self,
        conn: &Connection,
        table_name: &str,
    ) -> rusqlite::Result<Vec<TrackedChange>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT pk, change_type FROM {} WHERE table_name = ?1 ORDER BY rowid",
            self.tracking_table
        ))?;
        let rows = stmt.query_map([table_name], |row| {
            Ok(TrackedChange {
                pk: row.get(0)?,
                change_type: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    /// Clear tracked changes for a table (after commit).
    pub fn clear(&self, conn: &Connection, table_name: &str) -> rusqlite::Result<()> {
        conn.execute(
            &format!(
                "DELETE FROM {} WHERE table_name = ?1",
                self.tracking_table
            ),
            [table_name],
        )?;
        Ok(())
    }
}

/// A tracked change from the tracking table.
#[derive(Debug, Clone)]
pub struct TrackedChange {
    /// Primary key value as text.
    pub pk: String,
    /// Change type: 'I' (insert), 'U' (update), 'D' (delete).
    pub change_type: String,
}

impl TrackedChange {
    pub fn is_insert(&self) -> bool {
        self.change_type == "I"
    }

    pub fn is_update(&self) -> bool {
        self.change_type == "U"
    }

    pub fn is_delete(&self) -> bool {
        self.change_type == "D"
    }
}
