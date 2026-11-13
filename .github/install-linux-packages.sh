#!/usr/bin/env bash
# Installs the packages in $LINUX_PACKAGES on one of ci.yml's Ubuntu runners, GitHub's or RunsOn's.
#
# The runners fetch packages from Canonical's mirrors. When those can't be reached, every job that builds fails before
# it starts, as on 2026-11-11, when the runners couldn't resolve *.ubuntu.com for hours
# (actions/runner-images#15296). So if fetching the package lists from them fails, they come from mirrors that others
# run instead. apt checks every list against Ubuntu's signing key, whichever mirror serves it, so a mirror can't change
# what's installed.
set -euo pipefail

# Tried in this order, the fastest first. Since 26.04, arm64 is in the same archive as amd64, so each serves both.
# Each had 26.04's arm64 packages on 2026-11-11.
fallbacks=(http://mirrors.ocf.berkeley.edu/ubuntu/ http://mirrors.edge.kernel.org/ubuntu/ https://mirrors.mit.edu/ubuntu/)

# Without `--error-on=any`, a list that can't be fetched is only a warning, and the install goes on to the mirror that
# can't be reached. Fetching them all takes seconds, so the first try gives up after a minute.
update() { sudo timeout "$1" apt-get update -qq --error-on=any; }

if ! update 60; then
  echo "::warning::Ubuntu's mirrors couldn't be reached, so packages come from ${fallbacks[*]}"
  for i in "${!fallbacks[@]}"; do
    printf '%s\tpriority:%d\n' "${fallbacks[$i]}" "$((i + 1))"
  done | sudo tee /etc/apt/fallback-mirrors.txt > /dev/null
  # Every package here is Ubuntu's, so the image's other sources go too, and none of them can fail this.
  sudo rm -f /etc/apt/sources.list /etc/apt/sources.list.d/*
  codename=$(sed -n 's/^VERSION_CODENAME=//p' /etc/os-release)
  sudo tee /etc/apt/sources.list.d/fallback.sources > /dev/null << EOF
Types: deb
URIs: mirror+file:/etc/apt/fallback-mirrors.txt
Suites: $codename $codename-updates $codename-backports $codename-security
Components: main restricted universe multiverse
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
EOF
  update 300
fi

xargs sudo apt-get install -y -qq <<< "$LINUX_PACKAGES"
