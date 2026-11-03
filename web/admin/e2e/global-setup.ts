// Builds the dashboard, then starts both E2E servers (each builds and embeds it) and waits until
// they print their ready line. Returns the teardown, which closes their stdin so they shut down
// cleanly, and kills them if they don't.

import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { rmSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { REPOSITORY, SERVERS, type ServerSpec } from "./servers";

const START_TIMEOUT_MS = 30 * 60_000; // the first build on a busy machine is slow
const STOP_TIMEOUT_MS = 15_000;

function buildDashboard(): void {
  const build = spawnSync("npm", ["run", "build"], { cwd: `${REPOSITORY}/web/admin`, stdio: "inherit", shell: true });
  if (build.status !== 0) throw new Error("npm run build failed");
  const example = spawnSync("cargo", ["build", "--example", "e2e_server"], { cwd: REPOSITORY, stdio: "inherit" });
  if (example.status !== 0) throw new Error("cargo build --example e2e_server failed");
}

function start(spec: ServerSpec): Promise<ChildProcess> {
  rmSync(spec.stateFile, { force: true });
  rmSync(`${spec.stateFile}.session`, { force: true });
  const child = spawn("cargo", ["run", "--quiet", "--example", "e2e_server", "--", String(spec.port), spec.dataDir, spec.database], {
    cwd: REPOSITORY,
    stdio: ["pipe", "pipe", "inherit"],
    env: { ...process.env, CLAVREN_LOG: "warn" },
  });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${spec.database}: no ready line in time`)), START_TIMEOUT_MS);
    child.once("exit", (code) => reject(new Error(`${spec.database}: e2e_server exited with ${code}`)));
    createInterface({ input: child.stdout as NodeJS.ReadableStream }).once("line", (line) => {
      clearTimeout(timer);
      writeFileSync(spec.infoFile, line);
      resolve(child);
    });
  });
}

function stop(child: ChildProcess): Promise<void> {
  return new Promise((resolve) => {
    if (child.exitCode !== null) return resolve();
    const timer = setTimeout(() => {
      child.kill();
      resolve();
    }, STOP_TIMEOUT_MS);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
    child.stdin?.end();
  });
}

export default async function globalSetup(): Promise<() => Promise<void>> {
  buildDashboard();
  const servers = await Promise.all(Object.values(SERVERS).map(start));
  return async () => {
    await Promise.all(servers.map(stop));
  };
}
