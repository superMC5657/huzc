#!/bin/bash
# Regression test: compile and run every example, fail on any error or hang.
set -u
cd "$(dirname "$0")" || exit 1

mkdir -p out

cargo build 2>&1 | grep -E "^error" && { echo "BUILD FAILED"; exit 1; }

pass=0
fail=0

for f in examples/*.hz; do
  name=$(basename "$f" .hz)
  if ! ./target/debug/huzc.exe -i "$f" -o "out/$name" > /tmp/huzc_build.log 2>&1; then
    echo "FAIL(compile): $name"
    grep -E "error" /tmp/huzc_build.log | head -1
    fail=$((fail+1))
    continue
  fi
  if ! timeout 10 "./out/$name.exe" > /tmp/huzc_run.log 2>&1; then
    code=$?
    echo "FAIL(run/$code): $name"
    fail=$((fail+1))
    continue
  fi
  echo "PASS: $name"
  pass=$((pass+1))
done

echo "-----------------------------"
echo "$pass passed, $fail failed"
[ $fail -eq 0 ]
