use crate::{Result, Storage, StorageError};
use rusqlite::params;

/// Where the bytes of a transfer came from, as far as billing is concerned.
/// Iranian operators bill domestic traffic at a lower rate than
/// international traffic, so the two are counted apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficScope {
    Domestic,
    International,
}

impl TrafficScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Domestic => "domestic",
            Self::International => "international",
        }
    }
}

/// Bytes moved over a range of days.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrafficTotals {
    pub domestic_bytes: u64,
    pub international_bytes: u64,
}

impl Storage {
    /// Adds `bytes` to the running total for `day` (`YYYY-MM-DD`, local).
    pub fn record_traffic(&self, day: &str, scope: TrafficScope, bytes: u64) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }
        validate_day(day)?;
        let bytes = i64::try_from(bytes).map_err(|_| StorageError::IntegerTooLarge {
            field: "traffic_bytes",
            value: bytes,
        })?;
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO traffic_usage (day, scope, bytes) VALUES (?1, ?2, ?3)
            ON CONFLICT (day, scope) DO UPDATE SET bytes = bytes + excluded.bytes;
            "#,
            params![day, scope.as_str(), bytes],
        )?;
        Ok(())
    }

    /// Totals for the inclusive range `[from_day, to_day]`.
    pub fn traffic_between(&self, from_day: &str, to_day: &str) -> Result<TrafficTotals> {
        validate_day(from_day)?;
        validate_day(to_day)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT scope, SUM(bytes) FROM traffic_usage WHERE day >= ?1 AND day <= ?2 GROUP BY scope;",
        )?;
        let rows = statement.query_map(params![from_day, to_day], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut totals = TrafficTotals::default();
        for row in rows {
            let (scope, bytes) = row?;
            let bytes = u64::try_from(bytes).unwrap_or(0);
            match scope.as_str() {
                "domestic" => totals.domestic_bytes = bytes,
                _ => totals.international_bytes = bytes,
            }
        }
        Ok(totals)
    }

    /// Totals for each day of `[from_day, to_day]` that saw traffic, in
    /// day order. Days without traffic are left out.
    pub fn traffic_by_day(
        &self,
        from_day: &str,
        to_day: &str,
    ) -> Result<Vec<(String, TrafficTotals)>> {
        validate_day(from_day)?;
        validate_day(to_day)?;
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT day, scope, bytes FROM traffic_usage WHERE day >= ?1 AND day <= ?2 ORDER BY day;",
        )?;
        let rows = statement.query_map(params![from_day, to_day], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut days: Vec<(String, TrafficTotals)> = Vec::new();
        for row in rows {
            let (day, scope, bytes) = row?;
            let bytes = u64::try_from(bytes).unwrap_or(0);
            if days.last().is_none_or(|(last, _)| *last != day) {
                days.push((day, TrafficTotals::default()));
            }
            let totals = &mut days.last_mut().expect("just pushed").1;
            match scope.as_str() {
                "domestic" => totals.domestic_bytes += bytes,
                _ => totals.international_bytes += bytes,
            }
        }
        Ok(days)
    }
}

fn validate_day(day: &str) -> Result<()> {
    let bytes = day.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
    if shaped {
        Ok(())
    } else {
        Err(StorageError::InvalidTraffic(format!(
            "traffic day must look like 2026-09-24, got {day:?}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn totals_add_up_per_scope_and_respect_the_range() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();

        storage
            .record_traffic("2026-09-01", TrafficScope::Domestic, 100)
            .unwrap();
        storage
            .record_traffic("2026-09-01", TrafficScope::Domestic, 50)
            .unwrap();
        storage
            .record_traffic("2026-09-02", TrafficScope::International, 70)
            .unwrap();
        storage
            .record_traffic("2026-10-01", TrafficScope::International, 999)
            .unwrap();

        let september = storage.traffic_between("2026-09-01", "2026-09-30").unwrap();
        assert_eq!(
            september,
            TrafficTotals {
                domestic_bytes: 150,
                international_bytes: 70
            }
        );
    }

    #[test]
    fn malformed_days_are_refused() {
        let directory = tempdir().unwrap();
        let storage = Storage::open(directory.path().join("downloads.db")).unwrap();
        assert!(
            storage
                .record_traffic("24/09/2026", TrafficScope::Domestic, 1)
                .is_err()
        );
        assert!(storage.traffic_between("2026-9-1", "2026-09-30").is_err());
    }
}
