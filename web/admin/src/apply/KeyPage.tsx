// A product's "how to get a key" page at `/apply?product=<code>`: the administrator's
// instructions, and the application form when the product takes applications. Applications link
// to it from their key prompt (`Client::get_key_url()`), so its text changes for every installed
// copy at once. The instructions are plain text: line breaks are kept and `https://` addresses
// become links, built as React elements so the text can never add markup.

import { Fragment, useEffect, useState, type ReactNode } from "react";
import { Loading } from "../shared/components/Notice";
import { ApplyForm } from "./ApplyForm";
import { explain, loadKeyPage, PublicError, type KeyPage as KeyPageData } from "./publicApi";

const LINK = /(https:\/\/\S+)/g;
/** Punctuation that ends a sentence rather than the address. */
const TRAILING = /[.,;:!?)\]]+$/;

/** `text` with its `https://` addresses as links that open in a new tab. */
export function linkify(text: string): ReactNode[] {
  return text.split(LINK).map((part, index) => {
    if (index % 2 === 0) return <Fragment key={index}>{part}</Fragment>;
    const trailing = TRAILING.exec(part)?.[0] ?? "";
    const address = part.slice(0, part.length - trailing.length);
    return (
      <Fragment key={index}>
        <a href={address} target="_blank" rel="noopener noreferrer">
          {address}
        </a>
        {trailing}
      </Fragment>
    );
  });
}

function headingFor(page: KeyPageData | null): string {
  return page ? `Get a licence key for ${page.name}` : "Get a licence key";
}

export function KeyPage({ code, onSent }: { code: string; onSent: (secret: string) => void }) {
  const [page, setPage] = useState<KeyPageData | null>(null);
  const [loadError, setLoadError] = useState<unknown>(null);

  useEffect(() => {
    loadKeyPage(code).then(setPage, setLoadError);
  }, [code]);

  useEffect(() => {
    document.title = headingFor(page);
  }, [page]);

  if (loadError instanceof PublicError && loadError.code === "product_unavailable") {
    return (
      <>
        <h1>{headingFor(null)}</h1>
        <p className="notice notice--info">This product isn't available.</p>
      </>
    );
  }
  if (loadError) {
    return (
      <>
        <h1>{headingFor(null)}</h1>
        <p className="notice notice--error" role="alert">
          {explain(loadError)}
        </p>
      </>
    );
  }
  if (!page) return <Loading label="Loading…" />;
  return (
    <>
      <h1>{headingFor(page)}</h1>
      {page.description && <p className="apply-key-page__description">{page.description}</p>}
      {page.key_instructions && <p className="apply-key-page__instructions">{linkify(page.key_instructions)}</p>}
      {page.accepts_applications ? (
        <section className="apply-key-page__form" aria-labelledby="apply-heading">
          <h2 id="apply-heading">Apply for a licence</h2>
          <ApplyForm product={page.code} onSent={onSent} />
        </section>
      ) : (
        !page.key_instructions && <p className="notice notice--info">Ask the seller of {page.name} for a licence key.</p>
      )}
    </>
  );
}
