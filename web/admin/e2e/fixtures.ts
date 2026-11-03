// The fixtures every spec uses: `admin`, a signed-in page (signing in again with a recovery code
// when a test ended the shared session), `api`, the admin API for setting things up and for
// checking results, and `device`, a computer activating keys through the public API.

import { test as base, expect, request, type APIRequestContext, type Page } from "@playwright/test";
import { generateKeyPairSync } from "node:crypto";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { Credentials } from "./credentials";
import { SERVERS, type ServerName } from "./servers";

type Session = { cookie: string; csrf: string };

export const DEFAULT_PREFERENCES = { version: 1, theme: "system", tables: {} };

export class AdminApi {
  constructor(
    private readonly context: APIRequestContext,
    private readonly session: Session,
  ) {}

  async call<T>(method: string, path: string, data?: unknown): Promise<T> {
    const response = await this.context.fetch(`/api/admin${path}`, {
      method,
      data,
      headers: { cookie: `clavren_session=${this.session.cookie}`, "x-csrf-token": this.session.csrf },
    });
    if (!response.ok()) throw new Error(`${method} ${path}: ${response.status()} ${await response.text()}`);
    return (response.status() === 204 ? undefined : await response.json()) as T;
  }

  get<T>(path: string): Promise<T> {
    return this.call<T>("GET", path);
  }
}

/** Signs in through the API with the next spare code; the session is kept in a file. */
async function signIn(context: APIRequestContext, credentials: Credentials, sessionFile: string): Promise<Session> {
  if (existsSync(sessionFile)) {
    const saved = JSON.parse(readFileSync(sessionFile, "utf8")) as Session;
    const check = await context.get("/api/admin/session", { headers: { cookie: `clavren_session=${saved.cookie}` } });
    if (check.ok()) return saved;
  }
  const { username, password } = credentials.info;
  const response = await context.post("/api/admin/session", { data: { username, password, code: credentials.recoveryCode() } });
  expect(response.ok(), await response.text()).toBe(true);
  const cookie = /clavren_session=([^;]+)/.exec(response.headers()["set-cookie"] ?? "")?.[1];
  if (!cookie) throw new Error("sign-in set no session cookie");
  const session = { cookie, csrf: ((await response.json()) as { csrf_token: string }).csrf_token };
  writeFileSync(sessionFile, JSON.stringify(session));
  return session;
}

export async function signedInPage(page: Page, session: Session, baseURL: string): Promise<void> {
  const { hostname } = new URL(baseURL);
  await page.context().addCookies([{ name: "clavren_session", value: session.cookie, domain: hostname, path: "/", httpOnly: true, sameSite: "Strict" }]);
}

export class Device {
  readonly publicKey: string;
  constructor(readonly name: string) {
    const { publicKey } = generateKeyPairSync("ed25519");
    this.publicKey = publicKey.export({ format: "jwk" }).x as string;
  }

  /** Activates `key`; returns the activation ID. `address` keeps each test under the rate limit. */
  async activate(context: APIRequestContext, product: string, key: string, address: string): Promise<string> {
    const response = await context.post("/api/v1/activations", {
      data: { product, key, device_public_key: this.publicKey, device_name: this.name, platform: "windows-x86_64", app_version: "1.4.0" },
      headers: { "cf-connecting-ip": address },
    });
    expect(response.status(), await response.text()).toBe(201);
    return ((await response.json()) as { activation: string }).activation;
  }
}

type Fixtures = { server: ServerName; credentials: Credentials; session: Session; api: AdminApi; admin: Page };

export const test = base.extend<Fixtures>({
  server: ["flows", { option: true }],
  credentials: async ({ server }, use) => use(new Credentials(SERVERS[server])),
  session: async ({ server, credentials, baseURL }, use) => {
    const context = await request.newContext({ baseURL });
    await use(await signIn(context, credentials, `${SERVERS[server].stateFile}.session`));
    await context.dispose();
  },
  api: async ({ baseURL, session }, use) => {
    const context = await request.newContext({ baseURL });
    await use(new AdminApi(context, session));
    await context.dispose();
  },
  admin: async ({ page, session, api, baseURL }, use) => {
    // Every test starts from the defaults; the preferences test changes them on purpose.
    await api.call("PUT", "/preferences", DEFAULT_PREFERENCES);
    await signedInPage(page, session, baseURL as string);
    await use(page);
  },
});

export { expect };
