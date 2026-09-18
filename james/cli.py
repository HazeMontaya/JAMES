"""Minimal JAMES command-line entry point."""

from __future__ import annotations

import argparse

from .runtime import JamesRuntime


def main() -> None:
    parser = argparse.ArgumentParser(prog="james")
    parser.add_argument("command", nargs="?", default="status")
    args = parser.parse_args()

    runtime = JamesRuntime()
    if args.command == "status":
        print(runtime.status())
    else:
        parser.error(f"unknown command: {args.command}")


if __name__ == "__main__":
    main()
