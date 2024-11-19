#!/bin/bash
WORKDIR=$(pwd)  # Set working directory to the output of pwd
FILE="$WORKDIR/gen_netdevs"  # Use the full path for the file
PARENTPATH=$( cd "$(dirname "${BASH_SOURCE[0]}")" ; pwd -P ) # needs the source location when being used in lind-build.

if [ -f "$FILE" ]; then
    echo "$FILE exists."
else 
    echo "$FILE does not exist. Compiling."
    gcc "$PARENTPATH/gen_netdevs.c" -o "$WORKDIR/gen_netdevs"
    if [ $? -ne 0 ]; then  # Check if gcc succeeded
        echo "Compilation failed."
        exit 1
    fi
fi

echo "Generating netdevs"

"$WORKDIR/gen_netdevs" > "$WORKDIR/net_devices"  # Execute and output using full paths
