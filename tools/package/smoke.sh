#!/usr/bin/env bash
# Runs a build's command line and a hub it starts, as the release workflow checks what it packages, since packaging is
# often for another processor and can't run them: the hub and the CLI say they're BUILD, and the CLI starts a hub, from
# the copy of the daemon a stamped build makes, finds it, and stops it. The hub keeps its data, and its log, in
# WORK/home. Whatever fails, no hub is left running.
#
#   tools/package/smoke.sh BINS BUILD WORK
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# EXE.
# shellcheck source=SCRIPTDIR/../build.sh
source "$HERE/../build.sh"

[ $# -eq 3 ] || { echo "error: tools/package/smoke.sh BINS BUILD WORK. See the top of this script" >&2; exit 1; }
bins=$1 build=$2 work=$3
cli="$bins/roundhouse$EXE"
mkdir -p "$work"
export ROUNDHOUSE_HOME="$work/home" ROUNDHOUSE_WORKSPACES="$work/workspaces"
trap '"$cli" hub stop > /dev/null 2>&1 || true' EXIT
for bin in roundhoused roundhouse; do
  said="$("$bins/$bin$EXE" --version)"
  [ "$said" = "$bin $build" ] || { echo "error: $bin says it's \"$said\", not $build" >&2; exit 1; }
done
# Needs a hub, so it starts one.
"$cli" repo list
"$cli" hub status | tee "$work/status.txt"
"$cli" hub stop
grep -q " version $build " "$work/status.txt" || { echo "error: the hub isn't build $build" >&2; exit 1; }
