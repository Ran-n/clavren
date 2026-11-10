// Server updates: the running version, a newer release, checking now and installing it. One
// provider per sign-in loads `GET /updates` once, so the notice on every page and the Overview
// share the request; polling (after Check now, during an update) stops on sign-out.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, get } from "./api";
import type { Updates } from "./types";

/** Check now: the check runs on the server and usually takes a few seconds. */
const CHECK_POLL_MS = 2_000;
const CHECK_GIVE_UP_MS = 60_000;
/** An update: download, backup, install, restart. Connection errors while it restarts are normal. */
const INSTALL_POLL_MS = 3_000;
const INSTALL_GIVE_UP_MS = 5 * 60_000;
/** Remembers across the reload after an update which version it was, to say so once. */
const UPDATED_KEY = "clavren.updated_to";

export type CheckProgress = "idle" | "waiting" | "slow";

/** An update this page started or found running: where it is from the dashboard's point of view. */
export type Installing = { version: string; phase: "running" | "restarting" | "slow" };

type UpdatesContext = {
  /** The last answer, kept when a later request fails. */
  updates: Updates | null;
  /** Why the last request failed; cleared by the next answer. */
  error: unknown;
  /** Asks again after a failure. */
  retry: () => void;
  check: CheckProgress;
  checkNow: () => Promise<void>;
  installing: Installing | null;
  install: (version: string, skipBackup: boolean) => Promise<void>;
  /** The version this server was just updated to, shown once after the reload. */
  updatedTo: string | null;
};

const Context = createContext<UpdatesContext | null>(null);

function takeUpdatedTo(): string | null {
  try {
    const version = window.sessionStorage.getItem(UPDATED_KEY);
    window.sessionStorage.removeItem(UPDATED_KEY);
    return version;
  } catch {
    return null; // storage blocked: the page still works, it just doesn't say "Updated"
  }
}

function rememberUpdatedTo(version: string): void {
  try {
    window.sessionStorage.setItem(UPDATED_KEY, version);
  } catch {
    // storage blocked: the reload still happens, without the one-time message
  }
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export function UpdatesProvider({ children }: { children: ReactNode }) {
  const [updates, setUpdates] = useState<Updates | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [check, setCheck] = useState<CheckProgress>("idle");
  const [installing, setInstalling] = useState<Installing | null>(null);
  const [updatedTo] = useState(takeUpdatedTo);
  // Unmounting (signing out) ends every poll.
  const alive = useRef(true);
  const polling = useRef(false);

  const load = useCallback(async () => {
    const answer = await get<Updates>("/updates");
    if (!alive.current) return answer;
    setUpdates(answer);
    setError(null);
    return answer;
  }, []);

  const follow = useCallback(async (version: string) => {
    if (polling.current) return;
    polling.current = true;
    setInstalling({ version, phase: "running" });
    const giveUpAt = Date.now() + INSTALL_GIVE_UP_MS;
    try {
      while (alive.current) {
        await wait(INSTALL_POLL_MS);
        if (!alive.current) return;
        if (Date.now() > giveUpAt) {
          setInstalling({ version, phase: "slow" });
          return;
        }
        let answer: Updates;
        try {
          answer = await get<Updates>("/updates");
        } catch {
          setInstalling({ version, phase: "restarting" }); // expected while the server restarts
          continue;
        }
        if (!alive.current) return;
        setUpdates(answer);
        setError(null);
        if (answer.running_version === version) {
          rememberUpdatedTo(version);
          window.location.reload(); // the new release's dashboard files
          return;
        }
        if (answer.update?.state !== "running") {
          setInstalling(null); // failed: the notice shows why, with the button again
          return;
        }
        setInstalling({ version, phase: "running" });
      }
    } finally {
      polling.current = false;
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    load().then(
      (answer) => {
        const running = answer.update?.state === "running" ? answer.update.version : null;
        if (running) void follow(running);
      },
      (failure: unknown) => alive.current && setError(failure),
    );
    return () => {
      alive.current = false;
    };
  }, [load, follow]);

  const checkNow = useCallback(async () => {
    const before = updates?.last_check?.checked_at ?? null;
    await api("POST", "/updates/check");
    setCheck("waiting");
    const giveUpAt = Date.now() + CHECK_GIVE_UP_MS;
    while (alive.current && Date.now() < giveUpAt) {
      await wait(CHECK_POLL_MS);
      if (!alive.current) return;
      try {
        const answer = await load();
        if ((answer.last_check?.checked_at ?? null) !== before) {
          setCheck("idle");
          return;
        }
      } catch (failure) {
        setError(failure);
        setCheck("idle");
        return;
      }
    }
    if (alive.current) setCheck("slow");
  }, [updates, load]);

  const retry = useCallback(() => {
    setError(null);
    load().catch((failure: unknown) => alive.current && setError(failure));
  }, [load]);

  const install = useCallback(
    async (version: string, skipBackup: boolean) => {
      await api("POST", "/updates/install", { version, skip_backup: skipBackup });
      void follow(version);
    },
    [follow],
  );

  const value = useMemo(
    () => ({ updates, error, retry, check, checkNow, installing, install, updatedTo }),
    [updates, error, retry, check, checkNow, installing, install, updatedTo],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useUpdates(): UpdatesContext {
  const context = useContext(Context);
  if (!context) throw new Error("useUpdates outside UpdatesProvider");
  return context;
}
