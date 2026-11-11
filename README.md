# runY

A small Linux container runtime written in Rust

runY launches a command in separate UTS, PID, and mount namespaces, switches to a prepared root filesystem with `pivot_root`, and mounts a fresh `/proc`. It waits for the command to finish and returns its exit code.

## Build

From the project directory inside Linux:

```sh
cargo build --locked
```

The executable is `./target/debug/runY`.

## Prepare a root filesystem

Use a fresh directory for extraction. The archive below comes from the [official Alpine release directory](https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/).

```sh
curl -LO https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/alpine-minirootfs-3.24.0-aarch64.tar.gz
curl -LO https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/alpine-minirootfs-3.24.0-aarch64.tar.gz.sha256
sha256sum -c alpine-minirootfs-3.24.0-aarch64.tar.gz.sha256

mkdir rootfs
sudo tar -xzf alpine-minirootfs-3.24.0-aarch64.tar.gz -C rootfs
sudo mkdir -p rootfs/proc rootfs/dev
```

The rootfs must contain the command you want to run, along with its dynamic loader and libraries if required. Extract the complete minirootfs rather than copying `/bin/sh` alone.

`/proc` must exist before launch: runY mounts procfs there but does not create the directory. Device files are not prepared automatically. `/dev` may remain empty for the examples below; commands that need `/dev/null`, `/dev/zero`, or other devices require additional preparation. runY does not mount the VM's `/dev` into the container.

The rootfs is writable, and changes made by the command persist between runs. Use one run at a time for a given rootfs: setup temporarily creates a `.oldroot` directory inside it.

## Run

```sh
sudo ./target/debug/runY run --rootfs ./rootfs --hostname demo -- /bin/sh -c 'hostname; echo $$; pwd; ps'
```

Expected behavior:

- `hostname` prints `demo`.
- The shell prints PID `1`.
- `pwd` prints `/`.
- `ps` lists processes from the container's PID namespace.

`--rootfs` is required and accepts a relative or absolute path. `--hostname` is optional; if omitted, runY generates a UUID. Arguments after `--` form the command executed inside the rootfs.

To open a shell:

```sh
sudo ./target/debug/runY run --rootfs ./rootfs -- /bin/sh
```

## How it works

The process chain is `run → __bootstrap → __container → exec(command)`.

1. `run` starts an internal bootstrap process and waits for its result.
2. `__bootstrap` resolves the rootfs path, creates UTS, PID, and mount namespaces, makes mount propagation private, and sets the hostname. It then starts `__container` and waits for it.
3. `__container` is the first child in the new PID namespace, so it has PID 1. It bind-mounts the rootfs onto itself, creates `.oldroot`, calls `pivot_root`, changes its working directory to `/`, detaches the old root, and removes `.oldroot`.
4. It mounts fresh procfs, closes inherited file descriptors numbered 3 and above, and replaces itself with the requested command through `exec`. Standard input, output, and error remain connected to the caller.

Preparation errors stop execution before the requested command starts. The command's exit code is returned through both waiting processes; signal termination is represented as `128 + signal number`.

## Current limitations

This is an educational runtime for trusted commands in a test VM. It is not ready for untrusted workloads.

- The command itself runs as PID 1. A dedicated init, signal forwarding, and child reaping are not implemented yet.
- Full interactive terminal support, including PTY allocation and job control, is not implemented.
- Network, IPC, user, and cgroup namespaces are not isolated.
- Resource limits, capability reduction, seccomp, and `no_new_privs` are not implemented.
- The command inherits the caller's environment and runs with the privileges used to launch runY.
- Rootfs downloads, device preparation, OCI images, and container lifecycle commands are not managed by the runtime.
- A failed setup may leave `.oldroot` in the prepared rootfs. Inspect the failure and remove the leftover empty directory before retrying.
