// The admin API client: JSON in and out, the session's CSRF token on every change, and one error
// type for everything that can go wrong, network included.

export class ApiError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
    readonly ids: string[] = [],
    readonly retryAfterSeconds?: number,
  ) {
    super(message);
  }
}

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

let csrfToken = "";
let sessionEnded: () => void = () => {};

export function setCsrfToken(token: string): void {
  csrfToken = token;
}

/** Called when any request finds the session gone (expired, or signed out elsewhere). */
export function onSessionEnded(handler: () => void): void {
  sessionEnded = handler;
}

async function send(method: Method, path: string, body?: unknown): Promise<Response> {
  const headers: Record<string, string> = {};
  if (method !== "GET") headers["X-CSRF-Token"] = csrfToken;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  let response: Response;
  try {
    response = await fetch(`/api/admin${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: "same-origin",
    });
  } catch {
    throw new ApiError("network", "the server could not be reached", 0);
  }
  if (response.ok) return response;
  const parsed = (await response.json().catch(() => null)) as { error?: string; message?: string; ids?: string[] } | null;
  const code = parsed?.error ?? (response.status >= 500 ? "internal_error" : "unexpected");
  const retryAfter = Number(response.headers.get("Retry-After")) || undefined;
  if (code === "unauthorized" && !path.startsWith("/session")) sessionEnded();
  throw new ApiError(code, parsed?.message ?? "", response.status, parsed?.ids ?? [], retryAfter);
}

export async function api<T>(method: Method, path: string, body?: unknown): Promise<T> {
  const response = await send(method, path, body);
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export function get<T>(path: string): Promise<T> {
  return api<T>("GET", path);
}

/** POSTs `body` and saves the answer as a file named `filename`. */
export async function download(path: string, body: unknown, filename: string): Promise<void> {
  const response = await send("POST", path, body);
  const url = URL.createObjectURL(await response.blob());
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

type QueryValue = string | number | boolean | null | undefined;

/** `?a=1&b=2` from the values that are set; an empty string when none is. */
export function query(values: Record<string, QueryValue>): string {
  const params = new URLSearchParams();
  for (const [name, value] of Object.entries(values)) {
    if (value !== undefined && value !== null && value !== "") params.set(name, String(value));
  }
  const text = params.toString();
  return text ? `?${text}` : "";
}
