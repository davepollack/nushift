#!/bin/bash
set -euo pipefail

directories=(
    "nothing-nushift-app"
    "shm-nushift-app"
    "ebreak-test"
    "hello-world"
)

# Loop through directories and start a background task for each
for dir in "${directories[@]}"; do
    (
        cd "$dir" || exit
        just
    ) &
done

# Wait for all background tasks to complete
wait
