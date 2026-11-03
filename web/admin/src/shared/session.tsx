// Who is signed in. The session lives in an HttpOnly cookie; the dashboard only learns about it
// from the API, and keeps the CSRF token in memory.

import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { ApiError, api, get, onSessionEnded, setCsrfToken } from "./api";

export type Session = { admin: { id: string; username: string }; csrf_token: string; expires_at: string };

type SessionState =
  | { status: "loading" }
  | { status: "signed_out"; ended: boolean; error?: unknown }
  | { status: "signed_in"; session: Session };

type SessionContext = {
  state: SessionState;
  signedIn: (session: Session) => void;
  signOut: () => Promise<void>;
};

const Context = createContext<SessionContext | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<SessionState>({ status: "loading" });

  const signedIn = useCallback((session: Session) => {
    setCsrfToken(session.csrf_token);
    setState({ status: "signed_in", session });
  }, []);

  useEffect(() => {
    onSessionEnded(() => setState({ status: "signed_out", ended: true }));
    get<Session>("/session").then(signedIn, (error: unknown) => {
      const signedOut = error instanceof ApiError && error.code === "unauthorized";
      setState({ status: "signed_out", ended: false, error: signedOut ? undefined : error });
    });
  }, [signedIn]);

  const signOut = useCallback(async () => {
    await api("DELETE", "/session");
    setCsrfToken("");
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
