// A table of records: the visible columns, optional row checkboxes with "select all matching",
// an optional editable first row for creating a record, and rows that can unfold below
// themselves (an inline confirmation, say). On a phone each row becomes a card, one field per line
// (components.css), so nothing scrolls sideways.

import { Fragment, type ReactNode } from "react";
import type { Selection } from "../hooks";

export type Column<T> = {
  key: string;
  label: string;
  render: (row: T) => ReactNode;
  /** Can be hidden from the column picker. */
  optional?: boolean;
  className?: string;
};

type Props<T> = {
  rows: T[];
  columns: Column<T>[];
  /** The optional columns shown; all of them when not set. */
  visible?: string[];
  rowKey: (row: T) => string;
  label: string;
  selection?: Selection;
  hasMore?: boolean;
  firstRow?: ReactNode;
  below?: (row: T) => ReactNode;
  empty: ReactNode;
  loading: boolean;
  /** Why the rows couldn't be loaded; then the table says nothing about being empty. */
  error?: unknown;
};

export function shownColumns<T>(columns: Column<T>[], visible?: string[]): Column<T>[] {
  return columns.filter((column) => !column.optional || !visible || visible.includes(column.key));
}

export function DataTable<T>({ rows, columns, visible, rowKey, label, selection, hasMore, firstRow, below, empty, loading, error }: Props<T>) {
  const shown = shownColumns(columns, visible);
  const span = shown.length + (selection ? 1 : 0);
  const pageIds = rows.map(rowKey);
  const pageSelected = pageIds.length > 0 && pageIds.every((id) => selection?.has(id));
  return (
    <div className="table-wrap" aria-busy={loading || undefined}>
      {selection && pageSelected && hasMore && !selection.allMatching && (
        <p className="table-note">
          Every row on this page is selected.{" "}
          <button type="button" className="link-button" onClick={selection.selectAllMatching}>
            Select all that match the filter
          </button>
        </p>
      )}
      <table className={`table ${loading ? "is-loading" : ""}`} aria-label={label}>
        <thead>
          <tr>
            {selection && (
              <th className="table__check">
                <input
                  type="checkbox"
                  aria-label="Select every row on this page"
                  checked={pageSelected}
                  onChange={(event) => selection.setPage(pageIds, event.target.checked)}
                />
              </th>
            )}
            {shown.map((column) => (
              <th key={column.key} scope="col" className={column.className}>
                {column.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {firstRow}
          {rows.map((row) => {
            const id = rowKey(row);
            const extra = below?.(row);
            return (
              <Fragment key={id}>
                <tr className={selection?.has(id) ? "is-selected" : undefined}>
                  {selection && (
                    <td className="table__check">
                      <input
                        type="checkbox"
                        aria-label="Select this row"
                        checked={selection.has(id)}
                        onChange={() => selection.toggle(id)}
                      />
                    </td>
                  )}
                  {shown.map((column) => (
                    <td key={column.key} className={column.className} data-label={column.label}>
                      {column.render(row)}
                    </td>
                  ))}
                </tr>
                {extra && (
                  <tr className="table__below">
                    <td colSpan={span}>{extra}</td>
                  </tr>
                )}
              </Fragment>
            );
          })}
          {!loading && !error && rows.length === 0 && (
            <tr>
              <td colSpan={span} className="table__empty">
                {empty}
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
