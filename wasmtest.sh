#!/bin/bash

# wasmtest.sh is a shell script that can compile and run wasm tests.
# There are three funtions for wasmteest.sh(1.test single file 2.test all files 3.test all tests from the file user give)
# To test single file use: ./wasmtest.sh single <your file name>.c OR ./wasmtest.sh s <your file name>.c
# To compile single file use: ./wasmtest.sh singlecompile <your file name>.c OR ./wasmtest.sh sc <your file name>.c
# To run single file use: ./wasmtest.sh singlerun <your file name>.c OR ./wasmtest.sh sr <your file name>.c
# To test all files use: ./wasmtest.sh all OR ./wasmtest.sh a
# To compile all files use: ./wasmtest.sh allcompile OR ./wasmtest.sh ac
# To run all files use: ./wasmtest.sh allrun OR ./wasmtest.sh ar
# To test all tests from the file user give use: ./wasmtest.sh files OR ./wasmtest.sh file OR ./wasmtest.sh f
# To compile all tests from the file user give use: ./wasmtest.sh filescompile OR ./wasmtest.sh filecompile OR /wasmtest.sh fc
# To run all tests from the file user give use: ./wasmtest.sh filesrun OR ./wasmtest.sh filerun OR /wasmtest.sh fr
# To modify timeout time use(default is 5s): ./wasmtime <the method> --timeout=<the time you want in second>
# To modify LIND_WASM_BASE use(default is /home/lind-wasm): export LIND_WASM_BASE=<path you want>

LIND_WASM_BASE="${LIND_WASM_BASE:-/home/lind-wasm}"

DEFAULT_TIMEOUT=5
TIMEOUT=$DEFAULT_TIMEOUT

glibc_base="$LIND_WASM_BASE/src/glibc"
wasmtime_base="$LIND_WASM_BASE/src/wasmtime"

CC="${CLANG:=$LIND_WASM_BASE/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04}/bin/clang"

compile_test_cmd_fork_test="$CC -pthread --target=wasm32-unknown-wasi --sysroot $glibc_base/sysroot -Wl,--import-memory,--export-memory,--max-memory=67108864,--export="__stack_pointer",--export=__stack_low [input] -g -O0 -o [output] && wasm-opt --asyncify --debuginfo [output] -o [output]"
precompile_wasm="$wasmtime_base/target/debug/wasmtime compile [input] -o [output]"
run_cmd_precompile="$wasmtime_base/target/debug/wasmtime run --allow-precompiled --wasi threads=y --wasi preview2=n [target]"

test_file_base="$LIND_WASM_BASE/tests/unit-tests"

#color codes for terminal output
RED='\033[31m'
GREEN='\033[32m'
RESET='\033[0m'

for arg in "$@"; do
    if [[ "$arg" == --timeout=* ]]; then
        TIMEOUT="${arg#*=}"
        if ! [[ "$TIMEOUT" =~ ^[0-9]+$ ]]; then
            echo -e "${RED}Error: Timeout must be a positive integer.${RESET}"
            exit 1
        fi
        # Remove --timeout argument from the argument list
        set -- "${@/$arg}"
    fi
done

#function to compile a single test file
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

    final_cmd=$(echo "$compile_test_cmd_fork_test" | sed "s|\[input\]|$source_file|g" | sed "s|\[output\]|$output_file_wasm|g")
    pre_compile=$(echo "$precompile_wasm" | sed "s|\[input\]|$output_file_wasm|g" | sed "s|\[output\]|$output_file_cwasm|g")

    final_cmd="${final_cmd} && ${pre_compile}"

    echo -e "${GREEN}Compiling test: $test_name in $output_dir${RESET}"
    echo -e "${GREEN}$final_cmd${RESET}"

    if [ "$pmode" -eq 0 ]; then
        eval "$final_cmd"
    fi
}

#function to run a single test file
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

    echo -e "${GREEN}Running: $final_cmd${RESET}"

    if timeout "${TIMEOUT}s" bash -c "$final_cmd"; then
        echo
        echo -e "${GREEN}Test $new_test_name completed successfully.${RESET}"
    else
        echo
        echo -e "${RED}Test $new_test_name timed out or failed.${RESET}"
        pkill -f "$final_cmd" 2>/dev/null || true
    fi

    echo
}

#function to compile all the .c test files in the unit-tests folder
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

#function to run all the .c test files in the unit-tests folder
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

#function to compile all the files from the file user give
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

#function to run all the files from the file user give
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
    filescompile|filecompile|fc)
        if [ -z "$2" ]; then
            echo -e "${RED}error: file list not provided${RESET}"
            exit 1
        fi
        compile_from_files "$2"
        ;;

    filesrun|filerun|fr)
        if [ -z "$2" ]; then
            echo -e "${RED}error: file list not provided${RESET}"
            exit 1
        fi
        run_from_files "$2"
        ;;

    files|file|f)
        if [ -z "$2" ]; then
            echo -e "${RED}error: file list not provided${RESET}"
            exit 1
        fi
        compile_from_files "$2"
        run_from_files "$2"
        ;;

    singlecompile|sc)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi
        compile_single_test "$2"
        ;;

    singlerun|sr)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi
        run_single_test "$2"
        ;;

    single|s)
        if [ -z "$2" ]; then
            echo -e "${RED}error: source file name not provided${RESET}"
            exit 1
        fi
        compile_single_test "$2"
        run_single_test "$2"
        ;;
    allcompile|ac)
        compile_all_tests
        ;;
    allrun|ar)
        run_all_tests
        ;;
    all|a)
        compile_all_tests
        run_all_tests
        ;;
esac
