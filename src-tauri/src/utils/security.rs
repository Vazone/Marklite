use std::borrow::Cow;
use std::collections::HashSet;

pub fn sanitize_html(raw: &str) -> String {
    sanitize_html_with_policy(raw, false)
}

pub fn strip_export_raw_images(raw: &str) -> (String, bool) {
    // Detection uses the HTML parser so img-like text in attributes or scripts
    // does not produce a warning. Other raw tags stay intact until the final,
    // whole-document sanitizer can see their opening and closing events together.
    if !raw
        .as_bytes()
        .windows(4)
        .any(|prefix| prefix.eq_ignore_ascii_case(b"<img"))
        || !sanitize_html(raw).contains("<img")
    {
        return (raw.to_owned(), false);
    }
    let bytes = raw.as_bytes();
    let mut output = String::with_capacity(raw.len());
    let mut copied_until = 0;
    let mut index = 0;
    let mut removed = false;
    while index < bytes.len() {
        if bytes[index] != b'<' {
            index += 1;
            continue;
        }
        if raw[index..].starts_with("<!--") {
            index = raw[index + 4..]
                .find("-->")
                .map(|end| index + 4 + end + 3)
                .unwrap_or(bytes.len());
            continue;
        }
        if !bytes
            .get(index + 1)
            .is_some_and(|next| next.is_ascii_alphabetic() || matches!(next, b'/' | b'!' | b'?'))
        {
            index += 1;
            continue;
        }
        let is_image = bytes
            .get(index + 1..index + 4)
            .is_some_and(|name| name.eq_ignore_ascii_case(b"img"))
            && bytes
                .get(index + 4)
                .is_some_and(|next| next.is_ascii_whitespace() || matches!(next, b'/' | b'>'));
        let end = html_tag_end(bytes, index + 1);
        if is_image {
            output.push_str(&raw[copied_until..index]);
            copied_until = end;
            removed = true;
        }
        index = end;
    }
    output.push_str(&raw[copied_until..]);
    (output, removed)
}

pub(crate) fn html_tag_end(bytes: &[u8], start: usize) -> usize {
    let mut quote = None;
    for (index, byte) in bytes.iter().enumerate().skip(start) {
        match (quote, byte) {
            (Some(expected), current) if *current == expected => quote = None,
            (None, b'\'' | b'"') => quote = Some(*byte),
            (None, b'>') => return index + 1,
            (None, b'<') => return index,
            _ => {}
        }
    }
    bytes.len()
}

fn sanitize_html_with_policy(raw: &str, remove_images: bool) -> String {
    let mut builder = ammonia::Builder::default();
    if remove_images {
        builder.rm_tags(["img"]);
    }
    builder.add_tags(["input", "mark"]);
    builder.add_tag_attributes("input", ["type", "checked", "disabled"]);
    builder.add_tag_attributes("th", ["style"]);
    builder.add_tag_attributes("td", ["style"]);
    builder.add_generic_attributes(["class", "id", "title"]);
    builder.attribute_filter(|element, attribute, value| {
        if attribute != "style" {
            return Some(Cow::Borrowed(value));
        }
        if matches!(element, "th" | "td")
            && matches!(
                value,
                "text-align: left" | "text-align: center" | "text-align: right"
            )
        {
            Some(Cow::Borrowed(value))
        } else {
            None
        }
    });
    builder.url_schemes(HashSet::from(["http", "https", "marklite"]));
    builder.clean(raw).to_string()
}

#[cfg(test)]
mod tests {
    use super::{sanitize_html, strip_export_raw_images};

    #[test]
    fn keeps_typed_transport_and_http_but_strips_dangerous_raw_schemes() {
        let safe = sanitize_html(
            r#"<a href="marklite:relative%2Emd">local</a><a href="https://example.com">web</a>"#,
        );
        assert!(safe.contains("href=\"marklite:relative%2Emd\""));
        assert!(safe.contains("href=\"https://example.com\""));

        for scheme in [
            "javascript:alert(1)",
            "data:text/html,bad",
            "file:///C:/bad.md",
            "ftp://example.com",
            "mailto:writer@example.com",
        ] {
            let cleaned = sanitize_html(&format!(r#"<a href="{scheme}">bad</a>"#));
            assert!(!cleaned.contains("href="));
        }
    }

    #[test]
    fn permits_fixed_table_alignment_and_rejects_other_styles() {
        let cleaned = sanitize_html(
            r#"<table><tr><th style="text-align: center">ok</th><td style="color: red">bad</td></tr></table><p style="text-align: left">bad</p>"#,
        );

        assert!(cleaned.contains(r#"<th style="text-align: center">ok</th>"#));
        assert!(!cleaned.contains("color: red"));
        assert!(!cleaned.contains(r#"<p style="#));
    }

    #[test]
    fn export_raw_html_removes_images_but_keeps_safe_links() {
        let (cleaned, removed) = strip_export_raw_images(
            r#"<a href="https://example.com">site</a><img src="https://example.com/a.png" srcset="local.png 1x, https://example.com/b.png 2x" alt="diagram">"#,
        );

        assert!(removed);
        assert!(sanitize_html(&cleaned).contains(r#"href="https://example.com""#));
        assert!(!cleaned.contains("<img"));
        assert!(!cleaned.contains("src="));
        assert!(!cleaned.contains("srcset="));
    }

    #[test]
    fn raw_image_filter_preserves_neighboring_tags_and_quoted_delimiters() {
        let source = "<em>before</em><IMG src='https://example.org/a>b.png' onerror='bad()'><strong>after</strong>";
        let (filtered, removed) = strip_export_raw_images(source);
        assert!(removed);
        assert_eq!(filtered, "<em>before</em><strong>after</strong>");

        let (comment, removed) = strip_export_raw_images("<!-- <img src='fake'> --><em>ok</em>");
        assert!(!removed);
        assert!(comment.contains("<!-- <img src='fake'> -->"));
    }
}
