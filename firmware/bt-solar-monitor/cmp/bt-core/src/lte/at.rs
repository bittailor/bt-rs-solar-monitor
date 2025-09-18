#![allow(async_fn_in_trait)]

pub mod network;
pub mod packet_domain;
pub mod serial_interface;

use core::mem::replace;

use embassy_futures::select::select;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{Read, Write};
use heapless::{CapacityError, String, Vec};
use nom::{IResult, bytes::complete::tag};

pub const ERROR_STRING_SIZE: usize = 64;
const CHANNEL_SIZE: usize = 2;
const AT_BUFFER_SIZE: usize = 256;
const MAX_RESPONSE_LINES: usize = 8;

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AtError {
    Timeout,
    FormatError,
    CapacityError,
    EnumParseError(String<ERROR_STRING_SIZE>),
    ResponseLineCountMismatch { expected: usize, actual: usize },
    Error,
}

impl From<core::fmt::Error> for AtError {
    fn from(_: core::fmt::Error) -> Self {
        AtError::FormatError
    }
}

impl From<nom::Err<nom::error::Error<&str>>> for AtError {
    fn from(_err: nom::Err<nom::error::Error<&str>>) -> Self {
        AtError::Error
    }
}

impl From<CapacityError> for AtError {
    fn from(_err: CapacityError) -> Self {
        AtError::CapacityError
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AtRequestMessage {
    command: String<AT_BUFFER_SIZE>,
    timeout: Duration,
}

impl AtRequestMessage {
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn send(self, client: &impl AtClient) -> Result<AtResponseMessage, AtError> {
        client.send(self).await;
        client.receive().await
    }
}

//type AtResponseMessage = Result<Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>, AtError>;
pub struct AtResponseMessage {
    lines: Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>,
}

impl AtResponseMessage {
    pub fn new(lines: Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>) -> Self {
        Self { lines }
    }

    pub fn ensure_lines(&self, n: usize) -> Result<(), AtError> {
        if self.lines.len() == n {
            Ok(())
        } else {
            Err(AtError::ResponseLineCountMismatch {
                expected: n,
                actual: self.lines.len(),
            })
        }
    }

    pub fn line(&self, n: usize) -> Result<&str, AtError> {
        self.lines.get(n).map(|s| s.as_str()).ok_or(AtError::ResponseLineCountMismatch {
            expected: n + 1,
            actual: self.lines.len(),
        })
    }
}

pub struct State {
    pub(super) tx_channel: Channel<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    pub(super) rx_channel: Channel<NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
}

impl State {
    pub fn new() -> Self {
        Self {
            tx_channel: Channel::<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>::new(),
            rx_channel: Channel::<NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>::new(),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

pub trait AtClient {
    async fn send(&self, request: AtRequestMessage);
    async fn receive(&self) -> Result<AtResponseMessage, AtError>;
}

pub struct AtClientImpl<'ch> {
    tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    rx: Receiver<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
}

impl<'ch> AtClientImpl<'ch> {
    pub fn new(
        tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
        rx: Receiver<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    ) -> Self {
        Self { tx, rx }
    }
}

impl<'ch> AtClient for AtClientImpl<'ch> {
    async fn send(&self, request: AtRequestMessage) {
        self.tx.send(request).await;
    }

    async fn receive(&self) -> Result<AtResponseMessage, AtError> {
        self.rx.receive().await
    }
}

#[macro_export]
macro_rules! at_request {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        let req_str = heapless::format!($s $(, $x)*)?;
        $crate::lte::at::AtRequestMessage { command: req_str, timeout: embassy_time::Duration::from_secs(5) }
    }};
}

pub async fn at(client: &impl AtClient) -> Result<(), AtError> {
    at_request!("AT").with_timeout(Duration::from_millis(200)).send(client).await?;
    Ok(())
}

// parsers tokens

fn sperator(input: &str) -> IResult<&str, ()> {
    let (input, _) = tag(",")(input)?;
    Ok((input, ()))
}

fn number(input: &str) -> IResult<&str, u32> {
    nom::character::complete::u32(input)
}

//

pub struct Runner<'ch, S: Read + Write> {
    receiver: Receiver<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    sender: Sender<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    at_controller: AtController<S>,
}

impl<'ch, S: Read + Write> Runner<'ch, S> {
    pub fn new(
        stream: S,
        receiver: Receiver<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
        sender: Sender<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    ) -> Self {
        Self {
            receiver,
            sender,
            at_controller: AtController::new(stream),
        }
    }

    pub async fn run(mut self) {
        loop {
            match select(self.receiver.receive(), self.at_controller.poll_urc()).await {
                embassy_futures::select::Either::First(cmd) => {
                    let response = self.send_command(cmd).await;
                    self.sender.send(response).await;
                }
                embassy_futures::select::Either::Second(urc) => self.handle_urc(urc).await,
            };
        }
    }

    async fn send_command(&mut self, command: AtRequestMessage) -> Result<AtResponseMessage, AtError> {
        if let Err(_e) = self.at_controller.stream.write_all(command.command.as_bytes()).await {
            error!("Failed to send command: {}", command.command);
            return Err(AtError::Error);
        }
        if let Err(_e) = self.at_controller.stream.write_all(b"\r\n").await {
            error!("Failed to send command: {}", command.command);
            return Err(AtError::Error);
        }
        info!("UART.TX> {}", command.command);

        let mut lines = Vec::<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>::new();

        match with_timeout(command.timeout, async {
            loop {
                let line = self.at_controller.read_line().await;
                if line == "OK" {
                    info!("Command success => {} response lines", lines.len());
                    break Ok(lines);
                } else if line == "ERROR" {
                    warn!("Command error => {} response lines", lines.len());
                    break Err(AtError::Error);
                } else {
                    if lines.is_empty() && line == command.command {
                        trace!("Skipping echo line");
                        continue; // skip echo line
                    }
                    info!(" R<{}> {}", lines.len(), line.as_str());
                    match lines.push(line) {
                        Ok(_) => {}
                        Err(_) => {
                            error!("Response buffer full");
                            break Err(AtError::Error);
                        }
                    }
                }
            }
        })
        .await
        {
            Ok(response) => {
                info!("Command '{}' => completed", command.command);
                response.map(AtResponseMessage::new)
            }
            Err(_e) => {
                error!("Command '{}' => timeout", command.command);
                Err(AtError::Timeout)
            }
        }
    }

    async fn handle_urc(&mut self, urc: String<AT_BUFFER_SIZE>) {
        info!("Handling URC: {}", urc.as_str());
    }
}

struct AtController<S: Read + Write> {
    stream: S,
    line_buffer: heapless::Vec<u8, AT_BUFFER_SIZE>,
}

impl<S: Read + Write> AtController<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            line_buffer: heapless::Vec::new(),
        }
    }

    async fn poll_urc(&mut self) -> String<AT_BUFFER_SIZE> {
        let urc_line = self.read_line().await;
        debug!("URC.RX> {}", urc_line.as_str());
        urc_line
    }

    async fn read_line(&mut self) -> String<AT_BUFFER_SIZE> {
        loop {
            let mut char_buf = [0u8; 1];
            match self.stream.read(&mut char_buf).await {
                Ok(_) => {
                    if char_buf[0] == b'\n' || char_buf[0] == b'\r' {
                        trace!("UART.RX line of lenght {}", self.line_buffer.len());
                        if !self.line_buffer.is_empty() {
                            match String::from_utf8(replace(&mut self.line_buffer, heapless::Vec::new())) {
                                Ok(line) => {
                                    debug!("UART.RX> {}", line.as_str());
                                    return line;
                                }
                                Err(_) => error!("Invalid UTF-8 sequence"),
                            }
                            self.line_buffer.clear();
                        }
                    } else {
                        self.line_buffer.push(char_buf[0]).unwrap();
                    }
                }
                Err(_e) => warn!("Read error"),
            };
        }
    }
}

#[cfg(test)]
pub mod mocks {
    use crate::lte::at::{AT_BUFFER_SIZE, AtError, MAX_RESPONSE_LINES};
    use core::cell::RefCell;

    use super::{AtClient, AtRequestMessage, AtResponseMessage};

    pub struct AtClientMock {
        request: AtRequestMessage,
        response: RefCell<Option<AtResponseMessage>>,
    }

    impl AtClientMock {
        pub fn new(request: AtRequestMessage, response: AtResponseMessage) -> Self {
            Self {
                request,
                response: RefCell::new(Some(response)),
            }
        }
    }

    impl AtClient for AtClientMock {
        async fn send(&self, request: AtRequestMessage) {
            assert_eq!(self.request, request);
        }

        async fn receive(&self) -> Result<AtResponseMessage, AtError> {
            Ok(self.response.take().unwrap())
        }
    }

    pub fn mock_request(command: &str, response_lines: &[&str]) -> AtClientMock {
        let mut lines = heapless::Vec::<heapless::String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>::new();
        for line in response_lines {
            lines.push(heapless::String::<AT_BUFFER_SIZE>::try_from(*line).unwrap()).unwrap();
        }

        AtClientMock::new(
            AtRequestMessage {
                command: heapless::String::try_from(command).unwrap(),
                timeout: embassy_time::Duration::from_secs(5),
            },
            AtResponseMessage::new(lines),
        )
    }
}
