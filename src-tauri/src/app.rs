use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::State;

use crate::{
    domain::{
        decide, external_link_view, home_view, search_items, AttachedRepositoryInput, Context,
        ContextAttentionDefault, DomainState, Event, ExternalChangePolicy, ExternalLinkView,
        ExternalObjectInput, ExternalObjectKind, ExternalProvider, ExternalSnapshot, HomeView,
        Item, ItemRelation, ItemRelationKind, ItemStatus, ItemView, Project, ProjectDefaults,
        Repository, Workset, WorksetRepositoryInput,
    },
    git::GitCli,
    persistence::SqliteStore,
    provider::{classify_url, resolve_gh_executable, GithubCli},
};

pub struct Runtime {
    store: SqliteStore,
    state: DomainState,
    gh_executable_path: Option<PathBuf>,
    pending_workset_removal: Option<WorksetRemovalReport>,
}

impl Runtime {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let mut store = SqliteStore::open(path).map_err(|error| error.to_string())?;
        let state = store.load_state().map_err(|error| error.to_string())?;
        let configured_gh_path = store
            .gh_executable_path()
            .map_err(|error| error.to_string())?;
        let gh_executable_path = resolve_gh_executable(configured_gh_path.as_deref()).ok();
        if gh_executable_path != configured_gh_path {
            if let Some(path) = gh_executable_path.as_deref() {
                store
                    .set_gh_executable_path(path)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(Self {
            store,
            state,
            gh_executable_path,
            pending_workset_removal: None,
        })
    }

    fn create_context(&mut self, name: String) -> Result<Context, String> {
        let decision = decide(self.state.clone(), Event::CreateContext { name })
            .map_err(|error| error.to_string())?;
        let context = decision
            .state
            .contexts
            .last()
            .cloned()
            .ok_or_else(|| "Context creation produced no Context".to_owned())?;
        self.commit(decision)?;
        Ok(context)
    }

    fn create_project(
        &mut self,
        name: String,
        context_id: i64,
        defaults: ProjectDefaults,
    ) -> Result<Project, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateProject {
                context_id,
                name,
                defaults,
            },
        )
        .map_err(|error| error.to_string())?;
        let project = decision
            .state
            .projects
            .last()
            .cloned()
            .ok_or_else(|| "Project creation produced no Project".to_owned())?;
        self.commit(decision)?;
        Ok(project)
    }

    fn register_repository(
        &mut self,
        project_id: i64,
        name: String,
        remote_url: String,
    ) -> Result<Repository, String> {
        let decision = decide(
            self.state.clone(),
            Event::RegisterRepository {
                project_id,
                name,
                remote_url,
            },
        )
        .map_err(|error| error.to_string())?;
        let repository = decision
            .state
            .repositories
            .last()
            .cloned()
            .ok_or_else(|| "Repository registration produced no Repository".to_owned())?;
        self.commit(decision)?;
        Ok(repository)
    }

    fn create_workset(
        &mut self,
        item_id: i64,
        root_directory: String,
        branch: String,
        repositories: Vec<WorksetRepositoryInput>,
    ) -> Result<Workset, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateWorkset {
                item_id,
                root_directory,
                branch,
                repositories,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .last()
            .cloned()
            .ok_or_else(|| "Workset creation produced no Workset".to_owned())?;
        let checkout = self.checkout_new_workset(&workset)?;
        if let Err(error) = self.commit(decision) {
            let cleanup_error = checkout.cleanup().err();
            return Err(format_commit_error(error, cleanup_error));
        }
        Ok(workset)
    }

    fn attach_workset(&mut self, item_id: i64, root_directory: String) -> Result<Workset, String> {
        let root = Path::new(&root_directory);
        let repositories = GitCli::system()
            .inspect_workset(root)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|repository| AttachedRepositoryInput {
                name: repository.name,
                remote_url: repository.remote_url,
                current_branch: repository.current_branch,
                is_dirty: repository.is_dirty,
            })
            .collect();
        let decision = decide(
            self.state.clone(),
            Event::AttachWorkset {
                item_id,
                root_directory,
                repositories,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .last()
            .cloned()
            .ok_or_else(|| "Workset attachment produced no Workset".to_owned())?;
        self.commit(decision)?;
        Ok(workset)
    }

    fn add_repository_to_workset(
        &mut self,
        workset_id: i64,
        repository_id: i64,
        branch_override: Option<String>,
        base_branch_override: Option<String>,
    ) -> Result<Workset, String> {
        let decision = decide(
            self.state.clone(),
            Event::AddRepositoryToWorkset {
                workset_id,
                repository_id,
                branch_override,
                base_branch_override,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id)
            .cloned()
            .ok_or_else(|| "Workset update produced no Workset".to_owned())?;
        let selected = workset
            .repositories
            .iter()
            .find(|selected| selected.repository_id == repository_id)
            .ok_or_else(|| "Workset update produced no Repository selection".to_owned())?;
        let repository = self
            .state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| "Workset update produced no Repository".to_owned())?;
        let destination = match self.checkout_repository(
            &workset,
            &repository,
            selected.branch_override.as_deref(),
            selected.base_branch_override.as_deref(),
        ) {
            Ok(destination) => destination,
            Err(error) => {
                let cleanup_error = CheckoutReceipt {
                    root: Path::new(&workset.root_directory).to_owned(),
                    root_was_created: false,
                    destinations: vec![Path::new(&workset.root_directory).join(&repository.name)],
                }
                .cleanup()
                .err();
                return Err(format_commit_error(error, cleanup_error));
            }
        };
        if let Err(error) = self.commit(decision) {
            let cleanup_error = CheckoutReceipt {
                root: Path::new(&workset.root_directory).to_owned(),
                root_was_created: false,
                destinations: vec![destination],
            }
            .cleanup()
            .err();
            return Err(format_commit_error(error, cleanup_error));
        }
        Ok(workset)
    }

    fn set_workset_archived(&mut self, workset_id: i64, archived: bool) -> Result<Workset, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetWorksetArchived {
                workset_id,
                archived,
            },
        )
        .map_err(|error| error.to_string())?;
        let workset = decision
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id)
            .cloned()
            .ok_or_else(|| "Workset archive update produced no Workset".to_owned())?;
        self.commit(decision)?;
        Ok(workset)
    }

    fn build_workset_removal_report(
        &self,
        workset_id: i64,
    ) -> Result<WorksetRemovalReport, String> {
        let workset = self
            .state
            .worksets
            .iter()
            .find(|workset| workset.id == workset_id)
            .ok_or_else(|| format!("Workset {workset_id} does not exist"))?;
        let root = Path::new(&workset.root_directory);
        let inspected = GitCli::system()
            .inspect_workset(root)
            .map_err(|error| error.to_string())?;
        let repositories = workset
            .repositories
            .iter()
            .map(|selected| {
                let repository = self.repository(selected.repository_id)?;
                let repository_state = inspected
                    .iter()
                    .find(|candidate| candidate.name == repository.name)
                    .ok_or_else(|| {
                        format!(
                            "Repository directory is missing from Workset: {}",
                            repository.name
                        )
                    })?;
                Ok(RepositoryRemovalReport {
                    repository_id: repository.id,
                    name: repository.name.clone(),
                    path: root.join(&repository.name).to_string_lossy().into_owned(),
                    current_branch: repository_state.current_branch.clone(),
                    unpushed_commits: repository_state.unpushed_commits.clone(),
                    unpushed_commits_unknown: repository_state.unpushed_commits_unknown,
                    uncommitted_changes: repository_state.uncommitted_changes.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(WorksetRemovalReport {
            workset_id,
            root_directory: workset.root_directory.clone(),
            repositories,
        })
    }

    fn prepare_workset_removal(&mut self, workset_id: i64) -> Result<WorksetRemovalReport, String> {
        let report = self.build_workset_removal_report(workset_id)?;
        self.pending_workset_removal = Some(report.clone());
        Ok(report)
    }

    fn remove_workset(&mut self, workset_id: i64, confirmed: bool) -> Result<(), String> {
        if !confirmed {
            return Err(
                "Workset removal requires explicit confirmation after reviewing its safety report"
                    .into(),
            );
        }
        let pending = self
            .pending_workset_removal
            .as_ref()
            .filter(|report| report.workset_id == workset_id)
            .cloned()
            .ok_or_else(|| {
                "Review the Workset removal safety report before removing it".to_owned()
            })?;
        let current = self.build_workset_removal_report(workset_id)?;
        if current != pending {
            return Err(
                "The Workset changed after the safety report; review the updated report before removing it"
                    .into(),
            );
        }

        let decision = decide(self.state.clone(), Event::RemoveWorkset { workset_id })
            .map_err(|error| error.to_string())?;
        let root = Path::new(&current.root_directory);
        let staging = workset_removal_staging_path(root, workset_id)?;
        fs::rename(root, &staging)
            .map_err(|error| format!("Could not stage Workset directory for removal: {error}"))?;
        if let Err(error) = self.commit(decision) {
            let restore_error = fs::rename(&staging, root).err().map(|restore_error| {
                format!(
                    "could not restore Workset directory after persistence failed: {restore_error}"
                )
            });
            return Err(format_commit_error(error, restore_error));
        }
        self.pending_workset_removal = None;
        fs::remove_dir_all(&staging).map_err(|error| {
            format!(
                "Workset was removed from Mission Manager, but its staged directory could not be deleted at {}: {error}",
                staging.display()
            )
        })
    }

    fn checkout_new_workset(&self, workset: &Workset) -> Result<CheckoutReceipt, String> {
        let root = Path::new(&workset.root_directory);
        let root_was_created = if root.exists() {
            if !root.is_dir() {
                return Err(format!(
                    "Workset root is not a directory: {}",
                    root.display()
                ));
            }
            if fs::read_dir(root)
                .map_err(|error| format!("Could not inspect Workset root: {error}"))?
                .next()
                .is_some()
            {
                return Err(format!("Workset root is not empty: {}", root.display()));
            }
            false
        } else {
            fs::create_dir_all(root)
                .map_err(|error| format!("Could not create Workset root: {error}"))?;
            true
        };

        let mut destinations = Vec::new();
        for selected in &workset.repositories {
            let repository = self.repository(selected.repository_id)?;
            let destination = root.join(&repository.name);
            if destination.exists() {
                if root_was_created {
                    let _ = fs::remove_dir(root);
                }
                return Err(format!(
                    "Repository checkout destination already exists: {}",
                    destination.display()
                ));
            }
            destinations.push((repository, selected.clone(), destination));
        }

        let mut attempted = Vec::new();
        for (repository, selected, destination) in destinations {
            attempted.push(destination.clone());
            if let Err(error) = self.checkout_repository(
                workset,
                &repository,
                selected.branch_override.as_deref(),
                selected.base_branch_override.as_deref(),
            ) {
                for attempted_destination in attempted.iter().rev() {
                    if attempted_destination.exists() {
                        let _ = fs::remove_dir_all(attempted_destination);
                    }
                }
                if root_was_created && root.exists() {
                    let is_empty = fs::read_dir(root)
                        .map(|mut entries| entries.next().is_none())
                        .unwrap_or(false);
                    if is_empty {
                        let _ = fs::remove_dir(root);
                    }
                }
                return Err(error);
            }
        }
        Ok(CheckoutReceipt {
            root: root.to_owned(),
            root_was_created,
            destinations: attempted,
        })
    }

    fn checkout_repository(
        &self,
        workset: &Workset,
        repository: &Repository,
        branch_override: Option<&str>,
        base_branch_override: Option<&str>,
    ) -> Result<PathBuf, String> {
        let branch = branch_override.unwrap_or(&workset.branch);
        let destination = Path::new(&workset.root_directory).join(&repository.name);
        GitCli::system()
            .checkout_repository(repository, &destination, branch, base_branch_override)
            .map_err(|error| error.to_string())
            .map(|()| destination)
    }

    fn repository(&self, repository_id: i64) -> Result<Repository, String> {
        self.state
            .repositories
            .iter()
            .find(|repository| repository.id == repository_id)
            .cloned()
            .ok_or_else(|| format!("Repository {repository_id} does not exist"))
    }

    fn create_item(
        &mut self,
        title: String,
        context_id: i64,
        project_id: i64,
    ) -> Result<Item, String> {
        let decision = decide(
            self.state.clone(),
            Event::CreateItem {
                title,
                context_id,
                project_id,
            },
        )
        .map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .last()
            .cloned()
            .ok_or_else(|| "Item creation produced no Item".to_owned())?;
        self.commit(decision)?;
        Ok(item)
    }

    fn update_item(&mut self, event: Event, item_id: i64) -> Result<Item, String> {
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        let item = decision
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .cloned()
            .ok_or_else(|| "Item update produced no Item".to_owned())?;
        self.commit(decision)?;
        Ok(item)
    }

    fn set_item_relation(
        &mut self,
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    ) -> Result<ItemRelation, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetItemRelation {
                from_item_id,
                to_item_id,
                kind,
            },
        )
        .map_err(|error| error.to_string())?;
        let relation = decision
            .state
            .relationships
            .last()
            .cloned()
            .ok_or_else(|| "Item relationship produced no relationship".to_owned())?;
        self.commit(decision)?;
        Ok(relation)
    }

    fn link_external_object(
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

    fn create_github_issue(
        &mut self,
        item_id: i64,
        repository: String,
        title: String,
        body: String,
    ) -> Result<ExternalLinkAction, String> {
        let item_exists = self.state.items.iter().any(|item| item.id == item_id);
        if !item_exists {
            return Err(format!("Item {item_id} does not exist"));
        }
        if repository.trim().is_empty() {
            return Err("A GitHub repository is required".into());
        }
        if title.trim().is_empty() {
            return Err("A GitHub Issue title is required".into());
        }

        let executable = self.gh_executable_path()?;
        let created_url = GithubCli::new(executable.clone())
            .create_issue(repository.trim(), title.trim(), &body)
            .map_err(|error| error.to_string())?;
        let object = classify_url(&created_url).map_err(|error| error.to_string())?;
        if object.provider != ExternalProvider::GitHub || object.kind != ExternalObjectKind::Issue {
            return Err("GitHub CLI returned a URL that is not a GitHub Issue".into());
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
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .last()
            .cloned()
            .ok_or_else(|| "Issue creation produced no Link".to_owned())?;
        self.commit(decision)?;
        let view = external_link_view(&self.state, &link)
            .ok_or_else(|| "Issue creation produced no External Object".to_owned())?;
        Ok(ExternalLinkAction {
            link: view,
            warning,
        })
    }

    fn add_external_comment(
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

    fn refresh_external_object(
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

    fn poll_external_objects(&mut self) -> PollResult {
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

    fn set_link_attention_policy(
        &mut self,
        link_id: i64,
        policy: Option<ExternalChangePolicy>,
    ) -> Result<ExternalLinkView, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetLinkAttentionPolicy { link_id, policy },
        )
        .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| "Link policy update produced no Link".to_owned())?;
        self.commit(decision)?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| "Link policy update produced no External Object".to_owned())
    }

    fn set_link_schedule(
        &mut self,
        event: Event,
        link_id: i64,
        error_prefix: &str,
    ) -> Result<ExternalLinkView, String> {
        let decision = decide(self.state.clone(), event).map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| format!("{error_prefix} produced no Link"))?;
        self.commit(decision)?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| format!("{error_prefix} produced no External Object"))
    }

    fn set_context_attention_default(
        &mut self,
        context_id: i64,
        object_kind: ExternalObjectKind,
        policy: ExternalChangePolicy,
    ) -> Result<ContextAttentionDefault, String> {
        let decision = decide(
            self.state.clone(),
            Event::SetContextAttentionDefault {
                context_id,
                object_kind,
                policy,
            },
        )
        .map_err(|error| error.to_string())?;
        let attention_default = decision
            .state
            .attention_defaults
            .iter()
            .find(|attention_default| {
                attention_default.context_id == context_id
                    && attention_default.object_kind == object_kind
            })
            .cloned()
            .ok_or_else(|| "Context default update produced no default".to_owned())?;
        self.commit(decision)?;
        Ok(attention_default)
    }

    fn mark_link_reviewed(&mut self, link_id: i64) -> Result<ExternalLinkView, String> {
        let decision = decide(self.state.clone(), Event::MarkLinkReviewed { link_id })
            .map_err(|error| error.to_string())?;
        let link = decision
            .state
            .links
            .iter()
            .find(|link| link.id == link_id)
            .cloned()
            .ok_or_else(|| "Mark reviewed produced no Link".to_owned())?;
        self.commit(decision)?;
        external_link_view(&self.state, &link)
            .ok_or_else(|| "Mark reviewed produced no External Object".to_owned())
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

    fn gh_executable_path(&mut self) -> Result<PathBuf, String> {
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

    fn commit(&mut self, decision: crate::domain::Decision) -> Result<(), String> {
        self.store
            .apply(&decision.effects)
            .map_err(|error| error.to_string())?;
        self.state = decision.state;
        Ok(())
    }
}

struct CheckoutReceipt {
    root: PathBuf,
    root_was_created: bool,
    destinations: Vec<PathBuf>,
}

impl CheckoutReceipt {
    fn cleanup(self) -> Result<(), String> {
        let mut errors = Vec::new();
        for destination in self.destinations.iter().rev() {
            if destination.exists() {
                if let Err(error) = fs::remove_dir_all(destination) {
                    errors.push(format!("{}: {error}", destination.display()));
                }
            }
        }
        if self.root_was_created && self.root.exists() {
            let is_empty = fs::read_dir(&self.root)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false);
            if is_empty {
                if let Err(error) = fs::remove_dir(&self.root) {
                    errors.push(format!("{}: {error}", self.root.display()));
                }
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!("could not remove checkout: {}", errors.join(", ")))
        }
    }
}

fn format_commit_error(error: String, cleanup_error: Option<String>) -> String {
    match cleanup_error {
        Some(cleanup_error) => format!("{error}; {cleanup_error}"),
        None => error,
    }
}

fn workset_removal_staging_path(root: &Path, workset_id: i64) -> Result<PathBuf, String> {
    let parent = root.parent().unwrap_or_else(|| Path::new("."));
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            format!(
                "Workset root has no usable directory name: {}",
                root.display()
            )
        })?;
    let staging = parent.join(format!(".{name}.mission-manager-removing-{workset_id}"));
    if staging.exists() {
        return Err(format!(
            "Workset removal staging path already exists: {}",
            staging.display()
        ));
    }
    Ok(staging)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RepositoryRemovalReport {
    pub repository_id: i64,
    pub name: String,
    pub path: String,
    pub current_branch: String,
    pub unpushed_commits: Vec<String>,
    pub unpushed_commits_unknown: bool,
    pub uncommitted_changes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorksetRemovalReport {
    pub workset_id: i64,
    pub root_directory: String,
    pub repositories: Vec<RepositoryRemovalReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExternalLinkAction {
    pub link: ExternalLinkView,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PollFailure {
    pub external_object_id: i64,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PollResult {
    pub refreshed: usize,
    pub failures: Vec<PollFailure>,
}

fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_contexts(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Context>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.contexts.clone())
}

#[tauri::command]
pub fn list_projects(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Project>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.projects.clone())
}

#[tauri::command]
pub fn list_repositories(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Repository>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.repositories.clone())
}

#[tauri::command]
pub fn list_context_attention_defaults(
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ContextAttentionDefault>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| runtime.state.attention_defaults.clone())
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_home(
    context_id: Option<i64>,
    now: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<HomeView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| home_view(&runtime.state, context_id, &now))
}

#[tauri::command(rename_all = "camelCase")]
pub fn search_items_command(
    query: String,
    context_id: Option<i64>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Vec<ItemView>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| search_items(&runtime.state, &query, context_id))
}

#[tauri::command]
pub fn create_context(name: String, state: State<'_, Mutex<Runtime>>) -> Result<Context, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_context(name)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_project(
    name: String,
    context_id: i64,
    default_item_status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Project, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_project(
            name,
            context_id,
            ProjectDefaults {
                item_status: default_item_status,
            },
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn register_repository(
    project_id: i64,
    name: String,
    remote_url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Repository, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .register_repository(project_id, name, remote_url)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_workset(
    item_id: i64,
    root_directory: String,
    branch: String,
    repositories: Vec<WorksetRepositoryInput>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_workset(item_id, root_directory, branch, repositories)
}

#[tauri::command(rename_all = "camelCase")]
pub fn attach_workset(
    item_id: i64,
    root_directory: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .attach_workset(item_id, root_directory)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_repository_to_workset(
    workset_id: i64,
    repository_id: i64,
    branch_override: Option<String>,
    base_branch_override: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .add_repository_to_workset(
            workset_id,
            repository_id,
            branch_override,
            base_branch_override,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_workset_archived(
    workset_id: i64,
    archived: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Workset, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_workset_archived(workset_id, archived)
}

#[tauri::command(rename_all = "camelCase")]
pub fn prepare_workset_removal(
    workset_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<WorksetRemovalReport, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .prepare_workset_removal(workset_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_workset(
    workset_id: i64,
    confirmed: bool,
    state: State<'_, Mutex<Runtime>>,
) -> Result<(), String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .remove_workset(workset_id, confirmed)
}

#[tauri::command]
pub fn list_inbox_items(state: State<'_, Mutex<Runtime>>) -> Result<Vec<Item>, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())
        .map(|runtime| {
            runtime
                .state
                .items
                .iter()
                .filter(|item| item.status == ItemStatus::Inbox)
                .cloned()
                .collect()
        })
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_item(
    title: String,
    context_id: i64,
    project_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_item(title, context_id, project_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_status(
    item_id: i64,
    status: ItemStatus,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemStatus { item_id, status }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_notes(
    item_id: i64,
    notes: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::SetItemNotes { item_id, notes }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_item_reminder(
    item_id: i64,
    remind_at: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(Event::AddItemReminder { item_id, remind_at }, item_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn remove_item_reminder(
    item_id: i64,
    reminder_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<Item, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .update_item(
            Event::RemoveItemReminder {
                item_id,
                reminder_id,
            },
            item_id,
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_item_relation(
    from_item_id: i64,
    to_item_id: i64,
    kind: ItemRelationKind,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ItemRelation, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_item_relation(from_item_id, to_item_id, kind)
}

#[tauri::command(rename_all = "camelCase")]
pub fn link_external_object(
    item_id: i64,
    url: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .link_external_object(item_id, url)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_github_issue(
    item_id: i64,
    repository: String,
    title: String,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkAction, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .create_github_issue(item_id, repository, title, body)
}

#[tauri::command(rename_all = "camelCase")]
pub fn add_external_comment(
    link_id: i64,
    body: String,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .add_external_comment(link_id, body)
}

#[tauri::command(rename_all = "camelCase")]
pub fn refresh_external_object(
    external_object_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalSnapshot, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .refresh_external_object(external_object_id)
}

#[tauri::command]
pub fn poll_external_objects(state: State<'_, Mutex<Runtime>>) -> Result<PollResult, String> {
    Ok(state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .poll_external_objects())
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_attention_policy(
    link_id: i64,
    policy: Option<ExternalChangePolicy>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_attention_policy(link_id, policy)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_watch_until(
    link_id: i64,
    watch_until: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_schedule(
            Event::SetLinkWatchUntil {
                link_id,
                watch_until,
            },
            link_id,
            "Setting watch period",
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_link_review_at(
    link_id: i64,
    review_at: Option<String>,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_schedule(
            Event::SetLinkReviewAt { link_id, review_at },
            link_id,
            "Setting review date",
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn clear_link_review_at(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_link_schedule(
            Event::ClearLinkReviewAt { link_id },
            link_id,
            "Clearing review date",
        )
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_context_attention_default(
    context_id: i64,
    object_kind: ExternalObjectKind,
    policy: ExternalChangePolicy,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ContextAttentionDefault, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .set_context_attention_default(context_id, object_kind, policy)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mark_link_reviewed(
    link_id: i64,
    state: State<'_, Mutex<Runtime>>,
) -> Result<ExternalLinkView, String> {
    state
        .lock()
        .map_err(|_| "Mission Manager state is unavailable".to_owned())?
        .mark_link_reviewed(link_id)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use tempfile::tempdir;

    use super::*;
    use crate::persistence::SqliteStore;

    #[cfg(unix)]
    #[test]
    fn creating_a_github_issue_links_it_without_replacing_item_context() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("temporary app directory should exist");
        let database = directory.path().join("mission-manager.sqlite");
        let executable = directory.path().join("gh");
        let script = r#"#!/bin/sh
if [ "$1" = api ] && [ "$2" = repos/acme/app/issues ]; then
  printf '%s' '{"html_url":"https://github.com/acme/app/issues/42"}'
elif [ "$1" = issue ] && [ "$2" = view ]; then
  printf '%s' '{"number":42,"title":"Created from Mission Manager","state":"OPEN","author":null,"labels":[],"milestone":null,"updatedAt":null}'
fi
"#;
        fs::write(&executable, script).expect("fake gh should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("fake gh should be executable");
        let mut store = SqliteStore::open(&database).expect("database should open");
        store
            .set_gh_executable_path(&executable)
            .expect("fake gh path should persist");
        drop(store);

        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Keep this Item".into(), 1, 1)
            .expect("Item should be created");
        runtime
            .update_item(
                Event::SetItemNotes {
                    item_id: 1,
                    notes: "Private handoff context".into(),
                },
                1,
            )
            .expect("Item notes should be saved");
        runtime
            .create_item("Related Item".into(), 1, 1)
            .expect("related Item should be created");
        runtime
            .set_item_relation(1, 2, ItemRelationKind::Blocks)
            .expect("Item relationship should be saved");

        let result = runtime
            .create_github_issue(
                1,
                "acme/app".into(),
                "Public title".into(),
                "Public body".into(),
            )
            .expect("GitHub Issue should be created and linked");

        assert_eq!(
            result.link.object.canonical_url,
            "https://github.com/acme/app/issues/42"
        );
        assert_eq!(runtime.state.items[0].human_identifier, "MC-1");
        assert_eq!(runtime.state.items[0].title, "Keep this Item");
        assert_eq!(runtime.state.items[0].notes, "Private handoff context");
        assert_eq!(runtime.state.relationships.len(), 1);
        assert_eq!(runtime.state.links[0].item_id, 1);
        assert_eq!(
            runtime.state.snapshots[0].title,
            "Created from Mission Manager"
        );
    }

    #[test]
    fn creating_a_workset_checks_out_a_repository_and_persists_the_execution_root() {
        let directory = tempdir().expect("temporary app directory should exist");
        let seed = directory.path().join("seed");
        run_git(directory.path(), &["init", "--initial-branch=main", "seed"]);
        run_git(&seed, &["config", "user.email", "test@example.com"]);
        run_git(&seed, &["config", "user.name", "Test User"]);
        fs::write(seed.join("README.md"), "service-a\n").expect("seed file should be written");
        run_git(&seed, &["add", "README.md"]);
        run_git(&seed, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        let origin_url = origin.to_string_lossy().into_owned();
        run_git(&seed, &["remote", "add", "origin", &origin_url]);
        run_git(&seed, &["push", "origin", "main"]);

        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .register_repository(1, "service-a".into(), origin.to_string_lossy().into_owned())
            .expect("Repository should register");
        runtime
            .create_item("Implement the platform change".into(), 1, 1)
            .expect("Item should be created");

        let root = directory.path().join("workset-root");
        let workset = runtime
            .create_workset(
                1,
                root.to_string_lossy().into_owned(),
                "feature/platform-change".into(),
                vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: Some("main".into()),
                }],
            )
            .expect("Workset should be created");

        assert_eq!(workset.branch, "feature/platform-change");
        assert!(root.join("service-a/README.md").is_file());
        assert_eq!(
            run_git_output(&root.join("service-a"), &["branch", "--show-current"]),
            "feature/platform-change"
        );
        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.worksets, vec![workset]);
    }

    #[test]
    fn attaching_a_workset_does_not_change_its_branch_configuration_or_files() {
        let directory = tempdir().expect("temporary app directory should exist");
        let root = directory.path().join("workset-root");
        fs::create_dir(&root).expect("Workset root should exist");
        let repository = root.join("service-a");
        run_git(&root, &["init", "--initial-branch=main", "service-a"]);
        run_git(&repository, &["config", "user.email", "test@example.com"]);
        run_git(&repository, &["config", "user.name", "Test User"]);
        fs::write(repository.join("README.md"), "service-a\n")
            .expect("repository file should be written");
        run_git(&repository, &["add", "README.md"]);
        run_git(&repository, &["commit", "-m", "initial"]);
        let origin = directory.path().join("service-a.git");
        run_git(directory.path(), &["init", "--bare", "service-a.git"]);
        let origin_url = origin.to_string_lossy().into_owned();
        run_git(&repository, &["remote", "add", "origin", &origin_url]);
        fs::write(repository.join("notes.txt"), "keep this work\n")
            .expect("uncommitted file should be written");

        let branch_before = run_git_output(&repository, &["branch", "--show-current"]);
        let config_before = run_git_output(&repository, &["config", "--local", "--list"]);
        let status_before = run_git_output(&repository, &["status", "--porcelain"]);
        let database = directory.path().join("mission-manager.sqlite");
        let mut runtime = Runtime::open(&database).expect("runtime should open");
        runtime
            .create_item("Adopt the platform change".into(), 1, 1)
            .expect("Item should be created");

        let workset = runtime
            .attach_workset(1, root.to_string_lossy().into_owned())
            .expect("Workset should attach");

        assert_eq!(workset.branch, "main");
        assert_eq!(workset.repositories[0].current_branch, "main");
        assert!(workset.repositories[0].is_dirty);
        assert_eq!(
            run_git_output(&repository, &["branch", "--show-current"]),
            branch_before
        );
        assert_eq!(
            run_git_output(&repository, &["config", "--local", "--list"]),
            config_before
        );
        assert_eq!(
            run_git_output(&repository, &["status", "--porcelain"]),
            status_before
        );
        let reopened = Runtime::open(&database).expect("runtime should reopen");
        assert_eq!(reopened.state.worksets, vec![workset]);
        assert_eq!(reopened.state.repositories[0].remote_url, origin_url);

        runtime
            .set_workset_archived(1, true)
            .expect("Workset should be archivable");
        assert!(root.is_dir(), "archiving must leave the Workset on disk");
        assert!(runtime.state.worksets[0].archived);

        let report = runtime
            .prepare_workset_removal(1)
            .expect("removal safety report should be available");
        assert_eq!(report.repositories.len(), 1);
        assert!(!report.repositories[0].uncommitted_changes.is_empty());
        assert!(runtime.remove_workset(1, false).is_err());
        assert!(
            root.is_dir(),
            "a rejected confirmation must keep the Workset"
        );

        runtime
            .remove_workset(1, true)
            .expect("confirmed removal should delete the Workset");
        assert!(!root.exists());
        assert!(runtime.state.worksets.is_empty());
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
}
