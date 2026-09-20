use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use thiserror::Error;

use crate::domain::Repository;

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
