use byteorder::{ByteOrder,LittleEndian};

use unicorn_engine::{Unicorn, RegisterARM, ArmCpuModel};
use unicorn_engine::unicorn_const::{Arch, Mode, Permission};

struct CortexMDevice {}

trait Device {
    fn load_elf(&mut self, elfdata:&[u8]) -> Result<(), String>;

    fn read_u32(&mut self, addr:u32) -> Result<u32, String>;

    fn run(&mut self);
}

impl Device for Unicorn<'_, CortexMDevice> {
    fn read_u32(&mut self, addr:u32) -> Result<u32, String> {
        let mut buf = [0u8; 4];

        self.mem_read(addr as u64, &mut buf).and_then(|_| {
            Ok(LittleEndian::read_u32(&buf))
        }).or_else(|e| {
            Err(format!("Could not read 0x{:08x} ({:?})", addr, e))
        })
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
}


fn main() {
    let dev = CortexMDevice {};
    let mut emu = unicorn_engine::Unicorn::new_with_data(Arch::ARM, Mode::LITTLE_ENDIAN, dev).unwrap();
    
    emu.ctl_set_cpu_model(ArmCpuModel::UC_CPU_ARM_CORTEX_M0.into()).unwrap();

    emu.add_intr_hook(|emu, intno| { 
        let pc = emu.reg_read(RegisterARM::PC).unwrap();
        println!("intr={}, pc=0x{:08x}", intno, pc);
        emu.emu_stop().unwrap();
    }).unwrap();

    emu.add_insn_invalid_hook(|emu| {
        let pc = emu.reg_read(RegisterARM::PC).unwrap();

        println!("invalid instruction at 0x{:08x}", pc);

        false
    }).unwrap();

    // For now, let's map some "Flash"and some RAM -- TODO: remove me
    emu.mem_map(0x0000_0000, 64*1024, Permission::ALL).unwrap();
    emu.mem_map(0x2000_0000, 16*1024, Permission::ALL).unwrap();

    emu.load_elf(include_bytes!("test.elf")).unwrap();

    emu.run();
}
