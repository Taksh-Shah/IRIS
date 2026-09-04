"""`python -m iris_bench` — thin launcher.

    python -m iris_bench                       # tier0 smoke, default config
    python -m iris_bench --test_module tier1   # a specific tier
    python -m iris_bench -c path/to/config.yml

Passes through to Mobly's test_runner with a sane default config path.
"""

import argparse
import os
import sys

from mobly import suite_runner

_HERE = os.path.dirname(__file__)
_DEFAULT_CONFIG = os.path.join(_HERE, "configs", "bench_2phone.yml")

_MODULES = {
    "tier0": "iris_bench.test_tier0_smoke",
    "tier1": "iris_bench.test_tier1_ble",
    "tier2": "iris_bench.test_tier2_wifidirect",
}


def main():
    ap = argparse.ArgumentParser(prog="iris_bench")
    ap.add_argument("-c", "--config", default=_DEFAULT_CONFIG)
    ap.add_argument("--test_module", default="tier0", choices=sorted(_MODULES))
    ap.add_argument("--tests", nargs="*", help="specific test method names")
    args, rest = ap.parse_known_args()

    mod_path = _MODULES[args.test_module]
    __import__(mod_path)
    module = sys.modules[mod_path]
    cls = next(
        v for v in vars(module).values()
        if isinstance(v, type) and v.__module__ == mod_path and v.__name__ != "IrisBenchBase"
    )

    argv = ["-c", args.config]
    if args.tests:
        argv += ["--tests"] + [f"{cls.__name__}.{t}" for t in args.tests]
    suite_runner.run_suite([cls], argv=argv)


if __name__ == "__main__":
    main()
