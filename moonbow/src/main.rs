mod device;

use std::io::Read;
use device::Emulation;

mod args {
    #[derive(clap::Parser)]
    #[command(author, version)]
    pub struct Args {
        /// Load ELF file
        #[arg(short, long)]
        pub elf: Vec<clio::Input>,

    }
}

fn main() {
    let env = env_logger::Env::new()
        .filter_or("MY_LOG", "trace")
        .write_style_or("MY_LOG_STYLE", "always");
    env_logger::init_from_env(env);

    let args = <args::Args as clap::Parser>::parse();

    let mut emu = device::create().unwrap();

    for mut f in args.elf {
        let mut data = Vec::new();

        f.read_to_end(&mut data).unwrap();
        emu.load_elf(&data.as_slice()).unwrap();
    }

    emu.run().unwrap();
}
