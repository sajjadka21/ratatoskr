use crate::{Result, Storage, StorageError};
use dm_common::HostProfile;
use rusqlite::{OptionalExtension, params};

impl Storage {
    pub fn get_host_profile(&self, host: &str) -> Result<Option<HostProfile>> {
        validate_host_key(host)?;
        let connection = self.connection()?;
        connection
            .query_row(
                r#"
                SELECT host, preferred_max_connections, rate_limited_count,
                       busy_count, last_status, updated_at
                FROM host_profiles
                WHERE host = ?1;
                "#,
                [host],
                |row| {
                    Ok(HostProfile {
                        host: row.get(0)?,
                        preferred_max_connections: row.get::<_, i64>(1)?.try_into().map_err(
                            |_| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    1,
                                    rusqlite::types::Type::Integer,
                                    "negative preferred connection count".into(),
                                )
                            },
                        )?,
                        rate_limited_count: row.get::<_, i64>(2)?.try_into().map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                2,
                                rusqlite::types::Type::Integer,
                                "negative rate limit count".into(),
                            )
                        })?,
                        busy_count: row.get::<_, i64>(3)?.try_into().map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Integer,
                                "negative busy count".into(),
                            )
                        })?,
                        last_status: row.get::<_, Option<i64>>(4)?.map(|value| value as u16),
                        updated_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn record_host_observation(
        &self,
        host: &str,
        status: Option<u16>,
        preferred_max_connections: u32,
        updated_at: i64,
    ) -> Result<HostProfile> {
        validate_host_key(host)?;
        if !matches!(status, None | Some(429) | Some(503)) {
            return Err(StorageError::InvalidHostProfile(
                "status must be 429 or 503".to_owned(),
            ));
        }
        if preferred_max_connections == 0 {
            return Err(StorageError::InvalidHostProfile(
                "preferred connection count must be positive".to_owned(),
            ));
        }

        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO host_profiles (
                host, preferred_max_connections, rate_limited_count,
                busy_count, last_status, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(host) DO UPDATE SET
                preferred_max_connections = excluded.preferred_max_connections,
                rate_limited_count = host_profiles.rate_limited_count + excluded.rate_limited_count,
                busy_count = host_profiles.busy_count + excluded.busy_count,
                last_status = excluded.last_status,
                updated_at = excluded.updated_at;
            "#,
            params![
                host,
                i64::from(preferred_max_connections),
                i64::from(u8::from(status == Some(429))),
                i64::from(u8::from(status == Some(503))),
                status.map(i64::from),
                updated_at,
            ],
        )?;
        drop(connection);

        self.get_host_profile(host)?.ok_or_else(|| {
            StorageError::InvalidHostProfile("host profile was not persisted".to_owned())
        })
    }
}

fn validate_host_key(host: &str) -> Result<()> {
    if host.trim().is_empty()
        || host != host.trim()
        || host
            .chars()
            .any(|character| matches!(character, '/' | '?' | '#' | '@' | '\\'))
    {
        return Err(StorageError::InvalidHostProfile(
            "host key must contain only a normalized hostname".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_host_observations_without_url_components() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        let profile = storage
            .record_host_observation("example.com", Some(429), 2, 100)
            .unwrap();

        assert_eq!(profile.host, "example.com");
        assert_eq!(profile.preferred_max_connections, 2);
        assert_eq!(profile.rate_limited_count, 1);
        assert_eq!(profile.busy_count, 0);
        assert!(
            storage
                .get_host_profile("https://example.com/private?token=secret")
                .is_err()
        );
    }

    #[test]
    fn host_observations_survive_reopen_and_accumulate_statuses() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("downloads.db");
        {
            let storage = Storage::open(&path).unwrap();
            storage
                .record_host_observation("example.com", Some(503), 1, 100)
                .unwrap();
        }
        {
            let storage = Storage::open(&path).unwrap();
            let profile = storage
                .record_host_observation("example.com", Some(503), 1, 200)
                .unwrap();
            assert_eq!(profile.busy_count, 2);
            assert_eq!(profile.updated_at, 200);
        }
    }
}
