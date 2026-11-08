// Every product, for filters and the issue panel: few enough to load at once. `loaded` says the
// list is complete, so a filter naming a product that is gone can be told apart from one still
// loading.

import { useEffect, useState } from "react";
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
