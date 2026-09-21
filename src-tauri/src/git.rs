use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use thiserror::Error;

use crate::domain::{Machine, MachineTransport, Repository};
use crate::terminal::run_machine_shell;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("could not start Git for {operation}: {source}")]
    Start {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("Git {operation} failed: {details}")]
    Failed {
        operation: &'static str,
        details: String,
    },
    #[error("checkout destination already exists: {path}")]
    DestinationExists { path: PathBuf },
    #[error("checkout destination is not empty: {path}")]
    DestinationNotEmpty { path: PathBuf },
    #[error("checkout path is not a directory: {path}")]
    InvalidCheckoutPath { path: PathBuf },
    #[error("Workset root is not a directory: {path}")]
    InvalidWorksetRoot { path: PathBuf },
    #[error("could not inspect Workset root {path}: {source}")]
    ReadWorksetRoot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no child Git repositories found in Workset root: {path}")]
    NoRepositories { path: PathBuf },
    #[error("Repository remote does not match the configured remote: expected {expected}, found {actual}")]
    RemoteMismatch { expected: String, actual: String },
    #[error("Repository has no configured remote matching {expected}")]
    RemoteNotConfigured { expected: String },
    #[error("configured remote base branch does not exist: {remote}/{branch}")]
    BaseBranchNotFound { remote: String, branch: String },
    #[error("target branch already exists: {branch}")]
    BranchAlreadyExists { branch: String },
    #[error("target branch is already attached to a Worktree: {branch} at {path}")]
    BranchAlreadyAttached { branch: String, path: PathBuf },
    #[error("target branch does not exist for reuse: {branch}")]
    BranchNotFound { branch: String },
    #[error("path is not a registered Git Worktree: {path}")]
    NotAWorktree { path: PathBuf },
    #[error("Git Worktree branch does not match: expected {expected}, found {actual}")]
    WorktreeBranchMismatch { expected: String, actual: String },
    #[error("Git Worktree is dirty and needs explicit confirmation: {path}")]
    DirtyWorktree { path: PathBuf },
}

#[derive(Debug, Clone)]
pub struct GitCli {
    executable: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepositoryState {
    pub name: String,
    pub remote_url: String,
    pub current_branch: String,
    pub is_dirty: bool,
    pub unpushed_commits: Vec<String>,
    pub unpushed_commits_unknown: bool,
    pub uncommitted_changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckoutInspection {
    pub remote_url: Option<String>,
    pub current_branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
}

impl GitCli {
    pub fn system() -> Self {
        Self::new(PathBuf::from("git"))
    }

    pub fn new(executable: PathBuf) -> Self {
        Self { executable }
    }

    pub fn checkout_repository(
        &self,
        repository: &Repository,
        destination: &Path,
        branch: &str,
        base_branch: Option<&str>,
    ) -> Result<(), GitError> {
        if destination.exists() {
            return Err(GitError::DestinationExists {
                path: destination.to_owned(),
            });
        }

        let destination_string = destination.to_string_lossy().into_owned();
        let mut clone_args = vec!["clone"];
        if let Some(base_branch) = base_branch {
            clone_args.extend(["--branch", base_branch]);
        }
        clone_args.extend([repository.remote_url.as_str(), destination_string.as_str()]);
        self.run("clone", &clone_args)?;

        let current_branch = self.run_with_output(
            "read the cloned branch",
            &[
                "-C",
                &destination.to_string_lossy(),
                "branch",
                "--show-current",
            ],
        )?;
        if current_branch.trim() != branch {
            self.run(
                "create the Workset branch",
                &[
                    "-C",
                    &destination.to_string_lossy(),
                    "switch",
                    "--create",
                    branch,
                ],
            )?;
        }
        Ok(())
    }

    pub fn inspect_checkout(&self, checkout_path: &Path) -> Result<CheckoutInspection, GitError> {
        if !checkout_path.is_dir() {
            return Err(GitError::InvalidCheckoutPath {
                path: checkout_path.to_owned(),
            });
        }
        let checkout = checkout_path.to_string_lossy().into_owned();
        self.run_with_output(
            "validate the checkout Git repository",
            &["-C", &checkout, "rev-parse", "--show-toplevel"],
        )?;
        let remote_url = self.read_optional_repository_remote(&checkout)?;
        let current_branch = self
            .run_optional_output(
                "read the checkout branch",
                &["-C", &checkout, "symbolic-ref", "--short", "HEAD"],
            )?
            .map(|branch| branch.trim().to_owned())
            .unwrap_or_else(|| "HEAD (detached)".into());
        let status = self.run_with_output(
            "read the checkout status",
            &[
                "-C",
                &checkout,
                "status",
                "--porcelain",
                "--untracked-files=all",
            ],
        )?;
        Ok(CheckoutInspection {
            remote_url,
            current_branch,
            is_dirty: !status.trim().is_empty(),
        })
    }

    pub fn inspect_checkout_on_machine(
        &self,
        machine: &Machine,
        checkout_path: &Path,
    ) -> Result<CheckoutInspection, GitError> {
        if matches!(machine.transport, MachineTransport::Local) {
            return self.inspect_checkout(checkout_path);
        }

        let checkout = shell_quote(&checkout_path.to_string_lossy());
        let run = |operation: &'static str, command: String| {
            run_machine_shell(machine, &command)
                .map_err(|details| GitError::Failed { operation, details })
        };
        run(
            "validate the remote checkout Git repository",
            format!("git -C {checkout} rev-parse --show-toplevel"),
        )?;
        let current_branch = run(
            "read the remote checkout branch",
            format!(
                "git -C {checkout} symbolic-ref --short HEAD 2>/dev/null || printf '%s' 'HEAD (detached)'"
            ),
        )?
        .trim()
        .to_owned();
        let status = run(
            "read the remote checkout status",
            format!("git -C {checkout} status --porcelain --untracked-files=all"),
        )?;
        Ok(CheckoutInspection {
            remote_url: None,
            current_branch,
            is_dirty: !status.trim().is_empty(),
        })
    }

    pub fn clone_repository(&self, remote_url: &str, destination: &Path) -> Result<(), GitError> {
        if destination.exists() {
            if !destination.is_dir() {
                return Err(GitError::DestinationExists {
                    path: destination.to_owned(),
                });
            }
            let mut entries =
                fs::read_dir(destination).map_err(|source| GitError::ReadWorksetRoot {
                    path: destination.to_owned(),
                    source,
                })?;
            if entries.next().is_some() {
                return Err(GitError::DestinationNotEmpty {
                    path: destination.to_owned(),
                });
            }
        }
        let remote_url = remote_url.trim();
        if remote_url.is_empty() {
            return Err(GitError::Failed {
                operation: "clone the repository",
                details: "remote URL cannot be blank".into(),
            });
        }
        let destination = destination.to_string_lossy().into_owned();
        self.run("clone the repository", &["clone", remote_url, &destination])
    }

    pub fn prepare_worktree(
        &self,
        repository: &Repository,
        canonical_checkout: &Path,
        destination: &Path,
        branch: &str,
        base_branch: &str,
        reuse_existing_branch: bool,
    ) -> Result<CheckoutInspection, GitError> {
        if branch.trim().is_empty() {
            return Err(GitError::Failed {
                operation: "prepare the Git Worktree",
                details: "target branch cannot be blank".into(),
            });
        }
        if base_branch.trim().is_empty() {
            return Err(GitError::Failed {
                operation: "prepare the Git Worktree",
                details: "base branch cannot be blank".into(),
            });
        }
        if destination.exists() {
            return Err(GitError::DestinationExists {
                path: destination.to_owned(),
            });
        }

        let remote = self.configured_remote(canonical_checkout, &repository.remote_url)?;
        let canonical = canonical_checkout.to_string_lossy().into_owned();
        self.run(
            "fetch the configured remote",
            &["-C", &canonical, "fetch", &remote],
        )?;

        let base_ref = format!("refs/remotes/{remote}/{base_branch}");
        if self
            .run_optional_output(
                "check the configured remote base branch",
                &["-C", &canonical, "rev-parse", "--verify", &base_ref],
            )?
            .is_none()
        {
            return Err(GitError::BaseBranchNotFound {
                remote,
                branch: base_branch.to_owned(),
            });
        }

        if let Some(entry) = self
            .list_worktrees(canonical_checkout)?
            .into_iter()
            .find(|entry| entry.branch.as_deref() == Some(branch))
        {
            return Err(GitError::BranchAlreadyAttached {
                branch: branch.to_owned(),
                path: entry.path,
            });
        }

        let local_ref = format!("refs/heads/{branch}");
        let remote_ref = format!("refs/remotes/{remote}/{branch}");
        let local_exists = self
            .run_optional_output(
                "check the target branch",
                &["-C", &canonical, "show-ref", "--verify", &local_ref],
            )?
            .is_some();
        let remote_exists = self
            .run_optional_output(
                "check the target remote branch",
                &["-C", &canonical, "show-ref", "--verify", &remote_ref],
            )?
            .is_some();

        let destination_string = destination.to_string_lossy().into_owned();
        if reuse_existing_branch {
            if local_exists {
                self.run(
                    "attach the existing target branch",
                    &[
                        "-C",
                        &canonical,
                        "worktree",
                        "add",
                        &destination_string,
                        branch,
                    ],
                )?;
            } else if remote_exists {
                self.run(
                    "attach the existing remote target branch",
                    &[
                        "-C",
                        &canonical,
                        "worktree",
                        "add",
                        "--track",
                        "-b",
                        branch,
                        &destination_string,
                        &remote_ref,
                    ],
                )?;
            } else {
                return Err(GitError::BranchNotFound {
                    branch: branch.to_owned(),
                });
            }
        } else {
            if local_exists || remote_exists {
                return Err(GitError::BranchAlreadyExists {
                    branch: branch.to_owned(),
                });
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|source| GitError::ReadWorksetRoot {
                    path: parent.to_owned(),
                    source,
                })?;
            }
            self.run(
                "create the target Git Worktree",
                &[
                    "-C",
                    &canonical,
                    "worktree",
                    "add",
                    "-b",
                    branch,
                    &destination_string,
                    &base_ref,
                ],
            )?;
            // Make the eventual same-name upstream explicit without contacting or mutating it.
            self.run(
                "configure the target branch upstream",
                &[
                    "-C",
                    &destination_string,
                    "config",
                    &format!("branch.{branch}.remote"),
                    &remote,
                ],
            )?;
            self.run(
                "configure the target branch merge name",
                &[
                    "-C",
                    &destination_string,
                    "config",
                    &format!("branch.{branch}.merge"),
                    &format!("refs/heads/{branch}"),
                ],
            )?;
        }

        self.validate_worktree_attachment(repository, canonical_checkout, destination, branch)
    }

    pub fn validate_worktree_attachment(
        &self,
        repository: &Repository,
        canonical_checkout: &Path,
        worktree_path: &Path,
        expected_branch: &str,
    ) -> Result<CheckoutInspection, GitError> {
        let canonical = canonical_checkout.to_string_lossy().into_owned();
        let canonical_worktree =
            fs::canonicalize(worktree_path).map_err(|source| GitError::ReadWorksetRoot {
                path: worktree_path.to_owned(),
                source,
            })?;
        let is_registered = self
            .list_worktrees(canonical_checkout)?
            .into_iter()
            .any(|entry| {
                fs::canonicalize(entry.path)
                    .map(|path| path == canonical_worktree && path != canonical_checkout)
                    .unwrap_or(false)
            });
        if !is_registered {
            return Err(GitError::NotAWorktree {
                path: worktree_path.to_owned(),
            });
        }

        let inspection = self.inspect_checkout(worktree_path)?;
        let actual_remote = inspection.remote_url.clone().unwrap_or_default();
        if actual_remote != repository.remote_url {
            return Err(GitError::RemoteMismatch {
                expected: repository.remote_url.clone(),
                actual: actual_remote,
            });
        }
        if inspection.current_branch != expected_branch {
            return Err(GitError::WorktreeBranchMismatch {
                expected: expected_branch.to_owned(),
                actual: inspection.current_branch,
            });
        }
        let _ = canonical;
        Ok(inspection)
    }

    pub fn list_worktrees(
        &self,
        canonical_checkout: &Path,
    ) -> Result<Vec<GitWorktreeEntry>, GitError> {
        let canonical = canonical_checkout.to_string_lossy().into_owned();
        let output = self.run_with_output(
            "list Git Worktrees",
            &["-C", &canonical, "worktree", "list", "--porcelain"],
        )?;
        let mut entries = Vec::new();
        let mut path = None;
        let mut branch = None;
        for line in output.lines().chain(std::iter::once("")) {
            if let Some(value) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(value));
            } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
                branch = Some(value.to_owned());
            } else if line.is_empty() {
                if let Some(path) = path.take() {
                    entries.push(GitWorktreeEntry {
                        path,
                        branch: branch.take(),
                    });
                }
            }
        }
        Ok(entries)
    }

    fn configured_remote(
        &self,
        canonical_checkout: &Path,
        expected_remote_url: &str,
    ) -> Result<String, GitError> {
        let canonical = canonical_checkout.to_string_lossy().into_owned();
        let remotes =
            self.run_with_output("list configured Git remotes", &["-C", &canonical, "remote"])?;
        for remote in remotes
            .lines()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
        {
            let actual = self.run_with_output(
                "read configured Git remote",
                &["-C", &canonical, "remote", "get-url", remote],
            )?;
            if actual.trim() == expected_remote_url {
                return Ok(remote.to_owned());
            }
        }
        if remotes
            .lines()
            .map(str::trim)
            .any(|remote| !remote.is_empty())
        {
            let actual = self
                .run_with_output(
                    "read configured Git remote",
                    &[
                        "-C",
                        &canonical,
                        "remote",
                        "get-url",
                        remotes.lines().next().unwrap(),
                    ],
                )?
                .trim()
                .to_owned();
            return Err(GitError::RemoteMismatch {
                expected: expected_remote_url.to_owned(),
                actual,
            });
        }
        Err(GitError::RemoteNotConfigured {
            expected: expected_remote_url.to_owned(),
        })
    }

    pub fn inspect_workset(&self, root: &Path) -> Result<Vec<GitRepositoryState>, GitError> {
        if !root.is_dir() {
            return Err(GitError::InvalidWorksetRoot {
                path: root.to_owned(),
            });
        }
        let canonical_root =
            fs::canonicalize(root).map_err(|source| GitError::ReadWorksetRoot {
                path: root.to_owned(),
                source,
            })?;
        let mut children = fs::read_dir(root)
            .map_err(|source| GitError::ReadWorksetRoot {
                path: root.to_owned(),
                source,
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| GitError::ReadWorksetRoot {
                path: root.to_owned(),
                source,
            })?;
        children.retain(|path| path.is_dir());
        children.sort();

        let mut repositories = Vec::new();
        for child in children {
            let child_string = child.to_string_lossy().into_owned();
            let repository_root = match self.run_optional_output(
                "find a child Git repository",
                &["-C", &child_string, "rev-parse", "--show-toplevel"],
            )? {
                Some(repository_root) => repository_root,
                None => continue,
            };
            let repository_root = Path::new(repository_root.trim());
            let canonical_child =
                fs::canonicalize(&child).map_err(|source| GitError::ReadWorksetRoot {
                    path: child.clone(),
                    source,
                })?;
            let canonical_repository_root =
                fs::canonicalize(repository_root).map_err(|source| GitError::ReadWorksetRoot {
                    path: repository_root.to_owned(),
                    source,
                })?;
            if canonical_repository_root != canonical_child
                || canonical_repository_root == canonical_root
            {
                continue;
            }

            let remote_url = self.read_repository_remote(&child_string)?;
            let current_branch = if let Some(branch) = self.run_optional_output(
                "read the child Git repository branch",
                &["-C", &child_string, "symbolic-ref", "--short", "HEAD"],
            )? {
                branch.trim().to_owned()
            } else {
                let commit = self.run_with_output(
                    "read the detached child Git repository commit",
                    &["-C", &child_string, "rev-parse", "--short", "HEAD"],
                )?;
                format!("HEAD (detached at {})", commit.trim())
            };
            let status = self.run_with_output(
                "read the child Git repository status",
                &[
                    "-C",
                    &child_string,
                    "status",
                    "--porcelain",
                    "--untracked-files=all",
                ],
            )?;
            let uncommitted_changes = status
                .lines()
                .map(str::trim_end)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect();
            let (unpushed_commits, unpushed_commits_unknown) =
                self.read_unpushed_commits(&child_string)?;
            let name = child
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| GitError::Failed {
                    operation: "read the child Git repository name",
                    details: format!("invalid repository directory name: {}", child.display()),
                })?
                .to_owned();
            repositories.push(GitRepositoryState {
                name,
                remote_url,
                current_branch,
                is_dirty: !status.trim().is_empty(),
                unpushed_commits,
                unpushed_commits_unknown,
                uncommitted_changes,
            });
        }

        if repositories.is_empty() {
            return Err(GitError::NoRepositories {
                path: root.to_owned(),
            });
        }
        Ok(repositories)
    }

    fn read_unpushed_commits(&self, repository: &str) -> Result<(Vec<String>, bool), GitError> {
        let upstream = self
            .run_optional_output(
                "read the child Git repository upstream",
                &[
                    "-C",
                    repository,
                    "rev-parse",
                    "--abbrev-ref",
                    "--symbolic-full-name",
                    "@{upstream}",
                ],
            )?
            .map(|upstream| upstream.trim().to_owned());
        let remote_default = self
            .run_optional_output(
                "read the child Git repository remote default branch",
                &[
                    "-C",
                    repository,
                    "symbolic-ref",
                    "--short",
                    "refs/remotes/origin/HEAD",
                ],
            )?
            .map(|branch| branch.trim().to_owned());
        let Some(range) = upstream
            .map(|branch| format!("{branch}..HEAD"))
            .or_else(|| remote_default.map(|branch| format!("{branch}..HEAD")))
        else {
            return Ok((Vec::new(), true));
        };
        let commits = self.run_with_output(
            "read unpushed child Git repository commits",
            &["-C", repository, "log", "--format=%s", range.as_str()],
        )?;
        Ok((
            commits
                .lines()
                .map(str::trim)
                .filter(|commit| !commit.is_empty())
                .map(str::to_owned)
                .collect(),
            false,
        ))
    }

    fn read_repository_remote(&self, child: &str) -> Result<String, GitError> {
        let remotes = self.run_with_output(
            "list child Git repository remotes",
            &["-C", child, "remote"],
        )?;
        let remote_name = remotes
            .lines()
            .map(str::trim)
            .find(|remote| !remote.is_empty());
        match remote_name {
            Some(remote_name) => self
                .run_with_output(
                    "read the child Git repository remote",
                    &["-C", child, "remote", "get-url", remote_name],
                )
                .map(|remote| remote.trim().to_owned()),
            None => Ok(child.to_owned()),
        }
    }

    fn read_optional_repository_remote(&self, checkout: &str) -> Result<Option<String>, GitError> {
        let remotes =
            self.run_with_output("list checkout Git remotes", &["-C", checkout, "remote"])?;
        let remote_name = remotes
            .lines()
            .map(str::trim)
            .find(|remote| !remote.is_empty());
        remote_name
            .map(|remote_name| {
                self.run_with_output(
                    "read the checkout Git remote",
                    &["-C", checkout, "remote", "get-url", remote_name],
                )
                .map(|remote| remote.trim().to_owned())
            })
            .transpose()
    }

    fn run(&self, operation: &'static str, args: &[&str]) -> Result<(), GitError> {
        self.run_with_output(operation, args).map(|_| ())
    }

    fn run_with_output(&self, operation: &'static str, args: &[&str]) -> Result<String, GitError> {
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .map_err(|source| GitError::Start { operation, source })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let details = if stderr.is_empty() { stdout } else { stderr };
            return Err(GitError::Failed {
                operation,
                details: if details.is_empty() {
                    format!("exit status {}", output.status)
                } else {
                    details
                },
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn run_optional_output(
        &self,
        operation: &'static str,
        args: &[&str],
    ) -> Result<Option<String>, GitError> {
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .map_err(|source| GitError::Start { operation, source })?;
        if output.status.success() {
            Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn checkout_creates_each_repository_under_its_directory_with_an_independent_branch() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "service-a\n").expect("seed file should be written");
        run_git(&seed, &["add", "README.md"]);
        run_git(&seed, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        run_git(&seed, &["remote", "add", "origin", &path_arg(&origin)]);
        run_git(&seed, &["push", "origin", "main"]);

        let repository = Repository {
            id: 1,
            project_id: 1,
            name: "service-a".into(),
            remote_url: origin.to_string_lossy().into_owned(),
            base_branch: "main".into(),
        };
        let root = directory.path().join("workset-root");
        fs::create_dir(&root).expect("Workset root should exist");
        let destination = root.join(&repository.name);

        GitCli::system()
            .checkout_repository(
                &repository,
                &destination,
                "feature/logical-name",
                Some("main"),
            )
            .expect("repository should be checked out");

        assert!(destination.join("README.md").is_file());
        assert_eq!(
            run_git_output(&destination, &["branch", "--show-current"]),
            "feature/logical-name"
        );
        assert_ne!(root.to_string_lossy(), "feature/logical-name");
    }

    #[test]
    fn inspect_reports_child_repository_state_without_changing_git() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let root = directory.path().join("workset");
        fs::create_dir(&root).expect("Workset root should exist");
        let repository_path = root.join("service-a");
        run_git(&root, &["init", "--initial-branch=main", "service-a"]);
        run_git(
            &repository_path,
            &["config", "user.email", "test@example.com"],
        );
        run_git(&repository_path, &["config", "user.name", "Test User"]);
        fs::write(repository_path.join("README.md"), "service-a\n")
            .expect("repository file should be written");
        run_git(&repository_path, &["add", "README.md"]);
        run_git(&repository_path, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        let origin_url = path_arg(&origin);
        run_git(&repository_path, &["remote", "add", "origin", &origin_url]);
        fs::write(repository_path.join("notes.txt"), "keep this work\n")
            .expect("uncommitted file should be written");
        fs::create_dir(root.join("not-a-repository")).expect("unrelated directory should exist");

        let branch_before = run_git_output(&repository_path, &["branch", "--show-current"]);
        let remote_before = run_git_output(&repository_path, &["remote", "get-url", "origin"]);
        let inspected = GitCli::system()
            .inspect_workset(&root)
            .expect("the Workset should be inspectable");

        assert_eq!(inspected.len(), 1);
        assert_eq!(inspected[0].name, "service-a");
        assert_eq!(inspected[0].remote_url, origin_url);
        assert_eq!(inspected[0].current_branch, "main");
        assert!(inspected[0].is_dirty);
        assert_eq!(
            run_git_output(&repository_path, &["branch", "--show-current"]),
            branch_before
        );
        assert_eq!(
            run_git_output(&repository_path, &["remote", "get-url", "origin"]),
            remote_before
        );
        assert!(repository_path.join("notes.txt").is_file());
    }

    #[test]
    fn inspect_checkout_reports_git_identity_branch_and_dirty_state() {
        let directory = tempdir().expect("temporary Git directory should exist");
        run_git(
            directory.path(),
            &["init", "--initial-branch=main", "checkout"],
        );
        let checkout = directory.path().join("checkout");
        run_git(&checkout, &["config", "user.email", "test@example.com"]);
        run_git(&checkout, &["config", "user.name", "Test User"]);
        fs::write(checkout.join("README.md"), "safe\n").expect("file should be written");
        run_git(&checkout, &["add", "README.md"]);
        run_git(&checkout, &["commit", "-m", "initial"]);
        let origin = directory.path().join("origin.git");
        run_git(directory.path(), &["init", "--bare", "origin.git"]);
        let origin_url = path_arg(&origin);
        run_git(&checkout, &["remote", "add", "origin", &origin_url]);
        fs::write(checkout.join("notes.txt"), "keep\n").expect("file should be written");

        let inspected = GitCli::system()
            .inspect_checkout(&checkout)
            .expect("checkout should be inspectable");
        assert_eq!(inspected.remote_url, Some(origin_url));
        assert_eq!(inspected.current_branch, "main");
        assert!(inspected.is_dirty);
    }

    #[test]
    fn clone_repository_accepts_an_empty_destination_and_rejects_non_empty_one() {
        let directory = tempdir().expect("temporary Git directory should exist");
        run_git(directory.path(), &["init", "--bare", "origin.git"]);
        let origin = directory.path().join("origin.git");
        let destination = directory.path().join("clone");
        fs::create_dir(&destination).expect("empty destination should exist");

        GitCli::system()
            .clone_repository(&path_arg(&origin), &destination)
            .expect("clone should accept an empty destination");
        assert!(destination.join(".git").is_dir());

        let error = GitCli::system()
            .clone_repository(&path_arg(&origin), &destination)
            .expect_err("clone should reject a non-empty destination");
        assert!(matches!(error, GitError::DestinationNotEmpty { .. }));
    }

    #[test]
    fn removal_report_lists_unpushed_commits_and_uncommitted_changes() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let root = directory.path().join("workset");
        fs::create_dir(&root).expect("Workset root should exist");
        let repository_path = root.join("service-a");
        run_git(&root, &["init", "--initial-branch=main", "service-a"]);
        run_git(
            &repository_path,
            &["config", "user.email", "test@example.com"],
        );
        run_git(&repository_path, &["config", "user.name", "Test User"]);
        fs::write(repository_path.join("README.md"), "service-a\n")
            .expect("repository file should be written");
        run_git(&repository_path, &["add", "README.md"]);
        run_git(&repository_path, &["commit", "-m", "initial"]);

        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        run_git(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        let origin_url = path_arg(&origin);
        run_git(&repository_path, &["remote", "add", "origin", &origin_url]);
        run_git(
            &repository_path,
            &["push", "--set-upstream", "origin", "main"],
        );
        fs::write(repository_path.join("README.md"), "local work\n")
            .expect("uncommitted change should be written");
        run_git(&repository_path, &["add", "README.md"]);
        run_git(&repository_path, &["commit", "-m", "local work"]);
        fs::write(repository_path.join("notes.txt"), "keep this\n")
            .expect("uncommitted file should be written");
        let report = GitCli::system()
            .inspect_workset(&root)
            .expect("the Workset should be safe to inspect");

        assert_eq!(report.len(), 1);
        assert_eq!(report[0].current_branch, "main");
        assert_eq!(report[0].unpushed_commits, vec!["local work"]);
        assert_eq!(report[0].uncommitted_changes, vec!["?? notes.txt"]);
    }

    #[test]
    fn inspect_accepts_a_child_repository_without_a_remote() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let root = directory.path().join("workset");
        fs::create_dir(&root).expect("Workset root should exist");
        let repository_path = root.join("local-only");
        run_git(&root, &["init", "--initial-branch=main", "local-only"]);
        run_git(
            &repository_path,
            &["config", "user.email", "test@example.com"],
        );
        run_git(&repository_path, &["config", "user.name", "Test User"]);
        fs::write(repository_path.join("README.md"), "local-only\n")
            .expect("repository file should be written");
        run_git(&repository_path, &["add", "README.md"]);
        run_git(&repository_path, &["commit", "-m", "initial"]);

        let inspected = GitCli::system()
            .inspect_workset(&root)
            .expect("a repository without a remote should be inspectable");

        assert_eq!(inspected.len(), 1);
        assert_eq!(inspected[0].name, "local-only");
        assert_eq!(inspected[0].remote_url, repository_path.to_string_lossy());
        assert_eq!(inspected[0].current_branch, "main");
        assert!(!inspected[0].is_dirty);
        assert!(inspected[0].unpushed_commits_unknown);
    }

    #[test]
    fn prepare_worktree_fetches_remote_and_leaves_canonical_checkout_on_its_branch() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let (canonical, repository) = remote_fixture(directory.path());
        let destination = directory
            .path()
            .join("worktrees")
            .join("workspace-42")
            .join("feature-fix")
            .join("service-a");

        let inspection = GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &destination,
                "feature/fix",
                "main",
                false,
            )
            .expect("a new Worktree should be created");

        assert_eq!(inspection.current_branch, "feature/fix");
        assert!(!inspection.is_dirty);
        assert_eq!(
            run_git_output(&canonical, &["branch", "--show-current"]),
            "main"
        );
        assert_eq!(
            run_git_output(
                &destination,
                &["config", "--get", "branch.feature/fix.merge"]
            ),
            "refs/heads/feature/fix"
        );
        assert_eq!(
            run_git_output(&canonical, &["worktree", "list", "--porcelain"])
                .lines()
                .filter(|line| line.starts_with("worktree "))
                .count(),
            2
        );
    }

    #[test]
    fn prepare_worktree_fails_when_fetch_fails() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let canonical = directory.path().join("canonical");
        run_git(
            directory.path(),
            &["init", "--initial-branch=main", "canonical"],
        );
        run_git(&canonical, &["config", "user.email", "test@example.com"]);
        run_git(&canonical, &["config", "user.name", "Test User"]);
        fs::write(canonical.join("README.md"), "canonical\n")
            .expect("canonical file should be written");
        run_git(&canonical, &["add", "README.md"]);
        run_git(&canonical, &["commit", "-m", "initial"]);
        let repository = Repository {
            id: 1,
            project_id: 1,
            name: "service-a".into(),
            remote_url: directory
                .path()
                .join("missing.git")
                .to_string_lossy()
                .into_owned(),
            base_branch: "main".into(),
        };
        run_git(
            &canonical,
            &["remote", "add", "origin", &repository.remote_url],
        );

        let error = GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &directory.path().join("worktree"),
                "feature/fix",
                "main",
                false,
            )
            .expect_err("a failed fetch must stop preparation");

        assert!(matches!(
            error,
            GitError::Failed {
                operation: "fetch the configured remote",
                ..
            }
        ));
        assert!(!directory.path().join("worktree").exists());
    }

    #[test]
    fn prepare_worktree_does_not_fall_back_when_configured_base_branch_is_missing() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let (canonical, repository) = remote_fixture(directory.path());

        let error = GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &directory.path().join("worktree"),
                "feature/fix",
                "develop",
                false,
            )
            .expect_err("a missing configured base must stop preparation");

        assert!(matches!(
            error,
            GitError::BaseBranchNotFound { remote, branch }
                if remote == "origin" && branch == "develop"
        ));
    }

    #[test]
    fn prepare_worktree_reports_an_already_attached_branch_instead_of_duplicating_it() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let (canonical, repository) = remote_fixture(directory.path());
        let first = directory.path().join("first");
        GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &first,
                "feature/fix",
                "main",
                false,
            )
            .expect("the first Worktree should be created");

        let error = GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &directory.path().join("second"),
                "feature/fix",
                "main",
                true,
            )
            .expect_err("a branch already attached to a Worktree must not be duplicated");

        assert!(
            matches!(
                error,
                GitError::BranchAlreadyAttached { ref branch, ref path }
                    if branch == "feature/fix"
                        && path == &fs::canonicalize(&first).expect("first Worktree should canonicalize")
            ),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn validate_worktree_attachment_checks_remote_branch_and_reports_dirty_state() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let (canonical, repository) = remote_fixture(directory.path());
        let destination = directory.path().join("attached");
        GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &destination,
                "feature/fix",
                "main",
                false,
            )
            .expect("the Worktree should be created");
        fs::write(destination.join("notes.txt"), "uncommitted\n")
            .expect("dirty file should be written");

        let inspection = GitCli::system()
            .validate_worktree_attachment(&repository, &canonical, &destination, "feature/fix")
            .expect("the existing Worktree should be validatable");

        assert!(inspection.is_dirty);
        assert_eq!(inspection.current_branch, "feature/fix");
    }

    fn remote_fixture(directory: &Path) -> (PathBuf, Repository) {
        let seed = directory.join("seed");
        run_git(directory, &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "seed\n").expect("seed file should be written");
        run_git(&seed, &["add", "README.md"]);
        run_git(&seed, &["commit", "-m", "initial"]);
        let origin = directory.join("origin.git");
        run_git(directory, &["init", "--bare", "origin.git"]);
        let origin_url = path_arg(&origin);
        run_git(&origin, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        run_git(&seed, &["remote", "add", "origin", &origin_url]);
        run_git(&seed, &["push", "origin", "main"]);
        let canonical = directory.join("canonical");
        run_git(directory, &["clone", &origin_url, &path_arg(&canonical)]);
        run_git(&canonical, &["config", "user.email", "test@example.com"]);
        run_git(&canonical, &["config", "user.name", "Test User"]);

        (
            canonical,
            Repository {
                id: 1,
                project_id: 1,
                name: "service-a".into(),
                remote_url: origin_url,
                base_branch: "main".into(),
            },
        )
    }

    fn run_git(directory: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .expect("Git should start");
        assert!(
            output.status.success(),
            "Git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_git_output(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(directory)
            .args(args)
            .output()
            .expect("Git should start");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn path_arg(path: &Path) -> String {
        PathBuf::from(path).to_string_lossy().into_owned()
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
