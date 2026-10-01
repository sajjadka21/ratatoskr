use dm_ipc::UiPreferencesResponse;

#[test]
fn legacy_preferences_without_a_mode_still_deserialize() {
    for theme in ["light", "dark", "system", "forest-rune"] {
        let value = serde_json::json!({
            "language": "fa",
            "theme": theme,
            "closeToTray": true,
        });
        let preferences: UiPreferencesResponse = serde_json::from_value(value).unwrap();
        assert_eq!(preferences.language, "fa");
        assert_eq!(preferences.theme, theme);
        assert!(preferences.close_to_tray);
        assert_eq!(preferences.appearance_mode, None);
    }
}

#[test]
fn explicit_display_mode_round_trips_without_erasing_the_brand() {
    for mode in ["light", "dark", "system"] {
        let original = serde_json::json!({
            "language": "en",
            "theme": "frost-byte",
            "closeToTray": false,
            "appearanceMode": mode,
        });
        let preferences: UiPreferencesResponse = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(preferences.appearance_mode.as_deref(), Some(mode));
        assert_eq!(preferences.theme, "frost-byte");
        assert_eq!(serde_json::to_value(preferences).unwrap(), original);
    }
}

#[test]
fn a_null_migration_field_is_backward_compatible() {
    let original = serde_json::json!({
        "language": "fa",
        "theme": "midnight-arcane",
        "closeToTray": true,
        "appearanceMode": null,
    });
    let preferences: UiPreferencesResponse = serde_json::from_value(original).unwrap();
    assert_eq!(preferences.appearance_mode, None);
    assert_eq!(preferences.theme, "midnight-arcane");
}
