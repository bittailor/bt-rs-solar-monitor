#![no_std]
#![no_main]

use bt_core::{info, net::cellular::sim_com_a67::SimComCellularModule};
use embassy_executor::Spawner;
use embassy_futures::join::*;
use embassy_nrf::{
    bind_interrupts,
    buffered_uarte::{self, BufferedUarte},
    gpio::{Input, Level, Output, OutputDrive, Pull},
    peripherals,
    uarte::{self, Uarte},
};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

const CONFIG_SOLAR_SENSOR_AVERAGING_DURATION: embassy_time::Duration = embassy_time::Duration::from_secs(5 * 60);
//const CONFIG_SOLAR_SENSOR_AVERAGING_DURATION: embassy_time::Duration = embassy_time::Duration::from_secs(20);

bind_interrupts!(struct Irqs {
    UARTE0 => buffered_uarte::InterruptHandler<peripherals::UARTE0>;
    UARTE1 => uarte::InterruptHandler<peripherals::UARTE1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    info!("nRF Solar Monitor starting up...");
    info!("Using backend URL: {}", bt_core::config::SOLAR_BACKEND_BASE_URL);
    info!("Using averaging duration: {}", CONFIG_SOLAR_SENSOR_AVERAGING_DURATION.as_secs());

    let mut led = Output::new(p.P1_12, Level::Low, OutputDrive::Standard);

    let mut red = Output::new(p.P0_13, Level::High, OutputDrive::Standard);
    let green = Output::new(p.P0_14, Level::High, OutputDrive::Standard);
    let mut blue = Output::new(p.P0_15, Level::Low, OutputDrive::Standard);

    let reset = Output::new(p.P0_03, Level::Low, OutputDrive::Standard);
    let pwrkey = Output::new(p.P0_04, Level::Low, OutputDrive::Standard);
    let mut netlight = Input::new(p.P0_28, Pull::None);

    let mut uart_lte_config = uarte::Config::default();
    uart_lte_config.parity = uarte::Parity::EXCLUDED;
    uart_lte_config.baudrate = uarte::Baudrate::BAUD115200;
    let mut uart_lte_tx_buffer = [0u8; 2048];
    let mut uart_lte_rx_buffer = [0u8; 2048];
    let uart_lte = BufferedUarte::new(
        p.UARTE0,
        p.TIMER0,
        p.PPI_CH0,
        p.PPI_CH1,
        p.PPI_GROUP0,
        p.P0_08,
        p.P0_06,
        Irqs,
        uart_lte_config,
        &mut uart_lte_rx_buffer,
        &mut uart_lte_tx_buffer,
    );

    let mut at_state = bt_core::at::State::new();
    let (at_runner, at_client) = bt_core::at::new(&mut at_state, uart_lte);
    let module = SimComCellularModule::new(at_client, pwrkey, reset);

    let mut uart_ve_config = uarte::Config::default();
    uart_ve_config.parity = uarte::Parity::EXCLUDED;
    uart_ve_config.baudrate = uarte::Baudrate::BAUD19200;
    let uart_ve = UartWrapper(Uarte::new(p.UARTE1, p.P1_10, p.P1_08, Irqs, uart_ve_config));

    let mut ve_state = bt_core::sensor::ve_direct::State::<8>::new();
    let (ve_direct_runner, ve_rx) = bt_core::sensor::ve_direct::new(&mut ve_state, uart_ve, CONFIG_SOLAR_SENSOR_AVERAGING_DURATION, green);
    let upload_channel = embassy_sync::channel::Channel::<embassy_sync::blocking_mutex::raw::NoopRawMutex, _, 4>::new();
    let solar_runner = bt_core::solar_monitor::upload::new(ve_rx, upload_channel.sender());
    let cloud_runner = bt_core::solar_monitor::cloud::new(module, upload_channel.receiver());

    let mut wdt_config = embassy_nrf::wdt::Config::default();
    wdt_config.timeout_ticks = 32768 * 10; // 10 seconds
    wdt_config.action_during_debug_halt = embassy_nrf::wdt::HaltConfig::PAUSE;
    let (_watchdog, [mut watchdog_handle]) = match embassy_nrf::wdt::Watchdog::try_new(p.WDT, wdt_config) {
        Ok(x) => x,
        Err(_) => {
            info!("Watchdog already active with wrong config, waiting for it to timeout...");
            red.set_low();
            loop {
                Timer::after_millis(250).await;
            }
        }
    };

    Timer::after_millis(100).await;
    info!("nRF Solar Monitor starting up...");
    blue.set_high();

    let blinky = async {
        loop {
            watchdog_handle.pet();
            led.set_high();
            Timer::after_millis(100).await;
            led.set_low();
            Timer::after_millis(900).await;
        }
    };

    let mut follow = |netlight: &Input<'_>| {
        let level = if netlight.is_high() { Level::Low } else { Level::High };
        blue.set_level(level);
        red.set_level(level);
    };

    let netlight_loop = async {
        follow(&netlight);
        loop {
            netlight.wait_for_any_edge().await;
            follow(&netlight);
        }
    };

    join(join(blinky, netlight_loop), join4(at_runner.run(), ve_direct_runner.run(), cloud_runner.run(), solar_runner.run())).await;
}

struct UartWrapper<'d>(Uarte<'d>);

impl embedded_io::ErrorType for UartWrapper<'_> {
    type Error = embassy_nrf::uarte::Error;
}

impl embedded_io_async::Read for UartWrapper<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0.read(buf).await?;
        Ok(buf.len())
    }
}

impl embedded_io_async::Write for UartWrapper<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        embedded_io_async::Write::write(&mut self.0, buf).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        embedded_io_async::Write::flush(&mut self.0).await
    }
}
