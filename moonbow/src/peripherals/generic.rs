use super::*;

use pow2::Pow2;

use moonbow_macros::Peripheral;

pub struct Sram {
    name: &'static str,
    base: u32,
    data: Box<[u8]>,
}

impl Sram {
    pub fn new(base: u32, size: u32, name: Option<&'static str>) -> Self {
        let name = name.unwrap_or("SRAM");
        let data = vec![0; size as usize].into_boxed_slice();
        Self {
            name,
            base,
            data
        }
    }
}

impl Peripheral for Sram {
    fn name(&self) -> &'static str {
        self.name
    }

    fn mappings(&mut self) -> Vec<MemoryMapping> {
        let mm = MemoryMapping::Direct {
            base: self.base,
            ptr: self.data.as_mut_ptr(),
            size: self.data.len() as u32,
            perms: Permissions { r: true, w: true, x: true },
        };
        vec!(mm)
    }
}

#[derive(Default)]
#[derive(Peripheral)]
pub struct FlashController {
    name: &'static str,
    flash_base: u32,
    ctrl_base: u32,

    data: Box<[u8]>,

    #[register]
    reg_addr: u32,

    #[register]
    reg_data: u32,

    #[register]
    #[read_const(0)]
    reg_write: (),

    #[register]
    #[read_const(0)]
    reg_erase: (),
}

impl FlashController {
    pub fn new(flash_base: u32, page_size: Pow2, page_count: u32,
            ctrl_base: u32, name: Option<&'static str>) -> Self {
        let name = name.unwrap_or("FLASH");
        let size = page_count * page_size;
        let data = vec![0; size as usize].into_boxed_slice();
        Self {
            name,
            flash_base,
            ctrl_base,
            data,
            ..Default::default()
        }
    }

    fn set_reg_write(&mut self, _value: u32) -> Result<(), String> {
        Ok(())
    }

    fn set_reg_erase(&mut self, _value: u32) -> Result<(), String> {
        Ok(())
    }
}

impl Peripheral for FlashController {
    fn name(&self) -> &'static str {
        self.name
    }

    fn mappings(&mut self) -> Vec<MemoryMapping> {
        vec![
            MemoryMapping::Direct {
                base: self.flash_base,
                ptr: self.data.as_mut_ptr(),
                size: self.data.len() as u32,
                perms: Permissions { r: true, w: false, x: true },
            },
            MemoryMapping::Mmio {
                base: self.ctrl_base,
                size: 1024,
            },
        ]
    }

    fn mmio_read(&self, base: u32, offset: u32, size: u32) -> Result<u32, String> {
        self.read_registers(base, offset, size)
    }

    fn mmio_write(&mut self, base: u32, offset: u32, size: u32, value: u32) -> Result<(), String> {
        self.write_registers(base, offset, size, value)
    }
}
