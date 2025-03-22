#!/bin/bash
# Script to run all terminal system tests
# Usage: ./run_terminal_tests.sh

set -e

echo "Running terminal system tests..."

# Install dependencies for web terminal
if [ ! -d "web/js" ] || [ ! -f "web/js/xterm.js" ]; then
    echo "Installing web terminal dependencies..."
    cd web && ./download_deps.sh && cd ..
fi

# Run the standard tests with terminal-web feature
echo "Running standard tests..."
cargo test --features terminal-web -- -Z unstable-options --format=json --report-time

# Run the integration tests individually since some require manual inspection
echo "Running integration tests..."
cargo test --features terminal-web --test terminal_integration

# Run the performance tests
echo "Running performance tests..."
cargo test --features terminal-web --test terminal_performance

# Run the console tests
echo "Running console terminal tests..."
cargo test --features terminal-web test_console_terminal

# Optionally run the WebSocket tests when asked
read -p "Do you want to run the WebSocket tests? (y/n) " run_ws
if [ "$run_ws" == "y" ]; then
    echo "Running WebSocket tests..."
    cargo test --features terminal-web websocket_test -- --nocapture --include-ignored
fi

echo "All tests completed!" 