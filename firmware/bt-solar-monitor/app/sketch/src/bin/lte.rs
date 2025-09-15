#![no_std]
#![no_main]

use bt_core::lte::LteError;
use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::*;
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

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);
    let mut reset = Output::new(p.PIN_16, Level::High);
    let mut _pwrkey = Output::new(p.PIN_17, Level::High);

    let (tx_pin, rx_pin, uart) = (p.PIN_0, p.PIN_1, p.UART0);

    static TX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let tx_buf = &mut TX_BUF.init([0; 1024])[..];
    static RX_BUF: StaticCell<[u8; 1024]> = StaticCell::new();
    let rx_buf = &mut RX_BUF.init([0; 1024])[..];
    let uart: BufferedUart = BufferedUart::new(uart, tx_pin, rx_pin, Irqs, tx_buf, rx_buf, uart::Config::default());

    let mut lte_state = bt_core::lte::State::new();
    let (lte, lte_runner) = bt_core::lte::new_lte(&mut lte_state, uart).await;

    /*
    let lte: bt_core::lte::Runner<_, ThreadModeRawMutex, 32> = bt_core::lte::Runner::new(uart, channel.receiver());
    let sender = channel.sender();
    */

    let blinky = async {
        loop {
            info!("loop");
            led.set_high();
            Timer::after_millis(300).await;
            led.set_low();
            Timer::after_millis(300).await;
        }
    };
    let commands = async {
        info!("reset ...");
        reset.set_low();
        Timer::after_millis(2500).await;
        reset.set_high();
        info!("... wait a bit for module to start ...");
        Timer::after_millis(5000).await;
        info!("... reset done");

        while lte.at().await.is_err() {
            error!("LTE module not responding to AT command, retrying...");
            Timer::after_secs(2).await;
        }
        lte.set_apn("gprs.swisscom.ch").await?;

        while lte.read_network_registration().await?.1 != bt_core::lte::at::NetworkRegistrationState::Registered {
            warn!("Not registered to network yet, waiting...");
            Timer::after_secs(2).await;
        }

        Ok::<(), LteError>(())
    };

    join3(blinky, lte_runner.run(), commands).await;
    //join(blinky, commands).await;
}
