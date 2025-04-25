#![no_std]

pub mod lz4;

#[derive(Debug)]
pub enum AbortReason {
    FwSizeInvalid,
    FwCrcMismatch,
}

pub trait NanoHal {
    const FW_START: usize;
    const FW_END: usize;
    const FW_SIZE_OFF: usize;

    fn abort(reason: AbortReason) -> !;
}

pub fn boot<HAL: NanoHal>() -> ! {
    // SAFETY: It is assumed that the HAL const parameters are valid.
    let fwsize = unsafe {
        core::ptr::read_volatile((HAL::FW_START + HAL::FW_SIZE_OFF) as *const usize)
    };

    // Check fwsize
    if fwsize == 0 || fwsize > (HAL::FW_END - HAL::FW_START) {
        HAL::abort(AbortReason::FwSizeInvalid);
    }

    // Calculate CRC address
    let fwcrc_addr = match HAL::FW_START.checked_add(fwsize) {
        Some(addr) => addr,
        None => HAL::abort(AbortReason::FwSizeInvalid)
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
    let fwcrc_exp = unsafe { core::ptr::read_volatile(fwcrc_ptr) };

    // SAFETY: It is assumed that the HAL const parameters are valid, and fwsize has been checked.
    let fwslice = unsafe { core::slice::from_raw_parts(HAL::FW_START as *const u8, fwsize) };

    // Calculate firmware CRC
    const CRC32:crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    let fwcrc_act = CRC32.checksum(fwslice);

    // Check firmware CRC
    if fwcrc_act != fwcrc_exp {
        HAL::abort(AbortReason::FwCrcMismatch);
    }

    // SAFETY: Writing to VTOR is always safe.
    unsafe { (*cortex_m::peripheral::SCB::PTR).vtor.write(HAL::FW_START as u32); }

    // SAFETY: Since CRC of firmware is valid, we assume that it is safe to boot into it.
    unsafe { cortex_m::asm::bootload(HAL::FW_START as *const u32); }
}
