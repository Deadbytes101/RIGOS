RIGOS SITE
==========

PURPOSE
-------

Static engineering history and architecture site for RIGOS.

No framework.
No package manager.
No build tool.
No browser JavaScript.
No analytics.
No external font.
No remote asset.

The site is plain HTML, plain CSS and checked-in SVG.


DIAGRAMS
--------

Editable architecture diagrams live in:

    site/DIAGRAMS.md

GitHub renders the Mermaid blocks natively. The website uses matching static
SVG files under:

    site/diagrams/

The browser does not download or execute Mermaid. Diagram rendering is not a
client-side dependency.


LOCAL PREVIEW
-------------

From the repository root:

    python3 -m http.server 8080 --directory site

Then open:

    http://127.0.0.1:8080/


PUBLISHING
----------

Cloudflare Pages publishes the site directory directly.

    production branch:      main
    framework preset:       None
    build command:          empty
    build output directory: site
    root directory:         repository root

Every push to main creates a new production deployment.


SOURCE AUTHORITY
----------------

Site prose is derived from the canonical repository documents:

    docs/PROJECT-HISTORY.md
    docs/architecture.md
    docs/product-contract.md
    docs/SECURITY-MODEL.md
    docs/PHYSICAL-EVIDENCE-ALPHA25.md
    docs/KNOWN-LIMITS.md

When a site summary conflicts with those files, the repository documents
and frozen release source are authoritative.
