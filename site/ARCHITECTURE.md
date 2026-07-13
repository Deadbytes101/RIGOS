RIGOS SITE ARCHITECTURE
=======================

The website is a static projection of the repository record. It does not
replace the canonical engineering documents.


PUBLISHING PIPELINE
-------------------

```mermaid
flowchart LR
    DOCS["Canonical RIGOS documents"]
    SITE["Handwritten HTML and CSS"]
    GIT["GitHub main branch"]
    PAGES["Cloudflare Pages"]
    READER["Browser without JavaScript"]

    DOCS --> SITE
    SITE --> GIT
    GIT --> PAGES
    PAGES --> READER
    DOCS -. "remains authority" .-> READER
```


DIAGRAM PUBLICATION
-------------------

```mermaid
flowchart LR
    SOURCE["Mermaid source in site/DIAGRAMS.md"]
    SVG["Checked-in static SVG"]
    HTML["Semantic HTML figure"]
    BROWSER["Browser image rendering"]

    SOURCE --> SVG
    SVG --> HTML
    HTML --> BROWSER
```

Mermaid is an authoring format, not a browser runtime. The published pages do
not load Mermaid, JavaScript, npm packages or a CDN. GitHub renders the source
blocks for repository readers; Cloudflare Pages serves static SVG files.


RUNTIME BOUNDARY
----------------

```text
RIGOS appliance runtime   unchanged
RIGOS release source      unchanged
RIGOS canonical docs      unchanged
site                      read-only publication surface
Cloudflare Pages          publishes site after a main-branch push
browser JavaScript        none
```

The website contains no package manager, framework, static-site generator,
client JavaScript, analytics, remote font, CDN asset or runtime diagram
dependency.
