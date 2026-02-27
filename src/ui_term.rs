use std::time::Duration;

use termwiz::escape::csi::CursorStyle;
use tui::{
  layout::{Margin, Rect},
  style::{Color, Style},
  text::{Line, Span, Text},
  widgets::{Clear, Paragraph, Widget},
  Frame,
};

use crate::{
  proc::{
    view::{ProcViewFrame, SearchState},
    CopyMode, Pos, ReplySender,
  },
  state::{Scope, State},
  theme::Theme,
};

pub fn render_term(
  area: Rect,
  frame: &mut Frame,
  state: &mut State,
  cursor_style: &mut CursorStyle,
) {
  if area.width < 3 || area.height < 3 {
    return;
  }

  let theme = Theme::default();

  let active = match state.scope {
    Scope::Procs => false,
    Scope::Term | Scope::TermZoom => true,
  };

  if let Some(proc) = state.get_current_proc() {
    let mut title = Vec::with_capacity(4);
    title.push(Span::styled("Terminal", theme.pane_title(active)));
    match proc.copy_mode {
      CopyMode::None(_) => (),
      CopyMode::Active(_, _, _) => {
        title.push(Span::raw(" "));
        title.push(Span::styled("COPY MODE", theme.copy_mode_label()));
      }
    };
    if proc.search.is_some() {
      title.push(Span::raw(" "));
      title.push(Span::styled(
        "SEARCH",
        Style::default()
          .bg(Color::Blue)
          .fg(Color::Black)
          .add_modifier(tui::style::Modifier::BOLD),
      ));
    }

    let block = theme.pane(active).title(Line::from(title));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let has_search = proc.search.is_some();
    let search_height = if has_search { 1 } else { 0 };

    let content_area = if has_search {
      Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: area.height.saturating_sub(search_height),
      }
    } else {
      area
    };

    match &proc.lock_view() {
      ProcViewFrame::Empty => (),
      ProcViewFrame::Vt(vt) => {
        let search_ref = proc.search.as_ref();

        // Determine which screen to use:
        // 1. Copy mode: use frozen copy screen
        // 2. Search mode: use frozen search screen
        // 3. Normal mode: use live VT screen
        enum ScreenSource<'a> {
          Live(&'a crate::vt100::Screen<ReplySender>),
          Frozen(&'a crate::vt100::Screen<ReplySender>),
        }

        let (screen_source, cursor, cursor_style_override) = match &proc.copy_mode {
          CopyMode::Active(screen, start, end) => {
            let pos = end.as_ref().unwrap_or(start);
            let y = content_area.y as i32 + 1 + (pos.y + screen.scrollback() as i32);
            let cursor = if y >= 0 {
              Some((content_area.x + 1 + pos.x as u16, y as u16))
            } else {
              None
            };
            (ScreenSource::Frozen(screen), cursor, Some(CursorStyle::Default))
          }
          CopyMode::None(_) => {
            // Check if search mode has a frozen screen
            if let Some(search) = search_ref {
              if let Some(frozen_screen) = &search.screen {
                let cursor = None; // No cursor in search mode
                (ScreenSource::Frozen(frozen_screen), cursor, None)
              } else {
                let screen = vt.screen();
                let cursor = if screen.hide_cursor() {
                  None
                } else {
                  let cursor = screen.cursor_position();
                  Some((content_area.x + 1 + cursor.1, content_area.y + 1 + cursor.0))
                };
                (ScreenSource::Live(screen), cursor, Some(screen.cursor_style()))
              }
            } else {
              let screen = vt.screen();
              let cursor = if screen.hide_cursor() {
                None
              } else {
                let cursor = screen.cursor_position();
                Some((content_area.x + 1 + cursor.1, content_area.y + 1 + cursor.0))
              };
              (ScreenSource::Live(screen), cursor, Some(screen.cursor_style()))
            }
          }
        };

        let screen: &crate::vt100::Screen<ReplySender> = match &screen_source {
          ScreenSource::Live(s) => s,
          ScreenSource::Frozen(s) => s,
        };

        let term_area = content_area.inner(Margin {
          vertical: 1,
          horizontal: 1,
        });
        let term = UiTerm::new(screen, &proc.copy_mode, search_ref);
        frame.render_widget(term, term_area);

        if active {
          if let Some(cursor) = cursor {
            if cursor.0 < content_area.x + content_area.width
              && cursor.1 < content_area.y + content_area.height - search_height
            {
              frame.set_cursor(cursor.0, cursor.1);
            }
            if let Some(style) = cursor_style_override {
              *cursor_style = style;
            }
          }
        }

        if let Some(search) = search_ref {
          render_search_bar(
            Rect {
              x: area.x + 1,
              y: area.y + area.height - 2,
              width: area.width.saturating_sub(2),
              height: 1,
            },
            frame,
            search,
          );
        }
      }
      ProcViewFrame::Err(err) => {
        let text =
          Text::styled(*err, Style::default().fg(tui::style::Color::Red));
        frame.render_widget(
          Paragraph::new(text),
          content_area.inner(Margin {
            vertical: 1,
            horizontal: 1,
          }),
        );
      }
    }
  }
}

fn render_search_bar(area: Rect, frame: &mut Frame, search: &SearchState) {

  let mut spans = vec![Span::styled("? ", Style::default().fg(Color::Yellow))];

  let input_text = search.input.value();
  // Make query bold when confirmed
  let query_style = if search.confirmed {
    Style::default().add_modifier(tui::style::Modifier::BOLD)
  } else {
    Style::default()
  };
  spans.push(Span::styled(input_text, query_style));

  if search.confirmed && !search.matches.is_empty() {
    let counter = format!(" {}/{}", search.current + 1, search.matches.len());
    spans.push(Span::styled(
      counter,
      Style::default().fg(Color::Green).add_modifier(tui::style::Modifier::BOLD),
    ));
  }

  if search.matches.is_empty() && !search.input.value().is_empty() {
    // Check if we should show bold feedback (Enter was pressed with no matches)
    let show_bold = search
      .no_match_feedback
      .map(|t| t.elapsed() < Duration::from_millis(250))
      .unwrap_or(false);
    let style = if show_bold {
      Style::default()
        .fg(Color::Red)
        .add_modifier(tui::style::Modifier::BOLD)
    } else {
      Style::default().fg(Color::Red)
    };
    spans.push(Span::styled(" (no matches)", style));
  }

  let line = Line::from(spans);
  let paragraph = Paragraph::new(line);
  frame.render_widget(paragraph, area);

  // Only show cursor when editing (not confirmed)
  if !search.confirmed {
    let cursor_pos = search.input.cursor();
    frame.set_cursor(area.x + 2 + cursor_pos as u16, area.y);
  }
}

pub struct UiTerm<'a> {
  screen: &'a crate::vt100::Screen<ReplySender>,
  copy_mode: &'a CopyMode,
  search: Option<&'a SearchState>,
}

impl<'a> UiTerm<'a> {
  pub fn new(
    screen: &'a crate::vt100::Screen<ReplySender>,
    copy_mode: &'a CopyMode,
    search: Option<&'a SearchState>,
  ) -> Self {
    UiTerm {
      screen,
      copy_mode,
      search,
    }
  }
}

impl Widget for UiTerm<'_> {
  fn render(self, area: Rect, buf: &mut tui::buffer::Buffer) {
    let screen = self.screen;

    for row in 0..area.height {
      for col in 0..area.width {
        let to_cell = buf.get_mut(area.x + col, area.y + row);
        if let Some(cell) = screen.cell(row, col) {
          *to_cell = cell.to_tui();
          if !cell.has_contents() {
            to_cell.set_char(' ');
          }

          let copy_mode = match self.copy_mode {
            CopyMode::None(_) => None,
            CopyMode::Active(_, start, end) => {
              Some((start, end.as_ref().unwrap_or(start)))
            }
          };
          if let Some((start, end)) = copy_mode {
            if Pos::within(
              start,
              end,
              &Pos {
                y: (row as i32) - screen.scrollback() as i32,
                x: col as i32,
              },
            ) {
              to_cell.fg = Color::Black;
              to_cell.bg = Color::Cyan;
            }
          }

          if let Some(search) = self.search {
            let abs_row = screen.visible_row_abs_start() + row as usize;
            let col_usize = col as usize;

            for (i, (match_row, match_col)) in
              search.matches.iter().enumerate()
            {
              if *match_row == abs_row {
                let query_len = search.query_len();
                if col_usize >= *match_col
                  && col_usize < *match_col + query_len
                {
                  if i == search.current && search.confirmed {
                    to_cell.bg = Color::Red;
                    to_cell.fg = Color::White;
                  } else {
                    to_cell.bg = Color::Yellow;
                    to_cell.fg = Color::Black;
                  }
                  break;
                }
              }
            }
          }
        } else {
          to_cell.set_char('?');
        }
      }
    }

    let scrollback = screen.scrollback();
    if scrollback > 0 {
      let str = format!(" -{} ", scrollback);
      let width = str.len() as u16;
      let span = Span::styled(
        str,
        Style::reset()
          .bg(tui::style::Color::LightYellow)
          .fg(tui::style::Color::Black),
      );
      let x = area.x + area.width - width;
      let y = area.y;
      buf.set_span(x, y, &span, width);
    }
  }
}

pub fn term_check_hit(area: Rect, x: u16, y: u16) -> bool {
  area.x <= x
    && area.x + area.width >= x + 1
    && area.y <= y
    && area.y + area.height >= y + 1
}
