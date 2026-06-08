use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;

use crate::db::Database;
use crate::models::Paper;

/// Build a safe filename for a PDF: `<year>_<first_author>_<slug>.pdf`
pub fn pdf_filename(paper: &Paper) -> String {
    let year = paper
        .year()
        .map(|y| y.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let first_author = paper
        .authors
        .first()
        .map(|a| {
            // Take the last component of the name (surname), lowercase, strip non-alpha
            let surname = a.name.split_whitespace().last().unwrap_or(&a.name);
            sanitize_component(surname)
        })
        .unwrap_or_else(|| "unknown".to_string());

    let title_slug = sanitize_component(&paper.title)
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join("_");

    format!("{}_{}_{}.pdf", year, first_author, title_slug)
}

/// Build the directory path for a paper: `<base>/<year>/<first_author>/`
pub fn pdf_dir_path(base: &Path, paper: &Paper) -> PathBuf {
    let year = paper
        .year()
        .map(|y| y.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let first_author = paper
        .authors
        .first()
        .map(|a| {
            let surname = a.name.split_whitespace().last().unwrap_or(&a.name);
            sanitize_component(surname)
        })
        .unwrap_or_else(|| "unknown".to_string());

    base.join(year).join(first_author)
}

fn sanitize_component(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Try to extract text from a PDF file using `pdftotext`.
/// Returns `Ok(None)` if pdftotext is not available or fails.
pub fn extract_text_from_pdf(path: &Path) -> Result<Option<String>> {
    // Check if pdftotext is available
    let check = std::process::Command::new("pdftotext").arg("-v").output();
    if check.is_err() {
        return Ok(None);
    }

    let output = std::process::Command::new("pdftotext")
        .arg(path)
        .arg("-") // Output to stdout
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            if text.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        }
        _ => Ok(None),
    }
}

/// Download a single paper's PDF.
pub struct PdfDownloader {
    client: Client,
    base_dir: PathBuf,
}

impl PdfDownloader {
    pub fn new(client: Client, base_dir: PathBuf) -> Self {
        Self { client, base_dir }
    }

    pub async fn download(&self, paper: &Paper, db: Option<&Database>) -> Result<PathBuf> {
        let pdf_url = paper
            .pdf_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No PDF URL for paper: {}", paper.title))?;

        let dir = pdf_dir_path(&self.base_dir, paper);
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Creating directory {:?}", dir))?;

        let filename = pdf_filename(paper);
        let dest = dir.join(&filename);

        if dest.exists() {
            return Ok(dest);
        }

        let response = self
            .client
            .get(pdf_url)
            .send()
            .await
            .with_context(|| format!("Fetching PDF from {}", pdf_url))?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {} fetching PDF from {}", response.status(), pdf_url);
        }

        let bytes = response
            .bytes()
            .await
            .context("Reading PDF bytes")?;

        std::fs::write(&dest, &bytes)
            .with_context(|| format!("Writing PDF to {:?}", dest))?;

        // Optionally extract text and store in DB
        if let Some(db) = db {
            if let Ok(Some(text)) = extract_text_from_pdf(&dest) {
                let _ = db.set_full_text(&paper.id, &text);
            }
            let _ = db.set_pdf_path(&paper.id, &dest.to_string_lossy());
        }

        Ok(dest)
    }

    /// Download all papers from a list, showing a progress bar.
    pub async fn download_all(
        &self,
        papers: &[Paper],
        db: Option<&Database>,
    ) -> Vec<(String, Result<PathBuf>)> {
        let pb = ProgressBar::new(papers.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar()),
        );

        let mut results = Vec::new();
        for paper in papers {
            pb.set_message(paper.title.chars().take(40).collect::<String>());
            let result = self.download(paper, db).await;
            results.push((paper.title.clone(), result));
            pb.inc(1);
        }
        pb.finish_with_message("Done");
        results
    }
}
