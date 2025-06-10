#![no_main]
#![no_std]

use mspm0_metapac as device;

use nanoloader::{Ignore, NanoHal, NanoReason, NanoResult};

#[cortex_m_rt::entry]
fn main() -> ! {
    let hal = MspM0CHal::default();

    nanoloader::boot::<_>(hal)
}

#[panic_handler]
fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
    MspM0CHal::abort(HalErr::Panic.into())
}

mod flash_util {
    use super::HalErr;
    use super::device::FLASHCTL;
    use super::device::flashctl::vals::*;
    use nanoloader::{NanoHal, NanoResult};

    pub fn pass_check() -> nanoloader::NanoResult {
        match FLASHCTL.statcmd().read().cmdpass() {
            true => nanoloader::OK,
            false => HalErr::FlashError.into(),
        }
    }

    pub fn blank_verify(addr: *const u64) -> NanoResult<bool> {
        FLASHCTL.cmdtype().write(|w| {
            w.set_command(Command::BLANKVERIFY);
            w.set_size(Size::ONEWORD);
        });
        FLASHCTL.cmdaddr().write(|w| {
            w.set_val(addr as u32);
        });

        flash_exec_wait();
        pass_check()?;

        Ok(!FLASHCTL.statcmd().read().failverify())
    }

    pub fn write_word(addr: *const u64, value: u64) -> NanoResult {
        FLASHCTL.cmdtype().write(|w| {
            w.set_command(Command::PROGRAM);
            w.set_size(Size::ONEWORD);
        });

        FLASHCTL.cmdaddr().write(|w| {
            w.set_val(addr as u32);
        });

        let (lo, hi) = (value as u32, (value >> 32) as u32);
        FLASHCTL.cmddata(0).write(|w| {
            w.set_val(lo);
        });
        FLASHCTL.cmddata(1).write(|w| {
            w.set_val(hi);
        });

        let sector = (addr as u32) / super::MspM0CHal::FW_PAGE_SZ;
        FLASHCTL.cmdweprota().write(|w| {
            w.set_val(!(1 << sector));
        });

        flash_exec_wait();
        pass_check()
    }

    pub fn erase_page(addr: *const u64) -> NanoResult {
        FLASHCTL.cmdtype().write(|w| {
            w.set_command(Command::ERASE);
            w.set_size(Size::SECTOR);
        });

        FLASHCTL.cmdaddr().write(|w| {
            w.set_val(addr as u32);
        });

        let sector = (addr as u32) / super::MspM0CHal::FW_PAGE_SZ;
        FLASHCTL.cmdweprota().write(|w| {
            w.set_val(!(1 << sector));
        });

        flash_exec_wait();
        pass_check()
    }

    #[unsafe(link_section = ".data")]
    #[inline(never)]
    fn flash_exec_wait() {
        FLASHCTL.cmdexec().write(|w| {
            w.set_val(true);
        });

        while !FLASHCTL.statcmd().read().cmddone() {}
    }
}

mod blinker {
    use super::device::GPIOA;

    fn pause<const T: u32>(units: u32) {
        cortex_m::asm::delay(units * T);
    }

    fn led<const IO: usize, const T: u32>(units: u32) {
        GPIOA.doutclr31_0().write(|w| {
            w.set_dio(IO, true);
        });

        pause::<T>(units);

        GPIOA.doutset31_0().write(|w| {
            w.set_dio(IO, true);
        });

        pause::<T>(units);
    }

    fn blink<const IO: usize, const T: u32>(value: u32) {
        let mut value = value;
        loop {
            let nibble = (value & 0x0f) + 1;
            for _ in 0..nibble {
                led::<IO, T>(1);
            }
            pause::<T>(2);

            value >>= 4;

            if value == 0 {
                break;
            }
        }
        pause::<T>(4);
    }

    pub fn pattern<const IO: usize, const T: u32>(values: &[u32]) {
        led::<IO, T>(4);
        for v in values {
            blink::<IO, T>(*v);
        }
    }
}

#[derive(Default)]
struct MspM0CHal {
    prog: FlashProgramming,
}

#[derive(Default)]
struct FlashProgramming {
    address: usize,
    buffer: u64,
    count: u8,
}

impl MspM0CHal {
    const BL_DATA_START: usize = (3 * 1024);

    fn get_bldata<T>() -> &'static [T] {
        // SAFETY: It is assumed that the configured address is valid.
        unsafe {
            core::slice::from_raw_parts(
                Self::BL_DATA_START as *const T,
                usize::from(Self::FW_PAGE_SZ) / size_of::<T>(),
            )
        }
    }

    fn update_find() -> Option<&'static u64> {
        // TODO -- I *think* that on devices which do not implement ECC for the Flash, an erased
        // word will reliably read as all 1 bits (0xffff_ffff_ffff_ffff), and that could be
        // exploited to make the coding of the data page more efficient. Also, TI forum posts
        // appear to imply that BLANKVERIFY will pass for words that have been written to all 1s,
        // at least for production devices. For now, let's believe the datasheet that says
        // differently. This will still allow for 64 updates before the page needs to be erased.

        let mut it = Self::get_bldata::<u64>().iter();
        loop {
            let w1 = it.next()?;
            let w2 = it.next()?;

            return if flash_util::blank_verify(w1).ok()? {
                None
            } else if flash_util::blank_verify(w2).ok()? {
                Some(w1)
            } else {
                continue;
            };

            // TODO -- should errors in blank_verify be handled?
        }
    }

    fn update_clear() {
        if let Some(up) = Self::update_find() {
            let addr: *const u64 = up;
            flash_util::write_word(addr.wrapping_add(1), 0).ignore_result();

            // TODO -- should errors in write_word be handled?
        }
    }

    fn program_add_byte(&mut self, value: u8) {
        self.prog.buffer |= (value as u64) << (self.prog.count * 8);
        self.prog.count += 1;
    }

    fn program_commit_word<const FORCE: bool>(&mut self) -> NanoResult {
        if self.prog.count == 8 || (FORCE && self.prog.count != 0) {
            if Self::FW_PAGE_SZ.is_aligned(self.prog.address) {
                flash_util::erase_page(self.prog.address as *const u64)?;
            }

            flash_util::write_word(self.prog.address as *const u64, self.prog.buffer)?;

            self.prog.address += 8;
            self.prog.buffer = !0;
            self.prog.count = 0;
        }
        nanoloader::OK
    }
}

#[repr(u16)]
enum HalErr {
    Panic,
    NotImplemented,
    FlashError,
}

impl From<HalErr> for NanoReason {
    fn from(item: HalErr) -> Self {
        NanoReason::HalError(item as u16)
    }
}
impl<T> From<HalErr> for NanoResult<T> {
    fn from(item: HalErr) -> Self {
        Err(item.into())
    }
}

impl NanoHal for MspM0CHal {
    const FW_START: usize = (4 * 1024);
    const FW_END: usize = (16 * 1024);
    const FW_SIZE_OFF: usize = 0x30;
    const FW_PAGE_SZ: pow2::Pow2 = pow2::pow2_const!(1024);

    fn abort(reason: NanoReason) -> ! {
        let values = match reason {
            NanoReason::HalError(e) => [0u32, e as u32],
            NanoReason::FwSizeInvalid => [1u32, 0],
            NanoReason::FwCrcMismatch => [1u32, 1],
        };
        for _ in 0..3 {
            blinker::pattern::<22, 6_000_000>(values.as_slice());
        }
        cortex_m::peripheral::SCB::sys_reset();
    }

    fn checksum(data: &[u8]) -> u32 {
        const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        CRC32.checksum(data)
    }

    fn update_address() -> Option<usize> {
        MspM0CHal::update_find().map(|x| *x as usize)
    }

    fn update_clear() {
        MspM0CHal::update_clear()
    }

    fn program_start(&mut self) -> NanoResult {
        self.prog.address = 0;
        self.prog.buffer = !0;
        self.prog.count = 0;

        nanoloader::OK
    }

    fn program_write(&mut self, value: u8) -> NanoResult {
        self.program_add_byte(value);
        self.program_commit_word::<false>()
    }

    fn program_read(&mut self, _offset: usize) -> NanoResult<u8> {
        HalErr::NotImplemented.into()
    }

    fn program_finish(&mut self) -> NanoResult {
        self.program_commit_word::<true>()
    }
}
