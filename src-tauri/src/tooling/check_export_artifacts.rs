//! Read-only checks for exports of scripts/fixtures/math-export.md.
use std::{collections::HashMap, env, fs::File, io::Read, path::Path};

use lopdf::{Document, Object};
use quick_xml::{events::Event, Reader, XmlVersion};
use serde_json::json;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Attributes = HashMap<String, String>;
const WEB: &str = "https://example.com/?a=1&b=2";
const MAIL: &str = "mailto:reader@example.com";

fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn elements(xml: &str) -> Result<Vec<(String, Attributes)>> {
    let mut reader = Reader::from_str(xml);
    let mut output = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(element) | Event::Empty(element) => {
                let mut attributes = Attributes::new();
                for attr in element.attributes() {
                    let attr = attr?;
                    attributes.insert(
                        String::from_utf8(attr.key.as_ref().to_vec())?,
                        attr.decoded_and_normalized_value(
                            XmlVersion::Explicit1_0,
                            reader.decoder(),
                        )?
                        .into_owned(),
                    );
                }
                output.push((
                    String::from_utf8(element.name().as_ref().to_vec())?,
                    attributes,
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(output)
}

fn check_docx(path: &Path) -> Result<serde_json::Value> {
    let mut archive = zip::ZipArchive::new(File::open(path)?)?;
    let mut xml = |name: &str| -> Result<String> {
        let mut text = String::new();
        archive.by_name(name)?.read_to_string(&mut text)?;
        Ok(text)
    };
    let body = elements(&xml("word/document.xml")?)?;
    let notes = elements(&xml("word/footnotes.xml")?)?;
    let body_rels = elements(&xml("word/_rels/document.xml.rels")?)?;
    let note_rels = elements(&xml("word/_rels/footnotes.xml.rels")?)?;
    let bookmarks: Vec<_> = body
        .iter()
        .filter(|(tag, _)| tag == "w:bookmarkStart")
        .filter_map(|(_, attrs)| attrs.get("w:name"))
        .collect();
    let mut counts = Vec::new();
    for (part, nodes, relationships, expected_math) in [
        ("document", &body, &body_rels, 9),
        ("footnotes", &notes, &note_rels, 2),
    ] {
        let mut rels = HashMap::new();
        for (_, attrs) in relationships
            .iter()
            .filter(|(tag, _)| tag == "Relationship")
        {
            let id = attrs.get("Id").ok_or("relationship has no Id")?;
            require(
                rels.insert(id, attrs).is_none(),
                "duplicate relationship Id",
            )?;
        }
        let math = nodes.iter().filter(|(tag, _)| tag == "m:oMath").count();
        require(
            math == expected_math,
            &format!("{part}: editable formula count {math}, expected {expected_math}"),
        )?;
        let mut urls = Vec::new();
        let mut anchors = 0;
        for (_, attrs) in nodes.iter().filter(|(tag, _)| tag == "w:hyperlink") {
            if let Some(anchor) = attrs.get("w:anchor") {
                require(
                    bookmarks.contains(&anchor),
                    "DOCX link has no body bookmark",
                )?;
                anchors += 1;
            } else {
                let id = attrs
                    .get("r:id")
                    .ok_or("hyperlink has neither anchor nor relationship")?;
                let relation = rels
                    .get(id)
                    .ok_or("hyperlink relationship missing in its own part")?;
                require(
                    relation.get("TargetMode").map(String::as_str) == Some("External"),
                    "invalid hyperlink target mode",
                )?;
                require(
                    relation
                        .get("Type")
                        .is_some_and(|t| t.ends_with("/hyperlink")),
                    "invalid hyperlink relationship type",
                )?;
                urls.push(
                    relation
                        .get("Target")
                        .ok_or("hyperlink target missing")?
                        .as_str(),
                );
            }
        }
        require(
            urls.contains(&WEB),
            "DOCX external query string missing or corrupted",
        )?;
        require(anchors > 0, "DOCX internal link missing")?;
        if part == "footnotes" {
            require(urls.contains(&MAIL), "DOCX footnote mailto link missing")?;
        }
        counts.push(json!({"part": part, "formulas": math, "externalLinks": urls.len(), "anchors": anchors}));
    }
    let images = body.iter().filter(|(tag, _)| tag == "a:blip").count();
    require(images == 2, "DOCX repeated diagrams missing")?;
    Ok(json!({"parts": counts, "diagrams": images}))
}

fn resolved<'a>(pdf: &'a Document, object: &'a Object) -> Result<&'a Object> {
    Ok(pdf.dereference(object)?.1)
}

fn check_pdf(path: &Path) -> Result<serde_json::Value> {
    let pdf = Document::load(path)?;
    let pages = pdf.get_pages();
    require(!pages.is_empty(), "PDF has no pages")?;
    let mut urls = Vec::new();
    let mut internal = 0;
    for page in pages.values() {
        for annotation in pdf.get_page_annotations(*page)? {
            if let Ok(action) = annotation.get(b"A") {
                let action = resolved(&pdf, action)?.as_dict()?;
                if let Ok(uri) = action.get(b"URI") {
                    urls.push(String::from_utf8(resolved(&pdf, uri)?.as_str()?.to_vec())?);
                }
            }
            if let Ok(destination) = annotation.get(b"Dest") {
                let destination = resolved(&pdf, destination)?;
                let destination = match destination {
                    Object::Name(name) | Object::String(name, _) => {
                        let destinations =
                            resolved(&pdf, pdf.catalog()?.get(b"Dests")?)?.as_dict()?;
                        resolved(&pdf, destinations.get(name)?)?
                    }
                    _ => destination,
                };
                let array = if let Object::Dictionary(dict) = destination {
                    resolved(&pdf, dict.get(b"D")?)?.as_array()?
                } else {
                    destination.as_array()?
                };
                let page = array
                    .first()
                    .ok_or("empty PDF destination")?
                    .as_reference()?;
                require(
                    pages.values().any(|id| *id == page),
                    "PDF destination page does not exist",
                )?;
                internal += 1;
            }
        }
    }
    require(
        urls.iter().any(|url| url == WEB),
        "PDF external query string missing or corrupted",
    )?;
    require(
        urls.iter().any(|url| url == MAIL),
        "PDF mailto link missing",
    )?;
    require(internal >= 3, "PDF footnote or heading links missing")?;
    Ok(json!({"pages": pages.len(), "externalLinks": urls.len(), "internalLinks": internal}))
}

fn check_png(path: &Path) -> Result<serde_json::Value> {
    let mut dimensions = Vec::new();
    let names = ["0001-Formula mapping.png", "0002-Matrices and links.png"];
    let files = std::fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
    require(
        files.len() == names.len(),
        "expected exactly two chapter images",
    )?;
    for name in names {
        let size = imagesize::size(path.join(name))?;
        require(
            size.width >= 500 && size.height >= 200,
            "chapter image dimensions too small",
        )?;
        dimensions.push(json!({"file": name, "width": size.width, "height": size.height}));
    }
    Ok(json!(dimensions))
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    require(
        args.len() == 3,
        "usage: check_export_artifacts <directory> <stem> <formats>",
    )?;
    let dir = Path::new(&args[0]);
    let stem = args[1].to_str().ok_or("non-UTF8 stem")?;
    let mut report = serde_json::Map::new();
    for format in args[2].to_str().ok_or("non-UTF8 formats")?.split(',') {
        let value = match format {
            "docx" => check_docx(&dir.join(format!("{stem}.docx")))?,
            "pdf" => check_pdf(&dir.join(format!("{stem}.pdf")))?,
            "png" => check_png(&dir.join(stem))?,
            "html" => continue, // Parsed by jsdom in the Node entry point.
            _ => return Err(format!("unknown format: {format}").into()),
        };
        report.insert(format.into(), value);
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
