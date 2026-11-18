import argparse
import inspect
import json
import sys
from pathlib import Path

from .model import DemoError, LIMITATIONS, OPERATIONS
from .runner import COMMANDS, Runner
from .scenarios import SCENARIOS


def show(events, json_output=False):
    for event in events:
        if json_output:
            print(json.dumps(event, sort_keys=True))
        else:
            details = {k: v for k, v in event.items() if k not in ("n", "actor", "op")}
            print(f"{event['n']:04d} {event['actor']:<14} {event['op']:<30} "
                  f"{json.dumps(details, sort_keys=True)}")


def run_sequence(steps, name, json_output=False, quiet=False):
    runner = Runner()
    try:
        runner.run(steps)
    except DemoError:
        if not quiet:
            show(runner.world.events, json_output)
        raise
    if not quiet:
        show(runner.world.events, json_output)
    if not json_output:
        print(f"PASS {name} ({len(steps)} steps)")
    return runner.world


def repl():
    runner = Runner()
    print("JSON step per line; commands, state, limitations, quit. No eval or shell execution.")
    position = 0
    while True:
        try:
            text = input("lind-if> ").strip()
        except EOFError:
            break
        if text == "quit":
            break
        if not text:
            continue
        if text == "commands":
            print(", ".join(COMMANDS))
            continue
        if text == "limitations":
            print(json.dumps(LIMITATIONS, indent=2))
            continue
        try:
            runner.execute(dict(op="state") if text == "state" else json.loads(text))
        except (DemoError, json.JSONDecodeError) as error:
            print(f"ERROR {getattr(error, 'code', 'INPUT')}: {error}", file=sys.stderr)
        show(runner.world.events[position:])
        position = len(runner.world.events)


def main(argv=None):
    parser = argparse.ArgumentParser(description="Deterministic Lind-IF model, not native runtime conformance.")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="list built-in sequences")
    sub.add_parser("commands", help="show custom-step names and signatures")
    sub.add_parser("limitations", help="show open contract questions and simulation assumptions")
    sub.add_parser("coverage", help="run all sequences and show observed interface operations")
    export = sub.add_parser("export", help="print a built-in sequence as editable JSON")
    export.add_argument("name", choices=SCENARIOS)
    run = sub.add_parser("run", help="run named sequences (each in a fresh world)")
    run.add_argument("names", nargs="+", choices=[*SCENARIOS, "all"])
    custom = sub.add_parser("file", help="run a JSON sequence; '-' reads stdin")
    custom.add_argument("path")
    sub.add_parser("repl", help="interactive JSON steps in a persistent world")
    for p in (run, custom):
        p.add_argument("--json", action="store_true", help="emit JSON Lines trace")
        p.add_argument("--quiet", action="store_true", help="omit trace (keep failure status)")
    args = parser.parse_args(argv)
    try:
        if args.command == "list":
            for name, spec in SCENARIOS.items():
                print(f"{name:<22} {spec['description']}")
                print(f"  limitations: {', '.join(spec['limitations'])}")
        elif args.command == "commands":
            for name in COMMANDS:
                print(name + str(inspect.signature(getattr(Runner().world, name))))
            print('Step metadata: "save": "name", "expect_error": "CODE". Reference saved values with "$name".')
        elif args.command == "limitations":
            for name, text in LIMITATIONS.items():
                print(f"{name}: {text}\n")
        elif args.command == "export":
            print(json.dumps(SCENARIOS[args.name]["steps"], indent=2))
        elif args.command == "repl":
            repl()
        elif args.command == "file":
            text = sys.stdin.read() if args.path == "-" else Path(args.path).read_text()
            run_sequence(json.loads(text), args.path, args.json, args.quiet)
        else:
            names = list(SCENARIOS) if args.command == "coverage" or "all" in args.names else args.names
            worlds = [run_sequence(SCENARIOS[name]["steps"], name,
                                   getattr(args, "json", False),
                                   args.command == "coverage" or getattr(args, "quiet", False))
                      for name in names]
            if args.command == "coverage":
                observed = {e["op"] for w in worlds for e in w.events}
                for operation in sorted(OPERATIONS):
                    print(f"{'SEEN' if operation in observed else 'MISSING':<8} {operation}")
                print(f"{len(OPERATIONS & observed)}/{len(OPERATIONS)} draft operations observed; "
                      "trace coverage, not implementation or concurrency conformance.")
                return int(bool(OPERATIONS - observed))
    except (DemoError, json.JSONDecodeError, OSError) as error:
        print(f"ERROR {getattr(error, 'code', 'INPUT')}: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
