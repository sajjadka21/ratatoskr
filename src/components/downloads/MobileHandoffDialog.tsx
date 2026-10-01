import { useEffect, useRef } from "react";
import { QRCodeSVG } from "qrcode.react";
import { X } from "lucide-react";
import { useI18n } from "../../i18n/I18n";
import { mobileHandoffUrl } from "../../utils/mobileHandoff";
import "./MobileHandoffDialog.css";

export function MobileHandoffDialog({ source, onClose }: { source: string; onClose: () => void }) {
  const ref = useRef<HTMLDialogElement>(null);
  const { language, dir } = useI18n();
  const fa = language === "fa";
  const payload = mobileHandoffUrl(source);
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    return () => { if (dialog.open) dialog.close(); };
  }, []);
  return <dialog ref={ref} className="mobile-handoff" dir={dir} aria-labelledby="handoff-title"
    onCancel={onClose} onClose={onClose}>
    <button className="mobile-handoff__close" autoFocus type="button" onClick={onClose}
      aria-label={fa ? "بستن" : "Close"}><X size={20} /></button>
    <h2 id="handoff-title">{fa ? "ارسال لینک به گوشی" : "Send link to your phone"}</h2>
    {payload ? <>
      <p>{fa ? "Ratatoskr را روی اندروید نصب کن، سپس این کد را با دوربین یا اسکنر QR باز کن. انتخاب دانلود در گوشی انجام می‌شود." :
        "Install Ratatoskr on Android, then open this code with your camera or QR scanner. Choose your download on your phone."}</p>
      <div className="mobile-handoff__qr"><QRCodeSVG value={payload} size={256} level="M" marginSize={4}
        title={fa ? "لینک دانلود برای اندروید" : "Android download link"} /></div>
      <p className="mobile-handoff__privacy">{fa ? "بدون حساب و سرویس واسط. کد حاوی لینک است؛ فقط با فرد مورد اعتماد به اشتراک بگذار." :
        "No account or relay service. This code contains the link; share it only with someone you trust."}</p>
    </> : <p role="alert">{fa ? "این لینک برای QR مناسب نیست؛ ممکن است طولانی، خصوصی یا دارای اطلاعات ورود باشد." :
      "This link cannot be shared as a QR. It may be too long, private, or contain credentials."}</p>}
  </dialog>;
}
