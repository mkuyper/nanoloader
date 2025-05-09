use moonbow_macros::RegBlock;
use super::*;

#[derive(RegBlock)]
#[base(0xe000e000)]
#[size(0x1000)]
pub struct SCS {
    #[offset(0xd08)]
    vtor: Register,
}
