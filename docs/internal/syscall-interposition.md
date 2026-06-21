# Syscall Interposition Infrastructure

## Overview

The syscall interposition layer is a mechanism that allows applications running under Lind to intercept and handle system calls at the glibc level, before they reach the kernel. This enables transparent redirection of syscalls to alternative implementations, such as `rawposix` handlers.

## Components

### 1. Core Files

- **`syscall-interpose.h`**: Header file defining the interposition layer interface
  - Declares `__syscall_interpose()` function
  - Declares `__syscall_interpose_enabled` flag
  - Declares `__enable_syscall_interpose()` function to register handlers
  
- **`syscall-interpose.c`**: Implementation of the interposition layer
  - Provides the `__syscall_interpose()` function that can redirect syscalls
  - Manages the global interposition state
  - Includes architecture-specific raw syscall implementation

### 2. Integration Points

The interposition layer must be integrated at:

1. **Build System** (`Makefile`):
   - Add `syscall-interpose` to `sysdep_routines` to include it in the build

2. **Symbol Export** (`Versions`):
   - Export `__syscall_interpose`
   - Export `__syscall_interpose_enabled`
   - Export `__enable_syscall_interpose`

3. **Syscall Stubs** (architecture-specific):
   - Modify syscall implementations to call `__syscall_interpose()` when enabled
   - This varies by architecture and glibc version

## Applying Changes to Different glibc Versions

### Prerequisites

- Git repositories for both source and target glibc versions
- Understanding of the specific glibc version's internal structure
- Access to architecture-specific syscall implementations

### Step-by-Step Process

#### 1. Identify Target Version

```bash
cd glibc-target-version
git describe --tags --always
git branch -a | grep -i "v2.X\|glibc-2.X"
```

#### 2. Create a Branch for Changes

```bash
git checkout -b feature/syscall-interposition
```

#### 3. Add the Core Infrastructure Files

Copy the `syscall-interpose.c` and `syscall-interpose.h` files:

```bash
cp /path/to/reference/sysdeps/unix/sysv/linux/syscall-interpose.* \
   sysdeps/unix/sysv/linux/
```

#### 4. Update Makefile

Edit `sysdeps/unix/sysv/linux/Makefile`:

**Find the sysdep_routines section:**
```makefile
ifeq ($(subdir),misc)
sysdep_routines += ... splice \
```

**Add syscall-interpose:**
```makefile
ifeq ($(subdir),misc)
sysdep_routines += ... splice \
                   syscall-interpose \
```

#### 5. Update Versions

Edit `sysdeps/unix/sysv/linux/Versions`:

**Find the GLIBC_2.X section (usually after `__netlink_assert_response`):**
```
    # functions used by nscd
    __netlink_assert_response;
```

**Add new symbols:**
```
    # functions used by nscd
    __netlink_assert_response;
    # syscall interposition
    __syscall_interpose;
    __syscall_interpose_enabled;
    __enable_syscall_interpose;
```

#### 6. Architecture-Specific Integration (Optional)

To fully integrate interposition with syscall stubs, you need to modify architecture-specific files.

**For glibc versions using Assembly (.S files):**

You may need to add calls to `__syscall_interpose()` in key syscall files:
- `sysdeps/unix/sysv/linux/x86_64/clone.S`
- `sysdeps/unix/sysv/linux/x86_64/vfork.S`
- `sysdeps/unix/sysv/linux/x86_64/syscall.S`
- Similar files for other architectures (aarch64, arm, etc.)

**For glibc versions using C (.c files):**

The integration is simpler - modify the C implementations to call `__syscall_interpose()`.

#### 7. Build and Test

```bash
# Configure glibc for your system
mkdir build && cd build
../configure --prefix=/usr

# Build
make -j$(nproc)

# Optionally install to a custom prefix for testing
make install DESTDIR=/tmp/glibc-test

# Run basic tests
make test -k "test-1"
```

## Version-Specific Considerations

### glibc 2.35

- Uses assembly-based syscall implementations (.S files)
- Integration with assembly requires careful register handling
- See `/home/fwilke/edu/NYU/glibc-2.35` for a complete example

### glibc 2.39+

- Gradually converted to C-based implementations
- Easier to integrate with C-based interposition layer
- Some syscall implementations already ported to C

### glibc master/main

- Full C implementations of syscall stubs
- Direct integration with interposition layer
- See `/home/fwilke/edu/NYU/glibc-new` for the upstream implementation

## Architecture Support

The `raw_syscall()` function in `syscall-interpose.c` currently includes implementations for:

- **x86_64**: AMD64 architecture with System V AMD64 ABI

To support additional architectures, add corresponding `raw_syscall()` implementations:

```c
#ifdef __aarch64__
static inline long
raw_syscall(long number, long arg1, long arg2, long arg3,
            long arg4, long arg5, long arg6)
{
    register long x0 __asm__("x0") = arg1;
    // ... ARM64-specific implementation
}
#endif
```

## API Usage

### Enabling Interposition

```c
#include <syscall-interpose.h>

// Define a custom syscall handler
long int my_syscall_handler(long int number,
                            long int arg1, long int arg2, long int arg3,
                            long int arg4, long int arg5, long int arg6,
                            int nargs) {
    // Custom syscall handling logic
    printf("Intercepted syscall: %ld\n", number);
    // Return to default behavior or custom behavior
    return -1;  // ENOSYS
}

// Enable interposition
int main() {
    __enable_syscall_interpose(my_syscall_handler);
    // Syscalls will now be intercepted
    return 0;
}
```

### Disabling Interposition

```c
__syscall_interpose_enabled = 0;
```

## Debugging and Development

### Enable Debug Output

Modify `syscall-interpose.c` to add debug logging:

```c
#ifdef DEBUG_INTERPOSE
#include <stdio.h>
#define DEBUG_PRINT(fmt, ...) fprintf(stderr, fmt, ##__VA_ARGS__)
#else
#define DEBUG_PRINT(fmt, ...) do {} while(0)
#endif
```

### Common Issues

1. **Symbol Not Exported**: Check that all three symbols are listed in `Versions`
2. **Build Fails**: Ensure `syscall-interpose.c` is listed in `Makefile` before attempting to build
3. **Syscalls Not Intercepted**: Verify that `__syscall_interpose_enabled` is set to 1 and a handler is registered
4. **Architecture Mismatches**: Ensure the `raw_syscall()` implementation matches your target architecture

## References

- **glibc Source**: https://github.com/bminor/glibc
- **Lind Project**: https://github.com/Lind-Project
- **x86_64 System V ABI**: https://refspecs.linuxbase.org/elf/x86-64-abi-0.99.pdf

## Related Documentation

- [libc.md](./libc.md) - General glibc modifications for Lind
- [grates.md](./grates.md) - GRATES sandboxing system
- [rawposix.md](./rawposix.md) - RawPOSIX implementation

## Maintenance Notes

When updating the syscall interposition infrastructure:

1. Keep the interface in `syscall-interpose.h` stable
2. Test on multiple glibc versions if possible
3. Document any architecture-specific requirements
4. Update this guide with new version-specific information as needed
