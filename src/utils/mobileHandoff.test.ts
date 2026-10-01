import { describe, expect, it } from "vitest";
import { mobileHandoffUrl } from "./mobileHandoff";

function destination(payload: string) {
  const parsed = new URL(payload);
  expect(parsed.protocol).toBe("ratatoskr:");
  expect(parsed.hostname).toBe("add");
  expect([...parsed.searchParams.keys()]).toEqual(["url"]);
  return parsed.searchParams.get("url");
}

describe("accountless mobile link handoff", () => {
  it("round-trips an Instagram link and strips the desktop-only quality fragment", () => {
    const source = "https://www.instagram.com/reel/Fixture123/?igsh=fixture%2Bvalue#rud-quality=720";
    const payload = mobileHandoffUrl(source);
    expect(payload).not.toBeNull();
    expect(payload).toBe("ratatoskr://add?url=" + encodeURIComponent(source.split("#")[0]!));
    expect(destination(payload!)).toBe(source.split("#")[0]);
  });

  it("preserves encoded path, signed query ordering, plus signs and encoded delimiters", () => {
    const source = "https://cdn.example.com/a%2Fb/file%20name.zip?X-Amz-Signature=fixture%2Bbytes&Expires=1700000000&label=a%2Bb+c&part=a%26b%3Dc";
    const payload = mobileHandoffUrl(source);
    expect(payload).not.toBeNull();
    expect(destination(payload!)).toBe(source);
  });

  it("normalizes a Unicode public hostname/path once and round-trips the resulting web URL", () => {
    const source = "https://مثال.ir/پرونده.zip?name=آزمایش#section";
    const clean = new URL(source); clean.hash = "";
    const payload = mobileHandoffUrl(source);
    expect(payload).not.toBeNull();
    expect(destination(payload!)).toBe(clean.href);
    expect(new TextEncoder().encode(payload!).length).toBeLessThanOrEqual(2000);
  });

  it.each(["https://example.com/file.zip", "http://93.184.216.34/file.zip", "https://[2001:4860:4860::8888]/file.zip"])(
    "accepts an unambiguous public web destination: %s", (source) => {
      const payload = mobileHandoffUrl(source);
      expect(payload).not.toBeNull();
      expect(destination(payload!)).toBe(new URL(source).href);
    },
  );

  it.each([
    "http://localhost/file", "http://localhost./file", "http://printer.local/file", "http://printer.local./file",
    "http://router.internal/file", "http://router.lan/file", "http://intranet/file",
    "http://0.1.2.3/file", "http://127.0.0.1/file", "http://10.1.2.3/file", "http://172.16.1.2/file",
    "http://192.168.1.1/file", "http://169.254.169.254/file", "http://100.64.0.1/file", "http://100.127.255.254/file",
    "http://224.0.0.1/file", "http://240.0.0.1/file", "http://255.255.255.255/file",
    "http://[::]/file", "http://[::1]/file", "http://[fc00::1]/file", "http://[fd12::1]/file",
    "http://[fe80::1]/file", "http://[ff02::1]/file", "http://[::ffff:192.168.1.1]/file",
    "http://134744072/file", "http://0x08080808/file", "http://010.010.010.010/file", "http://8.8.2056/file",
  ])("refuses local, reserved or ambiguous numeric destination %s", (source) => {
    expect(mobileHandoffUrl(source)).toBeNull();
  });

  it.each([
    "https://user@example.com/file", "https://user:fixture@example.com/file",
    "https://example.com/file?access_token=fixture", "https://example.com/file?AUTHORIZATION=fixture",
    "https://example.com/file?Password=fixture", "https://example.com/file?SESSIONID=fixture",
    "https://example.com/file?cookie=fixture", "https://example.com/file?%61ccess_token=fixture",
    "https://example.com/file?%41UTHORIZATION=fixture", "https://example.com/file?%70assword=fixture",
  ])("never puts credentials into a transferable QR: %s", (source) => {
    expect(mobileHandoffUrl(source)).toBeNull();
  });

  it.each(["", "not a URL", "https://", "ftp://example.com/file", "file:///tmp/file", "javascript:alert(1)",
    "https://example.com/file%ZZ.zip", "https://example.com/?bad=%", "https://example.com/a\nb"])(
    "rejects malformed or non-web input %s", (source) => expect(mobileHandoffUrl(source)).toBeNull(),
  );

  it("rejects payloads exceeding the UTF-8 budget after percent encoding", () => {
    expect(mobileHandoffUrl("https://example.com/?value=" + "x".repeat(2100))).toBeNull();
    // Short character count can still expand far past the QR byte budget.
    expect(mobileHandoffUrl("https://example.com/?value=" + "آ".repeat(400))).toBeNull();
  });
});
