use byteorder::ByteOrder;

use log;

use unicorn_engine::unicorn_const::{Arch, HookType, Mode, Permission};
use unicorn_engine::{ArmCpuModel, RegisterARM, Unicorn};

mod demisemihosting;
mod intelhex;

/// Emulation control
trait EmulationControl {
    fn stop_emu(&mut self, result: Result<(), String>);
    fn advance_pc(&mut self) -> Result<(), String>;
}

/// Register access
trait RegisterAccess {
    fn read_reg(&mut self, register: RegisterARM) -> u32;
    fn write_reg(&mut self, register: RegisterARM, value: u32);

    fn read_pc(&mut self) -> u32 {
        self.read_reg(RegisterARM::PC) & !1
    }

    fn write_pc(&mut self, pc: u32) {
        self.write_reg(RegisterARM::PC, pc | 1);
    }
}

/// Memory access
trait MemoryAccess {
    fn read_into(&mut self, address: u32, destination: &mut [u8]) -> Result<(), String>;

    fn read_mem<const N: usize>(&mut self, address: u32) -> Result<[u8; N], String> {
        let mut buf = [0u8; N];
        self.read_into(address, &mut buf).and_then(|_| Ok(buf))
    }

    fn read_u16(&mut self, address: u32) -> Result<u16, String> {
        self.read_mem::<2>(address)
            .and_then(|buf| Ok(byteorder::LittleEndian::read_u16(&buf)))
    }

    fn read_u32(&mut self, address: u32) -> Result<u32, String> {
        self.read_mem::<4>(address)
            .and_then(|buf| Ok(byteorder::LittleEndian::read_u32(&buf)))
    }

    fn read_buf(&mut self, address: u32, length: u32) -> Result<Vec<u8>, String> {
        let mut buf: Vec<u8> = vec![0; length as usize];
        self.read_into(address, &mut buf).and_then(|_| Ok(buf))
    }

    fn read_str(&mut self, address: u32, length: u32) -> Result<String, String> {
        self.read_buf(address, length).and_then(|buf| {
            String::from_utf8(buf)
                .or_else(|e| Err(format!("Invalid UTF-8 string ({e:?})")))
        })
    }

    #[allow(dead_code)] // TODO - remove me
    fn read_str_lossy(&mut self, address: u32, length: u32) -> Result<String, String> {
        self.read_buf(address, length)
            .and_then(|buf| Ok(String::from_utf8_lossy(&buf).into()))
    }
}

/// Hook handling
trait HookHandling {
    fn setup_hooks(&mut self) -> Result<(), String>;
}

/// Emulation
pub trait Emulation {
    fn run(&mut self) -> Result<(), String>;

    fn load_segment(&mut self, address: u32, data: &[u8]) -> Result<(), String>;
    fn load_elf(&mut self, elfdata: &[u8]) -> Result<(), String>;
    fn load_ihex(&mut self, ihexdata: &[u8]) -> Result<(), String>;
}

/// Debug
trait Debug {
    fn log(&mut self, data: &[u8]);
}

impl EmulationControl for Unicorn<'_, DeviceData> {
    fn stop_emu(&mut self, result: Result<(), String>) {
        match result {
            Err(e) => { log::error!("{e}"); }
            _ => ()
        };
        self.emu_stop().unwrap();
        log::debug!("Emulation stopped");
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

impl RegisterAccess for Unicorn<'_, DeviceData> {
    fn read_reg(&mut self, register: RegisterARM) -> u32 {
        self.reg_read(register).unwrap() as u32
    }

    fn write_reg(&mut self, register: RegisterARM, value: u32) {
        self.reg_write(register, value as u64).unwrap();
    }
}

impl MemoryAccess for Unicorn<'_, DeviceData> {
    fn read_into(&mut self, address: u32, destination: &mut [u8]) -> Result<(), String> {
        self.mem_read(address as u64, destination).or_else(|e| {
            let n = destination.len();
            Err(format!("Could not read {n} bytes at 0x{address:08x} ({e:?})"))
        })
    }
}

impl HookHandling for Unicorn<'_, DeviceData> {
    fn setup_hooks(&mut self) -> Result<(), String> {
        self.add_intr_hook(|emu, intno| {
            match intno {
                7 => { demisemihosting::dispatch(emu).unwrap(); }
                _ => { panic!("Unsupported interrupt {}", intno); }
            }
        }).or_else(|e| Err(format!("Could not set INTR hook ({e:?})")))?;

        self.add_insn_invalid_hook(|emu| {
            let pc = emu.read_pc();
            log::error!("[PC:{pc:08x}] invalid instruction");
            false
        }).or_else(|e| Err(format!("Could not set INSN_INVALID hook ({e:?})")))?;

        self.add_mem_hook(HookType::MEM_UNMAPPED, 1, 0, |emu, access, address, length, _value| {
            let pc = emu.read_pc();
            log::error!("[PC:{pc:08x}] {access:?} to 0x{address:08x} ({length} bytes)");
            false
        }).or_else(|e| Err(format!("Could not set MEM_UNMAPPED hook ({e:?})")))?;

        Ok(())
    }
}

impl Emulation for Unicorn<'_, DeviceData> {
    fn run(&mut self) -> Result<(), String> {
        let vtor = 0x0000_0000; // TODO: where should the initial value come from?

        let sp = self.read_u32(vtor + 0).unwrap();
        let pc = self.read_u32(vtor + 4).unwrap();

        self.write_reg(RegisterARM::SP, sp);
        self.write_reg(RegisterARM::PC, pc);

        self.emu_start(pc as u64, u64::MAX, 0, 0)
            .or_else(|e| Err(format!("Error during emulation ({e:?})")))
    }

    fn load_segment(&mut self, address: u32, data: &[u8]) -> Result<(), String> {
        log::debug!("Loading segment at 0x{:08x} ({} bytes)", address, data.len());

        self.mem_write(address as u64, data).or_else(|e| {
            Err(format!("Could not write {} bytes at 0x{:08x} ({e:?})", data.len(), address))
        })
    }

    fn load_elf(&mut self, elfdata: &[u8]) -> Result<(), String> {
        let elffile = elf::ElfBytes::<elf::endian::LittleEndian>::minimal_parse(elfdata)
            .or_else(|e| Err(format!("{e}")))?;

        match elffile.segments() {
            Some(segments) => {
                for phdr in segments.iter().filter(|phdr| {
                    phdr.p_type == elf::abi::PT_LOAD && phdr.p_filesz > 0
                }) {
                    let data = elffile.segment_data(&phdr).unwrap();

                    self.load_segment(phdr.p_paddr as u32, data)?;
                }
                Ok(())
            }
            None => Err(String::from("No segments found in ELF file"))
        }
    }

    fn load_ihex(&mut self, ihexdata: &[u8]) -> Result<(), String> {
        for segment in intelhex::segments(ihexdata)? {
            self.load_segment(segment.address as u32, segment.data.as_slice())?;
        }
        Ok(())
    }
}

impl Debug for Unicorn<'_, DeviceData> {
    fn log(&mut self, data: &[u8]) {
        let dev = self.get_data_mut();
        use std::io::Write;
        dev.log.write(data).expect("Log is not writable");
    }
}

struct LogWriter { }

impl LogWriter {
    pub fn new() -> LogWriter {
        LogWriter {}
    }
}

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(&buf);
        let t = s.trim_end();
        if t.len() > 0 {
            log::info!("{}", t);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct DeviceData {
    log: std::io::LineWriter<LogWriter>,
}

pub fn create<'a>() -> Result<Unicorn<'a, DeviceData>, String> {
    let dev = DeviceData {
        log: std::io::LineWriter::new(LogWriter::new()),
    };
    let mut emu = Unicorn::new_with_data(Arch::ARM, Mode::LITTLE_ENDIAN, dev).unwrap();

    // TODO - make configurable
    emu.ctl_set_cpu_model(ArmCpuModel::UC_CPU_ARM_CORTEX_M0.into()).unwrap();

    emu.setup_hooks()?;

    // For now, let's map some "Flash" and some RAM -- TODO: remove me
    emu.mem_map(0x0000_0000, 64 * 1024, Permission::ALL).unwrap();
    emu.mem_map(0x2000_0000, 16 * 1024, Permission::ALL).unwrap();

    // Also, let's pretend we have a System Control Space
    emu.mem_map(0xe000_e000, 4096, Permission::ALL).unwrap();

    Ok(emu)
}
