
# Set the default directory where all file-related operations will occur
DEFAULT_PATH="$HOME/home/lind-wasm/src/RawPOSIX/tmp" 
CURRENT_PATH="$DEFAULT_PATH"
#mkdir -p "$DEFAULT_PATH" # Ensure the directory exists

# Notify the user about the default path
echo "All file-related changes will occur in: $DEFAULT_PATH"

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

    # Parse the command and arguments
    command=$(echo "$input" | awk '{print $1}')
    args=$(echo "$input" | awk '{$1=""; print $0}' | sed 's/^ //')

    # Change directory for 'cd' command
    if [[ "$command" == "cd" ]]; then
        if [[ -z "$args" ]]; then
            # If no argument, reset to DEFAULT_PATH
            CURRENT_PATH="$DEFAULT_PATH"
        else
            # Append to the current path if not an absolute path
            if [[ ! "$args" =~ ^/ ]]; then
                args="$CURRENT_PATH/$args"
            fi

            # Update CURRENT_PATH if the directory exists
            if [[ -d "$args" ]]; then
                CURRENT_PATH="$args"
            else
                echo "cd: $args: No such file or directory"
            fi
        fi
        continue
    fi

    # Prepend the default path for file-related commands, if applicable
    if [[ "$command" =~ ^(ls|cat|touch|rm|mv|cp|mkdir)$ ]]; then
        if [[ ! "$args" =~ ^/ ]]; then
            args="$CURRENT_PATH/$args"
        fi
    fi

    # Execute the command
    $command $args 2>/dev/null || echo "Error: Command not found or failed"
done
