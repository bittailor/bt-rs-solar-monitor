use embassy_futures::yield_now;
use embassy_sync::channel::Sender;
use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};
use heapless::Vec;
use micropb::{MessageEncode, PbEncoder, PbWrite};

use crate::proto::bt_::solar_::UploadEntry;
use crate::{proto::bt_::solar_::Upload, sensor::ve_direct::Reading, time::UtcTime};

const UPLOAD_MAX_MESSAGE_SIZE: usize = Upload::MAX_SIZE.expect("Size known at compile time");
type UploadVec = Vec<u8, UPLOAD_MAX_MESSAGE_SIZE>;
struct UploadBuffer(UploadVec);

impl UploadBuffer {
    pub fn new() -> Self {
        UploadBuffer(Vec::new())
    }
}

impl PbWrite for UploadBuffer {
    type Error = heapless::CapacityError;

    #[inline]
    fn pb_write(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        self.0.extend_from_slice(data)
    }
}

pub struct Runner<'a, 'b, M: RawMutex, const NRECEIVER: usize, const NSENDER: usize> {
    reading_receiver: Receiver<'a, M, Reading, NRECEIVER>,
    upload_sender: Sender<'b, M, UploadVec, NSENDER>,
    upload: Option<Upload>,
}

pub fn new<'a, 'b, M: RawMutex, const NRECEIVER: usize, const NSENDER: usize>(
    reading_receiver: Receiver<'a, M, Reading, NRECEIVER>,
    upload_sender: Sender<'b, M, UploadVec, NSENDER>,
) -> Runner<'a, 'b, M, NRECEIVER, NSENDER> {
    Runner {
        reading_receiver,
        upload_sender,
        upload: None,
    }
}

impl<'a, 'b, M: RawMutex, const NRECEIVER: usize, const NSENDER: usize> Runner<'a, 'b, M, NRECEIVER, NSENDER> {
    pub async fn run(mut self) {
        loop {
            yield_now().await;
            self.run_once().await;
        }
    }

    async fn run_once(&mut self) {
        let reading = self.reading_receiver.receive().await;
        info!("VE.Reading> {:?}", reading);
        if let Some(upload) = self.handle_reading(reading).await {
            self.upload_sender.send(upload).await;
        }
    }

    async fn handle_reading(&mut self, reading: Reading) -> Option<UploadVec> {
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
                            entries: micropb::heapless::Vec::new(),
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
                    info!("Upload encoded ({} bytes)", upload_buffer.0.len());
                    return Some(upload_buffer.0);
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
        let sensor_channel = embassy_sync::channel::Channel::<NoopRawMutex, _, 10>::new();
        let upload_channel = embassy_sync::channel::Channel::<NoopRawMutex, _, 4>::new();
        let mut runner = super::new(sensor_channel.receiver(), upload_channel.sender());
        let mut runner = super::new(sensor_channel.receiver(), upload_channel.sender());
        let uploads = create_uploads(&mut runner, startup).await;
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

    #[serial(bt_time)]
    #[tokio::test]
    #[ignore]
    async fn check_server_upload() {
        let startup = NaiveDateTime::parse_from_str("2025-11-30 12:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        UtcTime::time_sync(startup).await;
        let sensor_channel = embassy_sync::channel::Channel::<NoopRawMutex, _, 10>::new();
        let upload_channel = embassy_sync::channel::Channel::<NoopRawMutex, _, 4>::new();
        let mut runner = super::new(sensor_channel.receiver(), upload_channel.sender());
        let uploads = create_uploads(&mut runner, startup).await;

        assert_eq!(uploads.len(), 2);
        let body_data = std::vec::Vec::from(uploads[0].as_slice());
        let client = reqwest::Client::new();
        let res = client.post("http://localhost:8000/api/v2/solar").body(body_data).send().await.unwrap();
        assert!(res.status().is_success());
        println!("Response: {:?}", res.text().await.unwrap());
    }

    async fn create_uploads<'a, 'b, M: RawMutex, const NRECEIVER: usize, const NSENDER: usize>(
        runner: &mut Runner<'a, 'b, M, NRECEIVER, NSENDER>,
        startup: NaiveDateTime,
    ) -> Vec<UploadVec, 4> {
        let mut uploads = Vec::<UploadVec, 4>::new();
        for i in 0..24 {
            let f = i as f32 / 10.0;
            let reading = Reading {
                battery_voltage: (10.0 + f),
                battery_current: (2.0 + f),
                panel_voltage: (18.0 + f),
                panel_power: (5.0 + f),
                load_current: (1.0 + f),
            };
            UtcTime::time_sync(startup + Duration::minutes(5) * i).await;
            if let Some(upload) = runner.handle_reading(reading).await {
                uploads.push(upload).unwrap();
            }
        }
        uploads
    }
}
