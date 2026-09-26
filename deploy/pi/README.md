# Running pod-controller on a Raspberry Pi

`pod-cli serve` (web editor on :8080, MCP at /mcp) runs fine on a Pi Zero 2 W
next to Companion Satellite. It uses about 3 MB of RAM. Its USB port is
high-speed, so the POD works directly or through a USB 2.0 hub (never a
USB 1.1 hub: at full speed the POD has no control interface).

Build a static binary on any Linux x86-64 box, so it doesn't depend on the
Pi's glibc:

```
rustup target add aarch64-unknown-linux-musl
sudo apt install gcc-aarch64-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --target aarch64-unknown-linux-musl -p pod-cli
```

Copy `target/aarch64-unknown-linux-musl/release/pod-cli`,
`crates/pod-cli/ui/` and this directory to the Pi, then run `install.sh`
there. It installs to `/opt/pod-controller` and runs the service as a
`podctl` system user. A udev rule gives only that group access to the POD,
and the kernel's `snd_usb_podhd` driver is blacklisted.

`MemoryMax` in the unit only applies if the kernel has the memory cgroup
controller on (Raspberry Pi OS kernels need `cgroup_enable=memory` in
`cmdline.txt`).
