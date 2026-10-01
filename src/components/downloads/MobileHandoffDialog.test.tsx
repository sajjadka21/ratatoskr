// @vitest-environment happy-dom

import { computeAccessibleName } from "dom-accessibility-api";
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { QRCodeSVG } from "qrcode.react";

import { I18nProvider } from "../../i18n/I18n";
import type { Language } from "../../i18n/messages";
import { MobileHandoffDialog } from "./MobileHandoffDialog";

// Forward to the real encoder and SVG renderer; observe the boundary value
// without replacing the QR or duplicating the URL-validation implementation.
vi.mock("qrcode.react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("qrcode.react")>();
  return {
    ...actual,
    QRCodeSVG: vi.fn((props: React.ComponentProps<typeof actual.QRCodeSVG>) => <actual.QRCodeSVG {...props} />),
  };
});

const instagramSource = "https://www.instagram.com/reel/Fixture123/?igsh=fixture%2Bvalue#rud-quality=720";

describe("mobile link handoff dialog", () => {
  let root: Root;
  let showModal: ReturnType<typeof vi.spyOn>;
  const closed = vi.fn();

  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    document.body.innerHTML = '<div id="mount"></div>';
    root = createRoot(document.querySelector("#mount")!);
    closed.mockClear();
    vi.mocked(QRCodeSVG).mockClear();
    // happy-dom supports these natively. Keep their actual behavior, including
    // the open state and close event, instead of assigning test-only behavior.
    showModal = vi.spyOn(HTMLDialogElement.prototype, "showModal");
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    vi.restoreAllMocks();
    document.body.replaceChildren();
  });

  async function render(source = instagramSource, language: Language = "en") {
    function Fixture() {
      const [open, setOpen] = useState(true);
      return <I18nProvider language={language}>
        {open && <MobileHandoffDialog source={source} onClose={() => { closed(); setOpen(false); }} />}
      </I18nProvider>;
    }
    await act(async () => root.render(<Fixture />));
    return document.querySelector<HTMLDialogElement>("dialog")!;
  }

  function qrValue() {
    const calls = vi.mocked(QRCodeSVG).mock.calls;
    expect(calls).toHaveLength(1);
    return calls[0]![0].value;
  }

  it.each<Language>(["en", "fa"])("opens a native modal with a translated name and direction (%s)", async (language) => {
    const dialog = await render(instagramSource, language);
    expect(showModal).toHaveBeenCalledOnce();
    expect(dialog.open).toBe(true);
    expect(computeAccessibleName(dialog)).toBe(language === "fa" ? "ارسال لینک به گوشی" : "Send link to your phone");
    expect(dialog.dir).toBe(language === "fa" ? "rtl" : "ltr");
    expect(computeAccessibleName(dialog.querySelector("button")!)).toBe(language === "fa" ? "بستن" : "Close");
    expect(dialog.querySelector(".mobile-handoff__qr svg title")?.textContent).toBe(
      language === "fa" ? "لینک دانلود برای اندروید" : "Android download link",
    );
  });

  it("renders a local QR containing only the public Instagram link, without the desktop quality fragment", async () => {
    const dialog = await render();
    const destination = "https://www.instagram.com/reel/Fixture123/?igsh=fixture%2Bvalue";
    expect(qrValue()).toBe("ratatoskr://add?url=" + encodeURIComponent(destination));
    expect(new URL(qrValue() as string).searchParams.get("url")).toBe(destination);
    const svg = dialog.querySelector(".mobile-handoff__qr svg")!;
    expect(svg.querySelectorAll("path").length).toBeGreaterThan(0);
    expect(svg.getAttribute("width")).toBe("256");
    expect(dialog.querySelector("img, image, canvas, iframe")).toBeNull();
    expect(dialog.textContent).toContain("No account or relay service.");
    expect(dialog.querySelector('[role="alert"]')).toBeNull();
  });

  it("preserves signed public query bytes through the actual QR boundary", async () => {
    const source = "https://cdn.example.com/a%2Fb/file.zip?X-Amz-Signature=fixture%2Bbytes&Expires=1700000000&label=a%2Bb+c";
    await render(source);
    expect(qrValue()).toBe("ratatoskr://add?url=" + encodeURIComponent(source));
    expect(new URL(qrValue() as string).searchParams.get("url")).toBe(source);
  });

  it.each([
    "http://127.0.0.1/file.zip",
    "https://example.com/file.zip?%61ccess_token=fixture",
    "file:///fixture/private.zip",
    "https://example.com/" + "x".repeat(2000),
  ])("shows an explanation and never encodes an unsupported destination: %s", async (source) => {
    const dialog = await render(source);
    expect(dialog.open).toBe(true);
    expect(dialog.querySelector('[role="alert"]')?.textContent).toContain("This link cannot be shared as a QR.");
    expect(dialog.querySelector(".mobile-handoff__qr")).toBeNull();
    expect(QRCodeSVG).not.toHaveBeenCalled();
  });

  it("explains an unsupported destination in Persian", async () => {
    const dialog = await render("http://localhost/file.zip", "fa");
    expect(dialog.querySelector('[role="alert"]')?.textContent).toContain("این لینک برای QR مناسب نیست");
    expect(QRCodeSVG).not.toHaveBeenCalled();
  });

  it("closes and removes the QR when the visible close button is clicked", async () => {
    const dialog = await render();
    await act(async () => dialog.querySelector<HTMLButtonElement>("button")!.click());
    expect(closed).toHaveBeenCalledOnce();
    expect(document.querySelector("dialog")).toBeNull();
    expect(dialog.open).toBe(false);
  });

  it("handles native cancel events such as Escape", async () => {
    const dialog = await render();
    await act(async () => dialog.dispatchEvent(new Event("cancel", { cancelable: true })));
    expect(closed).toHaveBeenCalledOnce();
    expect(document.querySelector("dialog")).toBeNull();
    expect(dialog.open).toBe(false);
  });

  it("handles a native close event and removes the mounted dialog", async () => {
    const dialog = await render();
    await act(async () => dialog.close());
    expect(closed).toHaveBeenCalledOnce();
    expect(document.querySelector("dialog")).toBeNull();
    expect(dialog.open).toBe(false);
  });
});
