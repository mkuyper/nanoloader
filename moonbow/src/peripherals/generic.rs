use super::*;

pub struct FlashController {
    name: &'static str,
    base: u32,
    data: Box<[u8]>,
}

impl FlashController {
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

impl Peripheral for FlashController {
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

