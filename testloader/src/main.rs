#![no_main]
#![no_std]

use cortex_m_semihosting::debug;
use cortex_m_semihosting::hprintln;
use crc;
use log::{Log, Level, Metadata, Record};
use volatile_register::{RO, RW, WO};

use nanoloader::{NanoHal, NanoReason, NanoResult};

struct Logger{}
impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        hprintln!("[NL] LOG - {}", record.args());
    }

    fn flush(&self) { }
}
static LOGGER: Logger = Logger {};

#[cortex_m_rt::entry]
fn main() -> ! {
    unsafe {
        log::set_logger_racy(&LOGGER).unwrap();
        log::set_max_level_racy(Level::Info.to_level_filter());
    }

    log::info!("hi there!");
    hprintln!("[NL] Starting");

    let hal = TestHal {
        ..Default::default()
    };

    nanoloader::boot::<_>(hal);
}

#[panic_handler]
fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}

#[unsafe(link_section = ".bl_opts")]
#[used]
static BL_OPTS: [u32; 256] = [u32::MAX; 256];

#[derive(Default)]
struct TestHal {
    current_prog_addr: u32,
    current_prog_data: u32,
}

impl TestHal {
    const FLASH: *const FlashController = 0x4000_0000 as *const FlashController;
    const WORD_SZ: pow2::Pow2 = pow2::Pow2::align_of::<u32>();

    fn update_find() -> Option<&'static u32> {
        BL_OPTS
            .iter()
            .find(|x| unsafe { core::ptr::read_volatile(*x) } != 0)
            .filter(|x| unsafe { core::ptr::read_volatile(*x) } != u32::MAX)
    }
}

impl NanoHal for TestHal {
    const FW_START: usize = (16 * 1024);
    const FW_END: usize = (64 * 1024);
    const FW_SIZE_OFF: usize = 0x30;
    const FW_PAGE_SZ: usize = 1024;

    fn abort(reason: NanoReason) -> ! {
        hprintln!("[NL] ABORT - {:?}", reason);
        debug::exit(debug::EXIT_FAILURE);
        // not reached
        loop {}
    }

    fn checksum(data: &[u8]) -> u32 {
        const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        CRC32.checksum(data)
    }

    fn update_address() -> Option<usize> {
        let up = TestHal::update_find().map(|x| *x as usize);

        if let Some(addr) = up {
            hprintln!("[NL] Update found: 0x{:08x}", addr);
        }

        up
    }

    fn update_clear() {
        if let Some(up) = TestHal::update_find() {
            let p = core::ptr::from_ref(up);

            unsafe {
                (*TestHal::FLASH).addr.write(p as u32);
                (*TestHal::FLASH).data.write(0);
                (*TestHal::FLASH).command.write(0x860cd758); // program
            }
            hprintln!("[NL] Update cleared");
        }
    }

    fn program_start(&mut self) -> NanoResult<()> {
        hprintln!("[NL] Programming stated");

        self.current_prog_addr = Self::FW_START as u32;
        self.current_prog_data = 0;

        nanoloader::OK
    }

    fn program_write(&mut self, value: u8) -> NanoResult<()> {
        self.current_prog_data = (self.current_prog_data << 8) | value as u32;

        self.current_prog_addr += 1;

        if Self::WORD_SZ.is_aligned(self.current_prog_addr) {
            let addr = self.current_prog_addr - size_of::<u32>() as u32;

            if pow2::pow2_const!(Self::FW_PAGE_SZ).is_aligned(addr) {
                hprintln!("[NL] Erasing flash page at 0x{:08x}", addr);
                unsafe {
                    (*TestHal::FLASH).addr.write(addr);
                    (*TestHal::FLASH).command.write(0x4c6f315f); // erase
                }
            }
            unsafe {
                (*TestHal::FLASH).addr.write(addr);
                (*TestHal::FLASH)
                    .data
                    .write(self.current_prog_data.swap_bytes());
                (*TestHal::FLASH).command.write(0x860cd758); // program
            }
            self.current_prog_data = 0;
        }
        nanoloader::OK
    }

    fn program_read(&mut self, _offset: usize) -> NanoResult<u8> {
        Err(NanoReason::HalError(0))
    }

    fn program_finish(&mut self) -> NanoResult<()> {
        while !Self::WORD_SZ.is_aligned(self.current_prog_addr) {
            self.program_write(u8::MAX)?;
        }

        hprintln!("[NL] Programming completed");

        nanoloader::OK
    }
}

#[repr(C)]
struct FlashController {
    status: RO<u32>,
    addr: RW<u32>,
    data: RW<u32>,
    command: WO<u32>,
}
