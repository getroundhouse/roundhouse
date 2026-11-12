#!/usr/bin/env bash
# Checks that only the binaries read the build stamp, as `docs/architecture.md` says, so the libraries compile the same
# whatever the stamp. Checks the binaries with one stamp and then another, and fails if the second compiles anything
# but a binary again. CI's lint job runs it. Needs `jq`.
#
#   tools/check-stamp.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# What `cargo check` compiled again with the stamp BUILD and NUMBER, rather than found fresh, as `name kind` lines.
compiled() {
  ROUNDHOUSE_BUILD="$1" ROUNDHOUSE_BUILD_NUMBER="$2" cargo check --workspace --bins --locked --message-format json |
    jq -r 'select(.reason == "compiler-artifact" and .fresh == false) | "\(.target.name) \(.target.kind | join(","))"'
}

indent() { while IFS= read -r line; do echo "  $line"; done; }

compiled stamp-check-one 1 >/dev/null
again="$(compiled stamp-check-two 2)"
echo "Compiled again with another stamp:"
indent <<<"${again:-nothing}"
not_binaries="$(grep -v ' bin$' <<<"$again" || true)"
if [ -n "$not_binaries" ]; then
  echo "error: these read the build stamp, so they compile again for every build. Only a binary's main should read it," \
    "with protocol::this_build!(), and pass the build down:" >&2
  indent <<<"$not_binaries" >&2
  exit 1
fi
