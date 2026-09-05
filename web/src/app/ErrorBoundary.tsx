import { Component, type ErrorInfo, type ReactNode } from 'react';
import { ErrorState } from '@/components/states/ErrorState';

interface Props {
  children: ReactNode;
}
interface State {
  hasError: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false };

  static getDerivedStateFromError(): State {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    // Technical details stay in the console; users see a friendly state.
    console.error('Unhandled UI error', error, info);
  }

  render(): ReactNode {
    if (this.state.hasError) {
      return (
        <ErrorState onRetry={() => window.location.reload()} fullHeight />
      );
    }
    return this.props.children;
  }
}
