// Who is signed in. The session lives in an HttpOnly cookie; the dashboard only learns about it
// from the API, and keeps the CSRF token in memory. When another tab signs in as someone else, the
// page follows the new session (see api.ts).

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { ApiError, api, get, onSessionEnded, onSessionReplaced, setSession } from "./api";
import type { Session } from "./types";

type SessionState =
  | { status: "loading" }
  | { status: "signed_out"; ended: boolean; error?: unknown }
  /** `switched`: the change that found another admin's session in the cookie, refused for it. */
  | { status: "signed_in"; session: Session; switched?: ApiError };

type SessionContext = {
  state: SessionState;
  signedIn: (session: Session) => void;
  signOut: () => Promise<void>;
};

const Context = createContext<SessionContext | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<SessionState>({ status: "loading" });

  const signedIn = useCallback((session: Session) => {
    setSession(session);
    setState({ status: "signed_in", session });
  }, []);

  useEffect(() => {
    onSessionEnded(() => setState({ status: "signed_out", ended: true }));
    // another admin signed in in another tab: this page now shows their account
    onSessionReplaced((session) =>
      setState({ status: "signed_in", session, switched: new ApiError("session_changed", "", 403) }),
    );
    get<Session>("/session").then(signedIn, (error: unknown) => {
      const signedOut = error instanceof ApiError && error.code === "unauthorized";
      setState({ status: "signed_out", ended: false, error: signedOut ? undefined : error });
    });
  }, [signedIn]);

  const signOut = useCallback(async () => {
    try {
      await api("DELETE", "/session");
    } catch (error: unknown) {
      // a session that already ended is as signed out as it gets
      if (!(error instanceof ApiError && error.code === "unauthorized")) throw error;
    }
    setSession(null);
    setState({ status: "signed_out", ended: false });
  }, []);

  const value = useMemo(() => ({ state, signedIn, signOut }), [state, signedIn, signOut]);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useSession(): SessionContext {
  const context = useContext(Context);
  if (!context) throw new Error("useSession outside SessionProvider");
  return context;
}

export function useSignedIn(): Session {
  const { state } = useSession();
  if (state.status !== "signed_in") throw new Error("useSignedIn while signed out");
  return state.session;
}
