use std::collections::HashMap;

//use moonbow_macros::RegBlock;

// Registers are always 32-bit in this universe.

struct Register {
    name: &'static str,
    value: u32,
    reset: u32,
}

/*
trait RegisterBlock {
    fn get(&self, addr:u32) -> Option<&Register>;
    //fn reset(&self); ?
}
*/

// not sure we need this, direct access should be fine
impl Register {
    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value:u32) {
        self.value = value;
    }
}


pub struct MmioDispatcher {
    mmrs: HashMap<u32, MemoryMappedRegion>,
}

impl MmioDispatcher {
    pub fn new() -> Self {
        let map = HashMap::<u32, MemoryMappedRegion>::new();

        MmioDispatcher {
            mmrs: map,
        }
    }

    pub fn add(&mut self, mmr: MemoryMappedRegion) {
        self.mmrs.insert(mmr.base, mmr);
    }

    pub fn mmio_read(&self, base: u32, offset: u32, size: u32) -> Result<u32, String> {
        self.get_mmr(base)?.read(offset, size)
    }

    pub fn mmio_write(&mut self, base: u32, offset: u32, size: u32, value: u32) -> Result<(), String> {
        self.get_mmr_mut(base)?.write(offset, size, value)
    }

    fn get_mmr_mut(&mut self, base:u32) -> Result<&mut MemoryMappedRegion, String> {
        match self.mmrs.get_mut(&base) {
            Some(mmr) => Ok(mmr),
            None => Err(format!("No memory mapped region at 0x{base:08x}")),
        }
    }

    fn get_mmr(&self, base:u32) -> Result<&MemoryMappedRegion, String> {
        match self.mmrs.get(&base) {
            Some(mmr) => Ok(mmr),
            None => Err(format!("No memory mapped region at 0x{base:08x}")),
        }
    }
}


pub struct MemoryMappedRegion {
    name: &'static str,
    base: u32,
    registers: HashMap<u32, Register>,
}

impl MemoryMappedRegion {
    pub fn new(base: u32, name: &'static str) -> Self {
        let map = HashMap::<u32, Register>::new();

        MemoryMappedRegion {
            name: name,
            base: base,
            registers: map,
        }
    }

    pub fn add(&mut self, addr:u32, name: &'static str) {
        let reg = Register {
            name: name,
            value: 0,
            reset: 0,
        };

        self.registers.insert(addr, reg);
    }

    pub fn read(&self, offset: u32, size: u32) -> Result<u32, String> {
        if size == 4 {
            Ok(self.get_reg(offset)?.read())
        } else {
            /*
            let base = offset & !3;
            let shift = offset - base;
            let reg = self.get_reg(base)?;
            match (size, shift) {
                (2, 0) => 0
            }
            */
            Err(String::from("not implemented"))
        }
    }

    pub fn write(&mut self, offset: u32, size: u32, value: u32) -> Result<(), String> {
        if size == 4 {
            self.get_reg_mut(offset)?.write(value);
            Ok(())
        } else {
            Err(String::from("not implemented"))
        }
    }
    
    fn get_reg_mut(&mut self, offset:u32) -> Result<&mut Register, String> {
        match self.registers.get_mut(&offset) {
            Some(r) => Ok(r),
            None => Err(format!("No register mapped at 0x{:08x} ({}+0x{:x})",
                    self.base + offset, self.name, offset)),
        }
    }

    fn get_reg(&self, offset:u32) -> Result<&Register, String> {
        match self.registers.get(&offset) {
            Some(r) => Ok(r),
            None => Err(format!("No register mapped at 0x{:08x} ({}+{:04x})",
                    self.base + offset, self.name, offset)),
        }
    }
}
