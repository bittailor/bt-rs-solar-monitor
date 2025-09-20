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
        match lte_sequence(&lte, &mut reset).await {
            Ok(_) => info!("LTE commands done"),
            Err(e) => error!("LTE commands error: {:?}", e),
        }
    };

    join3(blinky, lte_runner.run(), sequenc).await;
}

async fn lte_sequence(lte: &bt_core::lte::Lte<'_>, reset: &mut Output<'_>) -> Result<(), LteError> {
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

    while lte.read_network_registration().await?.1 != bt_core::at::network::NetworkRegistrationState::Registered {
        warn!("Not registered to network yet, waiting...");
        Timer::after_secs(2).await;
        info!("... retrying ...");
    }
    info!("network registered!");

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
}
