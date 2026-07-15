RIGOS DIRECT SYSTEM STATUS
==========================

PURPOSE
-------

Publish direct, signed operating-system evidence from a RIGOS appliance to
https://rigos.site without turning RIGOS into a hosted mining account or a
HiveOS-style remote owner.

The service is observation-only.

It does not accept or publish:

    wallet
    worker name
    pool
    hashrate
    shares
    hostname
    IP address
    password
    API token
    private key
    remote command

The appliance remains locally owned and locally operable when the website or
network is unavailable.


ROUTES
------

    POST /api/v1/observations
        Signed ingest endpoint used by rigos-status-agent.

    GET /api/v1/status
        Public sanitized JSON projection.

    GET /status
        Server-rendered status page with zero browser JavaScript.


INGEST CONTRACT
---------------

Maximum body size:

    65536 bytes

Required headers:

    Content-Type: application/json
    X-RigOS-Timestamp: <unix seconds>
    X-RigOS-Nonce: <32 lowercase hexadecimal characters>
    X-RigOS-Signature: sha256=<64 lowercase hexadecimal characters>

Canonical HMAC input:

    timestamp + "." + nonce + "." + exact request body bytes

Secret format:

    exactly 64 hexadecimal characters

Accepted observation schema:

    rigos.status-observation/v1

The endpoint requires the exact 19-component RIGOS registry. Unknown,
duplicated or missing components are rejected. Private mining and identity
fields are rejected before storage. Only a sanitized allowlisted projection is
stored.

Accepted response:

    HTTP 202

Bounded failures:

    HTTP 401   bad timestamp, nonce or HMAC
    HTTP 409   replayed nonce
    HTTP 413   body larger than 64 KiB
    HTTP 415   non-JSON media type
    HTTP 422   invalid schema or forbidden field
    HTTP 503   missing binding, secret or migration


FRESHNESS MODEL
---------------

The appliance timer emits every 30 seconds.

    LIVE       last accepted request <= 90 seconds ago
    STALE      last accepted request > 90 and <= 300 seconds ago
    OFFLINE    last accepted request > 300 seconds ago

Freshness is calculated by the server at read time. Stored system evidence is
not rewritten when a node becomes stale or offline.


CLOUDFLARE RESOURCES
--------------------

Cloudflare Pages project:

    production branch:      main
    framework preset:       None
    build command:          empty
    build output directory: site
    root directory:         repository root

Pages Functions live under:

    functions/

Create one D1 database and bind it to the Pages project:

    binding variable: RIGOS_STATUS_DB

Apply:

    migrations/0001_status.sql

Create one encrypted Pages secret:

    variable: RIGOS_STATUS_SECRET
    value:    64 hexadecimal characters

Generate a secret outside the repository:

    openssl rand -hex 32

Use the same secret in the appliance secret file. Never commit it, print it in
logs or place it in command arguments.

Configure bindings for both production and preview deployments when preview
verification is required. Redeploy after adding or changing a binding.


APPLIANCE CONFIGURATION
-----------------------

Place the 64-hex secret in a root-readable file, mode 0600, then run:

    sudo rig-status-agent configure \
        --server https://rigos.site \
        --secret-file /path/to/secret-file

The operator command writes persistent configuration, enables the timer and
attempts the first signed observation.

Inspect without exposing the secret:

    sudo rig-status-agent status
    sudo systemctl status rigos-status-agent.timer
    sudo journalctl -u rigos-status-agent.service --no-pager


TESTS
-----

No package installation is required.

    node --experimental-default-type=module \
        --test scripts/test-status-functions.mjs

The test suite covers:

    exact 19-component registry
    private-field rejection
    timestamp.nonce.body HMAC compatibility
    HTTP 202 acceptance path
    replay rejection
    wrong-secret rejection
    public source-ID truncation
    live / stale / offline transitions


RELEASE BOUNDARY
----------------

This status service does not make RIGOS production-ready. It publishes bounded
truth from an experimental appliance. It does not prove broad hardware
compatibility, unattended reliability, payout correctness or remote-control
safety because those are outside this feature.
