//! Lind-IF §3: ABI conversions and field accessors.
//!
//! Conversions are an open family (`GetRuntimeXYZfromLindXYZ` / `GetLindXYZfromRuntimeXYZ`).
//! `AbiConvert` is the shape each structure implements; `LindIovec` is the first instance.
//! All inputs are supervisor-local buffers, so bogus lengths are reported as `EFAULT`.

use crate::{AbiDescriptor, GuestAddress, LindError, LindResult};
use sysdefs::constants::err_const::Errno;

pub trait AbiConvert: Sized {
    /// Size of the runtime representation for `abi`.
    fn runtime_size(abi: AbiDescriptor) -> usize;
    /// `GetRuntimeXYZfromLindXYZ`.
    fn to_runtime(&self, abi: AbiDescriptor, out: &mut [u8]) -> LindResult<()>;
    /// `GetLindXYZfromRuntimeXYZ`.
    fn from_runtime(abi: AbiDescriptor, input: &[u8]) -> LindResult<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindIovec {
    pub base: GuestAddress,
    pub len: u64,
}

const WASM32_IOVEC_SIZE: usize = 8;

fn efault() -> LindError {
    LindError::Other(Errno::EFAULT)
}

fn read_u32(input: &[u8], offset: usize) -> LindResult<u32> {
    let bytes = input.get(offset..offset + 4).ok_or_else(efault)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn narrow(value: u64) -> LindResult<u32> {
    u32::try_from(value).map_err(|_| LindError::Other(Errno::EOVERFLOW))
}

impl AbiConvert for LindIovec {
    fn runtime_size(abi: AbiDescriptor) -> usize {
        match abi {
            AbiDescriptor::Wasm32 => WASM32_IOVEC_SIZE,
        }
    }

    fn to_runtime(&self, abi: AbiDescriptor, out: &mut [u8]) -> LindResult<()> {
        match abi {
            AbiDescriptor::Wasm32 => {
                let out = out.get_mut(..WASM32_IOVEC_SIZE).ok_or_else(efault)?;
                out[..4].copy_from_slice(&narrow(self.base)?.to_le_bytes());
                out[4..].copy_from_slice(&narrow(self.len)?.to_le_bytes());
                Ok(())
            }
        }
    }

    fn from_runtime(abi: AbiDescriptor, input: &[u8]) -> LindResult<Self> {
        match abi {
            AbiDescriptor::Wasm32 => Ok(Self {
                base: read_u32(input, 0)? as u64,
                len: read_u32(input, 4)? as u64,
            }),
        }
    }
}

/// `SetIovecBase`: overwrite the base of entry `index` in a runtime-format iovec array.
/// `new_guest_address` is the runtime representation of the new base pointer.
pub fn set_iovec_base(
    abi: AbiDescriptor,
    iovec: &mut [u8],
    index: usize,
    new_guest_address: &[u8],
) -> LindResult<()> {
    match abi {
        AbiDescriptor::Wasm32 => {
            let start = index
                .checked_mul(WASM32_IOVEC_SIZE)
                .ok_or_else(efault)?;
            let base = new_guest_address.get(..4).ok_or_else(efault)?;
            iovec
                .get_mut(start..start + 4)
                .ok_or_else(efault)?
                .copy_from_slice(base);
            Ok(())
        }
    }
}

/// Offset of the base field of iovec entry `index`, used by the cross-cage `SetGuestIovecBase`
/// wrapper in Core.
pub fn iovec_base_offset(abi: AbiDescriptor, index: usize) -> LindResult<u64> {
    match abi {
        AbiDescriptor::Wasm32 => (index as u64)
            .checked_mul(WASM32_IOVEC_SIZE as u64)
            .ok_or_else(efault),
    }
}

/// Runtime representation of a guest pointer.
pub fn encode_guest_address(abi: AbiDescriptor, address: GuestAddress) -> LindResult<Vec<u8>> {
    match abi {
        AbiDescriptor::Wasm32 => Ok(narrow(address)?.to_le_bytes().to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm32_iovec_round_trips_and_rejects_short_buffers() {
        let iov = LindIovec { base: 0x1000, len: 7 };
        let mut buf = [0u8; 16];
        iov.to_runtime(AbiDescriptor::Wasm32, &mut buf).unwrap();
        assert_eq!(LindIovec::from_runtime(AbiDescriptor::Wasm32, &buf).unwrap(), iov);

        let new_base = encode_guest_address(AbiDescriptor::Wasm32, 0x2000).unwrap();
        set_iovec_base(AbiDescriptor::Wasm32, &mut buf, 1, &new_base).unwrap();
        assert_eq!(LindIovec::from_runtime(AbiDescriptor::Wasm32, &buf[8..]).unwrap().base, 0x2000);

        assert_eq!(
            LindIovec::from_runtime(AbiDescriptor::Wasm32, &buf[..6]),
            Err(LindError::Other(Errno::EFAULT))
        );
        assert_eq!(
            set_iovec_base(AbiDescriptor::Wasm32, &mut buf, 2, &new_base),
            Err(LindError::Other(Errno::EFAULT))
        );
        assert_eq!(
            LindIovec { base: 1 << 32, len: 0 }.to_runtime(AbiDescriptor::Wasm32, &mut buf),
            Err(LindError::Other(Errno::EOVERFLOW))
        );
    }
}
