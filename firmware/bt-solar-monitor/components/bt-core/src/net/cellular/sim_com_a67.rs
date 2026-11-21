use core::str::{self};

use chrono::NaiveDateTime;
use embassy_futures::yield_now;
use embassy_time::{Duration, Timer, WithTimeout, with_timeout};
use embedded_hal::digital::OutputPin;
use embedded_io_async::Read;

use crate::{
    at::{AtClient, AtController, http::HttpStatusCode, network::NetworkRegistrationState, serial_interface::SleepMode, status_control::Rssi},
    net::cellular::CellularError,
};

pub use crate::at::mqtt::{MqttConfig, MqttMessage, MqttQos, MqttTlsConfig};
use crate::at::{AtError, mqtt};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MqttState {
    Stopped,
    Started,
    Acquired,
    Connected,
}

pub struct SimComCellularModule<'ch, Output: OutputPin, Ctr: AtController> {
    at_client: crate::at::AtClientImpl<'ch, Ctr>,
    pwrkey: Output,
    reset: Output,
    http_initialized: bool,
    mqtt_state: MqttState,
}

impl<'ch, Output: OutputPin, Ctr: AtController> SimComCellularModule<'ch, Output, Ctr> {
    pub fn new(at_client: crate::at::AtClientImpl<'ch, Ctr>, pwrkey: Output, reset: Output) -> Self {
        SimComCellularModule {
            at_client,
            pwrkey,
            reset,
            http_initialized: false,
            mqtt_state: MqttState::Stopped,
        }
    }

    pub async fn is_alive(&self) -> bool {
        crate::at::at(&self.at_client).await.is_ok()
    }

    pub async fn power_cycle(&mut self) -> Result<(), CellularError> {
        if self.is_alive().await {
            info!("still on => first power_down ...");
            self.power_down().await?;
            Timer::after_secs(1).await; // Just some 'safety' delay
        }
        self.power_on().await
    }

    pub async fn power_on(&mut self) -> Result<(), CellularError> {
        self.http_initialized = false;
        self.mqtt_state = MqttState::Stopped;
        info!("power on ...");
        self.pwrkey.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(50).await;
        self.pwrkey.set_high().map_err(|_| CellularError::GpioError {})?;
        info!("... wait 8s to startup ...");
        Timer::after_secs(8).await;
        self.at_client.use_controller(async |ctr| ctr.clear_mqtt()).await;
        info!("... check AT ...");
        self.ensure_at(Duration::from_secs(10)).await?;
        info!("... power on done");
        crate::at::network::set_automatic_time_and_time_zone_update(&self.at_client, true).await?;
        Ok(())
    }

    pub async fn startup_network(&mut self, apn: &str) -> Result<(), CellularError> {
        self.set_apn(apn).await?;

        while self.read_network_registration().await?.1 != NetworkRegistrationState::Registered {
            warn!("Not registered to network yet, waiting...");
            Timer::after_secs(1).await;
            info!("... retrying ...");
        }
        let _rtc = self.query_real_time_clock().await?;
        Ok(())
    }

    pub async fn power_down(&mut self) -> Result<(), CellularError> {
        crate::at::status_control::power_down(&self.at_client).await?;
        self.http_initialized = false;
        self.mqtt_state = MqttState::Stopped;
        Timer::after_secs(2).await; // Power off time
        Timer::after_secs(2).await; // Power off - power on buffer time
        Ok(())
    }

    pub async fn reset(&mut self) -> Result<(), CellularError> {
        info!("reset ...");
        self.reset.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(2500).await;
        self.reset.set_high().map_err(|_| CellularError::GpioError {})?;
        self.http_initialized = false;
        self.mqtt_state = MqttState::Stopped;
        info!("... wait a bit for module to start ...");
        Timer::after_millis(5000).await;
        self.at_client.use_controller(async |ctr| ctr.clear_mqtt()).await;
        info!("... reset done");
        Ok(())
    }

    async fn ensure_at(&self, timeout: Duration) -> Result<(), CellularError> {
        async { while crate::at::at(&self.at_client).await.is_err() {} }
            .with_timeout(timeout)
            .await
            .map_err(Into::into)
    }

    pub async fn set_apn(&self, apn: &str) -> Result<(), CellularError> {
        crate::at::packet_domain::set_apn(&self.at_client, apn).await.map_err(Into::into)
    }

    pub async fn read_network_registration(
        &self,
    ) -> Result<(crate::at::network::NetworkRegistrationUrcConfig, crate::at::network::NetworkRegistrationState), CellularError> {
        crate::at::network::get_network_registration(&self.at_client).await.map_err(Into::into)
    }

    pub async fn query_real_time_clock(&self) -> Result<NaiveDateTime, CellularError> {
        crate::at::status_control::query_real_time_clock(&self.at_client).await.map_err(Into::into)
    }

    // AT+CSCLK
    pub async fn read_sleep_mode(&self) -> Result<SleepMode, CellularError> {
        crate::at::serial_interface::read_sleep_mode(&self.at_client).await.map_err(Into::into)
    }

    pub async fn set_sleep_mode(&mut self, mode: SleepMode) -> Result<(), CellularError> {
        if mode != SleepMode::Off && self.mqtt_state != MqttState::Stopped {
            error!("Disconnect MQTT before entering sleep mode");
            return Err(AtError::InvalidState.into());
        }
        if self.http_initialized {
            crate::at::http::term(&self.at_client).await?;
            self.http_initialized = false;
        }
        crate::at::serial_interface::set_sleep_mode(&self.at_client, mode).await.map_err(Into::into)
    }

    pub async fn wake_up(&self) -> Result<(), CellularError> {
        with_timeout(Duration::from_secs(30), async {
            self.is_alive().await;
            while !self.is_alive().await {
                warn!("LTE module not alive, retrying...");
                Timer::after_millis(5).await;
                yield_now().await;
            }
            while self.read_network_registration().await?.1 != crate::at::network::NetworkRegistrationState::Registered {
                warn!("Not registered to network yet, waiting...");
                Timer::after_secs(2).await;
                info!("... retrying ...");
            }
            Ok(())
        })
        .await?
    }

    pub async fn query_signal_quality(&self) -> Result<Rssi, CellularError> {
        crate::at::status_control::query_signal_quality(&self.at_client)
            .await
            .map(|(rssi, _)| rssi)
            .map_err(Into::into)
    }

    pub async fn request(&mut self) -> Result<HttpRequest<'_, '_, Ctr>, CellularError> {
        if !self.http_initialized {
            crate::at::http::init(&self.at_client).await?;
            self.http_initialized = true;
        }
        HttpRequest::new(&self.at_client).await
    }

    /// Upload a trust anchor before connecting. No certificate bytes are logged.
    pub async fn mqtt_upload_ca(&mut self, filename: &str, certificate: &[u8]) -> Result<(), CellularError> {
        self.ensure_mqtt_state(MqttState::Stopped)?;
        // Disable echo before uploading raw certificate data.
        crate::at::mqtt::disable_echo(&self.at_client).await?;
        mqtt::upload_ca(&self.at_client, filename, certificate).await?;
        Ok(())
    }

    /// Start service, acquire client 0 and connect. On error call mqtt_disconnect
    /// to clean up the successfully completed stages before retrying.
    pub async fn mqtt_connect(&mut self, config: &MqttConfig<'_>) -> Result<(), CellularError> {
        self.ensure_mqtt_state(MqttState::Stopped)?;
        mqtt::validate(config)?;
        mqtt::start(&self.at_client).await?;
        self.mqtt_state = MqttState::Started;
        mqtt::acquire(&self.at_client, config).await?;
        self.mqtt_state = MqttState::Acquired;
        mqtt::connect(&self.at_client, config).await?;
        self.mqtt_state = MqttState::Connected;
        Ok(())
    }

    pub async fn mqtt_publish(&mut self, topic: &str, payload: &[u8], qos: MqttQos, retain: bool) -> Result<(), CellularError> {
        self.ensure_mqtt_state(MqttState::Connected)?;
        mqtt::publish(&self.at_client, topic, payload, qos, retain).await?;
        Ok(())
    }

    pub async fn mqtt_subscribe(&mut self, filter: &str, qos: MqttQos) -> Result<(), CellularError> {
        self.ensure_mqtt_state(MqttState::Connected)?;
        mqtt::subscribe(&self.at_client, filter, qos).await?;
        Ok(())
    }

    pub async fn mqtt_unsubscribe(&mut self, filter: &str) -> Result<(), CellularError> {
        self.ensure_mqtt_state(MqttState::Connected)?;
        mqtt::unsubscribe(&self.at_client, filter).await?;
        Ok(())
    }

    /// Receive a complete message, including any queued during another command.
    /// Do not externally cancel AT operations; use this internal timeout instead.
    pub async fn mqtt_receive(&mut self, timeout: Duration) -> Result<MqttMessage, CellularError> {
        self.ensure_mqtt_state(MqttState::Connected)?;
        let result = self.at_client.use_controller(async |ctr| ctr.handle_mqtt_receive(timeout).await).await;
        if matches!(result, Err(AtError::MqttConnectionLost(_))) {
            self.mqtt_state = MqttState::Acquired;
        }
        result.map_err(Into::into)
    }

    /// Explicit asynchronous cleanup is required; dropping the module cannot send AT commands.
    pub async fn mqtt_disconnect(&mut self) -> Result<(), CellularError> {
        if self.mqtt_state == MqttState::Connected {
            mqtt::disconnect(&self.at_client).await?;
            self.mqtt_state = MqttState::Acquired;
        }
        if self.mqtt_state == MqttState::Acquired {
            mqtt::release(&self.at_client).await?;
            self.mqtt_state = MqttState::Started;
        }
        if self.mqtt_state == MqttState::Started {
            mqtt::stop(&self.at_client).await?;
            self.mqtt_state = MqttState::Stopped;
        }
        Ok(())
    }

    fn ensure_mqtt_state(&self, expected: MqttState) -> Result<(), CellularError> {
        if self.mqtt_state != expected {
            error!("MQTT operation is invalid in current state");
            return Err(AtError::InvalidState.into());
        }
        Ok(())
    }
}

pub struct HttpRequest<'m, 'ch, Ctr: AtController> {
    at_client: &'m crate::at::AtClientImpl<'ch, Ctr>,
}

impl<'m, 'ch, Ctr: AtController> HttpRequest<'m, 'ch, Ctr> {
    async fn new(at_client: &'m crate::at::AtClientImpl<'ch, Ctr>) -> Result<Self, CellularError> {
        Ok(Self { at_client })
    }

    pub async fn set_header(&self, header: &str, value: &str) -> Result<&HttpRequest<'m, 'ch, Ctr>, CellularError> {
        crate::at::http::set_header(self.at_client, header, value).await?;
        Ok(self)
    }

    pub async fn get(&self, url: &str) -> Result<HttpResponse<'_, '_, Ctr>, CellularError> {
        crate::at::http::set_url(self.at_client, url).await?;
        crate::at::http::action(self.at_client, crate::at::http::HttpAction::Get)
            .await
            .map_err(Into::into)
            .map(|(status, len)| HttpResponse {
                status,
                body: HttpResponseBody::new(self.at_client, len),
            })
    }

    pub async fn post(&self, url: &str, body: &[u8]) -> Result<HttpResponse<'_, '_, Ctr>, CellularError> {
        crate::at::http::set_url(self.at_client, url).await?;
        self.at_client.use_controller(async |ctr| ctr.handle_http_write(body).await).await?;
        crate::at::http::action(self.at_client, crate::at::http::HttpAction::Post)
            .await
            .map_err(Into::into)
            .map(|(status, len)| HttpResponse {
                status,
                body: HttpResponseBody::new(self.at_client, len),
            })
    }
}

pub struct HttpResponse<'m, 'ch, Ctr: AtController> {
    status: HttpStatusCode,
    body: HttpResponseBody<'m, 'ch, Ctr>,
}

impl<'m, 'ch, Ctr: AtController> HttpResponse<'m, 'ch, Ctr> {
    pub fn status(&self) -> HttpStatusCode {
        self.status
    }

    pub fn body(&mut self) -> &mut HttpResponseBody<'m, 'ch, Ctr> {
        &mut self.body
    }
}

pub struct HttpResponseBody<'m, 'ch, Ctr: AtController> {
    at_client: &'m crate::at::AtClientImpl<'ch, Ctr>,
    len: usize,
    pos: usize,
}

impl<'m, 'ch, Ctr: AtController> HttpResponseBody<'m, 'ch, Ctr> {
    fn new(at_client: &'m crate::at::AtClientImpl<'ch, Ctr>, len: usize) -> Self {
        Self { at_client, len, pos: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub async fn read_to_end(&mut self, mut buf: &mut [u8]) -> Result<usize, CellularError> {
        let mut total_read = 0;
        while !buf.is_empty() {
            match self.read(buf).await {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                    total_read += n;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(total_read)
    }

    pub async fn read_as_str<'a>(&mut self, buf: &'a mut [u8]) -> Result<&'a str, CellularError> {
        let n = self.read_to_end(buf).await?;
        str::from_utf8(&buf[..n]).map_err(|_| {
            error!("http body not utf8");
            CellularError::AtError(crate::at::AtError::Error)
        })
    }
}

impl<'m, 'ch, Ctr: AtController> Read for HttpResponseBody<'m, 'ch, Ctr> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let remaining = self.len - self.pos;
        if remaining == 0 {
            return Ok(0);
        }
        let len = core::cmp::min(remaining, buf.len());
        self.at_client
            .use_controller(async |ctr| ctr.handle_http_read(&mut buf[0..len], self.pos).await)
            .await?;
        self.pos += len;
        Ok(len)
    }
}

impl<'m, 'ch, Ctr: AtController> embedded_io_async::ErrorType for HttpResponseBody<'m, 'ch, Ctr> {
    type Error = CellularError;
}
