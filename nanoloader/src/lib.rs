#![no_std]

pub mod lz4;

#[derive(Debug)]
pub enum AbortReason {
    FwSizeInvalid,
    FwCrcMismatch,
}

pub type NanoResult<T> = Result<T, ()>;

pub trait NanoHal {
    const FW_START: usize;
    const FW_END: usize;
    const FW_SIZE_OFF: usize;

    const FW_PAGE_SZ: pow2::Pow2;

    fn abort(reason: AbortReason) -> !;

    fn checksum(data: &[u8]) -> u32;

    fn update_address() -> Option<usize>;
    fn update_clear();

    fn program_start() -> NanoResult<()>;
    fn program_write(value: u8) -> NanoResult<()>;
    fn program_read(offset: usize) -> NanoResult<u8>;
    fn program_finish() -> NanoResult<()>;
}

pub fn boot<HAL: NanoHal>(_hal: HAL) -> ! {
    process_update::<HAL>();
    start_firmware::<HAL>();
}

fn start_firmware<HAL: NanoHal>() -> ! {
    // SAFETY: It is assumed that the HAL const parameters are valid.
    let fwsize = unsafe { core::ptr::read((HAL::FW_START + HAL::FW_SIZE_OFF) as *const usize) };

    // Check fwsize
    if fwsize == 0 || fwsize > (HAL::FW_END - HAL::FW_START) {
        HAL::abort(AbortReason::FwSizeInvalid);
    }

    // Calculate CRC address
    let fwcrc_addr = match HAL::FW_START.checked_add(fwsize) {
        Some(addr) => addr,
        None => HAL::abort(AbortReason::FwSizeInvalid),
    };

    // Check that CRC is not past firmware area and verify alignment
    if fwcrc_addr > (HAL::FW_END - size_of::<u32>()) {
        HAL::abort(AbortReason::FwSizeInvalid);
    }
    let fwcrc_ptr = fwcrc_addr as *const u32;
    if !fwcrc_ptr.is_aligned() {
        HAL::abort(AbortReason::FwSizeInvalid);
    }

    // SAFETY: CRC pointer address and alignment have been checked.
    let fwcrc_exp = unsafe { core::ptr::read(fwcrc_ptr) };

    // SAFETY: It is assumed that the HAL const parameters are valid, and fwsize has been checked.
    let fwslice = unsafe { core::slice::from_raw_parts(HAL::FW_START as *const u8, fwsize) };

    // Calculate firmware CRC
    let fwcrc_act = HAL::checksum(fwslice);

    // Check firmware CRC
    if fwcrc_act != fwcrc_exp {
        HAL::abort(AbortReason::FwCrcMismatch);
    }

    // SAFETY: Writing to VTOR is always safe.
    unsafe {
        (*cortex_m::peripheral::SCB::PTR)
            .vtor
            .write(HAL::FW_START as u32);
    }

    // SAFETY: Since CRC of firmware is valid, we assume that it is safe to boot into it.
    unsafe {
        cortex_m::asm::bootload(HAL::FW_START as *const u32);
    }
}

#[repr(C)]
struct UpdateInfo {
    /// Update checksum (includes everything except this field)
    checksum: u32,
    /// Update size (in bytes, including this header)
    upsize: u32,
    /// Update type
    uptype: u32,
    /// Firmware size (once unpacked)
    fwsize: u32,
}

impl UpdateInfo {
    const TYPE_PLAIN: u32 = 0;
}

struct Update {
    info: UpdateInfo,
    address: usize,
    data: &'static [u8],
}

fn process_update<HAL: NanoHal>() {
    if let Some(update) = check_update::<HAL>() {
        match update.info.uptype {
            UpdateInfo::TYPE_PLAIN => {
                install_plain::<HAL>(update);
            }
            _ => {
                // unknown or unsupported update type
            }
        }

        // TODO - If there a transient error occured during programming, the update might be
        // recoverable even if the firmware is now in an inconsistent state. If we clear the update
        // pointer unconditionally here, we risk bricking a device that can still be saved.

        HAL::update_clear();
    }
}

#[inline]
#[must_use]
fn ensure(b: bool) -> Option<()> {
    b.then_some(())
}

/// Check if there is a valid update available
fn check_update<HAL: NanoHal>() -> Option<Update> {
    // Ask HAL if a potential update exists
    let upinfo_addr = HAL::update_address()?;

    // Check that update info header is inside firmware area
    ensure(upinfo_addr >= HAL::FW_START)?;
    ensure(upinfo_addr < (HAL::FW_END - size_of::<UpdateInfo>()))?;

    // Verify alignment
    let upinfo_ptr = upinfo_addr as *const UpdateInfo;
    ensure(upinfo_ptr.is_aligned())?;

    // Read the update info header
    // SAFETY: Validity of pointer has just been checked.
    let upinfo = unsafe { core::ptr::read(upinfo_ptr) };

    // Check update size
    ensure(upinfo.upsize as usize >= size_of::<UpdateInfo>())?;
    let update_end = upinfo_addr.checked_add(upinfo.upsize as usize)?;
    ensure(update_end <= HAL::FW_END)?;

    // Create slice for entire update
    // SAFETY: Size of update has just been checked.
    let upslice =
        unsafe { core::slice::from_raw_parts(upinfo_addr as *const u8, upinfo.upsize as usize) };

    let checksum = HAL::checksum(upslice.get(size_of::<u32>()..)?);

    ensure(upinfo.checksum == checksum)?;

    Some(Update {
        info: upinfo,
        address: upinfo_addr,
        data: upslice.get(size_of::<UpdateInfo>()..)?,
    })
}

/// Install a plain update
fn install_plain<HAL: NanoHal>(update: Update) -> Option<()> {
    // Check update size
    ensure(update.info.fwsize as usize == update.data.len())?;
    let size = HAL::FW_PAGE_SZ.align_up(update.info.fwsize)?;
    ensure(HAL::FW_START.checked_add(size as usize)? <= update.address)?;

    // Copy new firmware into place
    HAL::program_start().ok()?;
    for b in update.data {
        HAL::program_write(*b).ok()?;
    }
    HAL::program_finish().ok()?;

    Some(())
}
