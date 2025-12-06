use core::str::{self};

use chrono::NaiveDateTime;
use embassy_futures::yield_now;
use embassy_time::{Duration, Timer, WithTimeout};
use embedded_hal::digital::OutputPin;
use embedded_io_async::Read;

use crate::{
    at::{AtClient, AtController, http::HttpStatusCode, network::NetworkRegistrationState, serial_interface::SleepMode, status_control::Rssi},
    net::cellular::CellularError,
};

pub struct SimComCellularModule<'ch, Output: OutputPin, Ctr: AtController> {
    at_client: crate::at::AtClientImpl<'ch, Ctr>,
    pwrkey: Output,
    reset: Output,
    http_initialized: bool,
}

impl<'ch, Output: OutputPin, Ctr: AtController> SimComCellularModule<'ch, Output, Ctr> {
    pub fn new(at_client: crate::at::AtClientImpl<'ch, Ctr>, pwrkey: Output, reset: Output) -> Self {
        SimComCellularModule {
            at_client,
            pwrkey,
            reset,
            http_initialized: false,
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
        info!("power on ...");
        self.pwrkey.set_low().map_err(|_| CellularError::GpioError {})?;
        Timer::after_millis(50).await;
        self.pwrkey.set_high().map_err(|_| CellularError::GpioError {})?;
        info!("... wait 8s to startup ...");
        Timer::after_secs(8).await;
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

    pub async fn query_real_time_clock(&self) -> Result<NaiveDateTime, CellularError> {
        crate::at::status_control::query_real_time_clock(&self.at_client).await.map_err(Into::into)
    }

    // AT+CSCLK
    pub async fn read_sleep_mode(&self) -> Result<SleepMode, CellularError> {
        crate::at::serial_interface::read_sleep_mode(&self.at_client).await.map_err(Into::into)
    }

    pub async fn set_sleep_mode(&self, mode: SleepMode) -> Result<(), CellularError> {
        crate::at::serial_interface::set_sleep_mode(&self.at_client, mode).await.map_err(Into::into)
    }

    pub async fn wake_up(&self) -> Result<(), CellularError> {
        self.is_alive().await;
        while !self.is_alive().await {
            warn!("LTE module not alive, retrying...");
            yield_now().await;
        }
        while self.read_network_registration().await?.1 != crate::at::network::NetworkRegistrationState::Registered {
            warn!("Not registered to network yet, waiting...");
            Timer::after_secs(2).await;
            info!("... retrying ...");
        }
        Ok(())
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
