use byteorder::ByteOrder;

use log;

use unicorn_engine::unicorn_const::{Arch, HookType, MemType, Mode, Permission};
use unicorn_engine::{RegisterARM, Unicorn};

mod demisemihosting;
mod intelhex;

use crate::peripherals::*;

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
            String::from_utf8(buf).or_else(|e| Err(format!("Invalid UTF-8 string ({e:?})")))
        })
    }

    #[allow(dead_code)] // TODO - remove me if not needed
    fn read_str_lossy(&mut self, address: u32, length: u32) -> Result<String, String> {
        self.read_buf(address, length)
            .and_then(|buf| Ok(String::from_utf8_lossy(&buf).into()))
    }
}

/// Emulator Setup
trait EmulatorSetup {
    fn setup_hooks(&mut self) -> Result<(), String>;
    fn setup_mapping(&mut self, mapping: MemoryMapping) -> Result<(), String>;
}

/// Emulation
pub trait Emulation {
    fn init(&mut self) -> Result<(), String>;

    fn run(&mut self) -> Result<(), String>;

    fn load_segment(&mut self, address: u32, data: &[u8]) -> Result<(), String>;
    fn load_elf(&mut self, elfdata: &[u8]) -> Result<(), String>;
    fn load_ihex(&mut self, ihexdata: &[u8]) -> Result<(), String>;
}

/// Debug
trait Debug {
    fn log(&mut self, data: &[u8]);
}

impl EmulationControl for Unicorn<'_, Context> {
    fn stop_emu(&mut self, result: Result<(), String>) {
        match result {
            Err(e) => {
                log::error!("{e}");
            }
            _ => (),
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

impl RegisterAccess for Unicorn<'_, Context> {
    fn read_reg(&mut self, register: RegisterARM) -> u32 {
        self.reg_read(register).unwrap() as u32
    }

    fn write_reg(&mut self, register: RegisterARM, value: u32) {
        self.reg_write(register, value as u64).unwrap();
    }
}

impl MemoryAccess for Unicorn<'_, Context> {
    fn read_into(&mut self, address: u32, destination: &mut [u8]) -> Result<(), String> {
        self.mem_read(address as u64, destination).or_else(|e| {
            let n = destination.len();
            Err(format!(
                "Could not read {n} bytes at 0x{address:08x} ({e:?})"
            ))
        })
    }
}

fn handle_intr(emu: &mut Unicorn<'_, Context>, intno: u32) {
    match intno {
        7 => {
            demisemihosting::dispatch(emu).unwrap();
        }
        _ => {
            panic!("Unsupported interrupt {}", intno);
        }
    }
}

fn handle_insn_invalid(emu: &mut Unicorn<'_, Context>) -> bool {
    let pc = emu.read_pc();
    log::error!("[PC:{:08x}] invalid instruction", pc);
    false
}

fn handle_mmio_read(emu: &mut Unicorn<'_, Context>, address: u64, length: usize, base: u32) -> u64 {
    let ctx = emu.get_data();

    ctx.dev
        .mmio_read(base, address as u32, length as u32)
        .unwrap_or_else(|e| {
            log::error!("mmio read failed: {e}");
            // TODO - trap? exception?
            0
        }) as u64
}

fn handle_mmio_write(
    emu: &mut Unicorn<'_, Context>,
    address: u64,
    length: usize,
    value: u64,
    base: u32,
) {
    let ctx = emu.get_data_mut();

    ctx.dev
        .mmio_write(base, address as u32, length as u32, value as u32)
        .unwrap_or_else(|e| {
            log::error!("mmio write failed: {e}");
            // TODO - trap? exception?
        });
}

fn handle_mem_unmapped(
    emu: &mut Unicorn<'_, Context>,
    access: MemType,
    address: u64,
    length: usize,
    _value: i64,
) -> bool {
    let pc = emu.read_pc();
    log::error!(
        "[PC:{:08x}] {:?} at 0x{:08x} ({} bytes)",
        pc,
        access,
        address,
        length
    );
    false
}

impl EmulatorSetup for Unicorn<'_, Context> {
    fn setup_hooks(&mut self) -> Result<(), String> {
        self.add_intr_hook(handle_intr)
            .or_else(|e| Err(format!("Could not set INTR hook ({e:?})")))?;

        self.add_insn_invalid_hook(handle_insn_invalid)
            .or_else(|e| Err(format!("Could not set INSN_INVALID hook ({e:?})")))?;

        self.add_mem_hook(HookType::MEM_UNMAPPED, 1, 0, handle_mem_unmapped)
            .or_else(|e| Err(format!("Could not set MEM_UNMAPPED hook ({e:?})")))?;

        /*
        self.add_code_hook(0, u64::MAX, |emu, address, _value| {
            //let pc = emu.read_pc();
            let pc = address;
            log::trace!("[PC:{pc:08x}]");
        }).or_else(|e| Err(format!("Could not set CODE hook ({e:?})")))?;
        */

        Ok(())
    }

    fn setup_mapping(&mut self, mapping: MemoryMapping) -> Result<(), String> {
        match mapping {
            MemoryMapping::Mmio { base, size } => {
                log::debug!("Mapping MMIO segment at 0x{:08x} ({} bytes)", base, size);

                self.mmio_map(
                    base as u64,
                    size as usize,
                    Some(move |emu: &mut Unicorn<'_, _>, address, length| {
                        handle_mmio_read(emu, address, length, base)
                    }),
                    Some(move |emu: &mut Unicorn<'_, _>, address, length, value| {
                        handle_mmio_write(emu, address, length, value, base)
                    }),
                )
                .or_else(|e| Err(format!("Could not map MMIO segment ({e:?})")))
            }
            MemoryMapping::Direct {
                base,
                ptr,
                size,
                perms,
            } => {
                log::debug!("Mapping memory segment at 0x{:08x} ({} bytes)", base, size);
                unsafe {
                    self.mem_map_ptr(
                        base as u64,
                        size as usize,
                        Permission::from(perms),
                        ptr as *mut std::ffi::c_void,
                    )
                }
                .or_else(|e| Err(format!("Could not map raw segment ({e:?})")))
            }
        }
    }
}

impl Emulation for Unicorn<'_, Context> {
    fn init(&mut self) -> Result<(), String> {
        self.ctl_set_cpu_model(self.get_data().dev.cpu_model.into())
            .or_else(|e| Err(format!("Error setting CPU model ({e:?})")))?;

        let mappings: Vec<_> = self
            .get_data_mut()
            .dev
            .peripherals
            .iter_mut()
            .map(|p| p.mappings())
            .flatten()
            .map(|m| m.clone())
            .collect();

        for m in mappings {
            self.setup_mapping(m)?;
        }

        self.setup_hooks()?;

        Ok(())
    }

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
        log::debug!(
            "Loading segment at 0x{:08x} ({} bytes)",
            address,
            data.len()
        );

        self.mem_write(address as u64, data).or_else(|e| {
            Err(format!(
                "Could not write {} bytes at 0x{:08x} ({e:?})",
                data.len(),
                address
            ))
        })
    }

    fn load_elf(&mut self, elfdata: &[u8]) -> Result<(), String> {
        let elffile = elf::ElfBytes::<elf::endian::LittleEndian>::minimal_parse(elfdata)
            .or_else(|e| Err(format!("{e}")))?;

        match elffile.segments() {
            Some(segments) => {
                for phdr in segments
                    .iter()
                    .filter(|phdr| phdr.p_type == elf::abi::PT_LOAD && phdr.p_filesz > 0)
                {
                    let data = elffile.segment_data(&phdr).unwrap();

                    self.load_segment(phdr.p_paddr as u32, data)?;
                }
                Ok(())
            }
            None => Err(String::from("No segments found in ELF file")),
        }
    }

    fn load_ihex(&mut self, ihexdata: &[u8]) -> Result<(), String> {
        for segment in intelhex::segments(ihexdata)? {
            self.load_segment(segment.address as u32, segment.data.as_slice())?;
        }
        Ok(())
    }
}

impl Debug for Unicorn<'_, Context> {
    fn log(&mut self, data: &[u8]) {
        let ctx = self.get_data_mut();
        use std::io::Write;
        ctx.log.write(data).expect("Log is not writable");
    }
}

struct LogWriter {}

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

pub struct Context {
    log: std::io::LineWriter<LogWriter>,
    dev: Device,
}

pub fn create_emulator<'a>(dev: Device) -> Result<Unicorn<'a, Context>, String> {
    let ctx = Context {
        log: std::io::LineWriter::new(LogWriter::new()),
        dev,
    };
    let mut emu = Unicorn::new_with_data(Arch::ARM, Mode::LITTLE_ENDIAN, ctx).unwrap();

    emu.init()?;

    Ok(emu)
}

// ------------------------------------------------------------------------------------------------
use std::collections::HashMap;
use unicorn_engine::ArmCpuModel;

pub enum CpuModel {
    M0Plus,
}

pub struct Device {
    peripherals: Vec<Box<dyn Peripheral>>,
    mmio_mappings: HashMap<u32, usize>,
    cpu_model: ArmCpuModel,
}

impl Device {
    pub fn new(model: CpuModel, mut peripherals: Vec<Box<dyn Peripheral>>) -> Self {
        let acm = match model {
            CpuModel::M0Plus => ArmCpuModel::UC_CPU_ARM_CORTEX_M0,
        };

        match model {
            CpuModel::M0Plus => {
                // TODO - SCS should be special as it contains the NVIC
                peripherals.push(Box::new(cortex_m0::SCS::new()));
            }
        };

        let mut dev = Self {
            peripherals: peripherals,
            mmio_mappings: HashMap::<u32, usize>::new(),
            cpu_model: acm,
        };

        for (idx, p) in dev.peripherals.iter_mut().enumerate() {
            for m in p.mappings() {
                match m {
                    MemoryMapping::Mmio { base, .. } => {
                        dev.mmio_mappings.insert(base, idx);
                    }
                    _ => {}
                }
            }
        }

        dev
    }

    fn get_peripheral_idx(&self, base: u32) -> Result<usize, String> {
        self.mmio_mappings
            .get(&base)
            .map(|i| *i)
            .ok_or_else(|| format!("No peripheral mapped at 0x{base:08x}"))
    }

    fn get_peripheral(&self, base: u32) -> Result<&Box<dyn Peripheral>, String> {
        let idx = self.get_peripheral_idx(base)?;
        Ok(&self.peripherals[idx])
    }

    fn get_peripheral_mut(&mut self, base: u32) -> Result<&mut Box<dyn Peripheral>, String> {
        let idx = self.get_peripheral_idx(base)?;
        Ok(&mut self.peripherals[idx])
    }

    fn mmio_read(&self, base: u32, offset: u32, size: u32) -> Result<u32, String> {
        self.get_peripheral(base)?.mmio_read(base, offset, size)
    }

    fn mmio_write(&mut self, base: u32, offset: u32, size: u32, value: u32) -> Result<(), String> {
        self.get_peripheral_mut(base)?
            .mmio_write(base, offset, size, value)
    }
}

impl From<Permissions> for unicorn_engine::Permission {
    fn from(p: Permissions) -> Self {
        let mut q = unicorn_engine::Permission::NONE;

        if p.r {
            q |= unicorn_engine::Permission::READ;
        }
        if p.w {
            q |= unicorn_engine::Permission::WRITE;
        }
        if p.x {
            q |= unicorn_engine::Permission::EXEC;
        }

        q
    }
}

// ------------------------------------------------------------------------------------------------
