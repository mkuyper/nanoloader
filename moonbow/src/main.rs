use byteorder::{ByteOrder,LittleEndian};

use unicorn_engine::{Unicorn, RegisterARM, ArmCpuModel};
use unicorn_engine::unicorn_const::{Arch, Mode, Permission, HookType};

mod arm_sh_const {
    pub const SYS_OPEN: u32 = 0x01;
    pub const SYS_WRITE: u32 = 0x05;
    pub const ANGEL_REPORT_EXCEPTION:u32 = 0x18;

    pub const ADP_STOPPED_RUNTIME_ERROR_UNKNOWN: u32 = 0x20023;
    pub const ADP_STOPPED_APPLICATION_EXIT: u32 = 0x20026;
}

struct CortexMDevice {}

// A bare-minimum ARM Semihosting implementation
trait DemiSemiHosting {
    const FILENO_STDIO_MAGIC: u32 = 0x1234;

    fn intr(&mut self) -> Result<(), String>;

    fn sh_sys_open(&mut self) -> Result<(), String>;
    fn sh_sys_write(&mut self) -> Result<(), String>;

    fn sh_angel_report_exception(&mut self) -> Result<(), String>;
}

impl DemiSemiHosting for Unicorn<'_, CortexMDevice> {
    fn intr(&mut self) -> Result<(), String> {
        let r0 = self.reg_read(RegisterARM::R0).unwrap() as u32;

        match r0 {
            arm_sh_const::SYS_OPEN => self.sh_sys_open(),
            arm_sh_const::SYS_WRITE => self.sh_sys_write(),
            arm_sh_const::ANGEL_REPORT_EXCEPTION => self.sh_angel_report_exception(),
            _ => Err(format!("Unsupported semihosting call {} (0x{:08x})", r0, r0))
        }.and_then(|_| {
            self.advance_pc()
        })
    }

    fn sh_sys_open(&mut self) -> Result<(), String> {
        let r1 = self.reg_read(RegisterARM::R1).unwrap() as u32;

        let fnptr = self.read_u32(r1 + 0)?;
        //let mode = self.read_u32(r1 + 4)?;
        let fnlen = self.read_u32(r1 + 8)?;

        let fname = self.read_str(fnptr, fnlen)?;

        let r0 = if fname == ":tt" { Self::FILENO_STDIO_MAGIC } else { 1u32.wrapping_neg() };

        self.reg_write(RegisterARM::R0, r0 as u64).unwrap();

        Ok(())
    }

    fn sh_sys_write(&mut self) -> Result<(), String> {
        let r1 = self.reg_read(RegisterARM::R1).unwrap() as u32;

        let fd = self.read_u32(r1 + 0)?;
        let dptr = self.read_u32(r1 + 4)?;
        let dlen = self.read_u32(r1 + 8)?;

        let r0 = match fd {
            Self::FILENO_STDIO_MAGIC => {
                let s = self.read_str_lossy(dptr, dlen)?;
                print!("{}", s);
                0
            },
            _ => dlen
        };

        self.reg_write(RegisterARM::R0, r0 as u64).unwrap();

        Ok(())
    }

    fn sh_angel_report_exception(&mut self) -> Result<(), String> {
        let r1 = self.reg_read(RegisterARM::R1).unwrap() as u32;

        match r1 {
            arm_sh_const::ADP_STOPPED_APPLICATION_EXIT => {
                self.emu_stop().unwrap();
                Ok(())
            },
            arm_sh_const::ADP_STOPPED_RUNTIME_ERROR_UNKNOWN => {
                println!("Application exited with error");
                self.emu_stop().unwrap();
                Ok(())
            },
            _ => Err(format!("Unsupported exception reported to angel: 0x{:08x}", r1))
        }
    }
}


trait Device {
    fn load_elf(&mut self, elfdata:&[u8]) -> Result<(), String>;

    fn read_u16(&mut self, addr:u32) -> Result<u16, String>;
    fn read_u32(&mut self, addr:u32) -> Result<u32, String>;
    fn read_str(&mut self, addr:u32, len:u32) -> Result<String, String>;
    fn read_str_lossy(&mut self, addr:u32, len:u32) -> Result<String, String>;

    fn advance_pc(&mut self) -> Result<(), String>;

    fn run(&mut self);

    fn intr_hook(&mut self, intno:u32);


}

impl Device for Unicorn<'_, CortexMDevice> {
    fn read_u16(&mut self, addr:u32) -> Result<u16, String> {
        let mut buf = [0u8; 2];

        self.mem_read(addr as u64, &mut buf).and_then(|_| {
            Ok(LittleEndian::read_u16(&buf))
        }).or_else(|e| {
            Err(format!("Could not read 0x{:08x} ({:?})", addr, e))
        })
    }

    fn read_u32(&mut self, addr:u32) -> Result<u32, String> {
        let mut buf = [0u8; 4];

        self.mem_read(addr as u64, &mut buf).and_then(|_| {
            Ok(LittleEndian::read_u32(&buf))
        }).or_else(|e| {
            Err(format!("Could not read 0x{:08x} ({:?})", addr, e))
        })
    }

    fn read_str(&mut self, addr:u32, len:u32) -> Result<String, String> {
        let mut buf: Vec<u8> = vec![0; len as usize];

        self.mem_read(addr as u64, &mut buf).or_else(|e| {
            Err(format!("Could not read 0x{:08x} ({:?})", addr, e))
        }).and_then(|_| {
            String::from_utf8(buf).and_then(|s| {
                Ok(s)
            }).or_else(|e| {
                Err(format!("Invalid UTF-8 string ({:?})", e))
            })
        })
    }

    fn read_str_lossy(&mut self, addr:u32, len:u32) -> Result<String, String> {
        let mut buf: Vec<u8> = vec![0; len as usize];

        self.mem_read(addr as u64, &mut buf).or_else(|e| {
            Err(format!("Could not read 0x{:08x} ({:?})", addr, e))
        }).and_then(|_| {
            Ok(String::from_utf8_lossy(&buf).into())
        })
    }

    fn advance_pc(&mut self) -> Result<(), String> {
        let pc = (self.reg_read(RegisterARM::PC).unwrap() as u32) & !1;

        // Check if current instruction is a 32-bit instruction
        let ins = self.read_u16(pc)?;
        let step = if (ins >> 11) > 0x1c { 4 } else { 2 };

        self.reg_write(RegisterARM::PC, ((pc + step) | 1) as u64).unwrap();
        Ok(())
    }

    fn run(&mut self) {
        let vtor = 0x0000_0000; // TODO: where should the initial value come from?

        let sp = self.read_u32(vtor + 0).unwrap();
        let pc = self.read_u32(vtor + 4).unwrap();

        self.reg_write(RegisterARM::SP, sp as u64).unwrap();
        self.reg_write(RegisterARM::PC, pc as u64).unwrap();

        self.emu_start(pc as u64, u64::MAX, 0, 0).expect("oops?");
    }

    fn load_elf(&mut self, elfdata:&[u8]) -> Result<(), String> {
        let file = elf::ElfBytes::<elf::endian::LittleEndian>::minimal_parse(elfdata).unwrap();

        for phdr in file.segments().unwrap().iter().filter(|phdr| {
            phdr.p_type == elf::abi::PT_LOAD && phdr.p_filesz > 0
        }) {
            let data = file.segment_data(&phdr).unwrap();
            self.mem_write(phdr.p_paddr, data).or_else(|e| {
                return Err(format!("Could not write {} bytes at 0x{:08x} ({:?})",
                    phdr.p_filesz, phdr.p_paddr, e));
            }).unwrap();
        }
        Ok(())
    }

    fn intr_hook(&mut self, intno:u32) {
        match intno {
            7 => { DemiSemiHosting::intr(self).unwrap(); },
            _ => { panic!("Unsupported interrupt {}", intno) }
        };
    }
}


fn main() {
    let dev = CortexMDevice {};
    let mut emu = unicorn_engine::Unicorn::new_with_data(Arch::ARM, Mode::LITTLE_ENDIAN, dev).unwrap();
    
    emu.ctl_set_cpu_model(ArmCpuModel::UC_CPU_ARM_CORTEX_M0.into()).unwrap();

    emu.add_intr_hook(|emu, intno| { 
        emu.intr_hook(intno);
    }).unwrap();

    emu.add_insn_invalid_hook(|emu| {
        let pc = emu.reg_read(RegisterARM::PC).unwrap();

        println!("[PC:{:08x}] invalid instruction", pc);

        false
    }).unwrap();

    emu.add_mem_hook(HookType::MEM_UNMAPPED, 1, 0, |emu, access, address, length, _value| {
        let pc = emu.reg_read(RegisterARM::PC).unwrap();

        println!("[PC:{:08x}] {:?} to 0x{:08x} ({} bytes)", pc, access, address, length);

        false
    }).unwrap();

    // For now, let's map some "Flash" and some RAM -- TODO: remove me
    emu.mem_map(0x0000_0000, 64*1024, Permission::ALL).unwrap();
    emu.mem_map(0x2000_0000, 16*1024, Permission::ALL).unwrap();

    // For now, let's load a little ELF file -- TODO: remove me
    emu.load_elf(include_bytes!("test.elf")).unwrap();

    emu.run();
}
