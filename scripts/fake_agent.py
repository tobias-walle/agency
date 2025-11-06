#!/usr/bin/env python3
import os
import sys


def main():
    # Read task text from argument or env var
    task = None
    if len(sys.argv) > 1:
        task = sys.argv[1]
    if not task:
        task = os.environ.get("AGENCY_TASK", "")

    # Print a small header so attach sees something immediately
    print("[agent] Got task:", task)
    print("[agent] Ready. Type to echo, Ctrl-Q to detach.")
    sys.stdout.flush()

    try:
        prompt()
        for line in sys.stdin:
            line = line.rstrip("\n")
            if not line:
                print()
                sys.stdout.flush()
                continue
            print(f"[agent] {line}?")
            prompt()
    except KeyboardInterrupt:
        pass


def prompt():
    print(" [user] ", end="")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
