pub mod cortex_m0;
pub mod generic;

#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub r: bool,
    pub w: bool,
    pub x: bool,
}

#[derive(Debug, Clone)]
pub enum MemoryMapping {
    Mmio {
        base: u32,
        size: u32,
    },
    Direct {
        base: u32,
        ptr: *mut u8,
        size: u32,
        perms: Permissions,
    },
}

pub trait Peripheral {
    fn name(&self) -> &'static str;

    fn mappings(&mut self) -> Vec<MemoryMapping>;

    // TODO - peripheral functions should either take an argument to an object that can interact
    // with the emulator/device (schedule things, set interrupts), or return a more complex
    // "Result" that can convey requests to do such things.

    fn mmio_read(&self, _base: u32, _offset: u32, _size: u32) -> Result<u32, String> {
        Err(String::from("not implemented"))
    }

    fn mmio_write(
        &mut self,
        _base: u32,
        _offset: u32,
        _size: u32,
        _value: u32,
    ) -> Result<(), String> {
        Err(String::from("not implemented"))
    }
}
