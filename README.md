![zipzap logo](https://raw.githubusercontent.com/mkeeter/zipzap/refs/heads/main/logo.png)

[![Crates.io](https://img.shields.io/crates/v/zipzap)](https://crates.io/crates/zipzap)
[![License](https://img.shields.io/crates/l/zipzap)](LICENSE.txt)
[![Build Status](https://img.shields.io/github/actions/workflow/status/mkeeter/zipzap/ci.yaml?branch=main)](https://github.com/mkeeter/zipzap/actions/workflows/ci.yaml?query=branch%3Amain)

`zipzap` is a tool for tracking and jumping into frequently-used directories.
It records which working directories you spend your time in, then provides a
shell function to jump to them quickly.

It's a cousin of the well-regarded [`z` tool](https://github.com/rupa/z),
making a slightly different set of choices:

- A single binary written in Rust, instead of a `bash` script using `awk` for
  the heavy lifting
- Data is stored in a SQLite database, instead of a flat file
- Only supports `frecency` metric for selecting directories
- always case-insensitive
- Easy shell integration: just `zipzap install`
- Modern CLI design
  (using [`clap`](https://github.com/clap-rs/clap) for argument parsing)

It uses the same metrics / scoring as `z`, and can import existing data from
`~/.z` into its own database.

# Quick start
```shell
cargo install --locked zipzap   # installs the binary
zipzap install                  # installs shell integrations
zipzap db import                # imports an existing `z` database
# restart your shell to load shell integrations, then use `z` to jump around
```
Keep reading for details on each of these steps!

# Installation
## `zipzap` command
```
cargo install --locked zipzap
```
After installing, try running `zipzap` to check that the Cargo binary directory
(e.g. `~/.cargo/bin`) is in your shell's path.

## Shell integration
Shell integration is needed to track directory changes and to jump around with
`z`.  The `zipzap` binary knows how to install it automatically:
```
zipzap install
```
This detects the parent shell and installs hooks for `fish`, `bash`, or `zsh`.
You'll need to restart your shell afterwards.

## Importing an existing `z` database
```
zipzap db import
```
This imports a `z` database from the default location (`~/.z`).  If the `zipzap`
database already exists, then the import uses timestamps to decide whether to
overwrite rows in the database.

# Details on shell integration
Shell integration should Just Work™ with `zipzap install`.

If you want to do it manually, there are two parts:

First, we need a hook which calls `zipzap add` when the directory changes.
The exact form of the directory hook depends on the shell:

- For `fish`, we add a hook function which watches `PWD` and calls `zipzap add`
  when it changes
- For `bash`, we add the hook to `PROMPT_COMMAND`, so it's run on every prompt –
  but keep track of the last known working directory, only calling `zipzap add`
  when it _changes_
- `zsh` is similar to bash, but stores the hook in `precmd_functions` instead

Second, we need a shell function (typically `z`) which calls `zipzap find` then
sets the result as the new working directory.  This function is similar for all
shells (and is indeed identical for `bash` and `zsh`, which are often
compatible).

Finally, we need a way to load this hook and function into the shell.

In `fish`, the hook is written to `~/.config/fish/conf.d/z.fish` and the `z`
function is written to `~/.config/fish/functions/z.fish`; these files are both
written by `zipzap install` and automatically loaded by the shell.

For `bash` and `zsh`, it's a little more complicated.  `zipzap source
[bash,zsh]` prints a script which loads the hook and `z` function into the
shell; this script can be executed with `eval` to configure the current shell.
`zipzap install` appends an `eval` line to the shell's startup script, e.g.
adding `eval $(zipzap source bash)` to your `~/.bashrc`.
