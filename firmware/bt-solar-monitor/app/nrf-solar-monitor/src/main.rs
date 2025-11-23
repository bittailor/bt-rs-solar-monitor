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
    UARTE1 => buffered_uarte::InterruptHandler<peripherals::UARTE1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);
    let reset = Output::new(p.P0_03, Level::Low, OutputDrive::Standard);
    let pwrkey = Output::new(p.P0_04, Level::Low, OutputDrive::Standard);

    let mut uart_lte_config = uarte::Config::default();
    uart_lte_config.parity = uarte::Parity::EXCLUDED;
    uart_lte_config.baudrate = uarte::Baudrate::BAUD115200;
    let mut uart_lte_tx_buffer = [0u8; 1024];
    let mut uart_lte_rx_buffer = [0u8; 1024];
    let uart_lte = BufferedUarte::new(
        p.UARTE0,
        p.TIMER0,
        p.PPI_CH0,
        p.PPI_CH1,
        p.PPI_GROUP0,
        p.P0_08,
        p.P0_06,
        Irqs,
        uart_lte_config,
        &mut uart_lte_rx_buffer,
        &mut uart_lte_tx_buffer,
    );

    let mut at_state = bt_core::at::State::new();
    let (at_runner, at_client) = bt_core::at::new(&mut at_state, uart_lte);
    let mut lte = CellularModule::new(at_client, pwrkey, reset);

    let mut uart_ve_config = uarte::Config::default();
    uart_ve_config.parity = uarte::Parity::EXCLUDED;
    uart_ve_config.baudrate = uarte::Baudrate::BAUD19200;
    let mut uart_ve_tx_buffer = [0u8; 256];
    let mut uart_ve_rx_buffer = [0u8; 256];
    let uart_ve = BufferedUarte::new(
        p.UARTE1,
        p.TIMER1,
        p.PPI_CH2,
        p.PPI_CH3,
        p.PPI_GROUP1,
        p.P1_10,
        p.P1_08,
        Irqs,
        uart_ve_config,
        &mut uart_ve_rx_buffer,
        &mut uart_ve_tx_buffer,
    );
    let ve_direct_runner = bt_core::sensor::ve_direct::new(uart_ve, embassy_time::Duration::from_secs(60));

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

    join4(at_runner.run(), ve_direct_runner.run(), blinky, sequence).await;
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

    let rtc = lte.query_real_time_clock().await?;
    info!("real time clock: {}", rtc);

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

    loop {
        let rssi = lte.query_signal_quality().await?;
        info!(" -> rssi: {}", rssi);

        let rtc = lte.query_real_time_clock().await?;
        info!("real time clock: {}", rtc);

        Timer::after_secs(10).await;

        info!("Set sleep mode");
        lte.set_sleep_mode(bt_core::at::serial_interface::SleepMode::RxSleep).await?;
        info!("... wait a bit in sleep mode ...");
        Timer::after_secs(30).await;
        while !lte.is_alive().await {
            error!("LTE module not alive, retrying...");
        }
        info!("check network registration again");
        while lte.read_network_registration().await?.1 != bt_core::at::network::NetworkRegistrationState::Registered {
            warn!("Not registered to network yet, waiting...");
            Timer::after_secs(2).await;
            info!("... retrying ...");
        }
    }
}
