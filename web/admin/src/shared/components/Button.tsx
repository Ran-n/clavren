// Buttons: one primary action per page, smaller secondary ones, red for destructive ones. A
// disabled button looks dimmed and says why in its tooltip, and to screen readers as its
// description (a disabled button can't take focus, so a tooltip alone never reaches them).

import { useId, type ButtonHTMLAttributes } from "react";

type Props = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "danger" | "quiet";
  size?: "large" | "normal" | "small";
  /** Why the action isn't available; disables the button and shows as its tooltip. */
  disabledReason?: string | null;
  busy?: boolean;
};

export function Button({ variant = "secondary", size = "normal", disabledReason, busy, disabled, className, children, type, ...rest }: Props) {
  const reasonId = useId();
  const unavailable = Boolean(disabled || disabledReason || busy);
  const classes = ["button", `button--${variant}`, `button--${size}`, busy ? "is-busy" : "", className ?? ""].join(" ").trim();
  const button = (
    <button
      {...rest}
      type={type ?? "button"}
      className={classes}
      disabled={unavailable}
      aria-busy={busy || undefined}
      aria-describedby={disabledReason ? reasonId : rest["aria-describedby"]}
    >
      {children}
    </button>
  );
  // A disabled button gets no pointer events in some browsers, so the reason sits on a wrapper.
  return disabledReason ? (
    <span className="button-reason" title={disabledReason}>
      {button}
      <span id={reasonId} className="visually-hidden">
        {disabledReason}
      </span>
    </span>
  ) : (
    button
  );
}
