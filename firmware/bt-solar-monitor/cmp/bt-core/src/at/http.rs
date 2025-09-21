use crate::{
    at::{AtClient, AtError},
    at_request,
};
use nom::{Parser, bytes::complete::tag};

pub enum HttpAction {
    Get = 0,
    Post = 1,
    Head = 2,
    Delete = 3,
}

pub struct HttpStatusCode(u32);

impl HttpStatusCode {
    pub fn is_ok(&self) -> bool {
        self.0 >= 200 && self.0 < 300
    }
}

impl core::fmt::Display for HttpStatusCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for HttpStatusCode {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", self.0);
    }
}

pub async fn init(client: &impl AtClient) -> Result<(), AtError> {
    at_request!("AT+HTTPINIT").send(client).await?;
    Ok(())
}

pub async fn term(client: &impl AtClient) -> Result<(), AtError> {
    at_request!("AT+HTTPTERM").send(client).await?;
    Ok(())
}

pub async fn set_url(client: &impl AtClient, url: &str) -> Result<(), AtError> {
    at_request!("AT+HTTPPARA=\"URL\",\"{}\"", url).send(client).await?;
    Ok(())
}

pub async fn action(client: &impl AtClient, action: HttpAction) -> Result<(HttpStatusCode, usize), AtError> {
    let response = at_request!("AT+HTTPACTION={}", action as u32)
        .with_urc_prefix("+HTTPACTION: ".try_into()?)
        .send(client)
        .await?;
    let (_, (_, _action, _, status_code, _, data_len)) =
        (tag("+HTTPACTION: "), nom::character::complete::u32, tag(","), nom::character::complete::u32, tag(","), nom::character::complete::usize)
            .parse(response.line(0)?)?;

    Ok((HttpStatusCode(status_code), data_len))
}
