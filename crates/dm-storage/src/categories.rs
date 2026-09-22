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
                rule.domain,
                rule.url_pattern,
                rule.extension,
                rule.mime_pattern,
                rule.min_size.map(|value| value as i64),
                rule.max_size.map(|value| value as i64),
                rule.category_id,
                rule.destination_directory,
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
}
