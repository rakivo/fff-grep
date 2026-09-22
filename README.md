# fff-grep

The CLI version of `fff`, used to benchmark against `rawgrep`.

`fff-grep` is a long-lived, indexed grep daemon (`fffd`) plus a thin client (`fffq`).

## How it works

- `fffd <dir>` starts the watcher: indexes `<dir>` in the background, watches it for changes, and listens on `/tmp/fffd.sock`.
- `fffq <pattern>` talks to the watcher by a unix socket and prints back matches as `path:line: content`.

## Usage

```console
cargo b --profile=release-fast
./target/release-fast/fffd <directory to watch> &
./target/release-fast/fffq <pattern>
```

> [!WARNING]
> All resources `fff` uses are configured to be unbound, so it can use a lot of memory on a large directory!

> [!WARNING]
> Only one `fffd` can run at a time, since for simplicity, the socket path is hardcoded to `/tmp/fffd.sock`. Starting a second instance (e.g. to watch a different directory) will conflict with the first.
