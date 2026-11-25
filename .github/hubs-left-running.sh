#!/usr/bin/env bash
# Fails if a hub or worker that the tests started is still running once they've ended, naming each, and stops them, so
# a leak shows here rather than as thousands of idle processes on a developer's machine. A test's hub stops once its
# test process has gone, as `process::TestProcess` says, which can take as long as stopping its workers does, up to
# their 15s shutdown grace, so this waits 30s for that first.
#
# It takes every process running the daemon as the tests', by either of its names, `roundhoused` or `Roundhouse Hub`,
# a build's copy, which a hub a stamped build starts runs from, wherever it runs from. That's only so on a runner, where
# nothing else runs it, so this is for CI, and not for a developer's machine.
set -euo pipefail

# Each process running the daemon, by PID. Either name is started by its whole path, which can have spaces in it.
running() {
  pgrep -f '^/.*/(roundhoused|Roundhouse Hub)( |$)' || true
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
