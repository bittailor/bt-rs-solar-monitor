use core::mem::replace;

use embassy_futures::select::select;
use embassy_sync::{
    blocking_mutex::raw::NoopRawMutex,
    channel::{Channel, Receiver, Sender},
};
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{Read, Write};
use heapless::{String, Vec, format};
use nom::{IResult, Parser, bytes::complete::tag};

pub const ERROR_STRING_SIZE: usize = 64;
const CHANNEL_SIZE: usize = 2;
const AT_BUFFER_SIZE: usize = 256;
const MAX_RESPONSE_LINES: usize = 8;

#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AtError {
    Timeout,
    FormatError,
    EnumParseError(String<ERROR_STRING_SIZE>),
    MissingResponseLine,
    ResponseLineCountMismatch { expected: usize, actual: usize },
    Error,
}

impl From<core::fmt::Error> for AtError {
    fn from(_: core::fmt::Error) -> Self {
        AtError::FormatError
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NetworkRegistrationUrcConfig {
    /// 0 disable network registration unsolicited result code.
    UrcDisabled = 0,
    /// 1 enable network registration unsolicited result code +CREG: <stat>.
    UrcEnabled = 1,
    /// enable network registration and location information unsolicited result code +CREG: <stat>[,<lac>,<ci>].
    UrcVerbose = 2,
}

impl TryFrom<u32> for NetworkRegistrationUrcConfig {
    type Error = AtError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NetworkRegistrationUrcConfig::UrcDisabled),
            1 => Ok(NetworkRegistrationUrcConfig::UrcEnabled),
            2 => Ok(NetworkRegistrationUrcConfig::UrcVerbose),
            _ => Err(AtError::EnumParseError(format!("Invalid NetworkRegistrationUrcConfig value: {}", value).unwrap_or_default())),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NetworkRegistrationState {
    /// 0 not registered, ME is not currently searching a new operator to register to.
    NotRegistered = 0,
    /// 1 registered, home network.
    Registered = 1,
    /// 2 not registered, but ME is currently searching a new operator to register to.
    NotRegisteredSearching = 2,
    /// 3 registration denied.
    RegistrationDenied = 3,
    /// 4 unknown.
    Unknown = 4,
    /// 5 registered, roaming.
    RegisteredRoaming = 5,
    /// 6 registered for "SMS only", home network (applicable only whenE-UTRAN)
    RegisteredSmsOnly = 6,
}

impl TryFrom<u32> for NetworkRegistrationState {
    type Error = AtError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(NetworkRegistrationState::NotRegistered),
            1 => Ok(NetworkRegistrationState::Registered),
            2 => Ok(NetworkRegistrationState::NotRegisteredSearching),
            3 => Ok(NetworkRegistrationState::RegistrationDenied),
            4 => Ok(NetworkRegistrationState::Unknown),
            5 => Ok(NetworkRegistrationState::RegisteredRoaming),
            6 => Ok(NetworkRegistrationState::RegisteredSmsOnly),
            11 => Ok(NetworkRegistrationState::NotRegisteredSearching), // ???
            _ => Err(AtError::EnumParseError(format!("Invalid NetworkRegistrationState value: {}", value).unwrap_or_default())),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SleepMode {
    Off = 0,
    DtrSleep = 1,
    RxSleep = 2,
}

impl TryFrom<u32> for SleepMode {
    type Error = AtError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SleepMode::Off),
            1 => Ok(SleepMode::DtrSleep),
            2 => Ok(SleepMode::RxSleep),
            _ => Err(AtError::EnumParseError(format!("Invalid SleepMode value: {}", value).unwrap_or_default())),
        }
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

    async fn send(self, ctr: &AtClient<'_>) -> AtResponseMessage {
        ctr.tx.send(self).await;
        let response = ctr.rx.receive().await?;
        Ok(response)
    }
}

type AtResponseMessage = Result<Vec<String<AT_BUFFER_SIZE>, MAX_RESPONSE_LINES>, AtError>;

pub struct State {
    pub(super) tx_channel: Channel<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    pub(super) rx_channel: Channel<NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>,
}

impl State {
    pub fn new() -> Self {
        Self {
            tx_channel: Channel::<NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>::new(),
            rx_channel: Channel::<NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>::new(),
        }
    }
}

pub struct AtClient<'ch> {
    tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>,
    rx: Receiver<'ch, NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>,
}

impl<'ch> AtClient<'ch> {
    pub fn new(tx: Sender<'ch, NoopRawMutex, AtRequestMessage, CHANNEL_SIZE>, rx: Receiver<'ch, NoopRawMutex, AtResponseMessage, CHANNEL_SIZE>) -> Self {
        Self { tx, rx }
    }
}

/*
macro_rules! request {
    ($ctr:expr, $n:expr, $s:literal $(, $x:expr)* $(,)?) => {{
        let req_str = heapless::format!($s $(, $x)*)?;
        $ctr.tx.send(req_str).await;
        let mut response = $ctr.rx.receive().await?;
        if response.len() != $n {
            return Err(AtError::Error);
        }
        response
    }};
}
*/

macro_rules! request {
    ($s:literal $(, $x:expr)* $(,)?) => {{
        let req_str = heapless::format!($s $(, $x)*)?;
        AtRequestMessage { command: req_str, timeout: Duration::from_secs(5) }
    }};
}

pub async fn at(ctr: &AtClient<'_>) -> Result<(), AtError> {
    request!("AT").with_timeout(Duration::from_millis(200)).send(ctr).await?;
    Ok(())
}

pub async fn set_apn(ctr: &AtClient<'_>, apn: &str) -> Result<(), AtError> {
    request!("AT+CGDCONT=1,\"IP\",\"{}\"", apn).send(ctr).await?;
    Ok(())
}

// +CREG: <n>,<stat>[,<lac>,<ci>]
// +CREG: 0,1
pub async fn get_network_registration(ctr: &AtClient<'_>) -> Result<(NetworkRegistrationUrcConfig, NetworkRegistrationState), AtError> {
    let response = request!("AT+CREG?").send(ctr).await?;
    if response.len() != 1 {
        return Err(AtError::ResponseLineCountMismatch {
            expected: 1,
            actual: response.len(),
        });
    }
    let (_, (_, n, _, stat)) = (tag("+CREG: "), number, sperator, number).parse(&response[0])?;
    Ok((n.try_into()?, stat.try_into()?))
}

pub async fn set_sleep_mode(ctr: &AtClient<'_>, mode: SleepMode) -> Result<(), AtError> {
    request!("AT+CSCLK={}", mode as i32).send(ctr).await?;
    Ok(())
}

pub async fn read_sleep_mode(ctr: &AtClient<'_>) -> Result<SleepMode, AtError> {
    let response = request!("AT+CSCLK?").send(ctr).await?;
    if response.len() != 1 {
        return Err(AtError::ResponseLineCountMismatch {
            expected: 1,
            actual: response.len(),
        });
    }
    let (_, (_, mode)) = (tag("+CSCLK: "), number).parse(&response[0])?;
    Ok(mode.try_into()?)
}

pub struct GetNetworkRegistrationStatus {}

impl GetNetworkRegistrationStatus {
    pub fn execute() -> Result<(NetworkRegistrationUrcConfig, NetworkRegistrationState), AtError> {
        todo!()
    }
}

impl From<nom::Err<nom::error::Error<&str>>> for AtError {
    fn from(_err: nom::Err<nom::error::Error<&str>>) -> Self {
        AtError::Error
    }
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

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_network_registration() {
        /*
        let (n, stat) = parse_network_registration_response("+CREG: 0,1").unwrap();
        assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
        assert_eq!(stat, NetworkRegistrationState::Registered);

        let (n, stat) = parse_network_registration_response("+CREG: 0,0").unwrap();
        assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
        assert_eq!(stat, NetworkRegistrationState::NotRegistered);

        let (n, stat) = parse_network_registration_response("+CREG: 0,11").unwrap();
        assert_eq!(n, NetworkRegistrationUrcConfig::UrcDisabled);
        assert_eq!(stat, NetworkRegistrationState::NotRegisteredSearching);
        */
    }
}
