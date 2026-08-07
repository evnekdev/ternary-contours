#!/usr/bin/env python3
"""CI guard for user-facing floating-point formatting.

Numerical, serialization, and diagnostic-core code is intentionally outside
this check. Qt presentation code must route display numbers through
numeric_display.hpp; this catches accidental raw Qt formatting in new code.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
FILES = list((ROOT / "apps" / "ternary-contours-qt" / "src").glob("*.cpp")) + list((ROOT / "apps" / "ternary-contours-qt" / "src").glob("*.hpp"))
FILES += list((ROOT / "apps" / "ternary-contours-qt" / "rust-bridge" / "src").glob("*.rs"))
FILES += list((ROOT / "tools" / "ternary-contours-gui-core" / "src").glob("*.rs"))
FILES += [ROOT / "tools" / "ternary-contours-cli" / "src" / "render.rs"]
# Projection CSV deliberately is not scanned: it is a data-serialization
# context that must write shortest round-trip-safe f64 values, never GUI text.
DATA_SERIALIZATION_CONTEXTS = {
    ROOT / "tools" / "ternary-contours-cli" / "src" / "projection_csv.rs",
}
assert all(context not in FILES for context in DATA_SERIALIZATION_CONTEXTS)
PATTERNS = [
    re.compile(r"(?:QString::number|toString)\s*\([^\n]*,\s*['\"](?:g|f)['\"]"),
    re.compile(r"format!\s*\([^\n]*\{[^\n}]*:\.?\d+(?:\.\d+)?f"),
]
violations = []
for path in FILES:
    for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if "presentation-format: allow" in line:
            continue
        for pattern in PATTERNS:
            if pattern.search(line):
                violations.append(f"{path.relative_to(ROOT)}:{line_no}: {line.strip()}")
if violations:
    print("Unclassified presentation float formatting detected:")
    print("\n".join(violations))
    sys.exit(1)
print(f"presentation formatting guard passed ({len(FILES)} files)")
