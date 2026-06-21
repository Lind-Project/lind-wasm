# Applying Syscall Interposition to Different glibc Versions

This guide provides step-by-step instructions for applying the syscall interposition infrastructure to different versions of glibc.

## Quick Start

For a quick reference, here's the minimal set of changes needed:

```bash
# 1. Checkout target glibc version
git checkout glibc-2.X  # or your target version tag

# 2. Create feature branch
git switch -c add/syscall-interposition

# 3. Copy interposition files
cp /path/to/reference/glibc/sysdeps/unix/sysv/linux/syscall-interpose.* \
   sysdeps/unix/sysv/linux/

# 4. Apply patches (if available)
git apply /path/to/interposition.patch

# 5. Build and test
mkdir build && cd build && ../configure && make -j$(nproc)
```

## Detailed Walkthrough

### Setup

**Prerequisites:**
- glibc source tree for target version
- Reference implementation (e.g., from `lind-glibc` repository)
- Build tools: gcc, make, autoconf

**Recommended directory structure:**
```
glibc-work/
├── glibc-2.35/          # Target version
├── glibc-2.39/          # Another target version
├── glibc-new/           # Reference with latest changes
└── patches/             # Store generated patches
```

### Phase 1: Extract Changes as Patches

If you have a reference implementation with the changes already applied:

```bash
cd reference-glibc
git format-patch ORIGIN_TAG..HEAD -o ../patches/

# This creates numbered patch files like:
# 0001-basic-interposition-infra.patch
# 0002-tested-toggled-interposition.patch
```

### Phase 2: Apply to Target Version

```bash
cd target-glibc
git checkout -b feature/interposition

# Try automatic application first
git apply ../patches/*.patch

# If that fails, try with more flexibility:
git apply --reject --whitespace=fix ../patches/*.patch

# This will create *.rej files for failed hunks
```

### Phase 3: Manual Integration

If automatic patching fails (especially for older versions), manually apply changes:

#### Step 1: Add Core Files

```bash
# Copy the two main files
cp reference-glibc/sysdeps/unix/sysv/linux/syscall-interpose.{c,h} \
   target-glibc/sysdeps/unix/sysv/linux/

# Stage them
git add sysdeps/unix/sysv/linux/syscall-interpose.*
```

#### Step 2: Update Makefile

**Location:** `sysdeps/unix/sysv/linux/Makefile`

Find the line with `splice \` and add `syscall-interpose \` after it:

```diff
 ifeq ($(subdir),misc)
 sysdep_routines += ... splice \
+                  syscall-interpose \
                   sysctl \
```

**Tools to use:**
```bash
# Search for the location
grep -n "splice" sysdeps/unix/sysv/linux/Makefile

# Edit with your preferred editor
sed -i '/splice \\\/{a\                  syscall-interpose \\\
}' sysdeps/unix/sysv/linux/Makefile
```

#### Step 3: Update Versions

**Location:** `sysdeps/unix/sysv/linux/Versions`

Find the section with `__netlink_assert_response;` and add the new symbols:

```diff
     __sigtimedwait;
     # functions used by nscd
     __netlink_assert_response;
+    # syscall interposition
+    __syscall_interpose;
+    __syscall_interpose_enabled;
+    __enable_syscall_interpose;
```

**Tools to use:**
```bash
# Find line number
grep -n "__netlink_assert_response" sysdeps/unix/sysv/linux/Versions

# Use sed to insert lines
sed -i '/^    __netlink_assert_response;/a\    # syscall interposition\n    __syscall_interpose;\n    __syscall_interpose_enabled;\n    __enable_syscall_interpose;' \
    sysdeps/unix/sysv/linux/Versions
```

### Phase 4: Architecture-Specific Changes (Optional)

For full syscall interception, integrate with architecture-specific stubs:

#### For x86_64 Assembly Versions (glibc 2.35 and earlier)

You may want to add interposition calls to syscall wrapper functions. This requires modifying assembly files like:
- `sysdeps/unix/sysv/linux/x86_64/clone.S`
- `sysdeps/unix/sysv/linux/x86_64/vfork.S`
- `sysdeps/unix/sysv/linux/x86_64/syscall.S`

This is complex and version-specific. For most use cases, the basic infrastructure is sufficient.

#### For C-based Versions (glibc 2.39+)

If the version uses C instead of assembly, you can modify syscall wrappers to use the interposition layer more directly. Check files like:
- `sysdeps/unix/sysv/linux/x86_64/clone.c`
- `sysdeps/unix/sysv/linux/x86_64/vfork.c`

## Testing the Integration

### 1. Verify Files are in Place

```bash
ls -la sysdeps/unix/sysv/linux/syscall-interpose.*
```

### 2. Verify Makefile Update

```bash
grep syscall-interpose sysdeps/unix/sysv/linux/Makefile
```

### 3. Verify Versions Update

```bash
grep -A2 "syscall interposition" sysdeps/unix/sysv/linux/Versions
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

## Troubleshooting

### Build Fails: "syscall-interpose.c:X: error: ..."

**Problem:** Compilation errors in the interposition layer

**Solution:**
1. Check that all includes are available (particularly `<libc-symbols.h>`)
2. Verify architecture - the assembly inline code is x86_64 specific
3. Try adapting `raw_syscall()` for your target architecture

### Symbol Not Found at Runtime

**Problem:** "undefined symbol: __syscall_interpose"

**Solution:**
1. Verify Versions file has the symbols (see step 3 above)
2. Rebuild the libc.so with the changes
3. Check that old versions of libc aren't being linked

### Patch Fails to Apply

**Problem:** "patch does not apply"

**Solution:**
1. Check glibc version matches reference:
   ```bash
   git describe --tags --always
   ```
2. Look for *.rej files to see which hunks failed
3. Manually apply those sections
4. Or check if the target version already has similar changes

## Version-Specific Notes

### glibc 2.35

- **Status:** Fully supported (see `/home/fwilke/edu/NYU/glibc-2.35`)
- **Assembly-based:** Uses .S files for x86_64 syscalls
- **Integration:** Partial - infrastructure only, no assembly integration
- **Build:** Standard glibc build process

### glibc 2.36-2.38

- **Similar to 2.35**
- **Minor differences in Makefile/Versions**
- **Use same approach as 2.35**

### glibc 2.39+

- **Partially C-based:** Some syscalls converted to C
- **Easier integration:** Can modify C implementations directly
- **Recommended for new projects**

### glibc master (main branch)

- **Full C-based:** Complete C implementation of syscall stubs
- **See:** `/home/fwilke/edu/NYU/glibc-new` for full implementation
- **Direct integration:** Syscalls can call interposition layer directly

## Creating a Patch for Distribution

Once you've successfully applied changes to a version:

```bash
# Create a patch file
git format-patch origin/master.. --output-directory ../patches/

# Or for a single-commit change:
git show HEAD > ../patches/glibc-2.X-interposition.patch

# Create a patch bundle
git format-patch origin/master.. --cover-letter

# Apply later on another system:
git am ../patches/*.patch
```

## Integration with Lind Build System

To integrate an interposed glibc version into the Lind project:

1. Push changes to a feature branch in your glibc fork
2. Update Lind build configuration to use the new branch
3. Update documentation (this file) with version info
4. Run full integration tests

```bash
# Example in Lind build scripts:
GLIBC_URL="https://github.com/F-Wilke/lind-glibc.git"
GLIBC_BRANCH="v2.35-interposition"
GLIBC_TAG="v2.35"
```

## See Also

- [Syscall Interposition Infrastructure](./syscall-interposition.md) - Technical details
- [glibc Modifications](./libc.md) - General Lind glibc changes
- [Building glibc](https://sourceware.org/glibc/wiki/Setting%20up%20and%20testing%20glibc)
