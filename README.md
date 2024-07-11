# `skunk` - 🦨 A person-in-the-middle proxy

[![Build](https://github.com/jgraef/skunk/actions/workflows/build.yaml/badge.svg)](https://github.com/jgraef/skunk/actions/workflows/build.yaml)

**work in progress**

This crate is going to move to another Github namespace, once it's usable. See [issues][2] for our progress.

## What is this?

`skunk` 🦨 is a [person-in-the-middle][1] proxy, mainly focussed on HTTP(S), but also open for other protocols.
It's useful for API reverse engineering among other things.

## Development

### Generate root certificate

In order for `skunk` to decrypt TLS traffic, you have to install a certificate as trusted root certificate on the device you're intercepting.

To generate the root certificate, run `cargo run --bin skunk -- generate-cert`. `skunk` will output the location of the certificate (if you have logging set to `INFO`).

### Build UI

To build the UI, you'll need [`trunk`][3] and [`stylance`][4]. Then run `trunk build` (optionally with `--watch` flag) in the `skunk-ui` directory.
You do not need to use `trunk serve`, as `skunk-cli` serves the UI itself (with auto-reload support).

### Running the proxy

To run the proxy, run `cargo run --bin skunk -- proxy --socks --api`.

### Useful environment variables

```
# Set global logging level to WARN, and for skunk crates to DEBUG.
RUST_LOG=warn,skunk=debug

# Path to configuration directory. Defaults to `~/.local/feralsec/skunk`.
# This can also be set using the `-c` or `--config` command-line argument.
SKUNK_CONFIG=./my_test_config/

# Serve UI from the workspace and enable auto-reload.
SKUNK_UI_DEV=1

# Sets the country-code for running hostapd. This is required, when using hostapd.
HOSTAPD_CC=US
```

You can also put your environment variables in a `.env` file.


[1]: https://en.wikipedia.org/wiki/Man-in-the-middle_attack
[2]: https://github.com/jgraef/skunk/issues
[3]: https://trunkrs.dev/
[4]: https://github.com/basro/stylance-rs
