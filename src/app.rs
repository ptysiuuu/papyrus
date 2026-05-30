use tokio::sync::mpsc;

use crate::filters::FilterSet;
use crate::models::Paper;

#[derive(Debug, Clone)]
pub enum AppEvent {
    SearchStarted,
    PapersReceived(Vec<Paper>, Option<u64>, String), // papers, total, source_name
    SearchCompleted,
    SearchError(String, String), // source, message
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    None,
    Search,
    Filter,
    Export,
    Help,
    Tag,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Results,
    Detail,
}

pub struct App {
    pub filters: FilterSet,
    pub papers: Vec<Paper>,
    pub total_count: Option<u64>,
    pub selected_idx: usize,
    pub detail_scroll: usize,
    pub focus: Focus,
    pub modal: Modal,
    pub status_message: String,
    pub is_fetching: bool,
    pub fetch_errors: Vec<String>,
    pub modal_input: String,
    pub modal_cursor: usize,
    pub search_history: Vec<String>,
    pub history_idx: Option<usize>,
    pub fuzzy_filter: Option<String>,
    pub fuzzy_input: String,
    pub fuzzy_active: bool,
    pub page: u32,
    pub bibtex_buffer: Vec<Paper>,
    pub export_format_idx: usize,
    pub export_scope_idx: usize,
    pub export_path_input: String,
    pub filter_field_idx: usize,
    pub filter_fields: Vec<FilterField>,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pub sources_status: Vec<(String, SourceStatus)>,
}

#[derive(Debug, Clone)]
pub enum SourceStatus {
    Idle,
    Fetching,
    Done(usize),
    Error(String),
}

#[derive(Debug, Clone)]
pub struct FilterField {
    pub label: &'static str,
    pub value: String,
    pub field_type: FilterFieldType,
}

#[derive(Debug, Clone)]
pub enum FilterFieldType {
    Text,
    Toggle(bool),
    Select(usize, Vec<&'static str>),
}

impl App {
    pub fn new(filters: FilterSet) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let filter_fields = build_filter_fields(&filters);
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let export_path = format!("./papers-{}.json", today);
        Self {
            filters,
            papers: Vec::new(),
            total_count: None,
            selected_idx: 0,
            detail_scroll: 0,
            focus: Focus::Results,
            modal: Modal::None,
            status_message: String::from("Press / to search, f for filters, ? for help"),
            is_fetching: false,
            fetch_errors: Vec::new(),
            modal_input: String::new(),
            modal_cursor: 0,
            search_history: Vec::new(),
            history_idx: None,
            fuzzy_filter: None,
            fuzzy_input: String::new(),
            fuzzy_active: false,
            page: 0,
            bibtex_buffer: Vec::new(),
            export_format_idx: 0,
            export_scope_idx: 0,
            export_path_input: export_path,
            filter_field_idx: 0,
            filter_fields,
            event_tx: tx,
            event_rx: rx,
            sources_status: Vec::new(),
        }
    }

    pub fn selected_paper(&self) -> Option<&Paper> {
        self.visible_papers().into_iter().nth(self.selected_idx)
    }

    pub fn visible_papers(&self) -> Vec<&Paper> {
        if let Some(filt) = &self.fuzzy_filter {
            let filt_lower = filt.to_lowercase();
            self.papers
                .iter()
                .filter(|p| {
                    p.title.to_lowercase().contains(&filt_lower)
                        || p.authors_display().to_lowercase().contains(&filt_lower)
                })
                .collect()
        } else {
            self.papers.iter().collect()
        }
    }

    pub fn move_down(&mut self) {
        let len = self.visible_papers().len();
        if len > 0 && self.selected_idx < len - 1 {
            self.selected_idx += 1;
            self.detail_scroll = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
            self.detail_scroll = 0;
        }
    }

    pub fn jump_first(&mut self) {
        self.selected_idx = 0;
        self.detail_scroll = 0;
    }

    pub fn jump_last(&mut self) {
        let len = self.visible_papers().len();
        if len > 0 {
            self.selected_idx = len - 1;
        }
        self.detail_scroll = 0;
    }

    pub fn scroll_detail_down(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_add(3);
    }

    pub fn scroll_detail_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(3);
    }

    pub fn open_search_modal(&mut self) {
        self.modal = Modal::Search;
        self.modal_input = self.filters.query.clone().unwrap_or_default();
        self.modal_cursor = self.modal_input.len();
        self.history_idx = None;
    }

    pub fn open_filter_modal(&mut self) {
        self.filter_fields = build_filter_fields(&self.filters);
        self.filter_field_idx = 0;
        self.modal = Modal::Filter;
    }

    pub fn open_export_modal(&mut self) {
        self.modal = Modal::Export;
        self.export_format_idx = 0;
        self.export_scope_idx = 0;
    }

    pub fn close_modal(&mut self) {
        self.modal = Modal::None;
    }

    pub fn modal_input_push(&mut self, c: char) {
        self.modal_input.insert(self.modal_cursor, c);
        self.modal_cursor += c.len_utf8();
    }

    pub fn modal_input_backspace(&mut self) {
        if self.modal_cursor > 0 {
            let prev = self.modal_input[..self.modal_cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.modal_input.drain(prev..self.modal_cursor);
            self.modal_cursor = prev;
        }
    }

    pub fn apply_search_modal(&mut self) -> Option<FilterSet> {
        let query = self.modal_input.trim().to_string();
        if !query.is_empty() {
            if !self.search_history.contains(&query) {
                self.search_history.push(query.clone());
            }
            self.filters.query = Some(query);
        } else {
            self.filters.query = None;
        }
        self.modal = Modal::None;
        self.page = 0;
        Some(self.filters.clone())
    }

    pub fn history_up(&mut self) {
        if self.search_history.is_empty() {
            return;
        }
        let new_idx = match self.history_idx {
            None => self.search_history.len() - 1,
            Some(i) if i > 0 => i - 1,
            Some(i) => i,
        };
        self.history_idx = Some(new_idx);
        self.modal_input = self.search_history[new_idx].clone();
        self.modal_cursor = self.modal_input.len();
    }

    pub fn history_down(&mut self) {
        match self.history_idx {
            None => {}
            Some(i) if i + 1 < self.search_history.len() => {
                let new_idx = i + 1;
                self.history_idx = Some(new_idx);
                self.modal_input = self.search_history[new_idx].clone();
                self.modal_cursor = self.modal_input.len();
            }
            Some(_) => {
                self.history_idx = None;
                self.modal_input.clear();
                self.modal_cursor = 0;
            }
        }
    }

    pub fn add_to_bibtex(&mut self) {
        if let Some(paper) = self.selected_paper().cloned() {
            if !self.bibtex_buffer.iter().any(|p| p.id == paper.id) {
                self.status_message = format!("Added \"{}\" to BibTeX buffer ({} total)",
                    truncate(&paper.title, 40), self.bibtex_buffer.len() + 1);
                self.bibtex_buffer.push(paper);
            } else {
                self.status_message = "Already in BibTeX buffer".to_string();
            }
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Results => Focus::Detail,
            Focus::Detail => Focus::Results,
        };
    }

    pub fn apply_filter_modal(&mut self) -> Option<FilterSet> {
        // Sync filter_fields back to filters
        for field in &self.filter_fields {
            match field.label {
                "Query" => {
                    self.filters.query = if field.value.is_empty() {
                        None
                    } else {
                        Some(field.value.clone())
                    };
                }
                "Author" => {
                    self.filters.authors = field
                        .value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "Category" => {
                    self.filters.categories = field
                        .value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "From Date" => {
                    self.filters.date_from = crate::filters::parse_flexible_date_pub(&field.value);
                }
                "To Date" => {
                    self.filters.date_to = crate::filters::parse_flexible_date_pub(&field.value);
                }
                "Min Citations" => {
                    self.filters.min_citations = field.value.parse().ok();
                }
                "Has PDF" => {
                    if let FilterFieldType::Toggle(v) = field.field_type {
                        self.filters.has_pdf = v;
                    }
                }
                "Open Access" => {
                    if let FilterFieldType::Toggle(v) = field.field_type {
                        self.filters.open_access_only = v;
                    }
                }
                "Peer Reviewed" => {
                    if let FilterFieldType::Toggle(v) = field.field_type {
                        self.filters.peer_reviewed_only = v;
                    }
                }
                "Limit" => {
                    self.filters.limit = field.value.parse().unwrap_or(20).min(500);
                }
                _ => {}
            }
        }
        self.modal = Modal::None;
        self.page = 0;
        Some(self.filters.clone())
    }
}

fn build_filter_fields(f: &FilterSet) -> Vec<FilterField> {
    vec![
        FilterField {
            label: "Query",
            value: f.query.clone().unwrap_or_default(),
            field_type: FilterFieldType::Text,
        },
        FilterField {
            label: "Author",
            value: f.authors.join(", "),
            field_type: FilterFieldType::Text,
        },
        FilterField {
            label: "Category",
            value: f.categories.join(", "),
            field_type: FilterFieldType::Text,
        },
        FilterField {
            label: "From Date",
            value: f
                .date_from
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            field_type: FilterFieldType::Text,
        },
        FilterField {
            label: "To Date",
            value: f
                .date_to
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default(),
            field_type: FilterFieldType::Text,
        },
        FilterField {
            label: "Min Citations",
            value: f.min_citations.map(|c| c.to_string()).unwrap_or_default(),
            field_type: FilterFieldType::Text,
        },
        FilterField {
            label: "Has PDF",
            value: if f.has_pdf { "yes" } else { "no" }.to_string(),
            field_type: FilterFieldType::Toggle(f.has_pdf),
        },
        FilterField {
            label: "Open Access",
            value: if f.open_access_only { "yes" } else { "no" }.to_string(),
            field_type: FilterFieldType::Toggle(f.open_access_only),
        },
        FilterField {
            label: "Peer Reviewed",
            value: if f.peer_reviewed_only { "yes" } else { "no" }.to_string(),
            field_type: FilterFieldType::Toggle(f.peer_reviewed_only),
        },
        FilterField {
            label: "Limit",
            value: f.limit.to_string(),
            field_type: FilterFieldType::Text,
        },
    ]
}

pub fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut idx = max;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}
