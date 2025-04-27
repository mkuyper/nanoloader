//! A bare-minimum ARM Semihosting implementation

const SYS_OPEN: u32 = 0x01;
const SYS_WRITE: u32 = 0x05;
const ANGEL_REPORT_EXCEPTION: u32 = 0x18;

const ADP_STOPPED_RUNTIME_ERROR_UNKNOWN: u32 = 0x20023;
const ADP_STOPPED_APPLICATION_EXIT: u32 = 0x20026;

use crate::device::*;

const FILENO_STDIO_MAGIC: u32 = 0x1234;

pub fn dispatch<T>(emu: &mut T) -> Result<(), String>
where
    T: EmulationControl + RegisterAccess + MemoryAccess + Debug,
{
    let r0 = emu.read_reg(RegisterARM::R0);

    match r0 {
        SYS_OPEN => sys_open(emu),
        SYS_WRITE => sys_write(emu),
        ANGEL_REPORT_EXCEPTION => angel_report_exception(emu),
        _ => Err(format!("Unsupported semihosting call {r0} (0x{r0:08x})")),
    }
    .and_then(|_| emu.advance_pc())
}

fn sys_open<T>(emu: &mut T) -> Result<(), String>
where
    T: RegisterAccess + MemoryAccess,
{
    let r1 = emu.read_reg(RegisterARM::R1);

    let fnptr = emu.read_u32(r1 + 0)?;
    let fnlen = emu.read_u32(r1 + 8)?;

    let fname = emu.read_str(fnptr, fnlen)?;

    let r0 = if fname == ":tt" {
        FILENO_STDIO_MAGIC
    } else {
        -1_i32 as u32
    };

    emu.write_reg(RegisterARM::R0, r0);

    Ok(())
}

fn sys_write<T>(emu: &mut T) -> Result<(), String>
where
    T: RegisterAccess + MemoryAccess + Debug,
{
    let r1 = emu.read_reg(RegisterARM::R1);

    let fd = emu.read_u32(r1 + 0)?;
    let dptr = emu.read_u32(r1 + 4)?;
    let dlen = emu.read_u32(r1 + 8)?;

    let r0 = match fd {
        FILENO_STDIO_MAGIC => {
            let d = emu.read_buf(dptr, dlen)?;
            emu.log(&d.as_slice());
            0
        }
        _ => dlen,
    };

    emu.write_reg(RegisterARM::R0, r0);

    Ok(())
}

fn angel_report_exception<T>(emu: &mut T) -> Result<(), String>
where
    T: RegisterAccess + EmulationControl,
{
    let r1 = emu.read_reg(RegisterARM::R1);

    match r1 {
        ADP_STOPPED_APPLICATION_EXIT => {
            emu.stop_emu(Ok(()));
            Ok(())
        }
        ADP_STOPPED_RUNTIME_ERROR_UNKNOWN => {
            emu.stop_emu(Err(String::from("Application exited with error")));
            Ok(())
        }
        _ => Err(format!(
            "Unsupported exception reported to angel: 0x{r1:08x}"
        )),
    }
}
