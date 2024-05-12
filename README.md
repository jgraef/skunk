# `skunk` 🦨 A person-in-the-middle proxy

work in progress

# TODO

 - filters (switch):
   - [x] parsing mitmproxy filter expressions
   - [ ] effects of filters?
   - [ ] build `Layer` stack from filter set?
   - build hard-coded `Layer` so we can prototype stuff?
 - http proxy server
 - socks/http proxy `Connect` impl
 - tor `Connect` impl (with [arti][1])
 - UI: TUI or web or both? TUI can be directly integrated into `skunk-cli` or connect via network
 - network: [axum][2] with websocket, something like [remoc][1] for transparent channels?
 - storing flows: [sqlx][4] with sqlite

[1]: https://docs.rs/arti-client/latest/arti_client/index.html
[2]: https://docs.rs/axum/latest/axum/index.html
[3]: https://github.com/ENQT-GmbH/remoc
[4]: https://docs.rs/sqlx/latest/sqlx/index.html
