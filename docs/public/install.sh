#!/bin/sh
# Installs the flick binary from a GitHub release:
#   curl -fsSL https://bondiano.github.io/flick-lang/install.sh | sh
# FLICK_VERSION picks a compiler release, `v0.1.0` or `0.1.0` (default: the
# latest), FLICK_BIN_DIR the install directory (default: ~/.local/bin).
set -eu

repo=bondiano/flick-lang
bin_dir=${FLICK_BIN_DIR:-$HOME/.local/bin}
api=https://api.github.com/repos/$repo/releases?per_page=30

fail() {
  echo "flick: $*" >&2
  exit 1
}

# A compiler release is `v<major>.<minor>.<patch>`. A package release, such as
# `std-v0.1.0`, is a release of the same repository, and GitHub counts it in
# `releases/latest`: only the tag of a compiler release is downloaded here.
compiler_tag() {
  normalized=v${1#v}
  printf '%s' "$normalized" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' ||
    fail "no compiler release \`$1\`: FLICK_VERSION takes v0.1.0, or 0.1.0"
  printf '%s' "$normalized"
}

# The newest compiler release, from the release list, which is newest first.
latest_tag() {
  curl -fsSL -H 'accept: application/vnd.github+json' "$api" 2>/dev/null |
    sed -n 's/.*"tag_name": *"\(v[0-9][0-9.]*\)".*/\1/p' |
    head -n 1
}

case $(uname -s) in
  Darwin) os=apple-darwin ;;
  Linux) os=unknown-linux-gnu ;;
  *) fail "no prebuilt binary for $(uname -s); see https://github.com/$repo/releases" ;;
esac
case $(uname -m) in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) fail "no prebuilt binary for $(uname -m); see https://github.com/$repo/releases" ;;
esac
# an x86-64 shell under Rosetta still gets the native Apple silicon binary
if [ "$os" = apple-darwin ] && [ "$(sysctl -n hw.optional.arm64 2>/dev/null)" = 1 ]; then
  arch=aarch64
fi

archive=flick-$arch-$os.tar.gz
tag=
if [ -n "${FLICK_VERSION:-}" ]; then
  tag=$(compiler_tag "$FLICK_VERSION")
else
  tag=$(latest_tag)
fi
if [ -n "$tag" ]; then
  url=https://github.com/$repo/releases/download/$tag/$archive
else
  # the release list is unreachable, perhaps over its rate limit: the download
  # of `latest` still names the release it leads to, or fails with the URL
  url=https://github.com/$repo/releases/latest/download/$archive
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

echo "Downloading $url"
curl -fsSL "$url" -o "$tmp/$archive" || fail "download failed: $url"
curl -fsSL "$url.sha256" -o "$tmp/$archive.sha256" || fail "download failed: $url.sha256"
if command -v sha256sum >/dev/null; then
  (cd "$tmp" && sha256sum -c --quiet "$archive.sha256") || fail "checksum mismatch for $archive"
else
  (cd "$tmp" && shasum -a 256 -c --quiet "$archive.sha256") || fail "checksum mismatch for $archive"
fi

# only the binary is unpacked: nothing else the archive names is written
tar -xzf "$tmp/$archive" -C "$tmp" flick || fail "$archive holds no flick"
[ -f "$tmp/flick" ] || fail "$archive holds no flick"
mkdir -p "$bin_dir"
install -m 755 "$tmp/flick" "$bin_dir/flick"
echo "Installed $("$bin_dir/flick" --version) to $bin_dir/flick"

case :$PATH: in
  *:"$bin_dir":*) ;;
  *) echo "Add $bin_dir to your PATH, e.g.: export PATH=\"$bin_dir:\$PATH\"" ;;
esac
echo "Projects also need Node.js 24.18 or later and pnpm."
