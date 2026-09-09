use super::super::owner_operation_transport::OwnerOperationTransport;
use super::grants::RepositoryAnchor;
use crate::app_state::owner_scope::{self as app_state_scope, OwnerScopeToken};
use nostr::Event;
use serde_json::json;
use tauri::Manager;

pub(super) trait Access {
    fn token(&self) -> &OwnerScopeToken;
    async fn current(&self) -> Result<(), String>;
    async fn repository(&self, coordinate: &str) -> Result<RepositoryAnchor, String>;
    async fn head(&self, coordinate: &str, expected_id: &str) -> Result<(), String>;
    async fn snapshot_inputs(
        &self,
        coordinate: &str,
    ) -> Result<(serde_json::Value, serde_json::Value), String>;
}

pub(super) struct NativeAccess<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    token: OwnerScopeToken,
    transport: OwnerOperationTransport,
}
impl<R: tauri::Runtime> NativeAccess<R> {
    pub(super) async fn capture(
        app: tauri::AppHandle<R>,
        expected: OwnerScopeToken,
    ) -> Result<Self, String> {
        let captured = app_state_scope::capture(app.clone()).await?;
        if captured.token != expected {
            return Err(app_state_scope::OWNER_SCOPE_STALE.into());
        }
        let transport = OwnerOperationTransport::captured(
            &app.state::<crate::AppState>(),
            captured.token.scope.community.clone(),
            captured.keys,
            None,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            app,
            token: captured.token,
            transport,
        })
    }
}
impl<R: tauri::Runtime> Access for NativeAccess<R> {
    fn token(&self) -> &OwnerScopeToken {
        &self.token
    }
    async fn current(&self) -> Result<(), String> {
        app_state_scope::assert_current(self.app.clone(), &self.token).await
    }
    async fn repository(&self, coordinate: &str) -> Result<RepositoryAnchor, String> {
        let (owner, d) = coordinate_parts(coordinate)?;
        let events = self
            .transport
            .query(
                json!({"kinds":[30617],"authors":[owner],"#d":[d],"limit":2}),
                self.current(),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.current().await?;
        let event = exact_event(&events, 30617, owner, d)?;
        exact_coordinate_tag(event, coordinate)?;
        let modes: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| {
                tag.as_slice()
                    .first()
                    .is_some_and(|name| name == "crew-workspace-mode")
            })
            .collect();
        let mode = match modes.as_slice() {
            [] => "git",
            [tag]
                if tag.as_slice().len() == 2
                    && matches!(tag.as_slice()[1].as_str(), "git" | "folder") =>
            {
                tag.as_slice()[1].as_str()
            }
            _ => return Err("Repository workspace mode is invalid".into()),
        };
        Ok(RepositoryAnchor {
            coordinate: coordinate.into(),
            event_id: event.id.to_hex(),
            workspace_mode: mode.into(),
            label: d.into(),
        })
    }
    async fn head(&self, coordinate: &str, expected_id: &str) -> Result<(), String> {
        let (owner, repo) = coordinate_parts(coordinate)?;
        let d = format!("{repo}/_toc");
        let events = self
            .transport
            .query(
                json!({"kinds":[30623],"authors":[owner],"#d":[d],"limit":2}),
                self.current(),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.current().await?;
        let event = exact_event(&events, 30623, owner, &d)?;
        exact_coordinate_tag(event, coordinate)?;
        if event.id.to_hex() != expected_id {
            return Err("Source snapshot is not the current accepted Wiki".into());
        }
        Ok(())
    }

    async fn snapshot_inputs(
        &self,
        coordinate: &str,
    ) -> Result<(serde_json::Value, serde_json::Value), String> {
        let (owner, repo) = coordinate_parts(coordinate)?;
        let head_d = format!("{repo}/_toc");
        let heads = self
            .transport
            .query(
                json!({"kinds":[30623],"authors":[owner],"#d":[head_d],"limit":2}),
                self.current(),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.current().await?;
        let head = exact_event(&heads, 30623, owner, &head_d)?;
        exact_coordinate_tag(head, coordinate)?;
        let manifest_tag = head
            .tags
            .iter()
            .find(|tag| {
                tag.as_slice()
                    .first()
                    .is_some_and(|name| name == "wiki-manifest")
            })
            .ok_or("Live Wiki manifest reference is unavailable")?;
        if manifest_tag.as_slice().len() != 3 {
            return Err("Live Wiki manifest reference is invalid".into());
        }
        let manifest_id = manifest_tag.as_slice()[1].clone();
        let manifest_digest = manifest_tag.as_slice()[2].clone();
        if !is_hex(&manifest_id, 64) || !is_hex(&manifest_digest, 64) {
            return Err("Live Wiki manifest reference is invalid".into());
        }
        let manifest_d = format!("{repo}/m1-{manifest_digest}");
        let manifests = self
            .transport
            .query(
                json!({"kinds":[30623],"authors":[owner],"#d":[manifest_d],"ids":[manifest_id],"limit":1}),
                self.current(),
            )
            .await
            .map_err(|error| error.to_string())?;
        self.current().await?;
        let manifest = exact_event(&manifests, 30623, owner, &manifest_d)?;
        exact_coordinate_tag(manifest, coordinate)?;
        if manifest.id.to_hex() != manifest_tag.as_slice()[1] {
            return Err("Live Wiki manifest identity changed".into());
        }
        Ok((
            serde_json::to_value(head).map_err(|_| "Live Wiki head is not serializable")?,
            serde_json::to_value(manifest).map_err(|_| "Live Wiki manifest is not serializable")?,
        ))
    }
}

pub(super) fn coordinate_parts(coordinate: &str) -> Result<(&str, &str), String> {
    let mut parts = coordinate.splitn(3, ':');
    let kind = parts.next();
    let owner = parts.next().unwrap_or_default();
    let d = parts.next().unwrap_or_default();
    if kind != Some("30617")
        || owner.len() != 64
        || !owner
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || d.is_empty()
        || d.len() > 64
        || d.starts_with('.')
        || d.contains("..")
        || !d
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err("Source repository coordinate is invalid".into());
    }
    Ok((owner, d))
}
fn exact_event<'a>(
    events: &'a [Event],
    kind: u16,
    owner: &str,
    d: &str,
) -> Result<&'a Event, String> {
    let [event] = events else {
        return Err("Live Source anchor is unavailable".into());
    };
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    if event.kind.as_u16() != kind
        || event.pubkey.to_hex() != owner
        || event.verify().is_err()
        || tags.len() != 1
        || tags[0].as_slice().len() != 2
        || tags[0].as_slice()[1] != d
    {
        return Err("Live Source anchor is invalid".into());
    }
    Ok(event)
}

fn is_hex(value: &str, width: usize) -> bool {
    value.len() == width
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn exact_coordinate_tag(event: &Event, coordinate: &str) -> Result<(), String> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "a"))
        .collect();
    if tags.len() == 1 && tags[0].as_slice().len() == 2 && tags[0].as_slice()[1] == coordinate {
        Ok(())
    } else {
        Err("Live Source repository association is invalid".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn signed_repository(keys: &Keys, tags: Vec<Vec<&str>>) -> Event {
        let tags: Vec<Tag> = tags
            .into_iter()
            .map(|tag| Tag::parse(tag).expect("valid test tag"))
            .collect();
        EventBuilder::new(Kind::Custom(30617), "")
            .tags(tags)
            .sign_with_keys(keys)
            .expect("signed test event")
    }

    #[test]
    fn coordinate_requires_one_exact_signed_a_tag() {
        let keys = Keys::generate();
        let owner = keys.public_key().to_hex();
        let coordinate = format!("30617:{owner}:repo.demo");
        let event = signed_repository(&keys, vec![vec!["d", "repo.demo"], vec!["a", &coordinate]]);

        assert!(exact_event(&[event], 30617, &owner, "repo.demo").is_ok());
        let event = signed_repository(&keys, vec![vec!["d", "repo.demo"]]);
        assert!(exact_coordinate_tag(&event, &coordinate).is_err());
        let foreign = format!("30617:{}:repo.demo", "a".repeat(64));
        let event = signed_repository(&keys, vec![vec!["d", "repo.demo"], vec!["a", &foreign]]);
        assert!(exact_coordinate_tag(&event, &coordinate).is_err());
        let event = signed_repository(
            &keys,
            vec![
                vec!["d", "repo.demo"],
                vec!["a", &coordinate],
                vec!["a", &coordinate],
            ],
        );
        assert!(exact_coordinate_tag(&event, &coordinate).is_err());
    }
}
