#![no_main]
#![no_std]

use mspm0cloader::MspM0CHal;
use mspm0cloader::{LedSettings, NanoBoard};

struct TestBoard {}

impl NanoBoard for TestBoard {
    const LED: Option<LedSettings> = Some(LedSettings {
        gpio: 22,
        tu_cycles: 6_000_000,
    });
}

#[cortex_m_rt::entry]
fn main() -> ! {
    MspM0CHal::<TestBoard>::boot();
}

#[panic_handler]
fn panic(panic: &core::panic::PanicInfo<'_>) -> ! {
    MspM0CHal::<TestBoard>::panic(panic)
}
