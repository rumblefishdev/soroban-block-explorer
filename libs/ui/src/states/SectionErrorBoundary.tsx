import { Component, type ErrorInfo, type ReactNode } from 'react';

import { GenericErrorState } from './errors/GenericErrorState.js';

interface SectionErrorBoundaryProps {
  /** Optional name for observability — included in the onError callback. */
  sectionName?: string;
  /** Custom fallback. Receives the error and a reset function. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
  /** Observability hook — called on every caught error. */
  onError?: (error: Error, info: ErrorInfo, sectionName?: string) => void;
  children: ReactNode;
}

interface SectionErrorBoundaryState {
  error: Error | null;
}

/**
 * Scopes render errors to a single page section. Sibling sections on the
 * same page continue to render normally - critical for detail pages that
 * fetch multiple independent resources (account, contract, liquidity pool).
 *
 *   <SectionErrorBoundary sectionName="account-summary">
 *     <AccountSummary id={id} />
 *   </SectionErrorBoundary>
 */
export class SectionErrorBoundary extends Component<
  SectionErrorBoundaryProps,
  SectionErrorBoundaryState
> {
  override state: SectionErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): SectionErrorBoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    this.props.onError?.(error, info, this.props.sectionName);
  }

  reset = () => {
    this.setState({ error: null });
  };

  override render() {
    const { error } = this.state;
    if (error) {
      if (this.props.fallback) {
        return this.props.fallback(error, this.reset);
      }
      return (
        <GenericErrorState onRetry={this.reset} meta={this.props.sectionName} />
      );
    }
    return this.props.children;
  }
}
