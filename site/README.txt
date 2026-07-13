RIGOS FIELD ARCHIVE
===================

PURPOSE
-------

Static history and architecture site for RIGOS.

No framework.
No package manager.
No build tool.
No JavaScript.
No analytics.
No external font.

The site is plain HTML, plain CSS and one local SVG asset.

LOCAL PREVIEW
-------------

From the repository root:

    python3 -m http.server 8080 --directory site

Then open:

    http://127.0.0.1:8080/

PUBLISHING
----------

.github/workflows/rigos-pages.yml uploads the site directory directly to
GitHub Pages. The repository Pages source must be set to GitHub Actions.

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
