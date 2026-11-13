[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/03 16:07:10.331288 )
[//]: # (+ Revised: 	2026/11/13 08:31:35.213277 )
[//]: # ( ---------------------------------------------------------------------- )

# TODO

The backlog. Each item is self-contained: reading it alone is enough to
implement it fully, without asking anything. Work top to bottom unless an item
says otherwise.

Next item number: 24

## Index

18. [Take payments for keys through Stripe Checkout with Managed Payments](#18-take-payments-for-keys-through-stripe-checkout-with-managed-payments) (on hold)
19. [Add the public buy page and the purchase status page](#19-add-the-public-buy-page-and-the-purchase-status-page) (on hold)
20. [Show purchases in the dashboard and handle refunds](#20-show-purchases-in-the-dashboard-and-handle-refunds) (on hold)
21. [Let the application buy a key](#21-let-the-application-buy-a-key) (on hold)
22. [Take Bitcoin into a watch-only wallet checked through public explorers](#22-take-bitcoin-into-a-watch-only-wallet-checked-through-public-explorers) (on hold)
23. [Take Monero into a view-only wallet on a public node](#23-take-monero-into-a-view-only-wallet-on-a-public-node) (on hold)

Items 18 to 23 (payments) are on hold until the administrator decides how to take payments;
"do the next todo" skips them.

## Adding an item

- Write for someone who has never seen the conversation: what happens today,
  what should happen instead, and why each non-obvious decision was made.
- Be concrete: real file paths, names, sizes, colours, keys. No open "maybe";
  if the implementer must choose, say how to choose and how to check.
- Use the template below. Implementation is a numbered list of steps in the
  order to do them; Done when is a checklist.
- Number it with "Next item number" above, then increase that number.
- File order is priority: the first item is the next one to do.
- Numbers ascend in file order. Deleting leaves a gap; when adding at the top
  or moving an item breaks the order, renumber every item in file order (from
  the lowest number in use) and update every reference to them.
- Keep the Index in file order: one `<n>. [title](#anchor)` line per item;
  update it whenever an item is added, retitled, moved or deleted.
- Screenshots go in `docs/todo/<n>-<slug>.png`, linked from the item.
- Split anything bigger than one sitting into items that ship alone.

## Implementing an item

1. Read the whole item and every file it names; check `git status` first.
2. If the code has drifted from the item, adapt and note it; if the item
   contradicts a newer decision, ask before building.
3. Do the Implementation steps in order.
4. Write the listed tests, plus any others the change needs.
5. Lint, format, update file headers and run the full test suite until clean.
6. For visible changes, run the app and check it in the states the item names.
7. Update the listed docs; write the changelog line when committing.
8. Tick every Done when line, with evidence.
9. Delete the item, its Index line and its screenshots; add follow-up work as
   new items.

## Template

```markdown
## <n>. <short imperative title>

**Why:** what happens today and what is wrong with it.
**Goal:** the result, as the user will see it.

### Behaviour
- One bullet per case, edge cases included.

### Out of scope
- What this item deliberately doesn't change.

### Implementation
1. `<path>`: what changes and why.

### Tests
- `<test file>`: each case and what it proves.

### Docs
- `<doc file>`: what to change. Changelog: "<line>".

### Done when
- [ ] Checkable result.
```

---

## 18. Take payments for keys through Stripe Checkout with Managed Payments

**On hold (2026-11-13):** the administrator wants to settle how to take payments before any of
items 18 to 23 is built. Don't implement this item, and skip it for "do the next todo", until they
lift the hold; keep refining it on request.

**Why:** second step of "charge per key" (decided by the administrator 2026-11-12: record prices
first, then sell online). Do it after item 17, which gives products a price and licences a
recorded price. Today a buyer must pay somewhere else and wait for the administrator to issue
the key by hand. Stripe Checkout is the provider chosen for this item: it hosts the card page (so
Clavren never sees card numbers and needs no PCI work), works in the EU and in euros, and needs
only two HTTPS calls and a webhook, which fits a server on a Raspberry Pi.
Payments go through **providers behind one interface**: Stripe (cards) here, Bitcoin and
Monero watched by Clavren itself in items 22 and 23 (decided by the administrator 2026-11-12,
"alongside cards"; the BTCPay Server plan was replaced 2026-11-13 by wallets watched through
public nodes, so no extra server is needed).
Purchases, fulfilment, the status endpoint and the tables are written once for every provider,
so item 22 adds a provider without touching them.
**Stripe is the Merchant of Record** through Stripe Managed Payments (decided 2026-11-13: the
administrator sells as a hobby and wants no VAT registration or returns; Managed Payments was
chosen over Paddle, Polar, Lemon Squeezy and Gumroad). Stripe, through Link, is the legal seller
to the buyer: it charges and files VAT and sales tax in every country, sends the receipts, and
handles refunds requests and disputes; the administrator only declares the payouts as income.
Why Managed Payments over the others: it is the cheapest (about 3.5% on top of Stripe's card
fee, roughly 5% + €0.25 on an EU card), it is one parameter on an ordinary Checkout Session, so
the rest of this item is plain Stripe Checkout; Paddle reviews sellers strictly and leans to
businesses, Polar costs more on non-US cards, Lemon Squeezy is being folded into Managed
Payments, Gumroad takes 10%. Stripe's docs: `docs.stripe.com/payments/managed-payments`
(eligibility, `…/update-checkout`, `…/how-it-works`). The `PaymentProvider` interface keeps
another MoR possible if Stripe turns the account down.
This item is the server side only (API, Stripe, issuing); the public pages are item 19, the
dashboard's Purchases page and refunds item 20, buying from inside the application item 21.
**Goal:** a product marked **Sells online** can be bought through the public API: Clavren
creates a Stripe Checkout page for one to ten keys at the product's price, Stripe tells Clavren
when the payment went through, and Clavren issues the keys to a customer made from the buyer's
name and email, with the price recorded. The buyer collects the keys with a secret, as an
applicant collects an approved key.

### Behaviour
- **Turned on by configuration:** `CLAVREN_STRIPE_SECRET_KEY` (`sk_live_…`, `sk_test_…` or a
  restricted `rk_…` key) and `CLAVREN_STRIPE_WEBHOOK_SECRET` (`whsec_…`), both or neither (a
  single one fails startup naming the missing variable, as the Turnstile keys do). Without them
  nothing can be bought and products can't be marked Sells online.
  `CLAVREN_STRIPE_API_URL` (default `https://api.stripe.com`; `https://` required except a
  loopback address, so tests can point at a fake) is for tests only.
- **Products:** a new `sells_online` flag (off by default), settable only when the product has a
  price (item 17) and Stripe is configured; otherwise 409 `product_not_sellable` "set a price
  and the Stripe keys first". Clearing the price of a selling product turns `sells_online` off in
  the same update, recorded in the audit's changed fields. A free product (amount 0) can't sell
  online (Stripe takes no zero-amount payment): the same 409.
- **`POST /api/v1/purchases`** `{"product": "<code>", "quantity": 1, "method": "card"}`
  (quantity 1 to 10; more is a hand sale, as one buyer rarely needs more; `method` optional,
  default `card`, items 22 and 23 add `bitcoin` and `monero`; a method that isn't configured,
  or isn't known, →
  404 `product_unavailable`): for an active product that sells online, creates a purchase and
  asks the method's provider for a checkout. For `card`, a Stripe Checkout Session (`mode=payment`, one line item with
  `price_data[currency]` (lowercase), `price_data[unit_amount]`, `price_data[product_data][name]`
  = product name, `price_data[product_data][tax_code]=txcd_10202000` (Downloadable Software -
  personal use; Managed Payments refuses products without an eligible digital tax code),
  `price_data[tax_behavior]=inclusive`, `quantity`; `managed_payments[enabled]=true`;
  `client_reference_id` and `metadata[purchase]` = purchase ID;
  `success_url` = `<public url>/buy/status#<secret>`; `cancel_url` =
  `<public url>/buy?product=<code>`; `Idempotency-Key` header = purchase ID; `Stripe-Version`
  header pinned to `2026-04-22.dahlia`, the release that added `managed_payments`, so a change
  of the account's default API version can't break Clavren). Managed Payments manages and
  refuses `automatic_tax`, `tax_id_collection`, `invoice_creation`, `payment_method_types`,
  `customer_update[…]`, `shipping_…` and `payment_intent_data.receipt_email`: none is sent. Answers 201
  `{"secret": "<base64url 256-bit>", "checkout_url": "<Stripe's url>"}`. Only the BLAKE3 hash of
  the secret is stored, as for applications.
  - Unknown, inactive or not-selling product → 404 `product_unavailable` (one answer for all
    three, as `invalid_key` never tells products apart).
  - Stripe not configured → 404 `product_unavailable` as well.
  - Stripe unreachable or answering 5xx → 503 `payments_unavailable` "payments can't be taken
    right now; try again in a few minutes"; no purchase is kept (the insert is rolled back).
  - Stripe refuses the amount (`amount_too_small`) → 409 `price_too_small`, logged at `error`
    with the product ID so the administrator fixes the price.
  - Managed Payments not active on the account (not yet approved, terms not accepted, or
    turned off: Stripe answers 400 naming `managed_payments`) → 503 `payments_unavailable`,
    logged at `error` with Stripe's message so the administrator finishes the setup in
    `docs/deployment.md`; no purchase is kept. Clavren never falls back to a session without
    `managed_payments`, which would make the administrator the seller without them knowing.
  - Rate limit: 10 purchase starts an hour per client address (a new `purchases`
    `FailureLimiter` in `http/mod.rs`, refused attempts counted), then 429 with `Retry-After`.
- **`POST /api/v1/payments/stripe`** (the webhook; raw body): the `Stripe-Signature` header
  (`t=<unix time>,v1=<hex>`) must hold a `v1` equal to HMAC-SHA256 of `"<t>.<raw body>"` under the
  webhook secret (compared in constant time), and `t` within 5 minutes of the server clock;
  otherwise 400 and a `warn` log. SHA-256 here is dictated by Stripe, not a choice (the BLAKE3
  default doesn't apply).
  - `checkout.session.completed` with `payment_status` `paid`, or
    `checkout.session.async_payment_succeeded`: fulfil, in one transaction: lock the purchase by
    its provider and checkout reference (`stripe`, session ID); already paid → answer 200 and do nothing (Stripe resends events); else
    choose the customer (the existing customer whose email matches
    `customer_details.email` ignoring case, else a new one from `customer_details.name` and
    email; a missing name uses the email), issue `quantity` licences at the purchase's unit
    price, set `paid_at`, `provider_payment` (the payment intent) and `customer_id`, audit `purchase.paid`, and
    commit, then answer 200.
  - The product was deactivated after the checkout started: the keys are still issued (the
    buyer has paid). The product was deleted: the purchase is marked paid with no licences, an
    `error` log names the purchase, and item 20's page shows it for a refund by hand.
  - `checkout.session.completed` with `payment_status` `unpaid` (a bank transfer still on its
    way): nothing changes; the later `async_payment_succeeded` fulfils it.
  - `checkout.session.expired` or `checkout.session.async_payment_failed`: set `ended_at`, audit
    `purchase.ended`.
  - Any other event, or a session without `metadata[purchase]` (not made by Clavren): 200,
    logged at `debug`.
  - Database failure: 500 so Stripe retries (it does for three days).
- **`POST /api/v1/purchases/status`** `{"secret": "…"}`: `{"state": "pending" | "paid" |
  "ended", "product": {"code", "name"} | null, "quantity", "keys": [...]}`; `keys` only when
  paid, those whose licence still has a key (revoked or deleted ones are left out, as a withdrawn
  application shows none). Unknown secret → 404 `purchase_unknown`. Same rate limit as
  `applications/status` (a second `purchase_status` limiter with the same numbers).
- Pending purchases that were never paid are deleted by the hourly housekeeping 48 hours after
  they started (Stripe ends a session after 24 hours, so by then it can't be paid).
- A purchase's state is derived (pending: no `paid_at` and no `ended_at`; paid; ended), never
  stored, as for licences.
- **Tax and the seller:** the product's price includes tax (`tax_behavior=inclusive`), so the
  buyer pays exactly the price on the buy page, as EU law requires prices shown to consumers to
  include VAT; Stripe takes the tax out of it and pays it. Clavren records no tax: the licence's
  recorded price (item 17) is what the buyer paid, tax included. Stripe collects the buyer's
  address, works out and files the tax, and emails the receipt from Link (the statement reads
  `LINK.COM* <descriptor>`); tax records live in Stripe, so deleting a customer in Clavren
  (`docs/spec.md` §7) erases no tax record.

### Out of scope
- The `/buy` and `/buy/status` pages and the Products page toggle: item 19. The dashboard's
  Purchases page and refunds: item 20. Buying from the client crate: item 21.
- Bitcoin and Monero: items 22 and 23. Other providers (PayPal, a hosted crypto processor), coupons,
  subscriptions.
- Any tax handling in Clavren (rates, VAT numbers, invoices, reports): Stripe does it as the
  Merchant of Record. Selling without Managed Payments (the administrator as seller).
- Emailing the keys: Clavren sends no email (`docs/spec.md` §1); the buyer collects them from the
  status page, and Stripe's own receipt email can be turned on in the Stripe dashboard.

### Implementation
1. `docs/spec.md` §1: move "Selling: payments, …" out of "Out of scope": selling online through
   Stripe Checkout with Stripe Managed Payments as Merchant of Record (cards) and, with items 22 and 23, Bitcoin and Monero into the administrator's own wallets, is now in
   scope; shops, carts, subscriptions and other providers stay out.
   Do this first, so the code and the spec agree from the start.
2. `crates/clavren-server/Cargo.toml`: add `hmac` and `sha2` (current versions) for the webhook
   signature.
3. `migrations/<next timestamp>_purchases.up.sql` / `.down.sql`: `products.sells_online BOOLEAN
   NOT NULL DEFAULT FALSE` with `CHECK (NOT sells_online OR price_amount > 0)`; table `purchases`
   (`id` UUID PK, `product_id` UUID NULL REFERENCES products ON DELETE SET NULL (a payment record
   outlives its product), `quantity` INT CHECK 1..10, `unit_amount` BIGINT, `currency` TEXT
   (same CHECKs as item 17), `secret_hash` BYTEA UNIQUE length 32, `provider` TEXT NOT NULL
   CHECK (provider IN ('stripe')) (items 22 and 23 widen it), `provider_checkout` TEXT NULL
   (Stripe's session ID; for Bitcoin and Monero, the payment address), `provider_payment` TEXT NULL (Stripe's payment intent),
   UNIQUE (provider, provider_checkout), UNIQUE (provider, provider_payment), `customer_id` UUID NULL REFERENCES
   customers ON DELETE SET NULL, `created_at`, `paid_at` NULL, `ended_at` NULL, CHECK not both
   `paid_at` and `ended_at`); `licenses.purchase_id` UUID NULL REFERENCES purchases ON DELETE SET
   NULL with an index. No buyer name or email on `purchases`: they live on the customer, so
   deleting the customer still erases them (`docs/spec.md` §7 customers).
4. `crates/clavren-server/src/config/mod.rs`, `config/parse.rs`: `PaymentSettings { stripe:
   Option<StripeSettings { secret_key, webhook_secret, api_url }> }` on `Config` (items 22 and 23 add
   `bitcoin` and `monero` beside it), parsed like
   `turnstile` (both or neither, trimmed, `https://` rule). `deploy/clavren.env.example`: the
   two variables, commented, with a line on where to find them in the Stripe dashboard.
5. `crates/clavren-server/src/audit/mod.rs`: `Actor.admin_id` becomes `Option<Uuid>` with
   `Actor::system(ip, at)` for work no admin did (the webhook); `AuditEntry::by` passes it
   through (the column is already nullable). Update every use: `admin_auth/guard.rs`,
   `admin_preferences/mod.rs`, `audit/mod.rs`, `tests/admin_console.rs` (14 uses today; grep
   `\.admin_id` again before changing).
6. `crates/clavren-server/src/licences/issue.rs`: move the `is_active` check out of `issue_in`
   into `issue` and the approval path, so fulfilment can issue for a product deactivated after
   payment; add `purchase_id: Option<Uuid>` to `Batch` and `NewLicences`.
7. `crates/clavren-server/src/purchases/` (new feature module, as `applications/`): `mod.rs`
   (types, `Method` enum `Card` (items 22 and 23 add `Bitcoin` and `Monero`), derived state), `providers/mod.rs`
   (the `PaymentProvider` trait, Strategy pattern: `start(&Checkout) -> Result<Started {
   checkout_reference, checkout_url }, ProviderError>` with `ProviderError::{Unavailable,
   AmountTooSmall}`, and `event(&HeaderMap, &Bytes, now) -> Result<Option<ProviderEvent>,
   ProviderError>` (async, so a provider may ask its API to confirm), where `ProviderEvent {
   checkout_reference, kind: Paid { payment_reference, buyer: Option<Buyer { name, email }> } |
   Ended }` (item 22 adds `Seen`; a provider may have no webhook at all: items 22 and 23 watch
   wallets from a background task and hand the same `ProviderEvent`s to `fulfil`); `ProviderError::BadSignature` answers 400), `providers/stripe.rs` (the
   `Stripe` implementation on `ureq` inside `spawn_blocking`, timeout 10 s, built like
   `applications/human_check.rs` `Turnstile`), `providers/stripe_signature.rs` (header parsing
   and HMAC check, pure functions), `start.rs`, `fulfil.rs` (provider-neutral: takes a
   `ProviderEvent`), `repository.rs`, `public_routes.rs` (`/purchases`, `/purchases/status`, and
   one webhook route per provider, `/payments/<provider>`, nested under `/api/v1` in
   `http/mod.rs` with `no_store()`; webhook routes read the body as `Bytes` so signatures are
   checked over the exact bytes). `AppState` gets `payments: PaymentProviders` (the configured
   providers by `Method`, each an `Arc<dyn PaymentProvider>`) so tests inject fakes.
8. `crates/clavren-server/src/products/mod.rs`: `sells_online` on `Product`, `NewProduct`,
   `ProductChanges` and `changed_fields`, with the 409 rule and the price-cleared rule.
9. `crates/clavren-server/src/maintenance.rs`: delete pending purchases older than 48 hours.
10. `crates/clavren-server/src/http/error.rs`: the codes `product_unavailable`,
    `payments_unavailable`, `price_too_small`, `purchase_unknown`, `product_not_sellable`.
11. Logging (`tracing`, target `clavren::purchases`): `info` on start (purchase and product IDs,
    quantity), on paid (IDs, licence count), on ended; `warn` on a bad signature; `error` on a
    paid purchase that couldn't be fulfilled. Never the secret, keys, names or emails. Every
    Stripe error is surfaced (logged and answered), never swallowed.
12. `metadata -c` on new files, `metadata -m` on edited ones.

### Tests
- `purchases/providers/stripe_signature.rs` unit tests: a valid header (Stripe's documented example computed in
  the test), a wrong secret, a changed body, a timestamp 6 minutes old, a header with two `v1`
  values where the second is right, a malformed header.
- `crates/clavren-server/tests/purchases.rs` (new), against the real database and a fake
  `PaymentProvider`: `starting_a_purchase_creates_a_session_with_the_price` (amount, currency lowercase,
  quantity, URLs with the secret's fragment, idempotency key), `unknown_inactive_and_not_selling_products_read_the_same`,
  `stripe_down_keeps_no_purchase` (503), `the_amount_stripe_refuses_is_409`,
  `starts_are_rate_limited`, `an_unknown_or_unconfigured_method_reads_unavailable`,
  `a_signed_completed_event_issues_the_keys` (customer made from
  Stripe's name and email, price recorded, `purchase_id` set, audit `purchase.paid` with no
  admin), `the_same_event_twice_issues_once`, `an_existing_customers_email_is_reused`,
  `an_unpaid_completion_waits_for_async_success`, `an_expired_session_ends_the_purchase`,
  `a_bad_signature_is_400_and_changes_nothing`, `a_deactivated_product_still_gets_its_keys`,
  `a_deleted_product_marks_paid_without_licences`, `status_shows_keys_only_when_paid`,
  `revoked_keys_leave_the_status`, `housekeeping_deletes_old_pending_purchases`,
  `the_session_uses_managed_payments` (`managed_payments[enabled]=true`, the tax code,
  `tax_behavior=inclusive`, the pinned `Stripe-Version`, and none of the refused parameters),
  `managed_payments_not_active_is_503_and_keeps_no_purchase`.
- `crates/clavren-server/tests/products.rs`: `sells_online_needs_a_price_and_stripe`,
  `clearing_the_price_stops_selling`.
- `config/tests.rs`: both Stripe keys or neither; the `https://` rule for the API URL.
- `tests/schema.rs`, `tests/migrations.rs`: the new table's constraints and the migration both
  ways; the BCNF row.
- Manual, in Stripe test mode with `stripe listen --forward-to <url>/api/v1/payments/stripe`:
  start a purchase with `curl`, pay with card `4242 4242 4242 4242`, and confirm the keys appear
  at the status endpoint. Confirm Stripe's page shows the seller as Link, the total equals the
  product's price (tax inside it), and the payment in the Stripe dashboard is marked as Managed
  Payments. Then confirm the `charge.refunded` and `charge.dispute.created` events reach the
  webhook for a Managed Payments payment (item 20 relies on them; Stripe's Managed Payments docs
  don't say), and write the answer into item 20. Also confirm the browser lands on `/buy/status` **with** the `#secret`
  fragment after paying; if Stripe drops the fragment, write it down in the item 19 notes: item
  19 then keeps the secret in `sessionStorage` under `clavren.purchase` before redirecting.

### Docs
- `docs/spec.md`: §1 (step 1), §2 a "Buying a key" flow, §3 Purchase concept, §6 the three
  public routes, §7 `purchases` and the new columns, §12 housekeeping.
- `docs/api.md`: the public routes, the webhook (what it checks), the error codes, products'
  `sells_online`.
- `docs/deployment.md`: "Selling online": create the Stripe keys, add the webhook endpoint
  `https://<public url>/api/v1/payments/stripe` with the events `checkout.session.completed`,
  `checkout.session.async_payment_succeeded`, `checkout.session.async_payment_failed`,
  `checkout.session.expired` (and `charge.refunded` for item 20), set the two variables,
  restart; the Cloudflare Tunnel already exposes the path. Before the keys: "Managed Payments": the
  business must be in a supported country (`docs.stripe.com/payments/managed-payments/eligibility`),
  turn it on at `dashboard.stripe.com/settings/managed-payments`, accept its terms and wait for
  Stripe's eligibility review; add the terms of sale and privacy policy links in Checkout
  settings. Stripe is then the seller and handles tax; the administrator declares the payouts as
  income. Fees: about 3.5% on top of the card fee (Stripe's pricing page has the current rate).
- `docs/design-decisions.md`: "Selling through Stripe Checkout" (hosted page, no card data,
  secret instead of account, idempotent fulfilment, payment records outlive products, no buyer
  data outside customers, SHA-256 because Stripe requires it), "Stripe Managed Payments as Merchant
  of Record" (no tax work for a hobby seller; chosen over Stripe Tax, Paddle, Polar, Lemon Squeezy
  and Gumroad for fee and fit; prices include tax; no fallback to selling unmanaged) and the BCNF
  rows.
- Changelog: "Sell keys online: products marked Sells online can be bought through Stripe
  Checkout, with Stripe as the Merchant of Record handling tax, and the paid keys are issued by themselves to a
  customer made from the buyer."

### Done when
- [ ] In Stripe test mode, a purchase started with `curl` and paid with the test card issues the
      keys, with the price and a customer, and the status endpoint returns them.
- [ ] The same webhook event sent twice issues the keys once.
- [ ] A bad signature is refused with 400 and nothing changes.
- [ ] The test payment is marked Managed Payments in Stripe, sold through Link, with the total
      equal to the product's price; whether refund and dispute events reach the webhook is
      written into item 20.
- [ ] Spec §1 no longer lists online selling as out of scope.
- [ ] `cargo test`, clippy and the coverage gate pass.

## 19. Add the public buy page and the purchase status page

**On hold (2026-11-13):** the administrator wants to settle how to take payments before any of
items 18 to 23 is built. Don't implement this item, and skip it for "do the next todo", until they
lift the hold; keep refining it on request.

**Why:** after item 18 (the server side of selling) and item 14 (which makes
`/apply?product=<code>` each product's "how to get a key" page). Item 18 lets a purchase be made
through the API only; a buyer needs a page to click. The administrator also needs a way to turn
selling on for a product.
**Goal:** a product that sells online shows **Buy a key — €19.99** on its key page; the buy page
`/buy?product=<code>` asks how many keys, sends the buyer to Stripe, and after paying the buyer
lands on `/buy/status#<secret>`, which shows the keys with **Copy**. On the Products page the
administrator turns **Sells online** on and copies the buy link.

### Behaviour
- **Products page, Applications column becomes "Getting a key"** with both switches: the
  existing Open/Close applications, and **Sell online** / **Stop selling**. Sell online is
  disabled (dimmed, with a tooltip) when the product has no price ("Set a price first"), a free
  price ("Free keys can't be sold online") or the server has no payment provider ("Add the Stripe
  keys or a Bitcoin or Monero wallet to the server first: see Deployment"). While selling, **Copy buy link** copies
  `<public url>/buy?product=<code>`. Stop selling turns into an inline confirmation strip ("The
  buy page stops taking payments; paid keys keep working"). The server tells the dashboard
  which payment methods are configured through a new `payment_methods` list on `GET
  /api/admin/overview` (`["card"]` with Stripe; items 22 and 23 add `"bitcoin"` and `"monero"`;
  no new endpoint).
- **Key page** (`/apply?product=<code>`, item 14): when the product sells online, a primary
  **Buy a key — <price>** button on its own row above everything else, linking to
  `/buy?product=<code>`; the application form, when open, stays below as the secondary way.
- **`/buy?product=<code>`**: heading "Buy <product name>", the description, the price per key,
  a quantity field (1 to 10, default 1) with the total updating as it changes ("Total: €39.98",
  with "Tax included" under it in small text, since the price includes tax: item 18), a line
  "Sold through Link, Stripe's checkout, which handles tax and receipts." under the button, so
  the buyer isn't surprised by the seller name on Stripe's page and their statement,
  and a primary **Pay with card** button. Pressing it shows a loading state, calls
  `POST /api/v1/purchases`, and moves the browser to the returned `checkout_url`. Errors as plain
  English notices: unavailable product "This product can't be bought here.", 503 "Payments
  can't be taken right now; try again in a few minutes.", 429 "Too many attempts from your
  connection; try again in N minutes." Unknown, inactive or not-selling product on load: "This
  product can't be bought here." with no form.
- **`/buy/status#<secret>`**: reads the secret from the fragment, asks `POST
  /api/v1/purchases/status`, and shows: pending → "Waiting for your payment to be confirmed…",
  asking again every 5 seconds for 2 minutes, then every minute (Stripe usually confirms within
  seconds; bank payments take days); paid → "Your keys for <product>", each key with **Copy**
  and **Copy all**, and "Bookmark this page: it is the only place your keys are shown."; ended →
  "This payment didn't go through. Nothing was charged." with a link back to the buy page;
  unknown secret → "This purchase can't be found. Check the link." The secret never leaves the
  fragment (no query string, `Referrer-Policy: no-referrer` as `/apply/status`).
- Cancelling on Stripe's page returns to `/buy?product=<code>` with the quantity reset.
- Both pages follow the system light or dark theme as `/apply` does, and work at phone width.

### Out of scope
- A shop listing every product: each product's link is shared on its own.
- Changing the price on the buy page, coupons, a tax breakdown (Stripe's page and receipt show
  it once the buyer's country is known).

### Implementation
1. `web/admin/buy.html` (new entry, like `apply.html`) and `web/admin/src/buy/` (new):
   `main.tsx`, `BuyForm.tsx`, `PurchaseStatus.tsx`, `buyApi.ts` (the two public calls),
   reusing `apply/apply.css` styles by importing them (no copied CSS) and
   `shared/format.ts` `formatPrice` from item 17.
2. `web/admin/vite.config.ts`: `buy: "buy.html"` in `rollupOptions.input`, and the dev plugin
   (rename `applyPages` to `publicPages`) routing `/buy` and `/buy/*` to `/buy.html`.
3. `crates/clavren-server/src/dashboard.rs`: serve `buy.html` for `/buy` and the paths under it,
   with the same headers as `apply.html` (no-referrer included). Check `build.rs` embeds the new
   file without changes.
4. `GET /api/v1/purchases/product?code=<code>` (in `purchases/public_routes.rs`): the name,
   description and price of a product that sells online, else 404 `product_unavailable`; the buy
   page loads it. Add it to item 14's key page data too (a `price` and `sells_online` on its
   product answer) so the button can show the price.
5. `web/admin/src/apply/` (item 14's key page): the Buy button.
6. `crates/clavren-server/src/overview.rs`: `payment_methods`.
7. `web/admin/src/products/ProductsPage.tsx`: the "Getting a key" column with both switches,
   the disabled reasons and **Copy buy link**.
8. Use the existing `Button`, `Notice`, `ConfirmStrip` and `CopyButton` components; no modal.
9. `metadata -c` / `-m` on the files.

### Tests
- `crates/clavren-server/tests/purchases.rs`: `the_buy_page_data_hides_products_not_for_sale`;
  `tests/dashboard.rs`: `buy_pages_are_served_with_no_referrer`.
- `web/admin/e2e/buy.spec.ts` (new), against the e2e server with the fake Stripe from item 18
  (add it to `examples/e2e_server.rs`; the fake's checkout URL is a local page that "pays" by
  sending a signed webhook): buy two keys and see both on the status page; cancel and come back;
  an ended purchase; an unknown secret; the 429 notice.
- `web/admin/e2e/catalogue.spec.ts`: Sell online disabled without a price, enabled with one,
  Copy buy link, Stop selling with its strip.
- `web/admin/e2e/rendered.spec.ts`: baselines for `/buy`, `/buy/status` pending, paid and
  ended, the key page with the Buy button, and the Products page column, desktop and phone,
  light and dark; review each by eye.

### Docs
- `docs/spec.md` §2 "Buying a key" (the pages), §8 Products column.
- `docs/api.md`: `GET /api/v1/purchases/product`, the Overview's `payment_methods`.
- `README.md`: one line in the feature list on selling keys online.
- Changelog: "A public buy page for each product that sells online, and a private page where the
  buyer collects their keys."

### Done when
- [ ] With Stripe test keys, Sell online on the Products page, open the copied buy link, buy two
      keys with the test card, land on the status page and copy both keys.
- [ ] The key page shows the Buy button with the price.
- [ ] Screenshots of every state, light and dark, reviewed and kept as baselines.
- [ ] The Playwright suite passes.

## 20. Show purchases in the dashboard and handle refunds

**On hold (2026-11-13):** the administrator wants to settle how to take payments before any of
items 18 to 23 is built. Don't implement this item, and skip it for "do the next todo", until they
lift the hold; keep refining it on request.

**Why:** after item 18. Once keys are sold online, the administrator needs to see every
purchase (who bought what, for how much, whether it was paid) and what happens when a payment is
refunded or disputed; today a refunded buyer would keep working keys, and a paid purchase whose
product was deleted (item 18) has nowhere to be seen.
**Goal:** a **Purchases** page lists every purchase with its state, and a refund made in Stripe
revokes the keys it paid for by itself.

### Behaviour
- **Purchases page** (new, in the menu after Applications): table newest first: started, paid,
  customer (link), product, quantity, unit price, total, state (Pending, Paid, Ended, Refunded,
  Disputed), keys (links to the licences), and **Open in Stripe** (link to
  `https://dashboard.stripe.com/payments/<payment intent>`, `/test/payments/…` with a test key).
  Filters by state (Paid by default) and product, search by customer name or email; remembered
  like every table (`admin_preferences`). A paid purchase with no licences shows a warning badge
  "Paid, no keys: refund it in Stripe".
- **Refund:** the webhook event `charge.refunded` for a payment intent Clavren knows: a full
  refund (`amount_refunded` = `amount`) sets `refunded_at` and revokes every licence of the
  purchase with reason "Refunded" (their computers lock at the next check, as any revocation);
  a partial refund sets `refunded_amount` and revokes nothing (which keys to take back is the
  administrator's call), shown as "Partly refunded: €X".
- **Dispute:** `charge.dispute.created` sets `disputed_at` and revokes nothing (a dispute can be
  won); the row shows "Disputed" with a warning tone. `charge.dispute.closed` with status `lost`
  revokes the keys with reason "Payment disputed".
- Overview: the Charged section (item 17) subtracts full and partial refunds and shows
  "Refunded: €X" per currency for the same two periods.
- Licence detail: a licence sold online links to its purchase.
- Refunding itself is done in Stripe, not in Clavren (one place for money). Under Managed
  Payments Stripe (Link) also answers buyers and may refund within 60 days without asking when a
  product request goes unanswered for 48 hours, and it fights disputes itself: so refunds and
  lost disputes can arrive without the administrator doing anything, which is why they are
  handled from webhook events. Item 18's manual test records whether `charge.refunded` and
  `charge.dispute.*` reach the webhook for Managed Payments payments; if they don't, use the
  events it found instead (`refund.created`, or reading the payment intent back on
  `checkout.session` events) and update this item before building it.

### Out of scope
- Starting refunds from the dashboard; editing purchases; exporting purchases (the Licences CSV
  already has the price).

### Implementation
1. `migrations/<next timestamp>_purchase_refunds.up.sql` / `.down.sql`: `purchases.refunded_at`,
   `refunded_amount BIGINT NULL CHECK (refunded_amount BETWEEN 0 AND quantity * unit_amount)`,
   `disputed_at`.
2. `crates/clavren-server/src/purchases/fulfil.rs` (or a new `refunds.rs`): the three events,
   each in one transaction, idempotent (a second identical event changes nothing), revoking
   through `licences::lifecycle` so the audit entries and locking match a revocation by hand;
   audit `purchase.refunded`, `purchase.disputed`, `purchase.dispute_lost`.
3. `crates/clavren-server/src/purchases/admin_routes.rs`: `GET /api/admin/purchases` (filters,
   search, cursor pagination, `?expand=customer,product,licences` per the expand pattern) and
   `GET /api/admin/purchases/{id}`.
4. `crates/clavren-server/src/overview.rs`: refunds in the Charged sums.
5. `web/admin/src/purchases/PurchasesPage.tsx` (new), the route in `shared/router.tsx`, the menu
   entry in `shell/Layout.tsx`; `licences/LicenceDetailPage.tsx` the purchase link.
6. `docs/deployment.md` webhook event list gains `charge.refunded`, `charge.dispute.created`,
   `charge.dispute.closed`.
7. Logging at `info` for each event handled; `metadata` on the files.

### Tests
- `crates/clavren-server/tests/purchases.rs`: `a_full_refund_revokes_the_keys`,
  `a_partial_refund_revokes_nothing`, `a_lost_dispute_revokes_the_keys`,
  `a_won_dispute_keeps_them`, `refund_events_twice_change_nothing`, `listing_filters_and_searches`.
- `tests/dashboard.rs`: the Charged sums minus refunds.
- `web/admin/e2e/purchases.spec.ts` (new): the table, filters, the warning badge, the licence
  link; baselines desktop and phone, light and dark, reviewed by eye.

### Docs
- `docs/spec.md` §3 Purchase states, §8 Purchases page, Overview; `docs/api.md` the admin
  routes and events; `docs/design-decisions.md` "Refunds revoke, partial refunds don't".
- Changelog: "A Purchases page in the dashboard; a full refund or a lost dispute in Stripe
  revokes the keys it paid for."

### Done when
- [ ] In Stripe test mode, refunding a paid purchase in full revokes its keys and the Purchases
      page shows Refunded; a partial refund shows the amount and keeps the keys.
- [ ] The Overview's Charged figures drop by the refunded amount.
- [ ] Tests and baselines pass.

## 21. Let the application buy a key

**On hold (2026-11-13):** the administrator wants to settle how to take payments before any of
items 18 to 23 is built. Don't implement this item, and skip it for "do the next todo", until they
lift the hold; keep refining it on request.

**Why:** after items 18 and 19. An application can already apply for a key from its key prompt
(`Client::apply` in `crates/clavren-client/src/applying.rs`) and pick up the approved key by
itself; a buyer today would have to leave the app, buy on the web page and type the key in.
**Goal:** the app's key prompt can offer **Buy a key**: the client crate starts a purchase,
opens Stripe's page in the browser, and activates the paid key by itself, as it does for an
approved application.

### Behaviour
- `client.buy(quantity, method)` (`PaymentMethod::Card`; the enum is `#[non_exhaustive]` so
  items 22 and 23 add `PaymentMethod::Bitcoin` and `PaymentMethod::Monero` without breaking
  callers) calls `POST /api/v1/purchases`
  for the client's product and returns
  `Bought { checkout_url }` for the app to open in the default browser; the secret is kept in the
  client's state file next to an application's secret (encrypted with DPAPI like the rest).
- `client.check_purchase()` asks `POST /api/v1/purchases/status`; when paid it activates the
  first key on this computer and forgets the secret; for a quantity above 1 the other keys are
  kept until the app reads them with `client.purchased_keys()` (to show them for the buyer's
  other computers), which also forgets them.
  The background checker asks every minute for 30 minutes after `buy`, then at the application
  interval (`Config::application_check_interval`, 15 minutes).
- New outcomes: `PurchasePending`, `PurchaseEnded` (the payment didn't go through), and
  `PurchaseUnavailable` (the product doesn't sell online); errors map like `apply`'s.
- `client.dismiss_purchase()` forgets a pending purchase.
- A purchase and an application can't both wait: `buy` while an application waits returns the
  existing `ApplicationPending` (and the reverse), so the prompt shows one state.

### Out of scope
- Paying inside the app (no embedded card form: the hosted Stripe page is the only place card
  data goes).
- Editing avalio-lite or any other app (project rule: only this repository).

### Implementation
1. `crates/clavren-protocol/src/wire.rs`: the purchase request and answer types, shared with
   the server (move them there from item 18's server module so both sides use one definition).
2. `crates/clavren-client/src/buying.rs` (new, beside `applying.rs`): `buy`, `check_purchase`,
   `dismiss_purchase`; `state.rs`: the stored purchase secret; `outcome.rs`: the new outcomes;
   `background.rs`: the faster schedule after `buy`; `lib.rs`: exports.
3. `crates/clavren-client/tests/fake_server/mod.rs`: the purchase routes.
4. `metadata` on the files.

### Tests
- `crates/clavren-client/tests/`: `buy_returns_the_checkout_url_and_keeps_the_secret`,
  `a_paid_purchase_activates_by_itself`, `an_ended_purchase_is_reported`,
  `buying_while_an_application_waits_is_refused`, `dismiss_forgets_it`, the state file
  surviving a restart with a pending purchase.
- `crates/clavren-client/tests/against_server.rs`: buy, pay through the server's fake Stripe,
  activate.

### Docs
- `docs/client-integration.md`: a "Buying a key" section after "Applying for a licence", with
  what the prompt shows for each outcome.
- `docs/spec.md` §2 "Buying a key": from the application.
- Changelog: "`clavren-client`: `Client::buy`, `Client::check_purchase`,
  `Client::dismiss_purchase` and the outcomes `PurchasePending`, `PurchaseEnded` and
  `PurchaseUnavailable`: an application can sell its own keys."

### Done when
- [ ] Against the real server with Stripe test keys, `buy`, pay in the browser, and the client
      reports `Licensed` within a minute without typing the key.
- [ ] Client tests pass, including `against_server.rs`.

## 22. Take Bitcoin into a watch-only wallet checked through public explorers

**On hold (2026-11-13):** the administrator wants to settle how to take payments before any of
items 18 to 23 is built. Don't implement this item, and skip it for "do the next todo", until they
lift the hold; keep refining it on request.

**Deferred (2026-11-12):** the administrator put Bitcoin and Monero off for the future. Don't
start it until they ask for it again.
**Conflicts with the Merchant of Record decision (2026-11-13, item 18):** card sales go through
Stripe Managed Payments so the administrator is never the seller and does no tax work. No
Merchant of Record takes Bitcoin or Monero: every coin sale makes the administrator the seller,
with VAT registration and returns for it. When this item is picked up again, raise that first
(AskUserQuestion: coins with the tax work, coins as donations with no key in return, or drop
coins) before building.

**Why:** the administrator wants to be paid in Bitcoin and Monero as well as by card (decided
2026-11-12). The first plan ran BTCPay Server on its own machine; on 2026-11-13 the administrator
chose instead to run **no node and no extra server**: the Pi 3B+ can't hold Bitcoin and Monero
nodes, a VPS costs about €5-15 a month, and the old HP Pavilion laptop may not be up to it.
Clavren watches the administrator's own wallets through public services instead. This item
builds the coin-neutral parts (the payment page, the watcher, exchange rates, payment issues)
and Bitcoin; item 23 adds Monero on top. Do it after items 18 and 19 (purchases, the
`PaymentProvider` interface, `/buy` and `/buy/status`).
**What "public explorers" means and why two:** a block explorer (mempool.space,
blockstream.info) runs a Bitcoin node and answers questions about it over HTTPS, such as "what
transactions paid this address". Clavren believes the answer without checking the blockchain
itself, so a wrong or lying explorer could report a payment that never happened (a key for free)
or hide one (a buyer kept waiting). Asking **two independent explorers and acting only when both
agree** makes that unlikely. Nobody can take the coins: Clavren holds only the wallet's public
key (xpub), which can derive addresses and see payments but can't spend. The cost is privacy:
both explorers see the addresses Clavren asks about and the Pi's IP address.
**Goal:** the buy page offers **Pay with Bitcoin** beside **Pay with card**. Clavren shows its
own payment page with a fresh address, the amount in BTC (the euro price converted at the
current rate) and a QR code; once the payment has one confirmation on both explorers, Clavren
issues the keys exactly as for a card payment.

### Behaviour
- **Turned on by configuration:** `CLAVREN_BTC_ZPUB` (the account-level extended public key of a
  BIP84 native SegWit wallet, `zpub…`, or `vpub…` on testnet; anything else fails startup with
  "CLAVREN_BTC_ZPUB must be a BIP84 zpub (native SegWit); export it from Sparrow under
  Settings → Keystore"). Optional: `CLAVREN_BTC_EXPLORERS`, two comma-separated Esplora API base
  URLs, default `https://mempool.space/api,https://blockstream.info/api` (`https://` required
  except a loopback address for tests; exactly two, different hosts, else startup fails). With
  the zpub set, the method `bitcoin` exists; it is independent of Stripe.
- **Exchange rates** (coin-neutral, item 23 reuses them): the euro price per coin from two
  sources: Kraken's public ticker (`https://api.kraken.com/0/public/Ticker?pair=XBTEUR`, field
  `c[0]`) and CoinGecko (`https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=eur`).
  Both must answer and be within 2% of each other; Clavren uses the **higher** of the two (the
  buyer pays a little more rather than the administrator getting less). Either fails or they
  disagree → 503 `payments_unavailable`, logged at `warn` with both figures. Rates are cached 60
  seconds. Only currency EUR products can take coins (the sources are queried in the product's
  currency; another currency → `product_unavailable` for these methods).
- **`POST /api/v1/purchases`** with `"method": "bitcoin"`: Clavren takes the next address index
  (a database sequence, never reused), derives the address `m/0/<index>` from the zpub, works
  out the amount in satoshis (total in cents ÷ rate, **rounded up** to the satoshi), stores both
  with the rate and `expires_at` = now + 30 minutes, and answers 201 with `checkout_url` =
  `<public url>/buy/pay#<secret>`. No outside call is made except the rates, so the start can't
  half-fail. 30 minutes is the time the rate is held.
- **`/buy/pay#<secret>`** (new public page, same look and theme rules as `/buy`): "Pay
  <amount> BTC" (8 decimals, with **Copy**), the address (with **Copy**), a QR code of the BIP21
  URI `bitcoin:<address>?amount=<btc>`, the euro total, and a countdown "Price held for mm:ss".
  It asks `POST /api/v1/purchases/status` every 10 seconds and moves to `/buy/status#<secret>`
  when the payment is seen. When the countdown ends with nothing seen: "This price expired.
  Don't send coins to this address now." with a link back to the buy page.
- **The watcher** (`purchases/watch.rs`, a background task beside `maintenance::spawn`, every 60
  seconds, stopping on the same `CancellationToken`): for every coin purchase that is pending,
  or ended within the last 7 days, it asks both explorers `GET <base>/address/<address>/txs` and
  `GET <base>/blocks/tip/height`, and counts only transactions **both** report, with the same
  value paid to the address.
  - Seen (in the mempool or confirmed) before `expires_at`, summing to at least the amount →
    `seen_at` set (event `Seen`); the status page says "Payment seen, waiting for confirmation.
    This usually takes 10 to 60 minutes."
  - The same with 1 confirmation on both explorers (tip − block height + 1 ≥ 1) → `Paid`, handled
    by item 18's provider-neutral `fulfil` with no buyer (no customer: the Purchases page shows
    "Anonymous"; paying in coins is often chosen for privacy, so Clavren never asks for an
    email). `paid_with` = `btc`. One confirmation is enough because a key costs far less than a
    double-spend attempt.
  - Expired with nothing seen → `Ended`. Expired with less than the amount, or with a payment
    first seen after `expires_at` → `Ended` with `payment_issue` `partly_paid` or `paid_late` and
    a `warn` log: the buyer sent coins but got no keys, so the administrator settles it by hand
    (send the coins back, or issue the keys by hand). The status page says "This payment didn't
    complete (too little was sent, or it arrived after the price expired). Contact the seller
    with this page's address." instead of "Nothing was charged".
  - More than the amount → paid normally, the surplus logged at `info`.
  - One explorer down or the two disagreeing: nothing changes and the watcher tries again next
    round; after 10 minutes of it, a `warn` log naming the explorer. Never act on one answer.
  - Each round handles each purchase in its own transaction, so one failure doesn't stop the
    others; an error is logged and retried next round, never swallowed.
- **Wallet gap limit:** wallets stop looking for payments after 20 unused addresses in a row,
  and expired purchases leave unused addresses. The deployment guide tells the administrator to
  set Sparrow's gap limit to 1000 (Settings → Advanced → Gap limit) so every payment shows in
  their own wallet too.
- **Buy page** (item 19): with card and coins, **Pay with card** stays the primary button (one
  primary action per page) and **Pay with Bitcoin** is a secondary button on its own row below,
  with the Bitcoin logo; with coins only, the first coin button is primary. The overview's
  `payment_methods` gains `"bitcoin"`; the Products page's Sell online switch is enabled when any
  method is configured.
- **Purchases page** (item 20, when built): a Paid with column (Card, Bitcoin), the
  `payment_issue` warning badge, and **View on explorer** (`https://mempool.space/address/<address>`).
- Coin refunds are sent by the administrator from their own wallet, and the keys revoked by hand:
  Clavren has no spend key and nothing to watch.

### Out of scope
- Monero: item 23. Lightning (needs a hot wallet with keys online). Other coins.
- Hiding the Pi's IP address from the explorers (Tor): a later item if wanted.
- Running any node; a hosted processor.

### Implementation
1. `crates/clavren-server/Cargo.toml`: add `bitcoin` (rust-bitcoin, current version, default
   features off apart from `std`) for zpub parsing and BIP84 address derivation. No QR crate:
   the QR code is drawn in the browser (step 8).
2. `migrations/<next timestamp>_coin_payments.up.sql` / `.down.sql`: widen the
   `purchases.provider` CHECK to `('stripe', 'bitcoin')`; add `seen_at TIMESTAMPTZ NULL`,
   `payment_issue TEXT NULL CHECK (payment_issue IN ('partly_paid', 'paid_late'))` with
   `CHECK (payment_issue IS NULL OR ended_at IS NOT NULL)`, `paid_with TEXT NULL CHECK (paid_with
   IN ('card', 'btc'))` with `CHECK (paid_with IS NULL OR paid_at IS NOT NULL)` (existing paid
   purchases get `'card'`); new table `coin_payments` (`purchase_id` UUID PK REFERENCES purchases
   ON DELETE CASCADE, `coin` TEXT CHECK (coin IN ('btc')), `address` TEXT UNIQUE NOT NULL,
   `derivation_index` BIGINT NULL, `amount_atomic` NUMERIC(30,0) CHECK > 0, `rate_cents` BIGINT
   CHECK > 0 (euro cents per whole coin), `expires_at` TIMESTAMPTZ NOT NULL), with UNIQUE (coin,
   derivation_index); sequence `btc_address_index`. Everything depends on the purchase alone
   (BCNF row in `docs/design-decisions.md`).
3. `crates/clavren-server/src/config/mod.rs`, `config/parse.rs`: `BitcoinSettings { zpub,
   explorers: [Url; 2] }` as `PaymentSettings.bitcoin`, with the startup errors above.
   `deploy/clavren.env.example`: the two variables, commented.
4. `crates/clavren-server/src/purchases/rates.rs` (new): the two sources, the 2% rule, the higher
   rate, the 60-second cache; one function per coin pair so item 23 adds XMR.
5. `crates/clavren-server/src/purchases/providers/bitcoin.rs` (new): `Bitcoin` implements
   `PaymentProvider::start` (address, amount, `/buy/pay` URL); `providers/esplora.rs`: the
   explorer client (`ureq` in `spawn_blocking`, timeout 10 s, as `stripe.rs`); pure functions for
   "what both explorers agree on" so they unit-test without HTTP.
6. `crates/clavren-server/src/purchases/watch.rs` (new): the watcher loop, coin-neutral, asking a
   `CoinWatcher` trait (`observe(&CoinPayment, now) -> Result<Observation, WatchError>`; Bitcoin
   implements it here, Monero in item 23) and turning observations into `ProviderEvent`s for
   `fulfil`. Started in `lib.rs` next to the maintenance task, stopped on shutdown (clean
   shutdown on every quit path).
7. `crates/clavren-server/src/purchases/fulfil.rs`: `ProviderEvent::Paid` gains `paid_with`
   (Stripe passes `card`); handle `Seen` (idempotent), an ended event's payment issue, and a paid
   event without a buyer. `public_routes.rs`: `/purchases/status` answers `seen: bool`,
   `payment_issue`, and for coin purchases `coin: {"address", "amount", "coin", "expires_at"}`
   for the pay page.
8. `web/admin/src/buy/PayCoins.tsx` (new, the `/buy/pay` page, its route beside `/buy`), the QR
   code from the `qrcode` npm package (current version) rendered to a canvas; `BuyForm.tsx`: the
   coin button and the primary/secondary rule; `PurchaseStatus.tsx`: the seen and payment-issue
   states. `web/admin/public/bitcoin.svg` (the public-domain Bitcoin logo), 20 px high beside the
   label, `alt=""`.
9. `crates/clavren-client` (only if item 21 is done): `PaymentMethod::Bitcoin`, and
   `PurchasePending` carrying `seen: bool`.
10. Logging (`clavren::purchases`): `info` on start (purchase ID, method), seen, paid; `warn` on
    rate disagreement, a payment issue, an explorer out for 10 minutes. Never the secret, keys,
    addresses or the zpub.
11. `metadata -c` / `-m` on every file.

### Tests
- `purchases/rates.rs` unit tests: higher rate used, 2% disagreement refused, one source down
  refused, cache reused within 60 s.
- `providers/bitcoin.rs` unit tests: BIP84 derivation against the BIP84 test vectors (zpub of the
  `abandon … about` mnemonic: index 0 = `bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu`), a non-zpub
  refused, satoshis rounded up.
- `providers/esplora.rs` unit tests: agreement rules (one explorer missing a tx, different
  values, confirmations from the lower tip).
- `crates/clavren-server/tests/purchases.rs`, with two fake explorers and fake rate sources
  (local HTTP servers, so the real clients run): `a_bitcoin_purchase_gets_a_fresh_address_and_amount`,
  `addresses_are_never_reused`, `rates_disagreeing_is_503`, `seen_then_confirmed_issues_the_keys`,
  `one_explorer_alone_issues_nothing`, `underpaid_and_expired_ends_partly_paid`,
  `paid_after_expiry_ends_paid_late`, `overpaid_issues_normally`, `the_watcher_issues_once`,
  `a_coin_purchase_has_no_customer`, `non_eur_products_cant_take_coins`.
- `config/tests.rs`: zpub rules, explorer list rules.
- `tests/schema.rs`, `tests/migrations.rs`: the new table and columns, both ways.
- `web/admin/e2e/buy.spec.ts`: card primary and Bitcoin secondary; the pay page with QR,
  copy buttons and countdown; expired, seen and payment-issue states; baselines desktop and
  phone, light and dark, reviewed by eye.
- Manual: on testnet (`vpub` and the explorers' `/testnet/api` URLs), pay a purchase from Sparrow
  and see the keys issued after one confirmation.

### Docs
- `docs/spec.md` §2 "Buying a key" (coins), §3 Purchase (seen, payment issue, anonymous), §7 the
  new table and columns, §12 the watcher.
- `docs/api.md`: the `bitcoin` method and the status fields.
- `docs/deployment.md`: "Bitcoin": make a wallet in Sparrow (native SegWit), export the zpub,
  set the gap limit to 1000, set `CLAVREN_BTC_ZPUB`, restart; what the explorers can see.
- `docs/design-decisions.md`: "Coins watched through public explorers" (no node, no server, two
  explorers must agree, higher of two rates, watch-only key, the privacy cost; replaces the
  BTCPay plan) and the BCNF rows.
- Changelog: "Take Bitcoin: the buy page shows an address and amount, and the keys are issued
  once the payment is confirmed."

### Done when
- [ ] On testnet, a Bitcoin payment issues its keys after one confirmation, shown on the status
      page.
- [ ] With one fake explorer reporting a payment the other doesn't, nothing is issued.
- [ ] An underpaid purchase ends with the "didn't complete" message and a warning.
- [ ] `cargo test`, clippy, the coverage gate and the Playwright suite pass; baselines reviewed.

## 23. Take Monero into a view-only wallet on a public node

**On hold (2026-11-13):** the administrator wants to settle how to take payments before any of
items 18 to 23 is built. Don't implement this item, and skip it for "do the next todo", until they
lift the hold; keep refining it on request.

**Deferred (2026-11-12),** with item 22, and the same tax conflict applies: raise it first.

**Why:** the second coin. Do it after item 22, which builds the payment page, the watcher, the
rates and payment issues. Monero hides amounts and addresses on its blockchain, so an explorer
can't tell Clavren about payments; instead Clavren runs `monero-wallet-rpc` (Monero's official
wallet program, small enough for the Pi) holding a **view-only** wallet: the wallet's primary
address and private view key, which see incoming payments but can't spend. It connects to a
**public remote node** instead of downloading the blockchain, as phone wallets do. The wallet
downloads blocks from the node and checks them itself with the view key, which never leaves the
Pi, so the node learns only the Pi's IP address and when it syncs, not the payments. What is
trusted: the node could hide a payment (a buyer kept waiting) or show a made-up one, so Clavren
confirms each payment's transaction on a **second, independent node** before issuing keys.
Note for the administrator: from July 2027 EU-regulated exchanges may no longer handle Monero
(the EU anti-money-laundering regulation), so turning XMR into euros inside the EU gets harder;
Clavren still receives it.
**Goal:** the buy page offers **Pay with Monero**; the pay page from item 22 shows a fresh
subaddress, the amount in XMR and a QR code; after 10 confirmations, checked on two nodes, the
keys are issued.

### Behaviour
- **The wallet on the Pi:** `monero-wallet-rpc` (the official `linux-armv8` release, checked
  against its published hash) as a systemd service `clavren-monero-wallet`, user `clavren`,
  started before `clavren`, with `--wallet-file /var/lib/clavren/monero/view-only`,
  `--daemon-address <node 1>`, `--daemon-ssl enabled`, `--rpc-bind-ip 127.0.0.1`,
  `--rpc-bind-port 18088`, `--rpc-login clavren:<random>`, and **not** `--trusted-daemon` (the
  node isn't trusted, so the wallet asks it less). The view-only wallet is made with
  `monero-wallet-cli --generate-from-view-key` from the primary address and private view key
  shown in the administrator's wallet (Feather: Wallet → Keys), with a restore height of today so
  it doesn't scan years of blocks.
- **Turned on by configuration:** `CLAVREN_XMR_WALLET_RPC` (default `http://127.0.0.1:18088`,
  loopback only), `CLAVREN_XMR_RPC_LOGIN` (`user:password`, digest auth, as the service's
  `--rpc-login`), `CLAVREN_XMR_CHECK_NODE` (the second node's `https://` address). All three or
  none; with them the method `monero` exists. Nodes: two from `monero.fail` with SSL, run by
  different people, chosen when setting up and written in the deployment guide.
- **Exchange rates:** item 22's `rates.rs` with an XMR/EUR pair: CoinGecko (`ids=monero`) and a
  second independent source that still lists XMR/EUR when this is built (Kraken dropped Monero
  for European clients in 2024; check `https://api.kraken.com/0/public/Ticker?pair=XMREUR` and
  otherwise pick an exchange whose public ticker lists it, and write the choice down here). Same
  2%, higher-rate and 503 rules.
- **`POST /api/v1/purchases`** with `"method": "monero"`: `create_address` (account 0, label =
  purchase ID) gives a fresh subaddress; amount in piconero (rounded up); stored in
  `coin_payments` (`coin` `xmr`, `derivation_index` = the subaddress index); `expires_at` 30
  minutes; `checkout_url` = `/buy/pay#<secret>`. The pay page shows "Pay <amount> XMR" (12
  decimals trimmed of trailing zeros), the subaddress and a QR code of
  `monero:<address>?tx_amount=<xmr>`. Wallet RPC down → 503 `payments_unavailable`, logged at
  `error` naming the service.
- **Watching** (item 22's watcher, `CoinWatcher` for Monero): `get_transfers` with `in`, `pool`,
  `account_index` 0 and the purchase's `subaddr_indices`.
  - In the pool or confirmed before `expires_at`, summing to at least the amount → `Seen`
    ("Payment seen, waiting for confirmations. Monero takes about 20 minutes.").
  - Each transfer with at least 10 confirmations (Monero's own lock time) is checked on the
    second node (`POST <check node>/get_transactions` with `txs_hashes`, and `in_pool` false
    with the same block height); when all check out → `Paid`, `paid_with` `xmr`, no customer.
  - Second node doesn't know the transaction, or reports another height → nothing changes, and
    after 30 minutes a `warn` log ("the two Monero nodes disagree about a payment"). Never issue
    on one node.
  - Underpaid, late and overpaid as item 22.
  - Wallet RPC down: retried next round, `warn` after 10 minutes.
- **Buy page:** **Pay with Monero** is another secondary button on its own row, with the Monero
  logo. `payment_methods` gains `"monero"`.
- **Purchases page** (item 20): Paid with shows Monero; no explorer link (Monero payments
  can't be viewed publicly).

### Out of scope
- Running a Monero node (`monerod`). Monero refunds (sent by hand from the administrator's
  wallet).
- Hiding the Pi's IP address from the nodes (Tor).

### Implementation
1. `migrations/<next timestamp>_monero.up.sql` / `.down.sql`: widen `purchases.provider` to add
   `'monero'`, `paid_with` and `coin_payments.coin` to add `'xmr'`.
2. `deploy/clavren-monero-wallet.service` (new) with the flags above; `deploy/install.sh` installs it
   disabled (as it installs `clavren.service` and the backup and update units), so only an administrator who sets Monero up turns it on; the Pi bundle
   build (`cargo-zigbuild`, see `docs/development.md`) is unchanged, since `monero-wallet-rpc`
   is downloaded from getmonero.org, not shipped.
3. `crates/clavren-server/src/config/mod.rs`, `config/parse.rs`: `MoneroSettings { wallet_rpc,
   login, check_node }` as `PaymentSettings.monero`, all three or none, loopback rule for the RPC.
4. `crates/clavren-server/src/purchases/providers/monero.rs` (new): `Monero` implements
   `PaymentProvider::start` and `CoinWatcher`; the JSON-RPC client (`ureq`, digest auth, timeout
   10 s) and the second-node check, pure decision functions separate from HTTP.
5. `crates/clavren-server/src/purchases/rates.rs`: the XMR/EUR pair.
6. `web/admin/src/buy/BuyForm.tsx` and `PayCoins.tsx`: the Monero button and amount format;
   `web/admin/public/monero.svg` (the Monero logo, free to use under the Monero trademark
   policy).
7. `crates/clavren-client` (only if item 21 is done): `PaymentMethod::Monero`.
8. Logging and `metadata` as item 22.

### Tests
- `providers/monero.rs` unit tests: piconero rounding, the two-node agreement rule, the 10
  confirmations rule, amount formatting.
- `crates/clavren-server/tests/purchases.rs` with a fake wallet RPC and a fake second node:
  `a_monero_purchase_gets_a_fresh_subaddress`, `ten_confirmations_on_both_nodes_issue_the_keys`,
  `the_second_node_not_knowing_the_tx_issues_nothing`, `wallet_rpc_down_is_503`,
  `underpaid_monero_ends_partly_paid`.
- `config/tests.rs`: all three Monero variables or none; loopback rule.
- `web/admin/e2e/buy.spec.ts`: three methods (card primary, two secondary rows); the Monero pay
  page; baselines.
- Manual: on Monero stagenet (a stagenet view-only wallet and stagenet public nodes), pay from
  Feather and see the keys issued after 10 confirmations; then check the service survives a Pi
  reboot.

### Docs
- `docs/deployment.md`: "Monero": install `monero-wallet-rpc`, make the view-only wallet, pick
  two nodes, the service, the three variables; what the nodes can see; the 2027 EU note.
- `docs/spec.md`, `docs/api.md`, `docs/design-decisions.md` ("Monero through a view-only wallet
  on a remote node, confirmed on a second node").
- Changelog: "Take Monero: payments go to a view-only wallet on the server and the keys are
  issued after 10 confirmations."

### Done when
- [ ] On stagenet, a Monero payment issues its keys after 10 confirmations checked on two nodes.
- [ ] A payment the second node doesn't know issues nothing.
- [ ] The wallet service starts on boot and stops cleanly with the Pi.
- [ ] `cargo test`, clippy, the coverage gate and the Playwright suite pass; baselines reviewed.
