#!/bin/bash
# Regression test: compile and run every example, verify exit codes and
# byte-compare stdout against the checked-in snapshots under
# test/snapshots/<name>.out. Run with UPDATE=1 to (re)generate snapshots.
set -u
cd "$(dirname "$0")" || exit 1

snapshot_dir="test/snapshots"
update="${UPDATE:-0}"

mkdir -p out "$snapshot_dir"

cargo build 2>&1 | grep -E "^error" && { echo "BUILD FAILED"; exit 1; }

pass=0
fail=0
skip=0

for f in examples/*.hz; do
  name=$(basename "$f" .hz)
  if [ "$name" = "10_guess_number_game" ]; then
    echo "SKIP: $name (interactive)"
    skip=$((skip+1))
    continue
  fi
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
  snapshot="$snapshot_dir/$name.out"
  # Windows 的 C 运行时把 \n 转成 \r\n,快照统一剥掉 \r 以便跨平台比对
  if [ "$update" = "1" ]; then
    tr -d '\r' < /tmp/huzc_run.log > "$snapshot"
    echo "PASS: $name (snapshot updated)"
  elif [ ! -f "$snapshot" ]; then
    echo "FAIL(no-snapshot): $name (run UPDATE=1 ./test.sh to create)"
    fail=$((fail+1))
    continue
  elif ! diff -q <(tr -d '\r' < "$snapshot") <(tr -d '\r' < /tmp/huzc_run.log) > /dev/null; then
    echo "FAIL(snapshot-mismatch): $name"
    diff <(tr -d '\r' < "$snapshot") <(tr -d '\r' < /tmp/huzc_run.log) | head -5
    fail=$((fail+1))
    continue
  fi
  pass=$((pass+1))
done

echo "-----------------------------"
echo "$pass passed, $fail failed, $skip skipped"
[ $fail -eq 0 ]
