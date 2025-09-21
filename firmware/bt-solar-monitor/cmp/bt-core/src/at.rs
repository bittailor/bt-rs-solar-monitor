#![allow(async_fn_in_trait)]

pub mod http;
pub mod network;
pub mod packet_domain;
pub mod serial_interface;
pub mod status_control;

use core::mem::replace;

use embassy_futures::select::select;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{Read, Write};
use heapless::{CapacityError, String, Vec};

pub const ERROR_STRING_SIZE: usize = 64;
const CHANNEL_SIZE: usize = 2;
const AT_BUFFER_SIZE: usize = 256;
const MAX_RESPONSE_LINES: usize = 8;
pub const MAX_READ_BUFFER_SIZE: usize = AT_BUFFER_SIZE * MAX_RESPONSE_LINES;

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
pub struct AtCommandRequest {
    command: String<AT_BUFFER_SIZE>,
    timeout: Duration,
    urc_prefix: Option<String<AT_BUFFER_SIZE>>,
}

impl AtCommandRequest {
    fn new(command: String<AT_BUFFER_SIZE>) -> Self {
        AtCommandRequest {
            command,
            timeout: Duration::from_secs(5),
            urc_prefix: None,
        }
    }

    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn with_urc_prefix(mut self, urc_prefix: String<AT_BUFFER_SIZE>) -> Self {
        self.urc_prefix = Some(urc_prefix);
        self
    }

    async fn send(self, client: &impl AtClient) -> Result<AtCommandResponse, AtError> {
        client.send(AtRequestMessage::Command(self)).await;
        let response = match client.receive().await? {
            AtResponseMessage::Command(response) => response,
            AtResponseMessage::Read(_) => {
                error!("Unexpected 'Read' response instead of 'Command' response");
                return Err(AtError::Error);
            }
            AtResponseMessage::Write(_) => {
                error!("Unexpected 'Write' response instead of 'Command' response");
                return Err(AtError::Error);
            }
        };
        Ok(response)
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AtHttpReadRequest {
    offset: usize,
    len: usize,
}

impl AtHttpReadRequest {
    pub fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }

    pub async fn send(self, client: &impl AtClient) -> Result<AtHttpReadResponse, AtError> {
        client.send(AtRequestMessage::Read(self)).await;
        let response = match client.receive().await? {
            AtResponseMessage::Read(response) => response,
            AtResponseMessage::Command(_) => {
                error!("Unexpected 'Command' response instead of 'Read' response");
                return Err(AtError::Error);
            }
            AtResponseMessage::Write(_) => {
                error!("Unexpected 'Write' response instead of 'Read' response");
                return Err(AtError::Error);
            }
        };
        Ok(response)
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AtHttpWriteRequest {
    data: Vec<u8, MAX_READ_BUFFER_SIZE>,
}

impl AtHttpWriteRequest {
    pub fn new(data: &[u8]) -> Result<Self, AtError> {
        let mut vec = Vec::<u8, MAX_READ_BUFFER_SIZE>::new();
        vec.extend_from_slice(data)?;
        Ok(Self { data: vec })
    }

    pub async fn send(self, client: &impl AtClient) -> Result<(), AtError> {
        client.send(AtRequestMessage::Write(self)).await;
        match client.receive().await? {
            AtResponseMessage::Write(response) => response,
            AtResponseMessage::Read(_) => {
                error!("Unexpected 'Read' response instead of 'Write' response");
                return Err(AtError::Error);
            }
            AtResponseMessage::Command(_) => {
                error!("Unexpected 'Command' response instead of 'Write' response");
                return Err(AtError::Error);
            }
        };
        Ok(())
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum AtRequestMessage {
    Command(AtCommandRequest),
    Read(AtHttpReadRequest),
    Write(AtHttpWriteRequest),
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AtCommandResponse {
    lines: Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>,
}

impl AtCommandResponse {
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

impl Default for AtCommandResponse {
    fn default() -> Self {
        Self { lines: Vec::new() }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AtHttpReadResponse {
    data: Vec<u8, MAX_READ_BUFFER_SIZE>,
}

impl AtHttpReadResponse {
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, AtError> {
        if buf.len() < self.data.len() {
            return Err(AtError::CapacityError);
        }
        let len = core::cmp::min(buf.len(), self.data.len());
        buf[..len].copy_from_slice(&self.data[..len]);
        self.data.clear();
        Ok(len)
    }
}

impl Default for AtHttpReadResponse {
    fn default() -> Self {
        Self { data: Vec::new() }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AtHttpWriteResponse {}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Eq, PartialEq)]
pub enum AtResponseMessage {
    Command(AtCommandResponse),
    Read(AtHttpReadResponse),
    Write(AtHttpWriteResponse),
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
        $crate::at::AtCommandRequest::new(req_str)
    }};
}

pub async fn at(client: &impl AtClient) -> Result<(), AtError> {
    at_request!("AT").with_timeout(Duration::from_millis(200)).send(client).await?;
    Ok(())
}

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
                embassy_futures::select::Either::First(request) => match request {
                    AtRequestMessage::Command(cmd) => {
                        let response = self.handle_command(&cmd).await;
                        self.sender.send(response.map(AtResponseMessage::Command)).await;
                    }
                    AtRequestMessage::Read(read) => {
                        let response = self.handle_http_read(&read).await;
                        self.sender.send(response.map(AtResponseMessage::Read)).await;
                    }
                    AtRequestMessage::Write(write) => {
                        let response = self.handle_http_write(&write).await;
                        self.sender.send(response.map(AtResponseMessage::Write)).await;
                    }
                },
                embassy_futures::select::Either::Second(urc) => self.handle_urc(urc).await,
            };
        }
    }

    async fn handle_command(&mut self, cmd: &AtCommandRequest) -> Result<AtCommandResponse, AtError> {
        if let Err(_e) = self.at_controller.stream.write_all(cmd.command.as_bytes()).await {
            error!("Failed to send command: {}", cmd.command);
            return Err(AtError::Error);
        }
        if let Err(_e) = self.at_controller.stream.write_all(b"\r\n").await {
            error!("Failed to send command: {}", cmd.command);
            return Err(AtError::Error);
        }
        info!("UART.TX> {}", cmd.command);
        let mut response = AtCommandResponse::default();
        self.read_response_lines(cmd.command.as_str(), cmd.timeout, &mut response.lines).await?;

        if let Some(prefix) = &cmd.urc_prefix {
            self.read_line_until_urc(prefix.as_str(), cmd.timeout, &mut response.lines).await?;
        }
        debug!("'{}' => completed with {:?}", cmd.command, response);
        Ok(response)
    }

    async fn handle_http_read(&mut self, read: &AtHttpReadRequest) -> Result<AtHttpReadResponse, AtError> {
        let mut response = AtHttpReadResponse::default();
        response.data.resize(read.len, 0)?;
        self.http_read(read, &mut response.data).await?;
        Ok(response)
    }

    async fn handle_http_write(&mut self, write: &AtHttpWriteRequest) -> Result<AtHttpWriteResponse, AtError> {
        self.http_write(&write.data).await?;
        Ok(AtHttpWriteResponse {})
    }

    async fn read_response_lines(
        &mut self,
        command: &str,
        timeout: Duration,
        lines: &mut Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>,
    ) -> Result<(), AtError> {
        match with_timeout(timeout, async {
            loop {
                let line = self.at_controller.read_line().await?;
                if line == "OK" {
                    debug!("OK => success => {} response lines", lines.len());
                    break Ok(());
                } else if line == "DOWNLOAD" {
                    debug!("DOWNLOAD => success => {} response lines", lines.len());
                    break Ok(());
                } else if line == "ERROR" {
                    warn!("ERROR => error => {} response lines", lines.len());
                    break Err(AtError::Error);
                } else {
                    if line == command {
                        trace!("Skipping echo line");
                        continue;
                    }
                    debug!(" R[{}] {}", lines.len(), line.as_str());
                    lines.push(line).map_err(|_| AtError::CapacityError)?;
                }
            }
        })
        .await
        {
            Ok(Ok(l)) => {
                debug!("'{}' => completed", command);
                Ok(l)
            }
            Ok(Err(e)) => {
                error!("'{}' => error", command);
                Err(e)
            }
            Err(_e) => {
                error!("'{}' => timeout", command);
                Err(AtError::Timeout)
            }
        }
    }

    async fn read_line_until_urc(
        &mut self,
        prefix: &str,
        timeout: Duration,
        lines: &mut Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>,
    ) -> Result<(), AtError> {
        match with_timeout(timeout, async {
            loop {
                let line = self.at_controller.read_line().await?;
                let prefix_match = line.starts_with(prefix);
                lines.push(line).map_err(|_| AtError::CapacityError)?;
                if prefix_match {
                    debug!("Found URC prefix '{}'", prefix);
                    break Ok(());
                }
            }
        })
        .await
        {
            Ok(Ok(l)) => {
                debug!("urc '{}' => completed", prefix);
                Ok(l)
            }
            Ok(Err(e)) => {
                error!("urc '{}' => error", prefix);
                Err(e)
            }
            Err(_e) => {
                error!("urc '{}' => timeout", prefix);
                Err(AtError::Timeout)
            }
        }

        /*

        match response {
            Ok(mut response_msg) => loop {
                let line = self.at_controller.read_line().await;
                if line.is_empty() {
                    warn!("Empty URC line");
                    continue;
                }
                let found = line.starts_with(prefix.as_str());

                match response_msg.lines.push(line) {
                    Ok(_) => {}
                    Err(_) => {
                        return Err(AtError::Error);
                    }
                }
                if found {
                    return Ok(response_msg);
                }
            },
            Err(_) => response,
        }
        */
    }

    async fn http_read(&mut self, read: &AtHttpReadRequest, buf: &mut [u8]) -> Result<usize, AtError> {
        let cmd = heapless::format!(AT_BUFFER_SIZE; "AT+HTTPREAD={},{}", &read.offset, &read.len)?;
        self.at_controller.stream.write_all(cmd.as_bytes()).await.map_err(|_| AtError::Error)?;
        self.at_controller.stream.write_all(b"\r\n").await.map_err(|_| AtError::Error)?;

        let mut lines = heapless::Vec::new();
        self.read_response_lines(cmd.as_str(), Duration::from_secs(10), &mut lines).await?;
        lines.clear();
        let start_tag = heapless::format!(AT_BUFFER_SIZE; "+HTTPREAD: {}", &read.len)?;
        self.read_line_until_urc(start_tag.as_str(), Duration::from_secs(120), &mut lines).await?;
        self.at_controller.stream.read_exact(&mut buf[0..read.len]).await.map_err(|_| AtError::Error)?;
        self.read_line_until_urc("+HTTPREAD: 0", Duration::from_secs(120), &mut lines).await?;
        Ok(read.len)
    }

    async fn http_write(&mut self, buf: &[u8]) -> Result<usize, AtError> {
        let cmd = heapless::format!(AT_BUFFER_SIZE; "AT+HTTPDATA={},{}", &buf.len(), 60)?;
        self.at_controller.stream.write_all(cmd.as_bytes()).await.map_err(|_| AtError::Error)?;
        self.at_controller.stream.write_all(b"\r\n").await.map_err(|_| AtError::Error)?;

        let mut lines = heapless::Vec::new();
        self.read_response_lines(cmd.as_str(), Duration::from_secs(10), &mut lines).await?;
        lines.clear();
        self.at_controller.stream.write_all(buf).await.map_err(|_| AtError::Error)?;
        self.read_response_lines("", Duration::from_secs(10), &mut lines).await?;
        Ok(buf.len())
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
        loop {
            match self.read_line().await {
                Ok(urc_line) => {
                    debug!("URC.RX> {}", urc_line.as_str());
                    return urc_line;
                }
                Err(_) => {
                    warn!("read error while urc polling => ignore");
                }
            }
        }
    }

    async fn read_line(&mut self) -> Result<String<AT_BUFFER_SIZE>, AtError> {
        let mut have_cr = false;
        loop {
            let mut char_buf = [0u8; 1];
            match self.stream.read(&mut char_buf).await {
                Ok(_) => {
                    if char_buf[0] == b'\r' {
                        have_cr = true;
                        continue;
                    }
                    if char_buf[0] == b'\n' {
                        if !have_cr {
                            warn!("Line feed without preceding carriage return");
                        }
                        have_cr = false;
                        trace!("UART.RX line of lenght {}", self.line_buffer.len());
                        if !self.line_buffer.is_empty() {
                            match String::from_utf8(replace(&mut self.line_buffer, heapless::Vec::new())) {
                                Ok(line) => {
                                    debug!("UART.RX> {}", line.as_str());
                                    return Ok(line);
                                }
                                Err(_) => error!("Invalid UTF-8 sequence"),
                            }
                            self.line_buffer.clear();
                        }
                    } else {
                        self.line_buffer.push(char_buf[0]).map_err(|_| AtError::CapacityError)?;
                    }
                }
                Err(_e) => warn!("Read error"),
            };
        }
    }
}

#[cfg(test)]
pub mod mocks {
    use super::*;
    use crate::at::{AT_BUFFER_SIZE, AtCommandResponse, AtError, MAX_RESPONSE_LINES};
    use core::cell::RefCell;

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
            AtRequestMessage::Command(AtCommandRequest::new(heapless::String::<AT_BUFFER_SIZE>::try_from(command).unwrap())),
            AtResponseMessage::Command(AtCommandResponse::new(lines)),
        )
    }
}
