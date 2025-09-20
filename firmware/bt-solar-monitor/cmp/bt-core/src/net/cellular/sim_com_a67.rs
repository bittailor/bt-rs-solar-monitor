use core::str::{self};

use embassy_time::{Duration, Timer, WithTimeout};
use embedded_hal::digital::OutputPin;
use embedded_io_async::{Read, Write};

use crate::{
    at::{serial_interface::SleepMode, status_control::Rssi},
    net::cellular::CellularError,
};

pub struct State {
    at_state: crate::at::State,
}

impl State {
    pub fn new() -> Self {
        Self {
            at_state: crate::at::State::new(),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn new<'a, S: Read + Write, Output: OutputPin>(
    state: &'a mut State,
    stream: S,
    pwrkey: Output,
    reset: Output,
) -> (CellularModule<'a, Output>, crate::at::Runner<'a, S>) {
    let runner = crate::at::Runner::new(stream, state.at_state.tx_channel.receiver(), state.at_state.rx_channel.sender());
    let lte = CellularModule {
        at_client: crate::at::AtClientImpl::new(state.at_state.tx_channel.sender(), state.at_state.rx_channel.receiver()),
        pwrkey,
        reset,
    };
    (lte, runner)
}

pub struct CellularModule<'ch, Output: OutputPin> {
    at_client: crate::at::AtClientImpl<'ch>,
    pwrkey: Output,
    reset: Output,
}

impl<Output: OutputPin> CellularModule<'_, Output> {
    pub async fn power_on(&mut self) -> Result<(), CellularError> {
        info!("power on ...");
        self.pwrkey.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(50).await;
        self.pwrkey.set_high().map_err(|_| CellularError::GpioError {})?;
        info!("... wait 8s to startup ...");
        Timer::after_secs(8).await;
        info!("... check AT ...");
        self.ensure_at(Duration::from_secs(10)).await?;
        info!("... power on done");
        Ok(())
    }

    pub async fn power_down(&self) -> Result<(), CellularError> {
        crate::at::status_control::power_down(&self.at_client).await.map_err(Into::into)
    }

    pub async fn reset(&mut self) -> Result<(), CellularError> {
        info!("reset ...");
        self.reset.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(2500).await;
        self.reset.set_high().map_err(|_| CellularError::GpioError {})?;
        info!("... wait a bit for module to start ...");
        Timer::after_millis(5000).await;
        info!("... reset done");
        Ok(())
    }

    async fn ensure_at(&self, timeout: Duration) -> Result<(), CellularError> {
        async { while crate::at::at(&self.at_client).await.is_err() {} }
            .with_timeout(timeout)
            .await
            .map_err(Into::into)
    }

    pub async fn set_apn(&self, apn: &str) -> Result<(), CellularError> {
        crate::at::packet_domain::set_apn(&self.at_client, apn).await.map_err(Into::into)
    }

    pub async fn read_network_registration(
        &self,
    ) -> Result<(crate::at::network::NetworkRegistrationUrcConfig, crate::at::network::NetworkRegistrationState), CellularError> {
        crate::at::network::get_network_registration(&self.at_client).await.map_err(Into::into)
    }

    // AT+CSCLK
    pub async fn read_sleep_mode(&self) -> Result<SleepMode, CellularError> {
        crate::at::serial_interface::read_sleep_mode(&self.at_client).await.map_err(Into::into)
    }

    pub async fn set_sleep_mode(&self, mode: SleepMode) -> Result<(), CellularError> {
        crate::at::serial_interface::set_sleep_mode(&self.at_client, mode).await.map_err(Into::into)
    }

    pub async fn query_signal_quality(&self) -> Result<Rssi, CellularError> {
        crate::at::status_control::query_signal_quality(&self.at_client)
            .await
            .map(|(rssi, _)| rssi)
            .map_err(Into::into)
    }
}

struct HttpRequest<'m, 'ch> {
    at_client: &'m crate::at::AtClientImpl<'ch>,
}

impl<'m, 'ch> HttpRequest<'m, 'ch> {
    pub fn new(at_client: &'m crate::at::AtClientImpl<'ch>) -> Self {
        Self { at_client }
    }

    pub async fn init(&self) -> Result<(), CellularError> {
        crate::at::http::init(self.at_client).await.map_err(Into::into)
    }

    pub async fn set_url(&self, url: &str) -> Result<(), CellularError> {
        crate::at::http::set_url(self.at_client, url).await.map_err(Into::into)
    }

    pub async fn get(&self) -> Result<(), CellularError> {
        crate::at::http::action(self.at_client, crate::at::http::HttpAction::Get).await?;
        todo!()
    }

    /*
    pub async fn set_content(&self, content: &str) -> Result<(), CellularError> {
        crate::at::http::set_content(self.at_client, content).await.map_err(Into::into)
    }

    pub async fn post(&self) -> Result<u16, CellularError> {
        crate::at::http::post(self.at_client).await.map_err(Into::into)
    }

    pub async fn read_response(&self, buffer: &mut [u8]) -> Result<&str, CellularError> {
        let len = crate::at::http::read_response(self.at_client, buffer).await.map_err(Into::into)?;
        str::from_utf8(&buffer[..len]).map_err(|_| CellularError::AtError(crate::at::AtError::ParseError))
    }
    */
}
