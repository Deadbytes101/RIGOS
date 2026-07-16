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
  major_outage: 5,
});

const ASSET_REVISION = "20260716-3";

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function statusLabel(value) {
  return String(value || "unknown")
    .replaceAll("_", " ")
    .replace(/\b\w/g, (letter) => letter.toUpperCase());
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
    const candidate = node.connection === "live"
      ? (node.systemState || "unknown")
      : (node.connection || "unknown");
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
        detail: "The latest signed RIGOS observations report normal operation.",
      };
    case "stale":
      return {
        title: "Status updates are stale",
        detail: "At least one signed observation is older than the live window.",
      };
    case "offline":
      return {
        title: "Status updates unavailable",
        detail: "At least one node has not sent a fresh signed observation within the offline window.",
      };
    case "degraded":
      return {
        title: "Some systems are degraded",
        detail: "A live RIGOS observation reports one or more checks that need attention.",
      };
    case "partial_outage":
      return {
        title: "Partial system outage",
        detail: "A live RIGOS observation reports at least one unavailable component.",
      };
    case "major_outage":
      return {
        title: "Major system outage",
        detail: "A live RIGOS observation reports a critical system failure.",
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

function statusIcon(state) {
  const klass = statusClass(state);
  const common = 'width="20" height="20" viewBox="0 0 24 24" preserveAspectRatio="xMidYMid meet" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" focusable="false" aria-hidden="true"';

  if (["operational", "live"].includes(state)) {
    return `<svg class="status-icon ${klass}" ${common}><circle cx="12" cy="12" r="9"></circle><path d="m8.5 12 2.2 2.2 4.8-5"></path></svg>`;
  }
  if (["offline", "partial_outage", "major_outage"].includes(state)) {
    return `<svg class="status-icon ${klass}" ${common}><circle cx="12" cy="12" r="9"></circle><path d="m9 9 6 6M15 9l-6 6"></path></svg>`;
  }
  return `<svg class="status-icon ${klass}" ${common}><circle cx="12" cy="12" r="9"></circle><path d="M12 7v6"></path><path d="M12 17h.01"></path></svg>`;
}

function stateExplanation(status) {
  switch (status) {
    case "operational":
    case "live":
      return "Operational — the latest signed observation passed this check. Current sample, not uptime history.";
    case "stale":
      return "Stale — the latest signed observation is older than the live window.";
    case "degraded":
      return "Degraded — the latest signed observation reports a check that needs attention.";
    case "offline":
      return "Offline — no fresh signed observation was received within the offline window.";
    case "partial_outage":
      return "Partial outage — the latest live observation reports an unavailable component.";
    case "major_outage":
      return "Major outage — the latest live observation reports a critical system failure.";
    default:
      return "Unknown — no authoritative state is available for this current sample.";
  }
}

function currentSnapshotStrip(status, connection = "live") {
  const tooltip = connection === "live"
    ? stateExplanation(status)
    : `Last accepted signed sample: ${statusLabel(status)}. Node connection is ${statusLabel(connection)}; this is not live uptime.`;
  const periodLabel = connection === "live" ? "Latest sample" : "Last accepted sample";
  return `
    <div class="snapshot-wrap" aria-label="Current status sample only; this is not historical uptime">
      <span
        class="snapshot-current ${statusClass(status)}"
        tabindex="0"
        role="img"
        aria-label="${escapeHtml(tooltip)}"
        data-tooltip="${escapeHtml(tooltip)}"
      ></span>
      <div class="snapshot-periods"><span>${escapeHtml(periodLabel)}</span><span>Now</span></div>
    </div>`;
}

function componentHealthStrip(components) {
  const segments = components.map((component) => {
    const label = `${componentDisplayName(component.id)}: ${statusLabel(component.status)}`;
    return `
      <span
        class="snapshot ${statusClass(component.status)}"
        tabindex="0"
        role="img"
        aria-label="${escapeHtml(label)}"
        data-tooltip="${escapeHtml(label)}"
      ></span>`;
  }).join("");

  const operational = components.filter((component) => component.status === "operational").length;
  return `
    <div class="snapshot-wrap node-snapshot-wrap" aria-label="Current state of ${components.length} signed system checks">
      <div class="snapshots node-snapshots">${segments}</div>
      <div class="snapshot-periods"><span>${components.length} signed checks</span><span>${operational}/${components.length} operational</span></div>
    </div>`;
}

function componentRow(component, connection) {
  return `
    <li class="status-service">
      <div class="service-row-container">
        <div class="service-name">
          ${statusIcon(component.status)}
          <div class="service-copy">
            <strong>${escapeHtml(componentDisplayName(component.id))}</strong>
            <span>${escapeHtml(component.evidence?.summary || "No public evidence summary")}</span>
          </div>
        </div>
        <span class="status-state ${statusClass(component.status)}">${escapeHtml(statusLabel(component.status))}</span>
      </div>
      ${currentSnapshotStrip(component.status, connection)}
    </li>`;
}

function componentGroup(group, components, connection) {
  const byId = new Map(components.map((component) => [component.id, component]));
  const rows = group.ids
    .map((id) => byId.get(id))
    .filter(Boolean)
    .map((component) => componentRow(component, connection))
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
  const displayState = node.connection === "live" ? node.systemState : node.connection;

  return `
  <section class="status-node" aria-labelledby="node-${escapeHtml(node.nodeId)}">
    <h2 id="node-${escapeHtml(node.nodeId)}">Observed RIGOS system</h2>

    <div class="status-block node-block">
      <div class="service-row-container node-heading-row">
        <div class="service-name">
          ${statusIcon(displayState)}
          <div class="service-copy">
            <strong>RIGOS node ${escapeHtml(node.nodeId)}</strong>
            <span>${escapeHtml(statusLabel(node.connection))} connection · last received ${escapeHtml(humanAge(node.ageSeconds))} ago</span>
          </div>
        </div>
        <span class="status-state ${statusClass(displayState)}">${escapeHtml(statusLabel(displayState))}</span>
      </div>

      ${componentHealthStrip(components)}

      <dl class="status node-status" aria-label="Observed RIGOS node details">
        <dt>Connection</dt><dd>${escapeHtml(statusLabel(node.connection))}</dd>
        <dt>Last observed system state</dt><dd>${escapeHtml(statusLabel(node.systemState))}</dd>
        <dt>Last received</dt><dd>${escapeHtml(node.receivedAt || "unknown")}</dd>
        <dt>Observed</dt><dd>${escapeHtml(node.observedAt || "unknown")}</dd>
        <dt>Release</dt><dd>${escapeHtml(release.version || "unknown")}</dd>
        <dt>Build</dt><dd>${escapeHtml(build)}</dd>
        <dt>Architecture</dt><dd>${escapeHtml(release.architecture || "unknown")}</dd>
        <dt>System checks</dt><dd>${operational}/${components.length} operational</dd>
        <dt>Rig health</dt><dd>${escapeHtml(health.summary || "No health summary")}</dd>
      </dl>
    </div>

    ${COMPONENT_GROUPS.map((group) => componentGroup(group, components, node.connection)).join("")}
  </section>`;
}

function emptySection(notice) {
  return `
    <section class="status-node">
      <h2>Observed RIGOS systems</h2>
      <div class="status-block empty-state">
        <strong>${notice ? "Status service unavailable" : "Waiting for the first observation"}</strong>
        <p>${escapeHtml(notice || "The public endpoint is ready, but no signed RIGOS appliance observation has been accepted yet.")}</p>
      </div>
    </section>`;
}

function renderStatusPage(publicStatus, statusCode = 200, notice = null) {
  const nodes = Array.isArray(publicStatus?.nodes) ? publicStatus.nodes : [];
  const counts = summaryCounts(nodes);
  const generatedAt = publicStatus?.generatedAt || new Date().toISOString();
  const totalNodeCount = Number(publicStatus?.totalNodeCount ?? nodes.length);
  const truncated = publicStatus?.truncated === true;
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
  <link rel="stylesheet" href="/style.css?v=${ASSET_REVISION}">
  <link rel="stylesheet" href="/status.css?v=${ASSET_REVISION}">
</head>
<body>
<a class="skip-link" href="#content">Skip to content</a>
<header class="site-header status-site-header">
  <div class="shell status-page-shell">
    <a class="brand" href="/">RIGOS</a>
    <p class="subtitle">Direct signed system status.</p>
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

<main id="content" class="shell status-page-shell status-shell">
  <div class="status-banner ${statusClass(overall)}">
    <div class="top-level-status">
      ${statusIcon(overall)}
      <div>
        <strong>${escapeHtml(copy.title)}</strong>
        <span>${escapeHtml(copy.detail)}</span>
      </div>
    </div>
  </div>

  <div class="status-overview" aria-label="Public status summary">
    <span><strong>${nodes.length}</strong> shown</span>
    <span><strong>${totalNodeCount}</strong> observed</span>
    <span><strong>${counts.live}</strong> live</span>
    <span><strong>${counts.stale}</strong> stale</span>
    <span><strong>${counts.offline}</strong> offline</span>
    <span class="status-generated">Generated ${escapeHtml(generatedAt)}</span>
  </div>
  ${truncated ? `<p class="note">Showing the newest ${nodes.length} of ${totalNodeCount} observed nodes.</p>` : ""}

  ${systems}

  <section class="status-boundary">
    <h2>Public status boundary</h2>
    <p>This page is public and read-only. It accepts signed machine observations only and exposes no remote-control surface.</p>
    <div class="callout"><strong>Not published:</strong> wallet, pool, worker name, hashrate, shares, hostname, IP address, password, token, private key or remote command.</div>
    <p class="note">The segmented bars show the latest accepted state only. They are not 90-day uptime history.</p>
  </section>
</main>

<footer class="site-footer">
  <div class="shell status-page-shell">
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
      "content-security-policy": "default-src 'none'; script-src 'none'; script-src-elem 'none'; style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
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
      { generatedAt: new Date().toISOString(), nodes: [], totalNodeCount: 0, truncated: false },
      503,
      "The status service is temporarily unavailable.",
    );
  }

  if (context.request.method === "HEAD") {
    return new Response(null, { status: response.status, headers: response.headers });
  }
  return response;
}

export {
  overallCopy,
  overallState,
  renderStatusPage,
};
