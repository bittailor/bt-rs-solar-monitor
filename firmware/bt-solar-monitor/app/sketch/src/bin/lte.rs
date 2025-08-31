#![no_std]
#![no_main]

use atat::asynch::AtatClient;
use atat::digest::ParseError;
use atat::{AtatIngress, DefaultDigester, Ingress, ResponseSlot, UrcChannel, asynch::Client};
use bt_core::lte::at::{Urc, http, network, packet_domain};
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{self, BufferedInterruptHandler, BufferedUart};
use embassy_time::Timer;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

const INGRESS_BUF_SIZE: usize = 1024;
const URC_CAPACITY: usize = 128;
const URC_SUBSCRIBERS: usize = 3;

#[embassy_executor::task]
async fn ingress_task(
    mut ingress: Ingress<
        'static,
        LoggingDigester<DefaultDigester<Urc>>,
        Urc,
        INGRESS_BUF_SIZE,
        URC_CAPACITY,
        URC_SUBSCRIBERS,
    >,
    mut reader: uart::BufferedUartRx,
) -> ! {
    ingress.read_from(&mut reader).await
}

struct LoggingDigester<D: atat::Digester> {
    inner: D,
}

impl<D: atat::Digester> atat::Digester for LoggingDigester<D> {
    fn digest<'a>(&mut self, buf: &'a [u8]) -> (atat::DigestResult<'a>, usize) {
        debug!("digest> {=[u8]:a}", buf);
        self.inner.digest(buf)
    }
}

pub fn success_response(resp: &[u8]) -> Result<(&[u8], usize), ParseError> {
    debug!("custom_success> {=[u8]:a}", resp);
    Err(ParseError::NoMatch)
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);
    let mut reset = Output::new(p.PIN_16, Level::High);
    let mut _pwrkey = Output::new(p.PIN_17, Level::High);

    let (tx_pin, rx_pin, uart) = (p.PIN_0, p.PIN_1, p.UART0);

    static TX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let tx_buf = &mut TX_BUF.init([0; 1024])[..];
    static RX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0; 1024])[..];
    let uart: BufferedUart = BufferedUart::new(
        uart,
        tx_pin,
        rx_pin,
        Irqs,
        tx_buf,
        rx_buf,
        uart::Config::default(),
    );
    let (writer, reader) = uart.split();

    static INGRESS_BUF: StaticCell<[u8; INGRESS_BUF_SIZE]> = StaticCell::new();
    static RES_SLOT: ResponseSlot<INGRESS_BUF_SIZE> = ResponseSlot::new();
    static URC_CHANNEL: UrcChannel<Urc, URC_CAPACITY, URC_SUBSCRIBERS> = UrcChannel::new();
    let ingress = Ingress::new(
        LoggingDigester {
            inner: DefaultDigester::<Urc>::default().with_custom_success(success_response),
        },
        INGRESS_BUF.init([0; INGRESS_BUF_SIZE]),
        &RES_SLOT,
        &URC_CHANNEL,
    );
    static BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let mut client = Client::new(
        writer,
        &RES_SLOT,
        BUF.init([0; 1024]),
        atat::Config::default(),
    );

    spawner.spawn(ingress_task(ingress, reader)).unwrap();

    info!("reset ...");
    reset.set_low();
    Timer::after_millis(2500).await;
    reset.set_high();
    info!("... wait a bit for module to start ...");
    Timer::after_millis(2000).await;
    info!("... reset done");

    info!("startup ...");
    while client.send(&bt_core::lte::at::AT).await.is_err() {
        Timer::after_millis(100).await;
        info!("... retrying");
    }

    client
        .send(&packet_domain::SetPDPContextDefinition {
            cid: packet_domain::ContextId(1),
            pdp_type: "IP",
            apn: "gprs.swisscom.ch",
        })
        .await
        .unwrap();

    info!("network registration ...");
    while !client
        .send(&network::GetNetworkRegistrationStatus)
        .await
        .is_ok_and(|resp| {
            info!("NetworkRegistrationStatus: {:?}", resp);
            resp.stat == network::NetworkRegistrationStat::Registered
        })
    {
        Timer::after_millis(100).await;
        info!("... retrying");
    }

    info!("... network registration done");

    info!("requests ...");

    match requests(&mut client, &URC_CHANNEL).await {
        Ok(_) => info!("... requests done"),
        Err(e) => warn!("... requests failed with {}", e),
    };

    loop {
        led.set_high();
        Timer::after_millis(300).await;
        led.set_low();
        Timer::after_millis(300).await;
    }
}

async fn requests(
    client: &mut Client<'_, uart::BufferedUartTx, INGRESS_BUF_SIZE>,
    urc_channel: &UrcChannel<Urc, URC_CAPACITY, URC_SUBSCRIBERS>,
) -> Result<(), atat::Error> {
    let mut subscribtion = urc_channel.subscribe().unwrap();

    client.send(&http::StartHttpService).await?;

    client
        .send(&http::SetHttpParameter {
            parameter: http::HttpParameter::Url("http://api.solar.bockmattli.ch/api/v1/solar"),
        })
        .await?;

    client
        .send(&http::HttpAction {
            method: http::HttpMethod::Get,
        })
        .await?;

    let next = subscribtion.next_message_pure().await;

    let response = match next {
        Urc::DummyIndication(_) => {
            info!("DummyIndication");
            return Ok(());
        }
        Urc::HttpActionResponseIndication(http_action_response) => {
            info!("HttpActionResponseIndication: {:?}", http_action_response);
            http_action_response
        }
        _ => {
            warn!("unexpected urc");
            return Ok(());
        }
    };

    info!(
        "Request status={} length={}",
        response.status_code, response.data_length
    );

    let data_length = client.send(&http::QueryHttpRead {}).await?.data_length;

    info!("data_length {}", data_length);

    client
        .send(&http::HttpRead {
            offset: 0,
            lenght: response.data_length,
        })
        .await?;

    Ok(())
}
