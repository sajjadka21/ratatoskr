use crate::{Result, Storage, StorageError};
use dm_common::{CategoryRecord, DownloadPriority, DownloadRule};
use rusqlite::{OptionalExtension, Row, params};
use std::str::FromStr;
use uuid::Uuid;

impl Storage {
    pub fn list_categories(&self) -> Result<Vec<CategoryRecord>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, extensions_json, mime_patterns_json, default_directory,
                    host_patterns_json, priority, queue_id
             FROM categories ORDER BY name ASC, id ASC;",
        )?;
        let rows = statement.query_map([], CategoryRow::from_row)?;
        rows.map(|row| row?.into_record()).collect()
    }

    pub fn get_category(&self, id: &str) -> Result<Option<CategoryRecord>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT id, name, extensions_json, mime_patterns_json, default_directory,
                        host_patterns_json, priority, queue_id
                 FROM categories WHERE id = ?1;",
                [id],
                CategoryRow::from_row,
            )
            .optional()?
            .map(CategoryRow::into_record)
            .transpose()
    }

    pub fn create_category(&self, category: &CategoryRecord) -> Result<CategoryRecord> {
        let name = category.name.trim();
        if name.is_empty() {
            return Err(StorageError::InvalidCategoryConfiguration(
                "category name must not be empty".to_owned(),
            ));
        }
        let id = if category.id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            category.id.clone()
        };
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO categories
             (id, name, extensions_json, mime_patterns_json, default_directory,
              host_patterns_json, priority, queue_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params![
                id,
                name,
                json_array(&category.extensions)?,
                json_array(&category.mime_patterns)?,
                category
                    .default_directory
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                json_array(&category.host_patterns)?,
                category.priority.as_str(),
                category.queue_id,
            ],
        )?;
        drop(connection);
        self.get_category(&id)?
            .ok_or(StorageError::InvalidCategoryConfiguration(
                "created category could not be read back".to_owned(),
            ))
    }

    /// Changes where a category's files go. `None` (or a blank path) sends
    /// them to the default folder again.
    pub fn set_category_directory(
        &self,
        id: &str,
        directory: Option<&str>,
    ) -> Result<CategoryRecord> {
        let directory = directory.map(str::trim).filter(|value| !value.is_empty());
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE categories SET default_directory = ?2 WHERE id = ?1;",
            params![id, directory],
        )?;
        drop(connection);
        if changed == 0 {
            return Err(StorageError::InvalidCategoryConfiguration(format!(
                "category not found: {id}"
            )));
        }
        self.get_category(id)?
            .ok_or(StorageError::InvalidCategoryConfiguration(format!(
                "category not found: {id}"
            )))
    }

    /// Replaces every field of an existing rule.
    pub fn update_rule(&self, rule: &DownloadRule) -> Result<DownloadRule> {
        validate_rule(rule)?;
        let connection = self.connection()?;
        let changed = connection.execute(
            "UPDATE download_rules SET
                name = ?2, enabled = ?3, sort_order = ?4, domain = ?5, url_pattern = ?6,
                extension = ?7, mime_pattern = ?8, min_size = ?9, max_size = ?10,
                category_id = ?11, destination_directory = ?12, queue_id = ?13,
                priority = ?14, max_connections = ?15, max_host_concurrency = ?16,
                speed_cap = ?17, browser_takeover_allowed = ?18
             WHERE id = ?1;",
            params![
                rule.id,
                rule.name.trim(),
                rule.enabled,
                rule.sort_order,
                blank_to_none(rule.domain.as_deref()),
                blank_to_none(rule.url_pattern.as_deref()),
                blank_to_none(rule.extension.as_deref()),
                blank_to_none(rule.mime_pattern.as_deref()),
                rule.min_size.map(|value| value as i64),
                rule.max_size.map(|value| value as i64),
                rule.category_id,
                blank_to_none(rule.destination_directory.as_deref()),
                rule.queue_id,
                rule.priority.map(|value| value.as_str()),
                rule.max_connections.map(i64::from),
                rule.max_host_concurrency.map(i64::from),
                rule.speed_cap.map(|value| value as i64),
                rule.browser_takeover_allowed,
            ],
        )?;
        drop(connection);
        if changed == 0 {
            return Err(StorageError::InvalidRuleConfiguration(format!(
                "rule not found: {}",
                rule.id
            )));
        }
        self.list_rules()?
            .into_iter()
            .find(|candidate| candidate.id == rule.id)
            .ok_or(StorageError::InvalidRuleConfiguration(
                "updated rule could not be read back".to_owned(),
            ))
    }

    pub fn delete_rule(&self, id: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute("DELETE FROM download_rules WHERE id = ?1;", params![id])?;
        Ok(())
    }

    pub fn list_rules(&self) -> Result<Vec<DownloadRule>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, enabled, sort_order, domain, url_pattern, extension,
                    mime_pattern, min_size, max_size, category_id, destination_directory,
                    queue_id, priority, max_connections, max_host_concurrency, speed_cap,
                    browser_takeover_allowed
             FROM download_rules ORDER BY sort_order ASC, id ASC;",
        )?;
        let rows = statement.query_map([], RuleRow::from_row)?;
        rows.map(|row| row?.into_record()).collect()
    }

    pub fn create_rule(&self, rule: &DownloadRule) -> Result<DownloadRule> {
        validate_rule(rule)?;
        let id = if rule.id.trim().is_empty() {
            Uuid::new_v4().to_string()
        } else {
            rule.id.clone()
        };
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO download_rules
             (id, name, enabled, sort_order, domain, url_pattern, extension, mime_pattern,
              min_size, max_size, category_id, destination_directory, queue_id, priority,
              max_connections, max_host_concurrency, speed_cap, browser_takeover_allowed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18);",
            params![
                id,
                rule.name.trim(),
                rule.enabled,
                rule.sort_order,
                blank_to_none(rule.domain.as_deref()),
                blank_to_none(rule.url_pattern.as_deref()),
                blank_to_none(rule.extension.as_deref()),
                blank_to_none(rule.mime_pattern.as_deref()),
                rule.min_size.map(|value| value as i64),
                rule.max_size.map(|value| value as i64),
                rule.category_id,
                blank_to_none(rule.destination_directory.as_deref()),
                rule.queue_id,
                rule.priority.map(|value| value.as_str()),
                rule.max_connections.map(i64::from),
                rule.max_host_concurrency.map(i64::from),
                rule.speed_cap.map(|value| value as i64),
                rule.browser_takeover_allowed,
            ],
        )?;
        drop(connection);
        self.list_rules()?
            .into_iter()
            .find(|candidate| candidate.id == id)
            .ok_or(StorageError::InvalidRuleConfiguration(
                "created rule could not be read back".to_owned(),
            ))
    }
}

fn json_array(values: &[String]) -> Result<String> {
    serde_json::to_string(values).map_err(|error| {
        StorageError::InvalidCategoryConfiguration(format!("cannot encode list: {error}"))
    })
}

struct CategoryRow {
    id: String,
    name: String,
    extensions_json: String,
    mime_patterns_json: String,
    default_directory: Option<String>,
    host_patterns_json: String,
    priority: String,
    queue_id: Option<String>,
}

impl CategoryRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            extensions_json: row.get(2)?,
            mime_patterns_json: row.get(3)?,
            default_directory: row.get(4)?,
            host_patterns_json: row.get(5)?,
            priority: row.get(6)?,
            queue_id: row.get(7)?,
        })
    }

    fn into_record(self) -> Result<CategoryRecord> {
        Ok(CategoryRecord {
            id: self.id,
            name: self.name,
            extensions: serde_json::from_str(&self.extensions_json)
                .map_err(|error| StorageError::InvalidCategoryConfiguration(error.to_string()))?,
            mime_patterns: serde_json::from_str(&self.mime_patterns_json)
                .map_err(|error| StorageError::InvalidCategoryConfiguration(error.to_string()))?,
            default_directory: self.default_directory,
            host_patterns: serde_json::from_str(&self.host_patterns_json)
                .map_err(|error| StorageError::InvalidCategoryConfiguration(error.to_string()))?,
            priority: DownloadPriority::from_str(&self.priority)
                .map_err(|error| StorageError::InvalidDownloadPriority(error.to_string()))?,
            queue_id: self.queue_id,
        })
    }
}

struct RuleRow {
    id: String,
    name: String,
    enabled: bool,
    sort_order: i64,
    domain: Option<String>,
    url_pattern: Option<String>,
    extension: Option<String>,
    mime_pattern: Option<String>,
    min_size: Option<i64>,
    max_size: Option<i64>,
    category_id: Option<String>,
    destination_directory: Option<String>,
    queue_id: Option<String>,
    priority: Option<String>,
    max_connections: Option<i64>,
    max_host_concurrency: Option<i64>,
    speed_cap: Option<i64>,
    browser_takeover_allowed: Option<bool>,
}

impl RuleRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            enabled: row.get(2)?,
            sort_order: row.get(3)?,
            domain: row.get(4)?,
            url_pattern: row.get(5)?,
            extension: row.get(6)?,
            mime_pattern: row.get(7)?,
            min_size: row.get(8)?,
            max_size: row.get(9)?,
            category_id: row.get(10)?,
            destination_directory: row.get(11)?,
            queue_id: row.get(12)?,
            priority: row.get(13)?,
            max_connections: row.get(14)?,
            max_host_concurrency: row.get(15)?,
            speed_cap: row.get(16)?,
            browser_takeover_allowed: row.get(17)?,
        })
    }

    fn into_record(self) -> Result<DownloadRule> {
        Ok(DownloadRule {
            id: self.id,
            name: self.name,
            enabled: self.enabled,
            sort_order: self.sort_order,
            domain: self.domain,
            url_pattern: self.url_pattern,
            extension: self.extension,
            mime_pattern: self.mime_pattern,
            min_size: self.min_size.map(|value| value as u64),
            max_size: self.max_size.map(|value| value as u64),
            category_id: self.category_id,
            destination_directory: self.destination_directory,
            queue_id: self.queue_id,
            priority: self
                .priority
                .map(|value| DownloadPriority::from_str(&value))
                .transpose()
                .map_err(|error| StorageError::InvalidDownloadPriority(error.to_string()))?,
            max_connections: self.max_connections.map(|value| value as u32),
            max_host_concurrency: self.max_host_concurrency.map(|value| value as u32),
            speed_cap: self.speed_cap.map(|value| value as u64),
            browser_takeover_allowed: self.browser_takeover_allowed,
        })
    }
}

fn validate_rule(rule: &DownloadRule) -> Result<()> {
    if rule.name.trim().is_empty() {
        return Err(StorageError::InvalidRuleConfiguration(
            "rule name must not be empty".to_owned(),
        ));
    }
    if rule
        .min_size
        .zip(rule.max_size)
        .is_some_and(|(min, max)| min > max)
    {
        return Err(StorageError::InvalidRuleConfiguration(
            "minimum size cannot exceed maximum size".to_owned(),
        ));
    }
    let has_condition = [
        rule.domain.as_deref(),
        rule.url_pattern.as_deref(),
        rule.extension.as_deref(),
        rule.mime_pattern.as_deref(),
    ]
    .into_iter()
    .any(|value| blank_to_none(value).is_some())
        || rule.min_size.is_some()
        || rule.max_size.is_some();
    if !has_condition {
        return Err(StorageError::InvalidRuleConfiguration(
            "a rule needs at least one condition".to_owned(),
        ));
    }
    Ok(())
}

fn blank_to_none(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn built_in_categories_and_custom_rules_persist() {
        let root = tempdir().unwrap();
        let storage = Storage::open(root.path().join("downloads.db")).unwrap();
        assert_eq!(storage.list_categories().unwrap().len(), 7);

        let category = storage
            .create_category(&CategoryRecord {
                id: String::new(),
                name: "Research".to_owned(),
                extensions: vec!["md".to_owned()],
                mime_patterns: vec!["text/markdown".to_owned()],
                default_directory: Some("C:\\Downloads\\Research".to_owned()),
                host_patterns: vec!["docs.example.com".to_owned()],
                priority: DownloadPriority::High,
                queue_id: None,
            })
            .unwrap();
        assert_eq!(
            storage.get_category(&category.id).unwrap().unwrap(),
            category
        );

        let rule = DownloadRule {
            id: String::new(),
            name: "Markdown docs".to_owned(),
            enabled: true,
            sort_order: 0,
            domain: Some("docs.example.com".to_owned()),
            url_pattern: None,
            extension: Some("md".to_owned()),
            mime_pattern: None,
            min_size: None,
            max_size: None,
            category_id: Some(category.id),
            destination_directory: None,
            queue_id: None,
            priority: Some(DownloadPriority::High),
            max_connections: Some(2),
            max_host_concurrency: Some(1),
            speed_cap: Some(100_000),
            browser_takeover_allowed: Some(false),
        };
        let created = storage.create_rule(&rule).unwrap();
        assert_eq!(storage.list_rules().unwrap(), vec![created]);
    }

    fn domain_rule(name: &str) -> DownloadRule {
        DownloadRule {
            id: String::new(),
            name: name.to_owned(),
            enabled: true,
            sort_order: 0,
            domain: Some("example.com".to_owned()),
            url_pattern: None,
            extension: None,
            mime_pattern: None,
            min_size: None,
            max_size: None,
            category_id: None,
            destination_directory: None,
            queue_id: None,
            priority: None,
            max_connections: None,
            max_host_concurrency: None,
            speed_cap: None,
            browser_takeover_allowed: None,
        }
    }

    #[test]
    fn category_directory_can_be_set_and_cleared() {
        let root = tempdir().unwrap();
        let storage = Storage::open(root.path().join("downloads.db")).unwrap();

        let video = storage
            .set_category_directory("video", Some("D:\\Videos"))
            .unwrap();
        assert_eq!(video.default_directory.as_deref(), Some("D:\\Videos"));

        let video = storage.set_category_directory("video", Some("  ")).unwrap();
        assert_eq!(video.default_directory, None);

        assert!(storage.set_category_directory("missing", None).is_err());
    }

    #[test]
    fn rules_can_be_updated_and_deleted() {
        let root = tempdir().unwrap();
        let storage = Storage::open(root.path().join("downloads.db")).unwrap();
        let mut rule = storage.create_rule(&domain_rule("Example")).unwrap();

        rule.enabled = false;
        rule.speed_cap = Some(50_000);
        rule.destination_directory = Some(" ".to_owned());
        let updated = storage.update_rule(&rule).unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.speed_cap, Some(50_000));
        assert_eq!(updated.destination_directory, None);

        storage.delete_rule(&rule.id).unwrap();
        assert!(storage.list_rules().unwrap().is_empty());
    }

    #[test]
    fn a_rule_without_any_condition_is_refused() {
        let root = tempdir().unwrap();
        let storage = Storage::open(root.path().join("downloads.db")).unwrap();
        let mut rule = domain_rule("Matches everything");
        rule.domain = Some("   ".to_owned());
        assert!(storage.create_rule(&rule).is_err());
    }
}
