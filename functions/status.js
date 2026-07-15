import {
  methodNotAllowed,
  readPublicStatus,
  renderStatusPage,
} from "./_lib/status.js";

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
