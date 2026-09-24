use super::*;

impl Runtime {
    pub(crate) fn link_external_object(
        &mut self,
        item_id: i64,
        url: String,
    ) -> Result<ExternalLinkAction, String> {
        let object_input = classify_url(&url).map_err(|error| error.to_string())?;
        let known_object = self.state.external_objects.iter().find(|object| {
            object.provider == object_input.provider
                && object.external_key == object_input.external_key
        });
        let mut warning = None;
        let snapshot =
            if object_input.provider == ExternalProvider::GitHub && known_object.is_none() {
                match self.fetch_github_object(&object_input) {
                    Ok(snapshot) => Some(snapshot),
                    Err(error) => {
                        warning = Some(error);
                        None
                    }
                }
            } else {
                None
            };
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id,
                object: object_input,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| "Link creation produced no Link".to_owned())?;
        self.commit(decision)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| "Link creation produced no External Object".to_owned())?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    pub(crate) fn create_github_issue(
        &mut self,
        item_id: i64,
        repository_id: i64,
        title: String,
        body: String,
    ) -> Result<ExternalLinkAction, String> {
        const UNAVAILABLE_SCOPE: &str =
            "The Item or Repository is not available in the Item's Project";

        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == item.project_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        if !self
            .state
            .contexts
            .iter()
            .any(|context| context.id == project.context_id)
        {
            return Err(UNAVAILABLE_SCOPE.into());
        }
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        let repository_project = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == repository.project_id)
            .ok_or_else(|| UNAVAILABLE_SCOPE.to_owned())?;
        if repository.project_id != project.id
            || repository_project.context_id != project.context_id
        {
            return Err(UNAVAILABLE_SCOPE.into());
        }
        let repository_name = github_repository_name(&repository.remote_url).ok_or_else(|| {
            "The selected Repository does not have a valid GitHub remote".to_owned()
        })?;
        if title.trim().is_empty() {
            return Err("A GitHub Issue title is required".into());
        }

        let executable = self.gh_executable_path()?;
        let created_url = GithubCli::new(executable.clone())
            .create_issue(&repository_name, title.trim(), &body)
            .map_err(|error| error.to_string())?;
        let object = classify_url(&created_url).map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue {
            return Err(format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: GitHub CLI returned a URL that is not a GitHub Issue"
            ));
        }

        let mut warning = None;
        let snapshot = match GithubCli::new(executable).fetch(&object, current_unix_seconds()) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                warning = Some(error.to_string());
                None
            }
        };
        let decision = decide(
            self.state.clone(),
            Event::LinkExternalObject {
                item_id,
                object,
                snapshot,
            },
        )
        .map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| {
                format!(
                    "GitHub Issue was created at {created_url}, but linking it to the Item failed: Issue creation produced no Link"
                )
            })?;
        self.commit(decision).map_err(|error| {
            format!(
                "GitHub Issue was created at {created_url}, but linking it to the Item failed: {error}"
            )
        })?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| {
                format!(
                    "GitHub Issue was created at {created_url}, but linking it to the Item failed: Issue creation produced no External Object"
                )
            })?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    pub(crate) fn add_external_comment(
        &mut self,
        link_id: i64,
        body: String,
    ) -> Result<ExternalLinkView, String> {
        let body = body.trim().to_owned();
        if body.is_empty() {
            return Err("A GitHub comment cannot be blank".into());
        }
        let link = self
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| format!("Link {link_id} does not exist"))?;
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
            .cloned()
            .ok_or_else(|| "The linked External Object does not exist".to_owned())?;
        if object.provider != ExternalProvider::GitHub || object.kind == ExternalObjectKind::Generic
        {
            return Err("Comments are only supported for GitHub Issues and pull requests".into());
        }

        let executable = self.gh_executable_path()?;
        GithubCli::new(executable)
            .add_comment(&object.canonical_url, &body)
            .map_err(|error| error.to_string())?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| "Comment target produced no External Object".to_owned())
    }

    pub(crate) fn refresh_external_object(
        &mut self,
        external_object_id: i64,
    ) -> Result<ExternalSnapshot, String> {
        let object = self
            .state
            .external_objects
            .iter()
            .find(|object| object.id == external_object_id)
            .cloned()
            .ok_or_else(|| format!("External Object {external_object_id} does not exist"))?;
        if object.provider != ExternalProvider::GitHub {
            return Err("Only GitHub External Objects can be refreshed".into());
        }
        let input = ExternalObjectInput {
            provider: object.provider,
            kind: object.kind,
            external_key: object.external_key.clone(),
            canonical_url: object.canonical_url.clone(),
        };
        let snapshot = self.fetch_github_object(&input)?;
        let decision = decide(
            self.state.clone(),
            Event::RefreshExternalObject {
                external_object_id,
                snapshot,
            },
        )
        .map_err(|error| error.to_string())?;
        let snapshot = decision
            .state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == external_object_id)
            .cloned()
            .ok_or_else(|| "Refresh produced no External snapshot".to_owned())?;
        self.commit(decision)?;
        Ok(snapshot)
    }

    pub(crate) fn poll_external_objects(&mut self) -> PollResult {
        let mut object_ids = self
            .state
            .links
            .iter()
            .filter_map(|link| {
                self.state
                    .external_objects
                    .iter()
                    .find(|object| object.id == link.external_object_id)
                    .filter(|object| object.provider == ExternalProvider::GitHub)
                    .map(|object| object.id)
            })
            .collect::<Vec<_>>();
        object_ids.sort_unstable();
        object_ids.dedup();

        let mut result = PollResult {
            refreshed: 0,
            failures: Vec::new(),
        };
        for external_object_id in object_ids {
            match self.refresh_external_object(external_object_id) {
                Ok(_) => result.refreshed += 1,
                Err(error) => result.failures.push(PollFailure {
                    external_object_id,
                    error,
                }),
            }
        }
        result
    }

    fn fetch_github_object(
        &mut self,
        object: &ExternalObjectInput,
    ) -> Result<crate::domain::ExternalSnapshotData, String> {
        let executable = self.gh_executable_path()?;
        GithubCli::new(executable)
            .fetch(object, current_unix_seconds())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn gh_executable_path(&mut self) -> Result<PathBuf, String> {
        let stored_path = self.gh_executable_path.as_deref();
        let executable = resolve_gh_executable(stored_path).map_err(|error| error.to_string())?;
        if self.gh_executable_path.as_deref() != Some(executable.as_path()) {
            self.store
                .set_gh_executable_path(&executable)
                .map_err(|error| error.to_string())?;
            self.gh_executable_path = Some(executable.clone());
        }
        Ok(executable)
    }
}
