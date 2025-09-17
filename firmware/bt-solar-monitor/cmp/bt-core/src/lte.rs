pub mod at;

use core::str::{self};

use embedded_io_async::{Read, Write};

use crate::lte::at::{AtError, SleepMode};

pub struct State {
    at_state: at::State,
}

impl State {
    pub fn new() -> Self {
        Self { at_state: at::State::new() }
    }
}

pub async fn new_lte<'a, S: Read + Write>(state: &'a mut State, stream: S) -> (Lte<'a>, at::Runner<'a, S>) {
    let runner = at::Runner::new(stream, state.at_state.tx_channel.receiver(), state.at_state.rx_channel.sender());
    let lte = Lte {
        at_ctr: at::AtClient::new(state.at_state.tx_channel.sender(), state.at_state.rx_channel.receiver()),
    };
    (lte, runner)
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LteError {
    Timeout,
    Error,
    AtError(AtError),
}

impl From<AtError> for LteError {
    fn from(err: AtError) -> Self {
        LteError::AtError(err)
    }
}

impl From<core::fmt::Error> for LteError {
    fn from(_err: core::fmt::Error) -> Self {
        LteError::Error
    }
}

pub struct Lte<'ch> {
    at_ctr: at::AtClient<'ch>,
}

impl Lte<'_> {
    pub async fn at(&self) -> Result<(), LteError> {
        at::at(&self.at_ctr).await.map_err(Into::into)
    }

    pub async fn set_apn(&self, apn: &str) -> Result<(), LteError> {
        at::set_apn(&self.at_ctr, apn).await.map_err(Into::into)
    }

    pub async fn read_network_registration(&self) -> Result<(at::NetworkRegistrationUrcConfig, at::NetworkRegistrationState), LteError> {
        at::get_network_registration(&self.at_ctr).await.map_err(Into::into)
    }

    // AT+CSCLK
    pub async fn read_sleep_mode(&self) -> Result<SleepMode, LteError> {
        at::read_sleep_mode(&self.at_ctr).await.map_err(Into::into)
    }

    pub async fn set_sleep_mode(&self, mode: SleepMode) -> Result<(), LteError> {
        at::set_sleep_mode(&self.at_ctr, mode).await.map_err(Into::into)
    }
}
