# `skunk` 🦨 A person-in-the-middle proxy

work in progress

# TODO

 - [ ] filters (switch):
   - [x] parsing mitmproxy filter expressions
   - [ ] effects of filters?
   - [ ] build `Layer` stack from filter set?
   - [ ] build hard-coded `Layer` so we can prototype stuff
   - [ ] fix `fn_layer` lifetimes
 - [ ] proxy (server/client)
   - [x] socks proxy server
   - [ ] http proxy server
   - [ ] socks proxy `Connect` impl
   - [ ] http proxy `Connect` impl
   - [ ] tor `Connect` impl (with [arti][1])
 - [ ] UI: TUI or web or both? TUI can be directly integrated into `skunk-cli` or connect via network
 - [ ] network (for ui/daemon communication)
   - [ ] server: [axum][2] with websocket 
   - [ ] client: use [reqwest][6] and [reqwest-websocket][5]
   - [ ] protocol: something like [remoc][1] for transparent channels?
   - [ ] `Connect` (and `Layer`?) that serves the axum router when connecting through the socks/http proxy
 - [ ] storing flows: [sqlx][4] with sqlite

[1]: https://docs.rs/arti-client/latest/arti_client/index.html
[2]: https://docs.rs/axum/latest/axum/index.html
[3]: https://github.com/ENQT-GmbH/remoc
[4]: https://docs.rs/sqlx/latest/sqlx/index.html
[5]: https://github.com/jgraef/reqwest-websocket
[6]: https://docs.rs/reqwest/latest/reqwest/index.html
