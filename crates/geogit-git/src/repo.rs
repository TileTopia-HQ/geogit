use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// A GeoGit repository wrapping a git repository.
pub struct Repository {
    /// Path to the repository root (working directory).
    pub workdir: PathBuf,
}

impl Repository {
    /// Initialize a new GeoGit repository at the given path.
    pub fn init(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path).context("failed to create repository directory")?;

        let output = Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .context("failed to run git init")?;

        if !output.status.success() {
            bail!(
                "git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(Self {
            workdir: path.to_path_buf(),
        })
    }

    /// Open an existing GeoGit repository.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.join(".git").exists() {
            bail!("not a git repository: {}", path.display());
        }
        Ok(Self {
            workdir: path.to_path_buf(),
        })
    }

    /// Get the HEAD commit hash, if any.
    pub fn head_commit(&self) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git rev-parse")?;

        if output.status.success() {
            Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            ))
        } else {
            Ok(None)
        }
    }

    /// List all branches.
    pub fn branches(&self) -> Result<Vec<BranchInfo>> {
        let output = Command::new("git")
            .args(["branch", "--format=%(refname:short) %(objectname:short)"])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git branch")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let branches = stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(|line| {
                let mut parts = line.splitn(2, ' ');
                let name = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                BranchInfo { name, target }
            })
            .collect();
        Ok(branches)
    }

    /// Get the current branch name.
    pub fn current_branch(&self) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git branch --show-current")?;

        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() {
            Ok(None)
        } else {
            Ok(Some(name))
        }
    }

    /// Create a new branch at the given target.
    pub fn create_branch(&self, name: &str, target: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["branch", name, target])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git branch")?;

        if !output.status.success() {
            bail!(
                "git branch failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Delete a branch.
    pub fn delete_branch(&self, name: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["branch", "-d", name])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git branch -d")?;

        if !output.status.success() {
            bail!(
                "git branch -d failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    /// Stage all changes and commit.
    pub fn commit(&self, message: &str) -> Result<String> {
        let output = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git add")?;

        if !output.status.success() {
            bail!(
                "git add failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let output = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.workdir)
            .output()
            .context("failed to run git commit")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("nothing to commit") {
                return Ok("nothing to commit".into());
            }
            bail!("git commit failed: {stderr}");
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// Information about a branch.
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub target: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_init_and_open() {
        let dir = tempdir();
        let _repo = Repository::init(dir.path()).unwrap();
        assert!(dir.path().join(".git").exists());

        let _repo2 = Repository::open(dir.path()).unwrap();
    }

    #[test]
    fn test_head_on_empty_repo() {
        let dir = tempdir();
        let repo = Repository::init(dir.path()).unwrap();
        let head = repo.head_commit().unwrap();
        assert!(head.is_none());
    }

    #[test]
    fn test_commit_and_branch() {
        let dir = tempdir();
        let repo = Repository::init(dir.path()).unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        let result = repo.commit("initial").unwrap();
        assert!(!result.contains("nothing to commit"));

        let head = repo.head_commit().unwrap();
        assert!(head.is_some());

        repo.create_branch("feature", "HEAD").unwrap();
        let branches = repo.branches().unwrap();
        assert!(branches.iter().any(|b| b.name == "feature"));

        repo.delete_branch("feature").unwrap();
        let branches = repo.branches().unwrap();
        assert!(!branches.iter().any(|b| b.name == "feature"));
    }

    fn tempdir() -> TempDir {
        TempDir::new()
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "geogit-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
