# Plugins

A plugin is one small JSON file of rules. It changes how Ratatoskr treats a link, a file name or a site, on
both the desktop app and Android, and it is the supported way to add behaviour without a new release or
to tune the app for yourself. **A plugin holds data only; it can never run code.**

Add one in *Settings → Plugins* (desktop) or the *Plugins* button on the main screen (Android). Switch it
off or remove it any time. A copy of the format with a working example is in
[`docs/plugins/example.json`](plugins/example.json).

```json
{
  "schema": 1,
  "id": "my-site",
  "name": "My site",
  "version": "1.0.0",
  "rules": [
    { "type": "rewrite_url", "match": "https://example.com/view/*", "replace": "https://cdn.example.com/files/{1}.zip" },
    { "type": "rename", "match": "*.mp4.part", "replace": "{1}.mp4" },
    { "type": "referer", "host": "cdn.example.com", "value": "https://example.com/" },
    { "type": "user_agent", "host": "cdn.example.com", "value": "Mozilla/5.0" }
  ]
}
```

| Rule | What it does | Desktop | Android |
|---|---|---|---|
| `rewrite_url` | A new link matching `match` is saved as `replace` instead | ✅ | ✅ |
| `rename` | A file the server names like `match` is saved as `replace` (a name you chose yourself always wins) | ✅ | ✅ |
| `referer` | Sends this `Referer` to `host` and its subdomains when the browser did not hand one over | ✅ | ignored |
| `user_agent` | Same for the `User-Agent` | ✅ | ignored |

## Patterns

`*` stands for any text (at most four per pattern). In `replace`, `{1}` to `{4}` put back what the first to
fourth `*` caught. There are no regular expressions, so a pattern cannot be made to run for a long time.
The first matching rule wins.

## Safety

- Files are limited to 64 KB and 50 rules; `id` is `a-z`, `0-9` and `-` only.
- A rewritten link must still be `http` or `https`; a new file name may not contain a path separator.
- `referer` must be an `http(s)` address and header values must be plain printable text.
- A file that fails any check is refused with a reason and never stored.
- Plugins cannot read your files, cookies or settings, and cannot make network requests of their own.

Rules that need real code (new sites, new post-download steps) are outside what a plugin can do; those go
through a normal release.
