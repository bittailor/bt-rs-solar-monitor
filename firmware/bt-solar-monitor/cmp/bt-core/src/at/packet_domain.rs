use crate::{
    at_request,
    at::{AtClient, AtError},
};

pub async fn set_apn(client: &impl AtClient, apn: &str) -> Result<(), AtError> {
    at_request!("AT+CGDCONT=1,\"IP\",\"{}\"", apn).send(client).await?;
    Ok(())
}
