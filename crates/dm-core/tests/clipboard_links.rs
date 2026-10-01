use dm_core::clipboard_links::extract;

/// Build genuine CF_HTML offsets from UTF-8 byte counts. The Persian prefix
/// deliberately makes character positions differ from the Windows offsets.
fn cf_html(source: &str, prefix: &str, fragment: &str, suffix: &str) -> String {
    fn header(
        source: &str,
        start: usize,
        end: usize,
        fragment_start: usize,
        fragment_end: usize,
    ) -> String {
        format!(
            "Version:1.0\r\nStartHTML:{start:010}\r\nEndHTML:{end:010}\r\nStartFragment:{fragment_start:010}\r\nEndFragment:{fragment_end:010}\r\nSourceURL:{source}\r\n"
        )
    }
    let start = header(source, 0, 0, 0, 0).len();
    let fragment_start = start + prefix.len();
    let fragment_end = fragment_start + fragment.len();
    let end = fragment_end + suffix.len();
    format!(
        "{}{prefix}{fragment}{suffix}",
        header(source, start, end, fragment_start, fragment_end)
    )
}

#[test]
fn extracts_plain_text_when_html_is_absent() {
    let text = "بخش اول: https://files.example.com/part2.zip\nhttps://files.example.com/part1.zip\nhttps://files.example.com/part2.zip";
    assert_eq!(
        extract(Some(text), None),
        vec![
            "https://files.example.com/part2.zip",
            "https://files.example.com/part1.zip",
        ]
    );
    assert!(extract(None, None).is_empty());
    assert!(extract(Some("فقط عنوان فایل"), Some("")).is_empty());
}

#[test]
fn extracts_anchor_destinations_from_labels_in_document_order_and_deduplicates() {
    let html = r#"<div><a href="https://files.example.com/part2.zip">بخش ۲</a>
        <A HREF='https://files.example.com/part1.zip'><strong>بخش ۱</strong></A>
        <a href="https://files.example.com/part2.zip">دوباره</a></div>"#;
    assert_eq!(
        extract(Some("بخش ۲\nبخش ۱\nدوباره"), Some(html)),
        vec![
            "https://files.example.com/part2.zip",
            "https://files.example.com/part1.zip",
        ]
    );
}

#[test]
fn decodes_entities_once_without_rewriting_signed_url_bytes() {
    let html = r#"<a href="https://cdn.example.com/a%2Fb/file%20name.zip?X-Amz-Signature=fixture%2Bbytes&amp;Expires=1700000000&amp;label=a%2Bb+c&amp;part=a%26b%3Dc">download</a>"#;
    assert_eq!(
        extract(None, Some(html)),
        vec![
            "https://cdn.example.com/a%2Fb/file%20name.zip?X-Amz-Signature=fixture%2Bbytes&Expires=1700000000&label=a%2Bb+c&part=a%26b%3Dc",
        ]
    );
    let once = r#"<a href="https://files.example.com/file.zip?label=a&amp;amp;b">download</a>"#;
    assert_eq!(
        extract(None, Some(once)),
        vec!["https://files.example.com/file.zip?label=a&amp;b"]
    );
}

#[test]
fn decodes_numeric_href_entities_without_accepting_an_obfuscated_script_scheme() {
    let html = r#"<a href="https:&#47;&#47;files.example.com/one.zip?a=1&#x26;b=2">one</a>
        <a href="java&#115;cript:alert(1)">script</a>"#;
    assert_eq!(
        extract(None, Some(html)),
        vec!["https://files.example.com/one.zip?a=1&b=2"]
    );
}

#[test]
fn source_url_resolves_relative_href_but_is_not_itself_a_download() {
    let html = cf_html(
        "https://files.example.com/releases/v1/index.html",
        "<html><body><!--StartFragment-->",
        r#"<a href="../part%202.zip?sig=fixture%2Bbytes&amp;n=1">part</a>
            <a href="/downloads/setup.exe">setup</a>
            <a href="//cdn.example.com/archive.zip">archive</a>"#,
        "<!--EndFragment--></body></html>",
    );
    assert_eq!(
        extract(None, Some(&html)),
        vec![
            "https://files.example.com/releases/part%202.zip?sig=fixture%2Bbytes&n=1",
            "https://files.example.com/downloads/setup.exe",
            "https://cdn.example.com/archive.zip",
        ]
    );
}

#[test]
fn honors_fragment_byte_offsets_and_excludes_anchors_outside_the_copied_selection() {
    let html = cf_html(
        "https://files.example.com/page.html",
        r#"<html><body>پیش‌گفتار فارسی<a href="https://files.example.com/before.zip">before</a>"#,
        r#"<a href="https://files.example.com/selected2.zip">بخش دوم</a><a href="https://files.example.com/selected1.zip">بخش اول</a>"#,
        r#"<a href="https://files.example.com/after.zip">after</a></body></html>"#,
    );
    assert_eq!(
        extract(Some("بخش دوم بخش اول"), Some(&html)),
        vec![
            "https://files.example.com/selected2.zip",
            "https://files.example.com/selected1.zip",
        ]
    );
}

#[test]
fn selected_link_label_imports_its_ancestor_href_but_not_unselected_neighbors() {
    let html = cf_html(
        "https://files.example.com/page.html",
        r#"<html><body><a href='https://files.example.com/part.zip'><!--StartFragment-->"#,
        "بخش اول برای دانلود",
        r#"<!--EndFragment--></a><a href='https://files.example.com/outside.zip'>outside</a></body></html>"#,
    );
    assert_eq!(
        extract(Some("بخش اول برای دانلود"), Some(&html)),
        vec!["https://files.example.com/part.zip"]
    );
}

#[test]
fn empty_and_fragment_only_hrefs_do_not_import_the_source_page() {
    let html = cf_html(
        "https://files.example.com/page.html",
        "<html><body><!--StartFragment-->",
        r#"<a href=''>empty</a><a href='   '>whitespace</a>
            <a href='#'>top</a><a href='#downloads'>jump</a>
            <a href='https://files.example.com/part.zip'>part</a>"#,
        "<!--EndFragment--></body></html>",
    );
    assert_eq!(
        extract(None, Some(&html)),
        vec!["https://files.example.com/part.zip"]
    );
}

#[test]
fn combines_plain_text_urls_with_anchor_links_without_duplicate_tasks() {
    let text = "https://files.example.com/plain.zip\nhttps://files.example.com/shared.zip";
    let html = r#"<a href="https://files.example.com/shared.zip">shared label</a>
        <a href="https://files.example.com/anchor.zip">anchor label</a>"#;
    assert_eq!(
        extract(Some(text), Some(html)),
        vec![
            "https://files.example.com/plain.zip",
            "https://files.example.com/shared.zip",
            "https://files.example.com/anchor.zip",
        ]
    );
}

#[test]
fn ignores_script_comment_embedded_sources_and_non_web_schemes() {
    let html = r#"<!-- <a href="https://files.example.com/comment.zip">comment</a> -->
        <script>const link = '<a href="https://files.example.com/script.zip">script</a>';</script>
        <img src="https://files.example.com/image.zip">
        <iframe src="https://files.example.com/frame.zip"></iframe>
        <a data-href="https://files.example.com/fake.zip">not an href</a>
        <a href="javascript:alert(1)">script</a><a href="data:text/plain,fixture">data</a>
        <a href="mailto:fixture@example.com">mail</a>
        <a href="https://files.example.com/real.zip">real</a>"#;
    assert_eq!(
        extract(None, Some(html)),
        vec!["https://files.example.com/real.zip"]
    );
}

#[test]
fn never_imports_links_containing_login_credentials() {
    let html = r#"<a href="https://fixture:fixture@example.com/file.zip">userinfo</a>
        <a href="https://example.com/file.zip?access_token=fixture">token</a>
        <a href="https://example.com/file.zip?%50assword=fixture">encoded key</a>
        <a href="https://files.example.com/public.zip">public</a>"#;
    let text = "https://fixture:fixture@example.com/plain.zip https://example.com/plain.zip?AUTHORIZATION=fixture";
    assert_eq!(
        extract(Some(text), Some(html)),
        vec!["https://files.example.com/public.zip"]
    );
}

#[test]
fn invalid_cf_html_offsets_never_panic_or_lose_plain_text_fallback() {
    for header in [
        "StartHTML:-1\r\nEndHTML:-1\r\nStartFragment:-1\r\nEndFragment:-1\r\n",
        "StartHTML:bad\r\nEndHTML:999\r\nStartFragment:999\r\nEndFragment:2\r\n",
        "StartHTML:0000000000\r\nEndHTML:18446744073709551616\r\nStartFragment:18446744073709551616\r\nEndFragment:18446744073709551616\r\n",
        "StartFragment:0000000001\r\nEndFragment:0000000002\r\n",
    ] {
        let html = format!("{header}<html><body>متن فارسی</body></html>");
        assert_eq!(
            extract(Some("https://files.example.com/fallback.zip"), Some(&html)),
            vec!["https://files.example.com/fallback.zip"]
        );
    }
}

#[test]
fn byte_offsets_inside_a_multibyte_character_are_safe() {
    let prefix = "<html><body>";
    let mut html = cf_html(
        "https://files.example.com/page.html",
        prefix,
        "ف",
        "</body></html>",
    );
    let field = "StartFragment:";
    let value_start = html.find(field).unwrap() + field.len();
    let original: usize = html[value_start..value_start + 10].parse().unwrap();
    html.replace_range(
        value_start..value_start + 10,
        &format!("{:010}", original + 1),
    );
    assert_eq!(
        extract(Some("https://files.example.com/fallback.zip"), Some(&html)),
        vec!["https://files.example.com/fallback.zip"]
    );
}

#[test]
fn large_selections_are_bounded_to_at_most_five_hundred_links() {
    let html: String = (0..750)
        .map(|index| {
            format!("<a href=\"https://files.example.com/part{index}.zip\">بخش {index}</a>")
        })
        .collect();
    let text = "https://files.example.com/plain.zip";
    let links = extract(Some(text), Some(&html));
    assert!(!links.is_empty());
    assert!(
        links.len() <= 500,
        "one clipboard selection must stay bounded"
    );
    assert_eq!(
        links.first().unwrap(),
        "https://files.example.com/plain.zip"
    );
    assert_eq!(
        links.iter().collect::<std::collections::HashSet<_>>().len(),
        links.len()
    );
    let oversized = format!("{}{}", "ف".repeat(1_000_000), html);
    assert!(extract(None, Some(&oversized)).len() <= 500);
}
