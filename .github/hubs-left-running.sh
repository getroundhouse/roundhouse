#!/usr/bin/env bash
# Fails if a hub or worker that the tests started is still running once they've ended, naming each, and stops them, so
# a leak shows here rather than as thousands of idle processes on a developer's machine. A test's hub stops once its
# test process has gone, as `process::testing::TestProcess` says, which can take as long as stopping its workers does,
# up to their 15s shutdown grace, so this waits 30s for that first.
#
# It takes every process running the daemon as the tests', by any of its names, wherever it runs from: the tests' own,
# `daemon_for_tests`, the product's, `roundhoused`, and `Roundhouse Hub`, the copy a stamped build's hub runs from.
# That's only so on a runner, where nothing else runs it. On a developer's machine it would stop the installed app's
# hub, and its workers with their agents, so it runs only in CI.
set -euo pipefail

if [ "${GITHUB_ACTIONS:-}" != true ]; then
  echo "error: this stops every Roundhouse hub and worker on the machine, so it runs only on a GitHub runner" >&2
  exit 1
fi

# Each process running the daemon, by PID. Each name is started by its whole path, which can have spaces in it.
running() {
  pgrep -f '^/.*/(daemon_for_tests|roundhoused|Roundhouse Hub)( |$)' || true
}

deadline=$((SECONDS + 30))
left="$(running)"
while [ -n "$left" ] && [ "$SECONDS" -lt "$deadline" ]; do
  sleep 0.05
  left="$(running)"
done
[ -z "$left" ] && exit 0
for pid in $left; do
  echo "::error::A process the tests started was still running 30s after them: $(ps -o pid=,args= -p "$pid" || echo "$pid")"
  kill -TERM "$pid" 2>/dev/null || true
done
exit 1
