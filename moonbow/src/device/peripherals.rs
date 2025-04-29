use std::collections::HashMap;

use moonbow_macros::RegBlock;

// Registers are always 32-bit in this universe.

struct Register {
    name: &'static str,
    value: u32,
}

trait RegisterBlock {
    fn get(&self, offset:u32) -> Option<&Register>;
    fn get_mut(&mut self, addr:u32) -> Option<&mut Register>;

    fn base(&self) -> Option<u32>;

    fn read(&self, offset: u32, size: u32) -> Result<u32, String> {
        let Some(reg) = self.get(offset) else {
            return Err(format!("No register mapped at {}+0x{:x}",
                    "(unknown)", offset));
        };

        let value = reg.read();

        let base = offset & !3;
        let shift = offset - base;

        match (size, shift) {
            (4, 0) => Ok(value),
            _ => Err(String::from("unaligned access"))
        }
    }

    fn write(&mut self, offset: u32, size: u32, value: u32) -> Result<(), String> {
        let Some(reg) = self.get_mut(offset) else {
            return Err(format!("No register mapped at {}+0x{:x}",
                    "(unknown)", offset));
        };

        let base = offset & !3;
        let shift = offset - base;

        match (size, shift) {
            (4, 0) => Ok(reg.write(value)),
            _ => Err(String::from("unaligned access"))
        }
    }
}

impl Register {
    fn read(&self) -> u32 {
        self.value
    }

    fn write(&mut self, value:u32) {
        log::debug!("Writing 0x{:08x} ({}) to register {}", value, value, self.name);

        self.value = value;
    }
}

mod cortex_m0 {
    use super::*;

    #[derive(RegBlock)]
    #[base(0xe000e000)]
    pub struct SystemControlSpace {

        #[offset(0xd08)]
        vtor: Register,
    }
}

pub struct TestDevice {
    blocks: HashMap<u32, Box<dyn RegisterBlock>>,
}

impl TestDevice {
    pub fn new() -> Self {
        let mut me = TestDevice {
            blocks: HashMap::<u32, Box<dyn RegisterBlock>>::new(),
        };

        me.add_block(Box::new(cortex_m0::SystemControlSpace::new()));

        me
    }

    fn add_block(&mut self, block: Box<dyn RegisterBlock>) {
        self.blocks.insert(block.base().unwrap(), block);
    }

    #[allow(dead_code)] // TODO - remove me
    fn add_block_at(&mut self, addr:u32, block: Box<dyn RegisterBlock>) {
        self.blocks.insert(addr, block);
    }

    pub fn mmio_read(&self, base: u32, offset: u32, size: u32) -> Result<u32, String> {
        self.get_block(base)?.read(offset, size)
    }

    pub fn mmio_write(&mut self, base: u32, offset: u32, size: u32, value: u32) -> Result<(), String> {
        self.get_block_mut(base)?.write(offset, size, value)
    }

    fn get_block_mut(&mut self, base:u32) -> Result<&mut Box<dyn RegisterBlock>, String> {
        match self.blocks.get_mut(&base) {
            Some(blk) => Ok(blk),
            None => Err(format!("No register block mapped at 0x{base:08x}")),
        }
    }

    fn get_block(&self, base:u32) -> Result<&Box<dyn RegisterBlock>, String> {
        match self.blocks.get(&base) {
            Some(blk) => Ok(blk),
            None => Err(format!("No register block mapped at 0x{base:08x}")),
        }
    }
}
