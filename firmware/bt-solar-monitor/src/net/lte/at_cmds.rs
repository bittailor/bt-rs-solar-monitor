use defmt::Format;

const AT_RESPONSE_MAX_LEN: usize = 128;
type AtString = heapless::String<AT_RESPONSE_MAX_LEN>;

pub struct AtClient<'a, RW>
where
    RW: embedded_io_async::Read + embedded_io_async::Write,
{
    pub rw: &'a mut RW,
}

impl<'a, RW> AtClient<'a, RW>
where
    RW: embedded_io_async::Read + embedded_io_async::Write,
    AtError: From<<RW as embedded_io_async::ErrorType>::Error>,
    <RW as embedded_io_async::ErrorType>::Error: Format,
{
    pub fn new(rw: &'a mut RW) -> Self {
        Self { rw }
    }

    pub async fn send_line(&mut self, line: &str) -> Result<(), AtError> {
        self.rw.write_all(line.as_bytes()).await?;
        self.rw.write_all(b"\r").await?;
        self.rw.flush().await?;
        Ok(())
    }

    pub async fn send_command(&mut self, cmd: &str) -> Result<(), AtError> {
        self.request_response::<0>(cmd).await?;
        Ok(())
    }

    pub async fn send_request(&mut self, cmd: &str) -> Result<AtString, AtError> {
        let mut responses = self.request_response::<1>(cmd).await?;
        responses.pop().ok_or(AtError::NoResponseReceived)
    }

    pub async fn request_response<const N: usize>(
        &mut self,
        cmd: &str,
    ) -> Result<heapless::Vec<AtString, N>, AtError> {
        defmt::info!("UART TX>{}", cmd);
        self.rw.write_all(cmd.as_bytes()).await?;
        self.rw.write_all(b"\r").await?;
        self.rw.flush().await?;

        let mut responses = heapless::Vec::<AtString, N>::new();

        loop {
            match self.read_line().await? {
                line if line == "OK" => return Ok(responses),
                line if line.starts_with("CONNECT") => return Ok(responses),
                line if line == "ERROR" => return Err(AtError::CommandFailed),
                line => {
                    if line != cmd {
                        match responses.push(line) {
                            Ok(()) => {}
                            Err(l) => {
                                defmt::warn!("AT response buffer full => drop {}", l);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn read_line(&mut self) -> Result<AtString, AtError> {
        let mut line_buffer = heapless::Vec::<u8, AT_RESPONSE_MAX_LEN>::new();
        loop {
            let mut char_buf = [0u8; 1];
            match self.rw.read(&mut char_buf).await {
                Ok(_) => {
                    if char_buf[0] == b'\n' || char_buf[0] == b'\r' {
                        if line_buffer.len() > 0 {
                            break;
                        }
                    } else {
                        line_buffer.push(char_buf[0]).unwrap();
                    }
                }
                Err(e) => defmt::warn!("Read error: {}", e),
            };
        }
        match AtString::from_utf8(line_buffer) {
            Ok(line) => {
                defmt::debug!("UART RX>{}", line.as_str());
                Ok(AtString::from(line))
            }
            Err(_) => {
                defmt::error!("UART RX! invalid UTF-8 sequence");
                Err(AtError::InvalidUtf8Sequence)
            }
        }
    }
}

#[derive(Format)]
pub enum AtError {
    InvalidUtf8Sequence,
    CommandFailed,
    NoResponseReceived,
    UartReadError,
    RpUartError { source: embassy_rp::uart::Error },
}

impl From<embassy_rp::uart::Error> for AtError {
    fn from(error: embassy_rp::uart::Error) -> Self {
        AtError::RpUartError { source: error }
    }
}

//struct AtCommandHandler {}

pub async fn read_response<R: embedded_io_async::Read>(rx: &mut R) -> Result<AtString, AtError>
where
    <R as embedded_io_async::ErrorType>::Error: defmt::Format,
{
    let mut line_buffer = heapless::Vec::<u8, AT_RESPONSE_MAX_LEN>::new();
    loop {
        let mut char_buf = [0u8; 1];
        match rx.read(&mut char_buf).await {
            Ok(_) => {
                if char_buf[0] == b'\n' || char_buf[0] == b'\r' {
                    if line_buffer.len() > 0 {
                        break;
                    }
                } else {
                    line_buffer.push(char_buf[0]).unwrap();
                }
            }
            Err(e) => defmt::warn!("Read error: {}", e),
        };
    }
    match AtString::from_utf8(line_buffer) {
        Ok(line) => {
            defmt::debug!("UART RX>{}", line.as_str());
            Ok(AtString::from(line))
        }
        Err(_) => {
            defmt::error!("UART RX! invalid UTF-8 sequence");
            Err(AtError::InvalidUtf8Sequence)
        }
    }
}
