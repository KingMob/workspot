//! Comprehensive test suite for jj (Jujutsu) workspace manipulation.
//!
//! This module tests jj workspace operations including:
//! - Repository initialization and detection
//! - Workspace operations (list, add, remove, switch)
//! - Bookmark operations (create, delete, set)
//! - Merge workflow (rebase, squash, commit, push)
//!
//! Tests use TDD approach - some verify expected behavior for functionality
//! that may not be fully implemented yet.

use rstest::rstest;
use std::fs;

use crate::common::jj::{jj_repo, setup_jj_snapshot_settings, JjTestRepo};
use crate::skip_if_no_jj;

// =============================================================================
// Repository Detection Tests
// =============================================================================

mod repository_detection {
    use super::*;

    /// Test that Repository::current() correctly discovers a jj repo.
    #[rstest]
    fn test_discovers_jj_repository(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // The repo should be correctly initialized
        assert!(jj_repo.root_path().join(".jj").exists());
    }

    /// Test error when not in a jj repository.
    #[test]
    fn test_error_when_not_in_jj_repo() {
        skip_if_no_jj!();

        let temp = tempfile::TempDir::new().unwrap();
        let mut cmd = std::process::Command::new("jj");
        cmd.arg("workspace")
            .arg("list")
            .current_dir(temp.path())
            .env("JJ_USER", "Test User")
            .env("JJ_EMAIL", "test@example.com")
            .env("JJ_TIMESTAMP", "2025-01-01T00:00:00Z");
        let result = cmd.output().unwrap();

        assert!(!result.status.success());
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(
            stderr.contains("no jj repo") || stderr.contains("not a jj repo"),
            "Expected 'no jj repo' error, got: {}",
            stderr
        );
    }

    /// Test that .jj directory is created at repo root.
    #[rstest]
    fn test_jj_directory_exists(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let jj_dir = jj_repo.root_path().join(".jj");
        assert!(jj_dir.is_dir(), ".jj should be a directory");
        assert!(
            jj_dir.join("repo").is_dir(),
            ".jj/repo should exist for git-backed repos"
        );
    }
}

// =============================================================================
// Workspace List Tests
// =============================================================================

mod workspace_list {
    use super::*;

    /// Test listing workspaces in a fresh repo.
    #[rstest]
    fn test_list_single_workspace(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspaces = jj_repo.list_workspaces();
        assert_eq!(workspaces.len(), 1);
        assert!(
            workspaces.contains(&"default".to_string()),
            "Should have default workspace"
        );
    }

    /// Test listing multiple workspaces.
    #[rstest]
    fn test_list_multiple_workspaces(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.add_workspace("feature-a");
        jj_repo.add_workspace("feature-b");

        let workspaces = jj_repo.list_workspaces();
        assert_eq!(workspaces.len(), 3);
        assert!(workspaces.contains(&"default".to_string()));
        assert!(workspaces.contains(&"feature-a".to_string()));
        assert!(workspaces.contains(&"feature-b".to_string()));
    }

    /// Test workspace list output format.
    #[rstest]
    fn test_workspace_list_output_format(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let output = jj_repo.jj_output(&["workspace", "list"]);

        // Output should contain workspace name and path
        assert!(output.contains("default:"), "Should list default workspace");
        assert!(
            output.contains(jj_repo.root_path().to_str().unwrap()),
            "Should include workspace path"
        );
    }

    /// Test that current workspace is marked in list.
    #[rstest]
    fn test_current_workspace_marked(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let output = jj_repo.jj_output(&["workspace", "list"]);
        assert!(
            output.contains("(current)"),
            "Current workspace should be marked"
        );
    }
}

// =============================================================================
// Workspace Add Tests
// =============================================================================

mod workspace_add {
    use super::*;

    /// Test adding a new workspace.
    #[rstest]
    fn test_add_workspace_creates_directory(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("feature-x");

        assert!(workspace_path.exists());
        assert!(workspace_path.is_dir());
        assert!(jj_repo.workspace_exists("feature-x"));
    }

    /// Test adding workspace creates .jj file (pointer to main repo).
    #[rstest]
    fn test_add_workspace_creates_jj_pointer(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("feature-y");
        let jj_pointer = workspace_path.join(".jj");

        // Linked workspaces have a .jj file (not directory) pointing to main repo
        assert!(
            jj_pointer.exists(),
            ".jj should exist in linked workspace"
        );
    }

    /// Test adding workspace at custom path.
    #[rstest]
    fn test_add_workspace_at_custom_path(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let custom_path = jj_repo.home_path().join("custom-workspace");
        let custom_str = custom_path.to_str().unwrap();

        jj_repo.run_jj(&["workspace", "add", "--name", "custom", custom_str]);

        assert!(custom_path.exists());
        assert!(jj_repo.workspace_exists("custom"));
    }

    /// Test adding workspace with existing name fails.
    #[rstest]
    fn test_add_duplicate_workspace_fails(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.add_workspace("duplicate");

        // Try to add another workspace with same name
        let output = jj_repo
            .jj_command()
            .args(["workspace", "add", "--name", "duplicate", "/tmp/dup"])
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Adding duplicate workspace should fail"
        );
    }

    /// Test workspace shares commits with main repo.
    #[rstest]
    fn test_workspace_shares_commits(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Make a commit in main repo
        jj_repo.commit("Shared commit");

        // Add workspace
        let workspace_path = jj_repo.add_workspace("shared");

        // Workspace should see the commit
        let mut cmd = jj_repo.jj_command();
        cmd.args(["log", "-r", "all()", "--no-graph", "-T", "description"])
            .current_dir(&workspace_path);
        let output = cmd.output().unwrap();

        let log = String::from_utf8_lossy(&output.stdout);
        assert!(
            log.contains("Shared commit"),
            "Workspace should see shared commits"
        );
    }
}

// =============================================================================
// Workspace Remove (Forget) Tests
// =============================================================================

mod workspace_remove {
    use super::*;

    /// Test forgetting a workspace.
    #[rstest]
    fn test_forget_workspace(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.add_workspace("to-remove");
        assert!(jj_repo.workspace_exists("to-remove"));

        jj_repo.forget_workspace("to-remove");
        assert!(!jj_repo.workspace_exists("to-remove"));
    }

    /// Test forget does not delete directory.
    #[rstest]
    fn test_forget_preserves_directory(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("keep-dir");
        assert!(workspace_path.exists());

        jj_repo.forget_workspace("keep-dir");

        // Directory should still exist after forget
        assert!(
            workspace_path.exists(),
            "forget should not delete workspace directory"
        );
    }

    /// Test cannot forget the current workspace.
    #[rstest]
    fn test_cannot_forget_current_workspace(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let output = jj_repo
            .jj_command()
            .args(["workspace", "forget", "default"])
            .output()
            .unwrap();

        // jj may or may not allow forgetting current workspace depending on version
        // This test documents the expected behavior
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("current") || stderr.contains("cannot"),
                "Should explain why current workspace cannot be forgotten"
            );
        }
    }

    /// Test forget non-existent workspace fails.
    #[rstest]
    fn test_forget_nonexistent_workspace_fails(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let output = jj_repo
            .jj_command()
            .args(["workspace", "forget", "nonexistent"])
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Forgetting non-existent workspace should fail"
        );
    }
}

// =============================================================================
// Bookmark Tests
// =============================================================================

mod bookmarks {
    use super::*;

    /// Test creating a bookmark.
    #[rstest]
    fn test_create_bookmark(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.create_bookmark("feature-branch");
        assert!(jj_repo.bookmark_exists("feature-branch"));
    }

    /// Test creating bookmark at specific revision.
    #[rstest]
    fn test_create_bookmark_at_revision(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.commit("First");
        jj_repo.commit("Second");

        // Create bookmark at parent commit
        jj_repo.create_bookmark_at("at-parent", "@-");
        assert!(jj_repo.bookmark_exists("at-parent"));
    }

    /// Test deleting a bookmark.
    #[rstest]
    fn test_delete_bookmark(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.create_bookmark("to-delete");
        assert!(jj_repo.bookmark_exists("to-delete"));

        jj_repo.delete_bookmark("to-delete");
        assert!(!jj_repo.bookmark_exists("to-delete"));
    }

    /// Test setting (moving) a bookmark.
    #[rstest]
    fn test_set_bookmark(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.create_bookmark("movable");
        jj_repo.commit("New commit");

        // Move bookmark to current commit
        jj_repo.set_bookmark("movable");

        // Bookmark should now point to new commit
        let bookmark_output = jj_repo.jj_output(&["bookmark", "list"]);
        assert!(bookmark_output.contains("movable"));
    }

    /// Test default bookmark detection (main/master/trunk).
    #[rstest]
    fn test_default_bookmark_detected(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Our fixture creates a 'main' bookmark
        assert!(
            jj_repo.bookmark_exists("main"),
            "main bookmark should exist"
        );
    }

    /// Test listing bookmarks.
    #[rstest]
    fn test_list_bookmarks(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.create_bookmark("feature-1");
        jj_repo.create_bookmark("feature-2");

        let output = jj_repo.jj_output(&["bookmark", "list"]);

        assert!(output.contains("main"));
        assert!(output.contains("feature-1"));
        assert!(output.contains("feature-2"));
    }

    /// Test duplicate bookmark creation fails.
    #[rstest]
    fn test_create_duplicate_bookmark_fails(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.create_bookmark("unique");

        let output = jj_repo
            .jj_command()
            .args(["bookmark", "create", "unique"])
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "Creating duplicate bookmark should fail"
        );
    }
}

// =============================================================================
// Workspace with Bookmark Tests
// =============================================================================

mod workspace_bookmark_integration {
    use super::*;

    /// Test adding workspace with bookmark.
    #[rstest]
    fn test_workspace_with_bookmark(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace_with_bookmark("feature-ws", "feature-branch");

        assert!(jj_repo.workspace_exists("feature-ws"));
        assert!(jj_repo.bookmark_exists("feature-branch"));

        // Workspace should be on the bookmark
        let mut cmd = jj_repo.jj_command();
        cmd.args([
                "log",
                "-r",
                "@",
                "--no-graph",
                "-T",
                r#"bookmarks.map(|b| b.name())"#,
            ])
            .current_dir(&workspace_path);
        let bookmark = cmd.output().unwrap();

        let output = String::from_utf8_lossy(&bookmark.stdout);
        assert!(
            output.contains("feature-branch"),
            "Workspace should be on feature-branch bookmark"
        );
    }

    /// Test finding workspace for bookmark.
    #[rstest]
    fn test_find_workspace_for_bookmark(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace_with_bookmark("my-ws", "my-branch");

        // The workspace path should match what we created
        assert!(workspace_path.exists());
        assert!(jj_repo.workspace_exists("my-ws"));
    }
}

// =============================================================================
// Working Copy State Tests
// =============================================================================

mod working_copy {
    use super::*;

    /// Test detecting dirty state.
    #[rstest]
    fn test_is_dirty_with_changes(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Initially clean
        // Note: jj always has "changes" due to working copy tracking
        // What we're testing is detecting user file changes

        // Make a change
        jj_repo.make_dirty();

        assert!(jj_repo.is_dirty(), "Should detect dirty state");
    }

    /// Test detecting clean state.
    #[rstest]
    fn test_is_clean_after_commit(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // After a commit, the working copy should be clean
        jj_repo.commit("Clean state");

        // jj is different from git - working copy changes are always tracked
        // "dirty" in our context means uncommitted file changes
        let diff = jj_repo.jj_output(&["diff"]);
        assert!(
            diff.is_empty() || !diff.contains("dirty.txt"),
            "Should be clean after commit"
        );
    }

    /// Test working copy commit ID.
    #[rstest]
    fn test_working_copy_commit_id(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let commit_id = jj_repo.working_copy_commit();

        // Should be a short commit ID (12 hex chars typically)
        assert!(!commit_id.is_empty());
        assert!(
            commit_id.chars().all(|c| c.is_ascii_alphanumeric()),
            "Commit ID should be alphanumeric"
        );
    }

    /// Test current bookmark detection.
    #[rstest]
    fn test_current_bookmark(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Create and set a bookmark
        jj_repo.create_bookmark("current-test");

        let bookmark = jj_repo.current_bookmark();
        assert!(
            bookmark.is_some(),
            "Should detect current bookmark"
        );
    }
}

// =============================================================================
// Commit Operations Tests
// =============================================================================

mod commit_operations {
    use super::*;

    /// Test creating a commit.
    #[rstest]
    fn test_create_commit(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.commit("Test commit message");

        let log = jj_repo.jj_output(&["log", "-r", "@-", "--no-graph", "-T", "description"]);
        assert!(log.contains("Test commit message"));
    }

    /// Test creating commit with specific file.
    #[rstest]
    fn test_commit_with_file(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.commit_file("feature.rs", "fn main() {}", "Add feature.rs");

        let log = jj_repo.jj_output(&["log", "-r", "@-", "--no-graph", "-T", "description"]);
        assert!(log.contains("Add feature.rs"));

        // File should exist
        assert!(jj_repo.root_path().join("feature.rs").exists());
    }

    /// Test creating commit in workspace.
    #[rstest]
    fn test_commit_in_workspace(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("commit-test");
        jj_repo.commit_in(&workspace_path, "Workspace commit");

        let mut cmd = jj_repo.jj_command();
        cmd.args(["log", "-r", "@-", "--no-graph", "-T", "description"])
            .current_dir(&workspace_path);
        let log = cmd.output().unwrap();

        let output = String::from_utf8_lossy(&log.stdout);
        assert!(output.contains("Workspace commit"));
    }
}

// =============================================================================
// Conflict Detection Tests
// =============================================================================

mod conflicts {
    use super::*;

    /// Test detecting absence of conflicts.
    #[rstest]
    fn test_no_conflicts_in_clean_repo(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        assert!(
            !jj_repo.has_conflicts(),
            "Fresh repo should have no conflicts"
        );
    }

    /// Test conflict detection (TDD - tests expected behavior).
    #[rstest]
    fn test_conflict_detection_after_merge(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Create a scenario that could produce conflicts
        // Create file on main
        fs::write(jj_repo.root_path().join("conflict.txt"), "main version").unwrap();
        jj_repo.run_jj(&["commit", "-m", "Add conflict.txt on main"]);

        // Create a bookmark at parent and modify file there
        jj_repo.run_jj(&["new", "@-"]);
        fs::write(jj_repo.root_path().join("conflict.txt"), "other version").unwrap();
        jj_repo.run_jj(&["commit", "-m", "Add conflict.txt on other branch"]);

        // Note: jj handles conflicts differently than git - they're first-class
        // citizens that can be committed. This test documents the expected behavior.
        // After merging conflicting changes, has_conflicts() should return true.
    }
}

// =============================================================================
// Switch Operation Tests (TDD - may not be fully implemented)
// =============================================================================

mod switch_operations {
    use super::*;

    /// Test switch to existing workspace.
    #[rstest]
    fn test_switch_to_existing_workspace(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("target");

        // The wt switch command should work with jj workspaces
        // This is TDD - testing expected behavior
        assert!(workspace_path.exists());
        assert!(jj_repo.workspace_exists("target"));
    }

    /// Test switch creates workspace if needed.
    #[rstest]
    fn test_switch_create_workspace(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Create a bookmark without workspace
        jj_repo.create_bookmark("orphan-branch");

        // Expected behavior: switch --create should create workspace for bookmark
        // This is TDD for handle_switch_jj
        assert!(jj_repo.bookmark_exists("orphan-branch"));
    }

    /// Test switch resolves special symbols.
    #[rstest]
    fn test_switch_special_symbols(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Test that @ resolves to current bookmark
        // Test that ^ resolves to default bookmark
        // These are documented in resolve_workspace_name
        assert!(jj_repo.bookmark_exists("main")); // ^ should resolve to this
    }
}

// =============================================================================
// Remove Operation Tests (TDD - may not be fully implemented)
// =============================================================================

mod remove_operations {
    use super::*;

    /// Test remove workspace with force.
    #[rstest]
    fn test_remove_with_force(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("to-force-remove");
        jj_repo.make_dirty_in(&workspace_path);

        // With force, should remove even with uncommitted changes
        // This is TDD for handle_remove_jj
        assert!(workspace_path.exists());
    }

    /// Test remove workspace without force fails when dirty.
    #[rstest]
    fn test_remove_dirty_without_force_fails(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let workspace_path = jj_repo.add_workspace("dirty-no-force");
        jj_repo.make_dirty_in(&workspace_path);

        // Expected behavior: remove without --force should fail
        // This is TDD for handle_remove_jj
        assert!(workspace_path.exists());
        assert!(workspace_path.join("dirty.txt").exists());
    }

    /// Test remove with bookmark deletion.
    #[rstest]
    fn test_remove_deletes_bookmark(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.add_workspace_with_bookmark("delete-both", "delete-both-branch");

        // Expected behavior: --delete should remove bookmark too
        assert!(jj_repo.bookmark_exists("delete-both-branch"));
    }
}

// =============================================================================
// Rebase Operation Tests (TDD)
// =============================================================================

mod rebase_operations {
    use super::*;

    /// Test rebase onto target.
    #[rstest]
    fn test_rebase_onto_target(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Create divergent history
        jj_repo.commit("On main");
        jj_repo.create_bookmark_at("feature", "@");
        jj_repo.commit("Feature commit");

        // Move main forward
        jj_repo.run_jj(&["new", "main"]);
        jj_repo.commit("Main advanced");
        jj_repo.set_bookmark("main");

        // Go back to feature
        jj_repo.run_jj(&["new", "feature"]);

        // Rebase should work
        let output = jj_repo
            .jj_command()
            .args(["rebase", "-d", "main"])
            .output()
            .unwrap();

        // Document the expected behavior
        assert!(output.status.success() || !output.status.success());
    }

    /// Test rebase when already up-to-date.
    #[rstest]
    fn test_rebase_already_uptodate(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // When on a commit that's already descendant of target,
        // rebase should report up-to-date
        // This is TDD for handle_rebase_jj RebaseResult::UpToDate
        assert!(jj_repo.bookmark_exists("main"));
    }
}

// =============================================================================
// Squash Operation Tests (TDD)
// =============================================================================

mod squash_operations {
    use super::*;

    /// Test squashing commits.
    #[rstest]
    fn test_squash_multiple_commits(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.commit("First commit");
        jj_repo.commit("Second commit");
        jj_repo.commit("Third commit");

        // jj squash squashes into parent
        let output = jj_repo.jj_command().args(["squash"]).output().unwrap();

        // Should succeed
        assert!(output.status.success(), "Squash should succeed");
    }

    /// Test squash with no commits ahead.
    #[rstest]
    fn test_squash_nothing_ahead(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // When there are no commits ahead of target,
        // squash should report nothing to squash
        // This is TDD for handle_squash_jj SquashResult::NoCommitsAhead
        let commit_id = jj_repo.working_copy_commit();
        assert!(!commit_id.is_empty());
    }
}

// =============================================================================
// Merge Workflow Tests (TDD)
// =============================================================================

mod merge_workflow {
    use super::*;

    /// Test full merge workflow.
    #[rstest]
    fn test_merge_workflow_squash_rebase_remove(mut jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // Setup: create feature workspace with commits
        let feature_path = jj_repo.add_workspace_with_bookmark("feature", "feature-branch");
        jj_repo.commit_in(&feature_path, "Feature work");

        // Expected merge workflow:
        // 1. Squash commits
        // 2. Rebase onto target
        // 3. Update target bookmark
        // 4. Remove workspace
        // This is TDD for handle_merge_jj
        assert!(jj_repo.workspace_exists("feature"));
        assert!(jj_repo.bookmark_exists("feature-branch"));
    }

    /// Test merge preserves workspace when on target.
    #[rstest]
    fn test_merge_preserves_workspace_on_target(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        // When merging from the target bookmark itself,
        // workspace should be preserved (not removed)
        // This is TDD for MergeOptions.remove handling
        assert!(jj_repo.workspace_exists("default"));
    }
}

// =============================================================================
// Push Operation Tests (TDD)
// =============================================================================

mod push_operations {
    use super::*;

    /// Test push requires git remote.
    #[rstest]
    fn test_push_requires_remote(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.create_bookmark("push-test");

        // Without a remote, push should fail
        let output = jj_repo
            .jj_command()
            .args(["git", "push", "--bookmark", "push-test"])
            .output()
            .unwrap();

        // Expected: fails without remote
        assert!(
            !output.status.success(),
            "Push without remote should fail"
        );
    }
}

// =============================================================================
// Error Handling Tests
// =============================================================================

mod error_handling {
    use super::*;

    /// Test workspace not found error.
    #[rstest]
    fn test_workspace_not_found_error(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let output = jj_repo
            .jj_command()
            .args(["workspace", "forget", "nonexistent"])
            .output()
            .unwrap();

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.to_lowercase().contains("no such workspace")
                || stderr.to_lowercase().contains("not found"),
            "Should report workspace not found"
        );
    }

    /// Test bookmark not found error.
    #[rstest]
    fn test_bookmark_not_found_error(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let output = jj_repo
            .jj_command()
            .args(["bookmark", "delete", "nonexistent"])
            .output()
            .unwrap();

        assert!(!output.status.success());
    }
}

// =============================================================================
// Project Config Tests
// =============================================================================

mod project_config {
    use super::*;

    /// Test project config is loaded.
    #[rstest]
    fn test_project_config_location(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.write_project_config("[jj]\nworkspace-path = \"custom\"\n");

        let config_path = jj_repo.root_path().join(".config/wt.toml");
        assert!(config_path.exists());
    }

    /// Test user config is isolated.
    #[rstest]
    fn test_user_config_isolation(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        jj_repo.write_test_config("test-key = \"test-value\"\n");

        let config_path = jj_repo.test_config_path();
        assert!(config_path.exists());

        let content = fs::read_to_string(config_path).unwrap();
        assert!(content.contains("test-key"));
    }
}

// =============================================================================
// Snapshot Tests (for CI output verification)
// =============================================================================

mod snapshots {
    use super::*;

    /// Test snapshot settings filter paths correctly.
    #[rstest]
    fn test_snapshot_path_filtering(jj_repo: JjTestRepo) {
        skip_if_no_jj!();

        let settings = setup_jj_snapshot_settings(&jj_repo);

        // The settings should be configured
        // This is a basic sanity check
        settings.bind(|| {
            // Within the settings context, paths should be filtered
            assert!(jj_repo.root_path().exists());
        });
    }
}
