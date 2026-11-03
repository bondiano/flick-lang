#!/bin/sh
# Installs the flick binary from a GitHub release:
#   curl -fsSL https://bondiano.github.io/flick-lang/install.sh | sh
# FLICK_VERSION picks a release tag (default: the latest), FLICK_BIN_DIR the
# install directory (default: ~/.local/bin).
set -eu

repo=bondiano/flick-lang
bin_dir=${FLICK_BIN_DIR:-$HOME/.local/bin}

fail() {
  echo "flick: $*" >&2
  exit 1
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
if [ -n "${FLICK_VERSION:-}" ]; then
  url=https://github.com/$repo/releases/download/$FLICK_VERSION/$archive
else
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

tar -xzf "$tmp/$archive" -C "$tmp"
mkdir -p "$bin_dir"
install -m 755 "$tmp/flick" "$bin_dir/flick"
echo "Installed $("$bin_dir/flick" --version) to $bin_dir/flick"

case :$PATH: in
  *:"$bin_dir":*) ;;
  *) echo "Add $bin_dir to your PATH, e.g.: export PATH=\"$bin_dir:\$PATH\"" ;;
esac
echo "Projects also need Node.js 24.18 or later and pnpm."
