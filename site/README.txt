RIGOS SITE
==========

PURPOSE
-------

Engineering history, architecture and direct system-status site for RIGOS.

No framework.
No package manager.
No build tool.
No browser JavaScript.
No analytics.
No external font.
No remote asset.

The document pages are plain HTML, plain CSS and checked-in SVG. Dynamic status
routes are server-rendered Cloudflare Pages Functions. The browser receives
HTML or JSON and executes no application JavaScript.


STATUS ROUTES
-------------

    /status
        Server-rendered direct RIGOS system status.

    /api/v1/status
        Public sanitized JSON status.

    /api/v1/observations
        POST-only signed appliance ingest.

Backend source:

    functions/

Database schema:

    migrations/0001_status.sql

Deployment and security contract:

    docs/STATUS-SERVICE.md


DIAGRAMS
--------

Editable architecture diagrams live in:

    site/DIAGRAMS.md

GitHub renders the Mermaid blocks natively. The website uses matching static
SVG files under:

    site/diagrams/

The browser does not download or execute Mermaid. Diagram rendering is not a
client-side dependency.


LOCAL STATIC PREVIEW
--------------------

From the repository root:

    python3 -m http.server 8080 --directory site

Then open:

    http://127.0.0.1:8080/

This previews static documents only. Pages Functions require a Cloudflare local
runtime and D1 binding.


PUBLISHING
----------

Cloudflare Pages publishes the site directory and discovers the root functions
directory.

    production branch:      main
    framework preset:       None
    build command:          empty
    build output directory: site
    root directory:         repository root

Required production bindings:

    RIGOS_STATUS_DB         D1 database
    RIGOS_STATUS_SECRET     encrypted 64-hex secret

Every push to main creates a new production deployment. Binding changes require
a redeploy.


SOURCE AUTHORITY
----------------

Site prose is derived from the canonical repository documents:

    docs/PROJECT-HISTORY.md
    docs/architecture.md
    docs/product-contract.md
    docs/SECURITY-MODEL.md
    docs/PHYSICAL-EVIDENCE-ALPHA25.md
    docs/KNOWN-LIMITS.md
    docs/STATUS-SERVICE.md

When a site summary conflicts with those files, the repository documents and
frozen release source are authoritative.
