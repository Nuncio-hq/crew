use crew_wiki::WikiError;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crew_wiki::snapshot_v1;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/wiki-snapshot-v1.json")).expect("fixture")
}

fn verify(value: &Value) -> Result<(), WikiError> {
    snapshot_v1::verify_snapshot(
        value["owner"].as_str().expect("owner"),
        value["repoD"].as_str().expect("repoD"),
        &value["head"],
        &value["manifest"],
        value["pages"].as_array().expect("pages"),
    )
    .map(|_| ())
}

fn hash(value: &Value) -> String {
    Sha256::digest(serde_json::to_vec(value).expect("JSON"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sign(event: &mut Value) {
    let keys = Keys::parse(&"01".repeat(32)).expect("fixture key");
    let tags = event["tags"]
        .as_array()
        .expect("tags")
        .iter()
        .map(|tag| {
            Tag::parse(
                tag.as_array()
                    .expect("tag")
                    .iter()
                    .map(|v| v.as_str().expect("tag string")),
            )
            .expect("tag parse")
        })
        .collect::<Vec<_>>();
    let signed = EventBuilder::new(
        Kind::Custom(30623),
        event["content"].as_str().expect("body"),
    )
    .tags(tags)
    .custom_created_at(Timestamp::from(10))
    .sign_with_keys(&keys)
    .expect("sign");
    *event = serde_json::from_str(&signed.as_json()).expect("signed JSON");
}

fn set_tag(event: &mut Value, name: &str, values: Value) {
    let tags = event["tags"].as_array_mut().expect("tags");
    tags.retain(|tag| tag[0] != name);
    tags.push(values);
}

fn replace_manifest(value: &mut Value, manifest: Value) {
    let digest = hash(&manifest);
    value["manifest"]["content"] = json!(manifest.to_string());
    set_tag(
        &mut value["manifest"],
        "d",
        json!(["d", format!("Repo.demo/m1-{digest}")]),
    );
    sign(&mut value["manifest"]);
    let id = value["manifest"]["id"].clone();
    set_tag(
        &mut value["head"],
        "wiki-manifest",
        json!(["wiki-manifest", id, digest]),
    );
    sign(&mut value["head"]);
}

#[test]
fn accepts_shared_typescript_signed_goldens() {
    verify(&fixture()).expect("native accepts the exact TS golden bytes");
    let escaping = serde_json::from_str(include_str!("fixtures/wiki-snapshot-v1-escaping.json"))
        .expect("escaping fixture");
    verify(&escaping).expect("canonical Unicode and escaping are interoperable");
    let folder = serde_json::from_str(include_str!("fixtures/wiki-snapshot-v1-folder.json"))
        .expect("folder fixture");
    verify(&folder).expect("typed folder revision is interoperable");
}

#[test]
fn resigned_fixture_has_valid_membership() {
    let mut value = fixture();
    sign(&mut value["head"]);
    verify(&value).expect("negative-case signing helper must produce valid signatures");
    let manifest = serde_json::from_str(value["manifest"]["content"].as_str().expect("body"))
        .expect("manifest");
    replace_manifest(&mut value, manifest);
    verify(&value).expect("manifest rewrite helper must preserve valid membership");
}

fn repoint_page(value: &mut Value) {
    let mut manifest: Value =
        serde_json::from_str(value["manifest"]["content"].as_str().expect("body"))
            .expect("manifest");
    manifest[7][0][2] = value["pages"][0]["id"].clone();
    replace_manifest(value, manifest);
}

#[test]
fn duplicate_page_tags_are_rejected_with_current_signed_membership() {
    for name in [
        "a",
        "d",
        "commit",
        "wiki-version",
        "wiki-snapshot",
        "source-kind",
        "wiki-slug",
        "title",
        "section",
        "language",
        "wiki-source-files",
    ] {
        let mut value = fixture();
        let tags = value["pages"][0]["tags"].as_array_mut().expect("tags");
        let tag = tags
            .iter()
            .find(|tag| tag[0] == name)
            .expect("required")
            .clone();
        tags.push(tag);
        sign(&mut value["pages"][0]);
        repoint_page(&mut value);
        assert!(verify(&value).is_err(), "duplicate page {name}");
    }
}

#[test]
fn complete_signed_event_limit_and_unknown_version_fail_closed() {
    let mut oversized = fixture();
    oversized["head"]["tags"]
        .as_array_mut()
        .expect("tags")
        .push(json!(["padding", "x".repeat(192 * 1024)]));
    sign(&mut oversized["head"]);
    assert!(verify(&oversized).is_err());
    let mut version = fixture();
    let mut manifest: Value =
        serde_json::from_str(version["manifest"]["content"].as_str().expect("body"))
            .expect("manifest");
    manifest[0] = json!(2);
    replace_manifest(&mut version, manifest);
    assert!(verify(&version).is_err());
}

#[test]
fn rejects_tampered_signature_and_event_id() {
    for field in ["sig", "id"] {
        let mut value = fixture();
        value["pages"][0][field] = json!("0".repeat(if field == "sig" { 128 } else { 64 }));
        assert!(verify(&value).is_err(), "{field}");
    }
}

#[test]
fn signed_body_change_cannot_reuse_manifest_membership() {
    let mut value = fixture();
    value["pages"][0]["content"] = json!("new signed body");
    sign(&mut value["pages"][0]);
    repoint_page(&mut value);
    assert!(verify(&value).is_err());
}

#[test]
fn event_id_is_recomputed_even_when_signature_over_old_id_is_valid() {
    let mut value = fixture();
    value["head"]["created_at"] = json!(11);
    assert!(verify(&value).is_err());
}

#[test]
fn noncanonical_manifest_and_wrong_legacy_source_projection_are_rejected() {
    let mut value = fixture();
    let content = value["manifest"]["content"].as_str().unwrap();
    value["manifest"]["content"] = json!(format!(" {content}"));
    sign(&mut value["manifest"]);
    let id = value["manifest"]["id"].clone();
    let tags = value["head"]["tags"].as_array_mut().unwrap();
    tags.iter_mut().find(|t| t[0] == "wiki-manifest").unwrap()[1] = id;
    sign(&mut value["head"]);
    assert!(verify(&value).is_err());
    let mut value = fixture();
    set_tag(
        &mut value["pages"][0],
        "source",
        json!(["source", "wrong.rs"]),
    );
    sign(&mut value["pages"][0]);
    repoint_page(&mut value);
    assert!(verify(&value).is_err());
}

#[test]
fn duplicate_required_tags_are_rejected_even_when_signed() {
    for target in ["head", "manifest"] {
        for name in [
            "a",
            "d",
            "commit",
            "wiki-version",
            "wiki-snapshot",
            "source-kind",
        ] {
            let mut value = fixture();
            let tags = value[target]["tags"].as_array_mut().expect("tags");
            let tag = tags
                .iter()
                .find(|tag| tag[0] == name)
                .expect("required tag")
                .clone();
            tags.push(tag);
            sign(&mut value[target]);
            assert!(verify(&value).is_err(), "{target} {name}");
        }
    }
}

#[test]
fn signed_head_requires_exact_projection_and_expected_revision() {
    for tag in [
        json!(["expected-revision", "absent", "extra"]),
        json!(["expected-revision", "bad"]),
    ] {
        let mut value = fixture();
        set_tag(&mut value["head"], "expected-revision", tag);
        sign(&mut value["head"]);
        assert!(verify(&value).is_err());
    }
    let mut value = fixture();
    value["head"]["content"] = json!("{\"sections\":[]}");
    sign(&mut value["head"]);
    assert!(verify(&value).is_err());
}

#[test]
fn signed_manifest_cannot_change_order_membership_or_source_range() {
    for mutation in 0..5 {
        let mut value = fixture();
        let mut manifest: Value =
            serde_json::from_str(value["manifest"]["content"].as_str().expect("body"))
                .expect("manifest");
        match mutation {
            0 => manifest[6][0][2] = json!(["intro", "intro"]),
            1 => manifest[7][0][5] = json!("wrong-section"),
            2 => manifest[7][0][7][0][3] = json!(0),
            3 => manifest[7][0][7][0][4] = json!(8),
            _ => manifest[7][0][7][0][0] = json!("../escape"),
        }
        replace_manifest(&mut value, manifest);
        assert!(verify(&value).is_err(), "mutation {mutation}");
    }
}

#[test]
fn complete_membership_requires_exact_page_set() {
    let mut missing = fixture();
    missing["pages"] = json!([]);
    assert!(verify(&missing).is_err());
    let mut duplicate = fixture();
    let page = duplicate["pages"][0].clone();
    duplicate["pages"].as_array_mut().expect("pages").push(page);
    assert!(verify(&duplicate).is_err());
}

#[test]
fn owner_and_repository_are_exact_external_inputs() {
    let mut wrong = fixture();
    wrong["repoD"] = json!("repo.demo");
    assert!(verify(&wrong).is_err());
    wrong = fixture();
    wrong["owner"] = json!("f".repeat(64));
    assert!(verify(&wrong).is_err());
}
