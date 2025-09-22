use core::str::{self};

use embassy_time::{Duration, Timer, WithTimeout};
use embedded_hal::digital::OutputPin;
use embedded_io_async::{Read, Write};

use crate::{
    at::{AtController, AtHttpReadRequest, AtHttpWriteRequest, http::HttpStatusCode, serial_interface::SleepMode, status_control::Rssi},
    net::cellular::CellularError,
};

pub struct State<Stream: Read + Write> {
    at_state: crate::at::State<Stream>,
}

impl<Stream: Read + Write> State<Stream> {
    pub fn new() -> Self {
        Self {
            at_state: crate::at::State::new(),
        }
    }
}

impl<Stream: Read + Write> Default for State<Stream> {
    fn default() -> Self {
        Self::new()
    }
}

pub async fn new<'a, Stream: Read + Write, Output: OutputPin>(
    state: &'a mut State<Stream>,
    stream: Stream,
    pwrkey: Output,
    reset: Output,
) -> (CellularModule<'a, Output, crate::at::AtControllerImpl<Stream>>, crate::at::Runner<'a, crate::at::AtControllerImpl<Stream>>) {
    let (runner, at_client) = crate::at::new(&mut state.at_state, stream).await;

    let lte = CellularModule {
        at_client,
        pwrkey,
        reset,
        http_initialized: false,
    };
    (lte, runner)
}

pub struct CellularModule<'ch, Output: OutputPin, Ctr: AtController> {
    at_client: crate::at::AtClientImpl<'ch, Ctr>,
    pwrkey: Output,
    reset: Output,
    http_initialized: bool,
}

impl<Output: OutputPin, Ctr: AtController> CellularModule<'_, Output, Ctr> {
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
        info!("power on ...");
        self.pwrkey.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(50).await;
        self.pwrkey.set_high().map_err(|_| CellularError::GpioError {})?;
        info!("... wait 8s to startup ...");
        Timer::after_secs(8).await;
        info!("... check AT ...");
        self.ensure_at(Duration::from_secs(10)).await?;
        info!("... power on done");
        Ok(())
    }

    pub async fn power_down(&self) -> Result<(), CellularError> {
        crate::at::status_control::power_down(&self.at_client).await?;
        Timer::after_secs(2).await; // Power off time
        Timer::after_secs(2).await; // Power off - power on buffer time
        Ok(())
    }

    pub async fn reset(&mut self) -> Result<(), CellularError> {
        info!("reset ...");
        self.reset.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(2500).await;
        self.reset.set_high().map_err(|_| CellularError::GpioError {})?;
        info!("... wait a bit for module to start ...");
        Timer::after_millis(5000).await;
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

    // AT+CSCLK
    pub async fn read_sleep_mode(&self) -> Result<SleepMode, CellularError> {
        crate::at::serial_interface::read_sleep_mode(&self.at_client).await.map_err(Into::into)
    }

    pub async fn set_sleep_mode(&self, mode: SleepMode) -> Result<(), CellularError> {
        crate::at::serial_interface::set_sleep_mode(&self.at_client, mode).await.map_err(Into::into)
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
}

pub struct HttpRequest<'m, 'ch, Ctr: AtController> {
    at_client: &'m crate::at::AtClientImpl<'ch, Ctr>,
}

impl<'m, 'ch, Ctr: AtController> HttpRequest<'m, 'ch, Ctr> {
    async fn new(at_client: &'m crate::at::AtClientImpl<'ch, Ctr>) -> Result<Self, CellularError> {
        Ok(Self { at_client })
    }

    pub async fn set_url(&self, url: &str) -> Result<(), CellularError> {
        crate::at::http::set_url(self.at_client, url).await.map_err(Into::into)
    }

    pub fn body(&self) -> HttpRequestBody<'_, '_, Ctr> {
        HttpRequestBody::new(self.at_client)
    }

    pub async fn get(&self) -> Result<HttpResponse<'_, '_, Ctr>, CellularError> {
        crate::at::http::action(self.at_client, crate::at::http::HttpAction::Get)
            .await
            .map_err(Into::into)
            .map(|(status, len)| HttpResponse {
                status,
                body: HttpResponseBody::new(self.at_client, len),
            })
    }

    pub async fn post(&self) -> Result<HttpResponse<'_, '_, Ctr>, CellularError> {
        crate::at::http::action(self.at_client, crate::at::http::HttpAction::Post)
            .await
            .map_err(Into::into)
            .map(|(status, len)| HttpResponse {
                status,
                body: HttpResponseBody::new(self.at_client, len),
            })
    }
}

pub struct HttpRequestBody<'m, 'ch, Ctr: AtController> {
    at_client: &'m crate::at::AtClientImpl<'ch, Ctr>,
}

impl<'m, 'ch, Ctr: AtController> HttpRequestBody<'m, 'ch, Ctr> {
    fn new(at_client: &'m crate::at::AtClientImpl<'ch, Ctr>) -> Self {
        Self { at_client }
    }
}

impl<'m, 'ch, Ctr: AtController> Write for HttpRequestBody<'m, 'ch, Ctr> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        AtHttpWriteRequest::new(buf)?.send(self.at_client).await?;
        Ok(buf.len())
    }
}

impl<'m, 'ch, Ctr: AtController> embedded_io_async::ErrorType for HttpRequestBody<'m, 'ch, Ctr> {
    type Error = CellularError;
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
}

impl<'m, 'ch, Ctr: AtController> Read for HttpResponseBody<'m, 'ch, Ctr> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let remaining = self.len - self.pos;
        if remaining == 0 {
            return Ok(0);
        }

        let requested = core::cmp::min(remaining, buf.len());
        let read = core::cmp::min(requested, crate::at::MAX_READ_BUFFER_SIZE);
        let request = AtHttpReadRequest::new(self.pos, read);
        let mut response = request.send(self.at_client).await?;
        let len = response.read(buf)?;
        self.pos += len;
        Ok(len)
    }
}

impl<'m, 'ch, Ctr: AtController> embedded_io_async::ErrorType for HttpResponseBody<'m, 'ch, Ctr> {
    type Error = CellularError;
}
