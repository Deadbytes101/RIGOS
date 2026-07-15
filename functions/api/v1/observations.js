import {
  acceptObservation,
  errorResponse,
  methodNotAllowed,
} from "../../../_lib/status.js";

export async function onRequest(context) {
  if (context.request.method !== "POST") {
    return methodNotAllowed(["POST"]);
  }

  try {
    return await acceptObservation(context.request, context.env);
  } catch (error) {
    return errorResponse(error);
  }
}
