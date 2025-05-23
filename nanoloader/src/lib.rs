#![no_std]

pub mod lz4;

#[derive(Debug)]
pub enum NanoReason {
    HalError(u16),
    FwSizeInvalid,
    FwCrcMismatch,
}

pub type NanoResult<T = ()> = Result<T, NanoReason>;

pub trait NanoHal {
    const FW_START: usize;
    const FW_END: usize;
    const FW_SIZE_OFF: usize;

    const FW_PAGE_SZ: pow2::Pow2;

    fn abort(reason: NanoReason) -> !;

    fn checksum(data: &[u8]) -> u32;

    fn update_address() -> Option<usize>;
    fn update_clear();

    fn program_start(&mut self) -> NanoResult;
    fn program_write(&mut self, value: u8) -> NanoResult;
    fn program_read(&mut self, offset: usize) -> NanoResult<u8>;
    fn program_finish(&mut self) -> NanoResult;
}

pub fn boot<HAL: NanoHal>(mut hal: HAL) -> ! {
    // Process any pending update
    process_update::<HAL>(&mut hal);

    // Verify firmware is valid
    check_firmware::<HAL>().unwrap_or_else(|e| HAL::abort(e));

    // SAFETY: Since firmware is valid, we can assume that it is safe to boot into it
    unsafe {
        // Set VTOR to start of firmware (always safe on Cortex-M)
        (*cortex_m::peripheral::SCB::PTR)
            .vtor
            .write(HAL::FW_START as u32);

        // 3 .. 2 .. 1 .. lift-off!
        cortex_m::asm::bootload(HAL::FW_START as *const u32);
    }
}

#[inline]
#[must_use]
fn ensure(b: bool) -> Option<()> {
    b.then_some(())
}

fn read_checked<T: Copy>(slice: &[u8], offset: usize) -> Option<T> {
    slice
        .get(offset..offset.checked_add(size_of::<T>())?)
        .map(|s| s.as_ptr() as *const T)
        .and_then(|ptr| ptr.is_aligned().then_some(ptr))
        // SAFETY: Pointer range and alignment just verified
        .map(|ptr| unsafe { core::ptr::read(ptr) })
}

fn get_fwarea<HAL: NanoHal>() -> &'static [u8] {
    // SAFETY: It is assumed that the HAL's const parameters are valid.
    unsafe { core::slice::from_raw_parts(HAL::FW_START as *const u8, HAL::FW_END - HAL::FW_START) }
}

fn check_firmware<HAL: NanoHal>() -> NanoResult {
    let fwarea = get_fwarea::<HAL>();

    // Read firmware size
    let fwsize =
        read_checked::<usize>(fwarea, HAL::FW_SIZE_OFF).ok_or(NanoReason::FwSizeInvalid)?;

    // Split firmware area from rest
    let (firmware, rest) = fwarea
        .split_at_checked(fwsize)
        .ok_or(NanoReason::FwSizeInvalid)?;

    // Read expected firmware CRC
    let fwcrc_exp = read_checked::<u32>(rest, 0).ok_or(NanoReason::FwSizeInvalid)?;

    // Calculate firmware CRC
    let fwcrc_act = HAL::checksum(firmware);

    // Check firmware CRC
    ensure(fwcrc_act == fwcrc_exp).ok_or(NanoReason::FwCrcMismatch)?;

    Ok(())
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
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

fn process_update<HAL: NanoHal>(hal: &mut HAL) {
    if let Some(update) = check_update::<HAL>() {
        match update.info.uptype {
            UpdateInfo::TYPE_PLAIN => {
                install_plain::<HAL>(hal, update);
            }
            _ => {
                // unknown or unsupported update type
            }
        }

        // If a transient error occured during programming, the update might be recoverable even if
        // the firmware is now in an inconsistent state. Unconditionally clearing the update
        // pointer here would risk bricking a device that can still be saved. It is safer to only
        // clear the update if there is a valid firmware in Flash.

        if check_firmware::<HAL>().is_ok() {
            HAL::update_clear();
        }
    }
}

/// Check if there is a valid update available
fn check_update<HAL: NanoHal>() -> Option<Update> {
    // Ask HAL if a potential update exists
    let upinfo_addr = HAL::update_address()?;

    // Calculate offset of update into firmware area
    let upinfo_off = upinfo_addr.checked_sub(HAL::FW_START)?;

    let fwarea = get_fwarea::<HAL>();

    // Read the update info header
    let upinfo = read_checked::<UpdateInfo>(fwarea, upinfo_off)?;

    // Create slice for entire update
    let update_end = upinfo_off.checked_add(upinfo.upsize as usize)?;
    let upslice = fwarea.get(upinfo_off..update_end)?;

    let checksum = HAL::checksum(upslice.get(size_of::<u32>()..)?);

    ensure(upinfo.checksum == checksum)?;

    Some(Update {
        info: upinfo,
        address: upinfo_addr,
        data: upslice.get(size_of::<UpdateInfo>()..)?,
    })
}

/// Install a plain update
fn install_plain<HAL: NanoHal>(hal: &mut HAL, update: Update) -> Option<()> {
    // Check update size
    ensure(update.info.fwsize as usize == update.data.len())?;
    let size = HAL::FW_PAGE_SZ.align_up(update.info.fwsize)?;
    ensure(HAL::FW_START.checked_add(size as usize)? <= update.address)?;

    // Copy new firmware into place
    hal.program_start().ok()?;
    for b in update.data {
        hal.program_write(*b).ok()?;
    }
    hal.program_finish().ok()?;

    Some(())
}
