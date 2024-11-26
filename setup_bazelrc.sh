#!/bin/bash

# Get the current directory (where the script is run)
SCRIPT_DIR=$(pwd)

# Path to the generated .bazelrc file
BAZELRC_PATH="$SCRIPT_DIR/.bazelrc"

# Create the .bazelrc file and add the required configuration
cat <<EOF > "$BAZELRC_PATH"
# Disable test result caching
test --nocache_test_results

# Discard the analysis cache
build --discard_analysis_cache

# Disable tracking incremental state
build --nokeep_going

# Force standalone spawn strategy (no sandboxing or cache)
build --spawn_strategy=standalone

# Ensure test actions do not use cached results
test --spawn_strategy=standalone

# Set the WORKSPACE_ROOT environment variable
build --action_env=WORKSPACE_ROOT=$SCRIPT_DIR
EOF

echo ".bazelrc file has been created in $SCRIPT_DIR with the specified configuration."
