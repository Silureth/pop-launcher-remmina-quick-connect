# pop-launcher-remmina-quick-connect

A [pop-launcher](https://github.com/pop-os/launcher) plugin that opens your saved [Remmina](https://remmina.org) connections (RDP, VNC, SSH, …) straight from the launcher.

Trigger it by typing `rem` followed by your search query. Each result shows the connection name with its group, protocol and `user@server`, and you can search by any of them, e.g. `rem office db` or `rem 10.0.1`. With no query, the most recently used connections are listed first.

## Requirements

- [pop-launcher](https://github.com/pop-os/launcher)
- Remmina, with the `remmina` command in `PATH` (for the Flatpak, see [Configuration](#configuration))

The plugin is a single static binary for x86_64 or ARM64 Linux, so nothing else needs to be installed.

## Install

1. Download the latest release into your pop-launcher plugins folder:

   ```sh
   mkdir -p ~/.local/share/pop-launcher/plugins
   curl -fsSL "https://github.com/Silureth/pop-launcher-remmina-quick-connect/releases/latest/download/remmina-quick-connect-$(uname -m).tar.gz" \
     | tar -xz -C ~/.local/share/pop-launcher/plugins
   ```

2. Restart pop-launcher so it picks up the plugin:

   ```sh
   pkill pop-launcher
   ```

   It starts again the next time you open the launcher.

**To update**, repeat both steps. Your `config.json` is kept, since the release only contains `config.example.json`.

## How it finds connections

Connections are read from the `.remmina` files in Remmina's data folder. The plugin uses the custom folder set in Remmina's preferences (`datadir_path` in `remmina.pref`) if there is one, and also looks in the default locations for native, Flatpak and legacy installs:

- `~/.local/share/remmina`
- `~/.var/app/org.remmina.Remmina/data/remmina`
- `~/.remmina`

Only the `name`, `group`, `protocol`, `server` and `username` fields are read. Passwords and other settings are never touched. Selecting a result runs `remmina -c <file>`.

Results are ordered by when each profile file was last changed. Remmina rewrites a profile when you use it, so recent connections end up on top.

## Configuration

The defaults below apply out of the box. To change them, copy `config.example.json` to `config.json` in the plugin folder and edit it:

| Option | Default | Description |
|---|---|---|
| `remmina_command` | `["remmina"]` | Command used to launch Remmina; `-c <file>` is appended. For the Flatpak use `["flatpak", "run", "org.remmina.Remmina"]` |
| `data_dir` | `null` | Folder with the `.remmina` files. `null` detects it as described above |

### Changing the trigger

The trigger word is set in `plugin.ron`, in the `regex` and `help` fields under `query`. For example, to use `rdp` instead of `rem`:

```ron
regex: "^rdp( |$)",
help: "rdp",
```

Then run `pkill pop-launcher`. Releases include `plugin.ron`, so to keep your change when updating, leave it out when extracting:

```sh
curl -fsSL "https://github.com/Silureth/pop-launcher-remmina-quick-connect/releases/latest/download/remmina-quick-connect-$(uname -m).tar.gz" \
  | tar -xz -C ~/.local/share/pop-launcher/plugins --exclude='*/plugin.ron'
```

## Development

Requires a [Rust toolchain](https://rustup.rs).

```sh
cargo test
cargo build --release
```

To run your local build in the launcher, copy it with `plugin.ron` into the plugin folder:

```sh
dir=~/.local/share/pop-launcher/plugins/remmina-quick-connect
mkdir -p "$dir"
cp target/release/remmina-quick-connect plugin.ron "$dir/"
pkill pop-launcher
```

The plugin speaks JSON over stdin/stdout, so you can also try it directly in the terminal:

```sh
printf '{"Search":"rem my server"}\n"Exit"\n' | cargo run
```

### Releasing

Bump `version` in `Cargo.toml`, commit, then push a matching tag:

```sh
git tag v1.0.0
git push origin v1.0.0
```

The release workflow builds static x86_64 and ARM64 binaries and attaches them to a GitHub release.

## Changelog

- **2026-11-11** **v1.0.0:** first release

## AI assistance

This plugin was written with [Claude Code](https://claude.com/claude-code), Anthropic's AI coding assistant.
