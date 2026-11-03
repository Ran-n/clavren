// A small client-side router: the address bar is the state, links push to it, and components
// read it with useLocation.

import { useSyncExternalStore, type MouseEvent, type ReactNode } from "react";

const listeners = new Set<() => void>();
window.addEventListener("popstate", () => listeners.forEach((listener) => listener()));

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function current(): string {
  return window.location.pathname + window.location.search;
}

export function navigate(to: string, options: { replace?: boolean } = {}): void {
  if (to === current()) return;
  window.history[options.replace ? "replaceState" : "pushState"](null, "", to);
  listeners.forEach((listener) => listener());
}

export function useLocation(): { path: string; search: URLSearchParams } {
  const href = useSyncExternalStore(subscribe, current);
  const url = new URL(href, window.location.origin);
  return { path: url.pathname, search: url.searchParams };
}

type LinkProps = { to: string; children: ReactNode; className?: string; current?: boolean };

export function Link({ to, children, className, current: isCurrent }: LinkProps) {
  function follow(event: MouseEvent<HTMLAnchorElement>) {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    navigate(to);
  }
  return (
    <a href={to} onClick={follow} className={className} aria-current={isCurrent ? "page" : undefined}>
      {children}
    </a>
  );
}
