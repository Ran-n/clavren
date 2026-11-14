// Cursor paging over an API list: one page at a time with Previous and Next (`usePagedList`), or
// one list that grows page by page as it scrolls (`useInfiniteList`). Also the page sizes offered.

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

function withLimit(path: string, pageSize: number): string {
  return `${path}${path.includes("?") ? "&" : "?"}limit=${pageSize}`;
}

/**
 * The page of `source` after the cursor `after`, loaded again whenever `version` changes; with
 * `append`, a later page is added to the rows already loaded instead of replacing them. A slower
 * answer to an older request never replaces a newer one. A `null` source loads nothing.
 */
function useListPage<T>(source: string | null, after: string | null, version: number, append: boolean) {
  const [state, setState] = useState<ListState<T>>({ rows: [], next: null, loading: source !== null, error: null });
  useEffect(() => {
    if (source === null) return;
    let current = true;
    setState((previous) => ({ ...previous, loading: true, error: null }));
    const url = after ? `${source}&after=${encodeURIComponent(after)}` : source;
    get<Page<T>>(url).then(
      (page) =>
        current &&
        setState((previous) => ({ rows: append && after ? [...previous.rows, ...page.data] : page.data, next: page.next_cursor, loading: false, error: null })),
      (error: unknown) => current && setState((previous) => ({ ...previous, loading: false, error })),
    );
    return () => {
      current = false;
    };
  }, [source, after, version, append]);
  return [state, setState] as const;
}

/** One page of `path` (which already holds its filters), with Previous and Next. */
export function usePagedList<T>(path: string, pageSize: number): PagedList<T> {
  const source = withLimit(path, pageSize);
  const [shown, setShown] = useState(source);
  const [cursors, setCursors] = useState<(string | null)[]>([null]);
  const [version, setVersion] = useState(0);
  if (shown !== source) {
    // New filters start again from the first page.
    setShown(source);
    setCursors([null]);
  }
  const after = cursors[cursors.length - 1] ?? null;
  const [state, setState] = useListPage<T>(source, after, version, false);

  // while a page loads, `next` still belongs to the page before: a second press would repeat it
  const nextCursor = state.loading ? null : state.next;
  const goNext = useCallback(() => setCursors((stack) => (nextCursor ? [...stack, nextCursor] : stack)), [nextCursor]);
  const goPrevious = useCallback(() => setCursors((stack) => (stack.length > 1 ? stack.slice(0, -1) : stack)), []);
  const reload = useCallback(() => setVersion((value) => value + 1), []);
  const replace = useCallback((row: T, matches: (existing: T) => boolean) => {
    setState((previous) => ({ ...previous, rows: previous.rows.map((existing) => (matches(existing) ? row : existing)) }));
  }, []);

  return { ...state, page: cursors.length, hasPrevious: cursors.length > 1, goNext, goPrevious, reload, replace };
}

export type InfiniteList<T> = {
  rows: T[];
  loading: boolean;
  error: unknown;
  /** More rows follow the ones loaded. */
  more: boolean;
  loadMore: () => void;
  retry: () => void;
};

/** `path` page after page, each added below the last (for a list that loads as it scrolls); a
 * `null` path loads nothing. A new path starts again from the top. */
export function useInfiniteList<T>(path: string | null, pageSize: number): InfiniteList<T> {
  const source = path === null ? null : withLimit(path, pageSize);
  const [shown, setShown] = useState(source);
  const [after, setAfter] = useState<string | null>(null);
  const [version, setVersion] = useState(0);
  const [state, setState] = useListPage<T>(source, after, version, true);
  if (shown !== source) {
    setShown(source);
    setAfter(null);
    setState({ rows: [], next: null, loading: source !== null, error: null });
  }
  const { next, loading, error } = state;
  const loadMore = useCallback(() => {
    if (next && !loading && !error) setAfter(next);
  }, [next, loading, error]);
  const retry = useCallback(() => setVersion((value) => value + 1), []);
  return { rows: state.rows, loading, error, more: Boolean(next), loadMore, retry };
}
