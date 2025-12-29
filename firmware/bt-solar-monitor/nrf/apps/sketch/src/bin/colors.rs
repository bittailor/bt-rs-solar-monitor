#![no_std]
#![no_main]

use bt_core::info;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut red = Output::new(p.P0_13, Level::High, OutputDrive::Standard);
    let mut green = Output::new(p.P0_14, Level::High, OutputDrive::Standard);
    let mut blue = Output::new(p.P0_15, Level::High, OutputDrive::Standard);

    info!("Colors sketch started");
    loop {
        info!("red on");
        red.set_low();
        Timer::after_millis(1000).await;
        red.set_high();
        Timer::after_millis(200).await;
        info!("green on");
        green.set_low();
        Timer::after_millis(1000).await;
        green.set_high();
        Timer::after_millis(200).await;
        info!("blue on");
        blue.set_low();
        Timer::after_millis(1000).await;
        blue.set_high();
        Timer::after_millis(200).await;
        info!("red and green on => yellow");
        red.set_low();
        green.set_low();
        Timer::after_millis(1000).await;
        red.set_high();
        green.set_high();
        Timer::after_millis(200).await;
        info!("red and blue on => magenta");
        red.set_low();
        blue.set_low();
        Timer::after_millis(1000).await;
        red.set_high();
        blue.set_high();
        Timer::after_millis(200).await;
        info!("green and blue on => cyan");
        green.set_low();
        blue.set_low();
        Timer::after_millis(1000).await;
        green.set_high();
        blue.set_high();
        Timer::after_millis(200).await;
        info!("all on => white");
        red.set_low();
        green.set_low();
        blue.set_low();
        Timer::after_millis(1000).await;
        red.set_high();
        green.set_high();
        blue.set_high();
        Timer::after_millis(200).await;
    }
}
