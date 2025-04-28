use std::collections::HashMap;

// Registers are always 32-bit in this universe.

struct Register {
    name: &'static str,
    value: u32,
    reset: u32,
}

impl Register {
    fn reset(&mut self) {
        self.value = self.reset;
    }

    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value:u32) {
        self.value = value;
    }
    
    // TODO - unaligned access
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

    pub fn mmio_read(&self, addr: u32, size: u32) -> Result<u32, String> {
        match self.registers.get(&addr) {
            Some(r) => Ok(r.read()),
            None => Err(format!("No register at 0x{addr:08x}")),
        }
    }

    pub fn mmio_write(&mut self, addr: u32, size: u32, value: u32) -> Result<(), String> {
        match self.registers.get_mut(&addr) {
            Some(r) => { r.write(value); Ok(()) }
            None => Err(format!("No register at 0x{addr:08x}")),
        }
    }
}
