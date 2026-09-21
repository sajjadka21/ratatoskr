import {
  ClipboardPaste,
  Keyboard,
} from "lucide-react";

import "./SettingsPage.css";

export type AddDownloadInputMode =
  | "clipboard"
  | "manual";

type SettingsPageProps = {
  inputMode: AddDownloadInputMode;
  saving: boolean;
  error: string | null;
  onInputModeChange: (
    mode: AddDownloadInputMode,
  ) => void;
};

export function SettingsPage({
  inputMode,
  saving,
  error,
  onInputModeChange,
}: SettingsPageProps) {
  return (
    <section className="settings-page">
      <div className="settings-page__section">
        <div className="settings-page__section-heading">
          <h2>Add Download</h2>

          <p>
            Choose how Add Download gets its initial links.
          </p>
        </div>

        <div className="settings-page__group">
          <div className="settings-page__group-heading">
            <strong>Input mode</strong>

            <span>
              Single and batch detection is always automatic.
            </span>
          </div>

          <div className="settings-page__options">
            <button
              type="button"
              className={`settings-page__option ${
                inputMode === "clipboard"
                  ? "settings-page__option--active"
                  : ""
              }`}
              disabled={saving}
              onClick={() =>
                onInputModeChange("clipboard")
              }
            >
              <div className="settings-page__option-icon">
                <ClipboardPaste size={19} />
              </div>

              <div className="settings-page__option-copy">
                <strong>Clipboard</strong>

                <span>
                  Automatically extract valid HTTP and HTTPS links from the clipboard when Add Download opens.
                </span>
              </div>

              <span className="settings-page__radio">
                <span />
              </span>
            </button>

            <button
              type="button"
              className={`settings-page__option ${
                inputMode === "manual"
                  ? "settings-page__option--active"
                  : ""
              }`}
              disabled={saving}
              onClick={() =>
                onInputModeChange("manual")
              }
            >
              <div className="settings-page__option-icon">
                <Keyboard size={19} />
              </div>

              <div className="settings-page__option-copy">
                <strong>Manual</strong>

                <span>
                  Open Add Download empty and paste or type the links yourself.
                </span>
              </div>

              <span className="settings-page__radio">
                <span />
              </span>
            </button>
          </div>

          <div className="settings-page__hint">
            In both modes, one valid link becomes a single download and multiple unique links automatically become a batch.
          </div>

          {saving ? (
            <div className="settings-page__status">
              Saving...
            </div>
          ) : null}

          {error ? (
            <div className="settings-page__error">
              {error}
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}
