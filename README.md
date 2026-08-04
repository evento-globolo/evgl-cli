# evgl-cli

flags-2-env operator CLI for Evento Globolo health, listing, and WebSocket event watching.

**Product:** Evento Globolo — A global event discovery and aggregation platform.

Aggregate, normalize, deduplicate, search, and follow events from sources such as Eventbrite, Meetup, LinkedIn, Facebook, and Craigslist through authorized APIs or permitted ingestion paths.

## Safety and production boundary

Provider names are integration targets, not claims of affiliation. Use official APIs and permitted data-access methods; do not bypass authentication, anti-bot, rate-limit, copyright, or platform-policy controls.

This repository is an executable bootstrap, not a production deployment. Before live
use, add authentication, tenant authorization, rate limits, durable migrations,
observability, backups, incident response, dependency review, and secret management.
## Examples

```bash
cargo run -- health
cargo run -- --api-url http://127.0.0.1:8080 list
cargo run -- watch
```

Precedence is `CLI > environment > schema default`. The CLI audits
`.cli-flags.toml`, rejects unknown options and parse errors, and crosses into typed
configuration once before network work.
