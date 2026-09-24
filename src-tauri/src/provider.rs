use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use thiserror::Error;

use crate::dependencies::resolve_executable;
use crate::domain::{
    ExternalMetadata, ExternalObjectInput, ExternalObjectKind, ExternalProvider,
    ExternalSnapshotData,
};

const GH_JSON_FIELDS: &str = "number,title,state,author,labels,milestone,updatedAt";

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("the external URL cannot be blank")]
    EmptyUrl,
    #[error("GitHub CLI is not available at an executable absolute path")]
    GhNotFound,
    #[error("GitHub CLI failed: {message}")]
    GhFailed { message: String },
    #[error("GitHub returned invalid JSON: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("could not run GitHub CLI: {0}")]
    Io(#[from] std::io::Error),
}

pub fn classify_url(raw_url: &str) -> Result<ExternalObjectInput, ProviderError> {
    let raw_url = raw_url.trim();
    if raw_url.is_empty() {
        return Err(ProviderError::EmptyUrl);
    }

    let without_fragment = raw_url.split('#').next().unwrap_or(raw_url);
    let normalized_url = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment)
        .trim_end_matches('/');
    let parts = normalized_url.split('/').collect::<Vec<_>>();
    let github_parts = if parts.len() == 7
        && parts[0] == "https:"
        && (parts[2].eq_ignore_ascii_case("github.com")
            || parts[2].eq_ignore_ascii_case("www.github.com"))
        && !parts[3].is_empty()
        && !parts[4].is_empty()
    {
        match (parts[5], parts[6].parse::<u64>()) {
            ("issues", Ok(number)) => Some((ExternalObjectKind::Issue, number)),
            ("pull", Ok(number)) => Some((ExternalObjectKind::PullRequest, number)),
            _ => None,
        }
    } else {
        None
    };

    if let Some((kind, number)) = github_parts {
        let owner = parts[3].to_ascii_lowercase();
        let repository = parts[4].to_ascii_lowercase();
        let kind_key = match kind {
            ExternalObjectKind::Issue => "issue",
            ExternalObjectKind::PullRequest => "pull",
            ExternalObjectKind::Generic => "generic",
        };
        let path_kind = match kind {
            ExternalObjectKind::Issue => "issues",
            ExternalObjectKind::PullRequest => "pull",
            ExternalObjectKind::Generic => unreachable!(),
        };
        return Ok(ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind,
            external_key: format!("{kind_key}:{owner}/{repository}#{number}"),
            canonical_url: format!("https://github.com/{owner}/{repository}/{path_kind}/{number}"),
        });
    }

    Ok(ExternalObjectInput {
        provider: ExternalProvider::Generic,
        kind: ExternalObjectKind::Generic,
        external_key: raw_url.to_owned(),
        canonical_url: raw_url.to_owned(),
    })
}

pub(crate) fn github_repository_name(remote_url: &str) -> Option<String> {
    let remote_url = remote_url.trim();
    let path = if let Some(rest) = remote_url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        if !host.eq_ignore_ascii_case("github.com") {
            return None;
        }
        path
    } else if let Some(rest) = remote_url.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        if !authority.eq_ignore_ascii_case("git@github.com") {
            return None;
        }
        path
    } else {
        let (scheme, rest) = remote_url.split_once("://")?;
        if !matches!(scheme, "https" | "http" | "git") {
            return None;
        }
        let (host, path) = rest.split_once('/')?;
        if !host.eq_ignore_ascii_case("github.com") && !host.eq_ignore_ascii_case("www.github.com")
        {
            return None;
        }
        path
    };

    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = path.split('/');
    let owner = segments.next()?.trim();
    let repository = segments.next()?.trim();
    if owner.is_empty()
        || repository.is_empty()
        || segments.next().is_some()
        || owner.len() > 39
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !owner.as_bytes().first()?.is_ascii_alphanumeric()
        || !owner.as_bytes().last()?.is_ascii_alphanumeric()
        || repository.len() > 100
        || !repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || !repository.as_bytes().first()?.is_ascii_alphanumeric()
    {
        return None;
    }

    Some(format!(
        "{}/{}",
        owner.to_ascii_lowercase(),
        repository.to_ascii_lowercase()
    ))
}

pub struct GithubCli {
    executable: PathBuf,
}

impl GithubCli {
    pub fn new(executable: PathBuf) -> Self {
        Self { executable }
    }

    pub fn create_issue(
        &self,
        repository: &str,
        title: &str,
        body: &str,
    ) -> Result<String, ProviderError> {
        if repository.trim().is_empty() {
            return Err(ProviderError::GhFailed {
                message: "a GitHub repository is required to create an Issue".into(),
            });
        }
        if title.trim().is_empty() {
            return Err(ProviderError::GhFailed {
                message: "a GitHub Issue title cannot be blank".into(),
            });
        }

        let endpoint = format!("repos/{repository}/issues");
        let title_field = format!("title={title}");
        let body_field = format!("body={body}");
        let output = Command::new(&self.executable)
            .args([
                "api",
                endpoint.as_str(),
                "--method",
                "POST",
                "--raw-field",
                title_field.as_str(),
                "--raw-field",
                body_field.as_str(),
            ])
            .output()?;
        ensure_success(&output)?;

        let response = serde_json::from_slice::<GithubCreatedIssue>(&output.stdout)?;
        let object = classify_url(&response.html_url)?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue {
            return Err(ProviderError::GhFailed {
                message: "GitHub API returned a URL that is not a GitHub Issue".into(),
            });
        }
        Ok(object.canonical_url)
    }

    pub fn add_comment(&self, issue_url: &str, body: &str) -> Result<(), ProviderError> {
        if body.trim().is_empty() {
            return Err(ProviderError::GhFailed {
                message: "a GitHub comment cannot be blank".into(),
            });
        }

        let output = Command::new(&self.executable)
            .args(["issue", "comment", issue_url, "--body", body])
            .output()?;
        ensure_success(&output)
    }

    pub fn fetch(
        &self,
        object: &ExternalObjectInput,
        fetched_at: i64,
    ) -> Result<ExternalSnapshotData, ProviderError> {
        let command = match object.kind {
            ExternalObjectKind::Issue => "issue",
            ExternalObjectKind::PullRequest => "pr",
            ExternalObjectKind::Generic => {
                return Err(ProviderError::GhFailed {
                    message: "generic links do not have a GitHub snapshot".into(),
                })
            }
        };
        let output = Command::new(&self.executable)
            .args([
                command,
                "view",
                object.canonical_url.as_str(),
                "--json",
                GH_JSON_FIELDS,
            ])
            .output()?;
        ensure_success(&output)?;

        let response = serde_json::from_slice::<GithubResponse>(&output.stdout)?;
        Ok(response.into_snapshot(fetched_at))
    }
}

fn ensure_success(output: &std::process::Output) -> Result<(), ProviderError> {
    if output.status.success() {
        return Ok(());
    }

    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(ProviderError::GhFailed {
        message: if message.is_empty() {
            "the command returned a non-zero exit status".into()
        } else {
            message
        },
    })
}

pub fn resolve_gh_executable(stored_path: Option<&Path>) -> Result<PathBuf, ProviderError> {
    resolve_executable("gh", stored_path).ok_or(ProviderError::GhNotFound)
}

#[derive(Debug, Deserialize)]
struct GithubResponse {
    title: String,
    state: String,
    author: Option<GithubActor>,
    #[serde(default)]
    labels: Vec<GithubLabel>,
    milestone: Option<GithubMilestone>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    number: u64,
}

#[derive(Debug, Deserialize)]
struct GithubCreatedIssue {
    #[serde(rename = "html_url")]
    html_url: String,
}

impl GithubResponse {
    fn into_snapshot(self, fetched_at: i64) -> ExternalSnapshotData {
        let mut metadata = vec![ExternalMetadata {
            key: "number".into(),
            value: self.number.to_string(),
        }];
        if let Some(author) = self.author {
            metadata.push(ExternalMetadata {
                key: "author".into(),
                value: author.login,
            });
        }
        if !self.labels.is_empty() {
            metadata.push(ExternalMetadata {
                key: "labels".into(),
                value: self
                    .labels
                    .into_iter()
                    .map(|label| label.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        }
        if let Some(milestone) = self.milestone {
            metadata.push(ExternalMetadata {
                key: "milestone".into(),
                value: milestone.title,
            });
        }
        if let Some(updated_at) = self.updated_at {
            metadata.push(ExternalMetadata {
                key: "updated".into(),
                value: updated_at,
            });
        }
        ExternalSnapshotData {
            title: self.title,
            state: self.state,
            metadata,
            fetched_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubActor {
    login: String,
}

#[derive(Debug, Deserialize)]
struct GithubLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GithubMilestone {
    title: String,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn github_issue_and_pull_request_urls_have_stable_object_identity() {
        let issue = classify_url("https://github.com/Acme/App/issues/7?foo=bar")
            .expect("GitHub issue URL should classify");
        let pull_request = classify_url("https://www.github.com/acme/app/pull/7/")
            .expect("GitHub pull request URL should classify");

        assert_eq!(issue.provider, ExternalProvider::GitHub);
        assert_eq!(issue.kind, ExternalObjectKind::Issue);
        assert_eq!(issue.external_key, "issue:acme/app#7");
        assert_eq!(
            pull_request.canonical_url,
            "https://github.com/acme/app/pull/7"
        );
        assert_ne!(issue.external_key, pull_request.external_key);
    }

    #[test]
    fn an_unrecognised_url_becomes_a_generic_object() {
        let object = classify_url("https://example.com/a").expect("URL should be accepted");

        assert_eq!(object.provider, ExternalProvider::Generic);
        assert_eq!(object.kind, ExternalObjectKind::Generic);
        assert_eq!(object.canonical_url, "https://example.com/a");
    }

    #[test]
    fn github_repository_identity_comes_only_from_valid_github_remotes() {
        assert_eq!(
            github_repository_name("https://github.com/Acme/App.git"),
            Some("acme/app".into())
        );
        assert_eq!(
            github_repository_name("git@github.com:Acme/App.git"),
            Some("acme/app".into())
        );
        assert_eq!(
            github_repository_name("ssh://git@github.com/Acme/App.git"),
            Some("acme/app".into())
        );
        assert_eq!(github_repository_name("ftp://github.com/acme/app"), None);
        assert_eq!(github_repository_name("https://example.com/acme/app"), None);
        assert_eq!(
            github_repository_name("https://github.com/acme/app/issues"),
            None
        );
    }

    #[test]
    fn github_json_becomes_a_snapshot_with_displayable_metadata() {
        let response = serde_json::from_str::<GithubResponse>(
            r#"{
                "number": 7,
                "title": "Ship it",
                "state": "OPEN",
                "author": {"login": "octocat"},
                "labels": [{"name": "ready"}],
                "milestone": {"title": "v1"},
                "updatedAt": "2026-09-19T12:00:00Z"
            }"#,
        )
        .expect("fixture should parse");
        let snapshot = response.into_snapshot(123);

        assert_eq!(snapshot.title, "Ship it");
        assert_eq!(snapshot.state, "OPEN");
        assert_eq!(snapshot.fetched_at, 123);
        assert_eq!(snapshot.metadata[0].value, "7");
        assert_eq!(snapshot.metadata[1].value, "octocat");
        assert_eq!(snapshot.metadata[2].value, "ready");
    }

    #[cfg(unix)]
    #[test]
    fn github_cli_fetches_json_through_the_resolved_executable() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("gh");
        fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s' '{\"number\":7,\"title\":\"From gh\",\"state\":\"OPEN\",\"author\":null,\"labels\":[],\"milestone\":null,\"updatedAt\":null}'\n",
        )
        .expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");

        let object = classify_url("https://github.com/acme/app/issues/7")
            .expect("GitHub issue should classify");
        let snapshot = GithubCli::new(executable)
            .fetch(&object, 123)
            .expect("gh should run");

        assert_eq!(snapshot.title, "From gh");
        assert_eq!(snapshot.state, "OPEN");
        assert_eq!(snapshot.fetched_at, 123);
    }

    #[cfg(unix)]
    #[test]
    fn github_cli_creates_an_issue_and_adds_a_comment_explicitly() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("gh");
        let arguments = directory.path().join("arguments");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$@" >> '{}'
if [ "$1" = api ] && [ "$2" = repos/acme/app/issues ]; then
  printf '%s' '{{"html_url":"https://github.com/acme/app/issues/42"}}'
fi
"#,
            arguments.display()
        );
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");

        let cli = GithubCli::new(executable);
        let url = cli
            .create_issue("acme/app", "Ship the parser", "Keep the existing Item")
            .expect("issue creation should return its URL");
        cli.add_comment(&url, "Please review the edge case")
            .expect("comment creation should succeed");

        assert_eq!(url, "https://github.com/acme/app/issues/42");
        let arguments = fs::read_to_string(arguments).expect("CLI arguments should be recorded");
        assert_eq!(
            arguments.lines().collect::<Vec<_>>(),
            vec![
                "api",
                "repos/acme/app/issues",
                "--method",
                "POST",
                "--raw-field",
                "title=Ship the parser",
                "--raw-field",
                "body=Keep the existing Item",
                "issue",
                "comment",
                "https://github.com/acme/app/issues/42",
                "--body",
                "Please review the edge case",
            ]
        );
    }
}
