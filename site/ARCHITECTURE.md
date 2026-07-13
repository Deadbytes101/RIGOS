RIGOS SITE ARCHITECTURE
=======================

The website is a static projection of the repository record. It does not
replace the canonical engineering documents.


PUBLISHING PIPELINE
-------------------

```mermaid
flowchart LR
    DOCS["Canonical RIGOS documents\nproject history, architecture, contract\nsecurity, evidence and limits"]
    SITE["Handwritten static site\nHTML and CSS"]
    GIT["GitHub main branch"]
    PAGES["Cloudflare Pages\noutput directory: site"]
    READER["Browser\nno JavaScript required"]

    DOCS --> SITE
    SITE --> GIT
    GIT --> PAGES
    PAGES --> READER
    DOCS -. "remains source authority" .-> READER
```


RUNTIME BOUNDARY
----------------

```text
RIGOS appliance runtime   unchanged
RIGOS release source      unchanged
RIGOS canonical docs      unchanged
site                      read-only publication surface
Cloudflare Pages          publishes site after a main-branch push
```

The website contains no package manager, framework, static-site generator,
client JavaScript, analytics, remote font, CDN asset or build dependency.
