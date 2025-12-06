#![allow(async_fn_in_trait)]

pub mod http;
pub mod network;
pub mod packet_domain;
pub mod serial_interface;
pub mod status_control;

use core::mem::{MaybeUninit, replace};

use embassy_futures::select::select;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
    mutex::Mutex,
};
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{Read, Write};
use heapless::{CapacityError, String, Vec};

use crate::LoggingMutexGuard;

pub const ERROR_STRING_SIZE: usize = 64;
const CHANNEL_SIZE: usize = 2;
const AT_BUFFER_SIZE: usize = 256;
const MAX_RESPONSE_LINES: usize = 4;
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
        #[cfg(feature = "log")]
        debug!("Parsing error {:?} => AtError", _err);
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

    async fn send<'ch, Ctr: AtController>(self, client: &impl AtClient<'ch, Ctr>) -> Result<AtCommandResponse, AtError> {
        debug!("AT.Req> {:?}", self);
        let response = client.use_controller(async |ctr| ctr.handle_command(&self).await).await;
        debug!("AT.Rsp> {:?}", response);
        response
    }
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
enum AtRequestMessage {
    AcquireAtController,
    ReleaseAtController,
}

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum AtResponseMessage {
    Ok,
}

pub struct State<Stream: Read + Write> {
    tx_channel: Channel<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    rx_channel: Channel<NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    at_controller: MaybeUninit<Mutex<NoopRawMutex, AtControllerImpl<Stream>>>,
}

impl<Stream: Read + Write> State<Stream> {
    pub fn new() -> Self {
        Self {
            tx_channel: Channel::<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>::new(),
            rx_channel: Channel::<NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>::new(),
            at_controller: MaybeUninit::uninit(),
        }
    }
}

pub fn new<'a, Stream: Read + Write>(
    state: &'a mut State<Stream>,
    stream: Stream,
) -> (crate::at::Runner<'a, AtControllerImpl<Stream>>, AtClientImpl<'a, AtControllerImpl<Stream>>) {
    let at_client = Mutex::new(crate::at::AtControllerImpl::new(stream));
    state.at_controller.write(at_client);
    let ctr: &Mutex<NoopRawMutex, AtControllerImpl<Stream>> = unsafe { &*state.at_controller.as_ptr() };
    let handle = AtControllerHandle { inner: ctr };
    let runner = crate::at::Runner::new(handle, state.tx_channel.receiver(), state.rx_channel.sender());
    let client = AtClientImpl::new(state.tx_channel.sender(), state.rx_channel.receiver(), handle);
    (runner, client)
}

impl<Stream: Read + Write> Default for State<Stream> {
    fn default() -> Self {
        Self::new()
    }
}

#[macro_export]
macro_rules! at_request {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        let req_str = heapless::format!($s $(, $x)*)?;
        $crate::at::AtCommandRequest::new(req_str)
    }};
}

pub async fn at<'ch, Ctr: AtController>(client: &impl AtClient<'ch, Ctr>) -> Result<(), AtError> {
    at_request!("AT").with_timeout(Duration::from_millis(200)).send(client).await?;
    Ok(())
}

pub struct Runner<'ch, Ctr: AtController> {
    receiver: Receiver<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    sender: Sender<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    at_controller: AtControllerHandle<'ch, Ctr>,
}

impl<'ch, Ctr: AtController> Runner<'ch, Ctr> {
    fn new(
        at_controller: AtControllerHandle<'ch, Ctr>,
        receiver: Receiver<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
        sender: Sender<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    ) -> Self {
        Self {
            receiver,
            sender,
            at_controller,
        }
    }

    pub async fn run(mut self) {
        #[derive(Debug, Eq, PartialEq)]
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        enum State {
            UrcPoll,
            AtControllerAcquired,
        }

        let mut state = State::UrcPoll;
        loop {
            trace!("AT runner loop: enter {:?}", state);
            match state {
                State::UrcPoll => {
                    let next = {
                        let mut ctr = self.at_controller.inner("urc_poll").await;
                        select(self.receiver.receive(), ctr.poll_urc()).await
                    };
                    trace!("AT runner loop: handle {:?}", next);
                    match next {
                        embassy_futures::select::Either::First(request) => match request {
                            AtRequestMessage::AcquireAtController => {
                                state = State::AtControllerAcquired;
                                self.sender.send(Ok(AtResponseMessage::Ok)).await;
                            }
                            AtRequestMessage::ReleaseAtController => {
                                warn!("ReleaseAtController while not acquired");
                                self.sender.send(Ok(AtResponseMessage::Ok)).await;
                            }
                        },
                        embassy_futures::select::Either::Second(urc) => self.handle_urc(urc).await,
                    };
                }
                State::AtControllerAcquired => {
                    let next = self.receiver.receive().await;
                    trace!("AT runner loop: handle {:?}", next);
                    match next {
                        AtRequestMessage::AcquireAtController => {
                            warn!("AcquireAtController while already acquired");
                            self.sender.send(Ok(AtResponseMessage::Ok)).await;
                        }
                        AtRequestMessage::ReleaseAtController => {
                            state = State::UrcPoll;
                            self.sender.send(Ok(AtResponseMessage::Ok)).await;
                        }
                    };
                }
            }
            trace!("AT runner loop: exit");
        }
    }

    async fn handle_urc(&mut self, urc: String<AT_BUFFER_SIZE>) {
        info!("Handling URC: {}", urc.as_str());
    }
}

pub trait AtClient<'ch, Ctr: AtController> {
    async fn use_controller<'a, F, R>(&'a self, f: F) -> R
    where
        F: AsyncFnMut(&mut Ctr) -> R + 'a,
        Ctr: 'a;
}

pub struct AtClientImpl<'ch, Ctr: AtController> {
    tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    rx: Receiver<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
    at_controller: AtControllerHandle<'ch, Ctr>,
}

impl<'ch, Ctr: AtController> AtClientImpl<'ch, Ctr> {
    fn new(
        tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
        rx: Receiver<'ch, NoopRawMutex, Result<AtResponseMessage, AtError>, CHANNEL_SIZE>,
        at_controller: AtControllerHandle<'ch, Ctr>,
    ) -> Self {
        Self { tx, rx, at_controller }
    }
}

impl<'ch, Ctr: AtController> AtClient<'ch, Ctr> for AtClientImpl<'ch, Ctr> {
    async fn use_controller<'a, F, R>(&'a self, mut f: F) -> R
    where
        F: AsyncFnMut(&mut Ctr) -> R + 'a,
        Ctr: 'a,
    {
        self.tx.send(AtRequestMessage::AcquireAtController).await;
        let _ = self.rx.receive().await;
        let mut ctr = self.at_controller.inner("at_rx").await;
        let response = f(&mut ctr).await;
        drop(ctr);
        self.tx.send(AtRequestMessage::ReleaseAtController).await;
        let _ = self.rx.receive().await;
        response
    }
}

pub struct AtControllerHandle<'ch, Ctr: AtController> {
    inner: &'ch Mutex<NoopRawMutex, Ctr>,
}
impl<'ch, Ctr: AtController> Copy for AtControllerHandle<'ch, Ctr> {}
impl<'ch, Ctr: AtController> Clone for AtControllerHandle<'ch, Ctr> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'ch, Ctr: AtController> AtControllerHandle<'ch, Ctr> {
    async fn inner(&self, tag: &'static str) -> LoggingMutexGuard<'_, NoopRawMutex, Ctr> {
        LoggingMutexGuard::new(self.inner, tag).await
    }
}

pub trait AtController {
    async fn handle_command(&mut self, cmd: &AtCommandRequest) -> Result<AtCommandResponse, AtError>;
    async fn handle_http_read(&mut self, buf: &mut [u8], offset: usize) -> Result<(), AtError>;
    async fn handle_http_write(&mut self, buf: &[u8]) -> Result<(), AtError>;
    async fn poll_urc(&mut self) -> String<AT_BUFFER_SIZE>;
}

pub struct AtControllerImpl<S: Read + Write> {
    stream: S,
    line_buffer: heapless::Vec<u8, AT_BUFFER_SIZE>,
}

impl<S: Read + Write> AtController for AtControllerImpl<S> {
    async fn handle_command(&mut self, cmd: &AtCommandRequest) -> Result<AtCommandResponse, AtError> {
        if let Err(_e) = self.stream.write_all(cmd.command.as_bytes()).await {
            error!("Failed to send command: {}", cmd.command);
            return Err(AtError::Error);
        }
        if let Err(_e) = self.stream.write_all(b"\r\n").await {
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

    async fn handle_http_read(&mut self, buf: &mut [u8], offset: usize) -> Result<(), AtError> {
        self.http_read(buf, offset).await?;
        Ok(())
    }

    async fn handle_http_write(&mut self, buf: &[u8]) -> Result<(), AtError> {
        self.http_write(buf).await?;
        Ok(())
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
}

impl<S: Read + Write> AtControllerImpl<S> {
    pub fn new(stream: S) -> Self {
        Self {
            stream,
            line_buffer: heapless::Vec::new(),
        }
    }

    async fn http_read(&mut self, buf: &mut [u8], offset: usize) -> Result<usize, AtError> {
        let cmd = heapless::format!(AT_BUFFER_SIZE; "AT+HTTPREAD={},{}", offset, buf.len())?;
        self.stream.write_all(cmd.as_bytes()).await.map_err(|_| AtError::Error)?;
        self.stream.write_all(b"\r\n").await.map_err(|_| AtError::Error)?;

        let mut lines = heapless::Vec::new();
        self.read_response_lines(cmd.as_str(), Duration::from_secs(10), &mut lines).await?;
        lines.clear();
        let start_tag = heapless::format!(AT_BUFFER_SIZE; "+HTTPREAD: {}", buf.len())?;
        self.read_line_until_urc(start_tag.as_str(), Duration::from_secs(120), &mut lines).await?;
        self.stream.read_exact(buf).await.map_err(|_| AtError::Error)?;
        self.read_line_until_urc("+HTTPREAD: 0", Duration::from_secs(120), &mut lines).await?;
        Ok(buf.len())
    }

    async fn http_write(&mut self, buf: &[u8]) -> Result<usize, AtError> {
        let cmd = heapless::format!(AT_BUFFER_SIZE; "AT+HTTPDATA={},{}", &buf.len(), 60)?;
        self.stream.write_all(cmd.as_bytes()).await.map_err(|_| AtError::Error)?;
        self.stream.write_all(b"\r\n").await.map_err(|_| AtError::Error)?;

        let mut lines = heapless::Vec::new();
        self.read_response_lines(cmd.as_str(), Duration::from_secs(10), &mut lines).await?;
        lines.clear();
        self.stream.write_all(buf).await.map_err(|_| AtError::Error)?;
        self.read_response_lines("", Duration::from_secs(10), &mut lines).await?;
        Ok(buf.len())
    }

    async fn read_response_lines(
        &mut self,
        command: &str,
        timeout: Duration,
        lines: &mut Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>,
    ) -> Result<(), AtError> {
        match with_timeout(timeout, async {
            loop {
                let line = self.read_line().await?;
                if line == "OK" || line == "DOWNLOAD" {
                    debug!("{} => success => {} response lines", line, lines.len());
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
                let line = self.read_line().await?;
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
    use core::any::Any;
    use std::boxed::Box;

    pub struct AtControllerMock {
        request: Box<dyn Any>,
        response: Option<Box<dyn Any>>,
    }

    impl AtController for AtControllerMock {
        async fn handle_command(&mut self, cmd: &AtCommandRequest) -> Result<AtCommandResponse, AtError> {
            let request = self.request.downcast_ref::<AtCommandRequest>().unwrap();
            assert_eq!(cmd, request);

            let response = self.response.take().unwrap().downcast::<AtCommandResponse>().map(|r| *r).unwrap();
            Ok(response)
        }
        async fn handle_http_read(&mut self, _buf: &mut [u8], _offset: usize) -> Result<(), AtError> {
            Err(AtError::Error)
        }
        async fn handle_http_write(&mut self, _buf: &[u8]) -> Result<(), AtError> {
            Err(AtError::Error)
        }
        async fn poll_urc(&mut self) -> String<AT_BUFFER_SIZE> {
            String::new()
        }
    }

    pub struct AtClientMock {
        controller: tokio::sync::Mutex<AtControllerMock>,
    }

    impl AtClientMock {
        pub fn new(request: Box<dyn Any>, response: Box<dyn Any>) -> Self {
            Self {
                controller: tokio::sync::Mutex::new(AtControllerMock {
                    request,
                    response: Some(response),
                }),
            }
        }
    }

    impl<'ch> AtClient<'ch, AtControllerMock> for AtClientMock {
        async fn use_controller<'a, F, R>(&'a self, mut f: F) -> R
        where
            F: AsyncFnMut(&mut AtControllerMock) -> R + 'a,
            AtControllerMock: 'a,
        {
            let mut ctr = self.controller.lock().await;
            f(&mut ctr).await
        }
    }

    pub fn mock_request(command: &str, response_lines: &[&str]) -> AtClientMock {
        let mut lines = heapless::Vec::<heapless::String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>::new();
        for line in response_lines {
            lines.push(heapless::String::<AT_BUFFER_SIZE>::try_from(*line).unwrap()).unwrap();
        }

        AtClientMock::new(Box::new(AtCommandRequest::new(command.try_into().unwrap())), Box::new(AtCommandResponse::new(lines)))
    }
}
