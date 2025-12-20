#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_futures::join::join4;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);
    let mut red = Output::new(p.P0_13, Level::Low, OutputDrive::Standard);
    let mut green = Output::new(p.P0_14, Level::Low, OutputDrive::Standard);
    let mut blue = Output::new(p.P0_15, Level::Low, OutputDrive::Standard);

    let led_blinky = async {
        loop {
            led.set_high();
            Timer::after_millis(250).await;
            led.set_low();
            Timer::after_millis(250).await;
        }
    };

    let red_blinky = async {
        loop {
            red.set_high();
            Timer::after_millis(200).await;
            red.set_low();
            Timer::after_millis(200).await;
        }
    };

    let green_blinky = async {
        loop {
            green.set_high();
            Timer::after_millis(150).await;
            green.set_low();
            Timer::after_millis(150).await;
        }
    };

    let blue_blinky = async {
        loop {
            blue.set_high();
            Timer::after_millis(100).await;
            blue.set_low();
            Timer::after_millis(100).await;
        }
    };

    join4(led_blinky, red_blinky, green_blinky, blue_blinky).await;
}
