// What shows instead of a page that failed while drawing itself: a plain message and a way out,
// never a blank screen. The error itself goes to the browser's console for whoever looks into it.

import { Component, type ErrorInfo, type ReactNode } from "react";

export class ErrorBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo): void {
    console.error("the dashboard couldn't show this page", error, info.componentStack);
  }

  render(): ReactNode {
    if (!this.state.failed) return this.props.children;
    return (
      <div className="notice notice--error" role="alert">
        <span>Something went wrong while showing this page. Reload the page to try again.</span>
        <button type="button" className="link-button" onClick={() => window.location.reload()}>
          Reload
        </button>
      </div>
    );
  }
}
