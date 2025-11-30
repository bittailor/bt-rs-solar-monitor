#![no_std]
#![no_main]

use bt_core::net::cellular::sim_com_a67::SimComCellularModule;
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
use {defmt_rtt as _, panic_probe as _};

//const CONFIG_SOLAR_SENSOR_AVERAGING_DURATION: embassy_time::Duration = embassy_time::Duration::from_secs(5 * 60);
const CONFIG_SOLAR_SENSOR_AVERAGING_DURATION: embassy_time::Duration = embassy_time::Duration::from_secs(20);

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
    let module = SimComCellularModule::new(at_client, pwrkey, reset);

    let mut uart_ve_config = uarte::Config::default();
    uart_ve_config.parity = uarte::Parity::EXCLUDED;
    uart_ve_config.baudrate = uarte::Baudrate::BAUD19200;
    let mut uart_ve_tx_buffer = [0u8; 1024];
    let mut uart_ve_rx_buffer = [0u8; 1024];
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
    let mut ve_state = bt_core::sensor::ve_direct::State::<8>::new();
    let (ve_direct_runner, _ve_rx) = bt_core::sensor::ve_direct::new(&mut ve_state, uart_ve, CONFIG_SOLAR_SENSOR_AVERAGING_DURATION);

    let cloud_runner = bt_core::net::cloud::new(module);

    /*
    let sequence = async {
        match lte_sequence(&mut lte).await {
            Ok(_) => info!("LTE commands done"),
            Err(e) => error!("LTE commands error: {:?}", e),
        }
    };
    */

    let blinky = async {
        loop {
            info!("loop");
            led.set_high();
            Timer::after_millis(1000).await;
            led.set_low();
            Timer::after_millis(1000).await;
        }
    };

    let ve_sender = async {
        loop {
            let reading = _ve_rx.receive().await;
            info!("VE.Reading> {:?}", reading);
        }
    };

    join5(at_runner.run(), ve_direct_runner.run(), blinky, cloud_runner.run(), ve_sender).await;
}
