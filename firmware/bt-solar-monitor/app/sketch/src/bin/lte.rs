#![no_std]
#![no_main]

use bt_core::at::AtController;
use bt_core::net::cellular::CellularError;
use bt_core::net::cellular::sim_com_a67::CellularModule;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::*;
use embassy_nrf::{
    bind_interrupts,
    buffered_uarte::{self, BufferedUarte},
    gpio::{Level, Output, OutputDrive},
    peripherals, uarte,
};
use embassy_time::Timer;
use embedded_hal::digital::OutputPin;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UARTE0 => buffered_uarte::InterruptHandler<peripherals::UARTE0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);
    let reset = Output::new(p.P0_03, Level::Low, OutputDrive::Standard);
    let pwrkey = Output::new(p.P0_04, Level::Low, OutputDrive::Standard);

    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;
    let mut tx_buffer = [0u8; 4096];
    let mut rx_buffer = [0u8; 4096];
    let uart = BufferedUarte::new(p.UARTE0, p.TIMER0, p.PPI_CH0, p.PPI_CH1, p.PPI_GROUP0, p.P0_08, p.P0_06, Irqs, config, &mut rx_buffer, &mut tx_buffer);

    let mut at_state = bt_core::at::State::new();
    let (at_runner, at_client) = bt_core::at::new(&mut at_state, uart);
    let mut lte = CellularModule::new(at_client, pwrkey, reset);

    let sequence = async {
        match lte_sequence(&mut lte).await {
            Ok(_) => info!("LTE commands done"),
            Err(e) => error!("LTE commands error: {:?}", e),
        }
    };

    let blinky = async {
        loop {
            info!("loop");
            led.set_high();
            Timer::after_millis(1000).await;
            led.set_low();
            Timer::after_millis(1000).await;
        }
    };

    join3(blinky, at_runner.run(), sequence).await;
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

    /*
    let get_request = lte.request().await?;
    let mut get_response = get_request.get("http://api.solar.bockmattli.ch/api/v1/solar").await?;
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
    let mut post_response = post_request
        .post("http://api.solar.bockmattli.ch/api/v1/solar", b"{\"device\":\"test-device\",\"power\":123,\"energy\":456}")
        .await?;
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
    let mut post_response = multi_post_request
        .post("http://api.solar.bockmattli.ch/api/v1/solar", b"<one> <two> <three>")
        .await?;
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
    */

    let mut buf = [0u8; 1024];

    let response = lte
        .request()
        .await?
        .set_header("x-access-token", "1234")
        .await?
        .set_header("bt-token", "hdsjhidqdveu672676")
        .await?
        .get("http://api.solar.bockmattli.ch/api/v1/solar/headers")
        .await?
        .body()
        .read_as_str(&mut buf)
        .await?;
    info!("http get response => '{}'", response);

    let response = lte
        .request()
        .await?
        .post("http://api.solar.bockmattli.ch/api/v1/solar", b"{\"device\":\"test-device\",\"power\":123,\"energy\":456}")
        .await?
        .body()
        .read_as_str(&mut buf)
        .await?;
    info!("http post response: '{}'", response);

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
