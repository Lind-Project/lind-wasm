# Applying Syscall Interposition to Different glibc Versions

This guide provides step-by-step instructions for applying the syscall interposition infrastructure to different versions of glibc, using the recommended `git cherry-pick` approach.

## Quick Start

For a quick reference, here's the fastest way to apply all changes:

```bash
# 1. Identify the commits with your changes
cd /path/to/reference-glibc  # e.g., glibc-new
git log --oneline origin/master..HEAD
# Note the commit hashes (e.g., 922057b7e8, 5d435612f8)

# 2. Checkout target glibc version
cd /path/to/target-glibc
git checkout -b feature/interposition v2.35  # or your target tag

# 3. Cherry-pick the commits
git cherry-pick 922057b7e8 5d435612f8

# 4. If conflicts occur, resolve them (see section below)
# Then continue with:
git cherry-pick --continue

# 5. Verify and build
make -j$(nproc)
```

## Why Git Cherry-Pick?

Using `git cherry-pick` is better than manually applying patches because:

- **Preserves commit metadata**: Author, date, original commit message
- **Automatic merge detection**: Git automatically merges non-conflicting changes
- **Clear conflict resolution**: Conflicts are clearly marked for manual review
- **Easy continuation**: Resume with `--continue` after resolving conflicts
- **Maintains history**: Clean commit history in your branch

## Detailed Walkthrough

### Setup

**Prerequisites:**
- glibc source trees for both reference and target versions
- Commits with the changes ready (e.g., in `origin/master..HEAD`)
- Git configured locally

**Recommended directory structure:**
```
glibc-work/
├── glibc-new/           # Reference with latest changes
├── glibc-2.35/          # Target version
├── glibc-2.39/          # Another target version
└── patches/             # For exporting patches (optional)
```

### Phase 1: Identify Commits

In your reference glibc repository, identify the commits:

```bash
cd glibc-new
git log --oneline origin/master..HEAD

# Output example:
# 5d435612f8 [add:] tested toggled interposition
# 922057b7e8 [add:] basic interposition infra
```

Note these commit hashes - you'll use them in Phase 2.

### Phase 2: Prepare Target Repository

```bash
cd target-glibc

# Verify you're at the correct base version
git describe --tags --always

# Create a feature branch
git checkout -b feature/syscall-interposition v2.35
# (or use your target version tag/commit)
```

### Phase 3: Cherry-Pick Commits

```bash
# Apply commits in order (oldest first)
git cherry-pick 922057b7e8 5d435612f8
```

**What happens next:**

**Case A: No conflicts**
```
[feature/syscall-interposition abc1234] [add:] basic interposition infra
 Date: Tue Jun 16 04:08:40 2026 +0200
 12 files changed, 838 insertions(+), 518 deletions(-)
Auto-merging ...
[feature/syscall-interposition def5678] [add:] tested toggled interposition
 Date: Tue Jun 16 23:51:38 2026 +0200
 2 files changed, 45 insertions(+), 35 deletions(-)
```

You're done! Both commits applied cleanly.

**Case B: Conflicts detected**

If there are conflicts, see the **Conflict Resolution** section below.

### Phase 4: Verify Integration

After cherry-pick completes (with or without conflicts):

```bash
# Verify commits are in place
git log --oneline -3
# Should show your 2 new commits on top of the base version

# Verify key files exist
ls -la sysdeps/unix/sysv/linux/syscall-interpose.{c,h}
ls -la sysdeps/unix/sysv/linux/x86_64/{clone,clone3,vfork}.c

# Check for exported symbols
grep "syscall_interpose" sysdeps/unix/sysv/linux/Versions
```

### Phase 5: Build and Test

```bash
mkdir build
cd build

# Configure
../configure --prefix=/tmp/glibc-test

# Build (may take several minutes)
make -j$(nproc)

# Optional: run tests
make test -k "test-1"  # quick sanity check
```

## Conflict Resolution

Cherry-pick may fail if the target version has significant differences from the reference. Here's how to handle it:

### Understanding Conflicts

```
error: could not apply 922057b7e8... [add:] basic interposition infra
hint: After resolving the conflicts, mark them with
hint:   "git add/rm <pathspec>"
hint: then run "git cherry-pick --continue"
```

This means some files have conflicts that Git couldn't automatically merge.

### Common Conflict Scenarios

**1. File Deleted in Target but Modified in Patch**

For example, if your target v2.35 doesn't have `syscall_cancel.c`:

```bash
# Git will show this conflict
# Resolution: remove the file (patch intent is to remove it anyway)
git rm sysdeps/unix/sysv/linux/syscall_cancel.c
```

**2. Assembly Files vs. C Files**

Your patch converts assembly (.S) to C (.c):

```bash
# Remove the assembly files as patch intends
git rm sysdeps/unix/sysv/linux/x86_64/clone.S
git rm sysdeps/unix/sysv/linux/x86_64/clone3.S
git rm sysdeps/unix/sysv/linux/x86_64/vfork.S

# The new .c files should already be added by the patch
```

**3. Text Content Conflicts (Makefile, Versions)**

These files have conflict markers:

```makefile
<<<<<<< HEAD
  splice \
  sysctl \
=======
  splice \
  syscall-interpose \
  sysctl \
>>>>>>> 922057b7e8
```

Resolution options:

**Option A: Accept the incoming patch version**
```bash
git checkout --theirs sysdeps/unix/sysv/linux/Makefile
git checkout --theirs sysdeps/unix/sysv/linux/Versions
```

**Option B: Manually merge**
```bash
# Edit the files with your editor, remove conflict markers:
# Keep the lines you want from both sides
vim sysdeps/unix/sysv/linux/Makefile
```

**Option C: Use diff to understand both versions**
```bash
# See what HEAD (target) has
git show HEAD:sysdeps/unix/sysv/linux/Makefile > /tmp/makefile.target

# See what the patch wants
git show 922057b7e8:sysdeps/unix/sysv/linux/Makefile > /tmp/makefile.patch

# Use a merge tool or editor to combine them
diff -u /tmp/makefile.target /tmp/makefile.patch
```

### Completing the Cherry-Pick

After resolving all conflicts:

```bash
# Stage the resolved files
git add sysdeps/unix/sysv/linux/Makefile
git add sysdeps/unix/sysv/linux/Versions
# ... etc for all resolved files

# Stage deletions
git rm sysdeps/unix/sysv/linux/x86_64/clone.S
# ... etc for all deleted files

# Continue the cherry-pick
git cherry-pick --continue

# You can optionally edit the commit message
# (or use --no-edit to keep the original)
```

The cherry-pick will complete and you'll be back to `feature/syscall-interposition` branch.

### Aborting a Cherry-Pick

If you make a mistake and want to start over:

```bash
git cherry-pick --abort

# You'll be back to the state before cherry-pick started
git reset --hard origin/master  # or whatever your safe point is
```

## Testing Different Versions

### glibc 2.35

```bash
git checkout -b v2.35-interpose glibc-2.35
git cherry-pick 922057b7e8 5d435612f8
# Expected: minor conflicts in Makefile/Versions and assembly→C conversion
```

### glibc 2.39+

```bash
git checkout -b v2.39-interpose glibc-2.39
git cherry-pick 922057b7e8 5d435612f8
# Expected: fewer conflicts as many files already use C implementations
```

### glibc master/main

```bash
git checkout -b main-interpose main
git cherry-pick 922057b7e8 5d435612f8
# Expected: minimal conflicts (master may already have similar changes)
```

## Exporting Your Changes

Once you've successfully applied to a version, you can create patches for others:

```bash
# Export as patch files
git format-patch ORIGINAL_TAG..HEAD -o ../patches/

# Or as a single patch
git diff ORIGINAL_TAG..HEAD > ../patches/glibc-2.35-interposition.patch

# Share the patch or push the branch to your fork
git push origin feature/syscall-interposition
```

## Applying Exported Patches

If someone sends you a patch file:

```bash
# Apply the patch
git apply /path/to/glibc-2.35-interposition.patch

# Or if it's a formatted patch:
git am /path/to/0001-*.patch /path/to/0002-*.patch
```

## Automation Script

You can create a script to apply to multiple versions:

```bash
#!/bin/bash
REFERENCE="~/edu/NYU/glibc-new"
COMMITS="922057b7e8 5d435612f8"
VERSIONS=("glibc-2.35" "glibc-2.39" "glibc-2.40")

for version in "${VERSIONS[@]}"; do
    echo "Processing $version..."
    git clone --depth 1 --branch "$version" \
        https://github.com/bminor/glibc.git "glibc-$version"
    cd "glibc-$version"
    git checkout -b feature/interposition
    git cherry-pick $COMMITS
    if [ $? -eq 0 ]; then
        echo "✓ $version: Success"
    else
        echo "✗ $version: Conflicts - manual resolution needed"
    fi
    cd ..
done
```

## Troubleshooting

### Cherry-Pick Hangs or Takes Forever

This usually means it's prompting for a merge commit message:

```bash
# Press Ctrl+C to stop
# Then use --no-edit to auto-complete
git cherry-pick --abort
git cherry-pick 922057b7e8 --no-edit
git cherry-pick 5d435612f8 --no-edit
```

### "Your branch is ahead of 'origin/master' by X commits"

This is normal after cherry-picking. Your local branch has the new commits but origin doesn't yet.

### "fatal: cherry-pick failed"

This means the cherry-pick couldn't even start (e.g., the commit doesn't exist). Verify:

```bash
# Commit exists in your repo?
git log --oneline | grep 922057b7e8

# In the right repo?
pwd
git remote -v
```

### Build Fails After Cherry-Pick

```bash
# Make sure you resolved all conflicts
git status  # Should show nothing (clean working directory)

# Clean build directory
rm -rf build/
mkdir build && cd build
../configure
make -j$(nproc)
```

## Integration with Lind Build System

To integrate an interposed glibc version into the Lind project:

1. Push changes to your fork:
   ```bash
   git push origin feature/syscall-interposition
   ```

2. In Lind build configuration, reference the branch:
   ```bash
   GLIBC_URL="https://github.com/YOUR_USERNAME/glibc.git"
   GLIBC_BRANCH="feature/syscall-interposition"
   GLIBC_TAG="v2.35"
   ```

3. Tag the commit for release:
   ```bash
   git tag -a v2.35-with-interpose -m "glibc 2.35 with syscall interposition"
   git push origin v2.35-with-interpose
   ```

## See Also

- [Syscall Interposition Infrastructure](./syscall-interposition.md) - Technical details
- [glibc Modifications](./libc.md) - General Lind glibc changes
- [Building glibc](https://sourceware.org/glibc/wiki/Setting%20up%20and%20testing%20glibc)
- [Git Cherry-Pick Documentation](https://git-scm.com/docs/git-cherry-pick)
