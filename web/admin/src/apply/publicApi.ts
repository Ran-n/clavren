// The public application API (`/api/v1/applications`), and what each of its errors means to an
// applicant. Nothing here touches the dashboard's session.

export type FormProduct = { code: string; name: string; description: string | null };
export type Form = { products: FormProduct[]; turnstile_site_key: string | null };
export type KeyPage = {
  code: string;
  name: string;
  description: string | null;
  key_instructions: string | null;
  accepts_applications: boolean;
};
export type Created = { application: string; secret: string };
export type Status = {
  application: string;
  product: string;
  status: "pending" | "approved" | "rejected" | "withdrawn";
  created_at: string;
  decided_at: string | null;
  reason: string | null;
  key: string | null;
  licence_expires_at: string | null;
};

export class PublicError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly retryAfterSeconds?: number,
  ) {
    super(message);
  }
}

const MESSAGES: Record<string, string> = {
  network: "The licence server can't be reached. Check your connection and try again.",
  product_unavailable: "This product doesn't take applications at the moment.",
  application_pending: "This address already has an application waiting for this product. You'll hear back once it is decided.",
  application_unknown: "This link doesn't name an application. It may be mistyped, or the application was deleted.",
  human_check_failed: "The check that you're a person didn't pass. Complete it again, then send the form.",
  internal_error: "Something went wrong on the licence server. Try again in a few minutes.",
};

/** The error in plain words, with what to do. */
export function explain(error: unknown): string {
  if (!(error instanceof PublicError)) return "Something unexpected happened. Reload the page and try again.";
  if (error.code === "rate_limited") {
    const minutes = Math.max(1, Math.ceil((error.retryAfterSeconds ?? 60) / 60));
    return `Too many attempts from your network. Wait ${minutes} minute${minutes === 1 ? "" : "s"} and try again.`;
  }
  if (error.code === "invalid_request" && error.message) {
    return `${error.message.charAt(0).toUpperCase()}${error.message.slice(1)}.`;
  }
  return MESSAGES[error.code] ?? MESSAGES.internal_error ?? "";
}

async function call<T>(method: "GET" | "POST", path: string, body?: unknown): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`/api/v1${path}`, {
      method,
      headers: body === undefined ? {} : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: "omit",
    });
  } catch {
    throw new PublicError("network", "");
  }
  const parsed = (await response.json().catch(() => null)) as (T & { error?: string; message?: string }) | null;
  if (!response.ok || parsed === null) {
    const retryAfter = Number(response.headers.get("Retry-After")) || undefined;
    throw new PublicError(parsed?.error ?? "internal_error", parsed?.message ?? "", retryAfter);
  }
  return parsed;
}

export const loadForm = () => call<Form>("GET", "/applications/form");

export const loadKeyPage = (code: string) => call<KeyPage>("GET", `/applications/products/${encodeURIComponent(code)}`);

export type Application = { product: string; name: string; email: string; message: string; turnstileToken: string | null };

export const sendApplication = (application: Application) =>
  call<Created>("POST", "/applications", {
    product: application.product,
    name: application.name,
    email: application.email,
    message: application.message.trim() || undefined,
    source: "web",
    turnstile_token: application.turnstileToken ?? undefined,
  });

export const askStatus = (secret: string) => call<Status>("POST", "/applications/status", { secret });

/** The page that shows how the application with `secret` went. The secret is in the fragment, so
 * it never reaches the server's or any proxy's logs. */
export function statusLink(secret: string): string {
  return `${window.location.origin}/apply/status#${secret}`;
}
