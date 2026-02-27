use tui::{
  layout::{Margin, Rect},
  style::{Color, Style},
  text::{Line, Span, Text},
  widgets::{Clear, Paragraph},
  Frame,
};

use crate::{
  encode_term::print_key,
  event::AppEvent,
  key::Key,
  keymap::{Keymap, KeymapGroup},
  state::State,
  theme::Theme,
};
use crossterm::event::{KeyCode, KeyModifiers};

pub fn render_keymap(
  area: Rect,
  frame: &mut Frame,
  state: &mut State,
  keymap: &Keymap,
) {
  let theme = Theme::default();

  let block = theme
    .pane(false)
    .title(Span::styled("Help", theme.pane_title(false)));
  frame.render_widget(Clear, area);
  frame.render_widget(block, area);

  let group = state.get_keymap_group();

  // Check if search is active and confirmed
  let search_info = state.get_current_proc()
    .and_then(|p| p.search.as_ref().map(|s| (s.confirmed, s.input.value().is_empty())));

  // Build keymap display items
  let mut spans: Vec<Span> = Vec::new();

  if group == KeymapGroup::Term && search_info.is_some() {
    let (is_confirmed, is_empty) = search_info.unwrap();
    // Search mode: show hardcoded keys for search actions

    // SearchLeave - hardcoded Esc (always show)
    spans.push(Span::raw(" <"));
    spans.push(Span::styled(
      print_key(&Key::new(KeyCode::Esc, KeyModifiers::NONE)),
      Style::default().fg(Color::Yellow),
    ));
    spans.push(Span::raw(": "));
    spans.push(Span::raw(AppEvent::SearchLeave.desc()));
    spans.push(Span::raw("> "));

    // Only show n/N navigation hints after search is confirmed (Enter pressed)
    if is_confirmed && !is_empty {
      // SearchNext - Enter
      spans.push(Span::raw(" <"));
      spans.push(Span::styled(
        print_key(&Key::new(KeyCode::Enter, KeyModifiers::NONE)),
        Style::default().fg(Color::Yellow),
      ));
      spans.push(Span::raw(": "));
      spans.push(Span::raw(AppEvent::SearchNext.desc()));
      spans.push(Span::raw("> "));

      // SearchPrev - Shift+Enter
      spans.push(Span::raw(" <"));
      spans.push(Span::styled(
        print_key(&Key::new(KeyCode::Enter, KeyModifiers::SHIFT)),
        Style::default().fg(Color::Yellow),
      ));
      spans.push(Span::raw(": "));
      spans.push(Span::raw(AppEvent::SearchPrev.desc()));
      spans.push(Span::raw("> "));
    }
  } else {
    // Normal mode: use keymap lookups
    let items = match group {
      KeymapGroup::Procs => vec![
        AppEvent::ToggleFocus,
        AppEvent::Quit,
        AppEvent::NextProc,
        AppEvent::PrevProc,
        AppEvent::StartProc,
        AppEvent::TermProc,
        AppEvent::RestartProc,
        AppEvent::ToggleKeymapWindow,
      ],
      KeymapGroup::Term => {
        vec![AppEvent::ToggleFocus, AppEvent::SearchEnter]
      }
      KeymapGroup::Copy => vec![
        AppEvent::CopyModeEnd,
        AppEvent::CopyModeCopy,
        AppEvent::CopyModeLeave,
      ],
    };

    for (key, event) in items.into_iter().filter_map(|event| {
      keymap.resolve_key(group, &event).map(|k| (k, event))
    }) {
      spans.push(Span::raw(" <"));
      spans.push(Span::styled(print_key(key), Style::default().fg(Color::Yellow)));
      spans.push(Span::raw(": "));
      spans.push(Span::raw(event.desc()));
      spans.push(Span::raw("> "));
    }
  }

  let line = Line::from(spans);
  let line = Text::from(vec![line]);

  let p = Paragraph::new(line);
  frame.render_widget(
    p,
    area.inner(Margin {
      vertical: 1,
      horizontal: 1,
    }),
  );
}
