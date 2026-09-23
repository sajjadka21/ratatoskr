import { Moon, Power, X } from "lucide-react";
import { useEffect, useState } from "react";

import type { CompletionActionEvent } from "../../types/download";

import "./CompletionBanner.css";

const ACTION_TEXT: Record<string, string> = {
  exit_app: "Download Manager will close",
  sleep: "The computer will sleep",
  hibernate: "The computer will hibernate",
  shutdown: "The computer will shut down",
};

type CompletionBannerProps = {
  event: CompletionActionEvent;
  onCancel: () => void;
};

/// Countdown for the power action a finished queue scheduled. Stays on top
/// of everything so it cannot be missed.
export function CompletionBanner({ event, onCancel }: CompletionBannerProps) {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    const timer = window.setInterval(
      () => setNow(Math.floor(Date.now() / 1000)),
      1000,
    );
    return () => window.clearInterval(timer);
  }, []);

  const seconds = Math.max(0, event.dueAt - now);
  const Icon = event.action === "sleep" || event.action === "hibernate" ? Moon : Power;

  return (
    <div className="completion-banner" role="alertdialog" aria-live="assertive" aria-labelledby="completion-banner-title">
      <Icon size={18} aria-hidden="true" />
      <div className="completion-banner__text">
        <strong id="completion-banner-title">
          {ACTION_TEXT[event.action] ?? "A queue action will run"} in {seconds}s
        </strong>
        <span>“{event.queueName}” finished.</span>
      </div>
      <button type="button" className="completion-banner__cancel" onClick={onCancel} autoFocus>
        <X size={14} aria-hidden="true" /> Cancel
      </button>
    </div>
  );
}
