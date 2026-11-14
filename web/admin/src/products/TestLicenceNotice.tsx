// The notice after a product is created with a test licence: its key to copy, a link to the
// licence, and a dismiss.

import { Button } from "../shared/components/Button";
import { CopyButton } from "../shared/components/Copy";
import { Notice } from "../shared/components/Notice";
import { Link } from "../shared/router";
import type { CreatedProduct } from "./NewProductRow";

export function TestLicenceNotice({ product, onDismiss }: { product: CreatedProduct; onDismiss: () => void }) {
  const licence = product.test_licence;
  if (!licence) return null;
  return (
    <Notice tone="success">
      <span>
        {product.name} was created with a test licence: <code className="key">{licence.key}</code>
      </span>
      <span className="button-row">
        <CopyButton label="Copy key" text={() => Promise.resolve(licence.key)} />
        <Link to={`/licences/${licence.id}`} className="button button--quiet button--small">
          Open it
        </Link>
        <Button size="small" variant="quiet" onClick={onDismiss}>
          Dismiss
        </Button>
      </span>
    </Notice>
  );
}
