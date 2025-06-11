# File Descriptor Table Library For Lind

This library is meant to be used by implementers of a grate or microvisor for the Lind project.  The purpose of this library is to make it easy
for implementers to manage virtual -> real file descriptor mappings.  This library handles most of the complexity of that management so that
the grate / microvisor implementer can focus on functionality specific to their use case.

# Getting started

After cloning the repository, the usual commands will work.  

* `cargo build` -- Build the software with the default implementation
* `cargo test` -- Run the unit test and documentation tests
* `cargo doc` -- Build the documentation for the project
* `cargo bench` -- Run the criterion benchmarks on the current implementation.
* `cargo clippy` -- Should not complain.
* `cargo fmt` -- Should do nothing, since the code should match the desired style already.

There are also multiple algorithms supported.  To change the algorithm, copy the file from `impl_macros` and overwrite `src/current_impl` with
the desired implementation.  For example, to change to the `vanilla` algorithm do: `cp impl_macros/vanilla src/current_impl`.

If you want to test multiple implementations, there is a script `run_all` which will swap out the implementation for you and iterate through 
all copies.  Simply type something like `./run_all cargo test` to run the unit tests on all implementations.

To make a pretty benchmark comparison table, install criterion-table and run the following:
```
./run_all -o cargo criterion --message-format=json
cat target/*.out | criterion-table > BENCHMARKS.md
```

Then open BENCHMARKS.md to see the results.  It is in Github markdown format, so is best viewed there.

# An example use for fdtables
The fundamental problem which this solves is ensuring that different cages must have distinct file descriptor mappings.  In other words, suppose
you have two cages, A and B that communicate over a pipe.  For example, let's look at the pipeline: `grep foo myfile | wc`.  These cages will 
have distinct file descriptors so that `wc` will not have access to the file descriptor for `myfile` which is open by `grep`.  Similarly, the 
pipe between `grep`'s STDOUT and `wc`'s STDIN does not impact those file descriptors in the other cages.   

As a result, each cage will have a set of *virtual file descriptors* that translate into *real file descriptors* by the fdtables library.  This 
makes it so that different cages can use the same file descriptor number (their virtual fd) while referring to different real file descriptors.
Note also, that there is not a requirement that there is a real fd underlying every single virtual fd (e.g., for an in-memory pipe implementation
or a virtualized file system.)  In this case, a special value `NO_REAL_FD` signals this isn't backed by a real file descriptor.

Returning to our `grep foo myfile | wc` example, it plausibly would have a fdtable with entries that look like this:

| cage_id | virtualfd | realfd |
| --- | --- | --- |
| `grep` | `STDIN` (0) | `STDIN` (0) |
| `grep` | `STDOUT` (1) | `NO_REAL_FD` |
| `grep` | `STDERR` (2) | `STDERR` (2) |
| `grep` | "myfile" (3) | 4 |
| `wc` | `STDIN` (0) | `NO_REAL_FD` |
| `wc` | `STDOUT` (1) | `STDOUT` (1) |
| `wc` | `STDERR` (2) | `STDERR` (2) |

In this example, both `grep` and `wc` have their `STDERR` writing to the same realfd underneath.  Notice that `grep`'s open "myfile" is only
accessible by it because there isn't a virtual fd entry in the table for `wc`.  

### optionalinfo

Sometimes a library may want to track more information than what is in the table for a fd.  For example, if we want to have more than one pipe 
open, we may which to indicate which pipe buffer is being referred to.  To do this an `optionalinfo` field also exists.  One can use this to 
store a value to use to uniquely identify some aspect of the fd entry.

### should_cloexec

The library does track the `CLOEXEC` flag for each fd.  This makes handling exec much easier for the library user.  For more details, see the 
project documentation.

