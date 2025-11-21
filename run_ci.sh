#!/bin/bash
set -e

echo "=== 1. Building Release Version ==="
cargo build --release

echo "=== 2. Setting up Python Environment ==="
# 获取生成的 .dylib (macOS) 或 .so (Linux) 文件名
# 注意：Cargo 输出的文件名通常是 libllm_json_utils.dylib
# 但 Python 期望的是 llm_json_utils.so (或者 .abi3.so)

echo "=== 3. Running Tests ==="
echo "=== 3. Running Tests ==="
# 1. Build the test binary without running it
cargo test --no-default-features --test integration_test --no-run

# 2. Find the executable (it's in target/debug/deps/integration_test-*)
# We pick the most recent one
TEST_BIN=$(ls -t target/debug/deps/integration_test* | head -n 1)

# 3. Run it with the correct library path
export DYLD_LIBRARY_PATH=$(python3-config --prefix)/lib:$DYLD_LIBRARY_PATH
$TEST_BIN --nocapture

echo "=== 4. Cleanup ==="
# No cleanup needed for cargo test
echo "Done."
