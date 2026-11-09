// The two E2E servers: one for the flows, one with a fixed data set for the screenshots. Each
// has its own database and data directory under target/, and writes what the tests need to know
// (address, admin credentials) to a JSON file there.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const REPOSITORY = join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..");
const TARGET = join(REPOSITORY, "target");

export type ServerName = "flows" | "rendered";
export type ServerSpec = {
  port: number;
  url: string;
  database: string;
  dataDir: string;
  infoFile: string;
  stateFile: string;
  /** A second admin, for the tests where another admin signs in in another tab. */
  otherAdmin: boolean;
};

function spec(name: ServerName, port: number, otherAdmin: boolean): ServerSpec {
  return {
    port,
    otherAdmin,
    url: `http://127.0.0.1:${port}`,
    database: `clavren_e2e_${name}`,
    dataDir: join(TARGET, `e2e-${name}-data`),
    infoFile: join(TARGET, `e2e-${name}.json`),
    stateFile: join(TARGET, `e2e-${name}-credentials.json`),
  };
}

export const SERVERS: Record<ServerName, ServerSpec> = {
  flows: spec("flows", 8091, true),
  // no second admin: its creation would show in the screenshots' audit log
  rendered: spec("rendered", 8092, false),
};

/** What e2e_server prints when it is ready. */
export type ServerInfo = {
  url: string;
  username: string;
  password: string;
  secret: string;
  recovery_codes: string[];
  frozen_at: string;
  other: { username: string; password: string; recovery_codes: string[] } | null;
};
