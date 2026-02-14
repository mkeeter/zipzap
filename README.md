`zipzap` is a tool for tracking and jumping into frequently-used directories.

It's a cousin of the well-regarded [`z` tool](https://github.com/rupa/z),
making a slightly different set of choices:

- A single binary written in Rust, instead of a `bash` script using `awk` for
  the heavy lifting
- Data is stored in a SQLite database, instead of a flat file
- Only supports `frecency` metric for selecting directories
- always case-insensitive
- Easy shell integration: just `zipzap install`
- Modern CLI design (using `clap` for argument parsing)

It uses the same metrics / scoring as `z`, and can import existing data from
`~/.z` into its own database.

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
This automatically detects the parent shell and installs hooks for `fish`,
`bash`, or `zsh`.  You'll need to restart your shell afterwards.

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
  but keep track of the last known working directory, ony calling `zipzap add`
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
`zipzap install` appends a line to the startup script (e.g. `~/.bashrc`) which
simply calls `eval $(zipzap source bash)`.
