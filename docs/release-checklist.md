# Release checklist

## Current release target

MD Star is a native Rust/Volt UI Kit application. Releases target **Apple
silicon only**, not a universal Swift or Tauri application.

```text
version tag (v<crates/mdstar-volt version>)
  -> fetch pinned Volt UI Kit
  -> import Developer ID certificate + private key
  -> validate notarization credentials
  -> cargo volt release
       -> build app + Quick Look extension
       -> sign nested extension, then app (hardened runtime)
       -> notarize app, staple ticket, assess with Gatekeeper
       -> create/sign DMG, notarize/staple/assess DMG
  -> independently validate signatures and stapled tickets
  -> upload DMG + SHA256SUMS.txt
  -> create DRAFT GitHub release (tag runs only)
```

## Prerequisites

- App tests and library tests pass; CI builds the app and Quick Look bundle.
- `VOLT_UI_KIT_REF` matches in CI and release workflows.
- `VOLT_UI_KIT_TOKEN` can read the pinned kit repository.
- `MACOS_CERTIFICATE_P12` is **nonempty base64** of a password-protected PKCS#12
  export containing the Developer ID Application certificate **and private key**.
- `MACOS_CERTIFICATE_PASSWORD` matches that export.
- `APPLE_ID` and `APPLE_APP_PASSWORD` can notarize for team `A822K85XUW`.
- See [the app README](../crates/mdstar-volt/README.md) for secret setup.

Secret names appearing in `gh secret list` do not establish that their values
are usable. Never print secrets or enable shell tracing during setup.
`scripts/setup-macos-signing.sh` fails early on empty inputs, checks the imported
identity's team, and registers temporary-keychain cleanup before import.

## Validation

```sh
cargo test --workspace --all-features
cargo test --manifest-path crates/mdstar-volt/Cargo.toml --locked
python3 -B -m unittest discover -s tests/release -v
cd crates/mdstar-volt
cargo volt release --dry-run
```

After a release build:

```sh
codesign --verify --deep --strict --verbose=2 'target/release/bundle/MD Star.app'
xcrun stapler validate 'target/release/bundle/MD Star.app'
spctl --assess --type execute --verbose 'target/release/bundle/MD Star.app'
xcrun stapler validate target/release/bundle/MD-Star-*.dmg
```

- Verify the bundle version matches the tag; an older locally notarized app
  does not validate a newer release.
- Download the GitHub-built DMG and test installation, launch, Finder Open With,
  Quick Look, editing/saving, and `md` linking before publishing the draft.
- Confirm the embedded Quick Look extension is signed and present.
- The workflow publishes no standalone CLI tarballs, Linux packages, or Windows
  installers. Use `install.sh --link-app` with an installed macOS app.

## Diagnosing failures

- Import failure: check certificate input, export password, and private key first.
  This happens **before notarization** and is not an Apple rejection.
- Credential validation failure: check the Apple ID, app-specific password, team,
  and any developer-account agreement requirements.
- Notarization rejection: use the submission ID reported by `cargo volt release`:
  `xcrun notarytool log ID --keychain-profile mdstar`. Inspect Apple's actual log.
- Do not move an existing release tag just to change workflow code. Validate a
  fix using a branch's manual workflow run, then choose a new version/tag or an
  explicitly approved recovery plan.
