# Running Funveil under gVisor

## What is gVisor

[gVisor](https://gvisor.dev/) is an application kernel that provides an additional layer of isolation between containers and the host OS. Its runtime, `runsc`, intercepts application syscalls and implements them in a sandboxed user-space kernel — limiting the attack surface even if a container is compromised.

## Why use gVisor with Funveil

Funveil controls **file visibility** within AI agent workspaces, ensuring agents only see what they should. gVisor adds **OS-level sandboxing**, restricting what syscalls the process can make regardless of file visibility. Together they provide defense-in-depth:

| Layer | Tool | Protects against |
|-------|------|------------------|
| Application | Funveil | Agents reading sensitive files/code |
| OS / Syscall | gVisor (runsc) | Container escape, kernel exploits |

If you are running untrusted AI agents against a codebase, pairing funveil with gVisor is a natural fit.

## Installation

### Debian / Ubuntu

```bash
# Add gVisor signing key and repository
curl -fsSL https://gvisor.dev/archive.key | sudo gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" \
  | sudo tee /etc/apt/sources.list.d/gvisor.list > /dev/null

# Install runsc
sudo apt-get update && sudo apt-get install -y runsc

# Register runsc with Docker
sudo runsc install
sudo systemctl restart docker

# Verify
docker run --runtime=runsc --rm hello-world
```

## Running Funveil under gVisor

Pass `--runtime=runsc` to `docker run`. The funveil distroless image works with no changes:

```bash
# Initialize a workspace in blacklist mode
docker run --runtime=runsc --rm \
  -v $(pwd):/workspace -w /workspace \
  ghcr.io/e6qu/funveil:latest init --mode blacklist

# Veil a sensitive file
docker run --runtime=runsc --rm \
  -v $(pwd):/workspace -w /workspace \
  ghcr.io/e6qu/funveil:latest veil secrets.env

# Check status
docker run --runtime=runsc --rm \
  -v $(pwd):/workspace -w /workspace \
  ghcr.io/e6qu/funveil:latest status

# Unveil when done
docker run --runtime=runsc --rm \
  -v $(pwd):/workspace -w /workspace \
  ghcr.io/e6qu/funveil:latest unveil --all
```

### Shell alias

For convenience, add to your shell profile:

```bash
alias fv='docker run --runtime=runsc --rm -v $(pwd):/workspace -w /workspace ghcr.io/e6qu/funveil:latest'
```

Then use it like a native command:

```bash
fv init --mode blacklist
fv veil secrets.env
fv status
```

## Limitations

- **Linux only** — gVisor requires a Linux host. It does not run on macOS or Windows.
- **Performance overhead** — syscall interception adds minor latency. For funveil's workload (file I/O) the overhead is negligible.
- **Not all syscalls supported** — gVisor implements a subset of the Linux syscall table. Funveil's operations (file read/write, directory traversal) are fully supported.
