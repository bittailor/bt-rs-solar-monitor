use crate::{
    at::{AtClient, AtController, AtError},
    at_request,
};
use heapless::format;
use nom::{Parser, bytes::complete::tag};

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
