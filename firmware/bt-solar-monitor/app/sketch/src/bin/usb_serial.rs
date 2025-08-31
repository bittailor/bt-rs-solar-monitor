#![no_std]
#![no_main]

//use defmt::*;
use core::fmt::Write;
use embassy_executor::Spawner;
use embassy_futures::join::join3;
use embassy_rp::{
    bind_interrupts, gpio,
    peripherals::USB,
    usb::{Driver, InterruptHandler},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, pipe::Pipe};
use embassy_time::Timer;
use embassy_usb::{
    Builder, Config,
    class::cdc_acm::{CdcAcmClass, State},
};
use gpio::{Level, Output};
use {defmt_rtt as _, panic_probe as _};

// Program metadata for `picotool info`.
// This isn't needed, but it's recomended to have these minimal entries.
#[unsafe(link_section = ".bi_entries")]
#[used]
pub static PICOTOOL_ENTRIES: [embassy_rp::binary_info::EntryAddr; 4] = [
    embassy_rp::binary_info::rp_program_name!(c"Usb Serial"),
    embassy_rp::binary_info::rp_program_description!(c"Bla Bla"),
    embassy_rp::binary_info::rp_cargo_version!(),
    embassy_rp::binary_info::rp_program_build_attribute!(),
];

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let mut led = Output::new(p.PIN_25, Level::High);

    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, Irqs);

    // Create embassy-usb Config
    let mut config = Config::new(0xc0de, 0xcafe);
    config.manufacturer = Some("Bittailor");
    config.product = Some("BT Solar");
    config.serial_number = Some("_BT_SOLAR_");
    config.max_power = 500;
    config.max_packet_size_0 = 64;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut [], // no msos descriptors
        &mut control_buf,
    );

    // Create classes on the builder.
    let class = CdcAcmClass::new(&mut builder, &mut state, 64);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    let (mut usb_tx, mut usb_rx) = class.split();

    let usb_future = async {
        let buffer: Pipe<CriticalSectionRawMutex, 1024> = Pipe::new();
        loop {
            defmt::info!("Wait for USB connection");
            usb_rx.wait_connection().await;
            defmt::info!("Connected");
            let mut counter = 0;
            loop {
                let _ = write!(Writer(&buffer), "USB loop {}\r\n", counter);
                let mut rx: [u8; 1024 as usize] = [0; 1024 as usize];
                let len = buffer.read(&mut rx[..]).await;
                if usb_tx.dtr() && usb_tx.rts() {
                    defmt::info!("DTR and RTS are set, writing to USB");
                    match usb_tx.write_packet(&rx[..len]).await {
                        Ok(_) => {
                            defmt::info!("USB write successful");
                        }
                        Err(e) => {
                            defmt::error!("Failed to write to USB: {:?}", e);
                            break;
                        }
                    }
                }

                /*
                if !usb_tx.rts() {
                    break;
                }
                */
                counter += 1;
                Timer::after_millis(1000).await;
                defmt::info!("USB loop {}", counter);
            }
            defmt::info!("Disconnected");
        }
    };

    let blinky = async {
        loop {
            //defmt::info!("led on!");
            led.set_high();
            Timer::after_millis(250).await;

            //defmt::info!("led off!");
            led.set_low();
            Timer::after_millis(250).await;
        }
    };

    join3(usb_fut, usb_future, blinky).await;
}

/// A writer that writes to the USB logger buffer.
pub struct Writer<'d, const N: usize>(&'d Pipe<CriticalSectionRawMutex, N>);

impl<'d, const N: usize> core::fmt::Write for Writer<'d, N> {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        // The Pipe is implemented in such way that we cannot
        // write across the wraparound discontinuity.
        let b = s.as_bytes();
        if let Ok(n) = self.0.try_write(b) {
            if n < b.len() {
                // We wrote some data but not all, attempt again
                // as the reason might be a wraparound in the
                // ring buffer, which resolves on second attempt.
                let _ = self.0.try_write(&b[n..]);
            }
        }
        Ok(())
    }
}
