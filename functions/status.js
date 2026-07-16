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
  return `status-${String(value || "unknown").replaceAll("_", "-")}`;
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
    if (Object.hasOwn(counts, node.connection)) {
      counts[node.connection] += 1;
    }
  }
  return counts;
}

function overallState(nodes, notice) {
  if (notice || nodes.length === 0) return "unknown";

  return nodes.reduce((worst, node) => {
    const candidate = node.systemState || node.connection || "unknown";
    const candidateWeight = STATUS_WEIGHT[candidate] ?? 1;
    const currentWeight = STATUS_WEIGHT[worst] ?? 1;
    return candidateWeight > currentWeight ? candidate : worst;
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

function componentDisplayName(id) {
  return String(id)
    .split("-")
    .map((part) => part.length > 0 ? part[0].toUpperCase() + part.slice(1) : part)
    .join(" ");
}

function statusIcon(state) {
  const label = statusLabel(state);
  return `<span class="service-status-icon ${statusClass(state)}" aria-label="${escapeHtml(label)}"></span>`;
}

function serviceRow(name, status, detail, meta = "") {
  return `
    <div class="service-row">
      <div class="service-row-container">
        <span class="service-name">
          ${statusIcon(status)}
          <span class="service-name-copy">
            <strong>${escapeHtml(name)}</strong>
            <small>${escapeHtml(detail)}</small>
          </span>
        </span>
        <span class="service-status-label ${statusClass(status)}">${escapeHtml(statusLabel(status))}</span>
      </div>
      ${meta ? `<div class="service-meta">${meta}</div>` : ""}
    </div>`;
}

function componentBlock(group, components) {
  const byId = new Map(components.map((component) => [component.id, component]));
  const rows = group.ids
    .map((id) => byId.get(id))
    .filter(Boolean)
    .map((component) => serviceRow(
      componentDisplayName(component.id),
      component.status,
      component.evidence?.summary || "No public evidence summary",
    ))
    .join("");

  if (!rows) return "";

  return `
    <div class="block-container">
      <div class="block-name">${escapeHtml(group.title)}</div>
      <div class="section block">${rows}</div>
    </div>`;
}

function nodeBlock(node) {
  const release = node.release || {};
  const health = node.health || {};
  const components = Array.isArray(node.components) ? node.components : [];
  const operational = components.filter((component) => component.status === "operational").length;
  const build = release.buildCommit ? release.buildCommit.slice(0, 12) : "unavailable";
  const meta = `
    <dl class="node-meta">
      <div><dt>Last received</dt><dd>${escapeHtml(node.receivedAt || "unknown")} (${escapeHtml(humanAge(node.ageSeconds))} ago)</dd></div>
      <div><dt>Release</dt><dd>${escapeHtml(release.version || "unknown")}</dd></div>
      <div><dt>Build</dt><dd>${escapeHtml(build)}</dd></div>
      <div><dt>Architecture</dt><dd>${escapeHtml(release.architecture || "unknown")}</dd></div>
      <div><dt>System checks</dt><dd>${operational}/${components.length} operational</dd></div>
      <div><dt>Health</dt><dd>${escapeHtml(health.summary || "No health summary")}</dd></div>
    </dl>`;

  return `
    <div class="block-container">
      <div class="block-name">Observed RIGOS systems</div>
      <div class="section block">
        ${serviceRow(
          `RIGOS NODE ${node.nodeId}`,
          node.systemState,
          `${statusLabel(node.connection)} connection · signed operating-system evidence`,
          meta,
        )}
      </div>
    </div>
    ${COMPONENT_GROUPS.map((group) => componentBlock(group, components)).join("")}`;
}

function emptyBlock(notice) {
  return `
    <div class="block-container">
      <div class="block-name">Observed RIGOS systems</div>
      <div class="section block">
        <div class="empty-state">
          <strong>${notice ? "Status service unavailable" : "Waiting for the first observation"}</strong>
          <p>${escapeHtml(notice || "The public endpoint is ready, but no signed RIGOS appliance observation has been accepted yet.")}</p>
        </div>
      </div>
    </div>`;
}

function renderStatusPage(publicStatus, statusCode = 200, notice = null) {
  const nodes = Array.isArray(publicStatus?.nodes) ? publicStatus.nodes : [];
  const counts = summaryCounts(nodes);
  const generatedAt = publicStatus?.generatedAt || new Date().toISOString();
  const overall = overallState(nodes, notice);
  const copy = overallCopy(overall);
  const systems = nodes.length > 0 && !notice
    ? nodes.map(nodeBlock).join("")
    : emptyBlock(notice);

  const html = `<!doctype html>
<html lang="en" data-theme="dark">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="refresh" content="30">
  <title>RIGOS Status</title>
  <meta name="description" content="Public, read-only system status from signed RIGOS operating-system observations.">
  <meta name="robots" content="index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1">
  <meta name="theme-color" content="#0f1115">
  <link rel="canonical" href="https://rigos.site/status">
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="stylesheet" href="/status.css">
</head>
<body>
  <a class="skip-link" href="#content">Skip to status</a>

  <div class="wrapper">
    <header class="section header">
      <a class="header-logo" href="/" aria-label="RIGOS home">
        <span class="header-logo-mark" aria-hidden="true"></span>
        <span>RIGOS STATUS</span>
      </a>
      <div class="header-links">
        <a href="/api/v1/status">Status JSON</a>
        <a href="/">Go to RIGOS <span aria-hidden="true">↗</span></a>
      </div>
    </header>

    <main id="content">
      <section class="section current-status status-banner ${statusClass(overall)}">
        <div class="top-level-status">
          ${statusIcon(overall)}
          <div>
            <div class="status-title">${escapeHtml(copy.title)}</div>
            <div class="status-description">${escapeHtml(copy.detail)}</div>
          </div>
        </div>
      </section>

      <div class="status-overview">
        <span><strong>${nodes.length}</strong> observed</span>
        <span><strong>${counts.live}</strong> live</span>
        <span><strong>${counts.stale}</strong> stale</span>
        <span><strong>${counts.offline}</strong> offline</span>
        <span class="status-generated">Generated ${escapeHtml(generatedAt)}</span>
      </div>

      <section class="section services" aria-label="RIGOS system services">
        ${systems}
      </section>

      <section class="section public-note">
        <strong>Public and read-only.</strong>
        <p>Anyone can view this status page. It accepts signed operating-system evidence only. Wallet, pool, worker name, hashrate, shares, hostname, IP address, password, token, private key and remote commands are not accepted or published.</p>
      </section>

      <p class="history-note">Current observations only. Historical uptime percentages and 90-day graphs are not displayed until RIGOS stores real history.</p>
    </main>

    <footer class="section footer">
      <span>RIGOS public system status</span>
      <span>Automatic refresh: 30 seconds</span>
    </footer>
  </div>
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
