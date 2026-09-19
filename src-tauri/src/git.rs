use std::{
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
}

#[derive(Debug, Clone)]
pub struct GitCli {
    executable: PathBuf,
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
