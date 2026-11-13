// Cloudflare Turnstile's check that a person sends the form, when the server has a site key. The
// script loads from Cloudflare only on this page, and only then.

import { useEffect, useRef, useState } from "react";

type TurnstileApi = {
  render: (element: HTMLElement, options: Record<string, unknown>) => string;
  reset: (widget: string) => void;
  remove: (widget: string) => void;
};

declare global {
  interface Window {
    turnstile?: TurnstileApi;
  }
}

const SCRIPT = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
let loading: Promise<TurnstileApi> | null = null;

function loadScript(): Promise<TurnstileApi> {
  if (!loading) {
    loading = new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = SCRIPT;
      script.async = true;
      script.onload = () => (window.turnstile ? resolve(window.turnstile) : reject(new Error("no turnstile")));
      script.onerror = () => {
        loading = null;
        reject(new Error("the Turnstile script didn't load"));
      };
      document.head.appendChild(script);
    });
  }
  return loading;
}

type Props = {
  siteKey: string;
  /** The answer to send with the form; null when there is none (yet, or any more). */
  onAnswer: (answer: string | null) => void;
  /** Changing it asks for a fresh check (an answer works once). */
  round: number;
};

export function Turnstile({ siteKey, onAnswer, round }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let widget: string | null = null;
    let cancelled = false;
    onAnswer(null);
    loadScript().then(
      (turnstile) => {
        if (cancelled || !box.current) return;
        widget = turnstile.render(box.current, {
          sitekey: siteKey,
          callback: (answer: string) => onAnswer(answer),
          "expired-callback": () => onAnswer(null),
          "error-callback": () => onAnswer(null),
        });
      },
      () => !cancelled && setFailed(true),
    );
    return () => {
      cancelled = true;
      if (widget && window.turnstile) window.turnstile.remove(widget);
    };
  }, [siteKey, onAnswer, round]);
  if (failed) {
    return (
      <p className="notice notice--error" role="alert">
        The check that you're a person couldn't load. Allow challenges.cloudflare.com, or try another browser, then reload the page.
      </p>
    );
  }
  return <div ref={box} className="turnstile" />;
}
