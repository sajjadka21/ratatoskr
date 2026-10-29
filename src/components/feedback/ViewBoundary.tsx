import { Component, type ErrorInfo, type ReactNode } from "react";
import { RotateCcw, TriangleAlert } from "lucide-react";

import { useI18n } from "../../i18n/I18n";

import "./ViewBoundary.css";

type ViewBoundaryProps = {
  /** A new key (another page) clears the failure. */
  resetKey?: string;
  /** Leaves the broken view; without it only reloading is offered. */
  onBack?: () => void;
  children: ReactNode;
};

type ViewBoundaryState = { failed: boolean };

/**
 * Keeps one broken view from blanking the whole window. Downloads belong to
 * the backend and keep running; the user gets a way back instead of an empty
 * window that only a restart would fix.
 */
export class ViewBoundary extends Component<ViewBoundaryProps, ViewBoundaryState> {
  state: ViewBoundaryState = { failed: false };

  static getDerivedStateFromError(): ViewBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("A view failed to render:", error, info.componentStack);
  }

  componentDidUpdate(previous: ViewBoundaryProps) {
    if (this.state.failed && previous.resetKey !== this.props.resetKey) {
      this.setState({ failed: false });
    }
  }

  render() {
    if (!this.state.failed) return this.props.children;
    const { onBack } = this.props;
    return (
      <ViewFailure
        onBack={
          onBack
            ? () => {
                this.setState({ failed: false });
                onBack();
              }
            : undefined
        }
      />
    );
  }
}

function ViewFailure({ onBack }: { onBack?: () => void }) {
  const { t } = useI18n();
  return (
    <div className="view-failure" role="alert">
      <TriangleAlert size={28} aria-hidden="true" />
      <h2>{t("crash.title")}</h2>
      <p>{t("crash.body")}</p>
      <div className="view-failure__actions">
        {onBack ? (
          <button type="button" className="view-failure__primary" onClick={onBack}>
            {t("crash.back")}
          </button>
        ) : null}
        <button type="button" onClick={() => window.location.reload()}>
          <RotateCcw size={14} aria-hidden="true" />
          {t("crash.reload")}
        </button>
      </div>
    </div>
  );
}
