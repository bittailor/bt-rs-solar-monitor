pub mod at;

use core::mem::replace;

use defmt::Format;
use embassy_futures::select::select;
use embassy_sync::{blocking_mutex::raw::RawMutex, channel::Receiver};
use embassy_time::{Duration, with_timeout};
use embedded_io_async::{ErrorType, Read, Write};
use heapless::String;

const AT_BUFFER_SIZE: usize = 1024;

pub struct Command {
    command: String<AT_BUFFER_SIZE>,
    timeout: Duration,
}

impl Command {
    pub fn cmd(command: &str) -> Self {
        let mut new = Command {
            command: String::new(),
            timeout: Duration::from_secs(5),
        };
        new.command.push_str(command);
        new
    }
}

pub struct Runner<'ch, S: Read + Write, M: RawMutex, const N: usize>
where
    <S as ErrorType>::Error: Format,
{
    receiver: Receiver<'ch, M, Command, N>,
    at_controller: AtController<S>,
}

impl<'ch, S: Read + Write, M: RawMutex, const N: usize> Runner<'ch, S, M, N>
where
    <S as ErrorType>::Error: Format,
{
    pub fn new(stream: S, receiver: Receiver<'ch, M, Command, N>) -> Self {
        Self {
            receiver,
            at_controller: AtController::new(stream),
        }
    }

    pub async fn run(mut self) {
        loop {
            //let receiver = self.channel.receiver();
            match select(self.receiver.receive(), self.at_controller.poll_urc()).await {
                embassy_futures::select::Either::First(cmd) => self.send_command(cmd).await,
                embassy_futures::select::Either::Second(urc) => self.handle_urc(urc).await,
            };
        }
    }

    async fn send_command(&mut self, command: Command) {
        debug!("Try sent command: {}", command.command);
        if let Err(_e) = self.at_controller.stream.write_all(command.command.as_bytes()).await {
            error!("Failed to send command: {}", command.command);
            return;
        }

        if let Err(_e) = self.at_controller.stream.write_all(b"\r\n").await {
            error!("Failed to send command: {}", command.command);
            return;
        }
        info!("Command sent: {}", command.command);

        match with_timeout(command.timeout, async {
            let mut counter = 0;
            loop {
                let line = self.at_controller.read_line().await;
                if line == "OK" {
                    info!("Command completed with {} linse", counter);
                    break;
                } else if line == "ERROR" {
                    error!("Command error with {} lines", counter);
                    break;
                } else {
                    info!(">{}> {}", counter, line.as_str());
                    counter += 1;
                }
            }
        })
        .await
        {
            Ok(_) => info!("Command '{}' completed", command.command),
            Err(_e) => error!("Command '{}' timeout", command.command),
        }
    }

    async fn handle_urc(&mut self, urc: String<AT_BUFFER_SIZE>) {
        info!("Handling URC: {}", urc.as_str());
    }
}

struct AtController<S: Read + Write>
where
    <S as ErrorType>::Error: Format,
{
    stream: S,
    line_buffer: heapless::Vec<u8, AT_BUFFER_SIZE>,
}

impl<S: Read + Write> AtController<S>
where
    <S as ErrorType>::Error: Format,
{
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
                        debug!("UART.RX line of lenght {}", self.line_buffer.len());
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
                Err(e) => warn!("Read error: {}", e),
            };
        }
    }
}
