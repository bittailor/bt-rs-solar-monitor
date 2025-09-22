use heapless::format;
use nom::{Parser, bytes::complete::tag};

use crate::{
    at::{AtClient, AtController, AtError},
    at_request,
};

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SleepMode {
    Off = 0,
    DtrSleep = 1,
    RxSleep = 2,
}

impl TryFrom<u32> for SleepMode {
    type Error = AtError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SleepMode::Off),
            1 => Ok(SleepMode::DtrSleep),
            2 => Ok(SleepMode::RxSleep),
            _ => Err(AtError::EnumParseError(format!("Invalid SleepMode value: {}", value).unwrap_or_default())),
        }
    }
}

pub async fn set_sleep_mode<'ch, Ctr: AtController>(client: &impl AtClient<'ch, Ctr>, mode: SleepMode) -> Result<(), AtError> {
    at_request!("AT+CSCLK={}", mode as i32).send(client).await?;
    Ok(())
}

pub async fn read_sleep_mode<'ch, Ctr: AtController>(client: &impl AtClient<'ch, Ctr>) -> Result<SleepMode, AtError> {
    let response = at_request!("AT+CSCLK?").send(client).await?;
    let (_, (_, mode)) = (tag("+CSCLK: "), nom::character::complete::u32).parse(response.line(0)?)?;
    mode.try_into()
}

#[cfg(test)]
pub mod mocks {
    /*
    use super::*;
    use crate::at::mocks::mock_request;

    #[tokio::test]
    async fn test_read_sleep_mode() -> Result<(), AtError> {
        let mock = mock_request("AT+CSCLK?", &["+CSCLK: 0"]);
        let mode = read_sleep_mode(&mock).await?;
        assert_eq!(mode, SleepMode::Off);

        let mock = mock_request("AT+CSCLK?", &["+CSCLK: 1"]);
        let mode = read_sleep_mode(&mock).await?;
        assert_eq!(mode, SleepMode::DtrSleep);

        let mock = mock_request("AT+CSCLK?", &["+CSCLK: 2"]);
        let mode = read_sleep_mode(&mock).await?;
        assert_eq!(mode, SleepMode::RxSleep);

        Ok(())
    }
    */
}
