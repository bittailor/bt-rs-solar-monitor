use crate::{
    at::{AtClient, AtController, AtError},
    at_request,
};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use heapless::format;
use nom::{Parser, branch::alt, bytes::complete::tag};

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

fn parse_rtc_date(input: &str) -> nom::IResult<&str, NaiveDate> {
    let (remaining, (year, _, month, _, day)) =
        (nom::character::complete::i32, tag("/"), nom::character::complete::u32, tag("/"), nom::character::complete::u32).parse(input)?;

    let date = NaiveDate::from_ymd_opt(year + 2000, month, day).ok_or(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Fail)))?;
    Ok((remaining, date))
}

fn parse_rtc_time(input: &str) -> nom::IResult<&str, NaiveTime> {
    let (remaining, (hour, _, min, _, sec, sign, tz)) = (
        nom::character::complete::u32,
        tag(":"),
        nom::character::complete::u32,
        tag(":"),
        nom::character::complete::u32,
        alt((tag("+"), tag("-"))),
        nom::character::complete::u32,
    )
        .parse(input)?;
    let local = NaiveTime::from_hms_opt(hour, min, sec).ok_or(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Fail)))?;
    let offset = chrono::Duration::minutes((15 * tz).into()); // time zone (indicates the difference, expressed in quarters of an hour, between the local time and GMT
    let time = match sign {
        "+" => local - offset,
        "-" => local + offset,
        _ => local,
    };
    Ok((remaining, time))
}

fn parse_rtc_date_time(input: &str) -> nom::IResult<&str, NaiveDateTime> {
    let (remaining, (date, _, time)) = (parse_rtc_date, tag(","), parse_rtc_time).parse(input)?;
    let native_date_time = date.and_time(time);
    Ok((remaining, native_date_time))
}

// AT+CCLK?
// +CCLK: "25/11/24,21:19:07+04"
pub async fn query_real_time_clock<'ch, Ctr: AtController>(ctr: &impl AtClient<'ch, Ctr>) -> Result<NaiveDateTime, AtError> {
    let response = at_request!("AT+CCLK?").send(ctr).await?;
    let (_, (_, date_time, _)) = (tag("+CCLK: \""), parse_rtc_date_time, tag("\"")).parse(response.line(0)?)?;
    Ok(date_time)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_parse_rtc_date_time() {
        let input = "25/11/24,21:19:07+00";
        let (_, date_time) = parse_rtc_date_time(input).unwrap();
        assert_eq!(date_time.year(), 2025);
        assert_eq!(date_time.month(), 11);
        assert_eq!(date_time.day(), 24);
        assert_eq!(date_time.hour(), 21);
        assert_eq!(date_time.minute(), 19);
        assert_eq!(date_time.second(), 7);

        let input = "25/11/24,21:19:07+04";
        let (_, date_time) = parse_rtc_date_time(input).unwrap();
        assert_eq!(date_time.year(), 2025);
        assert_eq!(date_time.month(), 11);
        assert_eq!(date_time.day(), 24);
        assert_eq!(date_time.hour(), 20);
        assert_eq!(date_time.minute(), 19);
        assert_eq!(date_time.second(), 7);

        let input = "25/11/24,21:19:07-04";
        let (_, date_time) = parse_rtc_date_time(input).unwrap();
        assert_eq!(date_time.year(), 2025);
        assert_eq!(date_time.month(), 11);
        assert_eq!(date_time.day(), 24);
        assert_eq!(date_time.hour(), 22);
        assert_eq!(date_time.minute(), 19);
        assert_eq!(date_time.second(), 7);

        let input = "14/01/01,02:14:36+08";
        let (_, date_time) = parse_rtc_date_time(input).unwrap();
        assert_eq!(date_time.year(), 2014);
        assert_eq!(date_time.month(), 1);
        assert_eq!(date_time.day(), 1);
        assert_eq!(date_time.hour(), 0);
        assert_eq!(date_time.minute(), 14);
        assert_eq!(date_time.second(), 36);
    }
}
