mod device;

use std::io::Read;
use device::Emulation;

mod args {
    #[derive(clap::Parser)]
    #[command(author, version)]
    pub struct Args {
        /// Binary file to load
        #[arg(short, long)]
        pub load: Vec<clio::Input>,

    }
}

fn main() {
    let args = <args::Args as clap::Parser>::parse();

    let mut emu = device::create().unwrap();

    for mut f in args.load {
        let mut data = Vec::new();

        f.read_to_end(&mut data).unwrap();
        emu.load_elf(&data.as_slice()).unwrap();
    }

    emu.run().unwrap();
}
