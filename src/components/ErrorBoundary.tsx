// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Catches errors thrown while rendering its children and shows a
 * fallback in their place.
 *
 * Without a boundary, React unmounts the whole tree on a render error,
 * and the long-lived windows stay blank until reloaded. The boundary
 * stays in its fallback until it remounts; callers give it a `key`
 * that changes with the content, so opening another view starts fresh.
 *
 * `Suspense` above or below it keeps handling thrown promises.
 */

import { Component, type ErrorInfo, type ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback: (error: Error) => ReactNode;
  /** Called once per caught error, with React's component stack. */
  onError?: (error: Error, componentStack: string) => void;
}

interface ErrorBoundaryState {
  error: Error | null;
}

// Code may throw anything; the fallback and the log want an Error.
function toError(thrown: unknown): Error {
  return thrown instanceof Error ? thrown : new Error(String(thrown));
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(thrown: unknown): ErrorBoundaryState {
    return { error: toError(thrown) };
  }

  componentDidCatch(thrown: unknown, info: ErrorInfo): void {
    this.props.onError?.(toError(thrown), info.componentStack ?? "");
  }

  render(): ReactNode {
    return this.state.error ? this.props.fallback(this.state.error) : this.props.children;
  }
}
