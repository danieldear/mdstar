# MD Star on Volt

The native MD Star app, drawn with [Volt UI Kit](../../../rust/volt-ui-kit):
a Markdown reader and editor with a workspace file tree, tabs, find, a
split editor with live preview and a native macOS shell.

It depends on a local checkout of the kit, so it builds outside the main
workspace:

```sh
cargo run --release -- README.md      # a file, or a folder to browse
```

## Command line

The app bundle also carries `md`, the terminal viewer and live-preview
editor: `Contents/MacOS/md` links to the app's executable, which runs the
terminal viewer instead of opening a window when started under that name.

```sh
ln -sf "/Applications/MD Star.app/Contents/MacOS/md" ~/.local/bin/md   # or ../../install.sh --link-app
md README.md              # view; e edits with live preview, q quits
md --plain README.md      # print the rendered document
md --app README.md        # open it in the app
```

## Packaging

The app bundle, its document types and its signing are described in
`[package.metadata.volt.bundle]` in `Cargo.toml` and built with the kit's
`cargo volt` tool:

```sh
cargo volt bundle                         # target/release/bundle/MD Star.app
cargo volt release --universal --dry-run  # show the release steps
cargo volt release --universal            # signed, notarized MD-Star-<version>.dmg
```

`cargo volt release` signs with the Developer ID certificate in the keychain
and notarizes with the `mdstar` keychain profile. Create that profile once:

```sh
xcrun notarytool store-credentials mdstar --apple-id YOU@EXAMPLE.COM --team-id A822K85XUW
```

Install `cargo volt` from the kit with
`cargo install --path ../../../rust/volt-ui-kit --features dev-tools --bin cargo-volt`.
