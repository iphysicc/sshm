<p align="right"><b>English</b> · <a href="README.tr.md">Türkçe</a></p>

# sshm

[![CI](https://github.com/iphysicc/sshm/actions/workflows/ci.yml/badge.svg)](https://github.com/iphysicc/sshm/actions/workflows/ci.yml)

A terminal SSH connection manager. It runs your system OpenSSH, so keys, `known_hosts`,
2FA prompts and `~/.ssh/config` work exactly as they do today.

## Download

Prebuilt binaries are on the [Releases](https://github.com/iphysicc/sshm/releases/latest) page:

| System | File |
| --- | --- |
| Windows x64 | `sshm.exe`, or the `x86_64-pc-windows-msvc` zip |
| Windows ARM64 | `aarch64-pc-windows-msvc` zip |
| Linux x64 | `x86_64-unknown-linux-musl` (static, runs on any distro) |
| macOS Apple Silicon / Intel | `aarch64-apple-darwin` / `x86_64-apple-darwin` |

Put the binary in a folder on your `PATH`. You need an OpenSSH client (built into
Windows 10/11, macOS and most Linux distros). The binaries are not code-signed: Windows
SmartScreen may ask you to confirm (More info → Run anyway), and on macOS run
`xattr -d com.apple.quarantine sshm` once.

## Build from source

```powershell
cargo install --git https://github.com/iphysicc/sshm
```

Or clone the repo and run `cargo install --path .` inside it.

`sshm` is then available in any terminal. On first launch it imports `.bat`/`.cmd` files
that contain an `ssh` command from Documents and Desktop, plus `~/.ssh/config`.

## Usage

```
sshm               open the interface
sshm web           connect directly (a prefix works too: sshm we)
sshm --list        list connections
sshm --import      import again (existing endpoints are skipped)
sshm --lang en|tr  set the interface language
sshm --config      print the path of the connections file
sshm --version     print the version
```

| Key | Action |
| --- | --- |
| ⏎ | Connect in this terminal; you return to the list when the session ends |
| t | Connect in a new Windows Terminal tab |
| f | sftp |
| c | Copy the ssh command to the clipboard |
| / | Search; ⏎ connects when a single match is left |
| a / e / y / d | Add / edit / duplicate / delete |
| r | Check servers again |
| i | Import |
| l | Switch language (English / Türkçe) |
| ? | All shortcuts |

## Language

The interface is available in English and Turkish, with English as the default. On first
launch sshm asks which one you want. After that, the footer and the top bar always show
`l` as the language switch; press it to toggle, or run `sshm --lang tr`. The choice is
saved. `SSHM_LANG=tr` overrides it for a single run.

## Persistent sessions on the server

Put a name in the **tmux** field of a connection (e.g. `main`) and connecting runs
`tmux new -A -s main`. If the connection drops or you close it, the session keeps running
on the server, and reconnecting brings you back to where you were. The server needs
`tmux` installed (`sudo apt install tmux`).

Every connection gets `ServerAliveInterval=30` by default, so idle sessions don't drop.

## Status checks

At startup and on `r`, sshm opens one TCP connection to each server and reads the SSH
banner (latency + server version). It does not authenticate. This is intentionally not
periodic: frequent unauthenticated connections can trip fail2ban or OpenSSH's
PerSourcePenalties. Connections that use a jump host (ProxyJump) are not checked.

## Storage

`%APPDATA%\sshm\hosts.toml` on Windows (TOML, safe to edit by hand while sshm is not
running). Point `SSHM_CONFIG` at another file to use that instead.

## Releasing

Push a version tag and GitHub Actions builds every target and publishes the release:

```powershell
git tag v0.2.0
git push origin v0.2.0
```

Bump `version` in `Cargo.toml` first so `sshm --version` matches.

## License

MIT
