mod device;
mod peripherals;

use pow2::pow2_const;
use std::io::Read;

use device::Emulation;
use peripherals::generic::{FlashController, Sram};

mod args {
    #[derive(clap::Parser)]
    #[command(author, version)]
    pub struct Args {
        /// Load ELF file
        #[arg(short, long)]
        pub elf: Vec<clio::Input>,

        /// Load Intel HEX file
        #[arg(short, long)]
        pub ihex: Vec<clio::Input>,

    }
}

fn main() {
    let env = env_logger::Env::new()
        .filter_or("MY_LOG", "trace")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);

    let args = <args::Args as clap::Parser>::parse();

    let peripherals: Vec<Box<dyn peripherals::Peripheral>> = vec!(
        Box::new(Sram::new(0x2000_0000, 4 * 1024, None)),
        Box::new(FlashController::new(0x0000_0000, pow2_const!(1024), 64, 0x4000_000, None)),
    );
    let dev = device::Device::new(device::CpuModel::M0Plus, peripherals);

    let mut emu = device::create_emulator(dev).unwrap();

    for mut f in args.elf {
        let mut data = Vec::new();

        f.read_to_end(&mut data).unwrap();
        emu.load_elf(&data.as_slice()).unwrap();
    }

    for mut f in args.ihex {
        let mut data = Vec::new();

        f.read_to_end(&mut data).unwrap();
        emu.load_ihex(&data.as_slice()).unwrap();
    }

    emu.run().unwrap();
}
