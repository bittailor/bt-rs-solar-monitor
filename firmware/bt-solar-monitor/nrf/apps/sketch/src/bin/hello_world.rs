#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    info!("Hello, world!");
    let mut counter = 0;
    loop {
        info!("loop : {}", counter);
        counter += 1;
        Timer::after_millis(1000).await;
    }
}
