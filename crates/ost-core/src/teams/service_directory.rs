use super::*;

impl<'a> TeamsService<'a> {
    pub fn new(auth: &'a AuthService) -> Self {
        Self {
            auth,
            client: http_client(),
        }
    }

    pub async fn custom_reaction(&self, reaction_type: &str) -> Result<Option<CustomReaction>> {
        let Some((shortcut, document_id)) = custom_reaction_parts(reaction_type) else {
            return Ok(None);
        };
        let token = self.auth.skype_token().await?;
        let ams_base = self
            .auth
            .region_gtm("amsV2")
            .await
            .or(self.auth.region_gtm("ams").await)
            .unwrap_or_else(|| DEFAULT_AMS_SERVICE.to_owned())
            .trim_end_matches('/')
            .to_owned();
        let mut url = reqwest::Url::parse(&ams_base).context("Invalid Teams AMS service URL")?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid Teams AMS service URL"))?
            .extend(["v1", "objects", &document_id, "views", "imgt2_anim"]);
        let response = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_secs(5))
            .header(
                reqwest::header::AUTHORIZATION,
                microsoft_teams::ams_auth_value(&token),
            )
            .header(
                reqwest::header::COOKIE,
                microsoft_teams::asm_cookie_value(&token),
            )
            .header("Accept", "image/*")
            .header(reqwest::header::USER_AGENT, AMS_USER_AGENT)
            .send()
            .await
            .context("Could not retrieve the Teams custom reaction icon")?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response
            .error_for_status()
            .context("Teams rejected the custom reaction icon request")?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("image/png")
            .split(';')
            .next()
            .unwrap_or("image/png")
            .to_owned();
        if !content_type.starts_with("image/") {
            return Ok(None);
        }
        let bytes = response
            .bytes()
            .await
            .context("Could not read the Teams custom reaction icon")?;
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some(CustomReaction {
            reaction_type: reaction_type.trim().to_owned(),
            shortcut,
            document_id,
            content_type,
            content_base64: STANDARD.encode(bytes),
        }))
    }

    pub async fn list_teams(&self) -> Result<Vec<Team>> {
        self.list_teams_inner(true).await
    }

    pub async fn list_teams_uncached(&self) -> Result<Vec<Team>> {
        self.list_teams_inner(false).await
    }

    async fn list_teams_inner(&self, persist: bool) -> Result<Vec<Team>> {
        if persist
            && let Ok(cache) = team_cache().read()
            && let Some(cached) = cache.as_ref()
        {
            return Ok(cached.clone());
        }
        let teams: GraphCollection<GraphTeam> = self.graph_get("/me/joinedTeams").await?;
        let teams = futures::stream::iter(teams.value.into_iter().map(|team| async move {
            let path = format!("/teams/{}/channels", team.id);
            let channels = match self.graph_get::<GraphCollection<GraphChannel>>(&path).await {
                Ok(channels) => channels.value,
                Err(error) => {
                    tracing::warn!("Could not load channels for team {}: {error:#}", team.id);
                    Vec::new()
                }
            };
            Team {
                name: display_name_or_id(team.display_name.as_deref(), &team.id),
                id: team.id,
                channels: channels
                    .into_iter()
                    .map(|channel| Channel {
                        name: display_name_or_id(channel.display_name.as_deref(), &channel.id),
                        id: channel.id,
                    })
                    .collect(),
            }
        }))
        .buffered(8)
        .collect::<Vec<_>>()
        .await;
        if persist && let Ok(mut cache) = team_cache().write() {
            *cache = Some(teams.clone());
        }
        Ok(teams)
    }
}
