#![no_std]
#![no_main]

use bt_core::{info, unwrap};
use bt_nrf::driver::qspi_flash::QspiFlashDriver;
#[cfg(feature = "defmt")]
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::rng::Rng;
use embassy_nrf::{bind_interrupts, pac, peripherals, qspi, rng};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Instant;
use heapless::Vec;
use panic_probe as _;
use rand_core::RngCore;

bind_interrupts!(struct Irqs {
    QSPI => qspi::InterruptHandler<peripherals::QSPI>;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    // Enable DC-DC
    pac::POWER.dcdcen().write(|w| w.set_dcdcen(true));

    // Enable flash cache
    pac::NVMC.icachecnf().write(|w| w.set_cacheen(true));

    let p = embassy_nrf::init(Default::default());

    info!("startup");

    // Generate random seed.
    let mut rng = Rng::new(p.RNG, Irqs);
    let random_seed = rng.next_u32();

    // Config for the MX25L3233F (32 Mbit = 4 MB) present in the Particle Xenon board
    // Use READ2O with 8 MHz for reliable initialization
    // Higher frequencies (32 MHz) cause slow startup and wrong ID reads
    let mut config = qspi::Config::default();
    config.read_opcode = qspi::ReadOpcode::READ2O; // Dual output read
    config.write_opcode = qspi::WriteOpcode::PP; // Standard page program
    config.write_page_size = qspi::WritePageSize::_256BYTES;
    config.frequency = qspi::Frequency::M8; // 8 MHz for reliable initialization
    config.capacity = 4 * 1024 * 1024; // 4 MB (32 Mbit)

    /*
    let csn = p.P0_17;
    let sck = p.P0_19;
    let io0 = p.P0_20;
    let io1 = p.P0_21;
    let io2 = p.P0_22;
    let io3 = p.P0_23;
    */
    let mut q: qspi::Qspi = qspi::Qspi::new(p.QSPI, Irqs, p.P0_19, p.P0_17, p.P0_20, p.P0_21, p.P0_22, p.P0_23, config);
    info!("Qspi done");

    info!("read status");
    let mut status = [0; 1];
    unwrap!(q.custom_instruction(0x05, &[], &mut status).await);
    info!("status: {:b}", status);

    let mut id = [1; 3];
    unwrap!(q.custom_instruction(0x9F, &[], &mut id).await);
    info!("id: {:x}", id);

    // Read status register
    let mut status = [4; 1];
    unwrap!(q.custom_instruction(0x05, &[], &mut status).await);

    info!("status: {:?}", status[0]);

    if status[0] & 0x40 == 0 {
        status[0] |= 0x40;

        unwrap!(q.custom_instruction(0x01, &status, &mut []).await);

        info!("enabled quad in status");
    }

    let mut f = QspiFlashDriver::new(q);

    let mut config = ekv::Config::default();
    config.random_seed = random_seed;
    let db = ekv::Database::<_, NoopRawMutex>::new(&mut f, config);

    info!("Formatting...");
    let start = Instant::now();
    unwrap!(db.format().await);
    let ms = Instant::now().duration_since(start).as_millis();
    info!("Done in {} ms!", ms);

    const KEY_COUNT: usize = 100;
    const TX_SIZE: usize = 10;

    info!("Writing {} keys...", KEY_COUNT);
    let start = Instant::now();
    for k in 0..KEY_COUNT / TX_SIZE {
        let mut wtx = db.write_transaction().await;
        for j in 0..TX_SIZE {
            let i = k * TX_SIZE + j;
            let key = make_key(i);
            let val = make_value(i);

            wtx.write(&key, &val).await.unwrap();
        }
        wtx.commit().await.unwrap();
    }
    let ms = Instant::now().duration_since(start).as_millis();
    info!("Done in {} ms! {}ms/key", ms, ms / KEY_COUNT as u64);

    info!("Reading {} keys...", KEY_COUNT);
    let mut buf = [0u8; 32];
    let start = Instant::now();
    for i in 0..KEY_COUNT {
        let key = make_key(i);
        let val = make_value(i);

        let rtx = db.read_transaction().await;
        let n = rtx.read(&key, &mut buf).await.unwrap();
        assert_eq!(&buf[..n], &val[..]);
    }
    let ms = Instant::now().duration_since(start).as_millis();
    info!("Done in {} ms! {}ms/key", ms, ms / KEY_COUNT as u64);

    info!("ALL DONE");
    loop {
        embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
    }
}

fn make_key(i: usize) -> [u8; 2] {
    (i as u16).to_be_bytes()
}

fn make_value(i: usize) -> Vec<u8, 16> {
    let len = (i * 7) % 16;
    let mut v = Vec::new();
    v.resize(len, 0).unwrap();

    let val = i.to_le_bytes();
    let n = val.len().min(len);
    v[..n].copy_from_slice(&val[..n]);
    v
}
