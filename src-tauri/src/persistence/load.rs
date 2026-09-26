use super::codecs::*;
use super::*;

impl SqliteStore {
    pub fn load_state(&self) -> Result<DomainState, StoreError> {
        let next_context_id = self.sequence("next_context_id")?;
        let next_project_id = self.sequence("next_project_id")?;
        let next_item_id = self.sequence("next_item_id")?;
        let next_item_number = self.sequence("next_item_number")?;
        let next_repository_id = self.sequence("next_repository_id")?;
        let next_workspace_id = self.sequence("next_workspace_id")?;
        let next_worktree_id = self.sequence("next_worktree_id")?;
        let next_machine_id = self.sequence("next_machine_id")?;
        let next_run_id = self.sequence("next_run_id")?;
        let next_external_object_id = self.sequence("next_external_object_id")?;
        let next_link_id = self.sequence("next_link_id")?;
        let next_activity_id = self.sequence("next_activity_id")?;
        let next_reminder_id = self.sequence("next_reminder_id")?;
        let contexts = {
            let mut statement = self.connection.prepare(
                "SELECT id, name, execution_machine_id, grill_agent, grill_model, grill_effort,
                        implement_agent, implement_model, implement_effort
                     FROM contexts ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let agent: String = row.get(3)?;
                let model: String = row.get(4)?;
                let effort: String = row.get(5)?;
                let implement_agent: String = row.get(6)?;
                let implement_model: String = row.get(7)?;
                let implement_effort: String = row.get(8)?;
                Ok(Context {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    execution_machine_id: row.get(2)?,
                    grill_defaults: GrillConfiguration {
                        agent: parse_agent_kind(&agent).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        model,
                        effort,
                    },
                    implement_defaults: GrillConfiguration {
                        agent: parse_agent_kind(&implement_agent).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                6,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        model: implement_model,
                        effort: implement_effort,
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let projects = {
            let mut statement = self.connection.prepare(
                "SELECT id, context_id, name, default_item_status, default_execution_mode
                 FROM projects
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let status: String = row.get(3)?;
                let execution_mode: String = row.get(4)?;
                Ok(Project {
                    id: row.get(0)?,
                    context_id: row.get(1)?,
                    name: row.get(2)?,
                    defaults: ProjectDefaults {
                        item_status: parse_project_default_status(&status).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        execution_mode: parse_execution_mode(&execution_mode).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                4,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let repositories = {
            let mut statement = self.connection.prepare(
                "SELECT id, project_id, name, remote_url, base_branch
                 FROM repositories
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(Repository {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    remote_url: row.get(3)?,
                    base_branch: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let repository_locations = {
            let mut statement = self.connection.prepare(
                "SELECT repository_id, machine_id, checkout_path, worktree_root
                 FROM repository_locations
                 ORDER BY repository_id, machine_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(RepositoryLocation {
                    repository_id: row.get(0)?,
                    machine_id: row.get(1)?,
                    checkout_path: row.get(2)?,
                    worktree_root: row.get(3)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let machines = {
            let mut statement = self.connection.prepare(
                "SELECT id, context_id, name, socket_name, transport_json, last_observed,
                        last_observed_at
                 FROM machines
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let transport: String = row.get(4)?;
                let observation: String = row.get(5)?;
                Ok(Machine {
                    id: row.get(0)?,
                    context_id: row.get(1)?,
                    name: row.get(2)?,
                    socket_name: row.get(3)?,
                    transport: serde_json::from_str(&transport).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(StoreError::InvalidMachineTransport(error.to_string())),
                        )
                    })?,
                    last_observed: parse_machine_observation(&observation).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    last_observed_at: row.get(6)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut items = {
            let mut statement = self.connection.prepare(
                "SELECT id, human_identifier, title, project_id, status, notes
                 FROM items
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let status: String = row.get(4)?;
                Ok(Item {
                    id: row.get(0)?,
                    human_identifier: row.get(1)?,
                    title: row.get(2)?,
                    project_id: row.get(3)?,
                    status: parse_item_status(&status).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    notes: row.get(5)?,
                    reminders: Vec::new(),
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let reminders = {
            let mut statement = self.connection.prepare(
                "SELECT id, item_id, remind_at
                 FROM reminders
                 ORDER BY item_id, id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(1)?,
                    Reminder {
                        id: row.get(0)?,
                        remind_at: row.get(2)?,
                    },
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (item_id, reminder) in reminders {
            if let Some(item) = items.iter_mut().find(|item| item.id == item_id) {
                item.reminders.push(reminder);
            }
        }
        let mut workspaces = {
            let mut statement = self.connection.prepare(
                "SELECT id, item_id, preparation_state
                 FROM workspaces
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let preparation_state: String = row.get(2)?;
                Ok(Workspace {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    repositories: Vec::new(),
                    preparation_state: parse_workspace_preparation_state(&preparation_state)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let workspace_repositories = {
            let mut statement = self.connection.prepare(
                "SELECT workspace_id, repository_id, branch, base_branch
                 FROM workspace_repositories
                 ORDER BY workspace_id, repository_id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    WorkspaceRepository {
                        repository_id: row.get(1)?,
                        branch: row.get(2)?,
                        base_branch: row.get(3)?,
                    },
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (workspace_id, repository) in workspace_repositories {
            if let Some(workspace) = workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
            {
                workspace.repositories.push(repository);
            }
        }
        let worktrees = {
            let mut statement = self.connection.prepare(
                "SELECT id, workspace_id, repository_id, machine_id, path, branch,
                        base_branch, is_dirty
                 FROM worktrees
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                Ok(Worktree {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    repository_id: row.get(2)?,
                    machine_id: row.get(3)?,
                    path: row.get(4)?,
                    branch: row.get(5)?,
                    base_branch: row.get(6)?,
                    is_dirty: row.get::<_, i64>(7)? != 0,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let runs = {
            let mut statement = self.connection.prepare(
                "SELECT id, item_id, workspace_id, repository_id, worktree_id,
                        machine_id, agent, execution_profile, model, effort, skill_snapshot,
                        prompt, working_directory, session_name, pane_id, started_at, state,
                        last_applied_agent_state_sequence, pane_status,
                        direct_checkouts_json, transcript,
                        grill_question_group_json, grill_answers_json, grill_decisions_json,
                        grill_response, grill_phase, grill_action, grill_action_started_at
                 FROM runs
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let agent: String = row.get(6)?;
                let execution_profile: String = row.get(7)?;
                let direct_checkouts_json: String = row.get(19)?;
                let grill_question_group_json: Option<String> = row.get(21)?;
                let grill_answers_json: String = row.get(22)?;
                let grill_decisions_json: String = row.get(23)?;
                let grill_phase: Option<String> = row.get(25)?;
                let grill_action: Option<String> = row.get(26)?;
                let grill_question_group = grill_question_group_json
                    .map(|json| {
                        serde_json::from_str::<GrillQuestionGroup>(&json).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                21,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })
                    })
                    .transpose()?;
                let grill_answers = serde_json::from_str::<Vec<GrillAnswer>>(&grill_answers_json)
                    .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        22,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                let grill_decisions = serde_json::from_str::<Vec<GrillAnswer>>(
                    &grill_decisions_json,
                )
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        23,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                let grill_phase = grill_phase
                    .map(|phase| parse_grill_phase(&phase))
                    .transpose()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            25,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                let grill_action = grill_action
                    .map(|action| parse_grill_continuation_action(&action))
                    .transpose()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            26,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                Ok(Run {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    workspace_id: row.get(2)?,
                    repository_id: row.get(3)?,
                    worktree_id: row.get(4)?,
                    machine_id: row.get(5)?,
                    agent: parse_agent_kind(&agent).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    execution_profile: parse_execution_profile(&execution_profile).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                7,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    model: row.get(8)?,
                    effort: row.get(9)?,
                    skill_snapshot: row.get(10)?,
                    prompt: row.get(11)?,
                    working_directory: row.get(12)?,
                    session_name: row.get(13)?,
                    pane_id: row.get(14)?,
                    started_at: row.get(15)?,
                    state: parse_run_state(&row.get::<_, String>(16)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            16,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    last_applied_agent_state_sequence: row.get(17)?,
                    pane_status: parse_run_pane_status(&row.get::<_, String>(18)?).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                18,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    direct_checkouts: serde_json::from_str(&direct_checkouts_json).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                18,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    transcript: row.get(20)?,
                    grill_question_group,
                    grill_answers,
                    grill_decisions,
                    grill_response: row.get(24)?,
                    grill_phase,
                    grill_action,
                    grill_action_started_at: row.get(27)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let relationships = {
            let mut statement = self.connection.prepare(
                "SELECT from_item_id, to_item_id, kind
                 FROM item_relationships
                 ORDER BY from_item_id, to_item_id, kind",
            )?;
            let rows = statement.query_map([], |row| {
                let kind: String = row.get(2)?;
                Ok(ItemRelation {
                    from_item_id: row.get(0)?,
                    to_item_id: row.get(1)?,
                    kind: parse_item_relation_kind(&kind).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let external_objects = {
            let mut statement = self.connection.prepare(
                "SELECT id, provider, kind, external_key, canonical_url
                 FROM external_objects
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let provider: String = row.get(1)?;
                let kind: String = row.get(2)?;
                Ok(ExternalObject {
                    id: row.get(0)?,
                    provider: parse_external_provider(&provider).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    kind: parse_external_object_kind(&kind).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    external_key: row.get(3)?,
                    canonical_url: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let links = {
            let mut statement = self.connection.prepare(
                "SELECT external_links.id, external_links.item_id, external_links.external_object_id,
                        COALESCE(link_attention_state.reviewed_activity_id, 0),
                        link_attention_state.title_attention,
                        link_attention_state.state_attention,
                        link_attention_state.metadata_attention,
                        link_attention_state.watch_until,
                        link_attention_state.review_at,
                        link_attention_state.provenance_json,
                        external_links.is_spec
                 FROM external_links
                 LEFT JOIN link_attention_state
                   ON link_attention_state.link_id = external_links.id
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let title_attention: Option<i64> = row.get(4)?;
                let state_attention: Option<i64> = row.get(5)?;
                let metadata_attention: Option<i64> = row.get(6)?;
                let provenance_json: Option<String> = row.get(9)?;
                let attention_policy = match (title_attention, state_attention, metadata_attention)
                {
                    (Some(title), Some(state), Some(metadata)) => Some(ExternalChangePolicy {
                        title: title != 0,
                        state: state != 0,
                        metadata: metadata != 0,
                    }),
                    _ => None,
                };
                let provenance = provenance_json
                    .map(|json| serde_json::from_str::<LinkProvenance>(&json))
                    .transpose()
                    .map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            9,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                Ok(Link {
                    id: row.get(0)?,
                    item_id: row.get(1)?,
                    external_object_id: row.get(2)?,
                    reviewed_activity_id: row.get(3)?,
                    attention_policy,
                    watch_until: row.get(7)?,
                    review_at: row.get(8)?,
                    is_spec: row.get::<_, i64>(10)? != 0,
                    provenance,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let snapshots = {
            let mut statement = self.connection.prepare(
                "SELECT external_object_id, title, state, metadata_json, fetched_at
                 FROM external_snapshots
                 ORDER BY external_object_id",
            )?;
            let rows = statement.query_map([], |row| {
                let metadata_json: String = row.get(3)?;
                Ok(ExternalSnapshot {
                    external_object_id: row.get(0)?,
                    title: row.get(1)?,
                    state: row.get(2)?,
                    metadata: serde_json::from_str::<Vec<ExternalMetadata>>(&metadata_json)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    fetched_at: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let activities = {
            let mut statement = self.connection.prepare(
                "SELECT id, external_object_id, observed_at, changes_json
                 FROM activities
                 ORDER BY id",
            )?;
            let rows = statement.query_map([], |row| {
                let changes_json: String = row.get(3)?;
                Ok(Activity {
                    id: row.get(0)?,
                    external_object_id: row.get(1)?,
                    observed_at: row.get(2)?,
                    changes: serde_json::from_str(&changes_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let attention_defaults = {
            let mut statement = self.connection.prepare(
                "SELECT context_id, object_kind, title_attention, state_attention,
                        metadata_attention
                 FROM context_attention_defaults
                 ORDER BY context_id, object_kind",
            )?;
            let rows = statement.query_map([], |row| {
                let object_kind: String = row.get(1)?;
                Ok(ContextAttentionDefault {
                    context_id: row.get(0)?,
                    object_kind: parse_external_object_kind(&object_kind).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    policy: ExternalChangePolicy {
                        title: row.get::<_, i64>(2)? != 0,
                        state: row.get::<_, i64>(3)? != 0,
                        metadata: row.get::<_, i64>(4)? != 0,
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let implementation_queues = {
            let mut statement = self
                .connection
                .prepare("SELECT queue_json FROM implementation_queues ORDER BY id")?;
            let rows = statement.query_map([], |row| {
                let json: String = row.get(0)?;
                serde_json::from_str::<ImplementationQueue>(&json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        Ok(DomainState {
            next_context_id,
            next_project_id,
            next_item_id,
            next_item_number,
            next_repository_id,
            next_workspace_id,
            next_worktree_id,
            next_machine_id,
            next_run_id,
            next_external_object_id,
            next_link_id,
            next_activity_id,
            next_reminder_id,
            contexts,
            projects,
            repositories,
            repository_locations,
            items,
            workspaces,
            worktrees,
            machines,
            runs,
            implementation_queues,
            relationships,
            external_objects,
            links,
            snapshots,
            activities,
            attention_defaults,
        })
    }
}
