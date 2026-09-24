import { CheckCircle2, CircleAlert, Info, X } from "lucide-react";

import { useI18n } from "../../i18n/I18n";

import "./Toasts.css";

export type ToastKind = "success" | "error" | "info";

export type Toast = {
  id: number;
  kind: ToastKind;
  message: string;
};

type ToastsProps = {
  toasts: Toast[];
  onDismiss: (id: number) => void;
};

const ICONS = {
  success: CheckCircle2,
  error: CircleAlert,
  info: Info,
} as const;

/// App-wide feedback. Errors from actions taken outside a dialog (pause,
/// retry, bulk changes) land here so they are never swallowed.
export function Toasts({ toasts, onDismiss }: ToastsProps) {
  const { t } = useI18n();
  return (
    <div className="toasts" role="region" aria-label={t("toast.region")}>
      {toasts.map((toast) => {
        const Icon = ICONS[toast.kind];
        return (
          <div
            key={toast.id}
            className={`toast toast--${toast.kind}`}
            role={toast.kind === "error" ? "alert" : "status"}
          >
            <Icon size={16} strokeWidth={2} aria-hidden="true" />
            <span className="toast__message">{toast.message}</span>
            <button
              type="button"
              className="toast__close"
              aria-label={t("toast.dismiss")}
              onClick={() => onDismiss(toast.id)}
            >
              <X size={14} strokeWidth={2} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
