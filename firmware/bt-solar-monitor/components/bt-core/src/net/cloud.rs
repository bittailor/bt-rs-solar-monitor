use const_format::concatcp;
use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_hal::digital::OutputPin;
use heapless::Vec;
use micropb::{MessageEncode, PbEncoder};

use crate::{
    at::AtController,
    net::cellular::{CellularError, sim_com_a67::SimComCellularModule},
    proto::bt_::solar_::{OfflineEvent, OnlineEvent, StartupEvent, SystemEvent, SystemEvent_::Event},
    time::UtcTime,
};

pub const SOLAR_BACKEND_BASE_URL: &str = env!("SOLAR_BACKEND_BASE_URL");

const SOLAR_BACKEND_TOKEN: &str = env!("SOLAR_BACKEND_TOKEN");

pub struct Runner<'ch, 'a, Output: OutputPin, Ctr: AtController, M: RawMutex, const B: usize, const N: usize> {
    cloud_controller: CloudController<'ch, 'a, Output, Ctr, M, B, N>,
}

pub fn new<'ch, 'a, Output: OutputPin, Ctr: AtController, M: RawMutex, const B: usize, const N: usize>(
    module: SimComCellularModule<'ch, Output, Ctr>,
    upload_receiver: Receiver<'a, M, Vec<u8, B>, N>,
) -> Runner<'ch, 'a, Output, Ctr, M, B, N> {
    Runner {
        cloud_controller: CloudController {
            module,
            state: CloudClientState::Startup,
            upload_receiver,
        },
    }
}

impl<'ch, 'a, Output: OutputPin, Ctr: AtController, M: RawMutex, const B: usize, const N: usize> Runner<'ch, 'a, Output, Ctr, M, B, N> {
    pub async fn run(mut self) {
        loop {
            self.cloud_controller.once().await;
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum CloudClientState {
    Startup,
    Connected,
    Sleeping,
}

pub struct CloudController<'ch, 'a, Output: OutputPin, Ctr: AtController, M: RawMutex, const B: usize, const N: usize> {
    module: SimComCellularModule<'ch, Output, Ctr>,
    state: CloudClientState,
    upload_receiver: Receiver<'a, M, Vec<u8, B>, N>,
}
impl<'ch, 'a, Output: OutputPin, Ctr: AtController, M: RawMutex, const B: usize, const N: usize> CloudController<'ch, 'a, Output, Ctr, M, B, N> {
    pub async fn sleep(&mut self) -> Result<(), CellularError> {
        //self.module.set_sleep_mode(SleepMode::Enabled).await?;
        self.state = CloudClientState::Sleeping;
        Ok(())
    }

    async fn once(&mut self) {
        let result = match self.state {
            CloudClientState::Startup => self.handle_startup().await,
            CloudClientState::Connected => self.handle_connected().await,
            CloudClientState::Sleeping => self.handle_sleeping().await,
        };
        if let Err(e) = result {
            warn!("CloudClient error: {:?} => resetting module", e);
            while self.module.reset().await.is_err() {
                warn!("CloudClient reset error, retrying...");
                Timer::after_secs(30).await;
            }
            self.state = CloudClientState::Startup;
        }
    }

    async fn handle_startup(&mut self) -> Result<(), CellularError> {
        self.module.power_cycle().await?;
        self.module.startup_network("gprs.swisscom.ch").await?;
        let now = self.module.query_real_time_clock().await?;
        UtcTime::time_sync(now).await;
        self.state = CloudClientState::Connected;
        info!("CloudClient connected at {}", crate::fmt::FormatableNaiveDateTime(&now));
        self.upload_event(SystemEvent {
            timestamp: now.and_utc().timestamp(),
            event: Some(Event::StartupEvent(StartupEvent {
                uptime_seconds: Instant::now().as_secs() as u32,
            })),
        })
        .await?;
        Ok(())
    }

    async fn handle_connected(&mut self) -> Result<(), CellularError> {
        match with_timeout(Duration::from_secs(4), self.upload_receiver.receive()).await {
            Ok(data) => {
                info!("Uploading {} bytes to cloud...", data.len());
                let request = self.module.request().await?;
                request.set_header("Connection", "Keep-Alive").await?;
                request.set_header("X-Token", SOLAR_BACKEND_TOKEN).await?;
                let mut response = request
                    .post(concatcp!(SOLAR_BACKEND_BASE_URL, "/api/v2/solar/reading"), data.as_slice())
                    .await?;
                if response.status().is_ok() {
                    info!("Upload successful");
                } else {
                    warn!("Upload failed with status {}", response.status());
                }
                let body = response.body();
                if body.is_empty() {
                    info!("No response body");
                } else {
                    let mut body_buffer = [0u8; 1024];
                    info!("Response body [{}]: {}", body.len(), body.read_as_str(&mut body_buffer).await?);
                }
            }
            Err(_) => {
                if let Some(now) = UtcTime::now().await {
                    self.upload_event(SystemEvent {
                        timestamp: now.and_utc().timestamp(),
                        event: Some(Event::OfflineEvent(OfflineEvent {
                            uptime_seconds: Instant::now().as_secs() as u32,
                        })),
                    })
                    .await?;
                }
                info!("No data to upload, going to sleep...");
                self.module.set_sleep_mode(crate::at::serial_interface::SleepMode::RxSleep).await?;
                self.state = CloudClientState::Sleeping;
            }
        }
        Ok(())
    }

    async fn handle_sleeping(&mut self) -> Result<(), CellularError> {
        self.upload_receiver.ready_to_receive().await;
        self.module.wake_up().await?;
        if let Some(now) = UtcTime::now().await {
            self.upload_event(SystemEvent {
                timestamp: now.and_utc().timestamp(),
                event: Some(Event::OnlineEvent(OnlineEvent {
                    uptime_seconds: Instant::now().as_secs() as u32,
                })),
            })
            .await?;
        }
        self.state = CloudClientState::Connected;
        Ok(())
    }

    async fn upload_event(&mut self, event: SystemEvent) -> Result<(), CellularError> {
        const BUFFER_SIZE: usize = SystemEvent::MAX_SIZE.expect("Size known at compile time");
        let mut buffer = micropb::heapless::Vec::<u8, BUFFER_SIZE>::new();
        let mut encoder = PbEncoder::new(&mut buffer);
        event.encode(&mut encoder).map_err(|_| CellularError::Encoding())?;
        let request = self.module.request().await?;
        request.set_header("Connection", "Keep-Alive").await?;
        request.set_header("X-Token", SOLAR_BACKEND_TOKEN).await?;
        let mut response = request
            .post(concatcp!(SOLAR_BACKEND_BASE_URL, "/api/v2/solar/event"), buffer.as_slice())
            .await?;
        if response.status().is_ok() {
            info!("Upload successful");
        } else {
            warn!("Upload failed with status {}", response.status());
        }
        let body = response.body();
        if body.is_empty() {
            info!("No response body");
        } else {
            let mut body_buffer = [0u8; 1024];
            info!("Response body [{}]: {}", body.len(), body.read_as_str(&mut body_buffer).await?);
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use chrono::NaiveDateTime;
    use reqwest::header;
    use serial_test::serial;
    use std::fs;

    use super::*;

    #[serial(bt_time)]
    #[tokio::test]
    #[ignore]
    async fn check_startup_event() {
        let startup = NaiveDateTime::parse_from_str("2025-11-30 12:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let mut event = SystemEvent::default();
        event.timestamp = startup.and_utc().timestamp();
        event.event = Some(Event::StartupEvent(StartupEvent { uptime_seconds: 123 }));
        let mut body_data = std::vec::Vec::default();
        let mut encoder = PbEncoder::new(&mut body_data);
        event.encode(&mut encoder).unwrap();
        let client = reqwest::Client::new();
        let res = client
            .post(concatcp!(SOLAR_BACKEND_BASE_URL, "/api/v2/solar/event"))
            .header("X-TOKEN", SOLAR_BACKEND_TOKEN)
            .body(body_data)
            .send()
            .await
            .unwrap();
        let success = res.status().is_success();
        res.headers().iter().for_each(|(k, v)| {
            println!("Header: {}: {:?}", k, v);
        });
        let text = res.text().await.unwrap();
        if !success {
            fs::write("error.html", text).unwrap();
            println!("Error response: {}/error.html", std::env::current_dir().unwrap().display());
        } else {
            println!("Response: {:?}", text);
        }
        assert!(success);
    }
}
