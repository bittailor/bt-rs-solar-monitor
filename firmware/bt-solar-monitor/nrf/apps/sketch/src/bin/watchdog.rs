#![no_std]
#![no_main]

use core::marker::PhantomData;

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_nrf::{
    bind_interrupts,
    gpio::{Level, Output, OutputDrive},
    interrupt, peripherals,
    wdt::Instance,
};
use embassy_time::{Duration, Timer};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    WDT => WdtInterruptHandler<peripherals::WDT>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);
    let mut red = Output::new(p.P0_13, Level::High, OutputDrive::Standard);
    let mut green = Output::new(p.P0_14, Level::High, OutputDrive::Standard);
    let mut blue = Output::new(p.P0_15, Level::High, OutputDrive::Standard);
    info!("Watchdog sketch started");

    let mut wdt_config = embassy_nrf::wdt::Config::default();
    wdt_config.timeout_ticks = 32768 * 10;
    // This is needed for `probe-rs run` to be able to catch the panic message
    // in the WDT interrupt. The core resets 2 ticks after firing the interrupt.
    wdt_config.action_during_debug_halt = embassy_nrf::wdt::HaltConfig::PAUSE;
    let (mut wdt, [mut handle]) = match embassy_nrf::wdt::Watchdog::try_new(p.WDT, wdt_config) {
        Ok(x) => x,
        Err(_) => {
            info!("Watchdog already active with wrong config, waiting for it to timeout...");
            loop {
                Timer::after_millis(250).await;
            }
        }
    };
    wdt.enable_interrupt();

    let led_blinky = async {
        loop {
            handle.pet();
            led.set_high();
            Timer::after_millis(100).await;
            led.set_low();
            Timer::after_millis(100).await;
        }
    };

    let sequence = async {
        for _ in 0..5 {
            green.set_low();
            Timer::after(Duration::from_millis(250)).await;
            green.set_high();
            Timer::after(Duration::from_millis(250)).await;
        }
        for _ in 0..5 {
            blue.set_low();
            Timer::after(Duration::from_millis(250)).await;
            blue.set_high();
            Timer::after(Duration::from_millis(250)).await;
        }
        red.set_low();

        info!("start busy loop and wait for watchdog");
        #[allow(clippy::empty_loop)]
        loop {}
    };

    join(led_blinky, sequence).await;
}

pub struct WdtInterruptHandler<T: Instance> {
    _phantom: PhantomData<T>,
}

impl<T: Instance> interrupt::typelevel::Handler<T::Interrupt> for WdtInterruptHandler<T> {
    unsafe fn on_interrupt() {
        info!("Watchdog timeout occurred");
    }
}
