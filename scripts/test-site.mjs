import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { onRequest as statusPageRequest, renderStatusPage } from "../functions/status.js";
import { COMPONENT_IDS } from "../functions/_lib/status.js";
import { onRequest as publicStatusRequest } from "../functions/api/v1/status.js";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SITE = path.join(ROOT, "site");
const PRIMARY_PAGES = [
  "index.html",
  "history.html",
  "architecture.html",
  "evidence.html",
  "limits.html",
  "404.html",
];
const DYNAMIC_PATHS = new Set(["/status", "/api/v1/status"]);

function readSite(relative) {
  return readFileSync(path.join(SITE, relative), "utf8");
}

function resolveSiteReference(fromPage, reference) {
  if (
    !reference ||
    reference.startsWith("#") ||
    reference.startsWith("mailto:") ||
    reference.startsWith("tel:") ||
    reference.startsWith("data:") ||
    /^[a-z]+:\/\//i.test(reference)
  ) {
    return null;
  }

  const clean = reference.split(/[?#]/, 1)[0];
  if (DYNAMIC_PATHS.has(clean)) return null;
  if (clean === "/") return path.join(SITE, "index.html");
  if (clean.startsWith("/")) return path.join(SITE, clean.slice(1));
  return path.resolve(path.dirname(path.join(SITE, fromPage)), clean);
}

function references(html) {
  return [...html.matchAll(/\b(?:href|src)="([^"]+)"/g)].map((match) => match[1]);
}

function sampleNode(connection = "offline") {
  const components = COMPONENT_IDS.map((id, index) => ({
    id,
    status: "operational",
    observedAt: "2026-07-16T09:56:27Z",
    evidence: { summary: `component ${index} operational` },
  }));
  return {
    nodeId: "14f1aa76b01a",
    connection,
    ageSeconds: connection === "offline" ? 600 : 1,
    systemState: "operational",
    componentState: "operational",
    observedAt: "2026-07-16T09:56:27Z",
    receivedAt: "2026-07-16T09:56:28Z",
    release: {
      version: "0.0.4-alpha.26",
      buildCommit: "3e3440434172ebb68b96cbec8bdd9ef3b649d5af",
      architecture: "x86_64",
    },
    health: { summary: "rig health completed successfully" },
    components,
  };
}

test("all primary pages expose consistent navigation and metadata", () => {
  for (const page of PRIMARY_PAGES) {
    const html = readSite(page);
    assert.match(html, /<meta name="viewport"/i, page);
    assert.match(html, /<title>[^<]+<\/title>/i, page);
    assert.match(html, /<link rel="icon" href="\/favicon\.svg"/i, page);
    assert.match(html, /href="\/?status"/i, `${page} must link to system status`);
    if (page !== "404.html") {
      assert.match(html, /<link rel="canonical" href="https:\/\/rigos\.site\//i, page);
    }
  }
});

test("static internal links and assets resolve", () => {
  for (const page of PRIMARY_PAGES) {
    const html = readSite(page);
    for (const reference of references(html)) {
      const resolved = resolveSiteReference(page, reference);
      if (resolved === null) continue;
      assert.equal(existsSync(resolved), true, `${page}: missing ${reference}`);
    }
  }
});

test("sitemap entries resolve to static or dynamic routes", () => {
  const xml = readSite("sitemap.xml");
  const locations = [...xml.matchAll(/<loc>https:\/\/rigos\.site([^<]*)<\/loc>/g)]
    .map((match) => match[1] || "/");
  assert.ok(locations.includes("/status"));
  for (const location of locations) {
    if (DYNAMIC_PATHS.has(location)) continue;
    const resolved = location === "/"
      ? path.join(SITE, "index.html")
      : path.join(SITE, location.slice(1));
    assert.equal(existsSync(resolved), true, `sitemap: missing ${location}`);
  }
});

test("offline connection is not mislabeled as a major system outage", async () => {
  const response = renderStatusPage({
    generatedAt: "2026-07-16T11:23:01Z",
    nodeCount: 1,
    totalNodeCount: 1,
    truncated: false,
    nodes: [sampleNode("offline")],
  });
  const html = await response.text();
  assert.match(html, /Status updates unavailable/);
  assert.doesNotMatch(html, /Major system outage/);
  assert.match(html, /Last observed system state<\/dt><dd>Operational/);
  assert.doesNotMatch(html, /node-block state-offline/);
});

test("status strips use bounded DOM and keyboard-readable tooltips", async () => {
  const response = renderStatusPage({
    generatedAt: "2026-07-16T11:23:01Z",
    nodeCount: 1,
    totalNodeCount: 1,
    truncated: false,
    nodes: [sampleNode("live")],
  });
  const html = await response.text();
  const nodeSegments = (html.match(/class="snapshot state-/g) || []).length;
  const currentStrips = (html.match(/class="snapshot-current state-/g) || []).length;
  assert.equal(nodeSegments, 19);
  assert.equal(currentStrips, 19);
  assert.equal((html.match(/data-tooltip=/g) || []).length, 38);
  assert.equal((html.match(/tabindex="0"/g) || []).length, 38);
  assert.doesNotMatch(html, /\stitle="[^"]*(Operational|Offline|Stale)/);
});

test("status CSS keeps state colors scoped and uses square corners", () => {
  const css = readSite("status.css");
  assert.doesNotMatch(css, /border-radius\s*:/);
  assert.doesNotMatch(css, /(^|\n)\.state-(?:offline|operational|live|stale|degraded|unknown|partial-outage|major-outage)\s*[,\{]/);
  assert.match(css, /\.status-icon\.state-offline/);
  assert.match(css, /\.snapshot-current/);
  assert.match(css, /:focus-visible/);
});

test("status SVG dimensions are hard-locked against broad injected rules", () => {
  const css = readSite("status.css");
  const block = css.match(/svg\.status-icon\s*\{([\s\S]*?)\}/)?.[1] || "";
  assert.match(block, /width:\s*1\.25rem\s*!important/);
  assert.match(block, /height:\s*1\.25rem\s*!important/);
  assert.match(block, /max-width:\s*1\.25rem\s*!important/);
  assert.match(block, /max-height:\s*1\.25rem\s*!important/);
  assert.match(block, /flex:\s*0\s+0\s+1\.25rem\s*!important/);
});

test("public status HEAD responses never contain a JSON body, including errors", async () => {
  const response = await publicStatusRequest({
    request: new Request("https://rigos.site/api/v1/status", { method: "HEAD" }),
    env: {},
  });
  assert.equal(response.status, 503);
  assert.equal(await response.text(), "");
});

test("public status page hides internal database errors", async () => {
  const response = await statusPageRequest({
    request: new Request("https://rigos.site/status", { method: "GET" }),
    env: {
      RIGOS_STATUS_DB: {
        prepare() {
          throw new Error("D1 internal table and binding detail");
        },
      },
    },
  });
  const html = await response.text();
  assert.equal(response.status, 503);
  assert.match(html, /The status service is temporarily unavailable\./);
  assert.doesNotMatch(html, /D1 internal table and binding detail/);
});