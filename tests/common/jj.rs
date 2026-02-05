//! Test utilities for Jujutsu (jj) workspaces.
//!
//! This module provides test harnesses for testing jj workspace functionality.
//! It mirrors the `TestRepo` pattern but for jj-based repositories.
//!
//! ## JjTestRepo
//!
//! The `JjTestRepo` struct creates isolated jj repositories in temporary directories
//! with deterministic configuration. Each test gets a fresh repo that is automatically
//! cleaned up when the test ends.
//!
//! ## Feature Detection
//!
//! Tests using `JjTestRepo` require jj to be installed. Use the `jj_available()` function
//! to skip tests when jj is not available:
//!
//! ```ignore
//! #[rstest]
//! fn test_jj_workspace(jj_repo: JjTestRepo) {
//!     if !jj_available() {
//!         eprintln!("Skipping test: jj not installed");
//!         return;
//!     }
//!     // ... test code
//! }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{canonicalize, configure_cli_command, set_temp_home_env, wt_bin, TEST_EPOCH};

/// Check if jj is available on the system.
///
/// Returns true if `jj --version` succeeds.
pub fn jj_available() -> bool {
    Command::new("jj")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip test if jj is not available.
///
/// Prints a message and returns early if jj is not installed.
/// Use at the start of tests that require jj.
#[macro_export]
macro_rules! skip_if_no_jj {
    () => {
        if !$crate::common::jj::jj_available() {
            eprintln!("Skipping test: jj not installed");
            return;
        }
    };
}

/// Rstest fixture for jj test repos.
///
/// Creates a fresh `JjTestRepo` for each test.
///
/// Note: this fixture does **not** automatically skip tests when `jj` is not
/// available. Use the [`skip_if_no_jj!`](crate::common::jj::skip_if_no_jj)
/// macro (or [`jj_available`](crate::common::jj::jj_available)) at the start
/// of tests that require `jj`.
///
/// # Example
/// ```ignore
/// use rstest::rstest;
/// use crate::common::jj::{jj_repo, skip_if_no_jj};
///
/// #[rstest]
/// fn test_jj_workspace(jj_repo: JjTestRepo) {
///     skip_if_no_jj!();
///     // `jj_repo` is a fresh `JjTestRepo`
///     // ... test code ...
/// }
/// ```
#[rstest::fixture]
pub fn jj_repo() -> JjTestRepo {
    JjTestRepo::new()
}

/// A test repository using jj instead of git.
///
/// Provides isolation and deterministic timestamps similar to `TestRepo`.
pub struct JjTestRepo {
    temp_dir: TempDir,
    root: PathBuf,
    /// Workspaces created during the test (name -> path)
    pub workspaces: HashMap<String, PathBuf>,
    /// Isolated config file for this test
    test_config_path: PathBuf,
    /// Whether jj is available (for graceful skip)
    jj_available: bool,
}

impl JjTestRepo {
    /// Create a new jj test repository.
    ///
    /// Initializes a jj git repo with:
    /// - An initial commit on the main bookmark
    /// - Deterministic timestamps
    /// - Isolated configuration
    ///
    /// If jj is not installed, the repo is created in a minimal state
    /// and `is_available()` returns false.
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        let root = canonicalize(&root).unwrap();

        let test_config_path = temp_dir.path().join("test-config.toml");

        let repo = Self {
            temp_dir,
            root: root.clone(),
            workspaces: HashMap::new(),
            test_config_path,
            jj_available: jj_available(),
        };

        if repo.jj_available {
            // Initialize jj repository
            repo.run_jj(&["git", "init"]);

            // Configure user for commits
            repo.run_jj(&["config", "set", "--repo", "user.name", "Test User"]);
            repo.run_jj(&["config", "set", "--repo", "user.email", "test@example.com"]);

            // Create initial commit and main bookmark
            std::fs::write(root.join("README.md"), "# Test Repository\n").unwrap();
            repo.run_jj(&["commit", "-m", "Initial commit"]);

            // Create main bookmark at the initial commit
            repo.run_jj(&["bookmark", "create", "main", "-r", "@-"]);
        }

        repo
    }

    /// Check if jj is available for this test.
    ///
    /// Returns false if jj was not installed when the repo was created.
    /// Use this to skip tests gracefully.
    pub fn is_available(&self) -> bool {
        self.jj_available
    }

    /// Get the root path of the repository.
    pub fn root_path(&self) -> &Path {
        &self.root
    }

    /// Get the isolated HOME directory for this test.
    pub fn home_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get the path to the isolated test config file.
    pub fn test_config_path(&self) -> &Path {
        &self.test_config_path
    }

    /// Create a jj command configured for this test repo.
    pub fn jj_command(&self) -> Command {
        let mut cmd = Command::new("jj");
        cmd.current_dir(&self.root);
        cmd.env("JJ_USER", "Test User");
        cmd.env("JJ_EMAIL", "test@example.com");
        cmd.env("JJ_TIMESTAMP", "2025-01-01T00:00:00Z");
        cmd
    }

    /// Run a jj command in the repo, panicking on failure.
    pub fn run_jj(&self, args: &[&str]) {
        let output = self.jj_command().args(args).output().unwrap();
        if !output.status.success() {
            panic!(
                "jj {} failed:\nstdout: {}\nstderr: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    /// Run a jj command in a specific directory.
    pub fn run_jj_in(&self, dir: &Path, args: &[&str]) {
        let mut cmd = self.jj_command();
        cmd.current_dir(dir);
        let output = cmd.args(args).output().unwrap();
        if !output.status.success() {
            panic!(
                "jj {} (in {}) failed:\nstdout: {}\nstderr: {}",
                args.join(" "),
                dir.display(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    /// Run a jj command and return stdout as a trimmed string.
    pub fn jj_output(&self, args: &[&str]) -> String {
        let output = self.jj_command().args(args).output().unwrap();
        if !output.status.success() {
            panic!(
                "jj {} failed:\nstdout: {}\nstderr: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Create a wt command configured for this jj repo.
    pub fn wt_command(&self) -> Command {
        let mut cmd = Command::new(wt_bin());
        configure_cli_command(&mut cmd);
        cmd.env("WORKTRUNK_CONFIG_PATH", &self.test_config_path);
        cmd.env("JJ_USER", "Test User");
        cmd.env("JJ_EMAIL", "test@example.com");
        cmd.env("JJ_TIMESTAMP", "2025-01-01T00:00:00Z");
        cmd.env("WT_TEST_EPOCH", TEST_EPOCH.to_string());
        set_temp_home_env(&mut cmd, self.home_path());
        cmd.current_dir(self.root_path());
        cmd
    }

    /// Add a workspace with the given name.
    ///
    /// Creates a new jj workspace at a sibling directory.
    /// The workspace name is used as the directory suffix.
    pub fn add_workspace(&mut self, name: &str) -> PathBuf {
        let workspace_path = self.temp_dir.path().join(format!("repo.{}", name));
        let workspace_str = workspace_path.to_str().unwrap();

        self.run_jj(&["workspace", "add", "--name", name, workspace_str]);

        let canonical_path = canonicalize(&workspace_path).unwrap();
        self.workspaces.insert(name.to_string(), canonical_path.clone());
        canonical_path
    }

    /// Add a workspace with a bookmark.
    ///
    /// Creates a new workspace and sets the specified bookmark on it.
    pub fn add_workspace_with_bookmark(&mut self, name: &str, bookmark: &str) -> PathBuf {
        let workspace_path = self.add_workspace(name);

        // Create bookmark if it doesn't exist
        let bookmarks = self.jj_output(&["bookmark", "list"]);
        if !bookmarks.lines().any(|l| l.starts_with(bookmark)) {
            self.run_jj_in(&workspace_path, &["bookmark", "create", bookmark]);
        } else {
            self.run_jj_in(&workspace_path, &["bookmark", "set", bookmark]);
        }

        workspace_path
    }

    /// Create a commit with a file and message.
    pub fn commit(&self, message: &str) {
        let file_path = self.root.join("file.txt");
        std::fs::write(&file_path, message).unwrap();
        self.run_jj(&["commit", "-m", message]);
    }

    /// Create a commit in a specific workspace.
    pub fn commit_in(&self, workspace_path: &Path, message: &str) {
        let file_path = workspace_path.join("file.txt");
        std::fs::write(&file_path, message).unwrap();
        self.run_jj_in(workspace_path, &["commit", "-m", message]);
    }

    /// Create a commit with a specific file.
    pub fn commit_file(&self, filename: &str, content: &str, message: &str) {
        let file_path = self.root.join(filename);
        std::fs::write(&file_path, content).unwrap();
        self.run_jj(&["commit", "-m", message]);
    }

    /// Create a bookmark at the current commit.
    pub fn create_bookmark(&self, name: &str) {
        self.run_jj(&["bookmark", "create", name]);
    }

    /// Create a bookmark at a specific revision.
    pub fn create_bookmark_at(&self, name: &str, revision: &str) {
        self.run_jj(&["bookmark", "create", name, "-r", revision]);
    }

    /// Set (move) a bookmark to the current commit.
    pub fn set_bookmark(&self, name: &str) {
        self.run_jj(&["bookmark", "set", name]);
    }

    /// Delete a bookmark.
    pub fn delete_bookmark(&self, name: &str) {
        self.run_jj(&["bookmark", "delete", name]);
    }

    /// Check if a bookmark exists.
    pub fn bookmark_exists(&self, name: &str) -> bool {
        let output = self.jj_output(&["bookmark", "list"]);
        output.lines().any(|line| {
            let bookmark_name = line.split(':').next().unwrap_or("").trim();
            bookmark_name == name
        })
    }

    /// Get the list of workspace names.
    pub fn list_workspaces(&self) -> Vec<String> {
        let output = self.jj_output(&["workspace", "list"]);
        output
            .lines()
            .filter_map(|line| line.split(':').next().map(|s| s.trim().to_string()))
            .collect()
    }

    /// Check if a workspace exists.
    pub fn workspace_exists(&self, name: &str) -> bool {
        self.list_workspaces().contains(&name.to_string())
    }

    /// Remove a workspace.
    pub fn forget_workspace(&mut self, name: &str) {
        self.run_jj(&["workspace", "forget", name]);
        self.workspaces.remove(name);
    }

    /// Get the working copy commit ID.
    pub fn working_copy_commit(&self) -> String {
        self.jj_output(&["log", "-r", "@", "--no-graph", "-T", "commit_id.short()"])
    }

    /// Check if the workspace has uncommitted changes.
    pub fn is_dirty(&self) -> bool {
        let output = self.jj_output(&["diff", "--stat"]);
        !output.is_empty()
    }

    /// Check if the workspace has conflicts.
    pub fn has_conflicts(&self) -> bool {
        let output = self.jj_output(&["log", "-r", "@", "-T", "conflict"]);
        output.contains("true")
    }

    /// Get the current bookmark (if on one).
    pub fn current_bookmark(&self) -> Option<String> {
        let output = self.jj_output(&[
            "log",
            "-r",
            "@",
            "--no-graph",
            "-T",
            r#"bookmarks.map(|b| b.name() ++ "\n")"#,
        ]);
        output.lines().next().map(|s| s.to_string())
    }

    /// Write test configuration to the config file.
    pub fn write_test_config(&self, contents: &str) {
        let full_contents = format!("skip-commit-generation-prompt = true\n{}", contents);
        std::fs::write(&self.test_config_path, full_contents).unwrap();
    }

    /// Write project configuration (.config/wt.toml).
    pub fn write_project_config(&self, contents: &str) {
        let config_dir = self.root_path().join(".config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("wt.toml"), contents).unwrap();
    }

    /// Create uncommitted changes (dirty state).
    pub fn make_dirty(&self) {
        std::fs::write(self.root.join("dirty.txt"), "uncommitted changes").unwrap();
    }

    /// Create uncommitted changes in a workspace.
    pub fn make_dirty_in(&self, workspace_path: &Path) {
        std::fs::write(workspace_path.join("dirty.txt"), "uncommitted changes").unwrap();
    }
}

impl Default for JjTestRepo {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot settings filter for jj test repos.
///
/// Filters out paths that vary between test runs.
pub fn setup_jj_snapshot_settings(repo: &JjTestRepo) -> insta::Settings {
    let mut settings = insta::Settings::clone_current();

    // Filter the temp directory path
    let root_path = repo.root_path().to_string_lossy();
    settings.add_filter(&format!(r"{}/?", regex::escape(&root_path)), "[REPO]/");

    // Filter parent temp directory
    let home_path = repo.home_path().to_string_lossy();
    settings.add_filter(&format!(r"{}/?", regex::escape(&home_path)), "[HOME]/");

    // Filter commit IDs (short form - 12 hex chars)
    settings.add_filter(r"\b[a-f0-9]{12}\b", "[COMMIT_ID]");

    // Filter change IDs (jj uses different format)
    settings.add_filter(r"\b[a-z]{8,12}\b", "[CHANGE_ID]");

    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jj_available_returns_bool() {
        // Just verify it doesn't panic
        let _ = jj_available();
    }
}
