use crate::{Result, Storage, StorageError};
use dm_common::{CompletionAction, QueueSchedule, ScheduleKind};
use rusqlite::{OptionalExtension, Row, params};
use std::str::FromStr;

impl Storage {
    pub fn get_queue_schedule(&self, queue_id: &str) -> Result<Option<QueueSchedule>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT queue_id, enabled, kind, start_at, stop_at, weekdays_mask,
                        interval_seconds, completion_action, prevent_sleep, updated_at,
                        window_start_minute, window_end_minute
                 FROM queue_schedules WHERE queue_id = ?1;",
                [queue_id],
                ScheduleRow::from_row,
            )
            .optional()?
            .map(ScheduleRow::into_record)
            .transpose()
    }

    pub fn list_queue_schedules(&self) -> Result<Vec<QueueSchedule>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT queue_id, enabled, kind, start_at, stop_at, weekdays_mask,
                    interval_seconds, completion_action, prevent_sleep, updated_at,
                    window_start_minute, window_end_minute
             FROM queue_schedules ORDER BY queue_id ASC;",
        )?;
        let rows = statement.query_map([], ScheduleRow::from_row)?;
        rows.map(|row| row?.into_record()).collect()
    }

    pub fn upsert_queue_schedule(&self, schedule: &QueueSchedule) -> Result<QueueSchedule> {
        if schedule.queue_id.trim().is_empty() || schedule.start_at < 0 {
            return Err(StorageError::InvalidScheduleConfiguration(
                "queue id and non-negative start time are required".to_owned(),
            ));
        }
        if schedule.kind == ScheduleKind::Repeating
            && schedule.interval_seconds.is_none_or(|value| value == 0)
        {
            return Err(StorageError::InvalidScheduleConfiguration(
                "repeating schedules require a positive interval".to_owned(),
            ));
        }
        let window_is_valid = |minute: Option<u16>| minute.is_none_or(|value| value < 24 * 60);
        if !window_is_valid(schedule.window_start_minute)
            || !window_is_valid(schedule.window_end_minute)
            || schedule.window_start_minute.is_some() != schedule.window_end_minute.is_some()
        {
            return Err(StorageError::InvalidScheduleConfiguration(
                "a time window needs a start and an end within the day".to_owned(),
            ));
        }
        if schedule.kind == ScheduleKind::Weekdays && schedule.weekdays_mask == 0 {
            return Err(StorageError::InvalidScheduleConfiguration(
                "choose at least one weekday".to_owned(),
            ));
        }
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO queue_schedules
             (queue_id, enabled, kind, start_at, stop_at, weekdays_mask, interval_seconds,
              completion_action, prevent_sleep, updated_at, window_start_minute,
              window_end_minute)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(queue_id) DO UPDATE SET
                enabled = excluded.enabled, kind = excluded.kind, start_at = excluded.start_at,
                stop_at = excluded.stop_at, weekdays_mask = excluded.weekdays_mask,
                interval_seconds = excluded.interval_seconds,
                completion_action = excluded.completion_action,
                prevent_sleep = excluded.prevent_sleep, updated_at = excluded.updated_at,
                window_start_minute = excluded.window_start_minute,
                window_end_minute = excluded.window_end_minute;",
            params![
                schedule.queue_id,
                schedule.enabled,
                schedule.kind.as_str(),
                schedule.start_at,
                schedule.stop_at,
                i64::from(schedule.weekdays_mask),
                schedule.interval_seconds.map(|value| value as i64),
                schedule.completion_action.as_str(),
                schedule.prevent_sleep,
                schedule.updated_at,
                schedule.window_start_minute,
                schedule.window_end_minute,
            ],
        )?;
        drop(connection);
        self.get_queue_schedule(&schedule.queue_id)?.ok_or(
            StorageError::InvalidScheduleConfiguration(
                "schedule could not be read back".to_owned(),
            ),
        )
    }
}

struct ScheduleRow {
    queue_id: String,
    enabled: bool,
    kind: String,
    start_at: i64,
    stop_at: Option<i64>,
    weekdays_mask: i64,
    interval_seconds: Option<i64>,
    completion_action: String,
    prevent_sleep: bool,
    updated_at: i64,
    window_start_minute: Option<u16>,
    window_end_minute: Option<u16>,
}

impl ScheduleRow {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            queue_id: row.get(0)?,
            enabled: row.get(1)?,
            kind: row.get(2)?,
            start_at: row.get(3)?,
            stop_at: row.get(4)?,
            weekdays_mask: row.get(5)?,
            interval_seconds: row.get(6)?,
            completion_action: row.get(7)?,
            prevent_sleep: row.get(8)?,
            updated_at: row.get(9)?,
            window_start_minute: row.get(10)?,
            window_end_minute: row.get(11)?,
        })
    }

    fn into_record(self) -> Result<QueueSchedule> {
        let weekdays_mask = u8::try_from(self.weekdays_mask).map_err(|_| {
            StorageError::InvalidScheduleConfiguration("weekday mask is out of range".to_owned())
        })?;
        let interval_seconds = self
            .interval_seconds
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    StorageError::InvalidScheduleConfiguration("interval is negative".to_owned())
                })
            })
            .transpose()?;
        Ok(QueueSchedule {
            queue_id: self.queue_id,
            enabled: self.enabled,
            kind: ScheduleKind::from_str(&self.kind).map_err(|_| {
                StorageError::InvalidScheduleConfiguration("unknown schedule kind".to_owned())
            })?,
            start_at: self.start_at,
            stop_at: self.stop_at,
            weekdays_mask,
            interval_seconds,
            completion_action: CompletionAction::from_str(&self.completion_action).map_err(
                |_| {
                    StorageError::InvalidScheduleConfiguration(
                        "unknown completion action".to_owned(),
                    )
                },
            )?,
            prevent_sleep: self.prevent_sleep,
            updated_at: self.updated_at,
            window_start_minute: self.window_start_minute,
            window_end_minute: self.window_end_minute,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn schedules_persist_and_validate_repeating_intervals() {
        let root = tempdir().unwrap();
        let storage = Storage::open(root.path().join("downloads.db")).unwrap();
        let schedule = QueueSchedule {
            queue_id: "default".to_owned(),
            enabled: true,
            kind: ScheduleKind::Daily,
            start_at: 1_000,
            stop_at: Some(3_600),
            weekdays_mask: 0b0111_1111,
            interval_seconds: None,
            completion_action: CompletionAction::Notify,
            prevent_sleep: true,
            updated_at: 1_000,
            window_start_minute: Some(120),
            window_end_minute: Some(420),
        };
        assert_eq!(storage.upsert_queue_schedule(&schedule).unwrap(), schedule);
        assert_eq!(
            storage.list_queue_schedules().unwrap(),
            vec![schedule.clone()]
        );

        let invalid = QueueSchedule {
            kind: ScheduleKind::Repeating,
            ..schedule.clone()
        };
        assert!(matches!(
            storage.upsert_queue_schedule(&invalid),
            Err(StorageError::InvalidScheduleConfiguration(_))
        ));

        let half_window = QueueSchedule {
            window_end_minute: None,
            ..schedule.clone()
        };
        assert!(storage.upsert_queue_schedule(&half_window).is_err());

        let no_weekday = QueueSchedule {
            kind: ScheduleKind::Weekdays,
            weekdays_mask: 0,
            ..schedule
        };
        assert!(storage.upsert_queue_schedule(&no_weekday).is_err());
    }
}
