# Load configuration
source "$(dirname "$0")/lind_config.sh"

# ----------------------------------------------------------------------
# Function: verify_environment
#
# Purpose:
#   Verifies the presence of essential directories, compiler, header files, 
#   and required binaries for the project. It checks if the directories 
#   exist, if the specified compiler is available and executable, and if 
#   essential files like `stdlib.h` and required binaries (such as `wasmtime`, 
#   `cargo`, `gdb`, and `wasm-opt`) are found. The results of each check 
#   are printed to the terminal, and a final summary is provided.
#
# Variables:
# - Directories: 
#     - glibc_base: Path to the glibc directory.
#     - wasmtime_base: Path to the wasmtime directory.
#     - rustposix_base: Path to the rustposix directory.
#     - rawposix_base: Path to the rawposix directory.
#   - Compiler: 
#     - CC: The path to the clang compiler executable (default from the CLANG environment variable).
#   - Header file:
#     - stdlib.h: Path to the required `stdlib.h` header file within glibc.
#   - Binaries: 
#     - List of required binaries (`wasmtime`, `cargo`, `gdb`, `wasm-opt`).
#
# Output:
#   - Prints the results of each verification step to the terminal, indicating 
#     whether the checked item is found or missing. 
#
# Return Value:
#   - This function does not return any values but prints the results directly 
#     to the terminal.
# Exceptions:
#   - The function does not handle any exceptions or errors explicitly. 
# ----------------------------------------------------------------------

verify_environment() {
    echo "========================================"
    echo " Verifying Environment Setup"
    echo "========================================"

    local missing=0  # Counter for missing dependencies

    # Check directories
    for dir in "$glibc_base" "$wasmtime_base" "$rustposix_base" "$rawposix_base"; do
        if [ -d "$dir" ]; then
            echo -e "${GREEN}✔ Directory exists:${RESET} $dir"
        else
            echo -e "${RED}✘ Directory missing:${RESET} $dir"
            missing=$((missing + 1))
        fi
    done

    # Check compiler
    if [ -x "$CC" ]; then
        echo -e "${GREEN}✔ Compiler found:${RESET} $CC"
    else
        echo -e "${RED}✘ Compiler missing or not executable:${RESET} $CC"
        missing=$((missing + 1))
    fi

    # Check required header file (stdlib.h)
    stdlib_path="$glibc_base/sysroot/usr/include/stdlib.h"
    if [ -f "$stdlib_path" ]; then
        echo -e "${GREEN}✔ Found stdlib.h:${RESET} $stdlib_path"
    else
        echo -e "${RED}✘ Missing stdlib.h:${RESET} Expected at $stdlib_path"
        missing=$((missing + 1))
    fi

    # Check required binaries
    for bin in "$wasmtime_base/target/debug/wasmtime" "cargo" "gdb" "wasm-opt"; do
        if command -v "$bin" >/dev/null 2>&1 || [ -x "$bin" ]; then
            echo -e "${GREEN}✔ Found executable:${RESET} $bin"
        else
            echo -e "${RED}✘ Missing executable:${RESET} $bin"
            missing=$((missing + 1))
        fi
    done

    # Final summary
    echo "========================================"
    if [ "$missing" -eq 0 ]; then
        echo -e "${GREEN}All checks passed!${RESET}"
    else
        echo -e "${RED}$missing issue(s) found.${RESET} Please resolve them."
    fi
}

# verification of environment here
#verify_environment

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
    make_all|make_glibc)
        unset LD_LIBRARY_PATH
        echo -e "${GREEN}$make_cmd${RESET}"
        if [ "$pmode" -eq 0 ]; then
            eval "$make_cmd"
        fi
        ;;
    help)
        echo "avaliable commands are:"
        echo "1. compile_test"
        echo "2. run"
        echo "3. compile_wasmtime"
        echo "4. compile_src"
        echo "5. make_all"
        ;;
    *)
        echo "Unknown command identifier."
        ;;
esac