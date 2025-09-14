#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, gpio, peripherals::UART0, uart};
use embassy_time::Timer;
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};

// Program metadata for `picotool info`.
// This isn't needed, but it's recomended to have these minimal entries.
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Blinky Example"),
    embassy_rp::binary_info::rp_program_description!(
        c"This example tests the RP Pico on board LED, connected to gpio 25"
    ),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

bind_interrupts!(pub struct Irqs {
    UART0_IRQ  => uart::InterruptHandler<UART0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);

    let config = uart::Config::default();
    let (uart, tx_pin, tx_dma, rx_pin, rx_dma) = (p.UART0, p.PIN_0, p.DMA_CH0, p.PIN_1, p.DMA_CH1);
    let mut uart = uart::Uart::new(uart, tx_pin, rx_pin, Irqs, tx_dma, rx_dma, config);

    loop {
        info!("led on!");
        led.set_high();
        Timer::after_millis(250).await;

        info!("led off!");
        led.set_low();
        Timer::after_millis(250).await;
        match uart.write("hello there!\r\n".as_bytes()).await {
            Ok(_) => info!("Write successful"),
            Err(e) => error!("Write failed: {}", e),
        }
    }
}
