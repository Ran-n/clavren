[//]: # ( ---------------------------------------------------------------------- )
[//]: # (+ Authors: 	Ran# <ran.hash@proton.me> )
[//]: # (+ Created: 	2026/11/13 19:46:08.759494 )
[//]: # (+ Revised: 	2026/11/13 19:49:12.515397 )
[//]: # ( ---------------------------------------------------------------------- )

# Backlog

Ideas worth keeping that aren't planned yet. Planned work lives in
`TODO.md`; nothing here is picked up by "do next". Items are light on
purpose: the full spec is written when an item is promoted to `TODO.md`,
against the code as it is then.

Next backlog number: 7

## Index

- [B1. Take payments for keys through Stripe Checkout with Managed Payments](#b1-take-payments-for-keys-through-stripe-checkout-with-managed-payments)
- [B2. Add the public buy page and the purchase status page](#b2-add-the-public-buy-page-and-the-purchase-status-page)
- [B3. Show purchases in the dashboard and handle refunds](#b3-show-purchases-in-the-dashboard-and-handle-refunds)
- [B4. Let the application buy a key](#b4-let-the-application-buy-a-key)
- [B5. Take Bitcoin into a watch-only wallet checked through public explorers](#b5-take-bitcoin-into-a-watch-only-wallet-checked-through-public-explorers)
- [B6. Take Monero into a view-only wallet on a public node](#b6-take-monero-into-a-view-only-wallet-on-a-public-node)

B1 to B6 (payments) were parked on 2026-11-13 until the administrator decides how to take
payments. They build on each other: B1 first, then B2, then B3 and B4; B5 after B1 and B2, B6
after B5.

## Using the backlog

- Write for someone who has never seen the conversation: what happens today,
  what is wrong with it, and the result wanted.
- Number new items with "Next backlog number" (B1, B2, …), add them at the
  end, then increase that number. Deleting leaves a gap; nothing is renumbered.
- Keep the Index in file order: one `- [B<n>. title](#anchor)` line per item;
  update it whenever an item is added, retitled or deleted.
- Screenshots go in `docs/todo/B<n>-<slug>.png`, linked from the item.
- To build an item, promote it: it becomes a full `TODO.md` item and leaves
  this file.
- Items older than 6 months get reviewed: keep, promote or drop.

## Template

```markdown
## B<n>. <short imperative title>

**Added:** YYYY-MM-DD
**Why:** what happens today and what is wrong with it.
**Goal:** the result, as the user will see it.

### Notes
- What is already known: files involved, constraints, related items, links.
```

---

## B1. Take payments for keys through Stripe Checkout with Managed Payments

**Added:** 2026-11-13
**Why:** second step of "charge per key" (decided 2026-11-12: record prices first, then sell
online; prices are already done). Today a buyer must pay somewhere else and wait for the
administrator to issue the key by hand.
**Goal:** a product marked **Sells online** can be bought through the public API: Clavren
creates a Stripe Checkout page for one to ten keys at the product's price, Stripe tells Clavren
when the payment went through, and Clavren issues the keys to a customer made from the buyer's
name and email, with the price recorded. The buyer collects the keys with a secret, as an
applicant collects an approved key.

### Notes
- **Stripe is the Merchant of Record** through Stripe Managed Payments (decided 2026-11-13: the
  administrator sells as a hobby and wants no VAT registration or returns). Chosen over Paddle
  (strict seller review, leans to businesses), Polar (dearer on non-US cards), Lemon Squeezy
  (being folded into Managed Payments) and Gumroad (10%): about 3.5% on top of Stripe's card fee,
  and it is one parameter (`managed_payments[enabled]=true`) on an ordinary Checkout Session.
  Stripe docs: `docs.stripe.com/payments/managed-payments` (eligibility, `…/update-checkout`,
  `…/how-it-works`). Never fall back to a session without `managed_payments` (that would make the
  administrator the seller unknowingly): answer 503 `payments_unavailable` instead.
- Hosted Checkout means Clavren never sees card numbers (no PCI work); two HTTPS calls and a
  webhook fit the Pi.
- **Providers behind one `PaymentProvider` interface** (Strategy): purchases, fulfilment, the
  status endpoint and tables are provider-neutral so B5 and B6 add coins without touching them,
  and another MoR stays possible if Stripe turns the account down.
- Config: `CLAVREN_STRIPE_SECRET_KEY` and `CLAVREN_STRIPE_WEBHOOK_SECRET`, both or neither (as the
  Turnstile keys); `CLAVREN_STRIPE_API_URL` for tests only (`https://` except loopback).
- Routes planned: `POST /api/v1/purchases` (`product`, `quantity` 1–10, `method` default `card`;
  answers `secret` + `checkout_url`; only the BLAKE3 hash of the secret stored);
  `POST /api/v1/payments/stripe` (webhook, raw body, HMAC-SHA256 `Stripe-Signature` with a 5-minute
  window: SHA-256 is Stripe's requirement, not a choice); `POST /api/v1/purchases/status`.
  Unknown, inactive, not-selling product or unconfigured method all read as 404
  `product_unavailable`. Rate limits as `applications/status` (`FailureLimiter` in `http/mod.rs`).
- Session details: tax code `txcd_10202000`, `tax_behavior=inclusive` (EU consumer prices include
  VAT; the recorded licence price is what the buyer paid), `Idempotency-Key` = purchase ID,
  `Stripe-Version` pinned to `2026-04-22.dahlia`; Managed Payments refuses `automatic_tax`,
  `tax_id_collection`, `invoice_creation`, `payment_method_types`, `customer_update[…]`,
  `shipping_…`, `payment_intent_data.receipt_email`.
- Fulfilment is idempotent (Stripe resends events), reuses a customer by email ignoring case,
  issues even for a product deactivated after checkout (the buyer paid), and marks paid with no
  licences if the product was deleted. Pending purchases are deleted by housekeeping after 48 h.
- Schema idea: `products.sells_online` (needs a price above 0), table `purchases` (product, quantity,
  unit price, secret hash, provider + checkout/payment references, customer, created/paid/ended),
  `licenses.purchase_id`. No buyer name or email on `purchases`: they live on the customer, so
  deleting a customer still erases them (`docs/spec.md` §7). Payment records outlive products.
- Touches: `config/`, `audit/mod.rs` (`Actor.admin_id` becomes optional for system work),
  `licences/issue.rs` (move the `is_active` check out of `issue_in`), `products/mod.rs`,
  `maintenance.rs`, `http/error.rs`, a new `purchases/` module; `hmac` and `sha2` crates.
- `docs/spec.md` §1 lists selling as out of scope today: change it first when building.
- Manual test to do when built: in Stripe test mode, check the seller shows as Link, the total
  equals the price, `charge.refunded` / `charge.dispute.*` reach the webhook for Managed Payments
  payments (B3 relies on it), and whether the `#secret` fragment survives the redirect (else B2
  keeps the secret in `sessionStorage`).
- Out of scope: the pages (B2), the Purchases page and refunds (B3), buying from the client (B4),
  coins (B5, B6), other providers, coupons, subscriptions, any tax handling in Clavren, emailing
  keys (Clavren sends no email).

## B2. Add the public buy page and the purchase status page

**Added:** 2026-11-13
**Why:** after B1, a purchase can be made through the API only; a buyer needs a page to click,
and the administrator needs a way to turn selling on for a product.
**Goal:** a product that sells online shows **Buy a key — €19.99** on its key page; the buy page
`/buy?product=<code>` asks how many keys, sends the buyer to Stripe, and after paying the buyer
lands on `/buy/status#<secret>`, which shows the keys with **Copy**. On the Products page the
administrator turns **Sells online** on and copies the buy link.

### Notes
- Products page: the Applications column becomes "Getting a key" with Open/Close applications and
  Sell online / Stop selling (disabled with a reason when there is no price, a free price, or no
  payment provider; Stop selling uses an inline confirmation strip). The dashboard learns the
  configured methods from a `payment_methods` list on `GET /api/admin/overview`.
- Key page (`/apply?product=<code>`): a primary Buy button on its own row above the application
  form (CTA hierarchy).
- `/buy`: quantity 1–10 with live total and "Tax included", a line saying it's sold through Link
  (so the seller name on Stripe's page and the statement doesn't surprise), primary **Pay with
  card**; plain-English notices for unavailable, 503 and 429.
- `/buy/status#<secret>`: pending (poll every 5 s for 2 min, then every minute), paid (keys with
  Copy and Copy all, "Bookmark this page"), ended, unknown. The secret stays in the fragment;
  `Referrer-Policy: no-referrer` as `/apply/status`.
- Likely files: `web/admin/buy.html`, `web/admin/src/buy/`, `vite.config.ts` input and dev
  routing, `crates/clavren-server/src/dashboard.rs`, `overview.rs`, `ProductsPage.tsx`; reuse
  `apply/apply.css`, `shared/format.ts` `formatPrice`, `Button`, `Notice`, `ConfirmStrip`,
  `CopyButton`. A `GET /api/v1/purchases/product?code=` for the page's data.
- Rendered baselines for every state, desktop and phone, light and dark.
- Out of scope: a shop listing every product, coupons, a tax breakdown.

## B3. Show purchases in the dashboard and handle refunds

**Added:** 2026-11-13
**Why:** once keys are sold online (B1), the administrator needs to see every purchase and what
happens on a refund or dispute; otherwise a refunded buyer keeps working keys, and a paid
purchase whose product was deleted has nowhere to be seen.
**Goal:** a **Purchases** page lists every purchase with its state, and a refund made in Stripe
revokes the keys it paid for by itself.

### Notes
- Page after Applications in the menu: started, paid, customer, product, quantity, unit price,
  total, state (Pending, Paid, Ended, Refunded, Disputed), keys, **Open in Stripe**; filters and
  search remembered through `admin_preferences`; a "Paid, no keys" warning badge.
- Full refund (`charge.refunded`, `amount_refunded` = `amount`) revokes every key with reason
  "Refunded"; a partial refund revokes nothing (which keys is the administrator's call). A dispute
  revokes nothing until `charge.dispute.closed` is `lost` (disputes can be won). Revoke through
  `licences::lifecycle` so audit and locking match a manual revocation.
- Refunds are made in Stripe, not Clavren (one place for money). Under Managed Payments Stripe
  may refund by itself and fights disputes, which is why these come from webhook events. Confirm
  which events actually arrive for Managed Payments payments (B1's manual test).
- Overview's Charged section subtracts refunds; licence detail links to its purchase. Admin routes
  follow the expand pattern (`?expand=customer,product,licences`).
- Out of scope: starting refunds from the dashboard, editing or exporting purchases.

## B4. Let the application buy a key

**Added:** 2026-11-13
**Why:** after B1 and B2. An application can already apply for a key from its key prompt
(`Client::apply` in `crates/clavren-client/src/applying.rs`) and pick up the approved key by
itself; a buyer today must leave the app, buy on the web page and type the key in.
**Goal:** the app's key prompt can offer **Buy a key**: the client crate starts a purchase,
opens Stripe's page in the browser, and activates the paid key by itself, as it does for an
approved application.

### Notes
- API sketch: `client.buy(quantity, method)` → `Bought { checkout_url }` (`PaymentMethod` is
  `#[non_exhaustive]` so coins can be added); `check_purchase()` activates the first key when paid,
  the rest readable once via `purchased_keys()`; `dismiss_purchase()`. Outcomes `PurchasePending`,
  `PurchaseEnded`, `PurchaseUnavailable`.
- The secret is kept in the client's state file beside an application's secret (DPAPI-encrypted).
  Background check every minute for 30 minutes after `buy`, then at the application interval.
- A purchase and an application can't both wait: one prompt state.
- Wire types go in `crates/clavren-protocol/src/wire.rs`, shared with the server; fake server in
  `crates/clavren-client/tests/fake_server/mod.rs`.
- Out of scope: paying inside the app (card data only on Stripe's page); editing any other app.

## B5. Take Bitcoin into a watch-only wallet checked through public explorers

**Added:** 2026-11-13
**Why:** the administrator wanted to be paid in Bitcoin and Monero as well as by card (2026-11-12,
then deferred the same day). The BTCPay Server plan was replaced on 2026-11-13 by watching the
administrator's own wallets through public services: the Pi 3B+ can't hold nodes, a VPS costs
about €5–15 a month, and the old laptop may not cope.
**Goal:** the buy page offers **Pay with Bitcoin** beside **Pay with card**; Clavren shows its own
payment page with a fresh address, the amount in BTC at the current rate and a QR code, and issues
the keys once the payment has one confirmation on two explorers.

### Notes
- **Conflicts with the Merchant of Record decision (B1):** no MoR takes coins, so every coin sale
  makes the administrator the seller, with VAT and returns. Raise it first when promoting (coins
  with the tax work, coins as donations with no key, or drop coins).
- Watch-only: Clavren holds only the BIP84 zpub (`CLAVREN_BTC_ZPUB`), so nothing can be spent.
  Two independent Esplora explorers (default mempool.space and blockstream.info) must agree before
  acting, since one could lie or err; the privacy cost is that both see the addresses and the Pi's IP.
- Rates from two sources (Kraken, CoinGecko), within 2% of each other, using the higher; cached
  60 s; EUR products only. Price held 30 minutes; amount rounded up to the satoshi.
- Fresh address per purchase from a database sequence, never reused; tell the administrator to set
  Sparrow's gap limit to 1000 so expired purchases' addresses don't hide payments.
- A background watcher (beside `maintenance::spawn`, same `CancellationToken`) every 60 s; states
  Seen → Paid (one confirmation is enough for a key's price) → or Ended with `partly_paid` /
  `paid_late` for the administrator to settle by hand. Coin buyers stay anonymous (no customer,
  no email asked).
- Coin-neutral pieces (`/buy/pay` page, watcher with a `CoinWatcher` trait, `rates.rs`, payment
  issues) are built here so B6 only adds Monero. `rust-bitcoin` for derivation; QR drawn in the
  browser (`qrcode` npm). Test against the BIP84 vectors; manual test on testnet.
- Buy page: card stays primary, coins secondary on their own rows.
- Out of scope: Lightning (needs a hot wallet), other coins, Tor, running a node, hosted processors.

## B6. Take Monero into a view-only wallet on a public node

**Added:** 2026-11-13
**Why:** the second coin, after B5 (which builds the pay page, watcher, rates and payment
issues). Monero hides amounts and addresses, so explorers can't report payments.
**Goal:** the buy page offers **Pay with Monero**; the pay page shows a fresh subaddress, the
amount in XMR and a QR code; after 10 confirmations, checked on two nodes, the keys are issued.

### Notes
- Same Merchant of Record conflict as B5: raise it first.
- `monero-wallet-rpc` (official `linux-armv8` release, hash-checked) on the Pi as a systemd service
  `clavren-monero-wallet`, loopback-only RPC with a login, holding a **view-only** wallet (primary
  address + private view key, restore height of setup day) on a **public remote node**, not
  `--trusted-daemon`. The view key never leaves the Pi; the node sees only the IP and sync times.
- Every payment is confirmed on a second independent node (`get_transactions`) before issuing;
  never issue on one node. 10 confirmations (Monero's lock time).
- Config: `CLAVREN_XMR_WALLET_RPC`, `CLAVREN_XMR_RPC_LOGIN`, `CLAVREN_XMR_CHECK_NODE`, all or none.
  Nodes from `monero.fail` with SSL, run by different people.
- Rates: CoinGecko plus a second XMR/EUR source (Kraken dropped Monero for EU clients in 2024;
  check what still lists it when building).
- From July 2027 EU-regulated exchanges may stop handling Monero (EU AML regulation).
- `deploy/install.sh` installs the service disabled; the wallet binary is downloaded, not shipped.
- Out of scope: running `monerod`, Monero refunds (by hand), Tor.
