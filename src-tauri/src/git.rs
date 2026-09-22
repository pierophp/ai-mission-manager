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
    #[error("could not inspect directory {path}: {source}")]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
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
        self.inspect_remote_checkout(machine, checkout_path)
    }

    pub fn clone_repository(&self, remote_url: &str, destination: &Path) -> Result<(), GitError> {
        if destination.exists() {
            if !destination.is_dir() {
                return Err(GitError::DestinationExists {
                    path: destination.to_owned(),
                });
            }
            let mut entries =
                fs::read_dir(destination).map_err(|source| GitError::ReadDirectory {
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

    pub fn prepare_worktree_on_machine(
        &self,
        machine: &Machine,
        repository: &Repository,
        canonical_checkout: &Path,
        destination: &Path,
        branch: &str,
        base_branch: &str,
        reuse_existing_branch: bool,
        confirm_dirty_attachment: bool,
    ) -> Result<CheckoutInspection, GitError> {
        if matches!(machine.transport, MachineTransport::Local) {
            return self.prepare_worktree(
                repository,
                canonical_checkout,
                destination,
                branch,
                base_branch,
                reuse_existing_branch,
                confirm_dirty_attachment,
            );
        }

        if branch.trim().is_empty() || base_branch.trim().is_empty() {
            return Err(GitError::Failed {
                operation: "prepare the Git Worktree",
                details: "target and base branches cannot be blank".into(),
            });
        }
        if self.remote_path_exists(machine, destination)? {
            return Err(GitError::DestinationExists {
                path: destination.to_owned(),
            });
        }
        let remote =
            self.configured_remote_on_machine(machine, canonical_checkout, &repository.remote_url)?;
        self.remote_git(
            machine,
            "fetch the configured remote",
            canonical_checkout,
            &format!("fetch {}", shell_quote(&remote)),
        )?;
        let base_ref = format!("refs/remotes/{remote}/{base_branch}");
        if self
            .remote_git_optional(
                machine,
                "check the configured remote base branch",
                canonical_checkout,
                &format!("rev-parse --verify {}", shell_quote(&base_ref)),
            )?
            .is_none()
        {
            return Err(GitError::BaseBranchNotFound {
                remote,
                branch: base_branch.into(),
            });
        }
        if let Some(entry) = self
            .list_worktrees_on_machine(machine, canonical_checkout)?
            .into_iter()
            .find(|entry| entry.branch.as_deref() == Some(branch))
        {
            return Err(GitError::BranchAlreadyAttached {
                branch: branch.into(),
                path: entry.path,
            });
        }
        let local_ref = format!("refs/heads/{branch}");
        let remote_ref = format!("refs/remotes/{remote}/{branch}");
        let local_exists = self
            .remote_git_optional(
                machine,
                "check the target branch",
                canonical_checkout,
                &format!("show-ref --verify {}", shell_quote(&local_ref)),
            )?
            .is_some();
        let remote_exists = self
            .remote_git_optional(
                machine,
                "check the target remote branch",
                canonical_checkout,
                &format!("show-ref --verify {}", shell_quote(&remote_ref)),
            )?
            .is_some();
        if let Some(parent) = destination.parent() {
            self.remote_command(
                machine,
                "create the Worktree parent directory",
                &format!("mkdir -p {}", machine_path_arg(parent)),
            )?;
        }
        if reuse_existing_branch {
            if local_exists {
                self.remote_git_with_path(
                    machine,
                    "attach the existing target branch",
                    canonical_checkout,
                    &format!(
                        "worktree add {} {}",
                        machine_path_arg(destination),
                        shell_quote(branch)
                    ),
                )?;
            } else if remote_exists {
                self.remote_git_with_path(
                    machine,
                    "attach the existing remote target branch",
                    canonical_checkout,
                    &format!(
                        "worktree add --track -b {} {} {}",
                        shell_quote(branch),
                        machine_path_arg(destination),
                        shell_quote(&remote_ref)
                    ),
                )?;
            } else {
                return Err(GitError::BranchNotFound {
                    branch: branch.into(),
                });
            }
        } else {
            if local_exists || remote_exists {
                return Err(GitError::BranchAlreadyExists {
                    branch: branch.into(),
                });
            }
            self.remote_git_with_path(
                machine,
                "create the target Git Worktree",
                canonical_checkout,
                &format!(
                    "worktree add -b {} {} {}",
                    shell_quote(branch),
                    machine_path_arg(destination),
                    shell_quote(&base_ref)
                ),
            )?;
            self.remote_git_with_path(
                machine,
                "configure the target branch upstream",
                destination,
                &format!(
                    "config {} {}",
                    shell_quote(&format!("branch.{branch}.remote")),
                    shell_quote(&remote)
                ),
            )?;
            self.remote_git_with_path(
                machine,
                "configure the target branch merge name",
                destination,
                &format!(
                    "config {} {}",
                    shell_quote(&format!("branch.{branch}.merge")),
                    shell_quote(&format!("refs/heads/{branch}"))
                ),
            )?;
        }
        self.validate_worktree_attachment_on_machine(
            machine,
            repository,
            canonical_checkout,
            destination,
            branch,
            confirm_dirty_attachment,
        )
    }

    pub fn validate_worktree_attachment_on_machine(
        &self,
        machine: &Machine,
        repository: &Repository,
        canonical_checkout: &Path,
        worktree_path: &Path,
        expected_branch: &str,
        confirm_dirty_attachment: bool,
    ) -> Result<CheckoutInspection, GitError> {
        if matches!(machine.transport, MachineTransport::Local) {
            return self.validate_worktree_attachment(
                repository,
                canonical_checkout,
                worktree_path,
                expected_branch,
                confirm_dirty_attachment,
            );
        }
        let requested_path = self.remote_canonical_path(machine, worktree_path)?;
        let canonical_path = self.remote_canonical_path(machine, canonical_checkout)?;
        let registered = self
            .list_worktrees_on_machine(machine, canonical_checkout)?
            .into_iter()
            .any(|entry| {
                self.remote_canonical_path(machine, &entry.path)
                    .map(|path| path == requested_path && path != canonical_path)
                    .unwrap_or(false)
            });
        if !registered {
            return Err(GitError::NotAWorktree {
                path: worktree_path.to_owned(),
            });
        }
        let inspection = self.inspect_remote_checkout(machine, worktree_path)?;
        if inspection.remote_url.as_deref() != Some(repository.remote_url.as_str()) {
            return Err(GitError::RemoteMismatch {
                expected: repository.remote_url.clone(),
                actual: inspection.remote_url.unwrap_or_default(),
            });
        }
        if inspection.current_branch != expected_branch {
            return Err(GitError::WorktreeBranchMismatch {
                expected: expected_branch.into(),
                actual: inspection.current_branch,
            });
        }
        if inspection.is_dirty && !confirm_dirty_attachment {
            return Err(GitError::DirtyWorktree {
                path: worktree_path.to_owned(),
            });
        }
        Ok(inspection)
    }

    pub fn remove_worktree_on_machine(
        &self,
        machine: &Machine,
        canonical_checkout: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(), GitError> {
        if matches!(machine.transport, MachineTransport::Local) {
            return self.remove_worktree(canonical_checkout, worktree_path, force);
        }

        let requested_path = self.remote_canonical_path(machine, worktree_path)?;
        let canonical_path = self.remote_canonical_path(machine, canonical_checkout)?;
        let registered = self
            .list_worktrees_on_machine(machine, canonical_checkout)?
            .into_iter()
            .any(|entry| {
                self.remote_canonical_path(machine, &entry.path)
                    .map(|path| path == requested_path && path != canonical_path)
                    .unwrap_or(false)
            });
        if !registered {
            return Err(GitError::NotAWorktree {
                path: worktree_path.to_owned(),
            });
        }
        let force_flag = if force { "--force " } else { "" };
        self.remote_git(
            machine,
            "remove the Git Worktree",
            canonical_checkout,
            &format!(
                "worktree remove {force_flag}{}",
                machine_path_arg(worktree_path)
            ),
        )
        .map(|_| ())
    }

    pub fn remove_worktree(
        &self,
        canonical_checkout: &Path,
        worktree_path: &Path,
        force: bool,
    ) -> Result<(), GitError> {
        let canonical_worktree =
            fs::canonicalize(worktree_path).map_err(|source| GitError::ReadDirectory {
                path: worktree_path.to_owned(),
                source,
            })?;
        let canonical_checkout =
            fs::canonicalize(canonical_checkout).map_err(|source| GitError::ReadDirectory {
                path: canonical_checkout.to_owned(),
                source,
            })?;
        let registered = self
            .list_worktrees(&canonical_checkout)?
            .into_iter()
            .any(|entry| {
                fs::canonicalize(entry.path)
                    .map(|path| path == canonical_worktree && path != canonical_checkout)
                    .unwrap_or(false)
            });
        if !registered {
            return Err(GitError::NotAWorktree {
                path: worktree_path.to_owned(),
            });
        }

        let canonical = canonical_checkout.to_string_lossy().into_owned();
        let destination = worktree_path.to_string_lossy().into_owned();
        let mut arguments = vec!["-C", canonical.as_str(), "worktree", "remove"];
        if force {
            arguments.push("--force");
        }
        arguments.push(destination.as_str());
        self.run("remove the Git Worktree", &arguments)
    }

    fn inspect_remote_checkout(
        &self,
        machine: &Machine,
        checkout_path: &Path,
    ) -> Result<CheckoutInspection, GitError> {
        self.remote_git(
            machine,
            "validate the remote checkout Git repository",
            checkout_path,
            "rev-parse --show-toplevel",
        )?;
        let remote_url = self.remote_repository_url(machine, checkout_path)?;
        let current_branch = self
            .remote_git_optional(
                machine,
                "read the remote checkout branch",
                checkout_path,
                "symbolic-ref --short HEAD",
            )?
            .map(|branch| branch.trim().to_owned())
            .unwrap_or_else(|| "HEAD (detached)".into());
        let status = self.remote_git(
            machine,
            "read the remote checkout status",
            checkout_path,
            "status --porcelain --untracked-files=all",
        )?;
        Ok(CheckoutInspection {
            remote_url,
            current_branch,
            is_dirty: !status.trim().is_empty(),
        })
    }

    fn remote_repository_url(
        &self,
        machine: &Machine,
        checkout_path: &Path,
    ) -> Result<Option<String>, GitError> {
        let remotes = self.remote_git(
            machine,
            "list remote checkout remotes",
            checkout_path,
            "remote",
        )?;
        remotes
            .lines()
            .map(str::trim)
            .find(|remote| !remote.is_empty())
            .map(|remote| {
                self.remote_git(
                    machine,
                    "read remote checkout URL",
                    checkout_path,
                    &format!("remote get-url {}", shell_quote(remote)),
                )
                .map(|url| url.trim().to_owned())
            })
            .transpose()
    }

    fn configured_remote_on_machine(
        &self,
        machine: &Machine,
        checkout_path: &Path,
        expected: &str,
    ) -> Result<String, GitError> {
        let remotes = self.remote_git(
            machine,
            "list configured Git remotes",
            checkout_path,
            "remote",
        )?;
        for remote in remotes
            .lines()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
        {
            if self
                .remote_git(
                    machine,
                    "read configured Git remote",
                    checkout_path,
                    &format!("remote get-url {}", shell_quote(remote)),
                )?
                .trim()
                == expected
            {
                return Ok(remote.to_owned());
            }
        }
        if let Some(remote) = remotes
            .lines()
            .map(str::trim)
            .find(|remote| !remote.is_empty())
        {
            let actual = self
                .remote_git(
                    machine,
                    "read configured Git remote",
                    checkout_path,
                    &format!("remote get-url {}", shell_quote(remote)),
                )?
                .trim()
                .to_owned();
            return Err(GitError::RemoteMismatch {
                expected: expected.into(),
                actual,
            });
        }
        Err(GitError::RemoteNotConfigured {
            expected: expected.into(),
        })
    }

    fn list_worktrees_on_machine(
        &self,
        machine: &Machine,
        canonical_checkout: &Path,
    ) -> Result<Vec<GitWorktreeEntry>, GitError> {
        let output = self.remote_git(
            machine,
            "list Git Worktrees",
            canonical_checkout,
            "worktree list --porcelain",
        )?;
        Ok(parse_worktree_list(&output))
    }

    fn remote_path_exists(&self, machine: &Machine, path: &Path) -> Result<bool, GitError> {
        Ok(run_machine_shell(machine, &format!("test -e {}", machine_path_arg(path))).is_ok())
    }

    fn remote_canonical_path(&self, machine: &Machine, path: &Path) -> Result<PathBuf, GitError> {
        let output =
            run_machine_shell(machine, &format!("cd {} && pwd -P", machine_path_arg(path)))
                .map_err(|details| GitError::Failed {
                    operation: "canonicalize a remote Worktree path",
                    details,
                })?;
        Ok(PathBuf::from(output.trim()))
    }

    fn remote_command(
        &self,
        machine: &Machine,
        operation: &'static str,
        command: &str,
    ) -> Result<String, GitError> {
        run_machine_shell(machine, command)
            .map_err(|details| GitError::Failed { operation, details })
    }

    fn remote_git(
        &self,
        machine: &Machine,
        operation: &'static str,
        checkout_path: &Path,
        arguments: &str,
    ) -> Result<String, GitError> {
        self.remote_command(
            machine,
            operation,
            &format!("git -C {} {}", machine_path_arg(checkout_path), arguments),
        )
    }

    fn remote_git_with_path(
        &self,
        machine: &Machine,
        operation: &'static str,
        checkout_path: &Path,
        arguments: &str,
    ) -> Result<String, GitError> {
        self.remote_git(machine, operation, checkout_path, arguments)
    }

    fn remote_git_optional(
        &self,
        machine: &Machine,
        operation: &'static str,
        checkout_path: &Path,
        arguments: &str,
    ) -> Result<Option<String>, GitError> {
        let _ = operation;
        Ok(run_machine_shell(
            machine,
            &format!("git -C {} {}", machine_path_arg(checkout_path), arguments),
        )
        .ok())
    }

    pub fn prepare_worktree(
        &self,
        repository: &Repository,
        canonical_checkout: &Path,
        destination: &Path,
        branch: &str,
        base_branch: &str,
        reuse_existing_branch: bool,
        confirm_dirty_attachment: bool,
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
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|source| GitError::ReadDirectory {
                path: parent.to_owned(),
                source,
            })?;
        }
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

        self.validate_worktree_attachment(
            repository,
            canonical_checkout,
            destination,
            branch,
            confirm_dirty_attachment,
        )
    }

    pub fn validate_worktree_attachment(
        &self,
        repository: &Repository,
        canonical_checkout: &Path,
        worktree_path: &Path,
        expected_branch: &str,
        confirm_dirty_attachment: bool,
    ) -> Result<CheckoutInspection, GitError> {
        let canonical_worktree =
            fs::canonicalize(worktree_path).map_err(|source| GitError::ReadDirectory {
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
        if inspection.is_dirty && !confirm_dirty_attachment {
            return Err(GitError::DirtyWorktree {
                path: worktree_path.to_owned(),
            });
        }
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
                false,
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
                false,
            )
            .expect("the Worktree should be created");
        fs::write(destination.join("notes.txt"), "uncommitted\n")
            .expect("dirty file should be written");

        let error = GitCli::system()
            .validate_worktree_attachment(
                &repository,
                &canonical,
                &destination,
                "feature/fix",
                false,
            )
            .expect_err("dirty attachment should need confirmation");
        assert!(matches!(error, GitError::DirtyWorktree { path } if path == destination));

        let inspection = GitCli::system()
            .validate_worktree_attachment(
                &repository,
                &canonical,
                &destination,
                "feature/fix",
                true,
            )
            .expect("the existing Worktree should be validatable");

        assert!(inspection.is_dirty);
        assert_eq!(inspection.current_branch, "feature/fix");
    }

    #[test]
    fn removing_a_worktree_requires_force_when_dirty_and_never_deletes_its_branch() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let (canonical, repository) = remote_fixture(directory.path());
        let clean_destination = directory.path().join("clean");
        GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &clean_destination,
                "feature/clean-removal",
                "main",
                false,
                false,
            )
            .expect("the clean Worktree should be created");
        GitCli::system()
            .remove_worktree(&canonical, &clean_destination, false)
            .expect("an explicit clean removal should succeed");
        assert!(!clean_destination.exists());
        assert_eq!(
            run_git_output(
                &canonical,
                &["show-ref", "--verify", "refs/heads/feature/clean-removal"]
            ),
            format!(
                "{} refs/heads/feature/clean-removal",
                run_git_output(&canonical, &["rev-parse", "feature/clean-removal"])
            )
        );

        let dirty_destination = directory.path().join("dirty");
        GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &dirty_destination,
                "feature/dirty-removal",
                "main",
                false,
                false,
            )
            .expect("the dirty Worktree should be created");
        fs::write(
            dirty_destination.join("notes.txt"),
            "keep this until confirmation\n",
        )
        .expect("the Worktree should become dirty");
        let error = GitCli::system()
            .remove_worktree(&canonical, &dirty_destination, false)
            .expect_err("dirty cleanup must require destructive confirmation");
        assert!(matches!(
            error,
            GitError::Failed {
                operation: "remove the Git Worktree",
                ..
            }
        ));
        GitCli::system()
            .remove_worktree(&canonical, &dirty_destination, true)
            .expect("destructive confirmation should allow dirty cleanup");
        assert!(!dirty_destination.exists());
        assert!(run_git_output(
            &canonical,
            &["show-ref", "--verify", "refs/heads/feature/dirty-removal"]
        )
        .contains("refs/heads/feature/dirty-removal"));
    }

    #[test]
    fn prepare_worktree_reuses_existing_local_and_remote_branches_only_when_requested() {
        let directory = tempdir().expect("temporary Git directory should exist");
        let (canonical, repository) = remote_fixture(directory.path());
        run_git(&canonical, &["branch", "feature/local"]);
        let local_destination = directory.path().join("local");
        GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &local_destination,
                "feature/local",
                "main",
                true,
                false,
            )
            .expect("an existing local branch should be attachable when requested");
        assert_eq!(
            run_git_output(&local_destination, &["branch", "--show-current"]),
            "feature/local"
        );

        let seed = directory.path().join("seed");
        run_git(&seed, &["switch", "-c", "feature/remote"]);
        run_git(&seed, &["push", "origin", "feature/remote"]);
        let remote_destination = directory.path().join("remote");
        GitCli::system()
            .prepare_worktree(
                &repository,
                &canonical,
                &remote_destination,
                "feature/remote",
                "main",
                true,
                false,
            )
            .expect("an existing remote branch should be attachable when requested");
        assert_eq!(
            run_git_output(&remote_destination, &["branch", "--show-current"]),
            "feature/remote"
        );
        assert_eq!(
            run_git_output(
                &remote_destination,
                &["config", "--get", "branch.feature/remote.remote"]
            ),
            "origin"
        );
    }

    #[test]
    fn attachment_validation_reports_remote_branch_and_path_mismatches() {
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
                false,
            )
            .expect("the Worktree should be created");

        let branch_error = GitCli::system()
            .validate_worktree_attachment(
                &repository,
                &canonical,
                &destination,
                "feature/other",
                false,
            )
            .expect_err("a branch mismatch should be reported");
        assert!(matches!(
            branch_error,
            GitError::WorktreeBranchMismatch { expected, actual }
                if expected == "feature/other" && actual == "feature/fix"
        ));

        let wrong_remote = Repository {
            remote_url: directory
                .path()
                .join("another.git")
                .to_string_lossy()
                .into_owned(),
            ..repository.clone()
        };
        let remote_error = GitCli::system()
            .validate_worktree_attachment(
                &wrong_remote,
                &canonical,
                &destination,
                "feature/fix",
                false,
            )
            .expect_err("a remote mismatch should be reported");
        assert!(matches!(remote_error, GitError::RemoteMismatch { .. }));

        let wrong_path = directory.path().join("not-attached");
        run_git(
            directory.path(),
            &["clone", &repository.remote_url, &path_arg(&wrong_path)],
        );
        let path_error = GitCli::system()
            .validate_worktree_attachment(&repository, &canonical, &wrong_path, "main", false)
            .expect_err("an unregistered path should be reported");
        assert!(matches!(path_error, GitError::NotAWorktree { path } if path == wrong_path));
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

fn machine_path_arg(path: &Path) -> String {
    shell_quote(&path.to_string_lossy())
}

fn parse_worktree_list(output: &str) -> Vec<GitWorktreeEntry> {
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
    entries
}
