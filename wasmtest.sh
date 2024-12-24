#!/bin/bash
glibc_base="/home/lind-wasm/src/glibc"
wasmtime_base="/home/lind-wasm/src/wasmtime"

CC="${CLANG:=/home/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04}/bin/clang"

export_cmd="export LD_LIBRARY_PATH=$wasmtime_base/crates/rustposix:\$LD_LIBRARY_PATH"

compile_test_cmd_fork_test="$CC -pthread --target=wasm32-unknown-wasi --sysroot $glibc_base/sysroot -Wl,--import-memory,--export-memory,--max-memory=67108864,--export="__stack_pointer" [input] -g -O0 -o [output] && wasm-opt --asyncify --debuginfo [output] -o [output]"
precompile_wasm="$wasmtime_base/target/debug/wasmtime compile [input] -o [output]"
run_cmd_precompile="$wasmtime_base/target/debug/wasmtime run --allow-precompiled --wasi threads=y --wasi preview2=n [target]"

test_file_base="/home/lind-wasm/tests/unit-tests"

RED='\033[31m'
GREEN='\033[32m'
RESET='\033[0m'

compile_single_test() {
    local test_name="$1"

    source_file=$(find "$test_file_base" -type f -name "${test_name}" 2>/dev/null | head -n 1)

    if [ -z "$source_file" ]; then
        echo -e "${RED}Error: Source file ${test_name} not found in $test_file_base.${RESET}"
        exit 1
    fi

    output_dir=$(dirname "$source_file")
    without_c="${test_name%??}"
    output_file_wasm="${output_dir}/${twithout_c}.wasm"
    output_file_cwasm="${output_dir}/${without_c}.cwasm"

    eval $export_cmd

    final_cmd=$(echo "$compile_test_cmd_fork_test" | sed "s|\[input\]|$source_file|g" | sed "s|\[output\]|$output_file_wasm|g")
    pre_compile=$(echo "$precompile_wasm" | sed "s|\[input\]|$output_file_wasm|g" | sed "s|\[output\]|$output_file_cwasm|g")

    final_cmd="${final_cmd} && ${pre_compile}"

    echo -e "${GREEN}Compiling test: $test_name in $output_dir${RESET}"
    echo -e "${GREEN}$final_cmd${RESET}"

    if [ "$pmode" -eq 0 ]; then
        eval "$final_cmd"
    fi
}

run_single_test() {
    local test_name="$1"

    if [[ "$test_name" == /* ]]; then
        new_test_name="${test_name#/}"
    else
        new_test_name="$test_name"
    fi

    target_wasm="${output_dir}/${new_test_name}.wasm"
    target_cwasm="${output_dir}/${new_test_name}.cwasm"

    shift 1

    if [ -e "$target_cwasm" ]; then
        final_cmd=$(echo "$run_cmd_precompile" | sed "s|\[target\]|$target_cwasm|")
        final_cmd="${final_cmd} $@"
    else
        final_cmd=$(echo "$run_cmd" | sed "s|\[target\]|$target_wasm|")
        final_cmd="${final_cmd} $@"
    fi

    eval $export_cmd

    echo -e "${GREEN}Running: $final_cmd${RESET}"

    if timeout 10s bash -c "$final_cmd"; then
        echo
        echo -e "${GREEN}Test $new_test_name completed successfully.${RESET}"
    else
        echo
        echo -e "${RED}Test $new_test_name timed out or failed.${RESET}"
        pkill -f "$final_cmd" 2>/dev/null || true
    fi

    echo
}

compile_all_tests() {
    echo -e "${GREEN}Compiling all test cases in $test_file_base...${RESET}"

    find "$test_file_base" -type f -name "*.c" | while read -r test_file; do
        test_name=$(basename "$test_file" .c)
        test_dir=$(dirname "$test_file")

        echo -e "\n${GREEN}Compiling test: $test_name in $test_dir${RESET}"

        compile_single_test "$test_name.c"
    done

    echo -e "${GREEN}All tests compiled.${RESET}"
}

run_all_tests() {
    echo -e "${GREEN}Running all test cases in $test_file_base...${RESET}"

    find "$test_file_base" -type f -name "*.c" | while read -r test_file; do
        test_name=$(basename "$test_file" .c)
        test_dir=$(dirname "$test_file")

        echo -e "\n${GREEN}Running test: $test_name in $test_dir${RESET}"

        run_single_test "$test_dir/$test_name"
    done

    echo -e "${GREEN}All tests completed.${RESET}"
}

compile_from_files() {
    local file_list="$1"

    if [ ! -f "$file_list" ]; then
        echo -e "${RED}Error: File list $file_list not found.${RESET}"
        exit 1
    fi

    echo -e "${GREEN}Compiling test cases from file: $file_list...${RESET}"

    while IFS= read -r test_file; do
        if [ -n "$test_file" ]; then
            find "$test_file_base" -type f -name "$test_file" 2>/dev/null | while read -r full_path; do
                test_name=$(basename "$full_path" .c)
                test_dir=$(dirname "$full_path")
                compile_single_test "$test_file"
            done
        fi
    done < "$file_list"

    echo -e "${GREEN}All specified tests compiled.${RESET}"
}

run_from_files() {
    local file_list="$1"

    if [ ! -f "$file_list" ]; then
        echo -e "${RED}Error: File list $file_list not found.${RESET}"
        exit 1
    fi

    echo -e "${GREEN}Running tests from file list: $file_list...${RESET}"

    while IFS= read -r test_file; do
        if [ -n "$test_file" ]; then
            find "$test_file_base" -type f -name "$test_file" | while read -r full_path; do
                test_name=$(basename "$full_path" .c)
                test_dir=$(dirname "$full_path")

                echo -e "\n${GREEN}Running test: $test_name in $test_dir${RESET}"

                run_single_test "$test_dir/$test_name"
            done
        fi
    done < "$file_list"

    echo -e "${GREEN}All tests from file list completed.${RESET}"
}

pmode=0
if [ "${!#}" = "-p" ]; then
    pmode=1

    # Remove the last argument
    new_args=("${@:1:$#-1}")
    # Overwrite the positional parameters with the new arguments
    set -- "${new_args[@]}"
fi

case "$1" in
    filescompile)
        if [ -z "$2" ]; then
            echo -e "${RED}error: file list not provided${RESET}"
            exit 1
        fi
        compile_from_files "$2"
        ;;

    filesrun)
        if [ -z "$2" ]; then
            echo -e "${RED}error: file list not provided${RESET}"
            exit 1
        fi
        run_from_files "$2"
        ;;

    files)
        if [ -z "$2" ]; then
            echo -e "${RED}error: file list not provided${RESET}"
            exit 1
        fi
        compile_from_files "$2"
        run_from_files "$2"
        ;;

    singlecompile)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi
        compile_single_test "$2"
        ;;

    singlerun)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi
        run_single_test "$2"
        ;;

    single)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi
        compile_single_test "$2"
        run_single_test "$2"
        ;;
    allcompile)
        compile_all_tests
        ;;
    allrun)
        run_all_tests
        ;;
    all)
        compile_all_tests
        run_all_tests
        ;;
esac

