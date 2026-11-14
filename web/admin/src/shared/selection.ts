// Row selection for a table: the ticked rows, or every row matching the filters less the ones
// unticked since, so "select all" stays true as the list grows. Cleared when the filters change.

import { useCallback, useState } from "react";

export type Selection = {
  ids: Set<string>;
  /** Every row matching the filter, not only the ones ticked... */
  allMatching: boolean;
  /** ...but these, unticked since. */
  excluded: Set<string>;
  /** Whether the row `id` is ticked. */
  has: (id: string) => boolean;
  toggle: (id: string) => void;
  setPage: (ids: string[], selected: boolean) => void;
  selectAllMatching: () => void;
  clear: () => void;
};

/** Ticked rows; cleared whenever `scope` (the filters) changes. */
export function useSelection(scope: string): Selection {
  const [ids, setIds] = useState<Set<string>>(new Set());
  const [allMatching, setAllMatching] = useState(false);
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [shownScope, setShownScope] = useState(scope);
  if (shownScope !== scope) {
    setShownScope(scope);
    setIds(new Set());
    setAllMatching(false);
    setExcluded(new Set());
  }
  const flip = (id: string) => (current: Set<string>) => {
    const next = new Set(current);
    if (!next.delete(id)) next.add(id);
    return next;
  };
  // under "all matching", unticking a row excludes it, so every other matching row stays chosen
  const toggle = useCallback((id: string) => (allMatching ? setExcluded(flip(id)) : setIds(flip(id))), [allMatching]);
  const setPage = useCallback(
    (page: string[], selected: boolean) => {
      const apply = (add: boolean) => (current: Set<string>) => {
        const next = new Set(current);
        page.forEach((id) => (add ? next.add(id) : next.delete(id)));
        return next;
      };
      if (allMatching) setExcluded(apply(!selected));
      else setIds(apply(selected));
    },
    [allMatching],
  );
  const has = useCallback((id: string) => (allMatching ? !excluded.has(id) : ids.has(id)), [allMatching, excluded, ids]);
  const selectAllMatching = useCallback(() => {
    setAllMatching(true);
    setExcluded(new Set());
  }, []);
  const clear = useCallback(() => {
    setIds(new Set());
    setAllMatching(false);
    setExcluded(new Set());
  }, []);
  return { ids, allMatching, excluded, has, toggle, setPage, selectAllMatching, clear };
}
