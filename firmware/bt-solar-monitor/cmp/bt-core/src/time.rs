use crate::fmt::FormatableNaiveDateTime;
use chrono::{Duration, NaiveDateTime};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex};
use embassy_time::Instant;

static SYSTEM_BOOT_TIME: Mutex<CriticalSectionRawMutex, Option<NaiveDateTime>> = Mutex::new(None);

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
