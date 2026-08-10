use std::collections::HashSet;

pub fn sanitize_html(raw: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder.add_tags(["input", "mark"]);
    builder.add_tag_attributes("input", ["type", "checked", "disabled"]);
    builder.add_generic_attributes(["class", "id", "title"]);
    builder.url_schemes(HashSet::from(["http", "https", "marklite"]));
    builder.clean(raw).to_string()
}

#[cfg(test)]
mod tests {
    use super::sanitize_html;

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
        ] {
            let cleaned = sanitize_html(&format!(r#"<a href="{scheme}">bad</a>"#));
            assert!(!cleaned.contains("href="));
        }
    }
}
