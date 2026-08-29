#!/usr/bin/env python3
# huzi vs Rust vs Python 性能对比脚本
#
# 用法（在项目根目录 huzc/ 下执行）:
#   python test/bench_compare.py
#
# 脚本会：
#   1. 找到（或构建）huzc 编译器
#   2. 编译 test/bench_perf.hz 并多次运行计时
#   3. 用 rustc -O 编译 test/bench_perf.rs（Rust 原生参考版本）并多次运行计时
#   4. 用纯 Python 跑同样的三个内核并计时
#   5. 输出三者对比表格

import os
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TEST_DIR = os.path.join(ROOT, "test")
OUT_DIR = os.path.join(TEST_DIR, "out")

# 与 bench_perf.hz 中保持一致的参数
N_INT = 10_000_000
N_FIB = 30
N_FLOAT = 5_000_000

EXE_RUNS = 10  # 编译型程序跑多次取最小值，减小噪声
PY_RUNS = 1    # 解释型太慢，只跑一次


def find_compiler():
    for rel in ("target/release/huzc.exe", "target/release/huzc",
                "target/debug/huzc.exe", "target/debug/huzc"):
        path = os.path.join(ROOT, rel)
        if os.path.exists(path):
            return path
    return None


def build_compiler():
    print("未找到已构建的 huzc，执行 cargo build --release ...")
    subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True)
    exe = find_compiler()
    if exe is None:
        sys.exit("构建后仍未找到 huzc 可执行文件")
    return exe


def exe_suffix(name):
    return name + (".exe" if os.name == "nt" else "")


def compile_bench(compiler):
    os.makedirs(OUT_DIR, exist_ok=True)
    src = os.path.join(TEST_DIR, "bench_perf.hz")
    out = os.path.join(OUT_DIR, "bench_perf_hz")
    print(f"编译 {src} ...")
    subprocess.run([compiler, "-i", src, "-o", out], cwd=ROOT, check=True,
                   stdout=subprocess.DEVNULL)
    exe = exe_suffix(out)
    if not os.path.exists(exe):
        sys.exit(f"编译产物不存在: {exe}")
    return exe


def compile_rust_bench():
    os.makedirs(OUT_DIR, exist_ok=True)
    src = os.path.join(TEST_DIR, "bench_perf.rs")
    out = exe_suffix(os.path.join(OUT_DIR, "bench_perf_rs"))
    print(f"编译 {src} (rustc -O) ...")
    subprocess.run(["rustc", "-O", src, "-o", out], cwd=ROOT, check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT)
    if not os.path.exists(out):
        sys.exit(f"编译产物不存在: {out}")
    return out


# ---------------- 三个内核的 Python 等价实现 ----------------

def int_loop(n):
    s = 0
    for i in range(n):
        s = s + i % 7
    return s


def fib(n):
    if n <= 1:
        return n
    return fib(n - 1) + fib(n - 2)


def float_loop(n):
    f = 1.0
    for _ in range(n):
        f = f * 1.0000001 + 0.0000001
    return f


def time_callable(fn, runs):
    best = float("inf")
    for _ in range(runs):
        t0 = time.perf_counter()
        fn()
        best = min(best, time.perf_counter() - t0)
    return best


def main():
    compiler = find_compiler() or build_compiler()
    exe = compile_bench(compiler)
    rust_exe = compile_rust_bench()

    print(f"\n测试参数: int_loop({N_INT:,})  fib({N_FIB})  float_loop({N_FLOAT:,})")
    print(f"编译型程序运行 {EXE_RUNS} 次取最小值，Python 运行 {PY_RUNS} 次\n")

    def time_exe(path):
        times = []
        stdout = ""
        for _ in range(EXE_RUNS):
            t0 = time.perf_counter()
            proc = subprocess.run([path], capture_output=True, text=True)
            times.append(time.perf_counter() - t0)
            stdout = proc.stdout
        return min(times), stdout

    # ---- huzi ----
    huzi_best, huzi_out = time_exe(exe)
    print("huzi 输出:")
    print(huzi_out.rstrip())

    # ---- Rust ----
    rust_best, rust_out = time_exe(rust_exe)
    print("\nRust 输出:")
    print(rust_out.rstrip())

    # ---- Python ----
    print("\nPython 运行中，请稍候 ...")
    t0 = time.perf_counter()
    r1 = int_loop(N_INT)
    r2 = fib(N_FIB)
    r3 = float_loop(N_FLOAT)
    py_total = time.perf_counter() - t0
    print(f"int_loop result:{r1}")
    print(f"fib result:{r2}")
    print(f"float_loop result:{r3:.6f}")

    # ---- 汇总 ----
    def speedup(base):
        return py_total / base if base > 0 else float("inf")

    print("\n=================== 结果对比 ===================")
    print(f"huzi   (编译执行, {EXE_RUNS} 次取最小): {huzi_best * 1000:8.1f} ms   "
          f"比 Python 快 {speedup(huzi_best):7.1f} 倍")
    print(f"Rust   (rustc -O,  {EXE_RUNS} 次取最小): {rust_best * 1000:8.1f} ms   "
          f"比 Python 快 {speedup(rust_best):7.1f} 倍")
    print(f"Python (解释执行, {PY_RUNS} 次):             {py_total * 1000:8.1f} ms")
    hz_vs_rs = huzi_best / rust_best if rust_best > 0 else float("inf")
    print(f"huzi 相对 Rust (rustc -O):            {hz_vs_rs:8.2f} 倍耗时")
    print("================================================")

    # 结果一致性抽查（三边数值应完全一致）
    ok = ("29999994" in huzi_out and "832040" in huzi_out
          and "29999994" in rust_out and "832040" in rust_out)
    print("结果一致性:", "通过" if ok else "警告: 请人工核对三边输出")


if __name__ == "__main__":
    main()
