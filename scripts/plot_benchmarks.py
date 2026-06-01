#!/usr/bin/env python3
"""Generate dependency-free SVG charts from benchmark results."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Iterable


COLORS = {
    "green": "#2e7d32",
    "blue": "#1565c0",
    "orange": "#ef6c00",
    "red": "#c62828",
    "grid": "#d9dee7",
    "text": "#1f2937",
    "muted": "#6b7280",
    "background": "#ffffff",
}


def esc(text: object) -> str:
    return str(text).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def bar_chart(title: str, labels: list[str], values: list[float], ylabel: str, color: str) -> str:
    width = max(760, 110 + len(labels) * 74)
    height = 440
    left = 70
    right = 24
    top = 58
    bottom = 110
    plot_w = width - left - right
    plot_h = height - top - bottom
    max_value = max(values) if values else 1.0
    if max_value <= 0:
        max_value = 1.0
    step = plot_w / max(len(labels), 1)
    bar_w = min(42, step * 0.64)

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        f'<rect width="100%" height="100%" fill="{COLORS["background"]}"/>',
        f'<text x="{left}" y="32" font-family="Arial, sans-serif" font-size="22" font-weight="700" fill="{COLORS["text"]}">{esc(title)}</text>',
        f'<text x="18" y="{top + plot_h / 2}" font-family="Arial, sans-serif" font-size="13" fill="{COLORS["muted"]}" transform="rotate(-90 18 {top + plot_h / 2})">{esc(ylabel)}</text>',
    ]

    for i in range(5):
        y = top + plot_h - (plot_h * i / 4)
        value = max_value * i / 4
        parts.append(f'<line x1="{left}" y1="{y:.1f}" x2="{width - right}" y2="{y:.1f}" stroke="{COLORS["grid"]}" stroke-width="1"/>')
        parts.append(f'<text x="{left - 10}" y="{y + 4:.1f}" text-anchor="end" font-family="Arial, sans-serif" font-size="12" fill="{COLORS["muted"]}">{value:.0f}</text>')

    for idx, (label, value) in enumerate(zip(labels, values)):
        x = left + idx * step + (step - bar_w) / 2
        bar_h = plot_h * value / max_value
        y = top + plot_h - bar_h
        parts.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{bar_w:.1f}" height="{bar_h:.1f}" rx="4" fill="{color}"/>')
        parts.append(f'<text x="{x + bar_w / 2:.1f}" y="{y - 8:.1f}" text-anchor="middle" font-family="Arial, sans-serif" font-size="12" fill="{COLORS["text"]}">{value:.1f}</text>')
        parts.append(f'<text x="{x + bar_w / 2:.1f}" y="{height - 78}" text-anchor="end" font-family="Arial, sans-serif" font-size="12" fill="{COLORS["text"]}" transform="rotate(-45 {x + bar_w / 2:.1f} {height - 78})">{esc(label)}</text>')

    parts.append("</svg>")
    return "\n".join(parts) + "\n"


def status_chart(total: int, passed: int, failed: int) -> str:
    width = 640
    height = 260
    pass_w = 0 if total == 0 else 460 * passed / total
    fail_w = 0 if total == 0 else 460 * failed / total
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="100%" height="100%" fill="{COLORS["background"]}"/>
<text x="40" y="42" font-family="Arial, sans-serif" font-size="22" font-weight="700" fill="{COLORS["text"]}">Benchmark Status</text>
<rect x="40" y="88" width="460" height="44" rx="6" fill="{COLORS["grid"]}"/>
<rect x="40" y="88" width="{pass_w:.1f}" height="44" rx="6" fill="{COLORS["green"]}"/>
<rect x="{40 + pass_w:.1f}" y="88" width="{fail_w:.1f}" height="44" rx="0" fill="{COLORS["red"]}"/>
<text x="40" y="166" font-family="Arial, sans-serif" font-size="15" fill="{COLORS["text"]}">Passed: {passed}</text>
<text x="180" y="166" font-family="Arial, sans-serif" font-size="15" fill="{COLORS["text"]}">Failed: {failed}</text>
<text x="320" y="166" font-family="Arial, sans-serif" font-size="15" fill="{COLORS["text"]}">Total: {total}</text>
</svg>
"""


def numeric_values(results: Iterable[dict], key: str) -> tuple[list[str], list[float]]:
    labels: list[str] = []
    values: list[float] = []
    for result in results:
        value = result.get(key)
        if isinstance(value, (int, float)):
            labels.append(str(result.get("case", "case")))
            values.append(float(value))
    return labels, values


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", help="Path to benchmark results.json")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    results_path = Path(args.results)
    payload = json.loads(results_path.read_text(encoding="utf-8"))
    out_dir = results_path.parent
    results = payload.get("results", [])

    total = int(payload.get("summary", {}).get("total", len(results)))
    passed = int(payload.get("summary", {}).get("passed", 0))
    failed = int(payload.get("summary", {}).get("failed", 0))
    (out_dir / "status.svg").write_text(status_chart(total, passed, failed), encoding="utf-8")

    labels, values = numeric_values(results, "line_reduction_pct")
    (out_dir / "line-reduction.svg").write_text(
        bar_chart("Line Reduction by Fixture", labels, values, "reduction (%)", COLORS["green"]),
        encoding="utf-8",
    )

    labels, values = numeric_values(results, "duration_seconds")
    (out_dir / "runtime.svg").write_text(
        bar_chart("Runtime by Fixture", labels, values, "seconds", COLORS["blue"]),
        encoding="utf-8",
    )

    labels, values = numeric_values(results, "byte_reduction_pct")
    (out_dir / "byte-reduction.svg").write_text(
        bar_chart("Byte Reduction by Fixture", labels, values, "reduction (%)", COLORS["orange"]),
        encoding="utf-8",
    )

    print(f"Wrote SVG charts to {out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
