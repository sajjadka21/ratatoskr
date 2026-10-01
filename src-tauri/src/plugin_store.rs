//! The folder of plugin files (`plugins/` beside the database) and the list of
//! the ones the user switched off. The rules themselves live in
//! `dm_core::plugins`; this only finds, copies and removes the files.

use dm_core::plugins::{Plugin, PluginSet, MAX_PLUGIN_BYTES};
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub const SETTING_DISABLED: &str = "plugins_disabled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub rules: usize,
    pub enabled: bool,
}

pub fn folder(app_data: &Path) -> PathBuf {
    app_data.join("plugins")
}

pub fn parse_disabled(value: Option<String>) -> HashSet<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn join_disabled(ids: &HashSet<String>) -> String {
    let mut sorted: Vec<&str> = ids.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.join(",")
}

/// Every valid plugin in the folder, ordered by id. A damaged file is skipped,
/// never allowed to stop the others.
pub fn load_all(dir: &Path) -> Vec<Plugin> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut plugins: Vec<Plugin> = entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if metadata.len() as usize > MAX_PLUGIN_BYTES {
                return None;
            }
            Plugin::parse(&fs::read_to_string(entry.path()).ok()?).ok()
        })
        .collect();
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}

pub fn installed(dir: &Path, disabled: &HashSet<String>) -> Vec<InstalledPlugin> {
    load_all(dir)
        .into_iter()
        .map(|plugin| InstalledPlugin {
            enabled: !disabled.contains(&plugin.id),
            rules: plugin.rules.len(),
            id: plugin.id,
            name: plugin.name,
            version: plugin.version,
        })
        .collect()
}

pub fn active(dir: &Path, disabled: &HashSet<String>) -> PluginSet {
    PluginSet::new(
        load_all(dir)
            .into_iter()
            .filter(|plugin| !disabled.contains(&plugin.id))
            .collect(),
    )
}

/// Reads a plugin file chosen by the user, checks it and keeps a copy under its id.
pub fn import(dir: &Path, source: &Path) -> Result<Plugin, String> {
    let size = fs::metadata(source)
        .map_err(|error| error.to_string())?
        .len();
    if size as usize > MAX_PLUGIN_BYTES {
        return Err(dm_core::plugins::PluginError::TooLarge.to_string());
    }
    let text = fs::read_to_string(source).map_err(|error| error.to_string())?;
    let plugin = Plugin::parse(&text).map_err(|error| error.to_string())?;
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let target = dir.join(format!("{}.json", plugin.id));
    let temp = dir.join(format!("{}.json.tmp", plugin.id));
    fs::write(&temp, text).map_err(|error| error.to_string())?;
    fs::rename(&temp, &target).map_err(|error| error.to_string())?;
    Ok(plugin)
}

/// Deletes a plugin by id. The id is checked first so it can never name a path.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let valid = !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        return Err("not a plugin id".to_owned());
    }
    match fs::remove_file(dir.join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_json(id: &str) -> String {
        format!(
            r#"{{ "schema": 1, "id": "{id}", "name": "Plugin {id}", "version": "1.0.0", "rules": [
            {{ "type": "rename", "match": "*.part", "replace": "{{1}}" }} ] }}"#
        )
    }

    #[test]
    fn import_keeps_a_copy_and_lists_it() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("anything.json");
        fs::write(&source, plugin_json("alpha")).unwrap();
        let dir = folder(temp.path());

        let plugin = import(&dir, &source).unwrap();
        assert_eq!(plugin.id, "alpha");
        let listed = installed(&dir, &HashSet::new());
        assert_eq!(listed.len(), 1);
        assert!(listed[0].enabled);
        assert_eq!(listed[0].rules, 1);
    }

    #[test]
    fn a_disabled_plugin_is_listed_but_not_active() {
        let temp = tempfile::tempdir().unwrap();
        let dir = folder(temp.path());
        for id in ["a", "b"] {
            let source = temp.path().join(format!("{id}.json"));
            fs::write(&source, plugin_json(id)).unwrap();
            import(&dir, &source).unwrap();
        }
        let disabled = parse_disabled(Some("b".to_owned()));
        assert_eq!(
            installed(&dir, &disabled)
                .iter()
                .filter(|p| p.enabled)
                .count(),
            1
        );
        assert!(!active(&dir, &disabled).is_empty());
        assert!(active(&dir, &parse_disabled(Some("a,b".to_owned()))).is_empty());
        assert_eq!(join_disabled(&disabled), "b");
    }

    #[test]
    fn bad_files_are_refused_and_never_stored() {
        let temp = tempfile::tempdir().unwrap();
        let dir = folder(temp.path());
        let source = temp.path().join("bad.json");
        fs::write(&source, r#"{"schema":1,"id":"../evil"}"#).unwrap();
        assert!(import(&dir, &source).is_err());
        assert!(load_all(&dir).is_empty());

        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("broken.json"), "not json").unwrap();
        assert!(load_all(&dir).is_empty());
    }

    #[test]
    fn remove_deletes_by_id_and_refuses_paths() {
        let temp = tempfile::tempdir().unwrap();
        let dir = folder(temp.path());
        let source = temp.path().join("a.json");
        fs::write(&source, plugin_json("alpha")).unwrap();
        import(&dir, &source).unwrap();

        assert!(remove(&dir, "../alpha").is_err());
        assert!(remove(&dir, "alpha").is_ok());
        assert!(load_all(&dir).is_empty());
        assert!(remove(&dir, "alpha").is_ok());
    }
}
