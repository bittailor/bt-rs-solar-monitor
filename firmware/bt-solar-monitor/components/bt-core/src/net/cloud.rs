use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};
use embassy_time::{Duration, Timer, with_timeout};
use embedded_hal::digital::OutputPin;
use heapless::Vec;

use crate::{
    at::AtController,
    net::cellular::{CellularError, sim_com_a67::SimComCellularModule},
    time::UtcTime,
};

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
        Ok(())
    }

    async fn handle_connected(&mut self) -> Result<(), CellularError> {
        match with_timeout(Duration::from_secs(10), self.upload_receiver.receive()).await {
            Ok(data) => {
                info!("Uploading {} bytes to cloud...", data.len());
                let request = self.module.request().await?;
                let mut response = request.post("http://api.solar.bockmattli.ch/api/v2/solar/reading", data.as_slice()).await?;
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
        self.state = CloudClientState::Connected;
        Ok(())
    }
}
