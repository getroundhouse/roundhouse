#!/bin/sh
# Sets a stock image up for a workspace, as the adapter makes its sandbox: the tools a worker needs, the user its
# agents run as, and the agents' command lines. Run as root, and safe to run again on a sandbox it set up before.
set -eu
export DEBIAN_FRONTEND=noninteractive

apt-get update -qq
apt-get install -y -qq --no-install-recommends ca-certificates curl git openssh-client procps xz-utils > /dev/null

# Agents run as a user, since Claude Code won't skip its approvals as root.
id roundhouse > /dev/null 2>&1 || useradd --create-home --shell /bin/bash roundhouse
mkdir -p /opt/roundhouse

# The newest release of each agent's command line, into the user's ~/.local/bin.
su - roundhouse -c 'command -v ~/.local/bin/claude > /dev/null || curl -fsSL https://claude.ai/install.sh | bash'
arch="$(uname -m)"
su - roundhouse -c "test -x ~/.local/bin/codex || {
    mkdir -p ~/.local/bin && cd ~/.local/bin &&
    curl -fsSL https://github.com/openai/codex/releases/latest/download/codex-$arch-unknown-linux-musl.tar.gz | tar -xz &&
    mv codex-$arch-unknown-linux-musl codex
}"
