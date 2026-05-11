pub fn sanitize_html(raw: &str) -> String {
    let mut builder = ammonia::Builder::default();
    builder.add_tags(["input", "mark"]);
    builder.add_tag_attributes("input", ["type", "checked", "disabled"]);
    builder.add_generic_attributes(["class", "id", "title"]);
    builder.clean(raw).to_string()
}
