"""The finaltree command line."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .errors import FinaltreeError
from .fingerprint import compute_fingerprint
from .stamping import run_and_stamp, verify_stamped_log


def _parser():
    parser = argparse.ArgumentParser(
        prog="finaltree",
        description="Fingerprint the exact tree a gate ran on.",
    )
    sub = parser.add_subparsers(dest="command_name", required=True)

    finger = sub.add_parser("fingerprint", help="print a tree fingerprint")
    finger.add_argument("root")
    finger.add_argument("--generated", action="append", default=[])

    stamp = sub.add_parser("stamp", help="run a gate command and stamp its log")
    stamp.add_argument("root")
    stamp.add_argument("--log", required=True)
    stamp.add_argument("--generated", action="append", default=[])
    stamp.add_argument("command", nargs="*")

    verify = sub.add_parser("verify", help="verify a stamped log against a tree")
    verify.add_argument("log")
    verify.add_argument("root")
    verify.add_argument("--generated", action="append", default=None)
    return parser


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    stamp_command = []
    if argv and argv[0] == "stamp" and "--" in argv:
        cut = argv.index("--")
        stamp_command = argv[cut + 1:]
        argv = argv[:cut]
    args = _parser().parse_args(argv)
    try:
        if args.command_name == "fingerprint":
            fingerprint = compute_fingerprint(args.root, tuple(args.generated))
            print(
                json.dumps(
                    fingerprint.to_dict(), indent=2, sort_keys=True
                )
            )
            return 0
        if args.command_name == "stamp":
            command = stamp_command or list(args.command)
            if not command:
                print("error. the stamp command names no gate command", file=sys.stderr)
                return 2
            return run_and_stamp(
                args.root, command, Path(args.log), tuple(args.generated)
            )
        if args.command_name == "verify":
            patterns = None if args.generated is None else tuple(args.generated)
            result = verify_stamped_log(args.log, args.root, patterns)
            if result.matched:
                print("match")
                return 0
            print("mismatch")
            for difference in result.differences:
                print(difference)
            return 1
    except FinaltreeError as exc:
        print(f"error. {exc}", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
