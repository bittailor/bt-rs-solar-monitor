//! QSPI Flash driver for MX25L3233F (32Mbit/4MB) flash chip
//!
//! This driver implements the `ekv::flash::Flash` trait for use with the ekv
//! embedded key-value database. It handles alignment requirements for the QSPI
//! peripheral by automatically copying unaligned buffers to an aligned temporary buffer.

use core::convert::Infallible;

use bt_core::info;
use ekv::flash::PageID;
use embassy_nrf::qspi;

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

/// Aligned buffer wrapper for QSPI operations
#[repr(align(4))]
struct AlignedBuffer {
    data: [u8; 512],
}

/// QSPI Flash driver for MX25L3233F
///
/// Implements the `ekv::flash::Flash` trait for the MX25L3233F flash chip.
/// Automatically handles alignment requirements by copying data to/from an
/// aligned buffer when necessary.
pub struct QspiFlashDriver<'a> {
    qspi: qspi::Qspi<'a>,
    /// Aligned buffer for QSPI operations when ekv provides unaligned buffers
    aligned_buffer: AlignedBuffer,
}

impl<'a> QspiFlashDriver<'a> {
    /// Create a new QSPI flash driver
    pub fn new(qspi: qspi::Qspi<'a>) -> Self {
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

impl<'a> ekv::flash::Flash for QspiFlashDriver<'a> {
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
