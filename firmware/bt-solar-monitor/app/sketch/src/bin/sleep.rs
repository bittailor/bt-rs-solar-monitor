#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};

fn busy_cpu_loop(duration: Duration) {
    let start = embassy_time::Instant::now();
    while embassy_time::Instant::now() - start < duration {
        core::hint::spin_loop();
    }
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    loop {
        Timer::after_millis(1000).await;
        info!("Start");
        busy_cpu_loop(Duration::from_millis(500));
        info!("Stop");
    }
}
