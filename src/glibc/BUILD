genrule(
    name = "make_all",
    srcs = [
        "wasm-config.sh",
        "configure"
    ],
    outs = ["check.log"],  # Output file
    cmd = """
        echo "test" > $@
        
        export GLIBC_BASE=$$PWD/src/glibc

        export CLANG=$$PWD/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04
        export CC=$$PWD/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang

        echo $$GLIBC_BASE >> $@
        echo $$CLANG >> $@
        echo $$CC >> $@


        export BUILDDIR=$$PWD/src/glibc/build
        rm -rf $$BUILDDIR        
        mkdir -p $$BUILDDIR
        
        ls $$PWD/src/glibc >> $@
        
        # cd $$BUILDDIR
        bash $$GLIBC_BASE/configure >> $@
        echo $$PWD >> $@
    """,
)
