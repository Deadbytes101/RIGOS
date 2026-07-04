# JSON CLI Contract

JSON mode emits exactly one document followed by one newline. Human and JSON renderers consume the same typed model.

- Envelope: `rigos.cli-envelope/v1`
- Payloads: `machine-snapshot/v1`, `miner-snapshot/v1`, `doctor-report/v1`
- Status: `ok`, `partial`, `error`
- Timestamps: UTC RFC 3339 with milliseconds and `Z`
- Exit codes: 0 success/acceptable partial, 2 usage, 3 inspection error, 4 internal serialization/invariant failure

Consumers must ignore unknown object fields and must not depend on key order. Existing field names, types, units, nullability and enum meanings are immutable within a schema version. Breaking payload changes increment only that payload version.

Diagnostics use stable `code`, `severity`, and `component`; automation must not parse human `message` text. Secret values, authorization headers, URL userinfo, private keys and raw secret-bearing configuration are forbidden from all renderers and diagnostics.

Kernel-derived units are explicit: bytes, seconds, millicelsius and hashes per second. Unavailable observations carry a reason rather than a fabricated value.
