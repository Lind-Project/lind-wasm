use crate::runtime::vm::{SharedMemory, VMMemoryImport};

/// The value of an export passed from one instance to another.
pub enum Export {
    /// A function export value.
    Function(crate::Func),

    /// A table export value.
    Table(crate::Table),

    /// An unshared memory export value.
    Memory(crate::Memory),

    /// A shared memory export value.
    SharedMemory(SharedMemory, VMMemoryImport),

    /// A global export value.
    Global(crate::Global),

    /// A tag export value.
    Tag(crate::Tag),
}

pub enum ExportMemory {
    Unshared(crate::Memory),
    Shared(SharedMemory, VMMemoryImport),
}

impl ExportMemory {
    pub fn unshared(self) -> Option<crate::Memory> {
        match self {
            ExportMemory::Unshared(m) => Some(m),
            ExportMemory::Shared(..) => None,
        }
    }
    pub fn shared(self) -> Option<SharedMemory> {
        match self {
            ExportMemory::Unshared(_) => None,
            ExportMemory::Shared(m, _) => Some(m),
        }
    }

    /// Returns the base pointer of the memory without consuming self.
    /// For shared memory the pointer is read directly from VMMemoryDefinition;
    /// for unshared memory None is returned (caller must use data_ptr with a store).
    pub fn shared_base_ptr(&self) -> Option<*mut u8> {
        match self {
            ExportMemory::Unshared(_) => None,
            ExportMemory::Shared(vm, _) => {
                Some(unsafe { (*vm.vmmemory_ptr().as_ptr()).base.as_ptr() })
            }
        }
    }
}
