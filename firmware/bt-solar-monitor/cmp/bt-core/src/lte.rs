pub mod at;

use core::{
    mem::replace,
    str::{self, FromStr},
};

use embassy_futures::select::select;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{Read, Write};
use heapless::{String, Vec, format};

use crate::lte::at::parse_network_registration_response;

const CHANNEL_SIZE: usize = 2;
const AT_BUFFER_SIZE: usize = 256;
const MAX_RESPONSE_LINES: usize = 8;

pub struct State {
    tx_channel: Channel<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    rx_channel: Channel<NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>,
}

impl State {
    pub fn new() -> Self {
        Self {
            tx_channel: Channel::<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>::new(),
            rx_channel: Channel::<NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>::new(),
        }
    }
}

pub async fn new_lte<'a, S: Read + Write>(state: &'a mut State, stream: S) -> (Lte<'a>, Runner<'a, S>) {
    let runner = Runner::new(stream, state.tx_channel.receiver(), state.rx_channel.sender());
    let lte = Lte {
        tx: state.tx_channel.sender(),
        rx: state.rx_channel.receiver(),
    };

    (lte, runner)
}

pub enum LteError {
    Timeout,
    Error,
}

impl From<AtError> for LteError {
    fn from(err: AtError) -> Self {
        match err {
            AtError::Timeout => LteError::Timeout,
            AtError::Error => LteError::Error,
        }
    }
}

impl From<core::fmt::Error> for LteError {
    fn from(_err: core::fmt::Error) -> Self {
        LteError::Error
    }
}

pub struct Lte<'ch> {
    tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    rx: Receiver<'ch, NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>,
}

impl Lte<'_> {
    pub async fn at(&self) -> Result<(), LteError> {
        self.tx.send(cmd("AT").with_timeout(Duration::from_millis(200))).await;
        _ = self.rx.receive().await?;
        Ok(())
        /*
        match self.rx.receive().await {
            AtResponseMessage::Timeout => Err(LteError::Timeout),
            AtResponseMessage::Error => Err(LteError::Error),
            AtResponseMessage::Ok(_) => Ok(()),
        }
        */
    }

    pub async fn set_apn(&self, apn: &str) -> Result<(), LteError> {
        self.tx.send(fmt(format!("AT+CGDCONT=1,\"IP\",\"{}\"", apn)?)).await;
        _ = self.rx.receive().await?;
        Ok(())
    }

    pub async fn read_network_registration(&self) -> Result<(at::NetworkRegistrationUrcConfig, at::NetworkRegistrationState), LteError> {
        self.tx.send(cmd("AT+CREG?")).await;
        let mut response = self.rx.receive().await?;
        let (n, stat) = parse_network_registration_response(response.pop().ok_or(AtError::Error)?.as_str())?;
        Ok((n, stat))
    }
}

pub struct AtRequestMessage {
    command: String<AT_BUFFER_SIZE>,
    timeout: Duration,
}

impl AtRequestMessage {
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn cmd(command: &str) -> AtRequestMessage {
    match String::from_str(command) {
        Ok(command) => AtRequestMessage {
            command,
            timeout: Duration::from_secs(1),
        },
        Err(_) => {
            error!("Command too long: {}", command);
            AtRequestMessage {
                command: String::new(),
                timeout: Duration::from_secs(5),
            }
        }
    }
}

fn fmt(command: String<AT_BUFFER_SIZE>) -> AtRequestMessage {
    AtRequestMessage {
        command,
        timeout: Duration::from_secs(1),
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum AtError {
    Timeout,
    Error,
}

type AtResponseMessage = Result<Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>, AtError>;

pub struct Runner<'ch, S: Read + Write> {
    receiver: Receiver<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    sender: Sender<'ch, NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>,
    at_controller: AtController<S>,
}

impl<'ch, S: Read + Write> Runner<'ch, S> {
    pub fn new(
        stream: S,
        receiver: Receiver<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
        sender: Sender<'ch, NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>,
    ) -> Self {
        Self {
            receiver,
            sender,
            at_controller: AtController::new(stream),
        }
    }

    pub async fn run(mut self) {
        loop {
            //let receiver = self.channel.receiver();
            match select(self.receiver.receive(), self.at_controller.poll_urc()).await {
                embassy_futures::select::Either::First(cmd) => {
                    let response = self.send_command(cmd).await;
                    self.sender.send(response).await;
                }
                embassy_futures::select::Either::Second(urc) => self.handle_urc(urc).await,
            };
        }
    }

    async fn send_command(&mut self, command: AtRequestMessage) -> AtResponseMessage {
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
                    if lines.len() == 0 && line == command.command {
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
                response
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
                        if self.line_buffer.len() > 0 {
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
