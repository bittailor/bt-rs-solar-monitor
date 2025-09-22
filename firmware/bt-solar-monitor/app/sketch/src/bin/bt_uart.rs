//! This example shows how to use UART (Universal asynchronous receiver-transmitter) in the RP235x chip.
//!
//! No specific hardware is specified in this example. If you connect pin 0 and 1 you should get the same data back.
//! The Raspberry Pi Debug Probe (https://www.raspberrypi.com/products/debug-probe/) could be used
//! with its UART port.

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::UART0;
use embassy_rp::uart::{BufferedInterruptHandler, BufferedUart, BufferedUartRx, Config};
use embassy_time::Timer;
use embedded_io_async::{Read, Write};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let (tx_pin, rx_pin, uart) = (p.PIN_0, p.PIN_1, p.UART0);

    static TX_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    let tx_buf = &mut TX_BUF.init([0; 128])[..];
    static RX_BUF: StaticCell<[u8; 128]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0; 128])[..];
    let uart = BufferedUart::new(uart, tx_pin, rx_pin, Irqs, tx_buf, rx_buf, Config::default());
    let (mut tx, rx) = uart.split();

    unwrap!(spawner.spawn(reader(rx)));

    info!("Writing...");
    loop {
        let data = [
            // "Hello uart\n" in asscii plus a new
            0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x20, 0x75, 0x61, 0x72, 0x74, 0x21, 0xa,
        ];
        // info!("TX {:?}", data);
        tx.write_all(&data).await.unwrap();
        Timer::after_secs(2).await;
    }
}

#[embassy_executor::task]
async fn reader(mut rx: BufferedUartRx) {
    info!("Reading...");
    loop {
        let mut line_buffer = heapless::Vec::<u8, 128>::new();
        loop {
            let mut char_buf = [0u8; 1];
            match rx.read(&mut char_buf).await {
                Ok(_) => {
                    if char_buf[0] == b'\n' || char_buf[0] == b'\r' {
                        info!("Got line of {}", line_buffer.len());
                        if !line_buffer.is_empty() {
                            match str::from_utf8(&line_buffer) {
                                Ok(line) => info!("RX: '{}'", line),
                                Err(_) => error!("Invalid UTF-8 sequence"),
                            }
                            line_buffer.clear();
                            break;
                        }
                    } else {
                        line_buffer.push(char_buf[0]).unwrap();
                    }
                }
                Err(e) => warn!("Read error: {}", e),
            };
        }
    }
}
