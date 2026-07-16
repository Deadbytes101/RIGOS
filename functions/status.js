import {
  methodNotAllowed,
  readPublicStatus,
} from "./_lib/status.js";

const COMPONENT_GROUPS = Object.freeze([
  {
    title: "Boot and persistent state",
    ids: [
      "boot-device-verification",
      "persistent-state",
      "state-readiness",
      "recovery-access",
      "ssh-host-identity",
    ],
  },
  {
    title: "Configuration and runtime",
    ids: [
      "configuration-activation",
      "profile-apply",
      "runtime-render",
      "huge-page-authority",
      "randomx-msr",
    ],
  },
  {
    title: "Network and filesystem",
    ids: [
      "network-readiness",
      "root-filesystem-integrity",
      "state-capacity",
      "time-synchronization",
    ],
  },
  {
    title: "Integrity and remote access",
    ids: [
      "failed-unit-set",
      "operator-health",
      "kernel-integrity",
      "ssh-service",
      "remote-access-observer",
    ],
  },
]);

const STATUS_WEIGHT = Object.freeze({
  operational: 0,
  live: 0,
  unknown: 1,
  stale: 2,
  degraded: 2,
  partial_outage: 3,
  offline: 4,
  major_outage: 4,
});

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function statusLabel(value) {
  return String(value || "unknown").replaceAll("_", " ").toUpperCase();
}

function statusClass(value) {
  return `state-${String(value || "unknown").replaceAll("_", "-")}`;
}

function humanAge(seconds) {
  const value = Math.max(0, Number(seconds) || 0);
  if (value < 60) return `${value}s`;
  if (value < 3600) return `${Math.floor(value / 60)}m ${value % 60}s`;
  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  return `${hours}h ${minutes}m`;
}

function summaryCounts(nodes) {
  const counts = { live: 0, stale: 0, offline: 0 };
  for (const node of nodes) {
    if (Object.hasOwn(counts, node.connection)) counts[node.connection] += 1;
  }
  return counts;
}

function overallState(nodes, notice) {
  if (notice || nodes.length === 0) return "unknown";

  return nodes.reduce((worst, node) => {
    const candidate = node.systemState || node.connection || "unknown";
    return (STATUS_WEIGHT[candidate] ?? 1) > (STATUS_WEIGHT[worst] ?? 1)
      ? candidate
      : worst;
  }, "operational");
}

function overallCopy(state) {
  switch (state) {
    case "operational":
    case "live":
      return {
        title: "All systems operational",
        detail: "The latest signed RIGOS observation reports normal operation.",
      };
    case "stale":
      return {
        title: "Status updates are stale",
        detail: "The most recent signed observation is older than the live window.",
      };
    case "degraded":
      return {
        title: "Some systems are degraded",
        detail: "RIGOS is reachable, but one or more checks need attention.",
      };
    case "partial_outage":
      return {
        title: "Partial system outage",
        detail: "At least one reported RIGOS component is unavailable.",
      };
    case "offline":
    case "major_outage":
      return {
        title: "Major system outage",
        detail: "A RIGOS node is offline or has reported a critical failure.",
      };
    default:
      return {
        title: "System status unavailable",
        detail: "No current signed RIGOS observation is available.",
      };
  }
}

function componentDisplayName(id) {
  return String(id)
    .split("-")
    .map((part) => part.length > 0 ? part[0].toUpperCase() + part.slice(1) : part)
    .join(" ");
}

function componentRow(component) {
  return `
    <li class="status-service">
      <div class="status-service-copy">
        <strong>${escapeHtml(componentDisplayName(component.id))}</strong>
        <span>${escapeHtml(component.evidence?.summary || "No public evidence summary")}</span>
      </div>
      <span class="status-state ${statusClass(component.status)}">${escapeHtml(statusLabel(component.status))}</span>
    </li>`;
}

function componentGroup(group, components) {
  const byId = new Map(components.map((component) => [component.id, component]));
  const rows = group.ids
    .map((id) => byId.get(id))
    .filter(Boolean)
    .map(componentRow)
    .join("");

  if (!rows) return "";

  return `
    <section class="status-group">
      <h3>${escapeHtml(group.title)}</h3>
      <ul class="status-service-list">${rows}</ul>
    </section>`;
}

function nodeSection(node) {
  const release = node.release || {};
  const health = node.health || {};
  const components = Array.isArray(node.components) ? node.components : [];
  const operational = components.filter((component) => component.status === "operational").length;
  const build = release.buildCommit ? release.buildCommit.slice(0, 12) : "unavailable";

  return `
  <section class="status-node" aria-labelledby="node-${escapeHtml(node.nodeId)}">
    <div class="status-node-heading">
      <h2 id="node-${escapeHtml(node.nodeId)}">RIGOS node ${escapeHtml(node.nodeId)}</h2>
      <span class="status-state ${statusClass(node.systemState)}">${escapeHtml(statusLabel(node.systemState))}</span>
    </div>

    <p class="note">${escapeHtml(statusLabel(node.connection))} connection. Last signed observation received ${escapeHtml(humanAge(node.ageSeconds))} ago.</p>

    <dl class="status node-status" aria-label="Observed RIGOS node details">
      <dt>Last received</dt><dd>${escapeHtml(node.receivedAt || "unknown")}</dd>
      <dt>Observed</dt><dd>${escapeHtml(node.observedAt || "unknown")}</dd>
      <dt>Release</dt><dd>${escapeHtml(release.version || "unknown")}</dd>
      <dt>Build</dt><dd>${escapeHtml(build)}</dd>
      <dt>Architecture</dt><dd>${escapeHtml(release.architecture || "unknown")}</dd>
      <dt>System checks</dt><dd>${operational}/${components.length} operational</dd>
      <dt>Rig health</dt><dd>${escapeHtml(health.summary || "No health summary")}</dd>
    </dl>

    ${COMPONENT_GROUPS.map((group) => componentGroup(group, components)).join("")}
  </section>`;
}

function emptySection(notice) {
  return `
    <section>
      <h2>Observed RIGOS systems</h2>
      <div class="callout">
        <strong>${notice ? "Status service unavailable" : "Waiting for the first observation"}</strong>
        <p>${escapeHtml(notice || "The public endpoint is ready, but no signed RIGOS appliance observation has been accepted yet.")}</p>
      </div>
    </section>`;
}

function renderStatusPage(publicStatus, statusCode = 200, notice = null) {
  const nodes = Array.isArray(publicStatus?.nodes) ? publicStatus.nodes : [];
  const counts = summaryCounts(nodes);
  const generatedAt = publicStatus?.generatedAt || new Date().toISOString();
  const overall = overallState(nodes, notice);
  const copy = overallCopy(overall);
  const systems = nodes.length > 0 && !notice
    ? nodes.map(nodeSection).join("")
    : emptySection(notice);

  const html = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="30">
  <title>RIGOS System Status</title>
  <meta name="description" content="Public, read-only system status from signed RIGOS operating-system observations.">
  <meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1">
  <meta name="theme-color" content="#111214">
  <link rel="canonical" href="https://rigos.site/status">
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/style.css">
  <link rel="stylesheet" href="/status.css">
</head>
<body>
<a class="skip-link" href="#content">Skip to content</a>
<header class="site-header">
  <div class="shell">
    <a class="brand" href="/">RIGOS</a>
    <p class="subtitle">Engineering record and direct signed system status.</p>
    <nav class="nav" aria-label="Site navigation">
      <a href="/">Overview</a>
      <a href="/status" aria-current="page">System status</a>
      <a href="/history.html">History</a>
      <a href="/architecture.html">Architecture</a>
      <a href="/evidence.html">Evidence</a>
      <a href="/limits.html">Limits</a>
      <a href="https://github.com/Deadbytes101/RIGOS">Source code</a>
    </nav>
  </div>
</header>

<main id="content" class="shell status-shell">
  <h1>RIGOS system status</h1>
  <p class="lead">Public, read-only operating-system evidence received directly from RIGOS appliances. Anyone may view this page; it provides no remote-control surface.</p>

  <div class="callout status-banner ${statusClass(overall)}">
    <strong>${escapeHtml(copy.title)}</strong>
    <p>${escapeHtml(copy.detail)}</p>
  </div>

  <dl class="status status-summary" aria-label="Public status summary">
    <dt>Observed nodes</dt><dd>${nodes.length}</dd>
    <dt>Live</dt><dd>${counts.live}</dd>
    <dt>Stale</dt><dd>${counts.stale}</dd>
    <dt>Offline</dt><dd>${counts.offline}</dd>
    <dt>Generated</dt><dd>${escapeHtml(generatedAt)}</dd>
    <dt>Refresh</dt><dd>30 seconds</dd>
  </dl>

  ${systems}

  <section>
    <h2>Public status boundary</h2>
    <p>This board is open to everyone and requires no account. It reports signed machine observations only.</p>
    <div class="callout"><strong>Not published:</strong> wallet, pool, worker name, hashrate, shares, hostname, IP address, password, token, private key or remote command.</div>
    <p class="note">Current observations only. Historical uptime percentages and 90-day graphs are omitted until RIGOS stores real history.</p>
  </section>
</main>

<footer class="site-footer">
  <div class="shell">
    <p>RIGOS direct public system status. Server-rendered and read-only.</p>
    <p><a href="/api/v1/status">Public JSON status</a> · <a href="https://github.com/Deadbytes101/RIGOS">Source repository</a></p>
  </div>
</footer>
</body>
</html>`;

  return new Response(html, {
    status: statusCode,
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "no-store",
      "content-security-policy": "default-src 'none'; style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
      "referrer-policy": "no-referrer",
      "x-content-type-options": "nosniff",
      "x-frame-options": "DENY",
    },
  });
}

export async function onRequest(context) {
  if (context.request.method !== "GET" && context.request.method !== "HEAD") {
    return methodNotAllowed(["GET", "HEAD"]);
  }

  let response;
  try {
    response = renderStatusPage(await readPublicStatus(context.env));
  } catch (error) {
    console.error("rigos-status-page:", error);
    response = renderStatusPage(
      { generatedAt: new Date().toISOString(), nodes: [] },
      503,
      error?.message || "The status database is unavailable.",
    );
  }

  if (context.request.method === "HEAD") {
    return new Response(null, { status: response.status, headers: response.headers });
  }

  return response;
}
