use std::io::Write;
use std::path::Path;

use crate::models::Paper;

pub enum ExportFormat {
    Json,
    Csv,
    BibTeX,
}

impl ExportFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Some(ExportFormat::Json),
            Some("csv") => Some(ExportFormat::Csv),
            Some("bib") | Some("bibtex") => Some(ExportFormat::BibTeX),
            _ => None,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "csv" => Some(ExportFormat::Csv),
            "bibtex" | "bib" => Some(ExportFormat::BibTeX),
            _ => None,
        }
    }
}

pub fn export_papers(
    papers: &[Paper],
    format: &ExportFormat,
    writer: &mut dyn Write,
) -> anyhow::Result<()> {
    match format {
        ExportFormat::Json => export_json(papers, writer),
        ExportFormat::Csv => export_csv(papers, writer),
        ExportFormat::BibTeX => export_bibtex(papers, writer),
    }
}

fn export_json(papers: &[Paper], writer: &mut dyn Write) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(papers)?;
    writer.write_all(json.as_bytes())?;
    writeln!(writer)?;
    Ok(())
}

fn export_csv(papers: &[Paper], writer: &mut dyn Write) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_writer(writer);
    wtr.write_record([
        "source",
        "source_id",
        "title",
        "authors",
        "date",
        "doi",
        "arxiv_id",
        "journal",
        "categories",
        "citations",
        "pdf_url",
        "code_url",
        "is_open_access",
    ])?;
    for paper in papers {
        let authors = paper
            .authors
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        let categories = paper.categories.join("; ");
        let date = paper
            .published_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        wtr.write_record([
            paper.source.to_string().as_str(),
            paper.source_id.as_str(),
            paper.title.as_str(),
            authors.as_str(),
            date.as_str(),
            paper.doi.as_deref().unwrap_or(""),
            paper.arxiv_id.as_deref().unwrap_or(""),
            paper.journal.as_deref().unwrap_or(""),
            categories.as_str(),
            paper
                .citation_count
                .map(|c| c.to_string())
                .as_deref()
                .unwrap_or(""),
            paper.pdf_url.as_deref().unwrap_or(""),
            paper.code_url.as_deref().unwrap_or(""),
            if paper.is_open_access { "true" } else { "false" },
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn paper_to_bibtex(paper: &Paper) -> String {
    let key = bibtex_key(paper);
    let entry_type = if paper.is_peer_reviewed && paper.journal.is_some() {
        "article"
    } else {
        "misc"
    };

    let authors = paper
        .authors
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(" and ");
    let year = paper
        .published_date
        .map(|d| d.format("%Y").to_string())
        .unwrap_or_else(|| "n.d.".to_string());

    let mut fields: Vec<String> = Vec::new();
    fields.push(format!("  title        = {{{}}}", paper.title));
    if !authors.is_empty() {
        fields.push(format!("  author       = {{{}}}", authors));
    }
    fields.push(format!("  year         = {{{}}}", year));
    if let Some(journal) = &paper.journal {
        fields.push(format!("  journal      = {{{}}}", journal));
    }
    if let Some(doi) = &paper.doi {
        fields.push(format!("  doi          = {{{}}}", doi));
    }
    if let Some(arxiv_id) = &paper.arxiv_id {
        fields.push(format!("  eprint       = {{{}}}", arxiv_id));
        fields.push(format!("  archivePrefix= {{arXiv}}"));
    }
    if let Some(cat) = paper.categories.first() {
        fields.push(format!("  primaryClass = {{{}}}", cat));
    }
    let doi_url = paper.doi.as_ref().map(|d| format!("https://doi.org/{}", d));
    let url = paper.html_url.as_deref().or(doi_url.as_deref());
    if let Some(u) = url {
        fields.push(format!("  url          = {{{}}}", u));
    }

    format!("@{}{{{},\n{}\n}}", entry_type, key, fields.join(",\n"))
}

fn export_bibtex(papers: &[Paper], writer: &mut dyn Write) -> anyhow::Result<()> {
    for paper in papers {
        writeln!(writer, "{}\n", paper_to_bibtex(paper))?;
    }
    Ok(())
}

fn bibtex_key(paper: &Paper) -> String {
    let last_name = paper
        .authors
        .first()
        .map(|a| {
            a.name
                .split_whitespace()
                .last()
                .unwrap_or(&a.name)
                .to_string()
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let year = paper
        .published_date
        .map(|d| d.format("%Y").to_string())
        .unwrap_or_else(|| "0000".to_string());

    let first_word = paper
        .title
        .split_whitespace()
        .next()
        .unwrap_or("Paper")
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>();

    let sanitize = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };

    format!("{}{}{}", sanitize(&last_name), year, first_word)
}
