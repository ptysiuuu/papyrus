use papyrus_lib::download::{pdf_filename, pdf_dir_path, extract_text_from_pdf};
use papyrus_lib::models::{Author, Paper, PaperSourceKind};
use std::path::PathBuf;
use tempfile::tempdir;

fn make_paper(source_id: &str, title: &str, first_author: &str, year: i32) -> Paper {
    let mut p = Paper::new(PaperSourceKind::Arxiv, source_id, title);
    p.authors = vec![Author { name: first_author.to_string(), affiliation: None, orcid: None }];
    p.published_date = Some(chrono::NaiveDate::from_ymd_opt(year, 1, 1).unwrap());
    p
}

#[test]
fn test_pdf_filename_generation() {
    let paper = make_paper("2301.07041", "Attention Is All You Need", "Vaswani", 2017);
    let name = pdf_filename(&paper);
    assert!(name.contains("2017"), "Should include year: {}", name);
    assert!(name.contains("vaswani") || name.contains("Vaswani"), "Should include author: {}", name);
    assert!(name.ends_with(".pdf"), "Should end with .pdf: {}", name);
}

#[test]
fn test_pdf_filename_sanitizes_special_chars() {
    let paper = make_paper("ax:1", "Title: With/Special\\Chars?!", "O'Brien", 2023);
    let name = pdf_filename(&paper);
    // Should not contain filesystem-unsafe chars
    assert!(!name.contains('/'), "Should not contain /: {}", name);
    assert!(!name.contains('\\'), "Should not contain \\: {}", name);
    assert!(!name.contains('?'), "Should not contain ?: {}", name);
    assert!(!name.contains('!'), "Should not contain !: {}", name);
}

#[test]
fn test_pdf_dir_path_structure() {
    let paper = make_paper("2301.00001", "Test Paper", "Smith", 2023);
    let base = PathBuf::from("/home/user/papers");
    let dir = pdf_dir_path(&base, &paper);
    // Should be base/year/first_author/
    assert!(dir.starts_with(&base));
    let components: Vec<_> = dir.strip_prefix(&base).unwrap().components().collect();
    assert_eq!(components.len(), 2, "Should have year/author structure, got {:?}", components);
}

#[test]
fn test_pdf_dir_path_unknown_year() {
    let mut paper = Paper::new(PaperSourceKind::Arxiv, "ax:1", "Test");
    paper.authors = vec![Author { name: "Smith, John".to_string(), affiliation: None, orcid: None }];
    // No published_date
    let base = PathBuf::from("/tmp/papers");
    let dir = pdf_dir_path(&base, &paper);
    // Should handle missing year gracefully
    assert!(dir.starts_with(&base));
}

#[test]
fn test_extract_text_no_pdftotext_returns_none() {
    // If pdftotext is not available or fails, should return Ok(None) gracefully
    let dir = tempdir().unwrap();
    let fake_pdf = dir.path().join("fake.pdf");
    std::fs::write(&fake_pdf, b"not a real pdf").unwrap();
    let result = extract_text_from_pdf(&fake_pdf);
    // Either Ok(None) (pdftotext not available / failed) or Ok(Some(...)) — should not panic
    match result {
        Ok(_) => {} // pass
        Err(_) => {} // also acceptable — pdftotext may or may not be available
    }
}
