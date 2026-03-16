#!/usr/bin/env python3
"""Show uncovered lines and branches per source file from cargo llvm-cov.

Usage:
    # Generate coverage with missing lines:
    cargo +nightly llvm-cov --all-features --branch --show-missing-lines 2>&1 | tee /tmp/cov.txt

    # Or generate LCOV for line-level data:
    cargo +nightly llvm-cov --all-features --branch --lcov --output-path=/tmp/cov.lcov

    # Run report:
    python3 coverage_report.py [--lcov /tmp/cov.lcov] [--file <substring>]

Note on metrics:
    - "Lines" in llvm-cov JSON/text = region-based counting (stricter, counts ?-operator
      error branches as separate regions on the same line)
    - "Lines" in LCOV = pure line coverage (was the line executed at all?)
    - "Branches" = conditional branches (if/match/&&/||)
    - CI uses the JSON "lines" metric, which is the stricter one.
"""
import argparse
import collections
import re
import sys


def parse_llvm_cov_text(text_path):
    """Parse the text output from cargo llvm-cov --show-missing-lines."""
    results = {}
    totals = {}

    with open(text_path) as f:
        content = f.read()

    # Parse TOTAL line
    total_match = re.search(
        r'TOTAL\s+(\d+)\s+(\d+)\s+([\d.]+)%\s+'  # regions
        r'(\d+)\s+(\d+)\s+([\d.]+)%\s+'  # functions
        r'(\d+)\s+(\d+)\s+([\d.]+)%\s+'  # lines
        r'(\d+)\s+(\d+)\s+([\d.]+)%',  # branches
        content,
    )
    if total_match:
        totals = {
            'lines_total': int(total_match.group(7)),
            'lines_missed': int(total_match.group(8)),
            'lines_pct': float(total_match.group(9)),
            'branches_total': int(total_match.group(10)),
            'branches_missed': int(total_match.group(11)),
            'branches_pct': float(total_match.group(12)),
        }

    # Parse per-file missing lines
    for match in re.finditer(r'/[^:]+/src/([^:]+\.rs):\s*([\d, ]+)', content):
        filename = match.group(1)
        lines = [int(x.strip()) for x in match.group(2).split(',')]
        results[filename] = sorted(lines)

    return results, totals


def parse_lcov(lcov_path):
    """Parse LCOV for per-line coverage and branch data."""
    uncovered = collections.defaultdict(list)
    covered_count = collections.defaultdict(int)
    branch_uncov = collections.defaultdict(list)

    current_file = None
    with open(lcov_path) as f:
        for line in f:
            line = line.strip()
            if line.startswith("SF:"):
                raw = line[3:]
                current_file = raw.split("src/", 1)[1] if "src/" in raw else None
            elif line.startswith("DA:") and current_file:
                parts = line[3:].split(",")
                lineno, count = int(parts[0]), int(parts[1])
                if count == 0:
                    uncovered[current_file].append(lineno)
                else:
                    covered_count[current_file] += 1
            elif line.startswith("BRDA:") and current_file:
                parts = line[5:].split(",")
                lineno = int(parts[0])
                count_str = parts[3]
                if count_str == "0" or count_str == "-":
                    branch_uncov[current_file].append(lineno)

    results = {}
    totals_missed = 0
    totals_total = 0
    for f in sorted(set(list(uncovered.keys()) + list(covered_count.keys()))):
        results[f] = {
            'uncovered_lines': sorted(uncovered.get(f, [])),
            'uncovered_branch_lines': sorted(set(branch_uncov.get(f, []))),
            'total': len(uncovered.get(f, [])) + covered_count.get(f, 0),
            'missed': len(uncovered.get(f, [])),
        }
        totals_missed += len(uncovered.get(f, []))
        totals_total += results[f]['total']

    totals = {
        'lines_total': totals_total,
        'lines_missed': totals_missed,
        'lines_pct': (totals_total - totals_missed) / totals_total * 100 if totals_total else 0,
    }
    return results, totals


def ranges_str(lines):
    if not lines:
        return "(none)"
    sorted_lines = sorted(set(lines))
    ranges = []
    start = prev = sorted_lines[0]
    for l in sorted_lines[1:]:
        if l == prev + 1:
            prev = l
        else:
            ranges.append(f"L{start}" if start == prev else f"L{start}-{prev}")
            start = prev = l
    ranges.append(f"L{start}" if start == prev else f"L{start}-{prev}")
    return ", ".join(ranges)


def main():
    p = argparse.ArgumentParser(description="Coverage report for funveil")
    p.add_argument("--lcov", default=None, help="LCOV file (line-level accuracy)")
    p.add_argument("--text", default=None, help="Text output from cargo llvm-cov --show-missing-lines")
    p.add_argument("--file", "-f", default=None, help="Filter by filename substring")
    args = p.parse_args()

    if args.lcov:
        file_data, totals = parse_lcov(args.lcov)
        source = "LCOV (pure line coverage)"
    elif args.text:
        file_data_raw, totals = parse_llvm_cov_text(args.text)
        source = "llvm-cov text (region-based line coverage)"
        # Convert to dict format
        file_data = {}
        for f, lines in file_data_raw.items():
            file_data[f] = {'uncovered_lines': lines, 'missed': len(lines)}
    else:
        print("Usage: provide --lcov or --text", file=sys.stderr)
        sys.exit(1)

    print(f"Source: {source}")
    print(f"=== Line coverage: {totals['lines_pct']:.2f}% "
          f"({totals['lines_missed']} uncovered / {totals['lines_total']} total) ===")
    if 'branches_pct' in totals:
        print(f"=== Branch coverage: {totals['branches_pct']:.2f}% "
              f"({totals['branches_missed']} uncovered / {totals['branches_total']} total) ===")
    print()

    items = []
    for f, data in file_data.items():
        if args.file and args.file not in f:
            continue
        missed = data.get('missed', len(data.get('uncovered_lines', [])))
        if missed == 0:
            continue
        items.append((missed, f, data))

    items.sort(key=lambda x: -x[0])

    for missed, f, data in items:
        uncov = data.get('uncovered_lines', [])
        total = data.get('total', 0)
        pct_str = f" / {total}" if total else ""
        print(f"--- {f}  (-{missed} lines{pct_str}) ---")
        print(f"  {ranges_str(uncov)}")
        br = data.get('uncovered_branch_lines', [])
        if br:
            print(f"  branches: {ranges_str(br)}")
        print()


if __name__ == "__main__":
    main()
