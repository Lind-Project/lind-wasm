#!/bin/bash

# ================================================================
# Secure Restricted Shell Script
# ---------------------------------------------------------------
# This script creates a restricted shell environment where users
# can execute commands only within a predefined directory.
#
# Features:
# - Users are confined to a sandboxed directory ($DEFAULT_PATH)
# - Prevents moving outside the sandbox (e.g., cd .., mkdir ../)
# - Automatically appends the current directory path to commands
# - Prevents file operations outside the sandbox
# - Supports a help (-h, --help) option to list commands
#
# Usage:
# - Run the script normally: ./script.sh
# - Use 'cd <dir>' to navigate within the sandbox
# - Run commands like 'ls', 'mkdir', 'rm', 'touch', etc.
# - Type 'exit' to quit
# - Run '-h' or '--help' for usage details
# ================================================================


# Set the default directory where all file-related operations will occur
DEFAULT_PATH="/home/lind-wasm"
#/src/RawPOSIX/tmp" 
CURRENT_PATH="$DEFAULT_PATH"
mkdir -p "$DEFAULT_PATH" # Ensure the directory exists

# Function to display help message
show_help() {
    echo "Secure Restricted Shell - Help"
    echo "------------------------------"
    echo "This is a sandboxed shell environment that restricts users"
    echo "to a predefined directory and prevents unauthorized access."
    echo
    echo "Supported Commands:"
    echo "  cd <dir>        - Change directory within the sandbox"
    echo "  ls [options]    - List files in the current directory"
    echo "  mkdir <dir>     - Create a new directory inside the sandbox"
    echo "  touch <file>    - Create a new empty file inside the sandbox"
    echo "  rm <file>       - Remove a file (restricted to sandbox)"
    echo "  mv <src> <dest> - Move/rename a file within the sandbox"
    echo "  cp <src> <dest> - Copy files inside the sandbox"
    echo "  echo <text> > <file> - Write text to a file in the sandbox"
    echo "  cat <file>      - Display the contents of a file"
    echo "  pwd             - Print the current working directory"
    echo "  find <name>     - Search for a file in the sandbox"
    echo "  grep <text> <file> - Search for text inside a file"
    echo "  head <file>     - Show first few lines of a file"
    echo "  tail <file>     - Show last few lines of a file"
    echo "  chmod <mode> <file> - Change file permissions"
    echo "  exit            - Exit the shell"
    echo
    echo "Restrictions:"
    echo "  - You CANNOT navigate outside the sandbox directory."
    echo "  - All file-related commands are restricted to the sandbox."
    echo "  - Attempts to escape using '..' or absolute paths will be blocked."
}

# Function to show command-specific help
show_command_help() {
    case "$1" in
        cd)
            echo "Usage: cd <directory>"
            echo "Change to a directory within the sandbox."
            echo "Example: cd my_folder"
            ;;
        ls)
            echo "Usage: ls [options]"
            echo "List files and directories in the current sandbox directory."
            echo "Example: ls -l"
            ;;
        mkdir)
            echo "Usage: mkdir <directory>"
            echo "Create a new directory inside the sandbox."
            echo "Example: mkdir new_folder"
            ;;
        touch)
            echo "Usage: touch <file>"
            echo "Create a new empty file inside the sandbox."
            echo "Example: touch file.txt"
            ;;
        rm)
            echo "Usage: rm <file>"
            echo "Remove a file inside the sandbox."
            echo "Example: rm file.txt"
            ;;
        mv)
            echo "Usage: mv <source> <destination>"
            echo "Move or rename a file within the sandbox."
            echo "Example: mv old.txt new.txt"
            ;;
        cp)
            echo "Usage: cp <source> <destination>"
            echo "Copy files inside the sandbox."
            echo "Example: cp file1.txt file2.txt"
            ;;
        find)
            echo "Usage: find <name>"
            echo "Search for a file in the sandbox."
            echo "Example: find myfile.txt"
            ;;
        grep)
            echo "Usage: grep <text> <file>"
            echo "Search for text inside a file."
            echo "Example: grep 'error' logfile.txt"
            ;;
        head)
            echo "Usage: head <file>"
            echo "Show the first few lines of a file."
            echo "Example: head file.txt"
            ;;
        tail)
            echo "Usage: tail <file>"
            echo "Show the last few lines of a file."
            echo "Example: tail file.txt"
            ;;
        chmod)
            echo "Usage: chmod <mode> <file>"
            echo "Change file permissions."
            echo "Example: chmod 644 file.txt"
            ;;
        *)
            echo "No detailed help available for '$1'. Try '-h' for general help."
            ;;
    esac
}

# Function to validate arguments (prevents escaping sandbox)
validate_path() {
    local input_path="$1"

    # Resolve absolute path safely
    if [[ "$input_path" == /* ]]; then
        resolved_path="$input_path"  # Absolute path
    else
        resolved_path="$CURRENT_PATH/$input_path"  # Relative path
    fi

    # Normalize path (remove redundant slashes and handle '..' properly)
    resolved_path=$(cd "$(dirname "$resolved_path")" 2>/dev/null && pwd)/$(basename "$resolved_path")

    # Check if resolved path is within allowed directory
    if [[ "$resolved_path" == "$DEFAULT_PATH"* ]]; then
        echo "$resolved_path"  # Return the safe path
    else
        echo "Error: Access outside $DEFAULT_PATH is not allowed!" >&2
        return 1
    fi
}

# Notify the user about the default path
echo "All file-related changes will occur in: $DEFAULT_PATH"
echo "Run '-h' or '--help' for usage details"
echo "Current working directory: $CURRENT_PATH"

# Main shell loop
while true; do
    # Print the shell prompt with the default path
    echo -n "lind-wasm > "

    # Read user input
    read -r input

    # Exit the shell if the input is 'exit'
    if [[ "$input" == "exit" ]]; then
        echo "Exiting shell."
        break
    fi

    # Check if the user requested help
    if [[ "$input" == "-h" || "$input" == "--help" ]]; then
        show_help
        continue
    fi

    # Parse the command and arguments
    command=$(echo "$input" | awk '{print $1}')
    args=$(echo "$input" | awk '{$1=""; print $0}' | sed 's/^ //')

    # Command-specific help
    if [[ "$args" == "--help" ]]; then
        show_command_help "$command"
        continue
    fi

    # Handle 'pwd' command to always return current path
    if [[ "$command" == "pwd" ]]; then
        echo "$CURRENT_PATH"
        continue
    fi

    # Handle 'cd' command separately
    if [[ "$command" == "cd" ]]; then
        if [[ -z "$args" ]]; then
            # If no argument, reset to default path
            CURRENT_PATH="$DEFAULT_PATH"
        else
            # Manually construct absolute path
            if [[ "$args" == /* ]]; then
                new_path="$args"  # Absolute path
            else
                new_path="$CURRENT_PATH/$args"  # Relative path
            fi

            # Normalize path to resolve '..' safely
            new_path=$(cd "$new_path" 2>/dev/null && pwd)

            # Check if the new path is within the allowed directory
            if [[ "$new_path" == "$DEFAULT_PATH"* && -d "$new_path" ]]; then
                CURRENT_PATH="$new_path"
            else
                echo "cd: $args: No such directory or permission denied"
            fi
        fi
        continue
    fi

    # Handle 'cp' command
    if [[ "$command" == "cp" ]]; then
        # Split arguments into an array
        read -ra cmd_args <<< "$args"

        if [[ "${cmd_args[0]}" == "-r" ]]; then
            # Handle recursive copy
            src=$(validate_path "${cmd_args[1]}")
            dest=$(validate_path "${cmd_args[2]}")

            if [[ $? -eq 0 && -d "$src" ]]; then
                cp -r "$src" "$dest"
            else
                echo "Error: Invalid source or destination path!"
            fi
        else
            # Handle normal copy
            src=$(validate_path "${cmd_args[0]}")
            dest=$(validate_path "${cmd_args[1]}")

            if [[ $? -eq 0 && -f "$src" ]]; then
                cp "$src" "$dest"
            else
                echo "Error: Invalid source or destination path!"
            fi
        fi
        continue
    fi

    # Handle 'rm' command
    if [[ "$command" == "rm" ]]; then
        read -ra cmd_args <<< "$args"  # Split arguments into an array

        if [[ "${cmd_args[0]}" == "-r" ]]; then
            # Handle recursive deletion
            for ((i = 1; i < ${#cmd_args[@]}; i++)); do
                validated_path=$(validate_path "${cmd_args[i]}")
                if [[ $? -eq 0 && -d "$validated_path" ]]; then
                    rm -r "$validated_path"
                else
                    echo "Error: Cannot remove ${cmd_args[i]} (Invalid path or not a directory)"
                fi
            done
        else
            # Handle normal file deletion
            for item in "${cmd_args[@]}"; do
                validated_path=$(validate_path "$item")
                if [[ $? -eq 0 && -f "$validated_path" ]]; then
                    rm "$validated_path"
                else
                    echo "Error: Cannot remove $item (Invalid path or not a file)"
                fi
            done
        fi
        continue
    fi
    # Validate all command arguments
    validated_args=$(validate_path $args)
    if [[ $? -ne 0 ]]; then
        continue
    fi

    # Execute the command
    eval "$command $validated_args" 2>/dev/null || echo "Error: Command not found or failed"
done
