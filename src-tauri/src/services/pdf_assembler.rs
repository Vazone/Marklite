use crate::models::app_error::AppError;
use lopdf::{dictionary, xref::XrefType, Dictionary, Document, Object, ObjectId};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_PART_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OUTPUT_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_OBJECTS: u32 = 2_000_000;

/// Keep just one parsed part in memory. Serialized objects are appended to disk;
/// the final cross-reference table and page tree join the parts without keeping
/// their fonts, content streams, or images resident across successive parts.
pub(crate) struct PdfAssembler {
    file: File,
    next_id: u32,
    offsets: Vec<Option<(u64, u16)>>,
    page_roots: Vec<Object>,
    page_count: usize,
    destinations: BTreeMap<Vec<u8>, Object>,
    links: Vec<(ObjectId, Dictionary, Vec<u8>)>,
    info: Option<ObjectId>,
}

impl PdfAssembler {
    pub(crate) fn create(path: &Path) -> Result<Self, AppError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(merge_io)?;
        Ok(Self {
            file,
            next_id: 3,
            offsets: vec![None; 3],
            page_roots: Vec::new(),
            page_count: 0,
            destinations: BTreeMap::new(),
            links: Vec::new(),
            info: None,
        })
    }

    pub(crate) fn append(&mut self, path: &Path) -> Result<(), AppError> {
        if std::fs::metadata(path).map_err(merge_io)?.len() > MAX_PART_BYTES {
            return Err(AppError::new(
                "PDF_PART_TOO_LARGE",
                "单批 PDF 超过 64 MiB，未提交输出",
            ));
        }
        let mut part = Document::load(path).map_err(merge_error)?;
        if part.is_encrypted() {
            return Err(merge_error("平台 PDF 不应加密"));
        }
        let pages = part.get_pages().len();
        if pages == 0 {
            return Err(merge_error("平台 PDF 不含页面"));
        }
        let required = u32::try_from(part.objects.len()).map_err(merge_error)?;
        if self.next_id.saturating_add(required) > MAX_OBJECTS {
            return Err(merge_error("PDF 对象数量超过导出预算"));
        }
        part.renumber_objects_with(self.next_id);
        self.next_id = part
            .max_id
            .checked_add(1)
            .ok_or_else(|| merge_error("PDF 对象编号溢出"))?;
        if self.info.is_none() {
            self.info = part
                .trailer
                .get(b"Info")
                .and_then(Object::as_reference)
                .ok();
        }
        let root = part
            .catalog()
            .and_then(|catalog| catalog.get(b"Pages"))
            .and_then(Object::as_reference)
            .map_err(merge_error)?;
        part.get_object_mut(root)
            .and_then(Object::as_dict_mut)
            .map_err(merge_error)?
            .set("Parent", (2, 0));
        self.collect_links(&mut part)?;
        self.page_roots.push(Object::Reference(root));
        self.page_count += pages;
        self.write_part(&mut part)
    }

    fn collect_links(&mut self, part: &mut Document) -> Result<(), AppError> {
        if let Ok(destinations) = part.catalog().and_then(|catalog| catalog.get(b"Dests")) {
            let dictionary = match destinations {
                Object::Reference(id) => part.get_dictionary(*id).map_err(merge_error)?,
                Object::Dictionary(dictionary) => dictionary,
                _ => return Err(merge_error("PDF 锚点字典无效")),
            };
            for (name, value) in dictionary.iter() {
                self.destinations
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
        if let Ok(names) = part.catalog().and_then(|catalog| catalog.get(b"Names")) {
            let names = dereference(part, names)?.as_dict().map_err(merge_error)?;
            if let Ok(root) = names.get(b"Dests") {
                let mut pending = vec![root];
                while let Some(node) = pending.pop() {
                    let dictionary = dereference(part, node)?.as_dict().map_err(merge_error)?;
                    if let Ok(names) = dictionary.get(b"Names") {
                        let names = dereference(part, names)?.as_array().map_err(merge_error)?;
                        if names.len() % 2 != 0 {
                            return Err(merge_error("PDF 锚点名称树无效"));
                        }
                        for pair in names.chunks_exact(2) {
                            let key = pair[0].as_str().map_err(merge_error)?.to_vec();
                            self.destinations
                                .entry(key)
                                .or_insert_with(|| pair[1].clone());
                        }
                    }
                    if let Ok(kids) = dictionary.get(b"Kids") {
                        pending.extend(kids.as_array().map_err(merge_error)?.iter());
                    }
                }
            }
        }
        let mut probes = HashSet::new();
        for (id, object) in &part.objects {
            let Ok(dictionary) = object.as_dict() else {
                continue;
            };
            if dictionary.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Link") {
                continue;
            }
            if dictionary
                .get(b"Dest")
                .is_ok_and(|dest| matches!(dest, Object::Name(_) | Object::String(_, _)))
            {
                probes.insert(*id);
            }
            if let Ok(uri) = dictionary
                .get(b"A")
                .and_then(Object::as_dict)
                .and_then(|action| action.get(b"URI"))
                .and_then(Object::as_str)
            {
                if let Some(fragment) = String::from_utf8_lossy(uri)
                    .strip_prefix(super::export_html_writer::PDF_ANCHOR_URL)
                {
                    let name = percent_encoding::percent_decode_str(fragment).collect::<Vec<_>>();
                    self.links.push((*id, dictionary.clone(), name));
                }
            }
        }
        // Synthetic self-links exist only to make platform print publish names.
        // Remove their annotations; real cross-part links are rewritten below.
        for page in part.get_pages().values() {
            if let Ok(Object::Array(annotations)) = part
                .get_dictionary_mut(*page)
                .and_then(|page| page.get_mut(b"Annots"))
            {
                annotations
                    .retain(|item| item.as_reference().map_or(true, |id| !probes.contains(&id)));
            }
        }
        Ok(())
    }

    fn write_part(&mut self, part: &mut Document) -> Result<(), AppError> {
        let base = self.file.seek(SeekFrom::End(0)).map_err(merge_io)?;
        part.reference_table.cross_reference_type = XrefType::CrossReferenceTable;
        part.save_to(&mut self.file).map_err(merge_io)?;
        let end = self.file.stream_position().map_err(merge_io)?;
        if end > MAX_OUTPUT_BYTES {
            return Err(merge_error("PDF 总字节超过 1 GiB 导出预算"));
        }
        self.file
            .seek(SeekFrom::Start(end.saturating_sub(128).max(base)))
            .map_err(merge_io)?;
        let mut bytes = Vec::new();
        self.file.read_to_end(&mut bytes).map_err(merge_io)?;
        let tail = String::from_utf8_lossy(&bytes);
        let local_xref: u64 = tail
            .rsplit_once("startxref\n")
            .and_then(|(_, tail)| tail.lines().next())
            .ok_or_else(|| merge_error("缺少合并交叉引用"))?
            .trim()
            .parse()
            .map_err(merge_error)?;
        if local_xref >= end - base {
            return Err(merge_error("交叉引用偏移越界"));
        }
        self.file
            .seek(SeekFrom::Start(base + local_xref))
            .map_err(merge_io)?;
        let mut reader = BufReader::new(&mut self.file);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(merge_io)?;
        if line.trim() != "xref" {
            return Err(merge_error("合并交叉引用类型不正确"));
        }
        loop {
            line.clear();
            reader.read_line(&mut line).map_err(merge_io)?;
            if line.starts_with("trailer") {
                break;
            }
            let header = line.split_whitespace().collect::<Vec<_>>();
            if header.len() != 2 {
                return Err(merge_error("合并交叉引用段无效"));
            }
            let first: usize = header[0].parse().map_err(merge_error)?;
            let count: usize = header[1].parse().map_err(merge_error)?;
            let limit = first
                .checked_add(count)
                .filter(|limit| *limit <= MAX_OBJECTS as usize)
                .ok_or_else(|| merge_error("交叉引用数量超过预算"))?;
            self.offsets.resize(self.offsets.len().max(limit), None);
            for id in first..limit {
                line.clear();
                reader.read_line(&mut line).map_err(merge_io)?;
                let fields = line.split_whitespace().collect::<Vec<_>>();
                if fields.len() != 3 {
                    return Err(merge_error("合并交叉引用条目无效"));
                }
                if fields[2] == "n" {
                    let offset: u64 = fields[0].parse().map_err(merge_error)?;
                    let generation: u16 = fields[1].parse().map_err(merge_error)?;
                    self.offsets[id] = Some((base + offset, generation));
                }
            }
        }
        self.file.seek(SeekFrom::End(0)).map_err(merge_io)?;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<(), AppError> {
        if self.page_count == 0 {
            return Err(merge_error("PDF 合并没有页面"));
        }
        let mut trailer = Document::with_version("1.7");
        let mut catalog = dictionary! {"Type" => "Catalog", "Pages" => (2,0)};
        let mut destinations = Dictionary::new();
        for (name, value) in &self.destinations {
            destinations.set(name.clone(), value.clone());
        }
        if !destinations.is_empty() {
            catalog.set("Dests", destinations);
        }
        trailer.objects.insert((1, 0), catalog.into());
        for (id, mut link, name) in self.links.drain(..) {
            link.remove(b"A");
            if self.destinations.contains_key(&name) {
                link.set("Dest", Object::Name(name));
            }
            trailer.objects.insert(id, link.into());
        }
        trailer.objects.insert((2,0), dictionary! {"Type" => "Pages", "Kids" => std::mem::take(&mut self.page_roots), "Count" => self.page_count as i64}.into());
        trailer.max_id = self.next_id - 1;
        trailer.trailer.set("Root", (1, 0));
        self.write_part(&mut trailer)?;
        writeln!(self.file).map_err(merge_io)?;
        let xref = self.file.stream_position().map_err(merge_io)?;
        writeln!(
            self.file,
            "xref\n0 {}\n0000000000 65535 f ",
            self.offsets.len()
        )
        .map_err(merge_io)?;
        for entry in self.offsets.iter().skip(1) {
            match entry {
                Some((offset, generation)) => {
                    writeln!(self.file, "{offset:010} {generation:05} n ")
                }
                None => writeln!(self.file, "0000000000 00000 f "),
            }
            .map_err(merge_io)?;
        }
        let info = self
            .info
            .map(|(id, generation)| format!(" /Info {id} {generation} R"))
            .unwrap_or_default();
        write!(
            self.file,
            "trailer\n<< /Root 1 0 R /Size {}{info} >>\nstartxref\n{xref}\n%%EOF\n",
            self.offsets.len()
        )
        .map_err(merge_io)?;
        if self.file.stream_position().map_err(merge_io)? > MAX_OUTPUT_BYTES {
            return Err(merge_error("PDF 总字节超过 1 GiB 导出预算"));
        }
        self.file.sync_all().map_err(merge_io)
    }
}
fn dereference<'a>(part: &'a Document, object: &'a Object) -> Result<&'a Object, AppError> {
    match object {
        Object::Reference(id) => part.get_object(*id).map_err(merge_error),
        _ => Ok(object),
    }
}
fn merge_error(error: impl std::fmt::Display) -> AppError {
    AppError::new("PDF_MERGE_FAILED", format!("合并 PDF 失败：{error}"))
}
fn merge_io(error: io::Error) -> AppError {
    merge_error(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Stream;
    #[test]
    fn merges_in_order_preserving_page_inheritance_and_local_links() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("merged.pdf");
        let mut assembler = PdfAssembler::create(&output).unwrap();
        for number in 0..2 {
            let mut doc = Document::with_version("1.7");
            let pages = doc.new_object_id();
            let stream = doc.add_object(Stream::new(
                dictionary! {},
                format!("part {number}").into_bytes(),
            ));
            let page =
                doc.add_object(dictionary! {"Type"=>"Page", "Parent"=>pages, "Contents"=>stream});
            let link = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Link", "Dest"=>vec![Object::Reference(page), Object::Name(b"Fit".to_vec())]});
            doc.get_object_mut(page)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Annots", vec![Object::Reference(link)]);
            doc.objects.insert(pages, dictionary! {"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1, "MediaBox"=>vec![0.into(),0.into(),200.into(),300.into()]}.into());
            let mut destinations = Dictionary::new();
            destinations.set(
                format!("target{number}"),
                vec![Object::Reference(page), Object::Name(b"Fit".to_vec())],
            );
            if number == 0 {
                let cross = doc.add_object(dictionary! {"Type"=>"Annot", "Subtype"=>"Link", "A"=>dictionary! {"S"=>"URI", "URI"=>Object::string_literal(format!("{}target1", super::super::export_html_writer::PDF_ANCHOR_URL))}});
                doc.get_object_mut(page)
                    .unwrap()
                    .as_dict_mut()
                    .unwrap()
                    .set(
                        "Annots",
                        vec![Object::Reference(link), Object::Reference(cross)],
                    );
            }
            let catalog = doc
                .add_object(dictionary! {"Type"=>"Catalog", "Pages"=>pages, "Dests"=>destinations});
            doc.trailer.set("Root", catalog);
            let part = dir.path().join(format!("{number}.pdf"));
            doc.save(&part).unwrap();
            assembler.append(&part).unwrap();
        }
        assembler.finish().unwrap();
        let merged = Document::load(&output).unwrap();
        let pages = merged.get_pages();
        assert_eq!(pages.len(), 2);
        for (index, page) in pages.values().enumerate() {
            assert_eq!(
                String::from_utf8(merged.get_page_content(*page).unwrap())
                    .unwrap()
                    .trim(),
                format!("part {index}")
            );
            let parent = merged
                .get_dictionary(*page)
                .unwrap()
                .get(b"Parent")
                .unwrap()
                .as_reference()
                .unwrap();
            assert_eq!(
                merged
                    .get_dictionary(parent)
                    .unwrap()
                    .get(b"MediaBox")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
            let annotation = merged.get_page_annotations(*page).unwrap();
            assert_eq!(
                annotation[0].get(b"Dest").unwrap().as_array().unwrap()[0]
                    .as_reference()
                    .unwrap(),
                *page
            );
            if index == 0 {
                assert_eq!(
                    annotation[1].get(b"Dest").unwrap().as_name().unwrap(),
                    b"target1"
                );
                assert!(annotation[1].get(b"A").is_err());
            }
        }
        let target = merged
            .catalog()
            .unwrap()
            .get(b"Dests")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"target1")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_reference()
            .unwrap();
        assert_eq!(target, pages[&2]);
    }
}
