# spinel

Control networking devices using the Spinel protocol.

# Running
```
cargo run --bin spinel-cli -- -p ${DEVTTY}
```

# Developing

While developing, you can run the `socat` script in `scripts/socat.sh`.

## Macos

For MacOS, use the `/dev/cu.usbmodemXXXX` instead of the `/dev/tty.usbmodemXXXX`. Additionally, use a 0 baud.

```
./scripts/socat.sh -u /dev/cu.usbmodemXXXX
RUST_LOG=trace cargo run --bin spinel-cli -- -p debug-serial -b 0
```
