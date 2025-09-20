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

pub async fn action(client: &impl AtClient, action: HttpAction) -> Result<(), AtError> {
    at_request!("AT+HTTPACTION={}", action as u32).send(client).await?;
    Ok(())
}
