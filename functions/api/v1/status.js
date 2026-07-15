import {
  errorResponse,
  methodNotAllowed,
  publicStatusResponse,
  readPublicStatus,
} from "../../_lib/status.js";

export async function onRequest(context) {
  if (context.request.method !== "GET" && context.request.method !== "HEAD") {
    return methodNotAllowed(["GET", "HEAD"]);
  }

  try {
    const response = publicStatusResponse(await readPublicStatus(context.env));
    if (context.request.method === "HEAD") {
      return new Response(null, { status: response.status, headers: response.headers });
    }
    return response;
  } catch (error) {
    return errorResponse(error);
  }
}
