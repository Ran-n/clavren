// A text field with a list of choices under it that loads more as it scrolls: typing narrows the
// list (the caller fetches), ↓/↑ move the highlight, Enter picks, Escape closes, Tab moves on, and
// a click outside closes it. An optional last row offers something besides the rows (e.g. "New
// customer"). Generic: what a row is and what it shows belong to the caller.

import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { describe } from "../errors";

/** Within this many pixels of the bottom, the next page is loaded. */
const LOAD_MORE_MARGIN = 100;

type ExtraRow = { key: string; content: ReactNode; onPick: () => void };

type Props<T> = {
  label: string;
  placeholder: string;
  /** What the list is of, for the chevron and the list's name: "customers". */
  noun: string;
  text: string;
  onText: (text: string) => void;
  open: boolean;
  onOpen: (open: boolean) => void;
  rows: T[];
  rowKey: (row: T) => string;
  renderRow: (row: T) => ReactNode;
  onPick: (row: T) => void;
  extraRow?: ExtraRow | null;
  loading: boolean;
  error: unknown;
  onRetry: () => void;
  more: boolean;
  onLoadMore: () => void;
  /** Shown when nothing is loading, nothing failed and there are no rows. */
  empty: ReactNode;
};

export function Combobox<T>(props: Props<T>) {
  const { label, placeholder, noun, text, onText, open, onOpen, rows, rowKey, renderRow, onPick, extraRow, loading, error, onRetry, more, onLoadMore, empty } = props;
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const [highlight, setHighlight] = useState(-1);
  const choices: { key: string; pick: () => void; content: ReactNode }[] = [
    ...rows.map((row) => ({ key: rowKey(row), pick: () => onPick(row), content: renderRow(row) })),
    ...(extraRow ? [{ key: extraRow.key, pick: extraRow.onPick, content: extraRow.content }] : []),
  ];
  const optionId = (index: number) => `${id}-option-${index}`;
  const active = open && highlight >= 0 && highlight < choices.length ? highlight : -1;

  // A click anywhere else closes the list; the typed text stays. On the click, not the press: the
  // list is in the page's flow, and closing it on the press would move what was clicked.
  useEffect(() => {
    if (!open) return;
    function onClick(event: MouseEvent) {
      if (!root.current?.contains(event.target as Node)) onOpen(false);
    }
    document.addEventListener("click", onClick);
    return () => document.removeEventListener("click", onClick);
  }, [open, onOpen]);

  useEffect(() => {
    if (active >= 0) document.getElementById(`${id}-option-${active}`)?.scrollIntoView({ block: "nearest" });
  }, [active, id]);

  function onScroll() {
    const list = popup.current;
    if (list && more && !loading && list.scrollHeight - list.scrollTop - list.clientHeight < LOAD_MORE_MARGIN) onLoadMore();
  }

  function move(step: number) {
    if (!open) return onOpen(true);
    if (choices.length === 0) return;
    setHighlight((current) => (current < 0 ? (step > 0 ? 0 : choices.length - 1) : (current + step + choices.length) % choices.length));
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      move(event.key === "ArrowDown" ? 1 : -1);
    } else if (event.key === "Enter" && active >= 0) {
      event.preventDefault(); // a pick, not a form submit
      choices[active]?.pick();
    } else if (event.key === "Escape" && open) {
      event.stopPropagation(); // the first Escape closes the list; the next is the surrounding editor's
      onOpen(false);
    } else if (event.key === "Tab") {
      onOpen(false);
    }
  }

  let status: ReactNode = null;
  if (error) {
    status = (
      <>
        Couldn't load {noun}. {describe(error)}{" "}
        <button type="button" className="link-button" onClick={onRetry}>
          Retry
        </button>
      </>
    );
  } else if (loading) status = rows.length === 0 ? "Loading…" : "Loading more…";
  else if (rows.length === 0) status = empty;

  return (
    <div className="combobox" ref={root}>
      <label className="field combobox__field">
        <span className="field__label">{label}</span>
        <span className="combobox__control">
          <input
            role="combobox"
            aria-expanded={open}
            aria-controls={`${id}-list`}
            aria-autocomplete="list"
            aria-activedescendant={active >= 0 ? optionId(active) : undefined}
            value={text}
            placeholder={placeholder}
            maxLength={200}
            autoComplete="off"
            onFocus={() => onOpen(true)}
            onClick={() => onOpen(true)}
            onChange={(event) => {
              onText(event.target.value);
              setHighlight(-1);
              onOpen(true);
            }}
            onKeyDown={onKeyDown}
          />
          <button
            type="button"
            className="combobox__chevron"
            tabIndex={-1}
            aria-label={open ? `Hide ${noun}` : `Show ${noun}`}
            onPointerDown={(event) => event.preventDefault()} // the field keeps the focus
            onClick={() => onOpen(!open)}
          >
            ▾
          </button>
        </span>
      </label>
      {open && (
        <div className="combobox__popup" ref={popup} onScroll={onScroll}>
          <ul className="combobox__list" role="listbox" id={`${id}-list`} aria-label={label}>
            {choices.map((choice, index) => (
              <li
                key={choice.key}
                id={optionId(index)}
                role="option"
                aria-selected={index === active}
                className={`combobox__option ${index === active ? "is-active" : ""}`}
                onPointerDown={(event) => event.preventDefault()}
                onPointerEnter={() => setHighlight(index)}
                onClick={choice.pick}
              >
                {choice.content}
              </li>
            ))}
          </ul>
          {status && (
            <p className="combobox__status" role="status">
              {status}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
