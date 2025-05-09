pub mod generic;
pub mod cortex_m0;


#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub r: bool,
    pub w: bool,
    pub x: bool,
}

#[derive(Debug, Clone)]
pub enum MemoryMapping {
    Mmio { base: u32, size: u32 },
    Direct { base: u32, ptr: *mut u8, size: u32, perms: Permissions },
}

pub trait Peripheral {
    fn name(&self) -> &'static str;

    fn mappings(&mut self) -> Vec<MemoryMapping>;

    fn mmio_read(&self, _base: u32, _offset: u32, _size: u32) -> Result<u32, String> {
        Err(String::from("not implemented"))
    }

    fn mmio_write(&mut self, _base: u32, _offset: u32, _size: u32, _value: u32) -> Result<(), String> {
        Err(String::from("not implemented"))
    }
}

pub struct Register {
    pub name: &'static str,
    pub value: u32,
}

pub trait RegisterBlock {
    fn get(&self, offset:u32) -> Option<&Register>;
    fn get_mut(&mut self, addr:u32) -> Option<&mut Register>;

    fn base(&self) -> Option<u32>;
    fn size(&self) -> u32;
    fn name(&self) -> &'static str;

    fn read(&self, base: u32, offset: u32, size: u32) -> Result<u32, String> {
        let Some(reg) = self.get(offset) else {
            return Err(format!("No register mapped at 0x{:08x} ({}+0x{:x})",
                    base + offset, self.name(), offset));
        };

        let value = reg.value;

        if size == 4 && (offset & 3) == 0 {
            return Ok(value);
        }

        // TODO - unaligned access
        Err(String::from("unaligned access"))
    }

    fn write(&mut self, base: u32, offset: u32, size: u32, value: u32) -> Result<(), String> {
        let Some(reg) = self.get_mut(offset) else {
            return Err(format!("No register mapped at 0x{:08x} ({}+0x{:x})",
                    base + offset, self.name(), offset));
        };

        if size == 4 && (offset & 3) == 0 {
            reg.value = value;
            {
                let rname = reg.name;
                let bname = self.name();
                log::trace!("Writing 0x{:08x} ({}) to register at 0x{:08x} ({}:{})",
                        value, value, base + offset, bname, rname);
            }
            return Ok(());
        }

        // TODO - unaligned access
        Err(String::from("unaligned access"))
    }
}
