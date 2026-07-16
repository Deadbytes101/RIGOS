RIGOS DIRECT SYSTEM STATUS
==========================

PURPOSE
-------

Publish direct, signed operating-system evidence from a RIGOS appliance to
https://rigos.site without turning RIGOS into a hosted mining account or a
HiveOS-style remote owner.

The service is public, read-only and observation-only.

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

    HEAD /api/v1/status
        Same status and headers as GET, with no response body.

    GET /status
        Server-rendered public status page with zero browser JavaScript.


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

Accepted observation schema:

    rigos.status-observation/v1

The endpoint requires the exact 19-component RIGOS registry. Unknown,
duplicated or missing components are rejected.

The server applies exact allowlists to:

    observation fields
    release fields
    health fields
    component fields
    evidence fields
    facts allowed for each component ID

Sender-controlled health and evidence summaries are not published verbatim.
The server generates bounded public summaries from validated status, unit,
outcome and allowlisted fact values.

Unknown facts and sensitive key variants such as workerName, walletAddress,
poolAddress, authToken, miningIdentity and ipAddress are rejected before
storage.

Accepted response:

    HTTP 202

Bounded failures:

    HTTP 400   invalid length, UTF-8 or JSON
    HTTP 401   bad timestamp, nonce, HMAC or unregistered source ID
    HTTP 409   replayed nonce
    HTTP 413   body larger than 64 KiB
    HTTP 415   media type is not application/json
    HTTP 422   invalid schema, unsupported field or private field
    HTTP 503   missing binding, source registry or migration


SOURCE KEY REGISTRY
-------------------

A secret is bound to one source ID. A key accepted for one appliance cannot
sign an observation for another source ID.

Single-source deployment:

    RIGOS_STATUS_SOURCE_ID
        Exact persistent source ID emitted by that appliance.
        Format: 64 lowercase hexadecimal characters.

    RIGOS_STATUS_SECRET
        Secret shared only by the matching appliance and Pages deployment.
        Format: 64 hexadecimal characters.

Both variables must be configured together.

Multi-source deployment:

    RIGOS_STATUS_SOURCE_KEYS

The value is one encrypted JSON object mapping source IDs to their own keys:

    {
      "<64-hex-source-id-a>": "<64-hex-secret-a>",
      "<64-hex-source-id-b>": "<64-hex-secret-b>"
    }

The registry accepts from 1 through 64 sources. Do not reuse one secret across
multiple source IDs. Keep this value encrypted in Cloudflare Pages settings.
Do not commit it, print it or place it in shell history.

When RIGOS_STATUS_SOURCE_KEYS is present it is authoritative. The single-source
pair is ignored.


FRESHNESS AND SYSTEM STATE
--------------------------

The appliance timer normally emits every 30 seconds.

    LIVE       last accepted request <= 90 seconds ago
    STALE      last accepted request > 90 and <= 300 seconds ago
    OFFLINE    last accepted request > 300 seconds ago

Connection freshness and last observed system health are separate fields.

Example:

    connection:   offline
    systemState:  operational

This means the most recent accepted observation reported operational health,
but the server has not received a fresh observation. It does not prove a major
system outage.

A major outage is shown only when a live signed observation reports
major_outage.

Freshness is calculated by the server at read time. Stored evidence is not
rewritten when a node becomes stale or offline.


PUBLIC PROJECTION LIMIT
-----------------------

The public endpoint returns the newest 32 nodes per request.

Fields:

    nodeCount
        Number of nodes included in this response.

    totalNodeCount
        Total number of stored source IDs.

    truncated
        true when totalNodeCount is greater than nodeCount.

The status page prints a visible truncation notice rather than silently
claiming that 32 nodes are the whole fleet.


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

For one appliance configure encrypted variables:

    RIGOS_STATUS_SOURCE_ID
    RIGOS_STATUS_SECRET

For multiple appliances configure one encrypted variable:

    RIGOS_STATUS_SOURCE_KEYS

Configure bindings and encrypted variables separately for production and
preview when both environments are tested. Redeploy after adding or changing
a binding or variable.


APPLIANCE CONFIGURATION
-----------------------

Place the appliance-specific 64-hex secret in a root-readable file, mode 0600,
then run:

    sudo rig-status-agent configure \
        --server https://rigos.site \
        --secret-file /path/to/secret-file

The source ID is generated and persisted by the appliance. Read it locally
without exposing the secret:

    sudo cat /var/lib/rigos/status-agent/source-id

Set that exact value as RIGOS_STATUS_SOURCE_ID for a single-source deployment,
or use it as the key in RIGOS_STATUS_SOURCE_KEYS for a multi-source deployment.

The operator command writes persistent configuration, enables the timer and
attempts the first signed observation unless --no-start is supplied.

Inspect without exposing the secret:

    sudo rig-status-agent status
    sudo systemctl status rigos-status-agent.timer
    sudo journalctl -u rigos-status-agent.service --no-pager


STATUS PAGE
-----------

The /status page is public and requires no account.

It displays:

    connection freshness
    last observed system state
    release and build identity
    latest signed 19-component evidence
    generated time
    shown and total node counts

The segmented bars represent only the latest accepted sample. They are not
90-day uptime history. Per-component strips use one CSS-rendered element rather
than generating 48 repeated DOM elements. The 19-node summary strip keeps one
focusable segment per signed check.

Hovering or keyboard-focusing a segment displays its meaning. The same text is
present in aria-label for non-pointer users.


TESTS
-----

No package installation is required.

    node --experimental-default-type=module \
        --test \
        scripts/test-status-functions.mjs \
        scripts/test-site.mjs

The test suites cover:

    exact 19-component registry
    per-component fact allowlists
    private-field key variants
    replacement of sender-controlled summaries
    strict application/json media type
    timestamp.nonce.body HMAC compatibility
    single-source key binding
    multi-source key binding
    HTTP 202 acceptance
    replay rejection
    wrong-secret rejection
    public source-ID truncation
    live / stale / offline transitions
    separation of connection and system health
    visible public-node truncation
    HEAD response semantics
    hidden internal status-page errors
    static internal link resolution
    consistent site navigation
    sitemap route resolution
    square-corner and scoped-state CSS checks
    bounded snapshot DOM
    keyboard-readable segment explanations

GitHub Actions runs the same syntax and test gates for website-related pull
requests and pushes to main.


RELEASE BOUNDARY
----------------

This status service does not make RIGOS production-ready. It publishes bounded
truth from an experimental appliance. It does not prove broad hardware
compatibility, unattended reliability, payout correctness or remote-control
safety because those are outside this feature.
