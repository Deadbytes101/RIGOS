RIGOS SITE ARCHITECTURE
=======================

The website is a static projection of the repository record. It does not
replace the canonical engineering documents.


PUBLISHING PIPELINE
-------------------

```mermaid
flowchart LR
    DOCS["Canonical RIGOS documents\nproject history / architecture / contract\nsecurity / evidence / limits"]
    HTML["Handwritten static site\nHTML + CSS + local SVG"]
    CHECK["Pull-request validator\nparse HTML / resolve local links\nreject script tags"]
    ARTIFACT["GitHub Pages artifact\nsite/ directory only"]
    DEPLOY["GitHub Pages deployment"]
    READER["Browser\nno JavaScript required"]

    DOCS --> HTML
    HTML --> CHECK
    CHECK --> ARTIFACT
    ARTIFACT --> DEPLOY
    DEPLOY --> READER
    DOCS -. "remains source authority" .-> READER
```


RUNTIME BOUNDARY
----------------

```text
RIGOS appliance runtime   unchanged
RIGOS release source      unchanged
RIGOS canonical docs      unchanged
site/                     added read-only publication surface
Pages workflow            validates and publishes site/ only
```

The website contains no package manager, framework, static-site generator,
client JavaScript, analytics, remote font, CDN asset or build dependency.
