use byteorder::{ByteOrder, LittleEndian};

use unicorn_engine::unicorn_const::{Arch, HookType, Mode, Permission};
use unicorn_engine::{ArmCpuModel, RegisterARM, Unicorn};

mod demisemihosting;

/// Device emulation control
trait EmulationControl {
    fn stop_emu(&mut self);
    fn advance_pc(&mut self) -> Result<(), String>;
}

/// Device register access
trait RegisterAccess {
    fn read_reg(&mut self, register: RegisterARM) -> u32;
    fn write_reg(&mut self, register: RegisterARM, value: u32);

    fn read_pc(&mut self) -> u32;
    fn write_pc(&mut self, pc: u32);
}

/// Device memory access
trait MemoryAccess {
    fn read_into(&mut self, address: u32, destination: &mut [u8]) -> Result<(), String>;

    fn read_mem<const N: usize>(&mut self, address: u32) -> Result<[u8; N], String>;
    fn read_u16(&mut self, address: u32) -> Result<u16, String>;
    fn read_u32(&mut self, address: u32) -> Result<u32, String>;

    fn read_buf(&mut self, address: u32, length: u32) -> Result<Vec<u8>, String>;
    fn read_str(&mut self, address: u32, length: u32) -> Result<String, String>;
    fn read_str_lossy(&mut self, address: u32, length: u32) -> Result<String, String>;
}

impl<T> EmulationControl for Unicorn<'_, T> {
    fn stop_emu(&mut self) {
        self.emu_stop().unwrap();
    }

    fn advance_pc(&mut self) -> Result<(), String> {
        let pc = self.read_pc();

        // Check if current instruction is a 32-bit instruction
        let ins = self.read_u16(pc)?;
        let step = if (ins >> 11) > 0x1c { 4 } else { 2 };

        self.write_pc(pc + step);
        Ok(())
    }
}

impl<T> RegisterAccess for Unicorn<'_, T> {
    fn read_reg(&mut self, register: RegisterARM) -> u32 {
        self.reg_read(register).unwrap() as u32
    }

    fn write_reg(&mut self, register: RegisterARM, value: u32) {
        self.reg_write(register, value as u64).unwrap();
    }

    fn read_pc(&mut self) -> u32 {
        self.read_reg(RegisterARM::PC) & !1
    }

    fn write_pc(&mut self, pc: u32) {
        self.write_reg(RegisterARM::PC, pc | 1);
    }
}

impl<T> MemoryAccess for Unicorn<'_, T> {
    fn read_into(&mut self, address: u32, destination: &mut [u8]) -> Result<(), String> {
        self.mem_read(address as u64, destination).or_else(|e| {
            Err(format!(
                "Could not {} bytes at 0x{:08x} ({:?})",
                destination.len(),
                address,
                e
            ))
        })
    }

    fn read_mem<const N: usize>(&mut self, address: u32) -> Result<[u8; N], String> {
        let mut buf = [0u8; N];
        self.read_into(address, &mut buf).and_then(|_| Ok(buf))
    }

    fn read_u16(&mut self, address: u32) -> Result<u16, String> {
        self.read_mem::<2>(address)
            .and_then(|buf| Ok(LittleEndian::read_u16(&buf)))
    }

    fn read_u32(&mut self, address: u32) -> Result<u32, String> {
        self.read_mem::<4>(address)
            .and_then(|buf| Ok(LittleEndian::read_u32(&buf)))
    }

    fn read_buf(&mut self, address: u32, length: u32) -> Result<Vec<u8>, String> {
        let mut buf: Vec<u8> = vec![0; length as usize];
        self.read_into(address, &mut buf).and_then(|_| Ok(buf))
    }

    fn read_str(&mut self, address: u32, length: u32) -> Result<String, String> {
        self.read_buf(address, length).and_then(|buf| {
            String::from_utf8(buf)
                .and_then(|s| Ok(s))
                .or_else(|e| Err(format!("Invalid UTF-8 string ({:?})", e)))
        })
    }

    fn read_str_lossy(&mut self, address: u32, length: u32) -> Result<String, String> {
        self.read_buf(address, length)
            .and_then(|buf| Ok(String::from_utf8_lossy(&buf).into()))
    }
}

// Misc. stuff that needs to be organized
trait Device<T> {
    fn load_elf(&mut self, elfdata: &[u8]) -> Result<(), String>;

    fn run(&mut self);

    fn intr_hook(&mut self, intno: u32);
}

impl<T> Device<T> for Unicorn<'_, T> {
    fn run(&mut self) {
        let vtor = 0x0000_0000; // TODO: where should the initial value come from?

        let sp = self.read_u32(vtor + 0).unwrap();
        let pc = self.read_u32(vtor + 4).unwrap();

        self.write_reg(RegisterARM::SP, sp);
        self.write_reg(RegisterARM::PC, pc);

        self.emu_start(pc as u64, u64::MAX, 0, 0).expect("oops?");
    }

    fn load_elf(&mut self, elfdata: &[u8]) -> Result<(), String> {
        let file = elf::ElfBytes::<elf::endian::LittleEndian>::minimal_parse(elfdata).unwrap();

        for phdr in file
            .segments()
            .unwrap()
            .iter()
            .filter(|phdr| phdr.p_type == elf::abi::PT_LOAD && phdr.p_filesz > 0)
        {
            let data = file.segment_data(&phdr).unwrap();
            self.mem_write(phdr.p_paddr, data)
                .or_else(|e| {
                    return Err(format!(
                        "Could not write {} bytes at 0x{:08x} ({:?})",
                        phdr.p_filesz, phdr.p_paddr, e
                    ));
                })
                .unwrap();
        }
        Ok(())
    }

    fn intr_hook(&mut self, intno: u32) {
        match intno {
            7 => {
                demisemihosting::dispatch(self).unwrap();
            }
            _ => {
                panic!("Unsupported interrupt {}", intno)
            }
        };
    }
}

struct CortexMDevice {} // still not sure if we'll ever need this...

fn main() {
    let dev = CortexMDevice {};
    let mut emu =
        unicorn_engine::Unicorn::new_with_data(Arch::ARM, Mode::LITTLE_ENDIAN, dev).unwrap();

    emu.ctl_set_cpu_model(ArmCpuModel::UC_CPU_ARM_CORTEX_M0.into())
        .unwrap();

    emu.add_intr_hook(|emu, intno| {
        emu.intr_hook(intno);
    })
    .unwrap();

    emu.add_insn_invalid_hook(|emu| {
        let pc = emu.read_pc();

        println!("[PC:{:08x}] invalid instruction", pc);

        false
    })
    .unwrap();

    emu.add_mem_hook(
        HookType::MEM_UNMAPPED,
        1,
        0,
        |emu, access, address, length, _value| {
            let pc = emu.read_pc();

            println!(
                "[PC:{:08x}] {:?} to 0x{:08x} ({} bytes)",
                pc, access, address, length
            );

            false
        },
    )
    .unwrap();

    // For now, let's map some "Flash" and some RAM -- TODO: remove me
    emu.mem_map(0x0000_0000, 64 * 1024, Permission::ALL)
        .unwrap();
    emu.mem_map(0x2000_0000, 16 * 1024, Permission::ALL)
        .unwrap();

    // For now, let's load a little ELF file -- TODO: remove me
    emu.load_elf(include_bytes!("test.elf")).unwrap();

    emu.run();
}
