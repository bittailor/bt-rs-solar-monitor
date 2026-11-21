//! SIMCom A76xx MQTT 3.1.1 commands (AT manual, chapter 18).
use embassy_time::Duration;
use heapless::{Deque, Vec};

use super::{AtClient, AtCommandRequest, AtController, AtError};
use crate::{at_request, debug, error};

pub const MQTT_TOPIC_CAPACITY: usize = 256;
pub const MQTT_PAYLOAD_CAPACITY: usize = 1024;
pub const MQTT_QUEUE_CAPACITY: usize = 2;
const TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Copy)]
pub enum MqttQos {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

/// TLS 1.2 with server authentication, certificate time checks and SNI.
pub struct MqttTlsConfig<'a> {
    /// Name of a PEM/DER CA certificate already uploaded to the modem.
    pub ca_file: &'a str,
}

/// One client (modem index 0), using MQTT 3.1.1 and a clean session.
pub struct MqttConfig<'a> {
    pub host: &'a str,
    pub port: u16,
    pub client_id: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub keep_alive_secs: u16,
    pub tls: Option<MqttTlsConfig<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MqttMessage {
    pub topic: Vec<u8, MQTT_TOPIC_CAPACITY>,
    pub payload: Vec<u8, MQTT_PAYLOAD_CAPACITY>,
}

pub(crate) fn invalid_input<T>() -> Result<T, AtError> {
    error!("Invalid MQTT parameter");
    Err(AtError::InvalidParameter)
}

fn quoted(value: &str, max: usize) -> Result<(), AtError> {
    if value.is_empty() || value.len() > max || value.bytes().any(|b| b < 0x20 || b == b'"' || b == b'\\' || b == 0x7f) {
        return invalid_input();
    }
    Ok(())
}

pub(crate) fn validate(config: &MqttConfig<'_>) -> Result<(), AtError> {
    quoted(config.host, 240)?;
    if !config.host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') || config.port == 0 || !(1..=64800).contains(&config.keep_alive_secs) {
        return invalid_input();
    }
    quoted(config.client_id, 128)?;
    quoted(config.username, 256)?;
    quoted(config.password, 256)?;
    if let Some(tls) = &config.tls {
        validate_ca_file(tls.ca_file)?;
    }
    // Check the transport's command capacity before changing modem state.
    connect_request(config)?;
    Ok(())
}

fn validate_ca_file(file: &str) -> Result<(), AtError> {
    quoted(file, 108)?;
    if file.len() < 5 || !file.is_ascii() || !(file.ends_with(".pem") || file.ends_with(".der")) {
        return invalid_input();
    }
    Ok(())
}

pub(crate) fn validate_topic(topic: &str, filter: bool) -> Result<(), AtError> {
    if topic.is_empty() || topic.len() > 1024 || topic.contains('\0') || (!filter && (topic.contains('+') || topic.contains('#'))) {
        return invalid_input();
    }
    if filter {
        let mut levels = topic.split('/').peekable();
        while let Some(level) = levels.next() {
            if (level.contains('+') && level != "+") || (level.contains('#') && (level != "#" || levels.peek().is_some())) {
                return invalid_input();
            }
        }
    }
    Ok(())
}

fn connect_request(config: &MqttConfig<'_>) -> Result<AtCommandRequest, AtError> {
    Ok(
        at_request!("AT+CMQTTCONNECT=0,\"tcp://{}:{}\",{},1,\"{}\",\"{}\"", config.host, config.port, config.keep_alive_secs, config.username, config.password)
            .sensitive(),
    )
}

async fn result<'ch, C: AtController>(client: &impl AtClient<'ch, C>, request: AtCommandRequest, prefix: &str, indexed: bool) -> Result<(), AtError> {
    let response = request.with_timeout(TIMEOUT).with_urc_prefix(prefix.try_into()?).send(client).await?;
    let line = response.lines.iter().find_map(|line| line.strip_prefix(prefix)).ok_or(AtError::FormatError)?;
    let mut fields = line.trim().split(',').map(str::trim);
    if indexed && fields.next() != Some("0") {
        return Err(AtError::FormatError);
    }
    let code = fields.next().ok_or(AtError::FormatError)?.parse::<u16>().map_err(|_| AtError::FormatError)?;
    if fields.next().is_some() {
        return Err(AtError::FormatError);
    }
    if code != 0 {
        error!("MQTT command {} failed with code {}", prefix, code);
        return Err(AtError::Mqtt(code));
    }
    Ok(())
}

pub(crate) async fn upload_ca<'ch, C: AtController>(client: &impl AtClient<'ch, C>, filename: &str, certificate: &[u8]) -> Result<(), AtError> {
    validate_ca_file(filename)?;
    if certificate.is_empty() || certificate.len() > 10240 {
        return invalid_input();
    }
    let request = at_request!("AT+CCERTDOWN=\"{}\",{}", filename, certificate.len()).with_timeout(Duration::from_secs(120));
    client.use_controller(async |ctr| ctr.handle_data_command(&request, certificate).await).await
}

pub(crate) async fn start<'ch, C: AtController>(client: &impl AtClient<'ch, C>) -> Result<(), AtError> {
    // Raw data echo cannot be distinguished from incoming protocol framing.
    disable_echo(client).await?;
    super::serial_interface::set_sleep_mode(client, super::serial_interface::SleepMode::Off).await?;
    result(client, at_request!("AT+CMQTTSTART"), "+CMQTTSTART:", false).await?;
    client.use_controller(async |ctr| ctr.clear_mqtt()).await;
    Ok(())
}

pub(crate) async fn disable_echo<'ch, C: AtController>(client: &impl AtClient<'ch, C>) -> Result<(), AtError> {
    at_request!("ATE0").send(client).await?;
    Ok(())
}

pub(crate) async fn acquire<'ch, C: AtController>(client: &impl AtClient<'ch, C>, config: &MqttConfig<'_>) -> Result<(), AtError> {
    at_request!("AT+CMQTTACCQ=0,\"{}\",{}", config.client_id, u8::from(config.tls.is_some()))
        .send(client)
        .await?;
    Ok(())
}

pub(crate) async fn connect<'ch, C: AtController>(client: &impl AtClient<'ch, C>, config: &MqttConfig<'_>) -> Result<(), AtError> {
    at_request!("AT+CMQTTCFG=\"version\",0,4").send(client).await?;
    at_request!("AT+CMQTTCFG=\"checkUTF8\",0,0").send(client).await?;
    if let Some(tls) = &config.tls {
        at_request!("AT+CSSLCFG=\"sslversion\",0,3").send(client).await?;
        at_request!("AT+CSSLCFG=\"authmode\",0,1").send(client).await?;
        at_request!("AT+CSSLCFG=\"ignorelocaltime\",0,0").send(client).await?;
        at_request!("AT+CSSLCFG=\"enableSNI\",0,1").send(client).await?;
        at_request!("AT+CSSLCFG=\"negotiatetime\",0,60").send(client).await?;
        at_request!("AT+CSSLCFG=\"cacert\",0,\"{}\"", tls.ca_file).send(client).await?;
        at_request!("AT+CMQTTSSLCFG=0,0").send(client).await?;
    }
    result(client, connect_request(config)?, "+CMQTTCONNECT:", true).await
}

pub(crate) async fn publish<'ch, C: AtController>(
    client: &impl AtClient<'ch, C>,
    topic: &str,
    payload: &[u8],
    qos: MqttQos,
    retain: bool,
) -> Result<(), AtError> {
    validate_topic(topic, false)?;
    if payload.is_empty() || payload.len() > 10240 {
        return invalid_input();
    }
    let topic_request = at_request!("AT+CMQTTTOPIC=0,{}", topic.len());
    let payload_request = at_request!("AT+CMQTTPAYLOAD=0,{}", payload.len());
    client
        .use_controller(async |ctr| {
            ctr.handle_data_command(&topic_request, topic.as_bytes()).await?;
            ctr.handle_data_command(&payload_request, payload).await
        })
        .await?;
    result(client, at_request!("AT+CMQTTPUB=0,{},60,{},0", qos as u8, u8::from(retain)), "+CMQTTPUB:", true).await
}

pub(crate) async fn subscribe<'ch, C: AtController>(client: &impl AtClient<'ch, C>, topic: &str, qos: MqttQos) -> Result<(), AtError> {
    validate_topic(topic, true)?;
    let request = at_request!("AT+CMQTTSUBTOPIC=0,{},{}", topic.len(), qos as u8);
    client
        .use_controller(async |ctr| ctr.handle_data_command(&request, topic.as_bytes()).await)
        .await?;
    result(client, at_request!("AT+CMQTTSUB=0"), "+CMQTTSUB:", true).await
}

pub(crate) async fn unsubscribe<'ch, C: AtController>(client: &impl AtClient<'ch, C>, topic: &str) -> Result<(), AtError> {
    validate_topic(topic, true)?;
    let request = at_request!("AT+CMQTTUNSUBTOPIC=0,{}", topic.len());
    client
        .use_controller(async |ctr| ctr.handle_data_command(&request, topic.as_bytes()).await)
        .await?;
    result(client, at_request!("AT+CMQTTUNSUB=0,0"), "+CMQTTUNSUB:", true).await
}

pub(crate) async fn disconnect<'ch, C: AtController>(client: &impl AtClient<'ch, C>) -> Result<(), AtError> {
    result(client, at_request!("AT+CMQTTDISC=0,60"), "+CMQTTDISC:", true).await
}

pub(crate) async fn release<'ch, C: AtController>(client: &impl AtClient<'ch, C>) -> Result<(), AtError> {
    at_request!("AT+CMQTTREL=0").send(client).await?;
    Ok(())
}

pub(crate) async fn stop<'ch, C: AtController>(client: &impl AtClient<'ch, C>) -> Result<(), AtError> {
    result(client, at_request!("AT+CMQTTSTOP"), "+CMQTTSTOP:", false).await
}

/// Persistent state: the runner may be cancelled between any two UART bytes.
pub(super) struct Receiver {
    message: MqttMessage,
    queue: Deque<MqttMessage, MQTT_QUEUE_CAPACITY>,
    error: Option<AtError>,
    expected_topic: usize,
    expected_payload: usize,
    topic_read: usize,
    payload_read: usize,
    active: bool,
    remaining: usize,
    payload: bool,
    discard: bool,
}

impl Receiver {
    pub fn new() -> Self {
        Self {
            message: MqttMessage {
                topic: Vec::new(),
                payload: Vec::new(),
            },
            queue: Deque::new(),
            error: None,
            expected_topic: 0,
            expected_payload: 0,
            topic_read: 0,
            payload_read: 0,
            active: false,
            remaining: 0,
            payload: false,
            discard: false,
        }
    }

    fn fail(&mut self, error: AtError) {
        error!("MQTT receive error: {:?}", error);
        self.error = Some(error);
        self.discard = true;
    }

    pub fn take(&mut self) -> Option<Result<MqttMessage, AtError>> {
        if let Some(error) = self.error.take() {
            return Some(Err(error));
        }
        self.queue.pop_front().map(Ok)
    }

    pub fn byte(&mut self, byte: u8) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        let full = if self.payload {
            self.payload_read += 1;
            !self.discard && self.message.payload.push(byte).is_err()
        } else {
            self.topic_read += 1;
            !self.discard && self.message.topic.push(byte).is_err()
        };
        if full {
            self.fail(AtError::CapacityError);
        }
        true
    }

    pub fn line(&mut self, line: &str) -> Result<bool, AtError> {
        if let Some(fields) = line.strip_prefix("+CMQTTCONNLOST:") {
            let [index, cause] = numbers(fields)?;
            if index == 0 {
                self.fail(AtError::MqttConnectionLost(u16::try_from(cause).map_err(|_| AtError::FormatError)?));
            }
            return Ok(true);
        }
        if let Some(fields) = line.strip_prefix("+CMQTTRXSTART:") {
            let [index, topic, payload] = numbers(fields)?;
            debug!("MQTT RX start: client {}, topic {} bytes, payload {} bytes", index, topic, payload);
            self.message.topic.clear();
            self.message.payload.clear();
            self.expected_topic = topic;
            self.expected_payload = payload;
            self.topic_read = 0;
            self.payload_read = 0;
            self.active = true;
            self.discard = false;
            if index != 0 {
                self.fail(AtError::FormatError);
            } else if topic > MQTT_TOPIC_CAPACITY || payload > MQTT_PAYLOAD_CAPACITY {
                self.fail(AtError::CapacityError);
            }
            return Ok(true);
        }
        for (prefix, payload) in [("+CMQTTRXTOPIC:", false), ("+CMQTTRXPAYLOAD:", true)] {
            if let Some(fields) = line.strip_prefix(prefix) {
                let [index, len] = numbers(fields)?;
                self.remaining = len;
                self.payload = payload;
                if index != 0 || !self.active {
                    self.fail(AtError::FormatError);
                }
                return Ok(true);
            }
        }
        if let Some(fields) = line.strip_prefix("+CMQTTRXEND:") {
            let [index] = numbers(fields)?;
            if index != 0 || !self.active || self.topic_read != self.expected_topic || self.payload_read != self.expected_payload {
                self.fail(AtError::FormatError);
            }
            if !self.discard {
                let message = core::mem::replace(
                    &mut self.message,
                    MqttMessage {
                        topic: Vec::new(),
                        payload: Vec::new(),
                    },
                );
                if self.queue.push_back(message).is_err() {
                    self.fail(AtError::CapacityError);
                } else {
                    debug!("MQTT RX message queued");
                }
            }
            self.active = false;
            return Ok(true);
        }
        Ok(false)
    }
}

fn numbers<const N: usize>(fields: &str) -> Result<[usize; N], AtError> {
    let mut result = [0; N];
    let mut values = fields.trim().split(',');
    for value in &mut result {
        *value = values.next().ok_or(AtError::FormatError)?.trim().parse().map_err(|_| AtError::FormatError)?;
    }
    if values.next().is_some() {
        return Err(AtError::FormatError);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::at::AtControllerImpl;
    use embedded_io_async::{ErrorType, Read, Write};
    use std::{collections::VecDeque, vec::Vec as StdVec};

    enum Step {
        Read(StdVec<u8>),
        Write(StdVec<u8>),
    }

    struct Stream {
        steps: VecDeque<Step>,
        offset: usize,
    }

    impl Stream {
        fn new() -> Self {
            Self {
                steps: VecDeque::new(),
                offset: 0,
            }
        }

        fn read(&mut self, data: impl AsRef<[u8]>) {
            self.steps.push_back(Step::Read(data.as_ref().to_vec()));
        }

        fn write(&mut self, data: impl AsRef<[u8]>) {
            self.steps.push_back(Step::Write(data.as_ref().to_vec()));
        }

        fn command(&mut self, command: &str, response: &str) {
            self.write(std::format!("{command}\r\n"));
            self.read(response);
        }

        fn data(&mut self, command: &str, data: &[u8]) {
            self.command(command, "\r\n>");
            self.write(data);
            self.read(" \r\nOK\r\n");
        }
    }

    impl ErrorType for Stream {
        type Error = embedded_io_async::ErrorKind;
    }

    impl Read for Stream {
        async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            match self.steps.front() {
                Some(Step::Read(data)) => {
                    // Deliberately fragment all UART input down to one byte.
                    buf[0] = data[self.offset];
                    self.offset += 1;
                    if self.offset == data.len() {
                        self.steps.pop_front();
                        self.offset = 0;
                    }
                    Ok(1)
                }
                _ => core::future::pending().await,
            }
        }
    }

    impl Write for Stream {
        async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
            let Some(Step::Write(expected)) = self.steps.front() else {
                panic!("Unexpected write: {buf:?}");
            };
            let len = buf.len().min(expected.len() - self.offset);
            assert_eq!(&buf[..len], &expected[self.offset..self.offset + len]);
            self.offset += len;
            if self.offset == expected.len() {
                self.steps.pop_front();
                self.offset = 0;
            }
            Ok(len)
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    struct Client(tokio::sync::Mutex<AtControllerImpl<Stream>>);

    impl<'ch> AtClient<'ch, AtControllerImpl<Stream>> for Client {
        async fn use_controller<'a, F, R>(&'a self, mut f: F) -> R
        where
            F: AsyncFnMut(&mut AtControllerImpl<Stream>) -> R + 'a,
            AtControllerImpl<Stream>: 'a,
        {
            f(&mut *self.0.lock().await).await
        }
    }

    fn config() -> MqttConfig<'static> {
        MqttConfig {
            host: "broker.example",
            port: 8883,
            client_id: "test-client",
            username: "test-user",
            password: "test-password",
            keep_alive_secs: 60,
            tls: Some(MqttTlsConfig { ca_file: "test.pem" }),
        }
    }

    fn frame(topic: &[u8], payload: &[u8]) -> StdVec<u8> {
        let mut bytes = std::format!("\r\n+CMQTTRXSTART: 0,{},{}\r\n+CMQTTRXTOPIC: 0,{}\r\n", topic.len(), payload.len(), topic.len()).into_bytes();
        bytes.extend_from_slice(topic);
        bytes.extend_from_slice(std::format!("\r\n+CMQTTRXPAYLOAD: 0,{}\r\n", payload.len()).as_bytes());
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(b"\r\n+CMQTTRXEND: 0\r\n");
        bytes
    }

    #[tokio::test]
    async fn tls_lifecycle_and_loopback_during_publish() {
        let mut stream = Stream::new();
        stream.data("AT+CCERTDOWN=\"test.pem\",4", b"cert");
        stream.command("ATE0", "\r\nOK\r\n");
        stream.command("AT+CSCLK=0", "\r\nOK\r\n");
        stream.command("AT+CMQTTSTART", "\r\nOK\r\n+CMQTTSTART: 0\r\n");
        stream.command("AT+CMQTTACCQ=0,\"test-client\",1", "\r\nOK\r\n");
        stream.command("AT+CMQTTCFG=\"version\",0,4", "\r\nOK\r\n");
        stream.command("AT+CMQTTCFG=\"checkUTF8\",0,0", "\r\nOK\r\n");
        for command in [
            "AT+CSSLCFG=\"sslversion\",0,3",
            "AT+CSSLCFG=\"authmode\",0,1",
            "AT+CSSLCFG=\"ignorelocaltime\",0,0",
            "AT+CSSLCFG=\"enableSNI\",0,1",
            "AT+CSSLCFG=\"negotiatetime\",0,60",
            "AT+CSSLCFG=\"cacert\",0,\"test.pem\"",
            "AT+CMQTTSSLCFG=0,0",
        ] {
            stream.command(command, "\r\nOK\r\n");
        }
        stream.command("AT+CMQTTCONNECT=0,\"tcp://broker.example:8883\",60,1,\"test-user\",\"test-password\"", "\r\nOK\r\n+CMQTTCONNECT: 0,0\r\n");
        stream.data("AT+CMQTTSUBTOPIC=0,3,1", b"a/b");
        stream.command("AT+CMQTTSUB=0", "\r\nOK\r\n+CMQTTSUB: 0,0\r\n");
        stream.data("AT+CMQTTTOPIC=0,3", b"a/b");
        stream.data("AT+CMQTTPAYLOAD=0,6", b"\0\r\n\xffOK");
        stream.write("AT+CMQTTPUB=0,1,60,0,0\r\n");
        stream.read("\r\nOK\r\n");
        stream.read(frame(b"a/b", b"\0\r\n\xffOK"));
        stream.read("\r\n+CMQTTPUB: 0,0\r\n");
        stream.data("AT+CMQTTUNSUBTOPIC=0,3", b"a/b");
        stream.command("AT+CMQTTUNSUB=0,0", "\r\nOK\r\n+CMQTTUNSUB: 0,0\r\n");
        // Both result/OK orderings are documented for disconnect.
        stream.command("AT+CMQTTDISC=0,60", "\r\n+CMQTTDISC: 0,0\r\nOK\r\n");
        stream.command("AT+CMQTTREL=0", "\r\nOK\r\n");
        stream.command("AT+CMQTTSTOP", "\r\nOK\r\n+CMQTTSTOP: 0\r\n");
        let client = Client(tokio::sync::Mutex::new(AtControllerImpl::new(stream)));
        upload_ca(&client, "test.pem", b"cert").await.unwrap();
        start(&client).await.unwrap();
        acquire(&client, &config()).await.unwrap();
        connect(&client, &config()).await.unwrap();
        subscribe(&client, "a/b", MqttQos::AtLeastOnce).await.unwrap();
        publish(&client, "a/b", b"\0\r\n\xffOK", MqttQos::AtLeastOnce, false).await.unwrap();
        let message = client.0.lock().await.handle_mqtt_receive(Duration::from_millis(10)).await.unwrap();
        assert_eq!(message.topic.as_slice(), b"a/b");
        assert_eq!(message.payload.as_slice(), b"\0\r\n\xffOK");
        unsubscribe(&client, "a/b").await.unwrap();
        disconnect(&client).await.unwrap();
        release(&client).await.unwrap();
        stop(&client).await.unwrap();
        assert!(client.0.lock().await.stream.steps.is_empty());
    }

    #[tokio::test]
    async fn split_binary_receive_survives_timeout_mid_payload() {
        let mut stream = Stream::new();
        stream.read("\r\n+CMQTTRXSTART: 0,3,6\r\n+CMQTTRXTOPIC: 0,1\r\na\r\n+CMQTTRXTOPIC: 0,2\r\n/b\r\n+CMQTTRXPAYLOAD: 0,4\r\n\0");
        let mut ctr = AtControllerImpl::new(stream);
        assert_eq!(ctr.handle_mqtt_receive(Duration::from_millis(5)).await, Err(AtError::Timeout));
        ctr.stream.read(b"\r\n\xff\r\n+CMQTTRXPAYLOAD: 0,2\r\nOK\r\n+CMQTTRXEND: 0\r\n");
        let message = ctr.handle_mqtt_receive(Duration::from_millis(20)).await.unwrap();
        assert_eq!(message.topic.as_slice(), b"a/b");
        assert_eq!(message.payload.as_slice(), b"\0\r\n\xffOK");
    }

    #[tokio::test]
    async fn overflow_drains_frame_and_preserves_next_command() {
        let mut stream = Stream::new();
        stream.write("AT\r\n");
        stream.read(frame(b"a", &std::vec![0xff; MQTT_PAYLOAD_CAPACITY + 1]));
        stream.read("\r\nOK\r\n");
        let mut ctr = AtControllerImpl::new(stream);
        ctr.handle_command(&AtCommandRequest::new("AT".try_into().unwrap())).await.unwrap();
        assert_eq!(ctr.handle_mqtt_receive(Duration::from_millis(10)).await, Err(AtError::CapacityError));
        ctr.stream.command("AT", "\r\nOK\r\n");
        ctr.handle_command(&AtCommandRequest::new("AT".try_into().unwrap())).await.unwrap();
    }

    #[tokio::test]
    async fn receive_accepts_capacity_boundaries_and_empty_payload() {
        let topic = std::vec![b'a'; MQTT_TOPIC_CAPACITY];
        let payload = std::vec![0xff; MQTT_PAYLOAD_CAPACITY];
        let mut stream = Stream::new();
        stream.read(frame(&topic, &payload));
        stream.read(frame(b"empty", b""));
        let mut ctr = AtControllerImpl::new(stream);
        let message = ctr.handle_mqtt_receive(Duration::from_millis(100)).await.unwrap();
        assert_eq!(message.topic.as_slice(), topic);
        assert_eq!(message.payload.as_slice(), payload);
        assert!(ctr.handle_mqtt_receive(Duration::from_millis(100)).await.unwrap().payload.is_empty());
    }

    #[tokio::test]
    async fn queue_overflow_is_explicit_and_keeps_queued_messages() {
        let mut stream = Stream::new();
        stream.write("AT\r\n");
        for payload in [b"one", b"two", b"tri"] {
            stream.read(frame(b"a", payload));
        }
        stream.read("\r\nOK\r\n");
        let mut ctr = AtControllerImpl::new(stream);
        ctr.handle_command(&AtCommandRequest::new("AT".try_into().unwrap())).await.unwrap();
        assert_eq!(ctr.handle_mqtt_receive(Duration::from_millis(10)).await, Err(AtError::CapacityError));
        assert_eq!(ctr.handle_mqtt_receive(Duration::from_millis(10)).await.unwrap().payload.as_slice(), b"one");
        assert_eq!(ctr.handle_mqtt_receive(Duration::from_millis(10)).await.unwrap().payload.as_slice(), b"two");
    }

    #[tokio::test]
    async fn idle_runner_queues_message_and_receive_timeout_releases_controller() {
        let mut stream = Stream::new();
        stream.read(frame(b"a", b"hello"));
        let mut state = crate::at::State::new();
        let (runner, client) = crate::at::new(&mut state, stream);
        let sequence = async {
            tokio::time::sleep(core::time::Duration::from_millis(5)).await;
            let message = client
                .use_controller(async |ctr| ctr.handle_mqtt_receive(Duration::from_millis(10)).await)
                .await
                .unwrap();
            assert_eq!(message.payload.as_slice(), b"hello");
            let error = client
                .use_controller(async |ctr| ctr.handle_mqtt_receive(Duration::from_millis(5)).await)
                .await
                .unwrap_err();
            assert_eq!(error, AtError::Timeout);
            client.use_controller(async |ctr| ctr.stream.command("AT", "\r\nOK\r\n")).await;
            crate::at::at(&client).await.unwrap();
        };
        tokio::time::timeout(core::time::Duration::from_secs(1), async {
            embassy_futures::select::select(runner.run(), sequence).await;
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn modem_failure_and_connection_loss_are_reported() {
        let mut stream = Stream::new();
        stream.command("AT+CMQTTSTART", "\r\nOK\r\n+CMQTTSTART: 23\r\n");
        let client = Client(tokio::sync::Mutex::new(AtControllerImpl::new(stream)));
        let request = AtCommandRequest::new("AT+CMQTTSTART".try_into().unwrap());
        assert_eq!(result(&client, request, "+CMQTTSTART:", false).await, Err(AtError::Mqtt(23)));
        let mut stream = Stream::new();
        stream.read("\r\n+CMQTTCONNLOST: 0,3\r\n");
        let mut ctr = AtControllerImpl::new(stream);
        assert_eq!(ctr.handle_mqtt_receive(Duration::from_millis(10)).await, Err(AtError::MqttConnectionLost(3)));
    }

    #[tokio::test]
    async fn prompt_error_does_not_send_data() {
        let mut stream = Stream::new();
        stream.command("AT+CMQTTTOPIC=0,3", "\r\n+CME ERROR: 3\r\n");
        let mut ctr = AtControllerImpl::new(stream);
        let request = AtCommandRequest::new("AT+CMQTTTOPIC=0,3".try_into().unwrap());
        assert_eq!(ctr.handle_data_command(&request, b"a/b").await, Err(AtError::Error));
        assert!(ctr.stream.steps.is_empty());
    }

    #[test]
    fn validates_before_issuing_commands_and_redacts_credentials() {
        let mut config = config();
        validate(&config).unwrap();
        config.password = "secret\"\r\nAT";
        assert_eq!(validate(&config), Err(AtError::InvalidParameter));
        config.password = "test-password";
        config.keep_alive_secs = 0;
        assert_eq!(validate(&config), Err(AtError::InvalidParameter));
        assert_eq!(validate_topic("a/#/b", true), Err(AtError::InvalidParameter));
        assert_eq!(validate_topic("a+", true), Err(AtError::InvalidParameter));
        assert_eq!(validate_topic("a/+", false), Err(AtError::InvalidParameter));
        validate_topic("a/+/#", true).unwrap();
        validate_topic(&"a".repeat(1024), false).unwrap();
        assert_eq!(validate_topic(&"a".repeat(1025), false), Err(AtError::InvalidParameter));
        let debug = std::format!("{:?}", connect_request(&config).unwrap());
        assert!(!debug.contains("test-password"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn malformed_receive_lengths_are_not_delivered() {
        let mut receiver = Receiver::new();
        receiver.line("+CMQTTRXSTART: 0,3,1").unwrap();
        receiver.line("+CMQTTRXTOPIC: 0,1").unwrap();
        receiver.byte(b'a');
        receiver.line("+CMQTTRXPAYLOAD: 0,1").unwrap();
        receiver.byte(b'b');
        receiver.line("+CMQTTRXEND: 0").unwrap();
        assert_eq!(receiver.take(), Some(Err(AtError::FormatError)));
        assert!(receiver.take().is_none());
    }
}
