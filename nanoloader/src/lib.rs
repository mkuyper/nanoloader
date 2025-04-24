#![no_std]

pub mod lz4;

pub enum PanicKind {
    EXCEPTION,
    BOOTLOADER,
    FIRMWARE,
}

pub trait NanoHal {
    const FW_START: usize;
    const FW_END: usize;
    const FW_SIZE_OFF: usize;

    fn panic(kind:PanicKind, reason:usize, address:usize) -> !;
}

pub fn boot<HAL:NanoHal>() -> ! {
    // SAFETY: It is assumed that the HAL const parameters are valid.
    let fwsize = unsafe { *((HAL::FW_START + HAL::FW_SIZE_OFF) as *const usize) };

    // Check fwsize
    if fwsize > (HAL::FW_END - HAL::FW_START) {
        panic!();
    }

    // Calculate CRC address
    let fwcrc_addr = match HAL::FW_START.checked_add(fwsize) {
        Some(addr) => addr,
        None => panic!()
    };

    // Check that CRC is not past firmware area and verify alignment
    if fwcrc_addr > (HAL::FW_END - size_of::<u32>()) {
        panic!();
    }
    let fwcrc_ptr = fwcrc_addr as *const u32;
    if !fwcrc_ptr.is_aligned() {
        panic!();
    }

    // SAFETY: CRC pointer address and alignment have been checked.
    let fwcrc_exp = unsafe { *fwcrc_ptr };

    // SAFETY: It is assumed that the HAL const parameters are valid, and fwsize has been checked.
    let fwslice = unsafe { core::slice::from_raw_parts(HAL::FW_START as *const u8, fwsize) };

    const CRC32:crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    let fwcrc_act = CRC32.checksum(fwslice);

    if fwcrc_act != fwcrc_exp {
        panic!();
    }

    // SAFETY: Since CRC of firmware is valid, we assume that it is safe to boot into it
    unsafe { cortex_m::asm::bootload(HAL::FW_START as *const u32); }
}
