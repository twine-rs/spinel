# spinel

[![Crates.io][crates-badge]][crates-url]
[![MPL 2.0][mpl-badge]][mpl-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/spinel.svg
[crates-url]: https://crates.io/crates/spinel
[mpl-badge]: https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg
[mpl-url]: https://github.com/twine-rs/spinel/blob/main/LICENSE.md
[actions-badge]: https://img.shields.io/github/actions/workflow/status/twine-rs/spinel/ci.yml?branch=main
[actions-url]: https://github.com/twine-rs/spinel/actions/workflows/ci.yml?query=workflow%3ACI+branch%3Amain

Control networking devices using the Spinel protocol.

This workspace holds the `spinel` library and the `spinel-cli` package.
The library implements the host side of the protocol: the frame codecs
and a POSIX connection actor. The CLI package builds a binary named
`spinel` for controlling and testing Thread and 802.15.4 radio devices
over a serial link.

## Install

```
cargo install spinel-cli
```

The command installs a binary named `spinel`.

## Run

Monitor the device broadcasts:

```
spinel -p ${DEVTTY}
```

Enable raw link mode, print received frames, and transmit one frame:

```
spinel -p ${DEVTTY} raw --channel 11 --promiscuous full --transmit 0308a5ffffffff07
```

## Library features

The library core builds without `std` for embedded hosts:

```
cargo check -p spinel --no-default-features
```

The `alloc` feature adds allocation support. The `defmt` feature adds
defmt formatting for the codec types. The default set enables `std`,
which holds the tokio connection actor and the serial transport.

## Developing

The `socat` script bridges a serial device to a local PTY named
`debug-serial` and prints the traffic:

```
./scripts/socat.sh -u /dev/cu.usbmodemXXXX
RUST_LOG=trace cargo run --bin spinel -- -p debug-serial -b 0
```

On macOS, use the `/dev/cu.usbmodemXXXX` device instead of
`/dev/tty.usbmodemXXXX`, and set the baud rate to 0.

## License

MPL-2.0
