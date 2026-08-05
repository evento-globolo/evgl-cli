# Architecture

Rust Evento Globolo CLI integrated through flags-2-env for safe import, publish, and operations workflows.

## Fleet

- `evgl-interfaces`
- `evgl-api`
- `evgl-mash-web`
- `evgl-leptos-web`
- `evgl-dioxus-web`
- `evgl-sync`
- `evgl-cli`
- `evgl-infra`
- `evento-globolo-clients`
- `evento-globolo-libs`
- `evento-globolo.github.io`
- `evento-globolo-monorepo`

Interfaces own wire formats; libraries own reusable domain behavior; clients consume versioned contracts; runtimes own deployment behavior; monorepos coordinate pinned revisions. Edge code is allowlisted and never a generic proxy.
