use super::{managed_agent_avatar_url, preset_catalog_entry, PRESET_HARNESSES};

#[test]
fn resolves_vendor_avatars_for_every_preset_command() {
    let devin = PRESET_HARNESSES
        .iter()
        .find(|preset| preset.id == "devin")
        .expect("Devin should be a preset");
    assert_eq!(devin.command, "devin");
    assert_eq!(devin.args, &["acp"]);

    for preset in PRESET_HARNESSES {
        assert!(
            preset.avatar_url.starts_with("https://"),
            "{} must use a public vendor avatar",
            preset.id
        );
        assert_eq!(
            managed_agent_avatar_url(&format!("/usr/local/bin/{}", preset.command)),
            Some(preset.avatar_url.to_string()),
            "{} command path should resolve its vendor avatar",
            preset.id
        );
        let entry = preset_catalog_entry(preset, |_| None);
        assert_eq!(entry.avatar_url, preset.avatar_url);
    }
}
