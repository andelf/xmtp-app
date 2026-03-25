#!/usr/bin/env python3
import argparse
import pathlib
import statistics
import subprocess
import sys
import time


def run(cmd):
    return subprocess.run(cmd, check=True, text=True, capture_output=True)


def percentile(sorted_values, p):
    if not sorted_values:
        return 0
    if len(sorted_values) == 1:
        return sorted_values[0]
    rank = (len(sorted_values) - 1) * p
    low = int(rank)
    high = min(low + 1, len(sorted_values) - 1)
    weight = rank - low
    return round(sorted_values[low] * (1 - weight) + sorted_values[high] * weight, 2)


def main():
    parser = argparse.ArgumentParser(description="Benchmark daemon send latency from daemon stderr logs")
    parser.add_argument("--data-dir", default="./data")
    parser.add_argument("--kind", choices=["dm", "group"], required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--count", type=int, default=10)
    parser.add_argument("--prefix", default="bench")
    args = parser.parse_args()

    data_dir = pathlib.Path(args.data_dir)
    log_path = data_dir / "logs" / "daemon.stderr.log"
    cli = pathlib.Path("target/debug/xmtp-cli")

    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_path.write_text("")

    for idx in range(args.count):
        message = f"{args.prefix}-{idx}-{int(time.time() * 1000)}"
        cmd = [str(cli), "--data-dir", str(data_dir)]
        if args.kind == "dm":
            cmd += ["direct-message", args.target, message]
        else:
            cmd += ["group", "send", args.target, message]
        run(cmd)

    lines = log_path.read_text().splitlines()
    elapsed = []
    needle = "send dm recipient=" if args.kind == "dm" else "send group id="
    pending = 0
    for line in lines:
        if f"request payload={needle}" in line:
            pending += 1
            continue
        if pending and "request ok elapsed_ms=" in line:
            elapsed.append(int(line.rsplit("elapsed_ms=", 1)[1]))
            pending -= 1

    if len(elapsed) != args.count:
        print(f"expected {args.count} send latencies, got {len(elapsed)}", file=sys.stderr)
        print("\n".join(lines[-50:]), file=sys.stderr)
        sys.exit(1)

    values = sorted(elapsed)
    print(f"count={len(values)}")
    print(f"min={values[0]}ms")
    print(f"p50={percentile(values, 0.5)}ms")
    print(f"p95={percentile(values, 0.95)}ms")
    print(f"max={values[-1]}ms")
    print(f"mean={round(statistics.mean(values), 2)}ms")


if __name__ == "__main__":
    main()
