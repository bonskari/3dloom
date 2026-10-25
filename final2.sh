#!/usr/bin/env bash
cargo test --workspace --no-fail-fast 2>&1 | grep -E "^test .*FAILED|panicked|^test result|^error" > /tmp/loom-materials-test.txt
echo "TESTS: $(grep -c "^test result: ok" /tmp/loom-materials-test.txt) ok suites"
grep -v "^test result: ok" /tmp/loom-materials-test.txt | head -40
grep "^test result" /tmp/loom-materials-test.txt | awk "{p+=\$4; f+=\$6} END {print p\" passed, \"f\" failed\"}"
cargo clippy --workspace --tests --message-format short 2>&1 | grep -E "^(crates|error)" | grep -v "falling back to" > /tmp/loom-materials-clippy.txt
echo "CLIPPY total: $(wc -l < /tmp/loom-materials-clippy.txt)"
grep -E "material|maps\.rs|mesh_buffer|loom_bevy|loom_render|compound|^error" /tmp/loom-materials-clippy.txt
cargo build -q -p loom_app 2>&1 | tail -2
bash /tmp/loom-materials-shots.sh 2>&1 | grep -v WARN
echo ALL-DONE
