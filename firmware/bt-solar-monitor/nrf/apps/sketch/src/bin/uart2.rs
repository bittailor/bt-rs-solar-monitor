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
use heapless::{String, vec};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UARTE1 => buffered_uarte::InterruptHandler<peripherals::UARTE1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);

    let mut config = uarte::Config::default();
    config.parity = uarte::Parity::EXCLUDED;
    config.baudrate = uarte::Baudrate::BAUD19200;
    let mut tx_buffer = [0u8; 4096];
    let mut rx_buffer = [0u8; 4096];
    let mut uart = BufferedUarte::new(p.UARTE1, p.TIMER1, p.PPI_CH2, p.PPI_CH3, p.PPI_GROUP1, p.P1_10, p.P1_08, Irqs, config, &mut rx_buffer, &mut tx_buffer);

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
        loop {
            read_line(&mut uart).await;
        }
    };
    join(blinky, uart_task).await;
}

async fn read_line(uart: &mut BufferedUarte<'_>) {
    let mut line_buffer = vec::Vec::<u8, 256>::new();
    let mut have_cr = false;
    loop {
        let mut char_buf = [0u8; 1];
        match uart.read(&mut char_buf).await {
            Ok(_) => {
                if char_buf[0] == b'\r' {
                    have_cr = true;
                    continue;
                }
                if char_buf[0] == b'\n' {
                    if !have_cr {
                        warn!("Line feed without preceding carriage return");
                    }
                    have_cr = false;
                    trace!("UART.RX line of lenght {}", line_buffer.len());
                    if !line_buffer.is_empty() {
                        match String::from_utf8(line_buffer) {
                            Ok(line) => {
                                info!("UART.RX> {}", line.as_str());
                            }
                            Err(_) => error!("Invalid UTF-8 sequence"),
                        }
                        return;
                    }
                } else {
                    let _ = line_buffer.push(char_buf[0]);
                }
            }
            Err(_) => {
                error!("Uart read failed");
            }
        };
    }
}
