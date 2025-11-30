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
use embassy_time::{Duration, Timer, with_timeout};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UARTE0 => buffered_uarte::InterruptHandler<peripherals::UARTE0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);

    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD115200;
    let mut tx_buffer = [0u8; 4096];
    let mut rx_buffer = [0u8; 4096];
    let mut uart = BufferedUarte::new(p.UARTE0, p.TIMER0, p.PPI_CH0, p.PPI_CH1, p.PPI_GROUP0, p.P0_08, p.P0_06, Irqs, config, &mut rx_buffer, &mut tx_buffer);

    let blinky = async {
        info!("blinky loop start");
        loop {
            led.set_high();
            Timer::after_millis(250).await;
            led.set_low();
            Timer::after_millis(250).await;
        }
    };

    let uart_task = async {
        match uart.write("Wait for input to echo!\r\n".as_bytes()).await {
            Ok(_) => info!("Write successful"),
            Err(e) => error!("Write failed: {}", e),
        }
        loop {
            let mut buf = [0u8; 1024];
            match with_timeout(Duration::from_secs(20), uart.read(&mut buf)).await {
                Ok(Ok(len)) => {
                    info!("Echo back {} bytes", len);
                    match uart.write(&buf[..len]).await {
                        Ok(_) => info!("Write echo successful"),
                        Err(e) => error!("Write echo failed: {}", e),
                    }
                }
                Ok(Err(e)) => {
                    error!("Uart read failed: {}", e);
                }
                Err(_) => match uart.write("Wait for input to echo!\r\n".as_bytes()).await {
                    Ok(_) => info!("Write successful"),
                    Err(e) => error!("Write failed: {}", e),
                },
            };
        }
    };
    join(blinky, uart_task).await;
}
