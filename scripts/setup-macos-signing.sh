#!/bin/bash
# Run only on an ephemeral macOS release runner. Never enable shell tracing.
set -euo pipefail

for name in CERTIFICATE_P12 CERTIFICATE_PASSWORD APPLE_ID APPLE_APP_PASSWORD TEAM_ID RUNNER_TEMP GITHUB_ENV; do
  if [ -z "${!name:-}" ]; then
    case "$name" in
      CERTIFICATE_P12) source="repository Actions secret MACOS_CERTIFICATE_P12" ;;
      CERTIFICATE_PASSWORD) source="repository Actions secret MACOS_CERTIFICATE_PASSWORD" ;;
      APPLE_ID|APPLE_APP_PASSWORD) source="repository Actions secret $name" ;;
      TEAM_ID) source="workflow env setting TEAM_ID" ;;
      RUNNER_TEMP|GITHUB_ENV) source="GitHub Actions runner environment variable $name" ;;
    esac
    echo "::error::Required signing input is empty. Check $source."
    exit 1
  fi
done

umask 077
certificate="$RUNNER_TEMP/certificate.p12"
keychain="$RUNNER_TEMP/release.keychain-db"
# Register cleanup before any operation that can fail, including credential validation.
printf 'KEYCHAIN=%s\n' "$keychain" >> "$GITHUB_ENV"
trap 'rm -f "$certificate"' EXIT
if ! printf '%s' "$CERTIFICATE_P12" | base64 --decode > "$certificate"; then
  echo '::error::MACOS_CERTIFICATE_P12 is not valid base64.'
  exit 1
fi
if [ ! -s "$certificate" ]; then
  echo '::error::MACOS_CERTIFICATE_P12 decoded to an empty file.'
  exit 1
fi

password=$(openssl rand -hex 24)
echo "::add-mask::$password"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$certificate" -k "$keychain" -P "$CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" > /dev/null

# Preserve keychain paths containing spaces rather than splitting their words.
keychains=("$keychain")
while IFS= read -r path; do
  path=$(printf '%s' "$path" | sed 's/^[[:space:]]*"//; s/"[[:space:]]*$//')
  if [ -n "$path" ] && [ "$path" != "$keychain" ]; then
    keychains+=("$path")
  fi
done < <(security list-keychains -d user)
security list-keychains -d user -s "${keychains[@]}"
security default-keychain -s "$keychain"

# Validate the imported private key and team, not merely the presence of a certificate.
identities=$(security find-identity -v -p codesigning "$keychain")
identity=$(printf '%s\n' "$identities" | sed -n "s/.* \([A-Fa-f0-9]\{40\}\) \"Developer ID Application:.* ($TEAM_ID)\".*/\1/p")
if [ -z "$identity" ] || [ "$(printf '%s\n' "$identity" | wc -l | tr -d ' ')" != 1 ]; then
  echo "::error::Expected exactly one valid Developer ID Application identity for team $TEAM_ID, including its private key."
  exit 1
fi
printf 'VOLT_SIGNING_IDENTITY=%s\n' "$identity" >> "$GITHUB_ENV"
xcrun notarytool store-credentials mdstar --apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$APPLE_APP_PASSWORD" --keychain "$keychain"
