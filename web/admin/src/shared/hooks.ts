// General hooks: a value that settles after typing stops, the browser tab's title, and giving the
// keyboard its place back when an inline editor closes. Paging and selection live in paging.ts and
// selection.ts.

import { useEffect, useRef, useState, type RefObject } from "react";

/** `value`, once it has stopped changing for `delayMs`. */
export function useDebounced<T>(value: T, delayMs = 300): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(value), delayMs);
    return () => window.clearTimeout(timer);
  }, [value, delayMs]);
  return settled;
}

/**
 * Gives the keyboard its place back when an editor that took the place of its trigger closes
 * (Enter, Escape): without it focus falls to the page and Tab starts again from the top. Put the
 * ref on the trigger, or on what holds it; nothing happens when focus already went elsewhere
 * (the editor closed because the person clicked or tabbed away).
 */
export function useFocusOnClose<T extends HTMLElement>(open: boolean): RefObject<T | null> {
  const home = useRef<T>(null);
  const wasOpen = useRef(open);
  useEffect(() => {
    const closed = wasOpen.current && !open;
    wasOpen.current = open;
    if (closed) focusIfLost(home.current);
  }, [open]);
  return home;
}

const FOCUSABLE = "button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled)";

/** Focuses `home` (or the first control in it) when nothing on the page has focus. */
export function focusIfLost(home: HTMLElement | null): void {
  if (!home?.isConnected) return;
  if (document.activeElement && document.activeElement !== document.body) return;
  (home.matches(FOCUSABLE) ? home : home.querySelector<HTMLElement>(FOCUSABLE))?.focus();
}

/** Sets the browser tab's title for the page. */
export function useTitle(title: string): void {
  useEffect(() => {
    document.title = `${title} · Clavren`;
  }, [title]);
}
