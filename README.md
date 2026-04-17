# gvm

A small command-line tool for managing a single Google Compute Engine VM you use as a development or work machine. It can check the instance's status, start it on demand, wait for SSH to become available, and then drop you into an SSH session.

`gvm` is intentionally minimal: it targets exactly one instance (identified by name) in one project, configured entirely through environment variables.

## Features

- **`gvm status`** — print whether the instance is `RUNNING` or `STOPPED`.
- **`gvm start`** — start the instance (if it isn't already running), wait for it to accept SSH connections, and then SSH in.
- **`gvm ssh`** — SSH directly to the instance's current IP without touching its power state.

Any extra arguments after the subcommand are passed through to `ssh`, so you can do things like `gvm ssh -- ls ~` or `gvm start -- tmux attach`.

## Installation

You need a recent Rust toolchain (stable) and Cargo.

```sh
git clone <this-repo>
cd gvm
cargo build --release
```

The resulting binary will be at `target/release/gvm`. Copy or symlink it onto your `PATH`, e.g.:

```sh
install -m 0755 target/release/gvm ~/.local/bin/gvm
```

Or install straight from the source directory:

```sh
cargo install --path .
```

## Authentication

`gvm` uses Google Cloud application default credentials. Before running it, make sure one of the following is true:

- `GOOGLE_APPLICATION_CREDENTIALS` points to a service account key file, **or**
- You've run `gcloud auth application-default login` to create user credentials at the default location
  (`~/.config/gcloud/application_default_credentials.json` on Linux/macOS, `%APPDATA%\gcloud\application_default_credentials.json` on Windows).

If neither is present, `gvm` refuses to run and tells you how to fix it.

The credentials you use must have sufficient permissions on the target project to list, get, and start Compute Engine instances — for example the `roles/compute.instanceAdmin.v1` role, or a tighter custom role with `compute.instances.list`, `compute.instances.get`, and `compute.instances.start`.

## Configuration

All configuration is read from environment variables.

| Variable | Required for | Description |
| --- | --- | --- |
| `GOOGLE_CLOUD_PROJECT` | all commands | GCP project ID that owns the instance. |
| `GVM_INSTANCE` | all commands | Name of the Compute Engine instance. |
| `GVM_USER` | `ssh`, `start` | Local username on the VM to SSH as. |
| `GVM_START_TIMEOUT` | `start` (optional) | Seconds to wait for SSH to come up. Default: `180`. |
| `GOOGLE_APPLICATION_CREDENTIALS` | optional | Path to a service account key file. |

`gvm` looks the instance up via the aggregated list, so you don't have to specify its zone — it's discovered automatically.

### Example

```sh
export GOOGLE_CLOUD_PROJECT=my-gcp-project
export GVM_INSTANCE=dev-box
export GVM_USER=alice
```

## Usage

```sh
gvm <start|status|ssh> [args...]
```

### `gvm status`

Prints `RUNNING` or `STOPPED` (anything other than RUNNING is reported as STOPPED).

```sh
$ gvm status
RUNNING
```

### `gvm start`

Starts the instance if necessary, polls until it's `RUNNING` and TCP port 22 is reachable, then execs into an SSH session. If the instance is already running it skips straight to the SSH step.

```sh
$ gvm start
Starting instance `dev-box` in zone us-central1-a (current status: TERMINATED)...
  instance status: PROVISIONING
  instance status: STAGING
  instance status: RUNNING
  probing SSH on 34.123.45.67:22 ...
SSH is ready on 34.123.45.67
Connecting to alice@34.123.45.67 (instance `dev-box` in zone us-central1-a)
```

You can pass extra arguments through to `ssh`:

```sh
gvm start -- -A           # forward your SSH agent
gvm start -- tmux attach  # run a command instead of an interactive shell
```

If the instance doesn't become reachable within `GVM_START_TIMEOUT` seconds, `gvm` exits with an error.

### `gvm ssh`

SSH to the instance at its current IP without trying to start it. Useful when you know it's already running and don't want the extra status polling.

```sh
gvm ssh
gvm ssh -- uptime
```

## How it picks an IP

For both `ssh` and `start`, `gvm`:

1. Prefers the first external (NAT) IP on any network interface's access config.
2. Falls back to the first network interface's internal IP if no external IP is present.

If neither is available, the `ssh` subcommand fails with an error.

## Exit codes

- `gvm status` exits `0` on success.
- `gvm ssh` and `gvm start` propagate `ssh`'s exit code.
- Any configuration or API error causes a non-zero exit with a message on stderr.

## Troubleshooting

- **`GVM_INSTANCE environment variable is not set.`** — export the variables listed under [Configuration](#configuration).
- **`No Google Cloud credentials found.`** — run `gcloud auth application-default login` or point `GOOGLE_APPLICATION_CREDENTIALS` at a valid key file.
- **`instance \`X\` not found in project \`Y\``** — double-check `GVM_INSTANCE` and `GOOGLE_CLOUD_PROJECT`; the instance must exist somewhere in the project's aggregated list.
- **`timed out after Ns waiting for ... to accept SSH`** — the VM started but port 22 didn't open in time. Increase `GVM_START_TIMEOUT`, or check firewall rules / sshd on the instance.

## License

MIT
