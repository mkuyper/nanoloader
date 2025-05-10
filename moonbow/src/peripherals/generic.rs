use super::*;

use moonbow_macros::Peripheral;
use moonbow_macros::RegBlock;

pub struct OldFlashController {
    name: &'static str,
    base: u32,
    data: Box<[u8]>,
}

impl OldFlashController {
    pub fn new(base: u32, size: u32, name: Option<&'static str>) -> Self {
        let name = name.unwrap_or("FLASH");
        let data = vec![0; size as usize].into_boxed_slice();
        Self {
            name,
            base,
            data
        }
    }
}

impl Peripheral for OldFlashController {
    fn name(&self) -> &'static str {
        self.name
    }

    fn mappings(&mut self) -> Vec<MemoryMapping> {
        let mm = MemoryMapping::Direct {
            base: self.base,
            ptr: self.data.as_mut_ptr(),
            size: self.data.len() as u32,
            perms: Permissions { r: true, w: false, x: true },
        };
        vec!(mm)
    }
}

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
    CMDTYPE:u32,
    #[register]
    CMDEXEC:(),
}

impl FlashController {
    pub fn new(flash_base: u32, size: u32, ctrl_base: u32, name: Option<&'static str>) -> Self {
        let name = name.unwrap_or("FLASH");
        let data = vec![0; size as usize].into_boxed_slice();
        Self {
            name,
            flash_base,
            ctrl_base,
            data,
            ..Default::default()
        }
    }

    fn get_CMDEXEC(&self) -> Result<u32, String> {
        Ok(0)
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
        self.read_register(base, offset).and_then(|v| {
            if size == 4 && (offset & 3) == 0 {
                Ok(v)
            } else {
                Err(String::from("unaligned access"))
            }
        })
    }
}
