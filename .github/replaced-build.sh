#!/usr/bin/env bash
# Says whether a newer merge's build replaced the build named $JOB in this run, for release.yml's publishing jobs, by
# writing replaced=true to $GITHUB_OUTPUT. That merge's run releases it, so the publishing job stops quietly.
#
# A replaced build is cancelled before it gets a runner, so the jobs API says `completed cancelled 0` of it, and one that
# built says `completed success` and its runner. A publishing job starts only once its build has finished, but the API
# can take a few seconds to say so. On 2026-11-22, jobs that asked 4 or 5 seconds after their builds were cancelled
# read something else, went on, and failed to download binaries the builds never made (#2377). So this asks until the
# answer is one of those two, and prints each answer.
set -euo pipefail

for _ in $(seq 12); do
  build="$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID/jobs?per_page=100" \
    --jq '[.jobs[] | select(.name == env.JOB)] | select(length == 1) | .[0] | "\(.status) \(.conclusion) \(.runner_id)"')"
  [ -n "$build" ] || { echo "::error::This run has no one job named $JOB"; exit 1; }
  echo "The jobs API says $JOB is $build"
  case "$build" in
    "completed cancelled 0")
      echo "::notice::A newer merge's build replaced $JOB, so that merge's run releases it"
      echo "replaced=true" >> "$GITHUB_OUTPUT"
      exit 0
      ;;
    "completed success "[1-9]*) exit 0 ;;
  esac
  sleep 5
done
# Which field was behind on 2026-11-22 isn't known, so anything else is asked again, a failed build too.
echo "::error::After a minute, the jobs API says $JOB is $build, so it neither built nor was replaced"
exit 1
