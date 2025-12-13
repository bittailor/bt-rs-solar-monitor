#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    print_memory_usage(1);
    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);

    let mut _buffer = [0u8; 1024];
    let raw = &_buffer as *const _ as usize;
    info!("Stack pointer _buffer: {:x}", raw);

    let stack_val = 1;
    let raw = &stack_val as *const i32 as usize;
    info!("Stack pointer stack_val: {:x}", raw);

    print_memory_usage(1);

    let blinky = async {
        info!("blinky loop start");
        loop {
            led.set_high();
            Timer::after_millis(250).await;
            led.set_low();
            Timer::after_millis(250).await;
        }
    };

    let memory_task = async {
        info!("memory task start");
        loop {
            print_memory_usage(5);
            Timer::after_secs(1).await;
        }
    };
    join(blinky, memory_task).await;
}

fn print_memory_usage(i: i32) {
    if i == 0 {
        return;
    }
    let stack_val = i - 1;
    let raw = &stack_val as *const i32 as usize;
    info!("{} Stack pointer address: {:x}", i, raw);
    print_memory_usage(stack_val);
}
