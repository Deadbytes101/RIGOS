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
  return String(value)
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
        detail: "RIGOS is reporting healthy signed operating-system evidence.",
      };
    case "stale":
      return {
        title: "Status updates are stale",
        detail: "The last signed observation is older than the live window.",
      };
    case "degraded":
      return {
        title: "Some systems are degraded",
        detail: "RIGOS is reachable, but one or more checks need attention.",
      };
    case "partial_outage":
      return {
        title: "Partial system outage",
        detail: "At least one RIGOS system component is unavailable.",
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

function metricCard(label, value, tone, detail) {
  return `
    <article class="status-metric ${statusClass(tone)}">
      <span class="status-metric-label">${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
      <span class="status-metric-detail">${escapeHtml(detail)}</span>
    </article>`;
}

function componentRow(component) {
  return `
    <li class="component-row">
      <div class="component-copy">
        <strong>${escapeHtml(component.id)}</strong>
        <span>${escapeHtml(component.evidence?.summary || "No public evidence summary")}</span>
      </div>
      <span class="state-pill ${statusClass(component.status)}">${escapeHtml(statusLabel(component.status))}</span>
    </li>`;
}

function componentGroups(components) {
  const byId = new Map(components.map((component) => [component.id, component]));
  return COMPONENT_GROUPS.map((group) => {
    const rows = group.ids
      .map((id) => byId.get(id))
      .filter(Boolean)
      .map(componentRow)
      .join("");

    return `
      <section class="component-group">
        <h4>${escapeHtml(group.title)}</h4>
        <ul>${rows}</ul>
      </section>`;
  }).join("");
}

function nodeSection(node) {
  const release = node.release || {};
  const health = node.health || {};
  const components = Array.isArray(node.components) ? node.components : [];
  const operational = components.filter((component) => component.status === "operational").length;
  const build = release.buildCommit ? release.buildCommit.slice(0, 12) : "unavailable";

  return `
  <article class="status-node" aria-labelledby="node-${escapeHtml(node.nodeId)}">
    <header class="status-node-header">
      <div>
        <span class="eyebrow">Observed RIGOS appliance</span>
        <h3 id="node-${escapeHtml(node.nodeId)}">RIGOS NODE ${escapeHtml(node.nodeId)}</h3>
      </div>
      <div class="node-state-stack">
        <span class="state-pill ${statusClass(node.connection)}">${escapeHtml(statusLabel(node.connection))}</span>
        <span class="state-pill ${statusClass(node.systemState)}">${escapeHtml(statusLabel(node.systemState))}</span>
      </div>
    </header>

    <div class="node-facts" role="list" aria-label="Node facts">
      <div role="listitem"><span>Last received</span><strong>${escapeHtml(node.receivedAt || "unknown")}</strong><small>${escapeHtml(humanAge(node.ageSeconds))} ago</small></div>
      <div role="listitem"><span>Release</span><strong>${escapeHtml(release.version || "unknown")}</strong><small>${escapeHtml(release.channel || "unknown")} channel</small></div>
      <div role="listitem"><span>Build</span><strong>${escapeHtml(build)}</strong><small>${escapeHtml(release.architecture || "unknown")}</small></div>
      <div role="listitem"><span>System checks</span><strong>${operational}/${components.length}</strong><small>operational</small></div>
    </div>

    <div class="node-health-line">
      <div>
        <span class="eyebrow">Operator health</span>
        <strong>${escapeHtml(health.summary || "No health summary")}</strong>
      </div>
      <span class="state-pill ${statusClass(node.componentState)}">${escapeHtml(statusLabel(node.componentState))}</span>
    </div>

    <details class="component-details">
      <summary>
        <span>View ${components.length} signed system checks</span>
        <span class="details-hint">Public evidence</span>
      </summary>
      <div class="component-groups">${componentGroups(components)}</div>
    </details>
  </article>`;
}

function emptyState(notice) {
  return `
    <section class="status-empty">
      <span class="eyebrow">Public status board</span>
      <h2>${notice ? "Status service unavailable" : "Waiting for the first RIGOS observation"}</h2>
      <p>${escapeHtml(notice || "The public endpoint is ready, but no signed appliance observation has been accepted yet.")}</p>
    </section>`;
}

function renderStatusPage(publicStatus, statusCode = 200, notice = null) {
  const nodes = Array.isArray(publicStatus?.nodes) ? publicStatus.nodes : [];
  const counts = summaryCounts(nodes);
  const generatedAt = publicStatus?.generatedAt || new Date().toISOString();
  const overall = overallState(nodes, notice);
  const copy = overallCopy(overall);
  const nodeBody = nodes.length > 0 && !notice
    ? nodes.map(nodeSection).join("")
    : emptyState(notice);

  const html = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="30">
  <title>RIGOS Public System Status</title>
  <meta name="description" content="Open public system status from signed RIGOS operating-system observations. Read-only and privacy bounded.">
  <meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1">
  <meta name="theme-color" content="#0b0d10">
  <link rel="canonical" href="https://rigos.site/status">
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/style.css">
  <link rel="stylesheet" href="/status.css">
</head>
<body class="status-page">
<a class="skip-link" href="#content">Skip to status</a>
<header class="status-topbar">
  <div class="status-container status-nav">
    <a class="status-brand" href="/" aria-label="RIGOS home">RIGOS</a>
    <nav aria-label="Status navigation">
      <a href="/status" aria-current="page">Public status</a>
      <a href="/api/v1/status">Status JSON</a>
      <a href="/history.html">History</a>
      <a href="https://github.com/Deadbytes101/RIGOS">Source</a>
    </nav>
  </div>
</header>

<main id="content" class="status-container status-main">
  <section class="status-hero">
    <div>
      <span class="eyebrow">RIGOS public system status</span>
      <h1>Operating-system truth, published directly.</h1>
      <p>Anyone can view this board. It receives signed, read-only health evidence from RIGOS appliances and exposes no remote-control surface.</p>
    </div>
    <aside class="public-board-card" aria-label="Public access">
      <span class="public-dot" aria-hidden="true"></span>
      <div>
        <strong>Open public status</strong>
        <span>No account or authentication required</span>
      </div>
    </aside>
  </section>

  <section class="overall-banner ${statusClass(overall)}" aria-labelledby="overall-heading">
    <span class="overall-symbol" aria-hidden="true"></span>
    <div>
      <span class="eyebrow">Current RIGOS state</span>
      <h2 id="overall-heading">${escapeHtml(copy.title)}</h2>
      <p>${escapeHtml(copy.detail)}</p>
    </div>
    <span class="state-pill ${statusClass(overall)}">${escapeHtml(statusLabel(overall))}</span>
  </section>

  <section class="status-metrics" aria-label="Status summary">
    ${metricCard("Observed nodes", nodes.length, nodes.length ? "live" : "unknown", "signed sources")}
    ${metricCard("Live", counts.live, "live", "received within 90s")}
    ${metricCard("Stale", counts.stale, "stale", "older than 90s")}
    ${metricCard("Offline", counts.offline, "offline", "older than 300s")}
  </section>

  <section class="status-meta-strip" aria-label="Status board metadata">
    <div><span>Generated</span><strong>${escapeHtml(generatedAt)}</strong></div>
    <div><span>Automatic refresh</span><strong>30 seconds</strong></div>
    <div><span>Public schema</span><strong>rigos.public-status/v1</strong></div>
  </section>

  <section class="status-section" aria-labelledby="nodes-heading">
    <header class="section-heading">
      <div>
        <span class="eyebrow">System fleet</span>
        <h2 id="nodes-heading">Observed RIGOS systems</h2>
      </div>
      <span class="section-count">${nodes.length} ${nodes.length === 1 ? "node" : "nodes"}</span>
    </header>
    ${nodeBody}
  </section>

  <aside class="privacy-boundary">
    <div>
      <span class="eyebrow">Privacy boundary</span>
      <h2>Public by design, private by default.</h2>
    </div>
    <p>Wallet, pool, worker name, hashrate, shares, hostname, IP address, password, token, private key and remote command are not accepted or published.</p>
  </aside>

  <p class="status-release-note">RIGOS remains an experimental Alpha appliance. This board reports bounded machine evidence; it does not claim production readiness or broad hardware compatibility.</p>
</main>

<footer class="status-footer">
  <div class="status-container">
    <span>RIGOS public system status</span>
    <span>Signed observations · Read-only · Zero browser JavaScript</span>
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
