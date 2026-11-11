# shellcheck shell=bash
# What the packaging scripts share: the app's name to Velopack, and running the `vpk` that dotnet-tools.json pins.
# Sourced by `package.sh` and `package-windows.sh`, with HERE set to this directory.

# Velopack's name for the app, which names its packages, and the directory it installs in on Windows.
# shellcheck disable=SC2034  # The scripts that source this use it.
PACK_ID=Roundhouse

die() { echo "error: $1" >&2; exit 1; }

# Runs .NET from the directory that holds dotnet-tools.json, which is how it finds the vpk that file pins. .NET
# outside Windows skips checking NuGet packages' signatures unless it's asked to, and vpk comes from NuGet.
dotnet_here() {
  (cd "$HERE" && DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 DOTNET_NUGET_SIGNATURE_VERIFICATION=true dotnet "$@")
}

# Checks for an SDK that runs the vpk dotnet-tools.json pins, and installs that vpk.
dotnet_tools() {
  local sdk
  command -v dotnet > /dev/null || die "vpk needs the .NET SDK 8, 9 or 10, such as from \`brew install dotnet\`"
  sdk="$(dotnet_here --version)"
  case "${sdk%%.*}" in
    8 | 9 | 10) ;;
    *) die "vpk needs the .NET SDK 8, 9 or 10, and dotnet runs $sdk" ;;
  esac
  dotnet_here tool restore > /dev/null
}

# vpk also reads each option from a VPK_ variable, which would override the script that runs it.
refuse_vpk_variables() {
  local vpk_vars
  vpk_vars="$(compgen -e | grep '^VPK_' || true)"
  [ -z "$vpk_vars" ] || die "unset $(echo "$vpk_vars" | tr '\n' ' ')first, since vpk would take them over this script"
}

# Stops before the build if RELEASES has VERSION or a newer one in FEED, since vpk refuses a version that isn't newer
# than every one there, but only after a build. This is the check for packaging into an --out that holds earlier
# releases.
check_newer() {
  local releases=$1 feed=$2 newer
  [ -f "$releases/$feed" ] || return 0
  newer="$(jq -r --arg version "$VERSION" '
    def parts: split("-")[0] | split(".") | map(tonumber);
    [.Assets[].Version | select(parts >= ($version | parts))] | first // empty' "$releases/$feed")"
  [ -z "$newer" ] || die "$releases already has version $newer, and vpk only adds a newer one than every one there.
Package into another --out"
}
