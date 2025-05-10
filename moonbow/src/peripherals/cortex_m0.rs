use moonbow_macros::Peripheral;
use super::*;

#[derive(Default)]
#[derive(Peripheral)]
pub struct SCS {
    #[register]
    #[offset(0xd08)]
    vtor: u32,
}

impl SCS {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}

impl Peripheral for SCS {
    fn name(&self) -> &'static str {
        "SCS"
    }

    fn mappings(&mut self) -> Vec<MemoryMapping> {
        vec![
            MemoryMapping::Mmio {
                base: 0xe000e000,
                size: 4096,
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
