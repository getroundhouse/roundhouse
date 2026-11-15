"""Fails if a test in nextest's JUnit report took longer than a test may, naming each that did, as
`.config/nextest.toml` says.

Usage: python3 tests-over-limit.py REPORT
"""

import sys
import xml.etree.ElementTree as ElementTree

LIMIT_SECONDS = 10.0


def read(report):
    """Each test in the report, as (seconds, binary, test), with None for a test whose time it doesn't say."""
    cases = []
    for case in ElementTree.parse(report).iter("testcase"):
        time = case.get("time")
        cases.append((None if time is None else float(time), case.get("classname"), case.get("name")))
    return cases


def main(report):
    cases = read(report)
    # A report with none would pass whatever the tests took, as one would if nextest wrote another form.
    if not cases:
        print(f"::error::{report} has no tests in it.")
        return 1
    untimed = [(binary, test) for seconds, binary, test in cases if seconds is None]
    timed = [(seconds, binary, test) for seconds, binary, test in cases if seconds is not None]
    over = sorted((case for case in timed if case[0] > LIMIT_SECONDS), reverse=True)
    for binary, test in untimed:
        print(f"::error::{binary} {test} has no time in the report.")
    for seconds, binary, test in over:
        print(f"::error::{binary} {test} took {seconds:.3f}s, longer than the {LIMIT_SECONDS:.0f}s a test may take.")
    if untimed or over:
        print(f"{len(over)} of {len(cases)} took longer than {LIMIT_SECONDS:.0f}s, and {len(untimed)} say no time.")
        return 1
    print(f"None of {len(cases)} tests took longer than {LIMIT_SECONDS:.0f}s.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
