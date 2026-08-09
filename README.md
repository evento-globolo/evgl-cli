# evgl-cli

Evento Globolo operational CLI. Every option is translated to an environment
variable by `flags-2-env` using `.cli-flags.toml`; application code reads one
configuration surface.

The `flags-2-env` Rust client is pinned as a Git submodule under
`vendor/flags-2-env`. CI checks out submodules recursively.

```sh
export EVGL_TOKEN='your JWT'
cargo run -- providers --api-url http://localhost:8080
cargo run -- create-event --title "Rust Lima" --summary "Monthly meetup"               --starts-at 2026-09-04T23:00:00Z --ends-at 2026-09-05T01:00:00Z               --timezone America/Lima --canonical-url https://evento.example/e/rust-lima
cargo run -- cross-post --event-id ... --provider meetup --connection-id ...               --target-options '{"group_urlname":"rust-lima"}'
cargo run -- watch --job-id ...
```

`EVGL_TOKEN` is environment-only and intentionally has no CLI flag or default.
