use super::*;

impl<'a> TeamsService<'a> {
    pub async fn presences(&self, user_ids: &[String]) -> Result<Vec<UserPresence>> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = &user_ids[..user_ids.len().min(650)];
        match self.native_presences(ids).await {
            Ok(presences) => return Ok(presences),
            Err(error) => tracing::warn!("Could not load presence from Teams: {error:#}"),
        }
        let response: GraphCollection<GraphPresence> = self
            .graph_post(
                microsoft_teams::GRAPH_BULK_PRESENCE_PATH,
                &GraphPresenceRequest { ids },
            )
            .await?;
        Ok(response
            .value
            .into_iter()
            .map(|presence| UserPresence {
                user_id: presence.id,
                availability: presence.availability,
                activity: presence.activity,
            })
            .collect())
    }

    async fn native_presences(&self, user_ids: &[String]) -> Result<Vec<UserPresence>> {
        let requests = user_ids
            .iter()
            .map(|user_id| NativePresenceRequest {
                mri: format!("8:orgid:{user_id}"),
            })
            .collect::<Vec<_>>();
        let response = self
            .client
            .post(format!(
                "{}{}",
                microsoft_teams::PRESENCE_BASE,
                microsoft_teams::PRESENCE_GET_PATH
            ))
            .bearer_auth(self.auth.teams_access_token().await?)
            .json(&requests)
            .send()
            .await
            .context("Could not reach the Teams presence service")?
            .error_for_status()
            .context("Teams presence service rejected the request")?
            .json::<Vec<NativePresenceResponse>>()
            .await
            .context("Could not parse the Teams presence response")?;
        Ok(response
            .into_iter()
            .filter_map(|item| {
                let presence = item.presence?;
                Some(UserPresence {
                    user_id: item
                        .mri
                        .strip_prefix("8:orgid:")
                        .unwrap_or(&item.mri)
                        .to_owned(),
                    availability: presence.availability,
                    activity: presence.activity,
                })
            })
            .collect())
    }

    pub async fn user_profile(&self) -> Result<UserProfile> {
        let profile = self.current_user().await?;
        Ok(UserProfile {
            id: profile.id,
            display_name: profile.display_name.unwrap_or_else(|| "User".to_owned()),
            email: profile.mail.or(profile.user_principal_name),
        })
    }

    pub async fn user_details(&self, user_id: &str) -> Result<UserDetails> {
        let profile: GraphUser = self
            .graph_get(&microsoft_teams::graph_user_path(user_id))
            .await?;
        let graph_presence = self
            .graph_get::<GraphPresence>(&microsoft_teams::graph_user_presence_path(user_id))
            .await
            .ok();
        let fallback_presence = if graph_presence.is_none() {
            self.presences(&[user_id.to_owned()])
                .await
                .ok()
                .and_then(|presences| presences.into_iter().next())
        } else {
            None
        };
        let (availability, activity, status_message) = match graph_presence {
            Some(presence) => (
                Some(presence.availability),
                Some(presence.activity),
                presence
                    .status_message
                    .and_then(|status| status.message)
                    .and_then(|message| message.content)
                    .filter(|message| !message.trim().is_empty()),
            ),
            None => (
                fallback_presence
                    .as_ref()
                    .map(|presence| presence.availability.clone()),
                fallback_presence.map(|presence| presence.activity),
                None,
            ),
        };
        Ok(UserDetails {
            user_id: profile.id.unwrap_or_else(|| user_id.to_owned()),
            display_name: profile
                .display_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| user_id.to_owned()),
            email: profile
                .mail
                .or(profile.user_principal_name)
                .filter(|email| !email.trim().is_empty()),
            job_title: profile.job_title.filter(|title| !title.trim().is_empty()),
            availability,
            activity,
            status_message,
        })
    }

    async fn graph_photo(
        &self,
        paths: impl IntoIterator<Item = String>,
        kind: &str,
    ) -> Result<Option<String>> {
        let token = self.auth.graph_token().await?;
        for path in paths {
            let response = self
                .client
                .get(format!("{GRAPH_BASE}{path}"))
                .bearer_auth(&token)
                .send()
                .await
                .with_context(|| format!("Could not reach Microsoft Graph for the {kind} photo"))?;
            if !response.status().is_success() {
                tracing::debug!(
                    "Microsoft Graph {kind} photo endpoint {path} returned {}",
                    response.status()
                );
                continue;
            }
            return Ok(Some(STANDARD.encode(response.bytes().await.with_context(
                || format!("Could not read the Microsoft {kind} photo"),
            )?)));
        }
        Ok(None)
    }

    pub async fn profile_photo(&self, user_id: &str) -> Result<Option<String>> {
        self.graph_photo(
            [
                format!("/users/{user_id}/photo/$value"),
                format!("/users/{user_id}/photos/48x48/$value"),
            ],
            "profile",
        )
        .await
    }

    pub async fn team_photo(&self, team_id: &str) -> Result<Option<String>> {
        self.graph_photo(
            [
                format!("/teams/{team_id}/photo/$value"),
                format!("/groups/{team_id}/photo/$value"),
            ],
            "team",
        )
        .await
    }

    pub async fn chat_photo(&self, chat_id: &str) -> Result<Option<String>> {
        self.graph_photo([format!("/chats/{chat_id}/photo/$value")], "chat")
            .await
    }
}
