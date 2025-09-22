#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio;
use embassy_time::Timer;
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::task]
async fn mytask(id: u8) {
    for i in 0..=5 {
        info!("task {} - {}", id, i);
        Timer::after_millis(100).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::Low);

    for i in 0..=5 {
        match spawner.spawn(mytask(i)) {
            Ok(_) => debug!("task {} spawned", i),
            Err(e) => error!("task {} spawn error: {}", i, e),
        }
        let id_2 = i + 10;
        match spawner.spawn(mytask(id_2)) {
            Ok(_) => debug!("task {} spawned", id_2),
            Err(e) => error!("task {} spawn error: {}", id_2, e),
        }
        Timer::after_millis(1000).await;
    }

    loop {
        led.set_high();
        Timer::after_millis(250).await;
        led.set_low();
        Timer::after_millis(250).await;
    }
}
