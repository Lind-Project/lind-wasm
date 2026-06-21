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

### Recommended Approach: Git Cherry-Pick

The cleanest way to apply these changes is using `git cherry-pick`:

```bash
# Get the commit hashes from your reference implementation
cd reference-glibc
git log --oneline origin/master..HEAD
# Output:
# 5d435612f8 [add:] tested toggled interposition
# 922057b7e8 [add:] basic interposition infra

# Switch to target version branch
cd target-glibc
git checkout -b feature/syscall-interposition v2.35  # or your target version

# Cherry-pick the commits in order
git cherry-pick 922057b7e8 5d435612f8

# If conflicts occur, resolve them (see Conflict Resolution below)
# Then continue:
git cherry-pick --continue
```

### Step-by-Step Manual Application (If Cherry-Pick Fails)

#### 1. Identify Target Version

```bash
cd glibc-target-version
git describe --tags --always
git branch -a | grep -i "v2.X\|glibc-2.X"
```

#### 2. Create a Branch for Changes

```bash
git checkout -b feature/syscall-interposition v2.35
```

#### 3. Add the Core Infrastructure Files

Copy the `syscall-interpose.c` and `syscall-interpose.h` files:

```bash
cp /path/to/reference/sysdeps/unix/sysv/linux/syscall-interpose.* \
   sysdeps/unix/sysv/linux/
git add sysdeps/unix/sysv/linux/syscall-interpose.*
```

#### 4. Update Makefile

Edit `sysdeps/unix/sysv/linux/Makefile`:

Find the line with `splice \` and add `syscall-interpose \` after it:

```makefile
sysdep_routines += ... splice \
                   syscall-interpose \
```

#### 5. Update Versions

Edit `sysdeps/unix/sysv/linux/Versions`:

Find the section with `__netlink_assert_response;` and add the new symbols:

```
    # functions used by nscd
    __netlink_assert_response;
    # syscall interposition
    __syscall_interpose;
    __syscall_interpose_enabled;
    __enable_syscall_interpose;
```

#### 6. Convert Assembly to C (Optional but Recommended)

For full interposition support, convert key syscall implementations:

```bash
# Copy C implementations from reference
cp /path/to/reference/sysdeps/unix/sysv/linux/x86_64/clone.c \
   sysdeps/unix/sysv/linux/x86_64/
cp /path/to/reference/sysdeps/unix/sysv/linux/x86_64/clone3.c \
   sysdeps/unix/sysv/linux/x86_64/
cp /path/to/reference/sysdeps/unix/sysv/linux/x86_64/vfork.c \
   sysdeps/unix/sysv/linux/x86_64/

# Remove old assembly files
git rm sysdeps/unix/sysv/linux/x86_64/clone.S
git rm sysdeps/unix/sysv/linux/x86_64/clone3.S
git rm sysdeps/unix/sysv/linux/x86_64/vfork.S

# Update sysdep.h to use the C implementations
# (Usually simplify the macro definitions)
```

#### 7. Commit the Changes

```bash
git commit -m "[add:] syscall interposition infrastructure

This adds a syscall interposition layer that allows applications
to hook and intercept system calls at the glibc level.

Changes:
- Add syscall-interpose.c and .h for interposition layer
- Update Makefile to include syscall-interpose in build
- Export interposition symbols in Versions
- Convert x86_64 syscall stubs from assembly to C"
```

### Conflict Resolution

If `git cherry-pick` fails with conflicts, you'll need to resolve them:

```bash
# See which files have conflicts
git status

# For files deleted in the patch but present in your version:
git rm sysdeps/unix/sysv/linux/x86_64/clone.S  # etc.

# For files with merge conflicts:
# - Edit the file and fix the conflict markers
# - Or accept the incoming version from the patch:
git checkout --theirs sysdeps/unix/sysv/linux/Makefile

# Then stage and continue:
git add <resolved-files>
git cherry-pick --continue
```

## Testing the Integration

### 1. Verify Files are in Place

```bash
ls -la sysdeps/unix/sysv/linux/syscall-interpose.*
ls -la sysdeps/unix/sysv/linux/x86_64/clone.c
```

### 2. Verify Makefile Update

```bash
grep syscall-interpose sysdeps/unix/sysv/linux/Makefile
```

### 3. Verify Versions Update

```bash
grep -A3 "syscall interposition" sysdeps/unix/sysv/linux/Versions
```

### 4. Build Test

```bash
mkdir build
cd build

# Configure for current system
../configure

# Build
make -j$(nproc) 2>&1 | tee build.log

# Check for errors
grep -i "error" build.log

# Quick sanity check
make test -k "test-1"
```

### 5. Symbol Verification

After installation, verify symbols are exported:

```bash
nm /path/to/libc.so | grep __syscall_interpose

# Should show something like:
# 00000000001234 T __enable_syscall_interpose
# 00000000005678 B __syscall_interpose_enabled
# 0000000000abcd T __syscall_interpose
```

## Version-Specific Considerations

### glibc 2.35

- Uses assembly-based syscall implementations (.S files)
- See `/home/fwilke/edu/NYU/glibc-2.35` for a complete example
- Cherry-pick approach works well with conflict resolution for assembly→C conversion

### glibc 2.36-2.38

- Similar to 2.35
- Minor differences in Makefile/Versions locations
- Use same cherry-pick approach as 2.35

### glibc 2.39+

- Gradually converted to C-based implementations
- Easier to integrate with C-based interposition layer
- Some syscall implementations already ported to C

### glibc master/main

- Full C-based implementations of syscall stubs
- Complete interposition infrastructure already integrated
- See `/home/fwilke/edu/NYU/glibc-new` for the full implementation

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

## Exporting Changes as Patches

Once you've successfully applied changes to a version, you can create patches for distribution:

```bash
# Create patch files from commits
git format-patch ORIGINAL_TAG..HEAD -o ../patches/

# Or create a single patch
git diff ORIGINAL_TAG..HEAD > ../patches/glibc-2.X-interposition.patch

# Apply later on another system:
git apply ../patches/glibc-2.X-interposition.patch
# Or:
git am ../patches/0001-*.patch ../patches/0002-*.patch
```

## References

- **glibc Source**: https://github.com/bminor/glibc
- **Lind Project**: https://github.com/Lind-Project
- **x86_64 System V ABI**: https://refspecs.linuxbase.org/elf/x86-64-abi-0.99.pdf

## Related Documentation

- [glibc Modifications](./libc.md) - General glibc modifications for Lind
- [grates.md](./grates.md) - GRATES sandboxing system
- [rawposix.md](./rawposix.md) - RawPOSIX implementation

## Maintenance Notes

When updating the syscall interposition infrastructure:

1. Keep the interface in `syscall-interpose.h` stable
2. Test on multiple glibc versions if possible
3. Document any architecture-specific requirements
4. Update this guide with new version-specific information as needed
5. Use `git cherry-pick` for applying to new versions
