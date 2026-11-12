use crate::views::settings::VcsCommand;
use anyhow::{Context, Result, anyhow};
use git2::Repository;
use std::path::{Path, PathBuf};

/// Get the repository root for a given path using the configured VCS.
///
/// # Errors
///
/// Returns an error if the path is not within a repository for the configured VCS.
pub fn find_repository_root_for_vcs(path: &Path, vcs_command: &VcsCommand) -> Result<PathBuf> {
    match vcs_command {
        VcsCommand::Git => find_repository_root(path),
        VcsCommand::Jujutsu => find_jujutsu_repository_root(path),
    }
}

/// Get the Git repository root for a given path.
///
/// # Errors
///
/// Returns an error if the path is not within a Git repository.
pub fn find_repository_root(path: &Path) -> Result<PathBuf> {
    let repository = Repository::discover(path).context("Not a git repository")?;

    let workdir = repository
        .workdir()
        .ok_or_else(|| anyhow!("Bare repository has no working directory"))?;

    Ok(workdir.to_path_buf())
}

fn find_jujutsu_repository_root(path: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("jj")
        .arg("--no-pager")
        .arg("--ignore-working-copy")
        .arg("root")
        .current_dir(path)
        .output()
        .context("Failed to execute jj root")?;

    if !output.status.success() {
        let error_message = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("jj root failed: {error_message}"));
    }

    let root = String::from_utf8(output.stdout).context("jj root returned non-UTF-8 output")?;
    let root = root.strip_suffix('\n').unwrap_or(&root);
    let root = root.strip_suffix('\r').unwrap_or(root);

    if root.is_empty() || root.contains('\n') || root.contains('\r') {
        return Err(anyhow!("jj root returned malformed repository root output"));
    }

    Ok(PathBuf::from(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use uuid::Uuid;

    struct TemporaryDirectory(PathBuf);

    impl TemporaryDirectory {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("chloe-{name}-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temporary directory");
            fs::write(path.join("jj-config.toml"), "").expect("write isolated jj config");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn run_jj_setup(arguments: &[&str], directory: &Path) {
        let config_path = directory
            .ancestors()
            .find(|ancestor| ancestor.join("jj-config.toml").exists())
            .expect("find isolated jj config")
            .join("jj-config.toml");
        let output = Command::new("jj")
            .arg("--no-pager")
            .arg("--config=user.name=Chloe Test")
            .arg("--config=user.email=chloe-test@example.com")
            .args(arguments)
            .env("JJ_CONFIG", config_path)
            .current_dir(directory)
            .output()
            .expect("run jj setup command");
        assert!(
            output.status.success(),
            "jj setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_jj_root(directory: &Path) -> std::process::Output {
        Command::new("jj")
            .arg("--no-pager")
            .arg("--ignore-working-copy")
            .arg("root")
            .current_dir(directory)
            .output()
            .expect("run jj root")
    }

    #[test]
    fn finds_git_root_from_nested_directory() {
        let temporary_directory = TemporaryDirectory::new("git-root");
        Repository::init(temporary_directory.path()).expect("initialize Git repository");
        let nested_directory = temporary_directory.path().join("nested");
        fs::create_dir_all(&nested_directory).expect("create nested directory");

        let root = find_repository_root_for_vcs(&nested_directory, &VcsCommand::Git)
            .expect("find Git root");

        assert_eq!(root, temporary_directory.path());
    }

    #[test]
    fn git_root_discovery_fails_outside_repository() {
        let temporary_directory = TemporaryDirectory::new("not-git");

        assert!(
            find_repository_root_for_vcs(temporary_directory.path(), &VcsCommand::Git).is_err()
        );
    }

    #[test]
    #[ignore = "requires jj; run explicitly with --ignored"]
    fn finds_root_in_non_colocated_jj_workspace_with_spaces() {
        let temporary_directory = TemporaryDirectory::new("jj non colocated");
        run_jj_setup(
            &["git", "init", "--no-colocate"],
            temporary_directory.path(),
        );
        run_jj_setup(
            &["workspace", "add", "--name", "second", "second workspace"],
            temporary_directory.path(),
        );
        let workspace_path = temporary_directory.path().join("second workspace");
        assert!(workspace_path.join(".jj").exists());
        assert!(!workspace_path.join(".git").exists());
        let nested_directory = workspace_path.join("nested");
        fs::create_dir_all(&nested_directory).expect("create nested directory");

        let root = find_repository_root_for_vcs(&nested_directory, &VcsCommand::Jujutsu)
            .expect("find jj workspace root");

        assert_eq!(root, workspace_path);
    }

    #[test]
    #[ignore = "requires jj; run explicitly with --ignored"]
    fn finds_colocated_jj_root_after_multiple_workspace_adds() {
        let temporary_directory = TemporaryDirectory::new("jj colocated");
        run_jj_setup(&["git", "init", "--colocate"], temporary_directory.path());
        let first_workspace = temporary_directory.path().join("first workspace");
        let second_workspace = temporary_directory.path().join("second workspace");
        run_jj_setup(
            &[
                "--config=git.colocate=true",
                "workspace",
                "add",
                "--name",
                "first",
                first_workspace.to_str().unwrap(),
            ],
            temporary_directory.path(),
        );
        let original_root_after_first_add =
            find_repository_root_for_vcs(temporary_directory.path(), &VcsCommand::Jujutsu)
                .expect("find original jj root after first workspace add");
        assert_eq!(original_root_after_first_add, temporary_directory.path());

        run_jj_setup(
            &[
                "--config=git.colocate=true",
                "workspace",
                "add",
                "--name",
                "second",
                second_workspace.to_str().unwrap(),
            ],
            temporary_directory.path(),
        );
        let original_root_after_second_add =
            find_repository_root_for_vcs(temporary_directory.path(), &VcsCommand::Jujutsu)
                .expect("find original jj root after second workspace add");
        assert_eq!(original_root_after_second_add, temporary_directory.path());

        let nested_directory = second_workspace.join("nested");
        fs::create_dir_all(&nested_directory).expect("create nested directory");

        let root = find_repository_root_for_vcs(&nested_directory, &VcsCommand::Jujutsu)
            .expect("find jj workspace root");

        assert_eq!(root, second_workspace);
        let git_config = fs::read_to_string(temporary_directory.path().join(".git/config"))
            .expect("read colocated Git config");
        assert!(git_config.contains("relativeWorktrees"));
    }

    #[test]
    #[ignore = "requires jj; run explicitly with --ignored"]
    fn jj_root_failure_includes_command_error_without_git_fallback() {
        let temporary_directory = TemporaryDirectory::new("not-jj");
        let command_output = run_jj_root(temporary_directory.path());
        assert!(!command_output.status.success());
        let stderr = String::from_utf8_lossy(&command_output.stderr);
        assert!(!stderr.trim().is_empty());

        let error = find_repository_root_for_vcs(temporary_directory.path(), &VcsCommand::Jujutsu)
            .expect_err("non-repository must fail through jj");

        let error_message = error.to_string();
        assert!(error_message.contains("jj root failed"));
        assert!(error_message.contains(stderr.trim()));
        assert!(!error_message.contains("Not a git repository"));
    }
}
