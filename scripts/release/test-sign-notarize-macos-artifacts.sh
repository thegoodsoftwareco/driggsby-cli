#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/driggsby-sign-test.XXXXXX")"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

make_artifact() {
  local output_dir="$1"
  local artifact_name="$2"
  local extra_file="${3:-}"
  local root_dir="${artifact_name%.tar.xz}"
  local stage_dir="${tmp_dir}/stage-${root_dir}"

  mkdir -p "${stage_dir}/${root_dir}" "$output_dir"
  printf "fake driggsby\n" > "${stage_dir}/${root_dir}/driggsby"
  printf "license\n" > "${stage_dir}/${root_dir}/LICENSE"
  printf "readme\n" > "${stage_dir}/${root_dir}/README.md"
  chmod 755 "${stage_dir}/${root_dir}/driggsby"
  if [[ -n "$extra_file" ]]; then
    printf "unexpected\n" > "${stage_dir}/${root_dir}/${extra_file}"
  fi
  tar -cJf "${output_dir}/${artifact_name}" -C "$stage_dir" "$root_dir"
}

assert_file_contains() {
  local path="$1"
  local text="$2"

  if ! grep -F "$text" "$path" >/dev/null; then
    echo "Expected ${path} to contain ${text}." >&2
    exit 1
  fi
}

assert_valid_checksum() {
  local artifact_path="$1"
  local checksum_path="${artifact_path}.sha256"
  local artifact_name
  local expected

  artifact_name="$(basename "$artifact_path")"
  expected="$(shasum -a 256 "$artifact_path" | awk '{ print $1 }') *${artifact_name}"
  assert_file_contains "$checksum_path" "$expected"
}

valid_dir="${tmp_dir}/valid"
make_artifact "$valid_dir" "driggsby-aarch64-apple-darwin.tar.xz"

DRIGGSBY_SIGN_NOTARIZE_DRY_RUN=1 \
  "${repo_root}/scripts/release/sign-notarize-macos-artifacts.sh" "$valid_dir"

assert_valid_checksum "${valid_dir}/driggsby-aarch64-apple-darwin.tar.xz"
mkdir -p "${tmp_dir}/valid-extract"
tar -xJf "${valid_dir}/driggsby-aarch64-apple-darwin.tar.xz" -C "${tmp_dir}/valid-extract"
assert_file_contains \
  "${tmp_dir}/valid-extract/driggsby-aarch64-apple-darwin/driggsby" \
  "dry-run-signed"

invalid_dir="${tmp_dir}/invalid"
make_artifact "$invalid_dir" "driggsby-x86_64-apple-darwin.tar.xz" "extra.txt"

if DRIGGSBY_SIGN_NOTARIZE_DRY_RUN=1 \
  "${repo_root}/scripts/release/sign-notarize-macos-artifacts.sh" "$invalid_dir" \
  > "${tmp_dir}/invalid.log" 2>&1; then
  echo "Expected signing script to reject unexpected archive entries." >&2
  exit 1
fi
assert_file_contains "${tmp_dir}/invalid.log" "contains unexpected entry"
