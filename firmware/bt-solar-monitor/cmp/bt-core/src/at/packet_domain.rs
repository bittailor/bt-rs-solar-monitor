use embedded_io_async::{Read, Write};

use crate::{
    at::{AtClient, AtController, AtError},
    at_request,
};

pub async fn set_apn<'ch, Ctr: AtController>(client: &impl AtClient<'ch, Ctr>, apn: &str) -> Result<(), AtError> {
    at_request!("AT+CGDCONT=1,\"IP\",\"{}\"", apn).send(client).await?;
    Ok(())
}
