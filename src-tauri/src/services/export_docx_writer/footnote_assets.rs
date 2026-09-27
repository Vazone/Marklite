//! Footnotes own their relationship scope; docx-rs does not package these assets.
use docx_rs::{HyperlinkData, Pic};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(super) fn pack(
    package: &mut docx_rs::XMLDocx,
    images: Vec<Pic>,
    links: Vec<HyperlinkData>,
) -> Option<String> {
    if images.is_empty() && links.is_empty() {
        return None;
    }
    let mut media = package
        .media
        .iter()
        .map(|(id, bytes)| (Sha256::digest(bytes), id.clone()))
        .collect::<HashMap<_, _>>();
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for image in images {
        let digest = Sha256::digest(&image.image);
        let target = media.entry(digest).or_insert_with(|| {
            package.media.push((image.id.clone(), image.image));
            image.id.clone()
        });
        rels.push_str(&format!(r#"<Relationship Id="{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/{}.png"/>"#, image.id, target));
    }
    for link in links {
        if let HyperlinkData::External { rid, path } = link {
            // docx-rs Hyperlink::new has already XML-escaped the target;
            // re-escaping here would corrupt query strings containing ampersands.
            rels.push_str(&format!(r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{path}" TargetMode="External"/>"#));
        }
    }
    rels.push_str("</Relationships>");
    Some(rels)
}
