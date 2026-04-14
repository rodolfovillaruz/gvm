# gvm — Google Cloud VM Manager

A fast, single-binary CLI tool written in Rust for managing a Google Cloud Compute Engine VM instance from your terminal.

## Features

- **Start** a stopped VM with a single command
- **Status** check with external/internal IP, machine type, and zone
- **SSH** directly into the VM — IP resolution happens automatically under the hood
- Authenticates via **Application Default Credentials** (no API keys to manage)
- Colored, human-friendly terminal output
- Cross-platform (Unix `exec` for SSH, child process fallback on Windows)

## Prerequisites

| Requirement | How to install |
|---|---|
| **Rust 1.70+** | [rustup.rs](https://rustup.rs/) |
| **Google Cloud SDK** | [cloud.google.com/sdk/docs/install](https://cloud.google.com/sdk/docs/install) |
| **Application Default Credentials** | `gcloud auth application-default login` |
| **ssh** on your `$PATH` | Pre-installed on macOS/Linux; on Windows use Git Bash or WSL |

Your GCP account needs at least the **Compute Instance Admin (v1)** role on the target project, or equivalent permissions:

- `compute.instances.get`
- `compute.instances.start`

## Installation

### From source

```bash
git clone https://github.com/rodolfovillaruz/gvm.git
cd gvm
cargo build --release
```

The binary will be at `target/release/gvm`. Copy it somewhere on your `$PATH`:

```bash
cp target/release/gvm ~/.local/bin/
```

### Quick run (development)

```bash
cargo run -- --project my-project --instance my-vm status
```

## Configuration

Every invocation needs three pieces of information: **project**, **zone**, and **instance**.
You can supply them as flags or as environment variables.

| Flag | Environment Variable | Default | Required |
|---|---|---|---|
| `--project` | `GVM_PROJECT` | — | ✅ |
| `--zone` | `GVM_ZONE` | `us-central1-a` | — |
| `--instance` | `GVM_INSTANCE` | — | ✅ |

### Recommended: use a `.env` or shell profile

```bash
# ~/.bashrc or ~/.zshrc
export GVM_PROJECT="my-project"
export GVM_ZONE="us-west1-b"
export GVM_INSTANCE="dev-workstation"
```

After sourcing, every command becomes short:

```bash
gvm start
gvm status
gvm ssh
```

## Usage

### Check VM status

```bash
gvm status
```

```
→  Fetching status for instance dev-workstation…
────────────────────────────────────────
  Instance:        dev-workstation
  Status:          RUNNING
  Zone:            us-west1-b
  Machine type:    e2-standard-4
  NIC[0] internal: 10.128.0.42
  NIC[0] external: 34.82.XXX.XXX
────────────────────────────────────────
```

### Start a stopped VM

```bash
gvm start
```

```
▶  Starting instance dev-workstation…
✓  Start operation operation-abc123 dispatched (status: RUNNING).
ℹ  The instance may take ~30 s to reach RUNNING state.
```

If the VM is already running:

```
▶  Starting instance dev-workstation…
✓  Instance is already RUNNING.
```

### SSH into the VM

```bash
# Default user: ubuntu
gvm ssh

# Custom user
gvm ssh --user admin

# Custom user + private key
gvm ssh --user admin --key ~/.ssh/gcp_key

# Forward a port (extra args go after --)
gvm ssh -- -L 8080:localhost:8080

# Run a remote command non-interactively
gvm ssh -- ls -la /var/log
```

What happens under the hood:

1. Calls the Compute Engine API to resolve the instance's **external NAT IP**
2. Spawns (or `exec`s on Unix) an `ssh` process with the resolved IP

## Project Structure

```
gvm/
├── Cargo.toml          # Dependencies and metadata
└── src/
    ├── main.rs         # CLI definition (clap) and entry point
    ├── config.rs       # VmConfig struct and URL builder
    ├── gcp.rs          # GCP authentication, Compute Engine REST API calls
    └── ssh.rs          # IP resolution + SSH process spawning
```

### Architecture

```
┌──────────┐     ┌──────────┐     ┌─────────────────────────────┐
│  main.rs │────▶│  gcp.rs  │────▶│  Compute Engine REST API v1 │
│  (clap)  │     │(gcp_auth)│     └─────────────────────────────┘
└──────────┘     └──────────┘
      │
      │  ssh subcommand
      ▼
┌──────────┐
│  ssh.rs  │──── exec ssh ───▶  interactive terminal session
└──────────┘
```

## Dependencies

| Crate | Purpose |
|---|---|
| [`clap`](https://crates.io/crates/clap) | Argument parsing with derive macros |
| [`tokio`](https://crates.io/crates/tokio) | Async runtime |
| [`reqwest`](https://crates.io/crates/reqwest) | HTTP client for the Compute Engine API |
| [`gcp_auth`](https://crates.io/crates/gcp_auth) | Application Default Credentials / token management |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | JSON (de)serialisation |
| [`anyhow`](https://crates.io/crates/anyhow) / [`thiserror`](https://crates.io/crates/thiserror) | Error handling |
| [`colored`](https://crates.io/crates/colored) | Terminal colours |

## Troubleshooting

### "Failed to initialise GCP authentication"

Make sure you have logged in:

```bash
gcloud auth application-default login
```

Or if running on a GCE instance / Cloud Shell, ensure the VM has a service account with the required scopes.

### "No external IP found on the instance"

The VM needs an **access config** with a NAT IP. You can add one in the console under
**VM instance → Network interfaces → External IP**, or via:

```bash
gcloud compute instances add-access-config INSTANCE \
  --zone ZONE \
  --access-config-name "External NAT"
```

### SSH hangs or is refused

- The VM may still be booting — wait 30 seconds after `gvm start` and retry.
- Ensure **TCP port 22** is allowed in your VPC firewall rules.
- Verify the username is correct for your OS image (`ubuntu`, `debian`, `ec2-user`, etc.).

## License

MIT
