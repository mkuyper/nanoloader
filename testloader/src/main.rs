#![no_main]
#![no_std]

use cortex_m_semihosting::debug;
use cortex_m_semihosting::hprintln;
use crc;
use volatile_register::{RO, RW, WO};

use nanoloader::{NanoHal, NanoResult};

#[cortex_m_rt::entry]
fn main() -> ! {
    hprintln!("[NL] Starting");

    let hal = TestHal {};

    nanoloader::boot::<_>(hal);
}

#[panic_handler]
fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}

#[unsafe(link_section = ".bl_opts")]
#[used]
static BL_OPTS: [u32; 256] = [u32::MAX; 256];

struct TestHal {}

impl TestHal {
    const FLASH: *const FlashController = 0x4000_0000 as *const FlashController;

    fn update_find() -> Option<&'static u32> {
        BL_OPTS
            .iter()
            .find(|x| **x != 0)
            .filter(|x| **x != u32::MAX)
    }
}

impl NanoHal for TestHal {
    const FW_START: usize = (16 * 1024);
    const FW_END: usize = (64 * 1024) - TestHal::FW_START;
    const FW_SIZE_OFF: usize = 0x30;
    const FW_PAGE_SZ: pow2::Pow2 = pow2::pow2_const!(1024);

    fn abort(reason: nanoloader::AbortReason) -> ! {
        hprintln!("[NL] ABORT - {:?}", reason);
        debug::exit(debug::EXIT_FAILURE);
        // not reached
        loop {}
    }

    fn checksum(data: &[u8]) -> u32 {
        const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

        let cs = CRC32.checksum(data);

        hprintln!("[NL] checksum: 0x{:08x}", cs);

        cs
    }

    fn update_address() -> Option<usize> {
        let up = TestHal::update_find().map(|x| *x as usize);

        hprintln!("[NL] update: {:?}", up);

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
        }
    }

    fn program_start() -> NanoResult<()> {
        Err(())
    }

    fn program_write(_value: u8) -> NanoResult<()> {
        Err(())
    }

    fn program_read(_offset: usize) -> NanoResult<u8> {
        Err(())
    }

    fn program_finish() -> NanoResult<()> {
        Err(())
    }
}

#[repr(C)]
struct FlashController {
    status: RO<u32>,
    addr: RW<u32>,
    data: RW<u32>,
    command: WO<u32>,
}
