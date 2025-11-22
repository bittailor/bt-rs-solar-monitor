use crate::{
    at::{AtClient, AtController, AtError},
    at_request,
};
use heapless::{String, format};
use nom::{
    Parser,
    bytes::complete::{is_not, tag},
};

pub struct Rssi(i32);

impl core::fmt::Display for Rssi {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} dBm", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Rssi {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{} dBm", self.0);
    }
}

impl From<Rssi> for i32 {
    fn from(value: Rssi) -> Self {
        value.0
    }
}

// AT+CSQ
// +CSQ: <rssi>,<ber>
pub async fn query_signal_quality<'ch, Ctr: AtController>(ctr: &impl AtClient<'ch, Ctr>) -> Result<(Rssi, u32), AtError> {
    let response = at_request!("AT+CSQ").send(ctr).await?;
    let (_, (_, raw_rssi, _, raw_ber)) = (tag("+CSQ: "), nom::character::complete::i32, tag(","), nom::character::complete::u32).parse(response.line(0)?)?;
    let rssi = match raw_rssi {
        0..=31 => Rssi(-113 + (raw_rssi * 2)),
        99 => return Err(AtError::EnumParseError("Signal strength not known or not detectable".try_into()?)),
        _ => return Err(AtError::EnumParseError(format!("Invalid RSSI value: {}", raw_rssi)?)),
    };
    Ok((rssi, raw_ber))
}

// AT+CPOF
pub async fn power_down<'ch, Ctr: AtController>(ctr: &impl AtClient<'ch, Ctr>) -> Result<(), AtError> {
    at_request!("AT+CPOF").send(ctr).await?;
    Ok(())
}

// AT+CCLK?
// +CCLK: "14/01/01,02:14:36+08"
pub async fn query_real_time_clock<'ch, Ctr: AtController>(ctr: &impl AtClient<'ch, Ctr>) -> Result<String<64>, AtError> {
    let response = at_request!("AT+CCLK?").send(ctr).await?;
    //let (_, (_, time, _)) = (tag("+CCLK: \""), nom::character::complete::alphanumeric1, tag("\"")).parse(response.line(0)?)?;
    let (_, (_, time, _)) = (tag("+CCLK: \""), is_not("\""), tag("\"")).parse(response.line(0)?)?;
    Ok(time.try_into()?)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::at::mocks::mock_request;

    #[tokio::test]
    async fn test_query_real_time_clock() -> Result<(), AtError> {
        let mock = mock_request("AT+CCLK?", &["+CCLK: \"70/01/01,00:00:10+00\""]);
        let time = query_real_time_clock(&mock).await?;
        assert_eq!(time, "70/01/01,00:00:10+00");
        Ok(())
    }
}
