#![no_std]
#![no_main]

use core::convert::Infallible;

use bt_core::{info, unwrap};
#[cfg(feature = "defmt")]
use defmt_rtt as _;
use ekv::flash::PageID;
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

// MX25L3233F => https://www.macronix.com/Lists/Datasheet/Attachments/8933/MX25L3233F,%203V,%2032Mb,%20v1.7.pdf
// 32 Mbit = 4 MB total, organized as 4KB sectors
const FLASH_SIZE: usize = 4 * 1024 * 1024; // 4 MB
const PAGE_SIZE: usize = 4 * 1024; // 4 KB (sector size, minimum erase unit)
const PAGE_COUNT: usize = FLASH_SIZE / PAGE_SIZE; // 1024 pages
const ALIGN: usize = 4; // QSPI requires 4-byte alignment
const PROGRAM_SIZE: usize = 256; // MX25L3233F page program size

// Flash command opcodes
const CMD_READ_STATUS: u8 = 0x05;
const CMD_WRITE_ENABLE: u8 = 0x06;

// Aligned buffer wrapper for QSPI operations
#[repr(align(4))]
struct AlignedBuffer {
    data: [u8; 512],
}

struct FlashDriver<'a> {
    qspi: qspi::Qspi<'a>,
    // Aligned buffer for QSPI operations when ekv provides unaligned buffers
    aligned_buffer: AlignedBuffer,
}

impl<'a> FlashDriver<'a> {
    fn new(qspi: qspi::Qspi<'a>) -> Self {
        Self {
            qspi,
            aligned_buffer: AlignedBuffer { data: [0u8; 512] },
        }
    }

    /// Check if address and buffer are properly aligned for QSPI
    fn is_aligned(addr: u32, buffer: &[u8]) -> bool {
        let ptr_addr = buffer.as_ptr() as usize;
        addr % ALIGN as u32 == 0 && ptr_addr % ALIGN == 0 && buffer.len() % ALIGN == 0
    }

    /// Round up size to alignment boundary
    fn align_up(size: usize) -> usize {
        (size + ALIGN - 1) / ALIGN * ALIGN
    }

    /// Wait for the flash to be ready (WIP bit cleared)
    async fn wait_ready(&mut self) -> Result<(), Infallible> {
        loop {
            let mut status = [0u8; 1];
            self.qspi.custom_instruction(CMD_READ_STATUS, &[], &mut status).await.unwrap();
            if status[0] & 0x01 == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Enable writes (required before erase/write operations)
    async fn write_enable(&mut self) -> Result<(), Infallible> {
        self.qspi.custom_instruction(CMD_WRITE_ENABLE, &[], &mut []).await.unwrap();
        Ok(())
    }

    /// Perform aligned read using temporary buffer
    async fn read_unaligned(&mut self, addr: u32, data: &mut [u8]) -> Result<(), Infallible> {
        let len = data.len();
        let mut remaining = len;
        let mut offset = 0;

        while remaining > 0 {
            let chunk_size = remaining.min(self.aligned_buffer.data.len());
            let aligned_size = Self::align_up(chunk_size);

            self.qspi
                .read(addr + offset as u32, &mut self.aligned_buffer.data[..aligned_size])
                .await
                .unwrap();
            data[offset..offset + chunk_size].copy_from_slice(&self.aligned_buffer.data[..chunk_size]);

            remaining -= chunk_size;
            offset += chunk_size;
        }
        Ok(())
    }

    /// Perform aligned write using temporary buffer
    async fn write_unaligned(&mut self, addr: u32, data: &[u8]) -> Result<(), Infallible> {
        let aligned_size = Self::align_up(data.len());
        self.aligned_buffer.data[..data.len()].copy_from_slice(data);
        // Pad with 0xFF (erased flash value)
        for i in data.len()..aligned_size {
            self.aligned_buffer.data[i] = 0xFF;
        }
        self.qspi.write(addr, &self.aligned_buffer.data[..aligned_size]).await.unwrap();
        Ok(())
    }
}

impl<'a> ekv::flash::Flash for FlashDriver<'a> {
    type Error = Infallible;

    fn page_count(&self) -> usize {
        PAGE_COUNT
    }

    async fn erase(&mut self, page_id: PageID) -> Result<(), Self::Error> {
        let addr = (page_id.index() * PAGE_SIZE) as u32;

        info!("Erasing page {} at addr 0x{:x}", page_id.index(), addr);

        self.wait_ready().await?;
        self.write_enable().await?;
        self.qspi.erase(addr).await.unwrap();
        self.wait_ready().await?;

        Ok(())
    }

    async fn read(&mut self, page_id: PageID, offset: usize, data: &mut [u8]) -> Result<(), Self::Error> {
        let addr = (page_id.index() * PAGE_SIZE + offset) as u32;

        self.wait_ready().await?;

        if Self::is_aligned(addr, data) {
            self.qspi.read(addr, data).await.unwrap();
        } else {
            self.read_unaligned(addr, data).await?;
        }

        Ok(())
    }

    async fn write(&mut self, page_id: PageID, offset: usize, data: &[u8]) -> Result<(), Self::Error> {
        let addr = (page_id.index() * PAGE_SIZE + offset) as u32;
        let len = data.len();
        let mut offset_in_data = 0;

        while offset_in_data < len {
            let chunk_size = (len - offset_in_data).min(PROGRAM_SIZE);
            let chunk_addr = addr + offset_in_data as u32;
            let chunk = &data[offset_in_data..offset_in_data + chunk_size];

            self.wait_ready().await?;
            self.write_enable().await?;

            if Self::is_aligned(chunk_addr, chunk) {
                self.qspi.write(chunk_addr, chunk).await.unwrap();
            } else {
                self.write_unaligned(chunk_addr, chunk).await?;
            }

            offset_in_data += chunk_size;
        }

        self.wait_ready().await?;
        Ok(())
    }
}

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

    let mut f = FlashDriver::new(q);

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
    loop {}
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
