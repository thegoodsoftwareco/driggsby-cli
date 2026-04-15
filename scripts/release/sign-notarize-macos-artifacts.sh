#!/usr/bin/env bash
set -euo pipefail
umask 077

artifact_dir="${1:-target/distrib}"
dry_run="${DRIGGSBY_SIGN_NOTARIZE_DRY_RUN:-}"

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "Missing required environment variable: ${name}" >&2
    exit 1
  fi
}

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    echo "Missing required command: ${name}" >&2
    exit 1
  fi
}

decode_base64_to_file() {
  local encoded="$1"
  local destination="$2"

  : > "$destination"
  chmod 600 "$destination"
  if base64 --help 2>&1 | grep -q -- "--decode"; then
    printf "%s" "$encoded" | base64 --decode > "$destination"
  else
    printf "%s" "$encoded" | base64 -D > "$destination"
  fi
}

read_artifacts() {
  find "$artifact_dir" \
    -maxdepth 1 \
    -type f \
    -name "driggsby-*-apple-darwin.tar.xz" \
    -print |
    sort
}

reject_unsafe_path() {
  local artifact_name="$1"
  local path="$2"
  local segment
  local old_ifs

  if [[ -z "$path" || "$path" = /* ]]; then
    echo "Artifact ${artifact_name} contains an unsafe path: ${path}" >&2
    exit 1
  fi

  old_ifs="$IFS"
  IFS="/"
  for segment in $path; do
    if [[ "$segment" == ".." || "$segment" == -* ]]; then
      IFS="$old_ifs"
      echo "Artifact ${artifact_name} contains an unsafe path: ${path}" >&2
      exit 1
    fi
  done
  IFS="$old_ifs"
}

assert_regular_tar_entry() {
  local artifact_path="$1"
  local artifact_name="$2"
  local entry="$3"
  local listing

  listing="$(tar -tvf "$artifact_path" "$entry" | head -n 1)"
  if [[ "${listing:0:1}" != "-" ]]; then
    echo "Artifact ${artifact_name} entry ${entry} must be a regular file." >&2
    exit 1
  fi
}

validate_artifact_layout() {
  local artifact_path="$1"
  local artifact_name="$2"
  local root_dir="${artifact_name%.tar.xz}"
  local path
  local normalized
  local found_binary="false"
  local found_license="false"
  local found_readme="false"

  while IFS= read -r path; do
    normalized="${path#./}"
    reject_unsafe_path "$artifact_name" "$normalized"

    case "$normalized" in
      "${root_dir}/") ;;
      "${root_dir}/driggsby") found_binary="true" ;;
      "${root_dir}/LICENSE") found_license="true" ;;
      "${root_dir}/README.md") found_readme="true" ;;
      *)
        echo "Artifact ${artifact_name} contains unexpected entry: ${normalized}" >&2
        exit 1
        ;;
    esac
  done < <(tar -tf "$artifact_path")

  if [[ "$found_binary" != "true" || "$found_license" != "true" || "$found_readme" != "true" ]]; then
    echo "Artifact ${artifact_name} is missing driggsby, LICENSE, or README.md." >&2
    exit 1
  fi

  assert_regular_tar_entry "$artifact_path" "$artifact_name" "${root_dir}/driggsby"
  assert_regular_tar_entry "$artifact_path" "$artifact_name" "${root_dir}/LICENSE"
  assert_regular_tar_entry "$artifact_path" "$artifact_name" "${root_dir}/README.md"
}

prepare_real_signing() {
  local imported_identity

  require_env "APPLE_DEVELOPER_ID_CERTIFICATE_P12_BASE64"
  require_env "APPLE_DEVELOPER_ID_CERTIFICATE_PASSWORD"
  require_env "APPLE_NOTARY_KEY_P8_BASE64"
  require_env "APPLE_NOTARY_KEY_ID"
  require_env "APPLE_NOTARY_ISSUER_ID"
  require_env "APPLE_TEAM_ID"
  require_env "APPLE_CODESIGN_IDENTITY"

  case "$APPLE_CODESIGN_IDENTITY" in
    *"(${APPLE_TEAM_ID})"*) ;;
    *)
      echo "APPLE_CODESIGN_IDENTITY must include the configured Apple team ID." >&2
      exit 1
      ;;
  esac

  require_command "base64"
  require_command "codesign"
  require_command "ditto"
  require_command "plutil"
  require_command "security"
  require_command "spctl"
  require_command "uuidgen"
  require_command "xcrun"

  if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "macOS signing and notarization must run on a macOS runner." >&2
    exit 1
  fi

  keychain_password="$(uuidgen)"
  certificate_path="${tmp_dir}/developer-id-application.p12"
  notary_key_path="${tmp_dir}/notary-key.p8"

  decode_base64_to_file "$APPLE_DEVELOPER_ID_CERTIFICATE_P12_BASE64" "$certificate_path"
  decode_base64_to_file "$APPLE_NOTARY_KEY_P8_BASE64" "$notary_key_path"

  security create-keychain -p "$keychain_password" "$keychain_path" >/dev/null
  security set-keychain-settings -lut 600 "$keychain_path" >/dev/null
  security unlock-keychain -p "$keychain_password" "$keychain_path" >/dev/null
  security list-keychains -d user -s "$keychain_path" >/dev/null
  security default-keychain -d user -s "$keychain_path" >/dev/null
  security import "$certificate_path" \
    -k "$keychain_path" \
    -P "$APPLE_DEVELOPER_ID_CERTIFICATE_PASSWORD" \
    -T /usr/bin/codesign \
    -T /usr/bin/security \
    >/dev/null
  security set-key-partition-list \
    -S apple-tool:,apple:,codesign: \
    -s \
    -k "$keychain_password" \
    "$keychain_path" \
    >/dev/null

  imported_identity="$(
    security find-identity -v -p codesigning "$keychain_path" |
      awk -v identity="$APPLE_CODESIGN_IDENTITY" 'index($0, identity) { print $2; found=1; exit } END { if (!found) exit 1 }'
  )"
  if [[ -z "$imported_identity" ]]; then
    echo "Configured Developer ID signing identity was not found in the imported certificate." >&2
    exit 1
  fi
  codesign_identity="$imported_identity"
}

sign_binary() {
  local binary_path="$1"

  if [[ -n "$dry_run" ]]; then
    printf "\ndry-run-signed\n" >> "$binary_path"
    return
  fi

  security unlock-keychain -p "$keychain_password" "$keychain_path" >/dev/null
  codesign \
    --force \
    --timestamp \
    --options runtime \
    --sign "$codesign_identity" \
    --keychain "$keychain_path" \
    "$binary_path"
  codesign --verify --strict --verbose=2 "$binary_path"
}

notarize_binary() {
  local artifact_name="$1"
  local binary_path="$2"
  local work_dir="$3"
  local zip_path="${work_dir}/driggsby-notary-submit.zip"
  local notary_result_path="${work_dir}/notary-result.json"
  local notary_status
  local submission_id

  if [[ -n "$dry_run" ]]; then
    echo "Dry-run accepted ${artifact_name}"
    return
  fi

  ditto -c -k --sequesterRsrc --keepParent "$binary_path" "$zip_path"
  xcrun notarytool submit "$zip_path" \
    --key "$notary_key_path" \
    --key-id "$APPLE_NOTARY_KEY_ID" \
    --issuer "$APPLE_NOTARY_ISSUER_ID" \
    --wait \
    --timeout 30m \
    --output-format json \
    > "$notary_result_path"

  notary_status="$(plutil -extract status raw -o - "$notary_result_path")"
  if [[ "$notary_status" != "Accepted" ]]; then
    submission_id="$(plutil -extract id raw -o - "$notary_result_path" 2>/dev/null || true)"
    echo "Apple notarization failed for ${artifact_name} with status ${notary_status}." >&2
    if [[ -n "$submission_id" ]]; then
      xcrun notarytool log "$submission_id" \
        --key "$notary_key_path" \
        --key-id "$APPLE_NOTARY_KEY_ID" \
        --issuer "$APPLE_NOTARY_ISSUER_ID" \
        "${work_dir}/notary-log.json" \
        >/dev/null || true
      if [[ -f "${work_dir}/notary-log.json" ]]; then
        cat "${work_dir}/notary-log.json" >&2
      fi
    fi
    exit 1
  fi
}

repack_artifact() {
  local artifact_path="$1"
  local artifact_name="$2"
  local extract_dir="$3"
  local stage_dir="$4"
  local root_dir="${artifact_name%.tar.xz}"
  local artifact_hash

  mkdir -p "${stage_dir}/${root_dir}"
  cp "${extract_dir}/${root_dir}/driggsby" "${stage_dir}/${root_dir}/driggsby"
  cp "${extract_dir}/${root_dir}/LICENSE" "${stage_dir}/${root_dir}/LICENSE"
  cp "${extract_dir}/${root_dir}/README.md" "${stage_dir}/${root_dir}/README.md"
  chmod 755 "${stage_dir}/${root_dir}/driggsby"
  chmod 644 "${stage_dir}/${root_dir}/LICENSE" "${stage_dir}/${root_dir}/README.md"

  tar -cJf "$artifact_path" -C "$stage_dir" "$root_dir"
  artifact_hash="$(shasum -a 256 "$artifact_path" | awk '{ print $1 }')"
  printf "%s *%s\n" "$artifact_hash" "$artifact_name" > "${artifact_path}.sha256"
}

require_command "find"
require_command "grep"
require_command "head"
require_command "awk"
require_command "sed"
require_command "shasum"
require_command "sort"
require_command "tar"

artifacts=()
while IFS= read -r artifact_path; do
  artifacts+=("$artifact_path")
done < <(read_artifacts)

if [[ "${#artifacts[@]}" -eq 0 ]]; then
  echo "No macOS release artifacts were found in ${artifact_dir}." >&2
  exit 1
fi

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/driggsby-macos-sign.XXXXXX")"
keychain_path="${tmp_dir}/driggsby-signing.keychain-db"
keychain_password=""
codesign_identity=""
certificate_path=""
notary_key_path=""

cleanup() {
  if [[ -n "${keychain_path:-}" ]]; then
    security delete-keychain "$keychain_path" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

if [[ -z "$dry_run" ]]; then
  prepare_real_signing
fi

for artifact_path in "${artifacts[@]}"; do
  artifact_name="$(basename "$artifact_path")"
  root_dir="${artifact_name%.tar.xz}"
  work_dir="${tmp_dir}/${root_dir}"
  extract_dir="${work_dir}/extract"
  stage_dir="${work_dir}/stage"
  binary_path="${extract_dir}/${root_dir}/driggsby"

  mkdir -p "$extract_dir" "$stage_dir"
  validate_artifact_layout "$artifact_path" "$artifact_name"
  tar -xJf "$artifact_path" -C "$extract_dir"

  if [[ ! -f "$binary_path" || ! -x "$binary_path" ]]; then
    echo "Artifact ${artifact_name} does not contain an executable driggsby binary." >&2
    exit 1
  fi

  echo "Signing ${artifact_name}"
  sign_binary "$binary_path"
  echo "Submitting ${artifact_name} for notarization"
  notarize_binary "$artifact_name" "$binary_path" "$work_dir"
  if [[ -z "$dry_run" ]]; then
    spctl --assess --type execute --verbose=4 "$binary_path"
  fi
  repack_artifact "$artifact_path" "$artifact_name" "$extract_dir" "$stage_dir"
done

echo "Signed and notarized ${#artifacts[@]} macOS artifact(s)."
