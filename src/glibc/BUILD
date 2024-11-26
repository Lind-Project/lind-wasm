genrule(
    name = "make_all",
    srcs = [
        "wasm-config.sh",
        "configure"
    ],
    outs = ["check.log"],  # Output file
    cmd = """
        echo "test" > $@
        
        export GLIBC_BASE=$$WORKSPACE_ROOT/src/glibc

        export CLANG=$$WORKSPACE_ROOT/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04
        export CC=$$WORKSPACE_ROOT/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang

        echo $$GLIBC_BASE >> $@
        echo $$CLANG >> $@
        echo $$CC >> $@

        rm -rf $$GLIBC_BASE/build

        export BUILDDIR=$$GLIBC_BASE/build
        mkdir -p $$BUILDDIR
        # cd $$BUILDDIR
        # bash $$GLIBC_BASE/configure
        echo $$PWD >> $@
    """,
)
