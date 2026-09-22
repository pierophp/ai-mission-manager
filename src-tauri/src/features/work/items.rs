use super::*;

impl Runtime {
    /// Shared lookup used by Workspace and Run workflows.
    pub(crate) fn item_context_id(&self, item_id: i64) -> Result<i64, String> {
        let item = self
            .state
            .items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| format!("Item {item_id} does not exist"))?;
        self.state
            .projects
            .iter()
            .find(|project| project.id == item.project_id)
            .map(|project| project.context_id)
            .ok_or_else(|| format!("Project {} does not exist", item.project_id))
    }

    pub(crate) fn create_item(
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

    pub(crate) fn update_item(&mut self, event: Event, item_id: i64) -> Result<Item, String> {
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

    pub(crate) fn set_item_relation(
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

    pub(crate) fn set_link_attention_policy(
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

    pub(crate) fn set_link_schedule(
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

    pub(crate) fn mark_link_reviewed(&mut self, link_id: i64) -> Result<ExternalLinkView, String> {
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
}
