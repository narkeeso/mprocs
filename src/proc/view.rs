use crate::{
  config::ProcConfig,
  kernel::{kernel_message::SharedVt, proc::ProcId},
};

use super::{CopyMode, ReplySender};

use std::time::{Instant, Duration};

pub struct SearchState {
  pub input: tui_input::Input,
  pub matches: Vec<(usize, usize)>,
  pub current: usize,
  pub confirmed: bool,
  pub screen: Option<crate::vt100::Screen<ReplySender>>,
  pub no_match_feedback: Option<Instant>,
}

impl SearchState {
  pub fn new() -> Self {
    Self {
      input: tui_input::Input::default(),
      matches: Vec::new(),
      current: 0,
      confirmed: false,
      screen: None,
      no_match_feedback: None,
    }
  }

  pub fn next_match(&mut self) {
    if !self.matches.is_empty() {
      self.current = (self.current + 1) % self.matches.len();
    }
  }

  pub fn prev_match(&mut self) {
    if !self.matches.is_empty() {
      self.current = if self.current == 0 {
        self.matches.len() - 1
      } else {
        self.current - 1
      };
    }
  }

  pub fn run_search(&mut self, vt: &crate::vt100::Parser<ReplySender>) {
    self.matches.clear();
    let query = self.input.value();
    // Skip search if query is empty or only whitespace (leading spaces)
    if query.is_empty() || query.trim_start().is_empty() {
      return;
    }
    // Use trimmed query for matching (to avoid matching every space)
    let query = query.trim_start();
    // Smart case: case-insensitive when query is all lowercase,
    // case-sensitive when query contains any uppercase (like vim's smartcase)
    let smart_case = query.chars().all(|c| !c.is_uppercase());

    // Use frozen screen if available, otherwise use live VT
    let screen = self.screen.as_ref().unwrap_or_else(|| vt.screen());
    let total_rows = screen.total_rows();

    for row_idx in 0..total_rows {
      let row_text = screen.row_text(row_idx);
      if smart_case {
        let row_lower = row_text.to_lowercase();
        let query_lower = query.to_lowercase();
        for (match_idx, _) in row_lower.match_indices(&query_lower) {
          self.matches.push((row_idx, match_idx));
        }
      } else {
        for (match_idx, _) in row_text.match_indices(query) {
          self.matches.push((row_idx, match_idx));
        }
      }
    }

    if !self.matches.is_empty() {
      let visible_start = screen.visible_row_abs_start();
      self.current = self
        .matches
        .iter()
        .enumerate()
        .rev()
        .find(|(_, (row, _))| *row <= visible_start + screen.size().rows as usize)
        .map(|(i, _)| i)
        .unwrap_or(0);
    }
  }

  pub fn query_len(&self) -> usize {
    // Use trimmed length for match highlighting
    self.input.value().trim_start().len()
  }
}

/// Amount of time a process has to stay up for autorestart to trigger
pub const RESTART_THRESHOLD_SECONDS: f64 = 1.0;

#[derive(Clone, Copy)]
pub enum TargetState {
  None,
  Started,
  Stopped,
}

pub struct ProcView {
  pub id: ProcId,
  pub cfg: ProcConfig,

  pub is_up: bool,
  pub exit_code: Option<u32>,
  pub is_waiting: bool,
  pub vt: Option<SharedVt>,
  pub copy_mode: CopyMode,
  pub search: Option<SearchState>,

  pub target_state: TargetState,
  pub last_start: Option<Instant>,
  pub changed: bool,
}

impl ProcView {
  pub fn new(id: ProcId, cfg: ProcConfig) -> Self {
    Self {
      id,
      cfg,

      is_up: false,
      exit_code: None,
      is_waiting: false,
      vt: None,
      copy_mode: CopyMode::None(None),
      search: None,

      target_state: TargetState::None,
      last_start: None,
      changed: false,
    }
  }

  pub fn rename(&mut self, name: &str) {
    self.cfg.name.replace_range(.., name);
  }

  pub fn id(&self) -> ProcId {
    self.id
  }

  pub fn exit_code(&self) -> Option<u32> {
    self.exit_code
  }

  pub fn lock_view(&'_ self) -> ProcViewFrame<'_> {
    match &self.vt {
      None => ProcViewFrame::Empty,
      Some(vt) => vt
        .read()
        .map_or(ProcViewFrame::Empty, |vt| ProcViewFrame::Vt(vt)),
    }
  }

  pub fn name(&self) -> &str {
    &self.cfg.name
  }

  pub fn is_up(&self) -> bool {
    self.is_up
  }

  pub fn changed(&self) -> bool {
    self.changed
  }

  pub fn copy_mode(&self) -> &CopyMode {
    &self.copy_mode
  }

  pub fn focus(&mut self) {
    self.changed = false;
  }
}

pub enum ProcViewFrame<'a> {
  Empty,
  Vt(std::sync::RwLockReadGuard<'a, crate::vt100::Parser<ReplySender>>),
  Err(&'a str),
}
