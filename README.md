# evgl-cli

Rust Evento Globolo CLI integrated through flags-2-env for safe import, publish, and operations workflows.

Initialized through `DEN-1889` as a testable `cli` foundation. Product behavior continues through focused pull requests.

```bash
python3 scripts/verify_repo.py
```

## Environment secrets

Secrets live in this repo **encrypted** with [sops](https://github.com/getsops/sops) + [age](https://github.com/FiloSottile/age):
`env/enc/<dev|prod>.env.enc` is committed; `just env-use <name>` decrypts it to
`env/dec/<name>.env` (gitignored, mode 0600) and symlinks `./.env` to it. The
Nix dev shell provides the tooling, `just env-audit` runs keyless in CI, and
containers decrypt at `docker run` — never at build. See [`env/README.md`](env/README.md).
