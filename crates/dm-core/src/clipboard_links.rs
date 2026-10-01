//! Inert extraction from copied webpage fragments. No markup is rendered,
//! executed, fetched, logged or persisted; only validated link candidates leave.
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, EndTag, StartTag, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use reqwest::Url;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

pub const MAX_CLIPBOARD_BYTES: usize = 1024 * 1024;
pub const MAX_CLIPBOARD_LINKS: usize = 500;

fn clean(value: &str) -> Option<String> {
    let url = Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query_pairs().any(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "access_token" | "authorization" | "password" | "sessionid" | "cookie"
            )
        })
    {
        return None;
    }
    Some(url.to_string())
}

/// CF_HTML offsets count UTF-8 bytes, not characters. Reject invalid bounds
/// instead of accidentally importing links outside the copied selection.
fn fragment(input: &str) -> Option<(&str, &str, Option<Url>)> {
    let mut start = None;
    let mut end = None;
    let mut source = None;
    let mut header = false;
    for line in input.split(['\r', '\n']).take(30) {
        if line.trim_start().starts_with('<') {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key {
            "Version" | "StartHTML" | "EndHTML" => header = true,
            "StartFragment" => {
                header = true;
                start = Some(value.trim().parse::<usize>().ok()?);
            }
            "EndFragment" => {
                header = true;
                end = Some(value.trim().parse::<usize>().ok()?);
            }
            "SourceURL" => {
                source = clean(value).and_then(|value| Url::parse(&value).ok());
            }
            _ => {}
        }
    }
    if header {
        let (start, end) = (start?, end?);
        if start > end {
            return None;
        }
        return Some((input.get(start..end)?, input.get(..start)?, source));
    }
    if let Some(start) = input.find("<!--StartFragment-->") {
        let start = start + "<!--StartFragment-->".len();
        let end = start + input.get(start..)?.find("<!--EndFragment-->")?;
        return Some((input.get(start..end)?, input.get(..start)?, None));
    }
    Some((input, "", None))
}

struct Anchors {
    links: RefCell<Vec<String>>,
    seen: RefCell<HashSet<String>>,
    base: Option<Url>,
    inert: Cell<bool>,
    collect: bool,
    open_anchor: RefCell<Option<String>>,
}
impl TokenSink for Anchors {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        if let Token::TagToken(tag) = token {
            let name = tag.name.as_ref();
            if tag.kind == StartTag {
                let raw = match name {
                    "script" => Some(RawKind::ScriptData),
                    "style" | "iframe" | "xmp" | "noembed" | "noframes" => Some(RawKind::Rawtext),
                    "textarea" | "title" => Some(RawKind::Rcdata),
                    _ => None,
                };
                if let Some(kind) = raw {
                    self.inert.set(true);
                    return TokenSinkResult::RawData(kind);
                }
                if name == "a"
                    && !self.inert.get()
                    && self.links.borrow().len() < MAX_CLIPBOARD_LINKS
                {
                    self.open_anchor.replace(None);
                    if let Some(href) = tag
                        .attrs
                        .iter()
                        .find(|attr| attr.name.local.as_ref() == "href")
                    {
                        let raw = href.value.trim();
                        if raw.is_empty() || raw.starts_with('#') {
                            return TokenSinkResult::Continue;
                        }
                        let destination = clean(raw).or_else(|| {
                            self.base
                                .as_ref()?
                                .join(raw)
                                .ok()
                                .and_then(|url| clean(url.as_str()))
                        });
                        if let Some(value) = destination {
                            self.open_anchor.replace(Some(value.clone()));
                            if self.collect && self.seen.borrow_mut().insert(value.clone()) {
                                self.links.borrow_mut().push(value);
                            }
                        }
                    }
                }
            } else if tag.kind == EndTag && name == "a" {
                self.open_anchor.replace(None);
            } else if tag.kind == EndTag
                && matches!(
                    name,
                    "script"
                        | "style"
                        | "iframe"
                        | "xmp"
                        | "noembed"
                        | "noframes"
                        | "textarea"
                        | "title"
                )
            {
                self.inert.set(false);
            }
        }
        TokenSinkResult::Continue
    }
}

/// Text first, then selected anchors in document order; never duplicate tasks.
pub fn extract(text: Option<&str>, html: Option<&str>) -> Vec<String> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    if let Some(text) = text.filter(|text| text.len() <= MAX_CLIPBOARD_BYTES) {
        for candidate in crate::linkgrabber::extract_links(text) {
            if let Some(value) = clean(&candidate.url) {
                if seen.insert(value.clone()) {
                    links.push(value);
                }
                if links.len() >= MAX_CLIPBOARD_LINKS {
                    return links;
                }
            }
        }
    }
    if let Some((fragment, prefix, base)) = html
        .filter(|html| html.len() <= MAX_CLIPBOARD_BYTES)
        .and_then(fragment)
    {
        let context = Anchors {
            links: RefCell::new(Vec::new()),
            seen: RefCell::new(HashSet::new()),
            base: base.clone(),
            inert: Cell::new(false),
            collect: false,
            open_anchor: RefCell::new(None),
        };
        let input = BufferQueue::default();
        input.push_back(prefix.into());
        let context = Tokenizer::new(context, Default::default());
        let _ = context.feed(&input);
        context.end();
        let inert = context.sink.inert.get();
        if !inert
            && let Some(value) = context.sink.open_anchor.into_inner()
            && seen.insert(value.clone())
        {
            links.push(value);
        }
        let sink = Anchors {
            links: RefCell::new(links),
            seen: RefCell::new(seen),
            base,
            inert: Cell::new(inert),
            collect: true,
            open_anchor: RefCell::new(None),
        };
        let input = BufferQueue::default();
        input.push_back(fragment.into());
        let tokenizer = Tokenizer::new(sink, Default::default());
        let _ = tokenizer.feed(&input);
        tokenizer.end();
        return tokenizer.sink.links.into_inner();
    }
    links
}
