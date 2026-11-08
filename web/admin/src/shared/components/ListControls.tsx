// The controls around a table: the search box, a filter select, Previous/Next with the page size,
// and the column picker. Their values are remembered by the page through its table preferences.

import type { ReactNode } from "react";
import { PAGE_SIZES, type PagedList } from "../hooks";
import { Button } from "./Button";

export function SearchBox({ value, onChange, label }: { value: string; onChange: (value: string) => void; label: string }) {
  return (
    <input
      type="search"
      className="search-box"
      aria-label={label}
      placeholder={label}
      value={value}
      maxLength={200}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

export type Option = { value: string; label: string; group?: string };

function renderOptions(options: Option[]) {
  return options.map((option) => (
    <option key={option.value} value={option.value}>
      {option.label}
    </option>
  ));
}

/** A select; options with a `group` go under that heading, after the ungrouped ones. */
export function FilterSelect({ label, value, options, onChange }: { label: string; value: string; options: Option[]; onChange: (value: string) => void }) {
  const groups = [...new Set(options.flatMap((option) => (option.group ? [option.group] : [])))];
  return (
    <label className="filter">
      <span className="filter__label">{label}</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        {renderOptions(options.filter((option) => !option.group))}
        {groups.map((group) => (
          <optgroup key={group} label={group}>
            {renderOptions(options.filter((option) => option.group === group))}
          </optgroup>
        ))}
      </select>
    </label>
  );
}

const EXCEPT = "not:";

/**
 * One filter field that can keep ("Only") or hide ("Except") a value, in one select. The choice
 * lives in the filters as `name` or `not_<name>`; writing one clears the other, so both are never
 * set from the page.
 */
export function onlyOrExcept(name: string, any: string, options: Option[], exceptLabel: (option: Option) => string) {
  const exceptName = `not_${name}`;
  return {
    options: [
      { value: "", label: any },
      ...options.map((option) => ({ ...option, group: "Only" })),
      ...options.map((option) => ({ value: `${EXCEPT}${option.value}`, label: exceptLabel(option), group: "Except" })),
    ],
    read: (filters: Record<string, string | undefined>) =>
      filters[exceptName] ? `${EXCEPT}${filters[exceptName]}` : (filters[name] ?? ""),
    write: (choice: string): Record<string, string> =>
      choice.startsWith(EXCEPT) ? { [name]: "", [exceptName]: choice.slice(EXCEPT.length) } : { [name]: choice, [exceptName]: "" },
  };
}

export function Pager<T>({ list, pageSize, onPageSize }: { list: PagedList<T>; pageSize: number; onPageSize: (size: number) => void }) {
  return (
    <nav className="pager" aria-label="Pages">
      <Button size="small" onClick={list.goPrevious} disabledReason={list.hasPrevious ? null : "This is the first page"}>
        Previous
      </Button>
      <span className="pager__page">Page {list.page}</span>
      <Button size="small" onClick={list.goNext} disabledReason={list.next ? null : "This is the last page"}>
        Next
      </Button>
      <label className="pager__size">
        Rows
        <select value={pageSize} onChange={(event) => onPageSize(Number(event.target.value))}>
          {PAGE_SIZES.map((size) => (
            <option key={size} value={size}>
              {size}
            </option>
          ))}
        </select>
      </label>
    </nav>
  );
}

type PickerColumn = { key: string; label: string; optional?: boolean };

export function ColumnPicker({ columns, visible, onChange }: { columns: PickerColumn[]; visible?: string[]; onChange: (visible: string[]) => void }) {
  const optional = columns.filter((column) => column.optional);
  const shown = visible ?? optional.map((column) => column.key);
  function toggle(key: string, on: boolean) {
    onChange(on ? [...shown, key] : shown.filter((existing) => existing !== key));
  }
  return (
    <details className="column-picker">
      <summary>Columns</summary>
      <div className="column-picker__list">
        {optional.map((column) => (
          <label key={column.key}>
            <input type="checkbox" checked={shown.includes(column.key)} onChange={(event) => toggle(column.key, event.target.checked)} />
            {column.label}
          </label>
        ))}
      </div>
    </details>
  );
}

export function Toolbar({ children }: { children: ReactNode }) {
  return <div className="toolbar">{children}</div>;
}
