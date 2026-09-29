use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dependencies::resolve_executable;
use crate::domain::{
    ExternalMetadata, ExternalObjectInput, ExternalObjectKind, ExternalProvider,
    ExternalSnapshotData,
};

const GH_JSON_FIELDS: &str = "number,title,state,author,labels,milestone,createdAt,updatedAt";

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("the external URL cannot be blank")]
    EmptyUrl,
    #[error("GitHub CLI (`gh`) could not be found at the Context path or on PATH. Set its executable path in Settings → Contexts → Providers, or install GitHub CLI.")]
    GhNotFound,
    #[error("only GitHub Issues have a readable issue document")]
    NotAnIssue,
    #[error("{operation} is not implemented for the {provider:?} provider yet")]
    UnsupportedProvider {
        provider: ExternalProvider,
        operation: &'static str,
    },
    #[error("GitHub CLI failed: {message}")]
    GhFailed { message: String },
    #[error("Atlassian TWG CLI failed: {message}")]
    TwgFailed { message: String },
    #[error("Azure DevOps CLI (`az`) failed: {message}")]
    AzureDevOpsFailed { message: String },
    #[error("GitHub returned invalid JSON: {0}")]
    InvalidResponse(#[from] serde_json::Error),
    #[error("could not run GitHub CLI: {0}")]
    Io(#[from] std::io::Error),
}

/// Provider operations are selected in one place so feature code does not
/// depend on a provider's CLI or API directly.
pub struct ProviderDispatch {
    provider: ExternalProvider,
    executable: Option<PathBuf>,
    site: Option<String>,
    workspace: Option<String>,
    organization: Option<String>,
}

impl ProviderDispatch {
    pub fn new(provider: ExternalProvider, executable: Option<PathBuf>) -> Self {
        Self {
            provider,
            executable,
            site: None,
            workspace: None,
            organization: None,
        }
    }

    pub fn with_site(mut self, site: Option<String>) -> Self {
        self.site = site;
        self
    }

    pub fn with_workspace(mut self, workspace: Option<String>) -> Self {
        self.workspace = workspace;
        self
    }

    pub fn with_organization(mut self, organization: Option<String>) -> Self {
        self.organization = organization;
        self
    }

    fn github(&self, operation: &'static str) -> Result<GithubCli, ProviderError> {
        if self.provider != ExternalProvider::GitHub {
            return Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation,
            });
        }
        let executable = resolve_gh_executable(self.executable.as_deref())?;
        Ok(GithubCli::new(executable))
    }

    pub fn resolved_executable(&self) -> Result<PathBuf, ProviderError> {
        match self.provider {
            ExternalProvider::GitHub => resolve_gh_executable(self.executable.as_deref()),
            ExternalProvider::Atlassian => resolve_twg_executable(self.executable.as_deref()),
            ExternalProvider::AzureDevOps => resolve_az_executable(self.executable.as_deref()),
            _ => Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation: "executable resolution",
            }),
        }
    }

    pub fn fetch_snapshot(
        &self,
        object: &ExternalObjectInput,
        fetched_at: i64,
    ) -> Result<ExternalSnapshotData, ProviderError> {
        match self.provider {
            ExternalProvider::GitHub => self.github("snapshot fetching")?.fetch(object, fetched_at),
            ExternalProvider::Atlassian => self
                .atlassian("snapshot fetching")?
                .fetch_snapshot(object, fetched_at),
            ExternalProvider::AzureDevOps => self
                .azure_devops("snapshot fetching")?
                .fetch_snapshot(object, fetched_at),
            _ => Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation: "snapshot fetching",
            }),
        }
    }

    pub fn fetch_document(&self, object: &ExternalObjectInput) -> Result<String, ProviderError> {
        match self.provider {
            ExternalProvider::GitHub => self
                .github("document reading")?
                .fetch_document(&object.canonical_url),
            ExternalProvider::Atlassian => Ok(self
                .atlassian("document reading")?
                .fetch_document_with_format(object)?
                .body),
            ExternalProvider::AzureDevOps => Ok(self
                .azure_devops("document reading")?
                .fetch_document_with_format(object)?
                .body),
            _ => Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation: "document reading",
            }),
        }
    }

    pub fn fetch_document_with_format(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<ExternalDocument, ProviderError> {
        match self.provider {
            ExternalProvider::GitHub => Ok(ExternalDocument {
                body: self
                    .github("document reading")?
                    .fetch_document(&object.canonical_url)?,
                body_format: "markdown".into(),
            }),
            ExternalProvider::Atlassian => self
                .atlassian("document reading")?
                .fetch_document_with_format(object),
            ExternalProvider::AzureDevOps => self
                .azure_devops("document reading")?
                .fetch_document_with_format(object),
            _ => Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation: "document reading",
            }),
        }
    }

    pub fn fetch_comments(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<Vec<ExternalComment>, ProviderError> {
        match self.provider {
            ExternalProvider::GitHub => self.github("comment reading")?.fetch_comments(object),
            ExternalProvider::Atlassian => {
                self.atlassian("comment reading")?.fetch_comments(object)
            }
            ExternalProvider::AzureDevOps => {
                self.azure_devops("comment reading")?.fetch_comments(object)
            }
            _ => Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation: "comment reading",
            }),
        }
    }

    fn atlassian(&self, operation: &'static str) -> Result<TwgCli, ProviderError> {
        if self.provider != ExternalProvider::Atlassian {
            return Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation,
            });
        }
        let site = self
            .site
            .as_deref()
            .map(|site| site.trim().to_owned())
            .filter(|site| !site.is_empty());
        Ok(TwgCli {
            executable: resolve_twg_executable(self.executable.as_deref())?,
            site,
            workspace: self.workspace.clone(),
        })
    }

    fn azure_devops(&self, operation: &'static str) -> Result<AzureDevOpsCli, ProviderError> {
        if self.provider != ExternalProvider::AzureDevOps {
            return Err(ProviderError::UnsupportedProvider {
                provider: self.provider,
                operation,
            });
        }
        let organization = self
            .organization
            .as_deref()
            .map(str::trim)
            .filter(|organization| !organization.is_empty())
            .map(normalize_ado_organization);
        Ok(AzureDevOpsCli {
            executable: resolve_az_executable(self.executable.as_deref())?,
            organization,
        })
    }

    pub fn list_tickets(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<Vec<SubIssue>, ProviderError> {
        self.github("ticket listing")?
            .list_tickets(&object.canonical_url)
    }

    pub fn create_issue(
        &self,
        repository: &str,
        title: &str,
        body: &str,
    ) -> Result<String, ProviderError> {
        self.github("Issue creation")?
            .create_issue(repository, title, body)
    }

    pub fn add_comment(
        &self,
        object: &ExternalObjectInput,
        body: &str,
    ) -> Result<(), ProviderError> {
        self.github("comment writing")?
            .add_comment(&object.canonical_url, body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalComment {
    pub id: u64,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalDocument {
    pub body: String,
    pub body_format: String,
}

pub struct TwgCli {
    executable: PathBuf,
    site: Option<String>,
    workspace: Option<String>,
}

impl TwgCli {
    fn run(&self, args: &[&str]) -> Result<serde_json::Value, ProviderError> {
        if matches!(args.first(), Some(&"jira" | &"confluence")) && self.site.is_none() {
            return Err(ProviderError::TwgFailed {
                message: "an Atlassian site is not configured for this Context".into(),
            });
        }
        let mut command = Command::new(&self.executable);
        command.args(args);
        if let Some(site) = self.site.as_deref() {
            command.args(["--site", site]);
        }
        if let Some(workspace) = self.workspace.as_deref() {
            command.args(["--workspace", workspace]);
        }
        let output = command.args(["--output", "json"]).output()?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Err(ProviderError::TwgFailed {
                message: if message.is_empty() {
                    "the command returned a non-zero exit status".into()
                } else {
                    message
                },
            });
        }
        serde_json::from_slice(&output.stdout).map_err(|error| ProviderError::TwgFailed {
            message: format!("TWG returned invalid JSON: {error}"),
        })
    }

    fn fetch_snapshot(
        &self,
        object: &ExternalObjectInput,
        fetched_at: i64,
    ) -> Result<ExternalSnapshotData, ProviderError> {
        let value = match object.kind {
            ExternalObjectKind::Issue => {
                let key = object
                    .external_key
                    .strip_prefix("jira:")
                    .and_then(|key| key.split_once('#'))
                    .map(|(_, key)| key)
                    .ok_or_else(|| ProviderError::TwgFailed {
                        message: "invalid Jira work item identifier".into(),
                    })?;
                self.run(&["jira", "workitem", "get", key])?
            }
            ExternalObjectKind::Document => {
                let id = object
                    .external_key
                    .strip_prefix("confluence:")
                    .and_then(|key| key.split_once('#'))
                    .map(|(_, id)| id)
                    .ok_or_else(|| ProviderError::TwgFailed {
                        message: "invalid Confluence page identifier".into(),
                    })?;
                self.run(&["confluence", "content", "get", id, "--detail", "full"])?
            }
            ExternalObjectKind::PullRequest => {
                self.run(&["bitbucket", "pull-requests", "get", &object.canonical_url])?
            }
            ExternalObjectKind::Generic => {
                return Err(ProviderError::TwgFailed {
                    message: "generic links do not have an Atlassian snapshot".into(),
                })
            }
        };
        let data = unwrap_data(&value);
        let title = first_string(data, &["title", "summary", "name"]).unwrap_or_default();
        if title.is_empty() {
            return Err(ProviderError::TwgFailed {
                message: "TWG returned an object without a title".into(),
            });
        }
        let state = if object.kind == ExternalObjectKind::Document {
            version_value(data).unwrap_or_else(|| "".into())
        } else {
            first_string(data, &["state", "status"]).unwrap_or_default()
        };
        let metadata = if object.kind == ExternalObjectKind::Document {
            version_value(data)
                .map(|version| {
                    vec![ExternalMetadata {
                        key: "version".into(),
                        value: version,
                    }]
                })
                .unwrap_or_default()
        } else {
            [
                "key", "issueKey", "number", "id", "author", "assignee", "priority", "type",
                "created", "updated", "url",
            ]
            .iter()
            .filter_map(|key| {
                data.get(*key)
                    .and_then(value_string)
                    .map(|value| ExternalMetadata {
                        key: (*key).to_owned(),
                        value,
                    })
            })
            .collect()
        };
        Ok(ExternalSnapshotData {
            title,
            state,
            metadata,
            fetched_at,
        })
    }

    fn fetch_document_with_format(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<ExternalDocument, ProviderError> {
        match object.kind {
            ExternalObjectKind::Issue => {
                let key = jira_key(object)?;
                let value = self.run(&["jira", "workitem", "get", key])?;
                let body = find_value(unwrap_data(&value), &["description", "body"])
                    .map(render_value)
                    .unwrap_or_default();
                Ok(ExternalDocument {
                    body,
                    body_format: "markdown".into(),
                })
            }
            ExternalObjectKind::Document => {
                let id = confluence_id(object)?;
                let value = self.run(&["confluence", "content", "get", id, "--detail", "full"])?;
                let data = unwrap_data(&value);
                let body = find_value(data, &["body", "bodyHtml", "html"])
                    .map(render_value)
                    .unwrap_or_default();
                Ok(ExternalDocument {
                    body,
                    body_format: "html".into(),
                })
            }
            _ => Err(ProviderError::TwgFailed {
                message: "this Atlassian object does not have a readable document".into(),
            }),
        }
    }

    fn fetch_comments(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<Vec<ExternalComment>, ProviderError> {
        let value = match object.kind {
            ExternalObjectKind::Issue => {
                self.run(&["jira", "workitem", "comment", "query", jira_key(object)?])?
            }
            ExternalObjectKind::Document => self.run(&[
                "confluence",
                "content",
                "comments",
                "list",
                confluence_id(object)?,
            ])?,
            ExternalObjectKind::PullRequest => self.run(&[
                "bitbucket",
                "pull-requests",
                "comment",
                "query",
                &object.canonical_url,
            ])?,
            _ => {
                return Err(ProviderError::TwgFailed {
                    message: "comments are not available for this Atlassian object".into(),
                })
            }
        };
        let data = unwrap_data(&value);
        let entries = data
            .as_array()
            .or_else(|| data.get("comments").and_then(serde_json::Value::as_array))
            .cloned()
            .unwrap_or_default();
        Ok(entries
            .iter()
            .enumerate()
            .map(|(index, comment)| ExternalComment {
                id: comment
                    .get("id")
                    .and_then(value_string)
                    .and_then(|id| id.parse().ok())
                    .unwrap_or(index as u64),
                author: comment
                    .get("author")
                    .and_then(value_string)
                    .or_else(|| first_string(comment, &["displayName", "name", "login"]))
                    .unwrap_or_else(|| "Unknown".into()),
                body: find_value(comment, &["body", "text"])
                    .map(render_value)
                    .unwrap_or_default(),
                created_at: find_value(comment, &["created", "createdAt", "created_at"])
                    .map(render_value)
                    .unwrap_or_default(),
            })
            .collect())
    }
}

/// Azure DevOps CLI adapter. Organization is always supplied per command so
/// Contexts do not depend on mutable global `az devops configure` defaults.
pub struct AzureDevOpsCli {
    executable: PathBuf,
    organization: Option<String>,
}

impl AzureDevOpsCli {
    fn run(&self, args: &[&str]) -> Result<serde_json::Value, ProviderError> {
        let organization =
            self.organization
                .as_deref()
                .ok_or_else(|| ProviderError::AzureDevOpsFailed {
                    message: "an Azure DevOps organization is not configured for this Context"
                        .into(),
                })?;
        let output = Command::new(&self.executable)
            .args(args)
            .args(["--organization", organization, "-o", "json"])
            .output()?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let message = if message.is_empty() {
                "the command returned a non-zero exit status".into()
            } else if azure_devops_extension_missing(&message) {
                format!(
                    "Azure DevOps CLI extension `azure-devops` is unavailable. install it with `az extension add --name azure-devops`. {message}"
                )
            } else {
                message
            };
            return Err(ProviderError::AzureDevOpsFailed { message });
        }
        serde_json::from_slice(&output.stdout).map_err(|error| ProviderError::AzureDevOpsFailed {
            message: format!("Azure DevOps CLI returned invalid JSON: {error}"),
        })
    }

    fn fetch_snapshot(
        &self,
        object: &ExternalObjectInput,
        fetched_at: i64,
    ) -> Result<ExternalSnapshotData, ProviderError> {
        let (organization, project, id) = ado_identity(object)?;
        let data = if object.kind == ExternalObjectKind::Issue {
            self.run(&[
                "boards",
                "work-item",
                "show",
                "--id",
                id,
                "--fields",
                "System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo,System.CreatedDate,System.ChangedDate,System.Tags,System.Description",
            ])?
        } else if object.kind == ExternalObjectKind::PullRequest {
            self.run(&["repos", "pr", "show", "--id", id])?
        } else {
            return Err(ProviderError::AzureDevOpsFailed {
                message: "only Azure DevOps work items and pull requests have snapshots".into(),
            });
        };
        let (title, state, metadata) = if object.kind == ExternalObjectKind::Issue {
            let fields = data.get("fields").unwrap_or(&data);
            let title = first_string(fields, &["System.Title", "title"]).unwrap_or_default();
            let state = first_string(fields, &["System.State", "state"]).unwrap_or_default();
            let mut metadata = vec![ExternalMetadata {
                key: "id".into(),
                value: id.to_owned(),
            }];
            for (key, names) in [
                ("type", &["System.WorkItemType", "type"] as &[&str]),
                ("assignee", &["System.AssignedTo", "assignedTo"]),
                ("created", &["System.CreatedDate", "createdDate"]),
                ("updated", &["System.ChangedDate", "changedDate"]),
                ("tags", &["System.Tags", "tags"]),
            ] {
                if let Some(value) = find_value(fields, names).map(render_value) {
                    metadata.push(ExternalMetadata {
                        key: key.into(),
                        value,
                    });
                }
            }
            metadata.push(ExternalMetadata {
                key: "project".into(),
                value: project.to_owned(),
            });
            metadata.push(ExternalMetadata {
                key: "organization".into(),
                value: organization.to_owned(),
            });
            (title, state, metadata)
        } else {
            let title = first_string(&data, &["title", "name"]).unwrap_or_default();
            let state = first_string(&data, &["status", "state"]).unwrap_or_default();
            let mut metadata = vec![ExternalMetadata {
                key: "id".into(),
                value: id.to_owned(),
            }];
            for (key, names) in [
                ("author", &["createdBy", "author"] as &[&str]),
                ("repository", &["repository", "repository.name"]),
                ("created", &["creationDate", "createdDate"]),
                ("updated", &["closedDate", "closed_date"]),
            ] {
                if let Some(value) = nested_or_string(&data, names) {
                    metadata.push(ExternalMetadata {
                        key: key.into(),
                        value,
                    });
                }
            }
            metadata.push(ExternalMetadata {
                key: "project".into(),
                value: project.to_owned(),
            });
            metadata.push(ExternalMetadata {
                key: "organization".into(),
                value: organization.to_owned(),
            });
            (title, state, metadata)
        };
        if title.is_empty() {
            return Err(ProviderError::AzureDevOpsFailed {
                message: "Azure DevOps returned an object without a title".into(),
            });
        }
        Ok(ExternalSnapshotData {
            title,
            state,
            metadata,
            fetched_at,
        })
    }

    fn fetch_document_with_format(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<ExternalDocument, ProviderError> {
        if object.kind != ExternalObjectKind::Issue {
            return Err(ProviderError::AzureDevOpsFailed {
                message: "only Azure DevOps work items have a readable description".into(),
            });
        }
        let (_, _, id) = ado_identity(object)?;
        let data = self.run(&[
            "boards",
            "work-item",
            "show",
            "--id",
            id,
            "--fields",
            "System.Description",
        ])?;
        let fields = data.get("fields").unwrap_or(&data);
        Ok(ExternalDocument {
            body: find_value(fields, &["System.Description", "description"])
                .map(render_value)
                .unwrap_or_default(),
            body_format: "html".into(),
        })
    }

    fn fetch_comments(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<Vec<ExternalComment>, ProviderError> {
        if object.kind == ExternalObjectKind::PullRequest {
            let (_, project, id) = ado_identity(object)?;
            let pull_request = self.run(&["repos", "pr", "show", "--id", id])?;
            let repository_id = pull_request
                .get("repository")
                .and_then(|repository| repository.get("id"))
                .and_then(value_string)
                .filter(|repository_id| !repository_id.is_empty())
                .ok_or_else(|| ProviderError::AzureDevOpsFailed {
                    message: "Azure DevOps did not return a repository id for this pull request"
                        .into(),
                })?;
            let data = self.run(&[
                "devops",
                "invoke",
                "--area",
                "git",
                "--resource",
                &format!("repositories/{{repositoryId}}/pullRequests/{{pullRequestId}}/threads"),
                "--route-parameters",
                &format!("project={project}"),
                &format!("repositoryId={repository_id}"),
                &format!("pullRequestId={id}"),
                "--api-version",
                "7.1",
                "--http-method",
                "GET",
            ])?;
            let threads = data
                .get("value")
                .and_then(serde_json::Value::as_array)
                .or_else(|| data.get("threads").and_then(serde_json::Value::as_array))
                .or_else(|| data.as_array())
                .cloned()
                .unwrap_or_default();
            return Ok(threads
                .iter()
                .filter(|thread| thread.get("isDeleted") != Some(&serde_json::Value::Bool(true)))
                .filter_map(|thread| thread.get("comments").and_then(serde_json::Value::as_array))
                .flatten()
                .filter(|comment| comment.get("isDeleted") != Some(&serde_json::Value::Bool(true)))
                .filter(|comment| {
                    !matches!(
                        comment
                            .get("commentType")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_ascii_lowercase)
                            .as_deref(),
                        Some("system" | "codechange")
                    )
                })
                .map(|comment| ExternalComment {
                    id: comment
                        .get("id")
                        .and_then(value_string)
                        .and_then(|id| id.parse().ok())
                        .unwrap_or_default(),
                    author: nested_or_string(comment, &["author"])
                        .unwrap_or_else(|| "Unknown".into()),
                    body: find_value(comment, &["content"])
                        .map(render_value)
                        .unwrap_or_default(),
                    created_at: find_value(comment, &["publishedDate", "createdDate"])
                        .map(render_value)
                        .unwrap_or_default(),
                })
                .collect());
        }
        if object.kind != ExternalObjectKind::Issue {
            return Err(ProviderError::AzureDevOpsFailed {
                message:
                    "comments are available only for Azure DevOps work items and pull requests"
                        .into(),
            });
        }
        let (_, project, id) = ado_identity(object)?;
        let data = self.run(&[
            "devops",
            "invoke",
            "--area",
            "wit",
            "--resource",
            "workItems/{workItemId}/comments",
            "--route-parameters",
            &format!("project={project}"),
            &format!("workItemId={id}"),
            "--api-version",
            "7.1-preview.4",
            "--http-method",
            "GET",
        ])?;
        let entries = data
            .get("comments")
            .and_then(serde_json::Value::as_array)
            .or_else(|| data.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(entries
            .iter()
            .enumerate()
            .map(|(index, comment)| ExternalComment {
                id: comment
                    .get("commentId")
                    .or_else(|| comment.get("id"))
                    .and_then(value_string)
                    .and_then(|id| id.parse().ok())
                    .unwrap_or(index as u64),
                author: nested_or_string(comment, &["createdBy", "author"])
                    .unwrap_or_else(|| "Unknown".into()),
                body: find_value(comment, &["text", "body"])
                    .map(render_value)
                    .unwrap_or_default(),
                created_at: find_value(comment, &["createdDate", "createdAt"])
                    .map(render_value)
                    .unwrap_or_default(),
            })
            .collect())
    }
}

fn ado_identity(object: &ExternalObjectInput) -> Result<(&str, &str, &str), ProviderError> {
    let Some(identity) = object.external_key.strip_prefix("ado:") else {
        return Err(ProviderError::AzureDevOpsFailed {
            message: "invalid Azure DevOps object identifier".into(),
        });
    };
    let Some((org_project, id)) = identity.split_once('#') else {
        return Err(ProviderError::AzureDevOpsFailed {
            message: "invalid Azure DevOps object identifier".into(),
        });
    };
    let Some((organization, project)) = org_project.split_once('/') else {
        return Err(ProviderError::AzureDevOpsFailed {
            message: "invalid Azure DevOps object identifier".into(),
        });
    };
    if organization.is_empty() || project.is_empty() || id.parse::<u64>().is_err() {
        return Err(ProviderError::AzureDevOpsFailed {
            message: "invalid Azure DevOps object identifier".into(),
        });
    }
    Ok((organization, project, id))
}

fn normalize_ado_organization(organization: &str) -> String {
    let trimmed = organization.trim().trim_end_matches('/');
    if trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else {
        format!("https://dev.azure.com/{trimmed}")
    }
}

fn azure_devops_extension_missing(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("extension")
        || message.contains("command not found")
        || message.contains("not a valid command")
        || message.contains("not recognized")
}

fn nested_or_string(value: &serde_json::Value, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(value) = value.get(*name) {
            if let Some(value) = value_string(value) {
                return Some(value);
            }
            for nested in ["displayName", "name", "uniqueName"] {
                if let Some(value) = value.get(nested).and_then(value_string) {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn jira_key(object: &ExternalObjectInput) -> Result<&str, ProviderError> {
    object
        .external_key
        .strip_prefix("jira:")
        .and_then(|key| key.split_once('#'))
        .map(|(_, key)| key)
        .ok_or_else(|| ProviderError::TwgFailed {
            message: "invalid Jira work item identifier".into(),
        })
}
fn confluence_id(object: &ExternalObjectInput) -> Result<&str, ProviderError> {
    object
        .external_key
        .strip_prefix("confluence:")
        .and_then(|key| key.split_once('#'))
        .map(|(_, id)| id)
        .ok_or_else(|| ProviderError::TwgFailed {
            message: "invalid Confluence page identifier".into(),
        })
}
fn unwrap_data(value: &serde_json::Value) -> &serde_json::Value {
    value.get("data").unwrap_or(value)
}
fn value_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Object(map) => map
            .get("name")
            .or_else(|| map.get("value"))
            .or_else(|| map.get("displayName"))
            .map(render_value),
        _ => None,
    }
}
fn find_value<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a serde_json::Value> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .or_else(|| match value {
            serde_json::Value::Object(fields) => {
                fields.values().find_map(|child| find_value(child, keys))
            }
            serde_json::Value::Array(values) => {
                values.iter().find_map(|child| find_value(child, keys))
            }
            _ => None,
        })
}
fn first_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    find_value(value, keys).and_then(value_string)
}
fn version_value(value: &serde_json::Value) -> Option<String> {
    find_value(value, &["version"])
        .and_then(value_string)
        .or_else(|| find_value(value, &["number"]).and_then(value_string))
}
fn render_value(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    if let Some(storage) = value
        .get("storage")
        .and_then(|storage| storage.get("value"))
        .and_then(serde_json::Value::as_str)
    {
        return storage.to_owned();
    }
    fn collect(value: &serde_json::Value, out: &mut String) {
        if let Some(text) = value.get("text").and_then(serde_json::Value::as_str) {
            out.push_str(text);
        }
        if let Some(content) = value.get("content").and_then(serde_json::Value::as_array) {
            for child in content {
                collect(child, out);
            }
            if matches!(
                value.get("type").and_then(serde_json::Value::as_str),
                Some("paragraph" | "heading" | "listItem")
            ) {
                out.push('\n');
            }
        }
    }
    let mut text = String::new();
    collect(value, &mut text);
    if !text.is_empty() {
        text.trim().to_owned()
    } else {
        serde_json::to_string(value).unwrap_or_default()
    }
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
    let host = parts.get(2).copied().unwrap_or_default();
    let is_https = parts.first() == Some(&"https:");
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
            ExternalObjectKind::Document => "document",
            ExternalObjectKind::Generic => "generic",
        };
        let path_kind = match kind {
            ExternalObjectKind::Issue => "issues",
            ExternalObjectKind::PullRequest => "pull",
            ExternalObjectKind::Document => unreachable!(),
            ExternalObjectKind::Generic => unreachable!(),
        };
        return Ok(ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind,
            external_key: format!("{kind_key}:{owner}/{repository}#{number}"),
            canonical_url: format!("https://github.com/{owner}/{repository}/{path_kind}/{number}"),
        });
    }

    let atlassian_host = host.to_ascii_lowercase();
    if is_https {
        if let Some(site) = atlassian_host.strip_suffix(".atlassian.net") {
            if !site.is_empty() {
                let path = &parts[3..];
                if path.len() >= 2 && path[0] == "browse" && valid_jira_key(path[1]) {
                    let key = path[1];
                    return Ok(classified(
                        ExternalProvider::Atlassian,
                        ExternalObjectKind::Issue,
                        format!("jira:{site}#{key}"),
                        format!("https://{site}.atlassian.net/browse/{key}"),
                    ));
                }
                if path.len() >= 5
                    && path[0] == "wiki"
                    && path[1] == "spaces"
                    && !path[2].is_empty()
                    && path[3] == "pages"
                {
                    if let Ok(page_id) = path[4].parse::<u64>() {
                        return Ok(classified(
                            ExternalProvider::Atlassian,
                            ExternalObjectKind::Document,
                            format!("confluence:{site}#{page_id}"),
                            format!(
                                "https://{site}.atlassian.net/wiki/spaces/{}/pages/{page_id}",
                                path[2]
                            ),
                        ));
                    }
                }
            }
        }
    }
    if is_https
        && atlassian_host == "bitbucket.org"
        && parts.len() == 7
        && parts[3..5].iter().all(|segment| !segment.is_empty())
        && parts[5] == "pull-requests"
    {
        if let Ok(number) = parts[6].parse::<u64>() {
            let workspace = parts[3].to_ascii_lowercase();
            let repository = parts[4].to_ascii_lowercase();
            return Ok(classified(
                ExternalProvider::Atlassian,
                ExternalObjectKind::PullRequest,
                format!("bitbucket:{workspace}/{repository}#{number}"),
                format!("https://bitbucket.org/{workspace}/{repository}/pull-requests/{number}"),
            ));
        }
    }

    if is_https && atlassian_host == "dev.azure.com" && parts.len() >= 8 && !parts[3].is_empty() {
        let organization = parts[3].to_ascii_lowercase();
        let project = parts[4].to_ascii_lowercase();
        if !project.is_empty() && parts[5] == "_workitems" && parts[6] == "edit" {
            if let Ok(id) = parts[7].parse::<u64>() {
                return Ok(classified(
                    ExternalProvider::AzureDevOps,
                    ExternalObjectKind::Issue,
                    format!("ado:{organization}/{project}#{id}"),
                    format!("https://dev.azure.com/{organization}/{project}/_workitems/edit/{id}"),
                ));
            }
        }
        if !project.is_empty()
            && parts.len() >= 9
            && parts[5] == "_git"
            && !parts[6].is_empty()
            && parts[7] == "pullrequest"
        {
            if let Ok(id) = parts[8].parse::<u64>() {
                let repository = parts[6].to_ascii_lowercase();
                return Ok(classified(
                    ExternalProvider::AzureDevOps,
                    ExternalObjectKind::PullRequest,
                    format!("ado:{organization}/{project}#{id}"),
                    format!("https://dev.azure.com/{organization}/{project}/_git/{repository}/pullrequest/{id}"),
                ));
            }
        }
    }

    Ok(ExternalObjectInput {
        provider: ExternalProvider::Generic,
        kind: ExternalObjectKind::Generic,
        external_key: raw_url.to_owned(),
        canonical_url: raw_url.to_owned(),
    })
}

/// Classify a Markdown file inside a registered repository as a stable local object.
pub(crate) fn classify_local_markdown(
    repository_id: i64,
    repository_root: &Path,
    raw_path: &str,
) -> Option<ExternalObjectInput> {
    let root = repository_root.canonicalize().ok()?;
    let path = raw_path.strip_prefix("file://").unwrap_or(raw_path);
    let path = PathBuf::from(percent_decode_path(path)?);
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    let path = path.canonicalize().ok()?;
    let relative = path.strip_prefix(&root).ok()?;
    let extension = relative.extension()?.to_str()?;
    if !matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown") {
        return None;
    }
    let relative = relative
        .components()
        .map(|component| component.as_os_str().to_str())
        .collect::<Option<Vec<_>>>()?
        .join("/");
    let file_url = file_url(&path)?;
    Some(classified(
        ExternalProvider::Generic,
        ExternalObjectKind::Generic,
        format!("local:{repository_id}#{relative}"),
        file_url,
    ))
}

fn percent_decode_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = (*bytes.get(index + 1)? as char).to_digit(16)? as u8;
            let low = (*bytes.get(index + 2)? as char).to_digit(16)? as u8;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn file_url(path: &Path) -> Option<String> {
    let path = path.to_str()?;
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~:".contains(&byte) {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(encoded, "%{byte:02X}").ok()?;
        }
    }
    Some(format!("file://{encoded}"))
}

fn classified(
    provider: ExternalProvider,
    kind: ExternalObjectKind,
    external_key: String,
    canonical_url: String,
) -> ExternalObjectInput {
    ExternalObjectInput {
        provider,
        kind,
        external_key,
        canonical_url,
    }
}

fn valid_jira_key(key: &str) -> bool {
    let Some((project, number)) = key.rsplit_once('-') else {
        return false;
    };
    !project.is_empty()
        && project.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
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
            ExternalObjectKind::Document => {
                return Err(ProviderError::GhFailed {
                    message: "documents do not have a GitHub snapshot".into(),
                })
            }
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

impl GithubCli {
    /// Read an Issue or pull request body. Nothing is stored.
    pub fn fetch_document(&self, issue_url: &str) -> Result<String, ProviderError> {
        let issue = classify_url(issue_url)?;
        if !matches!(
            issue.kind,
            ExternalObjectKind::Issue | ExternalObjectKind::PullRequest
        ) {
            return Err(ProviderError::NotAnIssue);
        }
        let command = if issue.kind == ExternalObjectKind::PullRequest {
            "pr"
        } else {
            "issue"
        };
        let output = Command::new(&self.executable)
            .args([
                command,
                "view",
                issue.canonical_url.as_str(),
                "--json",
                "body",
            ])
            .output()?;
        ensure_success(&output)?;
        Ok(serde_json::from_slice::<GithubIssueBody>(&output.stdout)?.body)
    }

    pub fn fetch_comments(
        &self,
        object: &ExternalObjectInput,
    ) -> Result<Vec<ExternalComment>, ProviderError> {
        if !matches!(
            object.kind,
            ExternalObjectKind::Issue | ExternalObjectKind::PullRequest
        ) {
            return Err(ProviderError::GhFailed {
                message: "comments are only available for GitHub Issues and pull requests".into(),
            });
        }
        let (repository, number) = object
            .external_key
            .split_once(':')
            .and_then(|(_, key)| key.split_once('#'))
            .ok_or(ProviderError::NotAnIssue)?;
        let endpoint = format!("repos/{repository}/issues/{number}/comments");
        let output = Command::new(&self.executable)
            .args(["api", endpoint.as_str(), "--paginate", "--slurp"])
            .output()?;
        ensure_success(&output)?;
        let pages = serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout)?;
        let comments = pages
            .into_iter()
            .flat_map(|page| match page {
                serde_json::Value::Array(comments) => comments,
                comment => vec![comment],
            })
            .collect::<Vec<_>>();
        serde_json::from_value::<Vec<GithubComment>>(serde_json::Value::Array(comments))
            .map(|comments| comments.into_iter().map(Into::into).collect())
            .map_err(Into::into)
    }

    /// List an Issue's native sub-issues.
    pub fn list_tickets(&self, issue_url: &str) -> Result<Vec<SubIssue>, ProviderError> {
        let issue = classify_url(issue_url)?;
        if issue.kind != ExternalObjectKind::Issue {
            return Err(ProviderError::NotAnIssue);
        }
        let (repository, number) = issue
            .external_key
            .strip_prefix("issue:")
            .and_then(|key| key.split_once('#'))
            .ok_or(ProviderError::NotAnIssue)?;

        let sub_issues_path = format!("repos/{repository}/issues/{number}/sub_issues");
        let output = Command::new(&self.executable)
            .args([
                "api",
                sub_issues_path.as_str(),
                "--paginate",
                "--jq",
                ".[] | {number, title, state, html_url}",
            ])
            .output()?;
        ensure_success(&output)?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let sub_issue = serde_json::from_str::<GithubSubIssue>(line)?;
                // Canonical, so it matches the URL of a Link to the same Issue.
                let url = classify_url(&sub_issue.html_url)
                    .map(|object| object.canonical_url)
                    .unwrap_or(sub_issue.html_url);
                Ok(SubIssue {
                    number: sub_issue.number,
                    title: sub_issue.title,
                    state: sub_issue.state,
                    url,
                })
            })
            .collect::<Result<Vec<_>, ProviderError>>()
    }

    /// Compatibility helper used by the Spec view.
    #[cfg(test)]
    pub fn fetch_issue_document(&self, issue_url: &str) -> Result<IssueDocument, ProviderError> {
        if classify_url(issue_url)?.kind != ExternalObjectKind::Issue {
            return Err(ProviderError::NotAnIssue);
        }
        Ok(IssueDocument {
            body: self.fetch_document(issue_url)?,
            body_format: "markdown".into(),
            sub_issues: self.list_tickets(issue_url)?,
        })
    }
}

/// An Issue read as a document: its body and the sub-issues that break it down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDocument {
    pub body: String,
    pub body_format: String,
    pub sub_issues: Vec<SubIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubIssue {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct GithubIssueBody {
    #[serde(default)]
    body: String,
}

#[derive(Debug, Deserialize)]
struct GithubComment {
    id: u64,
    user: Option<GithubActor>,
    body: String,
    created_at: String,
}

impl From<GithubComment> for ExternalComment {
    fn from(comment: GithubComment) -> Self {
        Self {
            id: comment.id,
            author: comment
                .user
                .map_or_else(|| "Unknown".into(), |user| user.login),
            body: comment.body,
            created_at: comment.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GithubSubIssue {
    number: u64,
    title: String,
    state: String,
    html_url: String,
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

pub fn resolve_twg_executable(stored_path: Option<&Path>) -> Result<PathBuf, ProviderError> {
    resolve_executable("twg", stored_path).ok_or_else(|| ProviderError::TwgFailed { message: "Teamwork Graph CLI (`twg`) could not be found at the Context path or on PATH. Set its executable path in Settings → Contexts → Providers, or install TWG CLI.".into() })
}

pub fn resolve_az_executable(stored_path: Option<&Path>) -> Result<PathBuf, ProviderError> {
    resolve_executable("az", stored_path).ok_or_else(|| ProviderError::AzureDevOpsFailed {
        message: "Azure CLI (`az`) could not be found at the Context path or on PATH. Set its executable path in Settings → Contexts → Providers, or install Azure CLI with the `azure-devops` extension.".into(),
    })
}

#[derive(Debug, Deserialize)]
struct GithubResponse {
    title: String,
    state: String,
    author: Option<GithubActor>,
    #[serde(default)]
    labels: Vec<GithubLabel>,
    milestone: Option<GithubMilestone>,
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
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
        if let Some(created_at) = self.created_at {
            metadata.push(ExternalMetadata {
                key: "created".into(),
                value: created_at,
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
    fn provider_dispatch_explains_unimplemented_provider_operations() {
        let provider = ProviderDispatch::new(ExternalProvider::Atlassian, None);
        let object = ExternalObjectInput {
            provider: ExternalProvider::Atlassian,
            kind: ExternalObjectKind::Issue,
            external_key: "jira:acme#PROJ-1".into(),
            canonical_url: "https://acme.atlassian.net/browse/PROJ-1".into(),
        };

        let error = provider
            .fetch_comments(&object)
            .expect_err("a Context site is required");
        assert!(error.to_string().contains("site is not configured"));
    }

    #[cfg(unix)]
    #[test]
    fn azure_devops_adapter_uses_context_az_organization_and_parses_objects() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("az");
        let args_file = directory.path().join("args");
        let script = format!(
            r##"#!/bin/sh
printf '%s\n' "$*" >> '{}'
case "$1 $2 $3" in
  "boards work-item show") printf '%s' '{{"id":123,"fields":{{"System.Title":"ADO bug","System.State":"Active","System.WorkItemType":"Bug","System.AssignedTo":{{"displayName":"Ada Lovelace"}},"System.CreatedDate":"2026-09-02","System.ChangedDate":"2026-09-03","System.Tags":"urgent","System.Description":"<p>Work item body</p>"}}}}' ;;
  "repos pr show") printf '%s' '{{"pullRequestId":456,"title":"ADO PR","status":"active","createdBy":{{"displayName":"Grace Hopper"}},"repository":{{"id":"repo-guid-123","name":"engine"}},"creationDate":"2026-09-04"}}' ;;
esac
case "$*" in
  *"resource workItems/"*) printf '%s' '{{"comments":[{{"commentId":77,"createdBy":{{"displayName":"Lin"}},"text":"A comment","createdDate":"2026-09-05"}}]}}' ;;
  *"resource repositories/"*) printf '%s' '{{"value":[{{"id":1,"isDeleted":false,"comments":[{{"id":1,"author":{{"displayName":"System"}},"content":"System update","commentType":"system","publishedDate":"2026-09-06"}}]}},{{"id":2,"isDeleted":true,"comments":[{{"id":2,"content":"Deleted thread","commentType":"text"}}]}},{{"id":3,"isDeleted":false,"comments":[{{"id":4,"author":{{"displayName":"Reviewer"}},"content":"Review note","commentType":"text","publishedDate":"2026-09-07"}},{{"id":5,"author":{{"displayName":"Bot"}},"content":"Code changed","commentType":"codeChange"}},{{"id":6,"author":{{"displayName":"Reviewer"}},"content":"Deleted reply","commentType":"text","isDeleted":true}},{{"id":7,"author":{{"displayName":"Reviewer"}},"content":"Reply","commentType":"text","publishedDate":"2026-09-08"}}]}}]}}' ;;
esac
"##,
            args_file.display()
        );
        fs::write(&executable, script).expect("fake az should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake az should be executable");
        let dispatch = || {
            ProviderDispatch::new(ExternalProvider::AzureDevOps, Some(executable.clone()))
                .with_organization(Some("acme-engineering".into()))
        };
        let work_item =
            classify_url("https://dev.azure.com/acme-engineering/Platform/_workitems/edit/123")
                .unwrap();
        let snapshot = dispatch().fetch_snapshot(&work_item, 800).unwrap();
        assert_eq!(snapshot.title, "ADO bug");
        assert_eq!(snapshot.state, "Active");
        assert!(snapshot
            .metadata
            .iter()
            .any(|entry| entry.key == "type" && entry.value == "Bug"));
        let document = dispatch().fetch_document_with_format(&work_item).unwrap();
        assert_eq!(document.body, "<p>Work item body</p>");
        assert_eq!(document.body_format, "html");
        let comments = dispatch().fetch_comments(&work_item).unwrap();
        assert_eq!(comments[0].author, "Lin");
        assert_eq!(comments[0].body, "A comment");

        let pull_request = classify_url(
            "https://dev.azure.com/acme-engineering/Platform/_git/engine/pullrequest/456",
        )
        .unwrap();
        let snapshot = dispatch().fetch_snapshot(&pull_request, 801).unwrap();
        assert_eq!(snapshot.title, "ADO PR");
        assert_eq!(snapshot.state, "active");
        let comments = dispatch().fetch_comments(&pull_request).unwrap();
        assert_eq!(comments.len(), 2, "deleted and system comments are omitted");
        assert_eq!(comments[0].author, "Reviewer");
        assert_eq!(comments[0].body, "Review note");
        assert_eq!(comments[1].body, "Reply");
        let calls = fs::read_to_string(args_file).unwrap();
        assert!(calls.contains("boards work-item show --id 123 --fields System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo,System.CreatedDate,System.ChangedDate,System.Tags,System.Description --organization https://dev.azure.com/acme-engineering -o json"));
        assert!(calls.contains("devops invoke --area wit --resource workItems/{workItemId}/comments --route-parameters project=platform workItemId=123 --api-version 7.1-preview.4 --http-method GET --organization https://dev.azure.com/acme-engineering -o json"));
        assert!(calls.contains(
            "repos pr show --id 456 --organization https://dev.azure.com/acme-engineering -o json"
        ));
        assert!(calls.contains("devops invoke --area git --resource repositories/{repositoryId}/pullRequests/{pullRequestId}/threads --route-parameters project=platform repositoryId=repo-guid-123 pullRequestId=456 --api-version 7.1 --http-method GET --organization https://dev.azure.com/acme-engineering -o json"));
    }

    #[cfg(unix)]
    #[test]
    fn azure_devops_adapter_explains_missing_extension_and_preserves_cli_errors() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("az");
        fs::write(
            &executable,
            "#!/bin/sh\necho \"ERROR: command not recognized; install the azure-devops extension\" >&2\nexit 2\n",
        )
        .expect("fake az should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake az should be executable");
        let provider = ProviderDispatch::new(ExternalProvider::AzureDevOps, Some(executable))
            .with_organization(Some("acme-engineering".into()));
        let object =
            classify_url("https://dev.azure.com/acme-engineering/Platform/_workitems/edit/123")
                .unwrap();
        let error = provider
            .fetch_snapshot(&object, 1)
            .expect_err("missing extension should be reported");
        assert!(error.to_string().contains("azure-devops"));
        assert!(error.to_string().contains("install it with"));
        assert!(error.to_string().contains("command not recognized"));

        let executable = directory.path().join("az-authorized");
        fs::write(
            &executable,
            "#!/bin/sh\necho 'TF401019: The work item does not exist.' >&2\nexit 1\n",
        )
        .expect("failing fake az should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("failing fake az should be executable");
        let provider = ProviderDispatch::new(ExternalProvider::AzureDevOps, Some(executable))
            .with_organization(Some("acme-engineering".into()));
        let error = provider
            .fetch_snapshot(&object, 1)
            .expect_err("Azure DevOps failure should be reported");
        assert!(error
            .to_string()
            .contains("TF401019: The work item does not exist."));
        assert!(!error.to_string().contains("az extension add"));
    }

    #[cfg(unix)]
    #[test]
    fn twg_adapters_use_context_executable_site_and_bitbucket_workspace() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("twg");
        let args_file = directory.path().join("args");
        let script = format!(
            r##"#!/bin/sh
printf '%s\n' "$@" >> '{}'
case "$1 $2 $3" in
  "jira workitem get") printf '%s' '{{"key":"PROJ-7","summary":"Jira title","status":"In Progress","description":{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"Jira description"}}]}}]}}}}' ;;
  "jira workitem comment") printf '%s' '{{"comments":[{{"id":"31","author":{{"displayName":"Ada"}},"body":{{"type":"doc","content":[{{"type":"paragraph","content":[{{"type":"text","text":"Jira comment"}}]}}]}},"created":"2026-09-01"}}]}}' ;;
  "confluence content get") printf '%s' '{{"id":"456","title":"Runbook","version":{{"number":9}},"body":{{"storage":{{"value":"<p>Safe <strong>body</strong></p>"}}}}}}' ;;
  "confluence content comments") printf '%s' '{{"comments":[{{"id":"32","author":{{"displayName":"Grace"}},"body":"Page comment","created":"2026-09-02"}}]}}' ;;
  "bitbucket pull-requests get") printf '%s' '{{"id":89,"title":"PR title","state":"OPEN"}}' ;;
  "bitbucket pull-requests comment") printf '%s' '{{"comments":[{{"id":"33","author":{{"displayName":"Lin"}},"body":"PR comment","created":"2026-09-03"}}]}}' ;;
esac
"##,
            args_file.display()
        );
        fs::write(&executable, script).expect("fake twg should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake twg should be executable");
        let dispatch = |site: Option<&str>, workspace: Option<&str>| {
            ProviderDispatch::new(ExternalProvider::Atlassian, Some(executable.clone()))
                .with_site(site.map(str::to_owned))
                .with_workspace(workspace.map(str::to_owned))
        };
        let jira = classify_url("https://acme.atlassian.net/browse/PROJ-7").unwrap();
        let jira_snapshot = dispatch(Some("acme.atlassian.net"), None)
            .fetch_snapshot(&jira, 77)
            .unwrap();
        assert_eq!(jira_snapshot.title, "Jira title");
        assert_eq!(jira_snapshot.state, "In Progress");
        let jira_document = dispatch(Some("acme.atlassian.net"), None)
            .fetch_document_with_format(&jira)
            .unwrap();
        assert_eq!(jira_document.body, "Jira description");
        let jira_comments = dispatch(Some("acme.atlassian.net"), None)
            .fetch_comments(&jira)
            .unwrap();
        assert_eq!(jira_comments[0].body, "Jira comment");

        let page =
            classify_url("https://acme.atlassian.net/wiki/spaces/ENG/pages/456/Runbook").unwrap();
        let page_snapshot = dispatch(Some("acme.atlassian.net"), None)
            .fetch_snapshot(&page, 78)
            .unwrap();
        assert_eq!(page_snapshot.title, "Runbook");
        assert_eq!(page_snapshot.metadata[0].value, "9");
        let page_document = dispatch(Some("acme.atlassian.net"), None)
            .fetch_document_with_format(&page)
            .unwrap();
        assert_eq!(page_document.body_format, "html");
        assert_eq!(page_document.body, "<p>Safe <strong>body</strong></p>");
        assert_eq!(
            dispatch(Some("acme.atlassian.net"), None)
                .fetch_comments(&page)
                .unwrap()[0]
                .author,
            "Grace"
        );

        let pr = classify_url("https://bitbucket.org/team/repo/pull-requests/89").unwrap();
        let pr_snapshot = dispatch(None, Some("team"))
            .fetch_snapshot(&pr, 79)
            .unwrap();
        assert_eq!(pr_snapshot.title, "PR title");
        assert_eq!(
            dispatch(None, Some("team")).fetch_comments(&pr).unwrap()[0].body,
            "PR comment"
        );
        let args = fs::read_to_string(args_file).unwrap();
        assert!(args
            .contains("jira\nworkitem\nget\nPROJ-7\n--site\nacme.atlassian.net\n--output\njson"));
        assert!(args.contains(
            "confluence\ncontent\nget\n456\n--detail\nfull\n--site\nacme.atlassian.net\n--output\njson"
        ));
        assert!(args.contains("bitbucket\npull-requests\nget\nhttps://bitbucket.org/team/repo/pull-requests/89\n--workspace\nteam\n--output\njson"));
        assert!(args.contains("bitbucket\npull-requests\ncomment\nquery\nhttps://bitbucket.org/team/repo/pull-requests/89\n--workspace\nteam\n--output\njson"));
    }

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
    fn atlassian_and_azure_urls_share_provider_level_kinds_and_canonical_ids() {
        let jira = classify_url("https://ACME.atlassian.net/browse/PROJ-123/?x=1#comment")
            .expect("Jira URL should classify");
        assert_eq!(jira.provider, ExternalProvider::Atlassian);
        assert_eq!(jira.kind, ExternalObjectKind::Issue);
        assert_eq!(jira.external_key, "jira:acme#PROJ-123");
        assert_eq!(
            jira.canonical_url,
            "https://acme.atlassian.net/browse/PROJ-123"
        );

        let confluence =
            classify_url("https://ACME.atlassian.net/wiki/spaces/ENG/pages/456/Runbook?x=1#part")
                .expect("Confluence URL should classify");
        assert_eq!(confluence.provider, ExternalProvider::Atlassian);
        assert_eq!(confluence.kind, ExternalObjectKind::Document);
        assert_eq!(confluence.external_key, "confluence:acme#456");

        let bitbucket = classify_url("https://bitbucket.org/Ws/Repo/pull-requests/89/")
            .expect("Bitbucket URL should classify");
        assert_eq!(bitbucket.provider, ExternalProvider::Atlassian);
        assert_eq!(bitbucket.kind, ExternalObjectKind::PullRequest);
        assert_eq!(bitbucket.external_key, "bitbucket:ws/repo#89");

        let ado_issue = classify_url("https://dev.azure.com/ORG/Proj/_workitems/edit/123?x=1")
            .expect("ADO work item should classify");
        assert_eq!(ado_issue.provider, ExternalProvider::AzureDevOps);
        assert_eq!(ado_issue.kind, ExternalObjectKind::Issue);
        assert_eq!(ado_issue.external_key, "ado:org/proj#123");

        let ado_pr =
            classify_url("https://dev.azure.com/ORG/Proj/_git/Repo/pullrequest/456/#discussion")
                .expect("ADO pull request should classify");
        assert_eq!(ado_pr.provider, ExternalProvider::AzureDevOps);
        assert_eq!(ado_pr.kind, ExternalObjectKind::PullRequest);
        assert_eq!(ado_pr.external_key, "ado:org/proj#456");
    }

    #[test]
    fn local_markdown_identity_is_repository_relative_and_stable() {
        let directory = tempdir().expect("temporary repository should exist");
        let ticket = directory.path().join("docs/Issue 42.md");
        fs::create_dir_all(ticket.parent().expect("ticket parent should exist"))
            .expect("ticket directory should be created");
        fs::write(&ticket, "# Ticket").expect("ticket should be written");

        let object = classify_local_markdown(
            17,
            directory.path(),
            ticket.to_str().expect("ticket path should be UTF-8"),
        )
        .expect("Markdown in repository should classify");
        assert_eq!(object.provider, ExternalProvider::Generic);
        assert_eq!(object.kind, ExternalObjectKind::Generic);
        assert_eq!(object.external_key, "local:17#docs/Issue 42.md");
        assert!(object.canonical_url.ends_with("/docs/Issue%2042.md"));
        assert!(classify_local_markdown(17, directory.path(), "/tmp/outside.md").is_none());
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
    fn provider_dispatch_reads_issue_comments_from_the_context_executable() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("gh");
        let arguments = directory.path().join("arguments");
        let script = format!(
            r##"#!/bin/sh
printf '%s\n' "$*" >> '{}'
printf '%s' '[[{{"id":7,"user":{{"login":"octocat"}},"body":"Please fix this","created_at":"2026-09-19T12:00:00Z"}}],[{{"id":8,"user":{{"login":"hubot"}},"body":"Second page","created_at":"2026-09-20T12:00:00Z"}}]]'
"##,
            arguments.display()
        );
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");

        let object = classify_url("https://github.com/acme/app/issues/7")
            .expect("GitHub Issue should classify");
        let comments = ProviderDispatch::new(ExternalProvider::GitHub, Some(executable))
            .fetch_comments(&object)
            .expect("comments should be fetched");

        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author, "octocat");
        assert_eq!(comments[0].body, "Please fix this");
        assert_eq!(comments[0].created_at, "2026-09-19T12:00:00Z");
        assert_eq!(comments[1].author, "hubot");
        assert_eq!(comments[1].body, "Second page");
        assert_eq!(
            fs::read_to_string(arguments).unwrap().trim(),
            "api repos/acme/app/issues/7/comments --paginate --slurp"
        );
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

    #[cfg(unix)]
    #[test]
    fn github_cli_reads_an_issue_body_and_its_sub_issues() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary provider directory should exist");
        let executable = directory.path().join("gh");
        let script = r###"#!/bin/sh
if [ "$1" = issue ] && [ "$2" = view ]; then
  printf '%s' '{"body":"## Problem Statement\nDevTools open on every launch."}'
elif [ "$1" = api ] && [ "$2" = repos/acme/app/issues/7/sub_issues ]; then
  printf '%s\n' '{"html_url":"https://github.com/Acme/App/issues/8","number":8,"state":"open","title":"Gate DevTools"}'
  printf '%s\n' '{"html_url":"https://github.com/Acme/App/issues/9","number":9,"state":"closed","title":"Document the flag"}'
else
  exit 1
fi
"###;
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");

        let document = GithubCli::new(executable)
            .fetch_issue_document("https://github.com/acme/app/issues/7")
            .expect("the issue document should be read");

        assert_eq!(
            document.body,
            "## Problem Statement\nDevTools open on every launch."
        );
        assert_eq!(
            document.sub_issues,
            vec![
                SubIssue {
                    number: 8,
                    title: "Gate DevTools".into(),
                    state: "open".into(),
                    url: "https://github.com/acme/app/issues/8".into(),
                },
                SubIssue {
                    number: 9,
                    title: "Document the flag".into(),
                    state: "closed".into(),
                    url: "https://github.com/acme/app/issues/9".into(),
                },
            ]
        );
    }

    #[test]
    fn only_github_issues_have_an_issue_document() {
        let error = GithubCli::new(PathBuf::from("/nonexistent/gh"))
            .fetch_issue_document("https://github.com/acme/app/pull/7")
            .expect_err("a pull request is not an issue document");
        assert!(matches!(error, ProviderError::NotAnIssue));
    }
}
