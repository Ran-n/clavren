// The admin API client: JSON in and out, the session's CSRF token on every change, and one error
// type for everything that can go wrong, network included.

import type { Session } from "./types";

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

/** The session this page acts for: its CSRF token, and whose it is. */
let current: { csrfToken: string; adminId: string } | null = null;
let sessionEnded: () => void = () => {};
let sessionReplaced: (session: Session) => void = () => {};
/** What a request was sent with. */
type Sent = { csrfToken: string; adminId: string | null };
/** One `GET /session` for every request refused at the same time. */
let refreshing: Promise<Session> | null = null;

export function setSession(session: Session | null): void {
  current = session ? { csrfToken: session.csrf_token, adminId: session.admin.id } : null;
}

/** The admin this page acts for, or null when signed out. */
export function signedInAdmin(): string | null {
  return current?.adminId ?? null;
}

/** Called when any request finds the session gone (expired, or signed out elsewhere). */
export function onSessionEnded(handler: () => void): void {
  sessionEnded = handler;
}

/** Called when the cookie now belongs to another admin (signed in in another tab). */
export function onSessionReplaced(handler: (session: Session) => void): void {
  sessionReplaced = handler;
}

async function request(method: Method, path: string, csrfToken: string, body?: unknown): Promise<Response> {
  const headers: Record<string, string> = {};
  if (method !== "GET") headers["X-CSRF-Token"] = csrfToken;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  try {
    return await fetch(`/api/admin${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: "same-origin",
    });
  } catch {
    throw new ApiError("network", "the server could not be reached", 0);
  }
}

async function failure(response: Response): Promise<ApiError> {
  const parsed = (await response.json().catch(() => null)) as { error?: string; message?: string; ids?: string[] } | null;
  const code = parsed?.error ?? (response.status >= 500 ? "internal_error" : "unexpected");
  const retryAfter = Number(response.headers.get("Retry-After")) || undefined;
  return new ApiError(code, parsed?.message ?? "", response.status, parsed?.ids ?? [], retryAfter);
}

function refreshSession(): Promise<Session> {
  if (!refreshing) {
    refreshing = (async () => {
      const response = await request("GET", "/session", "");
      if (!response.ok) throw await failure(response);
      return (await response.json()) as Session;
    })().finally(() => {
      refreshing = null;
    });
  }
  return refreshing;
}

/**
 * A change refused for its CSRF token: the cookie now holds a newer session (signed in again in
 * another tab). The guard refuses before anything happens, so the change can be sent again, once,
 * with the new session's token, as long as it is still the same admin's.
 */
async function recover(method: Method, path: string, sent: Sent, refused: ApiError, body?: unknown): Promise<Response> {
  // signed out meanwhile: there is no session to recover
  if (!current) throw refused;
  // another request refused at the same time may have fetched the new session already
  if (current.csrfToken === sent.csrfToken) {
    let session: Session;
    try {
      session = await refreshSession();
    } catch (error: unknown) {
      if (error instanceof ApiError && error.code === "unauthorized") sessionEnded();
      throw error;
    }
    setSession(session);
    if (session.admin.id !== sent.adminId) sessionReplaced(session);
  }
  if (current?.adminId !== sent.adminId) {
    // sending it again would act as an admin the page doesn't name
    throw new ApiError("session_changed", "the session now belongs to another admin", 403);
  }
  return send(method, path, body, true);
}

async function send(method: Method, path: string, body?: unknown, retried = false): Promise<Response> {
  const sent: Sent = { csrfToken: current?.csrfToken ?? "", adminId: current?.adminId ?? null };
  const response = await request(method, path, sent.csrfToken, body);
  if (response.ok) return response;
  const error = await failure(response);
  if (error.code === "csrf_failed" && !retried && sent.adminId) return recover(method, path, sent, error, body);
  // signing in and asking whether a session exists answer `unauthorized` as their normal outcome;
  // anything else (signing out included) means the session has ended
  const asksAboutSession = path === "/session" && (method === "POST" || method === "GET");
  if (error.code === "unauthorized" && !asksAboutSession) sessionEnded();
  throw error;
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
