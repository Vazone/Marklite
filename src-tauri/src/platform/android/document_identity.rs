use percent_encoding::percent_decode_str;
use url::Url;

// Direct and tree-scoped SAF URIs can name one provider document. This key is
// only for equality; it never grants access or replaces ContentResolver checks.
pub(crate) fn document_key(uri: &str) -> Option<String> {
    let parsed = Url::parse(uri).ok()?;
    if parsed.scheme() != "content" {
        return None;
    }
    let authority = parsed.host_str()?;
    let segments: Vec<_> = parsed.path_segments()?.collect();
    if segments.len() < 2 || segments[segments.len() - 2] != "document" {
        return None;
    }
    let document_id = percent_decode_str(segments.last()?).decode_utf8().ok()?;
    if document_id.is_empty() {
        return None;
    }
    Some(format!("{authority}\0{document_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_and_tree_uris_share_an_identity_only_for_the_same_provider_document() {
        let direct = "content://com.android.externalstorage.documents/document/primary%3ADocuments%2FNotes%2FIndex.md";
        let tree = "content://com.android.externalstorage.documents/tree/primary%3ADocuments%2FNotes/document/primary%3ADocuments%2FNotes%2FIndex.md";
        assert_eq!(document_key(direct), document_key(tree));
        assert_ne!(
            document_key(direct),
            document_key(
                "content://other.provider/document/primary%3ADocuments%2FNotes%2FIndex.md"
            )
        );
        assert_ne!(
            document_key(direct),
            document_key("content://com.android.externalstorage.documents/document/primary%3ADocuments%2FOther%2FIndex.md")
        );
        assert_eq!(document_key("content://other.provider/tree/root"), None);
    }
}
