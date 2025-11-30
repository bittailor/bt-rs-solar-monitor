use crate::fmt::FormatableNaiveDateTime;
use chrono::{Duration, NaiveDateTime};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Instant;

static SYSTEM_BOOT_TIME: Mutex<CriticalSectionRawMutex, Option<NaiveDateTime>> = Mutex::new(None);

pub struct UtcTime {}

impl UtcTime {
    pub async fn time_sync(now: NaiveDateTime) {
        let since_system_boot = Instant::now();
        let new_system_boot_time = now - Duration::seconds(since_system_boot.as_secs() as i64);
        let mut guard = SYSTEM_BOOT_TIME.lock().await;
        match *guard {
            Some(current_system_boot_time) => {
                if current_system_boot_time != new_system_boot_time {
                    *guard = Some(new_system_boot_time);
                    let drift = new_system_boot_time - current_system_boot_time;
                    info!("System time re-synchronized: {} (drift: {} seconds)", FormatableNaiveDateTime(&now), drift.num_seconds());
                }
            }
            None => {
                *guard = Some(new_system_boot_time);
                info!("System time initially synchronized: {}", FormatableNaiveDateTime(&now));
            }
        };
    }

    pub async fn now() -> Option<NaiveDateTime> {
        let guard = SYSTEM_BOOT_TIME.lock().await;
        match *guard {
            Some(system_boot_time) => {
                let since_system_boot = Instant::now();
                Some(system_boot_time + Duration::seconds(since_system_boot.as_secs() as i64))
            }
            None => None,
        }
    }

    #[cfg(test)]
    async fn reset() {
        let mut guard = SYSTEM_BOOT_TIME.lock().await;
        *guard = None;
    }
}

#[cfg(test)]
pub mod tests {
    use serial_test::serial;

    use super::*;

    #[serial(bt_time)]
    #[tokio::test]
    async fn test_now_not_sync_yet() {
        UtcTime::reset().await;
        let now = UtcTime::now().await;
        assert!(now.is_none());
    }

    #[serial(bt_time)]
    #[tokio::test]
    async fn test_now_sync() {
        let sync = NaiveDateTime::parse_from_str("2025-11-30 12:30:21", "%Y-%m-%d %H:%M:%S").unwrap();
        UtcTime::time_sync(sync).await;
        let now = UtcTime::now().await;
        assert!(now.is_some());
        assert_eq!(now.unwrap(), sync);
    }

    #[serial(bt_time)]
    #[tokio::test]
    async fn test_now_sync_and_then_check_again() {
        let sync = NaiveDateTime::parse_from_str("2025-11-30 12:30:21", "%Y-%m-%d %H:%M:%S").unwrap();
        UtcTime::time_sync(sync).await;
        let now = UtcTime::now().await;
        assert!(now.is_some());
        assert_eq!(now.unwrap(), sync);
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let now = super::UtcTime::now().await;
        assert!(now.is_some());
        assert_eq!(now.unwrap(), sync + Duration::seconds(2));
    }

    #[serial(bt_time)]
    #[tokio::test]
    async fn test_now_sync_and_then_resync() {
        let sync_one = NaiveDateTime::parse_from_str("2025-11-30 12:30:21", "%Y-%m-%d %H:%M:%S").unwrap();
        UtcTime::time_sync(sync_one).await;
        let now_one = UtcTime::now().await;
        assert!(now_one.is_some());
        assert_eq!(now_one.unwrap(), sync_one);
        let sync_two = NaiveDateTime::parse_from_str("2025-11-30 12:45:34", "%Y-%m-%d %H:%M:%S").unwrap();
        UtcTime::time_sync(sync_two).await;
        let now_two = super::UtcTime::now().await;
        assert_eq!(now_two.unwrap(), sync_two);
    }
}
