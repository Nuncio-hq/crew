#!/usr/bin/env python3
"""Bounded, manifest-owned Crew staging operations."""

import argparse
import json
from pathlib import Path

from crew_staging_config import Refused, on_server, public_plan, validate
from crew_staging_resources import inspect_plan
from crew_staging_baseline import snapshot
from crew_staging_restore import restore, stop
from crew_staging_verify import verify
from crew_staging_profiles import export_profiles


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["plan", "snapshot", "restore", "verify", "stop", "profiles"])
    parser.add_argument("--config", required=True)
    parser.add_argument("--baseline")
    args = parser.parse_args()
    try:
        config = validate(json.loads(Path(args.config).read_text()))
        if args.operation == "plan":
            plan = public_plan(config)
            if on_server(config):
                inspect_plan(config)
                plan.update(status="PLANNED", reason="resolved Compose validated; live ownership preflight required before restore")
            print(json.dumps(plan, indent=2))
        elif args.operation == "snapshot":
            print(json.dumps(snapshot(config, args.baseline)))
        elif args.operation == "restore":
            print(json.dumps(restore(config, args.baseline)))
        elif args.operation == "stop":
            print(json.dumps(stop(config)))
        elif args.operation == "verify":
            print(json.dumps(verify(config, args.baseline)))
        elif args.operation == "profiles":
            print(json.dumps(export_profiles(config)))
        else:
            raise Refused("unresolved staging operation: server inspection and baseline required")
    except (Refused, OSError, ValueError, KeyError, AttributeError, TypeError) as error:
        message = str(error) if isinstance(error, Refused) else "cannot read valid staging configuration"
        parser.exit(2, message + "\n")


if __name__ == "__main__":
    main()
