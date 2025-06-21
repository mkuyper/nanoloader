#![no_std]

use mspm0_metapac as device;

use nanoloader::{Ignore, NanoHal, NanoReason, NanoResult};

const FLASH_PAGE_SZ: usize = 1024; // should this come from metapac?

pub struct LedSettings {
    pub gpio: usize,
    pub tu_cycles: u32,
}

pub trait NanoBoard {
    const LED: Option<LedSettings>;
}

mod flash_util {
    use super::HalErr;
    use super::device::FLASHCTL;
    use super::device::flashctl::vals::*;
    use nanoloader::NanoResult;

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

    fn unprotect_sector(addr: *const u64) {
        let sector = (addr as u32) / (super::FLASH_PAGE_SZ as u32);
        FLASHCTL.cmdweprota().write(|w| {
            w.set_val(!(1 << sector));
        });
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

        unprotect_sector(addr);
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

        unprotect_sector(addr);
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

struct Blinker {
    pin: usize,
    tu: u32,
}

impl Blinker {
    pub fn new(pin: usize, tu: u32) -> Self {
        Blinker { pin, tu }
    }

    fn pause(&self, units: u32) {
        cortex_m::asm::delay(units * self.tu);
    }

    fn led(&self, units: u32) {
        device::GPIOA.doutclr31_0().write(|w| {
            w.set_dio(self.pin, true);
        });

        self.pause(units);

        device::GPIOA.doutset31_0().write(|w| {
            w.set_dio(self.pin, true);
        });

        self.pause(units);
    }

    fn blink(&self, value: u32) {
        let mut value = value;
        loop {
            let nibble = (value & 0x0f) + 1;
            for _ in 0..nibble {
                self.led(1);
            }
            self.pause(2);

            value >>= 4;

            if value == 0 {
                break;
            }
        }
        self.pause(4);
    }

    pub fn pattern(&self, values: &[u32]) {
        self.led(4);
        for v in values {
            self.blink(*v);
        }
    }
}

pub struct MspM0CHal<B: NanoBoard> {
    prog: FlashProgramming,
    _marker: core::marker::PhantomData<B>,
}

// Manual implementation for Default because #[derive] places a trait bound on the generic
// parameter to PhantomData, which is undesired here.
// https://github.com/rust-lang/rust/issues/26925
impl<B: NanoBoard> Default for MspM0CHal<B> {
    fn default() -> Self {
        Self {
            prog: FlashProgramming::default(),
            _marker: core::marker::PhantomData,
        }
    }
}

#[derive(Default)]
struct FlashProgramming {
    address: usize,
    buffer: u64,
    count: u8,
}

impl<B: NanoBoard> MspM0CHal<B> {
    pub fn boot() -> ! {
        let hal: MspM0CHal<B> = Default::default();

        nanoloader::boot::<_>(hal)
    }
}

impl<B: NanoBoard> MspM0CHal<B> {
    const BL_DATA_START: usize = (3 * 1024);

    pub fn panic(_panic: &core::panic::PanicInfo<'_>) -> ! {
        Self::abort(HalErr::Panic.into())
    }

    fn get_bldata<T>() -> &'static [T] {
        // SAFETY: It is assumed that the configured address is valid.
        unsafe {
            core::slice::from_raw_parts(
                Self::BL_DATA_START as *const T,
                FLASH_PAGE_SZ / size_of::<T>(),
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
            if pow2::pow2_const!(FLASH_PAGE_SZ).is_aligned(self.prog.address) {
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
pub enum HalErr {
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

impl<B: NanoBoard> NanoHal for MspM0CHal<B> {
    const FW_START: usize = (4 * 1024);
    const FW_END: usize = (16 * 1024);
    const FW_SIZE_OFF: usize = 0x30;
    const FW_PAGE_SZ: usize = FLASH_PAGE_SZ;

    fn abort(reason: NanoReason) -> ! {
        if let Some(led) = B::LED {
            let values = match reason {
                NanoReason::HalError(e) => [0u32, e as u32],
                NanoReason::FwSizeInvalid => [1u32, 0],
                NanoReason::FwCrcMismatch => [1u32, 1],
            };
            let blinker = Blinker::new(led.gpio, led.tu_cycles);
            for _ in 0..3 {
                blinker.pattern(values.as_slice());
            }
        }
        cortex_m::peripheral::SCB::sys_reset();
    }

    fn checksum(data: &[u8]) -> u32 {
        const CRC32: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
        CRC32.checksum(data)
    }

    fn update_address() -> Option<usize> {
        MspM0CHal::<B>::update_find().map(|x| *x as usize)
    }

    fn update_clear() {
        MspM0CHal::<B>::update_clear()
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
