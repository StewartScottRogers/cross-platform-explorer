#!/usr/bin/env python3
"""A small but real Python sample for the code-preview + syntax-highlight path."""
from __future__ import annotations

import sys


def fib(n: int) -> int:
    """Return the n-th Fibonacci number (iterative)."""
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a


def main(argv: list[str]) -> int:
    count = int(argv[1]) if len(argv) > 1 else 10
    for i in range(count):
        print(f"fib({i}) = {fib(i)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
