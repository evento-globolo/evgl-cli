# flags-2-env integration

This CLI consumes the `flags2env` Rust package produced from
`ORESoftware/flags-2-env` and follows the bundled runtime contract documented at
commit `b596f2d1c99bd726b262a923916e3015bc80bc37`. The package is expected to be released as version `0.1.0`.

Secrets are environment-only and listed under `[env].ignore`; they are never
declared as command-line flags or defaults.
