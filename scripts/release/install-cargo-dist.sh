#!/usr/bin/env bash
set -euo pipefail

version="${CARGO_DIST_VERSION:?CARGO_DIST_VERSION must be set}"
tag="v${version}"
repo="axodotdev/cargo-dist"

case "${RUNNER_OS:-$(uname -s)}:${RUNNER_ARCH:-$(uname -m)}" in
  Linux:X64 | Linux:x86_64)
    target="x86_64-unknown-linux-gnu"
    expected_sha256="cd355dab0b4c02fb59038fef87655550021d07f45f1d82f947a34ef98560abb8"
    ;;
  Linux:ARM64 | Linux:aarch64 | Linux:arm64)
    target="aarch64-unknown-linux-gnu"
    expected_sha256="382cc29ff91ef12a5bf78ad8ee1804661d24e2fbe64b1bdedd6078723b677ae5"
    ;;
  macOS:X64 | Darwin:x86_64)
    target="x86_64-apple-darwin"
    expected_sha256="fd4d8f9f07802359cbcdc52bac3abd7d5201c4b73a7cbcdd6faca2232a389f0c"
    ;;
  macOS:ARM64 | Darwin:arm64)
    target="aarch64-apple-darwin"
    expected_sha256="decb01c64c12501931c3cac3111b368a7f48adf8d9e65455c08e5757b9a1fd6f"
    ;;
  *)
    echo "Unsupported cargo-dist host: ${RUNNER_OS:-$(uname -s)}/${RUNNER_ARCH:-$(uname -m)}" >&2
    exit 1
    ;;
esac

if [[ "$version" != "0.31.0" ]]; then
  echo "No pinned cargo-dist checksum for version ${version}; update install-cargo-dist.sh." >&2
  exit 1
fi

asset="cargo-dist-${target}.tar.xz"
url="https://github.com/${repo}/releases/download/${tag}/${asset}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
  --output "${tmp_dir}/${asset}" \
  "$url"

actual_sha256="$(shasum -a 256 "${tmp_dir}/${asset}" | awk '{ print $1 }')"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "cargo-dist checksum mismatch for ${asset}" >&2
  echo "expected: ${expected_sha256}" >&2
  echo "actual:   ${actual_sha256}" >&2
  exit 1
fi

mkdir -p "$HOME/.cargo/bin"
tar -xOf "${tmp_dir}/${asset}" "cargo-dist-${target}/dist" > "${tmp_dir}/dist"
chmod 0755 "${tmp_dir}/dist"
install -m 0755 "${tmp_dir}/dist" "$HOME/.cargo/bin/dist"

"$HOME/.cargo/bin/dist" --version | grep -Fx "cargo-dist ${version}" >/dev/null
