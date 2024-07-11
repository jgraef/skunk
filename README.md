# `skunk` - 🦨 A person-in-the-middle proxy

**work in progress**

This crate is going to move to another Github namespace, once it's usable. See [issues][2] for our progress.

## What is this?

`skunk` 🦨 is a [person-in-the-middle][1] proxy, mainly focussed on HTTP(S), but also open for other protocols.
It's useful for API reverse engineering among other things.

## Development

Useful environment variables:

```
# Path to configuration directory. Defaults to `~/.local/feralsec/skunk`.
# This can also be set using the `-c` or `--config` command-line argument.
SKUNK_CONFIG=./my_test_config/

# Serve UI from the workspace and enable auto-reload.
SKUNK_UI_DEV=1

# Sets the country-code for running hostapd. This is required, when using hostapd.
HOSTAPD_CC=US
```

[1]: https://en.wikipedia.org/wiki/Man-in-the-middle_attack
[2]: https://github.com/jgraef/skunk/issues
