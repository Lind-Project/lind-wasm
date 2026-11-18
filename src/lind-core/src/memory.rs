//! Memory mapping effects (Lind-IF §7). RawPOSIX owns the Vmmap bookkeeping and calls these
//! while holding the cage's Vmmap lock, so backends must not take that lock.

use crate::runtime::runtime_of;
use lind_runtime::{
    CageId, HostAddress, LindMmapArgs, LindMprotectArgs, LindMunmapArgs, LindResult,
};

pub fn do_mmap(cage: CageId, args: LindMmapArgs, is_reservation: bool) -> LindResult<HostAddress> {
    runtime_of(cage)?.do_mmap(cage, args, is_reservation)
}

pub fn do_mprotect(cage: CageId, args: LindMprotectArgs) -> LindResult<()> {
    runtime_of(cage)?.do_mprotect(cage, args)
}

pub fn do_munmap(cage: CageId, args: LindMunmapArgs) -> LindResult<()> {
    runtime_of(cage)?.do_munmap(cage, args)
}
