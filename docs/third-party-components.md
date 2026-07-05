# Third-Party Components

The immutable image bundles official XMRig 6.26.0 for Linux x86-64. RIGOS does
not patch, rebuild, hex-edit, redirect, or suppress its upstream donation
behavior. RIGOS receives none of that donation.

The build pins and checks both the upstream archive SHA-256 and the extracted
binary SHA-256, rejects unexpected archive paths, and records provenance at
`/usr/share/rigos/components/xmrig.json`. No miner is downloaded at boot.

RIGOS itself has no subscription, worker limit, mining fee, developer fee,
mandatory account, mandatory cloud, or forced pool.
