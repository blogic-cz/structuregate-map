//! `docx.rs` over a Word file built here: a zip (one entry stored, one deflated), a localized heading style, a bold
//! subsection, a table, and a code run in a monospace font.
use super::*;

/// A zip of `(name, bytes, deflate)` entries, as Word writes one: local headers, then the central directory.
pub fn zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes, deflate) in entries {
        let data = if *deflate { miniz_oxide::deflate::compress_to_vec(bytes, 6) } else { bytes.to_vec() };
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        let head = |sig: &[u8], out: &mut Vec<u8>| {
            out.extend(sig);
            out.extend([20u8, 0, 0, 0]);
            out.extend(method.to_le_bytes());
            out.extend([0u8; 8]);
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((bytes.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
        };
        head(&[0x50, 0x4b, 0x03, 0x04], &mut out);
        out.extend([0u8, 0]);
        out.extend(name.as_bytes());
        out.extend(&data);
        central.extend([0x50, 0x4b, 0x01, 0x02, 20, 0]);
        head(&[], &mut central);
        central.extend([0u8; 12]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let at = out.len() as u32;
    let n = entries.len() as u16;
    out.extend(&central);
    out.extend([0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    out.extend(n.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend([0u8, 0]);
    out
}

const STYLES: &str = r#"<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:styleId="Nadpis2"><w:name w:val="heading 2"/></w:style><w:style w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#;

pub const BODY: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Nadpis2"/></w:pPr><w:r><w:t>Item codes</w:t></w:r></w:p>
<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hardware line</w:t></w:r></w:p>
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Item code</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Name</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>7</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t xml:space="preserve">Alpha </w:t></w:r><w:r><w:t>laptop</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
<w:p><w:r><w:t xml:space="preserve">Call </w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Consolas"/></w:rPr><w:t>GET /api/items</w:t></w:r><w:r><w:rPr><w:b w:val="0"/></w:rPr><w:t>.</w:t></w:r></w:p>
</w:body></w:document>"#;

#[test]
fn a_word_file_is_the_blocks_a_snapshot_is() {
    let file = zip(&[("[Content_Types].xml", b"<Types/>", false), ("word/styles.xml", STYLES.as_bytes(), false), ("word/document.xml", BODY.as_bytes(), true)]);
    let blocks = blocks(&file).unwrap();
    assert_eq!(blocks[0], Block::Heading { level: 2, text: "Item codes".into() });
    assert!(blocks.contains(&Block::Para { text: "Hardware line".into(), bold: true }));
    assert!(blocks.contains(&Block::Table { rows: vec![(3, vec!["Item code".into(), "Name".into()]), (3, vec!["7".into(), "Alpha laptop".into()])], head: false }));
    assert!(blocks.contains(&Block::Para { text: "Call GET /api/items.".into(), bold: false }));
    assert!(blocks.contains(&Block::Text { at: 4, text: "GET /api/items".into(), code: true }));
    let found = super::super::reader::section(&blocks, "item codes", 1).unwrap();
    assert_eq!(found[0].subsection, "Hardware line");
}

#[test]
fn a_file_that_is_no_word_zip_says_so() {
    assert!(blocks(b"not a zip").unwrap_err().contains("no zip"));
    assert!(blocks(&zip(&[("a.txt", b"x", false)])).unwrap_err().contains("not a Word document"));
}

#[test]
fn a_zero_width_space_inside_a_word_run_is_no_part_of_the_text() {
    let body = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>\
<w:p><w:r><w:t>Call /api\u{200B}/items\u{200B}/run now.</w:t></w:r></w:p></w:body></w:document>";
    let read = blocks(&zip(&[("word/document.xml", body.as_bytes(), true)])).unwrap();
    assert!(read.contains(&Block::Text { at: 1, text: "Call /api/items/run now.".into(), code: false }), "{read:?}");
}
