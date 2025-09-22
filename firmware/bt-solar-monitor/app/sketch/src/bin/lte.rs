#![no_std]
#![no_main]

use bt_core::at::AtController;
use bt_core::net::cellular::CellularError;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::*;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{self, BufferedInterruptHandler, BufferedUart};
use embassy_time::Timer;
use embedded_hal::digital::OutputPin;
use embedded_io_async::{Read, Write};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);
    let reset = Output::new(p.PIN_16, Level::High);
    let pwrkey = Output::new(p.PIN_17, Level::High);

    let (tx_pin, rx_pin, uart) = (p.PIN_0, p.PIN_1, p.UART0);

    static TX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let tx_buf = &mut TX_BUF.init([0; 1024])[..];
    static RX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0; 1024])[..];
    let uart: BufferedUart = BufferedUart::new(uart, tx_pin, rx_pin, Irqs, tx_buf, rx_buf, uart::Config::default());

    let mut lte_state = bt_core::net::cellular::sim_com_a67::State::new();
    let (mut lte, lte_runner) = bt_core::net::cellular::sim_com_a67::new(&mut lte_state, uart, pwrkey, reset).await;

    let blinky = async {
        loop {
            info!("loop");
            led.set_high();
            Timer::after_millis(1000).await;
            led.set_low();
            Timer::after_millis(1000).await;
        }
    };

    let sequenc = async {
        match lte_sequence(&mut lte).await {
            Ok(_) => info!("LTE commands done"),
            Err(e) => error!("LTE commands error: {:?}", e),
        }
    };

    join3(blinky, lte_runner.run(), sequenc).await;
}

async fn lte_sequence(lte: &mut bt_core::net::cellular::sim_com_a67::CellularModule<'_, impl OutputPin, impl AtController>) -> Result<(), CellularError> {
    info!("start LTE sequence");

    lte.power_cycle().await?;

    lte.set_apn("gprs.swisscom.ch").await?;

    while lte.read_network_registration().await?.1 != bt_core::at::network::NetworkRegistrationState::Registered {
        warn!("Not registered to network yet, waiting...");
        Timer::after_secs(2).await;
        info!("... retrying ...");
    }
    info!("network registered!");

    let get_request = lte.request().await?;
    get_request.set_url("http://api.solar.bockmattli.ch/api/v1/solar").await?;
    let mut get_response = get_request.get().await?;
    info!("http get done: status={:?} len={}", get_response.status(), get_response.body().len());
    if get_response.status().is_ok() {
        let mut buf = [0u8; 1024];
        loop {
            let n = get_response.body().read(&mut buf).await?;
            if n == 0 {
                break;
            }
            info!("read {} bytes", n);
        }
        let response = core::str::from_utf8(&buf).map_err(|_| {
            error!("http get body not utf8");
            CellularError::AtError(bt_core::at::AtError::Error)
        })?;
        info!("http get body: '{}'", response);
    } else {
        error!("http get failed: code={}", get_response.status());
    }

    let post_request = lte.request().await?;
    post_request.set_url("http://api.solar.bockmattli.ch/api/v1/solar").await?;
    post_request
        .body()
        .write_all(b"{\"device\":\"test-device\",\"power\":123,\"energy\":456}")
        .await?;
    let mut post_response = post_request.post().await?;
    info!("http post done: status={:?} len={}", post_response.status(), post_response.body().len());
    if post_response.status().is_ok() {
        let mut buf = [0u8; 1024];
        loop {
            let n = post_response.body().read(&mut buf).await?;
            if n == 0 {
                break;
            }
            info!("read {} bytes", n);
        }
        let response = core::str::from_utf8(&buf).map_err(|_| {
            error!("http post body not utf8");
            CellularError::AtError(bt_core::at::AtError::Error)
        })?;
        info!("http post body: '{}'", response);
    } else {
        error!("http post failed: code={}", post_response.status());
    }

    let multi_post_request = lte.request().await?;
    multi_post_request.set_url("http://api.solar.bockmattli.ch/api/v1/solar").await?;
    multi_post_request.body().write_all(b"<one>").await?;
    multi_post_request.body().write_all(b"<two>").await?;
    multi_post_request.body().write_all(b"<three>").await?;
    let mut post_response = multi_post_request.post().await?;
    info!("http post done: status={:?} len={}", post_response.status(), post_response.body().len());
    if post_response.status().is_ok() {
        let mut buf = [0u8; 1024];
        loop {
            let n = post_response.body().read(&mut buf).await?;
            if n == 0 {
                break;
            }
            info!("read {} bytes", n);
        }
        let response = core::str::from_utf8(&buf).map_err(|_| {
            error!("http post body not utf8");
            CellularError::AtError(bt_core::at::AtError::Error)
        })?;
        info!("http post body: '{}'", response);
    } else {
        error!("http post failed: code={}", post_response.status());
    }

    info!("wait 10s before power off ...");
    Timer::after_secs(10).await;
    info!("... power off ...");
    lte.power_down().await?;
    info!("... power off done");

    Ok(())

    /*
    loop {
        let rssi = lte.query_signal_quality().await?;
        info!(" -> rssi: {}", rssi);
        Timer::after_secs(10).await;

        info!("Set sleep mode");
        lte.set_sleep_mode(bt_core::at::serial_interface::SleepMode::RxSleep).await?;
        info!("... wait a bit in sleep mode ...");
        Timer::after_secs(30).await;
        while lte.at().await.is_err() {
            error!("LTE module not responding to AT command, retrying...");
        }
        info!("check network registration again");
        while lte.read_network_registration().await?.1 != bt_core::at::network::NetworkRegistrationState::Registered {
            warn!("Not registered to network yet, waiting...");
            Timer::after_secs(2).await;
            info!("... retrying ...");
        }
    }
    */
}
