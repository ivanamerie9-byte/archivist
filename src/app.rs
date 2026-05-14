use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::domain::{
    ActionPlan, ExecutionReport, Issue, Library, LibraryKind, MatchCandidate, MediaKind,
    PlannedAction, ProgressEvent, ScanReport, TmdbMatch,
};
use crate::pipeline;
use crate::scanner;
use crate::tmdb::TmdbClient;
use crate::tui::screens::{
    applying::ApplyingState,
    candidate_modal::CandidateModalState,
    issue_list::{Entry, EntryKind, IssueListState, MatchState},
    library_select::{LibChoice, LibrarySelectState},
    report::ReportState,
    scanning::ScanningState,
    settings::SettingsState,
};
use crate::util::normalize::norm_title;

pub enum Screen {
    Settings(SettingsState),
    LibrarySelect(LibrarySelectState),
    Scanning(ScanningState),
    IssueList(IssueListState),
    Matching {
        list: IssueListState,
        current: u64,
        total: u64,
        message: String,
    },
    CandidateModal {
        list: IssueListState,
        modal: CandidateModalState,
        queue: Vec<usize>,
        current_entry: usize,
    },
    Applying(ApplyingState),
    Report(ReportState),
}

pub enum BgEvent {
    Progress(ProgressEvent),
    ScanDone(Result<ScanReport>),
    MatchAllDone(Vec<(usize, MatchState)>),
    RetrySearchDone(Vec<MatchCandidate>),
    ApplyDone(ExecutionReport),
    PingDone(Result<()>),
}

pub struct App {
    pub config: Config,
    pub tmdb: Option<Arc<TmdbClient>>,
    pub screen: Screen,
    pub bg_tx: UnboundedSender<BgEvent>,
    pub bg_rx: UnboundedReceiver<BgEvent>,
    pub cancel: CancellationToken,
    pub scan_report: Option<ScanReport>,
    pub should_quit: bool,
}

impl App {
    pub fn new(config: Config) -> Self {
        let tmdb = if config.tmdb_api_key.trim().is_empty() {
            None
        } else {
            TmdbClient::new(config.tmdb_api_key.clone())
                .ok()
                .map(Arc::new)
        };
        let (bg_tx, bg_rx) = unbounded_channel();
        let initial = if tmdb.is_none() {
            Screen::Settings(SettingsState::from_config(&config))
        } else {
            Screen::LibrarySelect(LibrarySelectState::default())
        };
        Self {
            config,
            tmdb,
            screen: initial,
            bg_tx,
            bg_rx,
            cancel: CancellationToken::new(),
            scan_report: None,
            should_quit: false,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        let mut term = setup_terminal()?;
        let mut events = EventStream::new();
        let mut tick = tokio::time::interval(Duration::from_millis(120));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let result: Result<()> = loop {
            // Render every iteration; cheap.
            if let Err(e) = term.draw(|f| self.render(f)) {
                break Err(e.into());
            }

            tokio::select! {
                maybe_ev = events.next() => {
                    match maybe_ev {
                        Some(Ok(Event::Key(k))) if k.kind == KeyEventKind::Press => {
                            self.handle_key(k);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            tracing::error!(error=%e, "event stream error");
                        }
                        None => {
                            self.should_quit = true;
                        }
                    }
                }
                Some(bg) = self.bg_rx.recv() => {
                    self.handle_bg(bg);
                }
                _ = tick.tick() => {}
            }

            if self.should_quit {
                break Ok(());
            }
        };

        cleanup_terminal(&mut term)?;
        result
    }

    fn render(&mut self, f: &mut ratatui::Frame) {
        let area = f.area();
        match &self.screen {
            Screen::Settings(s) => crate::tui::screens::settings::render(f, area, s),
            Screen::LibrarySelect(s) => crate::tui::screens::library_select::render(f, area, s),
            Screen::Scanning(s) => crate::tui::screens::scanning::render(f, area, s),
            Screen::IssueList(s) => crate::tui::screens::issue_list::render(f, area, s),
            Screen::Matching {
                list,
                current,
                total,
                message,
            } => {
                let s = ScanningState {
                    current: *current,
                    total: *total,
                    message: message.clone(),
                    log_tail: list.entries.iter().map(|e| e.label.clone()).collect(),
                };
                crate::tui::screens::scanning::render(f, area, &s);
            }
            Screen::CandidateModal { modal, .. } => {
                crate::tui::screens::candidate_modal::render(f, area, modal)
            }
            Screen::Applying(s) => crate::tui::screens::applying::render(f, area, s),
            Screen::Report(s) => crate::tui::screens::report::render(f, area, s),
        }
    }

    fn handle_key(&mut self, k: KeyEvent) {
        // Global: Ctrl+C quits.
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match &mut self.screen {
            Screen::Settings(_) => self.handle_settings(k),
            Screen::LibrarySelect(_) => self.handle_library_select(k),
            Screen::Scanning(_) => self.handle_scanning(k),
            Screen::IssueList(_) => self.handle_issue_list(k),
            Screen::Matching { .. } => {
                if k.code == KeyCode::Esc {
                    self.cancel.cancel();
                }
            }
            Screen::CandidateModal { .. } => self.handle_candidate_modal(k),
            Screen::Applying(_) => {
                if k.code == KeyCode::Esc {
                    self.cancel.cancel();
                }
            }
            Screen::Report(_) => match k.code {
                KeyCode::Enter => {
                    self.screen = Screen::LibrarySelect(LibrarySelectState::default());
                }
                KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
                _ => {}
            },
        }
    }

    // ---- Settings handling ----
    fn handle_settings(&mut self, k: KeyEvent) {
        let Screen::Settings(s) = &mut self.screen else {
            return;
        };
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match (ctrl, k.code) {
            (false, KeyCode::Esc) => {
                if !self.config.tmdb_api_key.trim().is_empty() && self.tmdb.is_some() {
                    self.screen = Screen::LibrarySelect(LibrarySelectState::default());
                } else {
                    s.status = Some((false, "Set a TMDB API key first.".into()));
                }
            }
            (false, KeyCode::Tab) => s.cycle_focus(true),
            (false, KeyCode::BackTab) => s.cycle_focus(false),
            (true, KeyCode::Char('m')) => s.mask_key = !s.mask_key,
            (true, KeyCode::Char('s')) => self.save_settings(),
            (true, KeyCode::Char('t')) => self.spawn_ping(),
            (false, KeyCode::Backspace) => {
                s.current_buf_mut().pop();
            }
            (false, KeyCode::Char(c)) => {
                s.current_buf_mut().push(c);
            }
            _ => {}
        }
    }

    fn save_settings(&mut self) {
        let Screen::Settings(s) = &mut self.screen else {
            return;
        };
        s.apply_to_config(&mut self.config);
        match self.config.save() {
            Ok(path) => {
                s.status = Some((true, format!("Saved to {}", path.display())));
                if !self.config.tmdb_api_key.trim().is_empty() {
                    self.tmdb = TmdbClient::new(self.config.tmdb_api_key.clone())
                        .ok()
                        .map(Arc::new);
                }
            }
            Err(e) => s.status = Some((false, format!("Save failed: {e}"))),
        }
    }

    fn spawn_ping(&mut self) {
        let Screen::Settings(s) = &mut self.screen else {
            return;
        };
        s.apply_to_config(&mut self.config);
        let key = self.config.tmdb_api_key.clone();
        if key.trim().is_empty() {
            s.status = Some((false, "API key is empty.".into()));
            return;
        }
        s.testing = true;
        let tx = self.bg_tx.clone();
        tokio::spawn(async move {
            let res = match TmdbClient::new(key) {
                Ok(c) => c.ping().await,
                Err(e) => Err(e),
            };
            let _ = tx.send(BgEvent::PingDone(res));
        });
    }

    // ---- LibrarySelect handling ----
    fn handle_library_select(&mut self, k: KeyEvent) {
        let mut enter_choice: Option<LibChoice> = None;
        {
            let Screen::LibrarySelect(s) = &mut self.screen else {
                return;
            };
            match k.code {
                KeyCode::Up => s.prev(),
                KeyCode::Down => s.next(),
                KeyCode::Char('m') | KeyCode::Char('M') => s.choice = LibChoice::Movies,
                KeyCode::Char('s') | KeyCode::Char('S') => s.choice = LibChoice::Series,
                KeyCode::Char('b') | KeyCode::Char('B') => s.choice = LibChoice::Both,
                KeyCode::Char('k') => {
                    self.screen = Screen::Settings(SettingsState::from_config(&self.config));
                    return;
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Enter => enter_choice = Some(s.choice),
                _ => {}
            }
        }
        if let Some(c) = enter_choice {
            self.spawn_scan(c);
        }
    }

    fn spawn_scan(&mut self, choice: LibChoice) {
        let libs: Vec<Library> = self
            .config
            .libraries()
            .into_iter()
            .filter(|l| match choice {
                LibChoice::Movies => l.kind == LibraryKind::Movies,
                LibChoice::Series => l.kind == LibraryKind::Series,
                LibChoice::Both => true,
            })
            .collect();
        let cancel = self.cancel.clone();
        let tx = self.bg_tx.clone();
        let (prog_tx, mut prog_rx) = unbounded_channel::<ProgressEvent>();
        let tx_for_progress = tx.clone();
        tokio::spawn(async move {
            while let Some(p) = prog_rx.recv().await {
                if tx_for_progress.send(BgEvent::Progress(p)).is_err() {
                    break;
                }
            }
        });
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            let res = scanner::scan(libs, prog_tx, cancel_for_task).await;
            let _ = tx.send(BgEvent::ScanDone(res));
        });
        self.screen = Screen::Scanning(ScanningState::default());
    }

    // ---- Scanning handling ----
    fn handle_scanning(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Esc {
            self.cancel.cancel();
            self.cancel = CancellationToken::new();
            self.screen = Screen::LibrarySelect(LibrarySelectState::default());
        }
    }

    // ---- IssueList handling ----
    fn handle_issue_list(&mut self, k: KeyEvent) {
        let Screen::IssueList(s) = &mut self.screen else {
            return;
        };
        match k.code {
            KeyCode::Up => s.cursor_up(),
            KeyCode::Down => s.cursor_down(),
            KeyCode::PageUp => s.page_up(10),
            KeyCode::PageDown => s.page_down(10),
            KeyCode::Home => s.cursor_home(),
            KeyCode::End => s.cursor_end(),
            KeyCode::Char(' ') => s.toggle(),
            KeyCode::Char('a') => s.toggle_all(),
            KeyCode::Char('k') => {
                self.screen = Screen::Settings(SettingsState::from_config(&self.config));
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.screen = Screen::LibrarySelect(LibrarySelectState::default());
            }
            KeyCode::Enter => self.spawn_match_all(),
            _ => {}
        }
    }

    fn spawn_match_all(&mut self) {
        let Screen::IssueList(list) = &mut self.screen else {
            return;
        };
        let Some(tmdb) = self.tmdb.clone() else {
            return;
        };
        let report = match self.scan_report.as_ref() {
            Some(r) => r.clone_titles(),
            None => return,
        };
        let checked = list.checked_indices();
        let mut tasks: Vec<(usize, EntryKind, LibraryKind, String, String, Option<u16>)> =
            Vec::new();
        for idx in &checked {
            let entry = &list.entries[*idx];
            // Compute query based on entry kind
            let (query, year) = derive_query(entry, &report);
            tasks.push((
                *idx,
                entry.kind.clone(),
                entry.library,
                entry.label.clone(),
                query,
                year,
            ));
        }
        let total = tasks.len() as u64;
        let tx = self.bg_tx.clone();

        tokio::spawn(async move {
            let mut out: Vec<(usize, MatchState)> = Vec::with_capacity(tasks.len());
            for (i, (idx, kind, lib, _label, query, year)) in tasks.into_iter().enumerate() {
                let _ = tx.send(BgEvent::Progress(ProgressEvent {
                    current: (i + 1) as u64,
                    total,
                    message: format!("Searching: {query}"),
                }));

                let media_kind = match &kind {
                    EntryKind::RestoreSeason { .. } => MediaKind::Tv,
                    _ => MediaKind::from_library(lib),
                };

                let state = match resolve_one(&tmdb, media_kind, &query, year).await {
                    Ok(MatchOutcome::Auto(m)) => MatchState::AutoMatched(m),
                    Ok(MatchOutcome::Choices(c)) => {
                        if c.is_empty() {
                            MatchState::Unresolvable("no candidates".into())
                        } else {
                            MatchState::NeedsChoice(c)
                        }
                    }
                    Err(e) => MatchState::Unresolvable(format!("{e:#}")),
                };
                out.push((idx, state));
            }
            let _ = tx.send(BgEvent::MatchAllDone(out));
        });

        self.screen = Screen::Matching {
            list: std::mem::replace(
                list,
                IssueListState {
                    entries: vec![],
                    cursor: 0,
                },
            ),
            current: 0,
            total,
            message: "Matching...".into(),
        };
    }

    // ---- Candidate modal handling ----
    fn handle_candidate_modal(&mut self, k: KeyEvent) {
        let Screen::CandidateModal {
            list,
            modal,
            queue,
            current_entry,
        } = &mut self.screen
        else {
            return;
        };

        if modal.editing_query {
            match k.code {
                KeyCode::Esc => modal.editing_query = false,
                KeyCode::Backspace => {
                    modal.query_input.pop();
                }
                KeyCode::Char(c) => modal.query_input.push(c),
                KeyCode::Enter => {
                    let query = modal.query_input.clone();
                    let entry = list.entries[*current_entry].clone();
                    let media_kind = match entry.kind {
                        EntryKind::RestoreSeason { .. } => MediaKind::Tv,
                        _ => MediaKind::from_library(entry.library),
                    };
                    let tmdb = match &self.tmdb {
                        Some(t) => t.clone(),
                        None => return,
                    };
                    let tx = self.bg_tx.clone();
                    tokio::spawn(async move {
                        let candidates = retry_search(&tmdb, media_kind, &query)
                            .await
                            .unwrap_or_default();
                        let _ = tx.send(BgEvent::RetrySearchDone(candidates));
                    });
                    modal.editing_query = false;
                }
                _ => {}
            }
            return;
        }

        match k.code {
            KeyCode::Up => modal.cursor_up(),
            KeyCode::Down => modal.cursor_down(),
            KeyCode::Char('s') => self.advance_modal(MatchState::Skipped),
            KeyCode::Char('r') => {
                modal.editing_query = true;
                modal.query_input.clear();
            }
            KeyCode::Esc => {
                // Skip this and all remaining choices
                for &i in queue.iter() {
                    list.entries[i].state = MatchState::Skipped;
                }
                queue.clear();
                self.advance_modal(MatchState::Skipped);
            }
            KeyCode::Enter => {
                if let Some(c) = modal.candidates.get(modal.cursor) {
                    let m = c.tmdb.clone();
                    self.advance_modal(MatchState::AutoMatched(m));
                }
            }
            _ => {}
        }
    }

    fn advance_modal(&mut self, set_state_for_current: MatchState) {
        let Screen::CandidateModal {
            list,
            queue,
            current_entry,
            ..
        } = &mut self.screen
        else {
            return;
        };
        list.entries[*current_entry].state = set_state_for_current;
        if let Some(next_idx) = queue.pop() {
            *current_entry = next_idx;
            let entry = &list.entries[next_idx];
            let cands = match &entry.state {
                MatchState::NeedsChoice(c) => c.clone(),
                _ => Vec::new(),
            };
            let new_modal = CandidateModalState {
                entry_label: entry.label.clone(),
                candidates: cands,
                cursor: 0,
                editing_query: false,
                query_input: String::new(),
                remaining_after_this: queue.len(),
            };
            // Re-create CandidateModal in place
            if let Screen::CandidateModal { modal, .. } = &mut self.screen {
                *modal = new_modal;
            }
        } else {
            // Done with all candidates → build action plan and apply.
            let list = std::mem::replace(
                list,
                IssueListState {
                    entries: vec![],
                    cursor: 0,
                },
            );
            self.start_apply(list);
        }
    }

    fn start_apply(&mut self, list: IssueListState) {
        let Some(report) = self.scan_report.as_ref() else {
            return;
        };
        let plan = build_action_plan(&list, report, &self.config);
        let total = plan.items.len() as u64;
        if total == 0 {
            self.screen = Screen::Report(ReportState {
                report: ExecutionReport::default(),
            });
            return;
        }
        let tmdb = match self.tmdb.clone() {
            Some(t) => t,
            None => return,
        };
        let cancel = self.cancel.clone();
        let tx = self.bg_tx.clone();
        let (prog_tx, mut prog_rx) = unbounded_channel::<ProgressEvent>();
        let tx_for_progress = tx.clone();
        tokio::spawn(async move {
            while let Some(p) = prog_rx.recv().await {
                if tx_for_progress.send(BgEvent::Progress(p)).is_err() {
                    break;
                }
            }
        });
        let concurrency = self.config.concurrency;
        tokio::spawn(async move {
            let exec = pipeline::execute(plan, tmdb, concurrency, prog_tx, cancel).await;
            let _ = tx.send(BgEvent::ApplyDone(exec));
        });
        self.screen = Screen::Applying(ApplyingState::default());
    }

    // ---- Background event handling ----
    fn handle_bg(&mut self, ev: BgEvent) {
        match ev {
            BgEvent::Progress(p) => match &mut self.screen {
                Screen::Scanning(s) => s.push(p.current, p.total, p.message),
                Screen::Matching {
                    current,
                    total,
                    message,
                    ..
                } => {
                    *current = p.current;
                    *total = p.total;
                    *message = p.message;
                }
                Screen::Applying(s) => s.push(p.current, p.total, p.message),
                _ => {}
            },
            BgEvent::ScanDone(res) => match res {
                Ok(report) => {
                    let issues = report.issues.len();
                    self.scan_report = Some(report);
                    if issues == 0 {
                        self.screen = Screen::Report(ReportState {
                            report: ExecutionReport {
                                successes: vec![
                                    "Library is already complete — nothing to do.".into()
                                ],
                                ..Default::default()
                            },
                        });
                    } else {
                        let list = IssueListState::from_report(self.scan_report.as_ref().unwrap());
                        self.screen = Screen::IssueList(list);
                    }
                }
                Err(e) => {
                    self.screen = Screen::Report(ReportState {
                        report: ExecutionReport {
                            failures: vec![("Scan failed".into(), format!("{e:#}"))],
                            ..Default::default()
                        },
                    });
                }
            },
            BgEvent::MatchAllDone(updates) => {
                let Screen::Matching { list, .. } = &mut self.screen else {
                    return;
                };
                for (idx, state) in updates {
                    if let Some(e) = list.entries.get_mut(idx) {
                        e.state = state;
                    }
                }
                // Build queue of NeedsChoice entries
                let queue: Vec<usize> = list
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| {
                        if matches!(e.state, MatchState::NeedsChoice(_)) {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .collect();
                let list = std::mem::replace(
                    list,
                    IssueListState {
                        entries: vec![],
                        cursor: 0,
                    },
                );
                if queue.is_empty() {
                    self.start_apply(list);
                } else {
                    let mut queue = queue;
                    let first = queue.pop().unwrap();
                    let entry = &list.entries[first];
                    let candidates = match &entry.state {
                        MatchState::NeedsChoice(c) => c.clone(),
                        _ => Vec::new(),
                    };
                    let modal = CandidateModalState {
                        entry_label: entry.label.clone(),
                        candidates,
                        cursor: 0,
                        editing_query: false,
                        query_input: String::new(),
                        remaining_after_this: queue.len(),
                    };
                    self.screen = Screen::CandidateModal {
                        list,
                        modal,
                        queue,
                        current_entry: first,
                    };
                }
            }
            BgEvent::RetrySearchDone(c) => {
                if let Screen::CandidateModal { modal, .. } = &mut self.screen {
                    modal.candidates = c;
                    modal.cursor = 0;
                }
            }
            BgEvent::ApplyDone(report) => {
                self.screen = Screen::Report(ReportState { report });
            }
            BgEvent::PingDone(res) => {
                if let Screen::Settings(s) = &mut self.screen {
                    s.testing = false;
                    s.status = Some(match res {
                        Ok(()) => (true, "TMDB connection OK".into()),
                        Err(e) => (false, format!("{e:#}")),
                    });
                }
            }
        }
    }
}

// ----- helpers -----

#[derive(Debug)]
enum MatchOutcome {
    Auto(TmdbMatch),
    Choices(Vec<MatchCandidate>),
}

async fn resolve_one(
    tmdb: &Arc<TmdbClient>,
    kind: MediaKind,
    query: &str,
    year: Option<u16>,
) -> Result<MatchOutcome> {
    let mut results = tmdb.search(kind, query, year).await?;
    if results.is_empty() && year.is_some() {
        results = tmdb.search(kind, query, None).await?;
    }
    if results.is_empty() {
        return Ok(MatchOutcome::Choices(Vec::new()));
    }

    // Score & sort
    let normalised_query = norm_title(query);
    let mut scored: Vec<(f32, TmdbMatch)> = results
        .into_iter()
        .map(|m| {
            let title_eq = if norm_title(&m.canonical_title) == normalised_query {
                1.0
            } else {
                0.0
            };
            let year_eq = match year {
                Some(y) if y == m.year => 1.0,
                Some(_) => 0.0,
                None => 0.5,
            };
            let score = 0.6 * title_eq + 0.3 * year_eq + 0.1 * 0.5; // popularity unused (no field on TmdbMatch)
            (score, m)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let top_score = scored[0].0;
    let second_score = scored.get(1).map(|x| x.0).unwrap_or(0.0);

    let auto_ok = norm_title(&scored[0].1.canonical_title) == normalised_query
        && match year {
            Some(y) => y == scored[0].1.year,
            None => false,
        }
        && (scored.len() == 1 || (top_score - second_score) >= 0.15);

    if auto_ok {
        // Resolve best poster path now.
        let mut chosen = scored.into_iter().next().unwrap().1;
        if let Ok(Some(p)) = tmdb.pick_best_poster_path(kind, chosen.id).await {
            chosen.poster_path = Some(p);
        }
        Ok(MatchOutcome::Auto(chosen))
    } else {
        let candidates: Vec<MatchCandidate> = scored
            .into_iter()
            .take(4)
            .map(|(score, m)| MatchCandidate {
                tmdb: m,
                overview: String::new(),
                auto_score: score,
            })
            .collect();
        Ok(MatchOutcome::Choices(candidates))
    }
}

async fn retry_search(
    tmdb: &Arc<TmdbClient>,
    kind: MediaKind,
    query: &str,
) -> Result<Vec<MatchCandidate>> {
    let results = tmdb.search(kind, query, None).await?;
    Ok(results
        .into_iter()
        .take(4)
        .map(|m| MatchCandidate {
            tmdb: m,
            overview: String::new(),
            auto_score: 0.0,
        })
        .collect())
}

fn derive_query(entry: &Entry, report: &ScanReport) -> (String, Option<u16>) {
    match &entry.kind {
        EntryKind::RestoreTitle { title_idx } | EntryKind::RestoreSeason { title_idx, .. } => {
            let t = &report.titles[*title_idx];
            (t.parsed.title.clone(), t.parsed.year)
        }
        EntryKind::WrapOrphanVideo { issue_idx } => {
            if let Issue::OrphanVideo { path, .. } = &report.issues[*issue_idx] {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let parsed = crate::parser::parse_release_name(stem);
                (parsed.title, parsed.year)
            } else {
                (String::new(), None)
            }
        }
        EntryKind::UnparseableTitle { issue_idx } => {
            if let Issue::UnparseableFolder { path, .. } = &report.issues[*issue_idx] {
                let basename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                (basename.to_string(), None)
            } else {
                (String::new(), None)
            }
        }
    }
}

fn build_action_plan(list: &IssueListState, report: &ScanReport, _config: &Config) -> ActionPlan {
    let mut items: Vec<PlannedAction> = Vec::new();

    for entry in &list.entries {
        if !entry.checked {
            continue;
        }
        let m = match &entry.state {
            MatchState::AutoMatched(m) => m.clone(),
            _ => continue, // Skipped, NeedsChoice (unhandled), Unresolvable, Pending — drop.
        };
        let Some(poster) = m.poster_path.clone() else {
            continue;
        };

        match &entry.kind {
            EntryKind::RestoreTitle { title_idx } => {
                let title = &report.titles[*title_idx];
                let lib = title.library;
                let canonical = m.canonical_folder_name();
                let current_basename = title
                    .folder
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Adopt-as-season: scene-style season folder in the Series
                // library that matches an existing well-named series → move it
                // inside as `Season N\`. Cover for the new season is left for
                // the next scan to pick up (it'll appear as MissingFolderJpg
                // for that season).
                if lib == LibraryKind::Series {
                    if let Some(season) = title.parsed.season {
                        let target_series = report.titles.iter().find(|t| {
                            t.library == LibraryKind::Series
                                && t.folder != title.folder
                                && t.folder
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    == Some(canonical.as_str())
                        });
                        if let Some(target) = target_series {
                            items.push(PlannedAction::AdoptAsSeason {
                                label: format!(
                                    "Adopt: {} → {}\\Season {}",
                                    current_basename, canonical, season
                                ),
                                src_dir: title.folder.clone(),
                                series_root: target.folder.clone(),
                                season,
                            });
                            continue;
                        }
                    }
                }

                let lib_root = lib_root(report, lib);
                let covers = lib_root.join("_covers");
                let basename = canonical.clone();
                let jpg = covers.join(format!("{basename}.jpg"));
                let ico = covers.join(format!("{basename}.ico"));

                // If the on-disk basename differs from the canonical name,
                // schedule an in-place rename FIRST. The cover then targets
                // the new path. Phase ordering in pipeline::execute guarantees
                // the rename happens before the cover write.
                let canonical_dir = if current_basename == canonical {
                    title.folder.clone()
                } else {
                    let new_path = title
                        .folder
                        .parent()
                        .map(|p| p.join(&canonical))
                        .unwrap_or_else(|| title.folder.clone());
                    items.push(PlannedAction::RenameTitleFolder {
                        label: format!("Rename: {} → {}", current_basename, canonical),
                        src: title.folder.clone(),
                        dst: new_path.clone(),
                    });
                    new_path
                };

                items.push(PlannedAction::RestoreCover {
                    label: format!("Cover for {}", basename),
                    dir: canonical_dir,
                    jpg_backup: jpg,
                    ico_path: ico,
                    poster_url: poster,
                });
            }
            EntryKind::RestoreSeason { title_idx, season } => {
                let title = &report.titles[*title_idx];
                let lib = title.library;
                let lib_root = lib_root(report, lib);
                let canonical = m.canonical_folder_name();
                let season_dir = title
                    .seasons
                    .iter()
                    .find(|s| s.number == *season)
                    .map(|s| s.path.clone())
                    .unwrap_or_else(|| title.folder.join(format!("Season {season}")));
                let covers = lib_root.join("_covers");
                let basename = format!("{canonical} - Season {season}");
                let jpg = covers.join(format!("{basename}.jpg"));
                let ico = covers.join(format!("{basename}.ico"));
                items.push(PlannedAction::RestoreCover {
                    label: format!("Cover for {basename}"),
                    dir: season_dir,
                    jpg_backup: jpg,
                    ico_path: ico,
                    poster_url: poster,
                });
            }
            EntryKind::WrapOrphanVideo { issue_idx } => {
                if let Issue::OrphanVideo {
                    path,
                    library,
                    sidecars,
                } = &report.issues[*issue_idx]
                {
                    let lib_root = lib_root(report, *library);
                    let canonical = m.canonical_folder_name();
                    let new_dir = lib_root.join(&canonical);
                    let covers = lib_root.join("_covers");
                    let jpg = covers.join(format!("{canonical}.jpg"));
                    let ico = covers.join(format!("{canonical}.ico"));
                    items.push(PlannedAction::WrapVideoIntoFolder {
                        label: format!("Wrap orphan: {}", path.display()),
                        video: path.clone(),
                        new_dir: new_dir.clone(),
                        sidecars: sidecars.clone(),
                    });
                    items.push(PlannedAction::RestoreCover {
                        label: format!("Cover for {canonical}"),
                        dir: new_dir,
                        jpg_backup: jpg,
                        ico_path: ico,
                        poster_url: poster,
                    });
                }
            }
            EntryKind::UnparseableTitle { issue_idx } => {
                if let Issue::UnparseableFolder { path, library } = &report.issues[*issue_idx] {
                    let lib_root = lib_root(report, *library);
                    let canonical = m.canonical_folder_name();
                    let new_parent = lib_root.join(&canonical);
                    let covers = lib_root.join("_covers");
                    let jpg = covers.join(format!("{canonical}.jpg"));
                    let ico = covers.join(format!("{canonical}.ico"));
                    items.push(PlannedAction::WrapDirectoryIntoFolder {
                        label: format!("Wrap unparseable: {}", path.display()),
                        src_dir: path.clone(),
                        new_parent: new_parent.clone(),
                        rename_to: None,
                    });
                    items.push(PlannedAction::RestoreCover {
                        label: format!("Cover for {canonical}"),
                        dir: new_parent,
                        jpg_backup: jpg,
                        ico_path: ico,
                        poster_url: poster,
                    });
                }
            }
        }
    }

    ActionPlan {
        items,
        dry_run: false,
    }
}

fn lib_root(report: &ScanReport, kind: LibraryKind) -> PathBuf {
    // Find any title from that library, take its parent (= library root).
    if let Some(t) = report.titles.iter().find(|t| t.library == kind) {
        if let Some(p) = t.folder.parent() {
            return p.to_path_buf();
        }
    }
    // Fallback: search Issues for OrphanVideo / UnparseableFolder, take parent
    for i in &report.issues {
        match i {
            Issue::OrphanVideo { path, library, .. } if *library == kind => {
                if let Some(p) = path.parent() {
                    return p.to_path_buf();
                }
            }
            Issue::UnparseableFolder { path, library } if *library == kind => {
                if let Some(p) = path.parent() {
                    return p.to_path_buf();
                }
            }
            _ => {}
        }
    }
    PathBuf::new()
}

// ----- terminal setup/cleanup -----

fn setup_terminal() -> Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    use crossterm::{execute, terminal::*};
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alt screen")?;
    let backend = CrosstermBackend::new(stdout);
    let term = Terminal::new(backend).context("create terminal")?;
    Ok(term)
}

fn cleanup_terminal(term: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
    use crossterm::{execute, terminal::*};
    disable_raw_mode().context("disable raw mode")?;
    execute!(term.backend_mut(), LeaveAlternateScreen).context("leave alt screen")?;
    term.show_cursor().context("show cursor")?;
    Ok(())
}

// Helper trait so we can keep ScanReport but clone a lightweight view.
trait ScanReportClone {
    fn clone_titles(&self) -> ScanReport;
}

impl ScanReportClone for ScanReport {
    fn clone_titles(&self) -> ScanReport {
        ScanReport {
            titles: self.titles.clone(),
            issues: self.issues.clone(),
        }
    }
}
