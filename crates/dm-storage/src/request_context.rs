use crate::{Result, Storage};
use dm_common::RequestContext;
use rusqlite::{OptionalExtension, params};

const MAX_FIELD_LENGTH: usize = 2048;

impl Storage {
    /// Stores the non-secret request context a browser handed over with a
    /// task. Empty values are dropped and overly long ones truncated so a
    /// hostile page cannot bloat the database.
    pub fn set_request_context(&self, download_id: &str, context: &RequestContext) -> Result<()> {
        let referrer = bounded(context.referrer.as_deref());
        let user_agent = bounded(context.user_agent.as_deref());
        let connection = self.connection()?;

        if referrer.is_none() && user_agent.is_none() {
            connection.execute(
                "DELETE FROM download_request_context WHERE download_id = ?1;",
                params![download_id],
            )?;
            return Ok(());
        }

        connection.execute(
            "
            INSERT INTO download_request_context (download_id, referrer, user_agent)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(download_id) DO UPDATE SET
                referrer = excluded.referrer,
                user_agent = excluded.user_agent;
            ",
            params![download_id, referrer, user_agent],
        )?;

        Ok(())
    }

    pub fn get_request_context(&self, download_id: &str) -> Result<RequestContext> {
        let connection = self.connection()?;
        let context = connection
            .query_row(
                "
                SELECT referrer, user_agent
                FROM download_request_context
                WHERE download_id = ?1;
                ",
                params![download_id],
                |row| {
                    Ok(RequestContext {
                        referrer: row.get(0)?,
                        user_agent: row.get(1)?,
                    })
                },
            )
            .optional()?;

        Ok(context.unwrap_or_default())
    }
}

fn bounded(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.chars().take(MAX_FIELD_LENGTH).collect())
}

#[cfg(test)]
mod tests {
    use crate::Storage;
    use dm_common::RequestContext;
    use tempfile::tempdir;

    #[test]
    fn request_context_round_trips_and_is_removed_with_its_task() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/file.bin", 1)
            .unwrap();

        assert_eq!(
            storage.get_request_context(&task.id).unwrap(),
            RequestContext::default()
        );

        let context = RequestContext {
            referrer: Some("https://example.com/page".to_owned()),
            user_agent: Some("Mozilla/5.0 Test".to_owned()),
        };
        storage.set_request_context(&task.id, &context).unwrap();
        assert_eq!(storage.get_request_context(&task.id).unwrap(), context);

        storage.remove_download_record(&task.id).unwrap();
        assert_eq!(
            storage.get_request_context(&task.id).unwrap(),
            RequestContext::default()
        );
    }

    #[test]
    fn request_context_drops_header_injection_and_blank_values() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        let task = storage
            .create_download("https://example.com/file.bin", 1)
            .unwrap();

        storage
            .set_request_context(
                &task.id,
                &RequestContext {
                    referrer: Some("https://a.example\r\nCookie: x".to_owned()),
                    user_agent: Some("   ".to_owned()),
                },
            )
            .unwrap();

        assert_eq!(
            storage.get_request_context(&task.id).unwrap(),
            RequestContext::default()
        );
    }
}
