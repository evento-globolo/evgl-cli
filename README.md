# evgl-cli

Evento Globolo operational CLI. Every option is translated to an environment
variable by `flags-2-env` using `.cli-flags.toml`; application code reads an
immutable `EnvMap` snapshot and never mutates the process environment.

```sh
export EVGL_TOKEN='your JWT'
cargo run -- providers --api-url http://localhost:8080
cargo run -- create-event --title "Rust Lima" --summary "Monthly meetup" \
              --starts-at 2026-09-04T23:00:00Z --ends-at 2026-09-05T01:00:00Z \
              --timezone America/Lima --canonical-url https://evento.example/e/rust-lima
cargo run -- cross-post --event-id ... --provider meetup --connection-id ... \
              --target-options '{"group_urlname":"rust-lima"}'
cargo run -- watch --job-id ...
cargo run -- health
cargo run -- list
```

`EVGL_TOKEN` is environment-only and intentionally has no CLI flag or default.

```bash
python3 scripts/verify_repo.py
```

## Environment secrets

Secrets live in this repo **encrypted** with [sops](https://github.com/getsops/sops) + [age](https://github.com/FiloSottile/age):
`env/enc/<dev|prod>.env.enc` is committed; `just env-use <name>` decrypts it to
`env/dec/<name>.env` (gitignored, mode 0600) and symlinks `./.env` to it. The
Nix dev shell provides the tooling, `just env-audit` runs keyless in CI, and
containers decrypt at `docker run` — never at build. See [`env/README.md`](env/README.md).
