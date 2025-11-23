#![no_std]
#![no_main]

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

bind_interrupts!(struct Irqs {
    UARTE1 => buffered_uarte::InterruptHandler<peripherals::UARTE1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);

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
    let ve_direct_runner = bt_core::sensor::ve_direct::new(uart_ve, embassy_time::Duration::from_secs(10));

    let blinky = async {
        loop {
            info!("loop");
            led.set_high();
            Timer::after_millis(1000).await;
            led.set_low();
            Timer::after_millis(1000).await;
        }
    };

    join(ve_direct_runner.run(), blinky).await;
}
