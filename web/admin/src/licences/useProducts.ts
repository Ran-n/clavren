// Every product, for filters and the issue panel: few enough to load at once. `loaded` says the
// list is complete, so a filter naming a product that is gone can be told apart from one still
// loading.

import { useEffect, useRef, useState } from "react";
import { get } from "../shared/api";
import type { Page, Product } from "../shared/types";

export function useProducts(): { products: Product[]; loaded: boolean; error: unknown } {
  const [products, setProducts] = useState<Product[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    let current = true;
    async function load() {
      const all: Product[] = [];
      let after: string | null = null;
      do {
        const page: Page<Product> = await get(`/products?limit=100${after ? `&after=${encodeURIComponent(after)}` : ""}`);
        all.push(...page.data);
        after = page.next_cursor;
      } while (after);
      if (current) setProducts(all);
    }
    load().catch((failure: unknown) => current && setError(failure));
    return () => {
      current = false;
    };
  }, []);
  return { products: products ?? [], loaded: products !== null, error };
}

/**
 * A remembered filter naming a product that has been deleted would hide everything: once the
 * products are in (and whenever they change), `drop` gets the names among `names` whose value in
 * `filters` is no product's ID, set to "".
 */
export function useDropDeletedProducts(
  filters: Record<string, string> | undefined,
  names: readonly string[],
  { products, loaded }: { products: Product[]; loaded: boolean },
  drop: (changes: Record<string, string>) => void,
): void {
  // the latest values, read when the products change: the check runs only then
  const latest = useRef({ filters, names, drop });
  latest.current = { filters, names, drop };
  useEffect(() => {
    if (!loaded) return;
    const { filters: current, names: checked, drop: dropping } = latest.current;
    const gone = checked.filter((name) => {
      const id = current?.[name];
      return id && !products.some((product) => product.id === id);
    });
    if (gone.length > 0) dropping(Object.fromEntries(gone.map((name) => [name, ""])));
  }, [loaded, products]);
}
