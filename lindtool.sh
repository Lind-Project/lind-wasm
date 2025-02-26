#!/bin/bash
LIND_WASM_BASE="${LIND_WASM_BASE:-/home/lind/lind-wasm}"

glibc_base="$LIND_WASM_BASE/src/glibc"
wasmtime_base="$LIND_WASM_BASE/src/wasmtime"
rustposix_base="$LIND_WASM_BASE/src/safeposix-rust"
rawposix_base="$LIND_WASM_BASE/src/RawPOSIX"

wasm_opt_base="$LIND_WASM_BASE/tools/binaryen/bin/wasm-opt"

CC="${CLANG:=/home/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04}/bin/clang"

export_cmd="export LD_LIBRARY_PATH=$wasmtime_base/crates/rustposix:\$LD_LIBRARY_PATH"

compile_test_cmd_fork="$CC --target=wasm32-unknown-wasi --sysroot $glibc_base/sysroot -Wl,--export="__stack_pointer",--export=__stack_low [input] -g -O0 -o [output] && $wasm_opt_base --epoch-injection --asyncify --debuginfo [output] -o [output]"
compile_test_cmd_noshared="$CC --target=wasm32-unknown-wasi --sysroot $glibc_base/sysroot -Wl,--export="__stack_pointer",--export=__stack_low [input] -g -O0 -o [output]"
compile_test_cmd="$CC -pthread --target=wasm32-unknown-wasi --sysroot $glibc_base/sysroot -Wl,--import-memory,--export-memory,--max-memory=67108864,--export=__stack_low [input] -g -O0 -o [output]"
precompile_wasm="$wasmtime_base/target/debug/wasmtime compile [input] -o [output]"

compile_test_cmd_fork_test="$CC -pthread --target=wasm32-unknown-wasi --sysroot $glibc_base/sysroot -Wl,--import-memory,--export-memory,--max-memory=67108864,--export="__stack_pointer",--export=__stack_low [input] -g -O0 -o [output] && $wasm_opt_base --epoch-injection --asyncify --debuginfo [output] -o [output]"

run_cmd="$wasmtime_base/target/debug/wasmtime run --wasi threads=y --wasi preview2=n [target]"
run_cmd_precompile="$wasmtime_base/target/debug/wasmtime run --allow-precompiled --wasi threads=y --wasi preview2=n [target]"
run_cmd_debug="gdb --args $wasmtime_base/target/debug/wasmtime run -D debug-info -O opt-level=0 --wasi threads=y --wasi preview2=n [target]"

compile_wasmtime_cmd="cd $wasmtime_base && cargo build"
compile_rustposix_cmd="cd $rustposix_base && cargo build && cp $rustposix_base/target/debug/librustposix.so $wasmtime_base/crates/rustposix"
compile_rawposix_cmd="cd $rawposix_base && cargo build && cp $rawposix_base/target/debug/librustposix.so $wasmtime_base/crates/rustposix"

compile_binaryen_cmd="cd $binaryen_base && cmake . && make"

compile_pthread_create="$CC --target=wasm32-unkown-wasi -v -Wno-int-conversion pthread_create.c -c -std=gnu11 -fgnu89-inline  -matomics -mbulk-memory -O0 -g -Wall -Wwrite-strings -Wundef -fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -Wstrict-prototypes -Wold-style-definition -fmath-errno    -fPIE     -ftls-model=local-exec     -I../include -I$glibc_base/build/nptl  -I$glibc_base/build  -I../sysdeps/lind  -I../lind_syscall  -I../sysdeps/unix/sysv/linux/i386/i686  -I../sysdeps/unix/sysv/linux/i386  -I../sysdeps/unix/sysv/linux/x86/include -I../sysdeps/unix/sysv/linux/x86  -I../sysdeps/x86/nptl  -I../sysdeps/i386/nptl  -I../sysdeps/unix/sysv/linux/include -I../sysdeps/unix/sysv/linux  -I../sysdeps/nptl  -I../sysdeps/pthread  -I../sysdeps/gnu  -I../sysdeps/unix/inet  -I../sysdeps/unix/sysv  -I../sysdeps/unix/i386  -I../sysdeps/unix  -I../sysdeps/posix  -I../sysdeps/i386/fpu  -I../sysdeps/x86/fpu  -I../sysdeps/i386  -I../sysdeps/x86/include -I../sysdeps/x86  -I../sysdeps/wordsize-32  -I../sysdeps/ieee754/float128  -I../sysdeps/ieee754/ldbl-96/include -I../sysdeps/ieee754/ldbl-96  -I../sysdeps/ieee754/dbl-64  -I../sysdeps/ieee754/flt-32  -I../sysdeps/ieee754  -I../sysdeps/generic  -I.. -I../libio -I. -nostdinc -isystem $CLANG/lib/clang/16/include -isystem /usr/i686-linux-gnu/include -D_LIBC_REENTRANT -include $glibc_base/build/libc-modules.h -DMODULE_NAME=libc -include ../include/libc-symbols.h  -DPIC     -DTOP_NAMESPACE=glibc -o $glibc_base/build/nptl/pthread_create.o -MD -MP -MF $glibc_base/build/nptl/pthread_create.o.dt -MT $glibc_base/build/nptl/pthread_create.o"
compile_wasi_thread_start="$CC --target=wasm32-wasi-threads -matomics -o $glibc_base/build/csu/wasi_thread_start.o -c $glibc_base/csu/wasm32/wasi_thread_start.s"
make_cmd="cd $glibc_base && rm -rf build && ./wasm-config.sh && cd build && make -j8 --keep-going 2>&1 THREAD_MODEL=posix | tee check.log && cd ../nptl && $compile_pthread_create && cd ../ && $compile_wasi_thread_start && ./gen_sysroot.sh"

RED='\033[31m'
GREEN='\033[32m'
RESET='\033[0m'

split_path() {
    local full_path="$1"
    local dir_var="$2"
    local file_var="$3"
    
    # Extract the directory path
    local dir_path="${full_path%/*}"
    
    # Extract the file name
    local file_name="${full_path##*/}"
    
    # If the file is in the current directory, adjust dir_path to '.'
    [ "$dir_path" = "$full_path" ] && dir_path="."
    
    # Assign the values back to the referenced variables
    eval "$dir_var='$dir_path'"
    eval "$file_var='$file_name'"
}

compile_src() {
    echo -e "${GREEN}cd $glibc_base/build${RESET}"
    cd $glibc_base/build

    target=""
    directory=""

    split_path $1 directory target

    escaped_target=$(echo "$target" | sed 's/[.[*+?^${}()|\\]/\\&/g')

    echo -e "${GREEN}cat check.log | grep -E '[ \/]$escaped_target' | grep -E '\-o[[:space:]]+[^[:space:]]+\.o[[:space:]]' | grep '\-\-target=wasm32\-unkown\-wasi'${RESET}"
    results=$(eval "cat check.log | grep -E '[ \/]$escaped_target' | grep -E '\-o[[:space:]]+[^[:space:]]+\.o[[:space:]]' | grep '\-\-target=wasm32\-unkown\-wasi'")

    if [ -z "$results" ]; then
        echo -e "${RED}error: compile command not found${RESET}"
        exit 1
    fi

    echo -e "${GREEN}cd $glibc_base/$directory${RESET}"
    if [ "$pmode" -eq 0 ]; then
        cd $glibc_base/$directory
    fi

    echo $results | while read -r line ; do
        if [ "$2" = "-O0" ]; then
            line=$(echo "$line" | sed "s| -O2 | -O0 |")
        elif [ -n "$2" ]; then
            echo "Warming: $2 option is not used"
        fi

        echo -e "${GREEN}$line${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$line"
            if [ $? -ne 0 ]; then
                return 1
            fi
        fi
    done

    if [ $? -ne 0 ]; then
        return 1
    fi

    gen_sysroot="cd $glibc_base && ./gen_sysroot.sh > /dev/null"
    echo -e "${GREEN}$gen_sysroot${RESET}"
    if [ "$pmode" -eq 0 ]; then
        eval "$gen_sysroot"
    fi
}

pmode=0
if [ "${!#}" = "-p" ]; then
    pmode=1

    # Remove the last argument
    new_args=("${@:1:$#-1}")
    # Overwrite the positional parameters with the new arguments
    set -- "${new_args[@]}"
fi

case $1 in
    export)
        echo -e "${GREEN}$export_cmd${RESET}"
        echo "please manually run the export command"
        ;;
    compile_test|cptest)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi

        source_file_c="$2.c"

        if [ -z "$3" ]; then
            output_file_wasm="$2.wasm"
            output_file_cwasm="$2.cwasm"
        else
            output_file_wasm="$3.wasm"
            output_file_cwasm="$3.cwasm"
        fi

        eval $export_cmd
        
        final_cmd=$(echo "$compile_test_cmd_fork_test" | sed "s|\[input\]|$source_file_c|g" | sed "s|\[output\]|$output_file_wasm|g")
        pre_compile=$(echo "$precompile_wasm" | sed "s|\[input\]|$output_file_wasm|g" | sed "s|\[output\]|$output_file_cwasm|g")

        final_cmd="${final_cmd} && ${pre_compile}"

        echo -e "${GREEN}$final_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$final_cmd"
        fi
        ;;
    run)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi

        target_wasm="$2.wasm"
        target_cwasm="$2.cwasm"

        shift 2

        if [ -e "$target_cwasm" ]; then
            final_cmd=$(echo "$run_cmd_precompile" | sed "s|\[target\]|$target_cwasm|")
            final_cmd="${final_cmd} $@"
        else
            final_cmd=$(echo "$run_cmd" | sed "s|\[target\]|$target_wasm|")
            final_cmd="${final_cmd} $@"
        fi

        eval $export_cmd

        echo -e "${GREEN}$final_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$final_cmd"
        fi
        ;;
    debug)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi

        target="$2"

        shift 2

        final_cmd=$(echo "$run_cmd_debug" | sed "s|\[target\]|$target|")
        final_cmd="${final_cmd} $@"

        eval $export_cmd

        echo -e "${GREEN}$final_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$final_cmd"
        fi
        ;;
    compile_wasmtime|cpwasm)
        echo -e "${GREEN}$compile_wasmtime_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$compile_wasmtime_cmd"
        fi
        ;;
    compile_rustposix|cpposix)
        echo -e "${GREEN}$compile_rustposix_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$compile_rustposix_cmd"
        fi
        ;;
    compile_rawposix|cpraw)
        echo -e "${GREEN}$compile_rawposix_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$compile_rawposix_cmd"
        fi
        ;;
    compile_src|cpsrc)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi

        compile_src $2 $3
        ;;
    make_all)
        unset LD_LIBRARY_PATH
        echo -e "${GREEN}$make_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$make_cmd"
        fi
        ;;
    make)
        result=$(lindmake "$glibc_base" "$pmode")
        if [ -z "$result" ]; then
            echo -e "${GREEN}No changes detected.${RESET}"
            exit 0
        fi
        echo $result

        # Set IFS to semicolon and read the string into an array
        IFS=';' read -r -a array <<< "$result"

        compiled=()

        # Print each element of the array
        for line in "${array[@]}"; do
            # Split the string into two variables
            if [[ "$line" =~ ^([^[:space:]]+)([[:space:]]+)(.+)$ ]]; then
                src_file=${BASH_REMATCH[1]}
                compiled+=($src_file)
                compile_src $src_file ${BASH_REMATCH[3]}

                if [ $? -ne 0 ]; then
                    echo -e "${RED}Compilation Failed.${RESET}"
                    exit 1
                fi
            else
                compile_src $line

                if [ $? -ne 0 ]; then
                    echo -e "${RED}Compilation Failed.${RESET}"
                    exit 1
                fi
            fi
        done

        echo ""
        for file in "${compiled[@]}"; do
            echo -e "${GREEN}Compiled $file successfully.${RESET}"
        done

        ;;
    help)
        echo "avaliable commands are:"
        echo "1. export"
        echo "2. compile_test"
        echo "3. run"
        echo "4. compile_wasmtime"
        echo "5. compile_rustposix"
        echo "6. compile_src"
        echo "7. make_all"
        echo "8. make"
        ;;
    *)
        echo "Unknown command identifier."
        ;;
esac