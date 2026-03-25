#!/usr/bin/env python3
import argparse
import csv
import os
from collections import defaultdict


def ensure_matplotlib():
    try:
        import matplotlib.pyplot as plt
        return plt
    except Exception:
        raise SystemExit(
            "matplotlib is required. Install with: python3 -m pip install matplotlib"
        )


def read_csv(path):
    with open(path, newline="", encoding="utf-8") as f:
        return list(csv.DictReader(f))


def as_float(v):
    try:
        return float(v)
    except Exception:
        return 0.0


def plot_hit_rate_by_scenario(plt, rows, out_dir):
    grouped = defaultdict(list)
    for r in rows:
        grouped[r["scenario"]].append(r)

    for scenario, items in grouped.items():
        variants = [x["variant"] for x in items]
        vals = [as_float(x["hit_rate"]) for x in items]

        fig = plt.figure(figsize=(10, 5))
        ax = fig.add_subplot(111)
        ax.bar(range(len(vals)), vals)
        ax.set_title(f"Hit Rate: {scenario}")
        ax.set_ylabel("Hit Rate")
        ax.set_ylim(0, 1)
        ax.set_xticks(range(len(vals)))
        ax.set_xticklabels(variants, rotation=35, ha="right")
        fig.tight_layout()
        fig.savefig(os.path.join(out_dir, f"{scenario}_hit_rate.png"), dpi=180)
        plt.close(fig)


def plot_latency_by_scenario(plt, rows, out_dir):
    grouped = defaultdict(list)
    for r in rows:
        grouped[r["scenario"]].append(r)

    for scenario, items in grouped.items():
        variants = [x["variant"] for x in items]
        p95 = [as_float(x["p95_ms"]) for x in items]

        fig = plt.figure(figsize=(10, 5))
        ax = fig.add_subplot(111)
        ax.bar(range(len(p95)), p95)
        ax.set_title(f"P95 Latency (ms): {scenario}")
        ax.set_ylabel("P95 (ms)")
        ax.set_xticks(range(len(p95)))
        ax.set_xticklabels(variants, rotation=35, ha="right")
        fig.tight_layout()
        fig.savefig(os.path.join(out_dir, f"{scenario}_p95_latency.png"), dpi=180)
        plt.close(fig)


def plot_warmup_curve(plt, warmup_rows, out_dir):
    series = defaultdict(list)
    for r in warmup_rows:
        key = (r["scenario"], r["variant"])
        series[key].append((int(r["window_index"]), as_float(r["hit_rate"])))

    for (scenario, variant), points in series.items():
        points = sorted(points)
        x = [p[0] for p in points]
        y = [p[1] for p in points]

        fig = plt.figure(figsize=(9, 5))
        ax = fig.add_subplot(111)
        ax.plot(x, y, marker="o")
        ax.set_title(f"Warm-up Curve: {scenario} ({variant})")
        ax.set_xlabel("Window Index")
        ax.set_ylabel("Hit Rate")
        ax.set_ylim(0, 1)
        fig.tight_layout()
        fig.savefig(
            os.path.join(out_dir, f"{scenario}_{variant}_warmup_curve.png"),
            dpi=180,
        )
        plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description="Generate dissertation graphs from evaluation CSVs")
    parser.add_argument("--input", required=True, help="Directory containing summary.csv and warmup_curve.csv")
    parser.add_argument("--output", required=True, help="Output directory for PNG plots")
    args = parser.parse_args()

    summary_path = os.path.join(args.input, "summary.csv")
    warmup_path = os.path.join(args.input, "warmup_curve.csv")

    if not os.path.exists(summary_path):
        raise SystemExit(f"Missing file: {summary_path}")
    if not os.path.exists(warmup_path):
        raise SystemExit(f"Missing file: {warmup_path}")

    os.makedirs(args.output, exist_ok=True)
    plt = ensure_matplotlib()

    rows = read_csv(summary_path)
    warmup_rows = read_csv(warmup_path)

    plot_hit_rate_by_scenario(plt, rows, args.output)
    plot_latency_by_scenario(plt, rows, args.output)
    plot_warmup_curve(plt, warmup_rows, args.output)

    print(f"generated_plots={args.output}")


if __name__ == "__main__":
    main()
