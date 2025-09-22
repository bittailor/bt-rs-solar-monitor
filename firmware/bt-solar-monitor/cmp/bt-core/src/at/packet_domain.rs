use embedded_io_async::{Read, Write};

use crate::{
    at::{AtClient, AtError},
    at_request,
};

pub async fn set_apn<'ch, Stream: Read + Write + 'ch>(client: &impl AtClient<'ch, Stream>, apn: &str) -> Result<(), AtError> {
    at_request!("AT+CGDCONT=1,\"IP\",\"{}\"", apn).send(client).await?;
    Ok(())
}
