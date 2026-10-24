#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

# Launch council-ui in demo mode by default if no arguments, or pass CLI args
if [ "$#" -eq 0 ]; then
    cargo run -- --demo
else
    cargo run -- "$@"
fi
