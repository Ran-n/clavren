// Hooks every table page uses: cursor paging over an API list, row selection, and a value that
// settles after typing stops.

import { useCallback, useEffect, useState } from "react";
import { get } from "./api";
import type { Page } from "./types";

export const PAGE_SIZES = [10, 25, 50, 100] as const;
export const DEFAULT_PAGE_SIZE = 50;

type ListState<T> = { rows: T[]; next: string | null; loading: boolean; error: unknown };

export type PagedList<T> = ListState<T> & {
  page: number;
  hasPrevious: boolean;
  goNext: () => void;
  goPrevious: () => void;
  reload: () => void;
  replace: (row: T, matches: (existing: T) => boolean) => void;
};

/** One page of `path` (which already holds its filters), with Previous and Next. */
export function usePagedList<T>(path: string, pageSize: number): PagedList<T> {
  const source = `${path}${path.includes("?") ? "&" : "?"}limit=${pageSize}`;
  const [shown, setShown] = useState(source);
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const [version, setVersion] = useState(0);
  const [state, setState] = useState<ListState<T>>({ rows: [], next: null, loading: true, error: null });
  if (shown !== source) {
    // New filters start again from the first page.
    setShown(source);
    setCursors([null]);
  }
  const after = cursors[cursors.length - 1] ?? null;

  useEffect(() => {
    let current = true;
    setState((previous) => ({ ...previous, loading: true, error: null }));
    const url = after ? `${source}&after=${encodeURIComponent(after)}` : source;
    get<Page<T>>(url).then(
      (page) => current && setState({ rows: page.data, next: page.next_cursor, loading: false, error: null }),
      (error: unknown) => current && setState((previous) => ({ ...previous, loading: false, error })),
    );
    return () => {
      current = false;
    };
  }, [source, after, version]);

  const goNext = useCallback(() => setCursors((stack) => (state.next ? [...stack, state.next] : stack)), [state.next]);
  const goPrevious = useCallback(() => setCursors((stack) => (stack.length > 1 ? stack.slice(0, -1) : stack)), []);
  const reload = useCallback(() => setVersion((value) => value + 1), []);
  const replace = useCallback((row: T, matches: (existing: T) => boolean) => {
    setState((previous) => ({ ...previous, rows: previous.rows.map((existing) => (matches(existing) ? row : existing)) }));
  }, []);

  return { ...state, page: cursors.length, hasPrevious: cursors.length > 1, goNext, goPrevious, reload, replace };
}

export type Selection = {
  ids: Set<string>;
  /** Every row matching the filter, not only the ones ticked. */
  allMatching: boolean;
  toggle: (id: string) => void;
  setPage: (ids: string[], selected: boolean) => void;
  selectAllMatching: () => void;
  clear: () => void;
};

/** Ticked rows; cleared whenever `scope` (the filters) changes. */
export function useSelection(scope: string): Selection {
  const [ids, setIds] = useState<Set<string>>(new Set());
  const [allMatching, setAllMatching] = useState(false);
  const [shownScope, setShownScope] = useState(scope);
  if (shownScope !== scope) {
    setShownScope(scope);
    setIds(new Set());
    setAllMatching(false);
  }
  const toggle = useCallback((id: string) => {
    setAllMatching(false);
    setIds((current) => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  }, []);
  const setPage = useCallback((page: string[], selected: boolean) => {
    setAllMatching(false);
    setIds((current) => {
      const next = new Set(current);
      page.forEach((id) => (selected ? next.add(id) : next.delete(id)));
      return next;
    });
  }, []);
  const selectAllMatching = useCallback(() => setAllMatching(true), []);
  const clear = useCallback(() => {
    setIds(new Set());
    setAllMatching(false);
  }, []);
  return { ids, allMatching, toggle, setPage, selectAllMatching, clear };
}

/** `value`, once it has stopped changing for `delayMs`. */
export function useDebounced<T>(value: T, delayMs = 300): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [value, delayMs]);
  return settled;
}

/** Sets the browser tab's title for the page. */
export function useTitle(title: string): void {
  useEffect(() => {
    document.title = `${title} · Clavren`;
  }, [title]);
}
