#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join4;
use embassy_nrf::{
    gpio::{Level, Output, OutputDrive},
    wdt::HaltConfig,
};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);
    let mut red = Output::new(p.P0_13, Level::Low, OutputDrive::Standard);
    let mut green = Output::new(p.P0_14, Level::Low, OutputDrive::Standard);
    let mut blue = Output::new(p.P0_15, Level::Low, OutputDrive::Standard);
    info!("Watchdog sketch started");

    let mut wdt_config = embassy_nrf::wdt::Config::default();
    wdt_config.timeout_ticks = 32768 * 10;
    // This is needed for `probe-rs run` to be able to catch the panic message
    // in the WDT interrupt. The core resets 2 ticks after firing the interrupt.
    wdt_config.action_during_debug_halt = HaltConfig::PAUSE;

    let (_wdt, [mut handle]) = match embassy_nrf::wdt::Watchdog::try_new(p.WDT, wdt_config) {
        Ok(x) => x,
        Err(_) => {
            info!("Watchdog already active with wrong config, waiting for it to timeout...");
            loop {
                Timer::after_millis(250).await;
            }
        }
    };

    let led_blinky = async {
        loop {
            handle.pet();
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
        let mut counter = 0;
        loop {
            blue.set_high();
            Timer::after_millis(100).await;
            blue.set_low();
            Timer::after_millis(100).await;
            counter += 1;
            if counter > 50 {
                info!("Simulating panic!");
                crate::panic!("Simulated panic for watchdog test");
            }
        }
    };

    join4(led_blinky, red_blinky, green_blinky, blue_blinky).await;
}
