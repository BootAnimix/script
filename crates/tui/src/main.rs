use bootanimix_core::{
    build_playback_schedule, build_project_to_archive, detect_ffmpeg, export_magisk_module,
    import_archive_to_project, import_frame_sequence_dir, validate_project, Diagnostic,
    DiagnosticSeverity, PlaybackSchedule, ProjectManifest, ScheduleOptions, ValidationReport,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceTab {
    Summary,
    PartsAndFrames,
    Diagnostics,
    Preview,
    Help,
}

impl WorkspaceTab {
    pub fn all() -> &'static [WorkspaceTab] {
        &[
            Self::Summary,
            Self::PartsAndFrames,
            Self::Diagnostics,
            Self::Preview,
            Self::Help,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Summary => "1:Summary",
            Self::PartsAndFrames => "2:Parts/Frames",
            Self::Diagnostics => "3:Diagnostics",
            Self::Preview => "4:Preview",
            Self::Help => "5:Help",
        }
    }
}

#[derive(Debug)]
pub enum WorkerEvent {
    Status(String),
    ProjectLoaded(Box<(ProjectManifest, PathBuf, ValidationReport, PlaybackSchedule)>),
    OperationDone(String),
    Error(String),
}

pub struct TuiApp {
    pub tab_index: usize,
    pub project_dir: Option<PathBuf>,
    pub manifest: Option<ProjectManifest>,
    pub validation: Option<ValidationReport>,
    pub schedule: Option<PlaybackSchedule>,
    pub selected_part: usize,
    pub preview_tick: usize,
    pub command_input: String,
    pub command_mode: bool,
    pub pending_confirmation: Option<(String, String)>,
    pub command_history: Vec<String>,
    pub history_cursor: Option<usize>,
    pub status_message: String,
    pub busy: bool,
    pub should_quit: bool,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
}

impl TuiApp {
    pub fn new(initial_project: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            tab_index: 0,
            project_dir: None,
            manifest: None,
            validation: None,
            schedule: None,
            selected_part: 0,
            preview_tick: 0,
            command_input: String::new(),
            command_mode: false,
            pending_confirmation: None,
            command_history: Vec::new(),
            history_cursor: None,
            status_message:
                "Ready. Press ':' for command bar, Tab/1-5 to switch panes, 'q' to quit."
                    .to_string(),
            busy: false,
            should_quit: false,
            tx,
            rx,
        };
        if let Some(path) = initial_project {
            app.open_project_sync(&path);
        }
        app
    }

    pub fn open_project_sync(&mut self, path: &Path) {
        let (manifest_file, dir) = if path.is_dir() {
            (path.join("project.bootanimix.json"), path.to_path_buf())
        } else {
            (
                path.to_path_buf(),
                path.parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf(),
            )
        };
        match ProjectManifest::load_from_file(&manifest_file) {
            Ok(manifest) => match validate_project(&manifest, &dir) {
                Ok(val) => {
                    let sched = build_playback_schedule(&manifest, ScheduleOptions::default());
                    self.status_message = format!("Loaded project '{}'", manifest.name);
                    self.project_dir = Some(dir);
                    self.manifest = Some(manifest);
                    self.validation = Some(val);
                    self.schedule = Some(sched);
                    self.selected_part = 0;
                    self.preview_tick = 0;
                }
                Err(e) => self.status_message = format!("Validation error: {e}"),
            },
            Err(e) => self.status_message = format!("Failed to load project: {e}"),
        }
    }

    pub fn poll_worker(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::Status(msg) => {
                    self.status_message = msg;
                }
                WorkerEvent::ProjectLoaded(boxed) => {
                    let (manifest, dir, val, sched) = *boxed;
                    self.busy = false;
                    self.status_message = format!("Project '{}' ready.", manifest.name);
                    self.project_dir = Some(dir);
                    self.manifest = Some(manifest);
                    self.validation = Some(val);
                    self.schedule = Some(sched);
                }
                WorkerEvent::OperationDone(msg) => {
                    self.busy = false;
                    self.status_message = msg;
                }
                WorkerEvent::Error(err) => {
                    self.busy = false;
                    self.status_message = format!("Error: {err}");
                }
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if let Some((_, cmd_to_run)) = self.pending_confirmation.take() {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                self.execute_command(&cmd_to_run, true);
            } else {
                self.status_message = "Operation cancelled.".to_string();
            }
            return;
        }

        if self.command_mode {
            match key.code {
                KeyCode::Esc => {
                    self.command_mode = false;
                    self.command_input.clear();
                }
                KeyCode::Enter => {
                    let cmd = self.command_input.trim().to_string();
                    self.command_mode = false;
                    self.command_input.clear();
                    if !cmd.is_empty() {
                        self.command_history.push(cmd.clone());
                        self.history_cursor = None;
                        self.execute_command(&cmd, false);
                    }
                }
                KeyCode::Backspace => {
                    self.command_input.pop();
                }
                KeyCode::Tab => {
                    self.autocomplete_command();
                }
                KeyCode::Up => {
                    if !self.command_history.is_empty() {
                        let next = match self.history_cursor {
                            Some(i) => i.saturating_sub(1),
                            None => self.command_history.len() - 1,
                        };
                        self.history_cursor = Some(next);
                        self.command_input = self.command_history[next].clone();
                    }
                }
                KeyCode::Char(c) => {
                    self.command_input.push(c);
                }
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char(':') => {
                self.command_mode = true;
                self.command_input.clear();
            }
            KeyCode::Tab => {
                self.tab_index = (self.tab_index + 1) % WorkspaceTab::all().len();
            }
            KeyCode::BackTab => {
                let len = WorkspaceTab::all().len();
                self.tab_index = (self.tab_index + len - 1) % len;
            }
            KeyCode::Char('1') => self.tab_index = 0,
            KeyCode::Char('2') => self.tab_index = 1,
            KeyCode::Char('3') => self.tab_index = 2,
            KeyCode::Char('4') => self.tab_index = 3,
            KeyCode::Char('5') => self.tab_index = 4,
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(m) = &self.manifest {
                    if !m.parts.is_empty() {
                        self.selected_part = (self.selected_part + 1) % m.parts.len();
                    }
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(m) = &self.manifest {
                    if !m.parts.is_empty() {
                        self.selected_part =
                            (self.selected_part + m.parts.len() - 1) % m.parts.len();
                    }
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(s) = &self.schedule {
                    if !s.frames.is_empty() {
                        self.preview_tick = (self.preview_tick + 1) % s.frames.len();
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(s) = &self.schedule {
                    if !s.frames.is_empty() {
                        self.preview_tick =
                            (self.preview_tick + s.frames.len() - 1) % s.frames.len();
                    }
                }
            }
            KeyCode::Char('v') => {
                if let (Some(m), Some(dir)) = (&self.manifest, &self.project_dir) {
                    if let Ok(val) = validate_project(m, dir) {
                        self.status_message = format!("Re-validated project: {:?}", val.outcome);
                        self.validation = Some(val);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn autocomplete_command(&mut self) {
        let known = [
            "open ", "import ", "build ", "validate", "magisk ", "doctor", "quit",
        ];
        for cand in known {
            if cand.starts_with(self.command_input.as_str()) {
                self.command_input = cand.to_string();
                return;
            }
        }
    }

    pub fn execute_command(&mut self, raw: &str, confirmed: bool) {
        let parts: Vec<&str> = raw.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }
        match parts[0] {
            "quit" | "q" => self.should_quit = true,
            "open" if parts.len() >= 2 => {
                let path = PathBuf::from(parts[1..].join(" "));
                self.open_project_sync(&path);
            }
            "validate" => {
                if let (Some(m), Some(dir)) = (&self.manifest, &self.project_dir) {
                    match validate_project(m, dir) {
                        Ok(val) => {
                            self.status_message = format!("Validation outcome: {:?}", val.outcome);
                            self.validation = Some(val);
                        }
                        Err(e) => self.status_message = format!("Validation failed: {e}"),
                    }
                } else {
                    self.status_message = "No project loaded.".to_string();
                }
            }
            "doctor" => {
                let ff = detect_ffmpeg(None);
                self.status_message = if ff.available {
                    format!("Doctor OK — FFmpeg available ({:?})", ff.version_line)
                } else {
                    "Doctor OK — FFmpeg not found (core ZIP & PNG tools active)".to_string()
                };
            }
            "import" if parts.len() >= 3 => {
                let src = PathBuf::from(parts[1]);
                let dst = PathBuf::from(parts[2]);
                if dst.exists() && !confirmed {
                    self.pending_confirmation = Some((
                        format!("Destination '{}' exists. Overwrite? (y/n)", dst.display()),
                        raw.to_string(),
                    ));
                    return;
                }
                self.busy = true;
                self.status_message = format!("Importing '{}'...", src.display());
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let res = if src.is_dir() {
                        import_frame_sequence_dir(&src, &dst, 30)
                    } else {
                        import_archive_to_project(&src, &dst)
                    };
                    match res {
                        Ok(m) => {
                            if let Ok(val) = validate_project(&m, &dst) {
                                let sched = build_playback_schedule(&m, ScheduleOptions::default());
                                let _ = tx.send(WorkerEvent::ProjectLoaded(Box::new((
                                    m, dst, val, sched,
                                ))));
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::Error(e.to_string()));
                        }
                    }
                });
            }
            "build" if parts.len() >= 2 => {
                let Some(manifest) = self.manifest.clone() else {
                    self.status_message = "Load a project first before building.".to_string();
                    return;
                };
                let Some(proj_dir) = self.project_dir.clone() else {
                    return;
                };
                let out_path = PathBuf::from(parts[1..].join(" "));
                if out_path.exists() && !confirmed {
                    self.pending_confirmation = Some((
                        format!("Output '{}' exists. Replace it? (y/n)", out_path.display()),
                        raw.to_string(),
                    ));
                    return;
                }
                self.busy = true;
                self.status_message = format!("Building '{}'...", out_path.display());
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    match build_project_to_archive(&manifest, &proj_dir, &out_path) {
                        Ok(rep) => {
                            let _ = tx.send(WorkerEvent::OperationDone(format!(
                                "Built {} ({} frames, {} bytes, ZIP_STORED verified)",
                                rep.output_path, rep.frame_count, rep.output_size_bytes
                            )));
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::Error(e.to_string()));
                        }
                    }
                });
            }
            "magisk" if parts.len() >= 2 => {
                let Some(manifest) = self.manifest.clone() else {
                    self.status_message = "Load a project first.".to_string();
                    return;
                };
                let Some(proj_dir) = self.project_dir.clone() else {
                    return;
                };
                let out_path = PathBuf::from(parts[1..].join(" "));
                self.busy = true;
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    match export_magisk_module(&manifest, &proj_dir, &out_path) {
                        Ok(rep) => {
                            let _ = tx.send(WorkerEvent::OperationDone(format!(
                                "Exported Magisk module '{}' (inner Stored verified={})",
                                rep.module_zip_path, rep.inner_stored_verified
                            )));
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerEvent::Error(e.to_string()));
                        }
                    }
                });
            }
            _ => {
                self.status_message = format!(
                    "Unknown command '{raw}'. Try :open <dir>, :import <src> <dst>, :build <out.zip>, :magisk <mod.zip>, :validate"
                );
            }
        }
    }

    pub fn render(&self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(f.area());

        let titles: Vec<Line> = WorkspaceTab::all()
            .iter()
            .map(|t| Line::from(Span::raw(t.title())))
            .collect();
        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" BootAnimix 3.0 Terminal Workspace "),
            )
            .select(self.tab_index)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(tabs, chunks[0]);

        let current_tab = WorkspaceTab::all()[self.tab_index];
        match current_tab {
            WorkspaceTab::Summary => self.render_summary(f, chunks[1]),
            WorkspaceTab::PartsAndFrames => self.render_parts(f, chunks[1]),
            WorkspaceTab::Diagnostics => self.render_diagnostics(f, chunks[1]),
            WorkspaceTab::Preview => self.render_preview(f, chunks[1]),
            WorkspaceTab::Help => self.render_help(f, chunks[1]),
        }

        let footer_title = if self.command_mode {
            " Command (: + Tab to autocomplete) "
        } else if self.busy {
            " Working... "
        } else {
            " Status "
        };
        let footer_text = if self.command_mode {
            format!(":{}", self.command_input)
        } else {
            self.status_message.clone()
        };
        let footer = Paragraph::new(footer_text)
            .block(Block::default().borders(Borders::ALL).title(footer_title));
        f.render_widget(footer, chunks[2]);

        if let Some((prompt_msg, _)) = &self.pending_confirmation {
            let popup_area = centered_rect(60, 25, f.area());
            f.render_widget(Clear, popup_area);
            let popup = Paragraph::new(prompt_msg.as_str()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Confirm Overwrite ")
                    .style(Style::default().fg(Color::Yellow)),
            );
            f.render_widget(popup, popup_area);
        }
    }

    fn render_summary(&self, f: &mut Frame, area: Rect) {
        let text = if let Some(m) = &self.manifest {
            let outcome_str = self
                .validation
                .as_ref()
                .map(|v| format!("{:?}", v.outcome))
                .unwrap_or_else(|| "Unchecked".to_string());
            vec![
                Line::from(format!("Project Name:   {}", m.name)),
                Line::from(format!(
                    "Resolution:     {}x{} @ {} FPS",
                    m.descriptor.width, m.descriptor.height, m.descriptor.fps
                )),
                Line::from(format!("Parts Count:    {}", m.parts.len())),
                Line::from(format!(
                    "Total Frames:   {}",
                    m.parts.iter().map(|p| p.frames.len()).sum::<usize>()
                )),
                Line::from(format!("Validation:     {outcome_str}")),
                Line::from(format!("Magisk ID:      {}", m.magisk.module_id)),
                Line::from(format!("Overlay Target: /{}", m.magisk.target_path)),
            ]
        } else {
            vec![
                Line::from("No project currently open."),
                Line::from(""),
                Line::from("Use ':open <project_dir>' or ':import <frames_or_zip> <project_dir>'"),
            ]
        };
        let p = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Project Summary "),
        );
        f.render_widget(p, area);
    }

    fn render_parts(&self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        if let Some(m) = &self.manifest {
            let items: Vec<ListItem> = m
                .descriptor
                .parts
                .iter()
                .enumerate()
                .map(|(idx, p)| {
                    let prefix = if idx == self.selected_part {
                        ">> "
                    } else {
                        "   "
                    };
                    ListItem::new(format!(
                        "{}{} (mode={:?}, repeat={:?}, pause={}f)",
                        prefix, p.path, p.mode, p.repeat, p.pause_frames
                    ))
                })
                .collect();
            f.render_widget(
                List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Parts (j/k) "),
                ),
                cols[0],
            );

            let frame_items: Vec<ListItem> = m
                .parts
                .get(self.selected_part)
                .map(|part| {
                    part.frames
                        .iter()
                        .take(50)
                        .enumerate()
                        .map(|(i, fr)| {
                            ListItem::new(format!(
                                "#{i:04}: {} (trim={:?})",
                                fr.relative_path, fr.trim
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            f.render_widget(
                List::new(frame_items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Frames in Selected Part "),
                ),
                cols[1],
            );
        } else {
            f.render_widget(
                Paragraph::new("No project loaded.")
                    .block(Block::default().borders(Borders::ALL).title(" Parts ")),
                area,
            );
        }
    }

    fn render_diagnostics(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = if let Some(val) = &self.validation {
            if val.diagnostics.is_empty() {
                vec![ListItem::new(
                    "No diagnostics — project is clean and valid.",
                )]
            } else {
                val.diagnostics
                    .iter()
                    .map(|d: &Diagnostic| {
                        let color = match d.severity {
                            DiagnosticSeverity::Error => Color::Red,
                            DiagnosticSeverity::Warning => Color::Yellow,
                            DiagnosticSeverity::Info => Color::Cyan,
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("[{}] ", d.code), Style::default().fg(color)),
                            Span::raw(format!("{} — {}", d.message, d.remediation)),
                        ]))
                    })
                    .collect()
            }
        } else {
            vec![ListItem::new("No validation report available.")]
        };
        f.render_widget(
            List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Structured Diagnostics ('v' to refresh) "),
            ),
            area,
        );
    }

    fn render_preview(&self, f: &mut Frame, area: Rect) {
        let lines = if let Some(sched) = &self.schedule {
            let cur = sched.frames.get(self.preview_tick);
            let mut out = vec![
                Line::from(format!(
                    "Canvas: {}x{} @ {} FPS | Infinite loop: {} | Finite duration: {:?}",
                    sched.width,
                    sched.height,
                    sched.fps,
                    sched.is_infinite,
                    sched.finite_duration_seconds
                )),
                Line::from(format!(
                    "Timeline Scrub (h/l or Left/Right): tick {} / {}",
                    self.preview_tick,
                    sched.frames.len().saturating_sub(1)
                )),
            ];
            if let Some(frame) = cur {
                out.push(Line::from(format!(
                    "Current Frame: part='{}' (loop #{}) frame_in_part={} path='{}' pause_hold={} opacity={:.2}",
                    frame.part_name,
                    frame.loop_iteration,
                    frame.frame_index_in_part,
                    frame.relative_path,
                    frame.is_pause_hold,
                    frame.fade_opacity
                )));
            }
            for w in &sched.fidelity_warnings {
                out.push(Line::from(format!("Fidelity note: {w}")));
            }
            out
        } else {
            vec![Line::from("No preview schedule loaded.")]
        };
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Playback Scheduler Inspector "),
            ),
            area,
        );
    }

    fn render_help(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from("Keyboard Shortcuts:"),
            Line::from("  Tab / Shift+Tab / 1..5   Switch workspace panes"),
            Line::from("  j / k or Up / Down       Select animation part"),
            Line::from("  h / l or Left / Right    Step preview timeline tick"),
            Line::from("  v                        Re-run project validator"),
            Line::from("  :                        Enter command prompt (Tab autocompletes, Up recalls history)"),
            Line::from("  q                        Quit TUI cleanly"),
            Line::from(""),
            Line::from("Commands:"),
            Line::from("  :open <project_dir>"),
            Line::from("  :import <frames_or_zip> <project_dir>"),
            Line::from("  :build <output_bootanimation.zip>"),
            Line::from("  :magisk <output_module.zip>"),
            Line::from("  :validate"),
            Line::from("  :doctor"),
        ];
        f.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Help ")),
            area,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn main() -> io::Result<()> {
    let initial_arg = std::env::args().nth(1).map(PathBuf::from);
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = TuiApp::new(initial_arg);

    while !app.should_quit {
        app.poll_worker();
        terminal.draw(|f| app.render(f))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootanimix_core::encode_solid_png;
    use crossterm::event::KeyModifiers;
    use ratatui::backend::TestBackend;

    #[test]
    fn tui_renders_and_executes_shared_core_workflows_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let frames_dir = dir.path().join("frames");
        std::fs::create_dir_all(&frames_dir).unwrap();
        let png = encode_solid_png(64, 64, [10, 120, 240, 255]).unwrap();
        std::fs::write(frames_dir.join("000.png"), &png).unwrap();

        let proj_dir = dir.path().join("proj");
        import_frame_sequence_dir(&frames_dir, &proj_dir, 30).unwrap();

        let mut app = TuiApp::new(Some(proj_dir.clone()));
        assert!(app.manifest.is_some());

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render all 5 tabs and verify no panics or terminal corruption
        for idx in 0..WorkspaceTab::all().len() {
            app.tab_index = idx;
            terminal.draw(|f| app.render(f)).unwrap();
        }

        // Test command autocomplete & execution via ':'
        app.handle_key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE));
        assert!(app.command_mode);
        app.command_input = "doc".to_string();
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(app.command_input, "doctor");
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!app.command_mode);
        assert!(app.status_message.contains("Doctor OK"));
    }
}
