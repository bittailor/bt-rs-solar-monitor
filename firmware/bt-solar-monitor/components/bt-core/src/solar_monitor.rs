use embassy_futures::yield_now;
use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};
use micropb::heapless::Vec;
use micropb::{MessageEncode, PbEncoder};

use crate::proto::bt_::solar_::UploadEntry;
use crate::{proto::bt_::solar_::Upload, sensor::ve_direct::Reading, time::UtcTime};

const UPLOAD_MAX_MESSAGE_SIZE: usize = Upload::MAX_SIZE.expect("Size known at compile time");
type UploadBuffer = Vec<u8, UPLOAD_MAX_MESSAGE_SIZE>;

pub struct Runner<'a, M: RawMutex, const N: usize> {
    reading_receiver: Receiver<'a, M, Reading, N>,
    upload: Option<Upload>,
}

pub fn new<'a, M: RawMutex, const N: usize>(reading_receiver: Receiver<'a, M, Reading, N>) -> Runner<'a, M, N> {
    Runner {
        reading_receiver,
        upload: None,
    }
}

impl<'a, M: RawMutex, const N: usize> Runner<'a, M, N> {
    pub async fn run(mut self) {
        loop {
            yield_now().await;
            self.run_once().await;
        }
    }

    async fn run_once(&mut self) {
        let reading = self.reading_receiver.receive().await;
        info!("VE.Reading> {:?}", reading);
        self.handle_reading(reading).await;
    }

    async fn handle_reading(&mut self, reading: Reading) -> Option<UploadBuffer> {
        match UtcTime::now().await {
            Some(timestamp) => {
                let mut entry = UploadEntry::default().init_offset_in_seconds(0).init_reading(reading.into());
                match self.upload {
                    Some(ref mut upload) => {
                        entry.set_offset_in_seconds((timestamp.and_utc().timestamp() - upload.start_timestamp) as i32);
                        let _ = upload.entries.push(entry);
                    }
                    None => {
                        let mut new_upload = Upload {
                            start_timestamp: timestamp.and_utc().timestamp(),
                            entries: Vec::new(),
                        };
                        let _ = new_upload.entries.push(entry);
                        self.upload = Some(new_upload);
                    }
                }
            }
            None => {
                warn!("Skipping reading upload: system time not synchronized yet");
                return None;
            }
        };
        if let Some(ref mut upload) = self.upload
            && upload.entries.is_full()
        {
            let upload = self.upload.take().unwrap();
            info!("Uploading {} readings", upload.entries.len());
            let mut upload_buffer = UploadBuffer::new();
            let mut encoder = PbEncoder::new(&mut upload_buffer);
            match upload.encode(&mut encoder) {
                Ok(_) => {
                    info!("Upload encoded ({} bytes)", upload_buffer.len());
                    return Some(upload_buffer);
                }
                Err(e) => {
                    error!("Failed to encode upload: {:?}", e);
                }
            }
        }
        None
    }
}

impl From<Reading> for crate::proto::bt_::solar_::Reading {
    fn from(reading: Reading) -> Self {
        const MILLI_FACTOR: f32 = 1000.0;
        Self {
            battery_voltage: (reading.battery_voltage * MILLI_FACTOR) as i32,
            battery_current: (reading.battery_current * MILLI_FACTOR) as i32,
            panel_voltage: (reading.panel_voltage * MILLI_FACTOR) as i32,
            panel_power: reading.panel_power as i32,
            load_current: (reading.load_current * MILLI_FACTOR) as i32,
        }
    }
}

#[cfg(test)]
pub mod tests {
    use chrono::{Duration, NaiveDateTime};
    use embassy_sync::blocking_mutex::raw::NoopRawMutex;
    use micropb::MessageDecode;
    use serial_test::serial;

    use super::*;

    #[serial(bt_time)]
    #[tokio::test]
    async fn check_handle_reading() {
        let startup = NaiveDateTime::parse_from_str("2025-11-30 12:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        UtcTime::time_sync(startup).await;
        let channel = embassy_sync::channel::Channel::<NoopRawMutex, _, 10>::new();
        let mut runner = super::new(channel.receiver());
        let mut uploads = Vec::<UploadBuffer, 4>::new();
        for i in 0..24 {
            let f = i as f32 / 10.0;
            let reading = Reading {
                battery_voltage: (10.0 + f),
                battery_current: (2.0 * f),
                panel_voltage: (18.0 + f),
                panel_power: (5.0 + f),
                load_current: (1.0 * f),
            };
            UtcTime::time_sync(startup + Duration::minutes(5) * i).await;
            if let Some(upload) = runner.handle_reading(reading).await {
                uploads.push(upload).unwrap();
            }
        }
        assert_eq!(uploads.len(), 2);

        let mut first = Upload::default();
        first.decode_from_bytes(&uploads[0]).unwrap();
        assert_eq!(first.start_timestamp, startup.and_utc().timestamp());
        assert_eq!(first.entries.len(), 12);
        assert_eq!(first.entries[0].offset_in_seconds, 0);
        assert_eq!(first.entries[1].offset_in_seconds, 60 * 5);
        assert_eq!(first.entries[11].offset_in_seconds, (60 * 5) * 11);

        let mut second = Upload::default();
        second.decode_from_bytes(&uploads[1]).unwrap();
        assert_eq!(second.start_timestamp, (startup + Duration::minutes(5) * 12).and_utc().timestamp());
        assert_eq!(second.entries.len(), 12);
        assert_eq!(first.entries[0].offset_in_seconds, 0);
        assert_eq!(first.entries[1].offset_in_seconds, 60 * 5);
        assert_eq!(first.entries[11].offset_in_seconds, (60 * 5) * 11);
    }
}
