use crate::app::{App, Form, Mode};
use crate::i18n::{self, s, Lang};
use crate::model::{now, Host};
use crate::probe::Status;
use crate::ssh;
use crate::tf;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Frame;

// Achromatic surfaces; color is used only for status.
const BG: Color = Color::Rgb(17, 17, 17);
const PANEL: Color = Color::Rgb(27, 27, 28);
const RAISED: Color = Color::Rgb(38, 38, 40);
const SEL: Color = Color::Rgb(44, 44, 47);
const TEXT: Color = Color::Rgb(236, 236, 236);
const MUTED: Color = Color::Rgb(150, 150, 150);
const DIM: Color = Color::Rgb(96, 96, 98);
const UP: Color = Color::Rgb(94, 214, 140);
const DOWN: Color = Color::Rgb(240, 113, 113);

fn st(fg: Color) -> Style {
    Style::new().fg(fg)
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Block::new().style(Style::new().bg(BG).fg(TEXT)), area);
    let [top, _, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_top(f, top.inner(Margin::new(1, 0)), app);
    draw_body(f, body, app);
    draw_footer(f, foot.inner(Margin::new(1, 0)), app);

    match &app.mode {
        Mode::Form(form) => draw_form(f, area, form),
        Mode::Confirm(i) => draw_confirm(f, area, &app.store.hosts[*i]),
        Mode::Help => draw_help(f, area, app),
        Mode::Lang(l) => draw_lang(f, area, *l),
        _ => {}
    }
}

fn draw_top(f: &mut Frame, area: Rect, app: &App) {
    let t = s();
    let total = app.store.hosts.len();
    let up = app
        .store
        .hosts
        .iter()
        .filter(|h| matches!(app.status_of(h), Some(Status::Up { .. })))
        .count();
    let left = Line::from(vec![
        Span::styled("sshm", st(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {}", t.subtitle), st(DIM)),
    ]);
    let mut right = vec![];
    if !app.filter.is_empty() {
        right.push(Span::styled(format!("/{}  ", app.filter), st(MUTED)));
        right.push(Span::styled(format!("{} · ", tf!(t.matches, app.visible().len())), st(DIM)));
    }
    right.push(Span::styled(tf!(t.servers, total), st(MUTED)));
    if total > 0 {
        right.push(Span::styled(" · ", st(DIM)));
        right.push(Span::styled(tf!(t.up_n, up), st(if up > 0 { UP } else { MUTED })));
    }
    // Language indicator: `l EN · TR`, the active one bright.
    let cur = i18n::lang();
    let lang_style = |l: Lang| if l == cur { st(TEXT).add_modifier(Modifier::BOLD) } else { st(DIM) };
    right.push(Span::styled("    l ", st(MUTED)));
    right.push(Span::styled("EN", lang_style(Lang::En)));
    right.push(Span::styled(" · ", st(DIM)));
    right.push(Span::styled("TR", lang_style(Lang::Tr)));
    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(Line::from(right)).right_aligned(), area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &App) {
    if area.width >= 78 {
        let list_w = (area.width * 2 / 5).clamp(30, 46);
        let [list, details] =
            Layout::horizontal([Constraint::Length(list_w), Constraint::Fill(1)]).spacing(1).areas(area);
        draw_list(f, list, app);
        draw_details(f, details.inner(Margin::new(1, 0)), app);
    } else {
        draw_list(f, area, app);
    }
}

fn status_cell(status: Option<&Status>) -> (&'static str, Color, String) {
    let t = s();
    match status {
        None => ("○", DIM, String::new()),
        Some(Status::Checking) => ("○", DIM, "…".into()),
        Some(Status::Up { ms, .. }) => ("●", UP, format!("{ms} ms")),
        Some(Status::Down(_)) => ("●", DOWN, t.cell_down.into()),
        Some(Status::Skipped) => ("◌", MUTED, t.cell_jump.into()),
    }
}

fn group_label(h: &Host) -> &str {
    if h.group.is_empty() { s().other_group } else { h.group.as_str() }
}

fn draw_list(f: &mut Frame, area: Rect, app: &App) {
    let t = s();
    let vis = app.visible();
    let inner = area.inner(Margin::new(1, 0));
    if vis.is_empty() {
        let lines = if app.store.hosts.is_empty() {
            vec![
                Line::styled(t.empty_title, st(TEXT)),
                Line::default(),
                Line::from(vec![Span::styled("a", st(TEXT)), Span::styled(format!("  {}", t.empty_add), st(MUTED))]),
                Line::from(vec![Span::styled("i", st(TEXT)), Span::styled(format!("  {}", t.empty_import), st(MUTED))]),
            ]
        } else {
            vec![Line::styled(t.no_match, st(TEXT)), Line::styled(t.no_match_hint, st(MUTED))]
        };
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return;
    }

    let width = inner.width as usize;
    let mut items: Vec<ListItem> = Vec::new();
    let mut sel_item = 0;
    let mut last_group: Option<String> = None;
    for (pos, &i) in vis.iter().enumerate() {
        let h = &app.store.hosts[i];
        let g = group_label(h).to_string();
        if last_group.as_deref() != Some(g.as_str()) {
            if last_group.is_some() {
                items.push(ListItem::new(Line::default()));
            }
            let count = vis.iter().filter(|&&j| group_label(&app.store.hosts[j]) == g).count();
            items.push(ListItem::new(Line::from(vec![
                Span::styled(g.clone(), st(MUTED).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {count}"), st(DIM)),
            ])));
            last_group = Some(g);
        }
        let (dot, dot_c, right) = status_cell(app.status_of(h));
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(dot, st(dot_c)),
            Span::raw("  "),
            Span::styled(h.name.clone(), st(TEXT)),
        ];
        let used: usize = spans.iter().map(|sp| sp.width()).sum();
        let pad = width.saturating_sub(used + right.chars().count() + 1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(right, st(DIM)));
        if pos == app.selected {
            sel_item = items.len();
        }
        items.push(ListItem::new(Line::from(spans)));
    }

    let mut state = ListState::default().with_selected(Some(sel_item));
    let list = List::new(items).highlight_style(Style::new().bg(SEL).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, inner, &mut state);
}

fn short_path(p: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Some(rest) = p.strip_prefix(&home.display().to_string()) {
            return format!("~{rest}");
        }
    }
    p.to_string()
}

fn draw_details(f: &mut Frame, area: Rect, app: &App) {
    let t = s();
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(i) = app.current() else { return };
    let h = &app.store.hosts[i];

    let row = |label: &str, value: Vec<Span<'static>>| -> Line<'static> {
        let mut spans = vec![Span::styled(format!("{label:<12}"), st(DIM))];
        spans.extend(value);
        Line::from(spans)
    };
    let txt = |v: String| vec![Span::styled(v, st(TEXT))];

    let mut lines = vec![
        Line::styled(h.name.clone(), st(TEXT).add_modifier(Modifier::BOLD)),
        Line::styled(h.target(), st(MUTED)),
        Line::default(),
    ];

    let status = app.status_of(h);
    let state = match status {
        None => vec![Span::styled(t.st_unchecked, st(MUTED))],
        Some(Status::Checking) => vec![Span::styled(t.st_checking, st(MUTED))],
        Some(Status::Up { ms, .. }) => {
            vec![Span::styled(t.st_up, st(UP)), Span::styled(format!(" · {ms} ms"), st(MUTED))]
        }
        Some(Status::Down(why)) => {
            vec![Span::styled(t.st_down, st(DOWN)), Span::styled(format!(" · {}", why.text()), st(MUTED))]
        }
        Some(Status::Skipped) => vec![Span::styled(t.st_skipped, st(MUTED))],
    };
    lines.push(row(t.r_status, state));
    if let Some(Status::Up { banner: Some(b), .. }) = status {
        lines.push(row(t.r_version, vec![Span::styled(b.clone(), st(MUTED))]));
    }
    lines.push(row(t.r_port, txt(h.port.to_string())));
    if !h.key.is_empty() {
        lines.push(row(t.r_key, txt(short_path(&h.key))));
    }
    if !h.jump.is_empty() {
        lines.push(row(t.r_jump, txt(h.jump.clone())));
    }
    if !h.tmux.is_empty() {
        lines.push(row(t.r_tmux, txt(tf!(t.persistent, h.tmux))));
    }
    if !h.forwards.is_empty() {
        lines.push(row(t.r_fwd, txt(h.forwards.join(", "))));
    }
    if !h.extra.is_empty() {
        lines.push(row(t.r_extra, txt(h.extra.clone())));
    }
    let last = match h.last_used {
        Some(ts) => format!("{} · {}", i18n::ago(now().saturating_sub(ts)), i18n::times(h.uses)),
        None => t.never_used.into(),
    };
    lines.push(row(t.r_last, vec![Span::styled(last, st(MUTED))]));
    if !h.notes.is_empty() {
        lines.push(Line::default());
        lines.push(Line::styled(h.notes.clone(), st(MUTED)));
    }

    let cmd = ssh::command_line(h, false);
    let cmd_h = ((cmd.chars().count() as u16) / inner.width.max(1) + 1).min(4);
    let [top, cmd_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(cmd_h)]).spacing(1).areas(inner);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), top);
    f.render_widget(Paragraph::new(Line::styled(cmd, st(DIM))).wrap(Wrap { trim: false }), cmd_area);
}

fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(k.to_string(), st(TEXT)));
        spans.push(Span::styled(format!(" {label}"), st(DIM)));
    }
    Line::from(spans)
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let t = s();
    if let Mode::Filter = app.mode {
        let line = Line::from(vec![Span::styled("/", st(MUTED)), Span::styled(app.filter.clone(), st(TEXT))]);
        f.render_widget(Paragraph::new(line), area);
        let x = area.x + 1 + Span::raw(app.filter.clone()).width() as u16;
        f.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), area.y));
        f.render_widget(Paragraph::new(hints(&[("⏎", t.k_ok), ("Esc", t.k_clear)])).right_aligned(), area);
        return;
    }
    if let Some(m) = &app.message {
        f.render_widget(Paragraph::new(Line::styled(m.text.clone(), st(if m.error { DOWN } else { TEXT }))), area);
        return;
    }
    let mut shown: Vec<(&str, &str)> = vec![
        ("⏎", t.k_connect),
        ("t", t.k_tab),
        ("f", t.k_sftp),
        ("/", t.k_search),
        ("a", t.k_add),
        ("e", t.k_edit),
        ("c", t.k_copy),
        ("l", t.k_lang),
        ("?", t.k_help),
        ("q", t.k_quit),
    ];
    // Drop from the middle when narrow so l, ? and q always stay.
    while hints(&shown).width() > area.width as usize && shown.len() > 4 {
        shown.remove(shown.len() - 4);
    }
    f.render_widget(Paragraph::new(hints(&shown)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn draw_form(f: &mut Frame, area: Rect, form: &Form) {
    let t = s();
    // padding + title + gap + fields + gap + hint + error + keys
    let want_h = form.fields.len() as u16 + 8;
    let rect = centered(area, 76, want_h);
    f.render_widget(Clear, rect);
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 4 || inner.width < 20 {
        return;
    }

    let [title_a, _, fields_a, _, hint_a, err_a, keys_a] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    f.render_widget(
        Paragraph::new(Line::styled(form.title.clone(), st(TEXT).add_modifier(Modifier::BOLD))),
        title_a,
    );

    let label_w = 13u16;
    let rows = fields_a.height as usize;
    let start = if form.focus >= rows { form.focus + 1 - rows } else { 0 };
    for (row, (idx, field)) in form.fields.iter().enumerate().skip(start).take(rows).enumerate() {
        let y = fields_a.y + row as u16;
        let focused = idx == form.focus;
        let label_rect = Rect::new(fields_a.x, y, label_w, 1);
        let val_rect = Rect::new(fields_a.x + label_w, y, fields_a.width.saturating_sub(label_w), 1);
        f.render_widget(
            Paragraph::new(Span::styled(field.label, st(if focused { TEXT } else { MUTED }))),
            label_rect,
        );
        let vw = val_rect.width.saturating_sub(2) as usize;
        let chars: Vec<char> = field.value.chars().collect();
        let scroll = field.cursor.saturating_sub(vw);
        let visible: String = chars.iter().skip(scroll).take(vw).collect();
        let style = if focused { Style::new().bg(RAISED).fg(TEXT) } else { Style::new().fg(TEXT) };
        let content = if field.value.is_empty() && !focused {
            Span::styled(" —", st(DIM))
        } else {
            Span::raw(format!(" {visible}"))
        };
        f.render_widget(Paragraph::new(Line::from(content)).style(style), val_rect);
        if focused {
            let before: String = chars[scroll..field.cursor].iter().collect();
            let cx = val_rect.x + 1 + Span::raw(before).width() as u16;
            f.set_cursor_position(Position::new(cx.min(val_rect.right().saturating_sub(1)), y));
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" ".repeat(label_w as usize + 1)),
            Span::styled(form.fields[form.focus].hint, st(DIM)),
        ])),
        hint_a,
    );
    if let Some(e) = &form.error {
        f.render_widget(Paragraph::new(Line::styled(e.clone(), st(DOWN))), err_a);
    }
    f.render_widget(
        Paragraph::new(hints(&[("Tab", t.k_next_field), ("Ctrl+S", t.k_save), ("Esc", t.k_cancel)])),
        keys_a,
    );
}

fn draw_confirm(f: &mut Frame, area: Rect, h: &Host) {
    let t = s();
    let rect = centered(area, 52, 6);
    f.render_widget(Clear, rect);
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines = vec![
        Line::from(vec![
            Span::styled(t.confirm_pre, st(TEXT)),
            Span::styled(h.name.clone(), st(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(t.confirm_post, st(TEXT)),
        ]),
        Line::default(),
        hints(&[("y", t.k_delete), ("…", t.k_any)]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_lang(f: &mut Frame, area: Rect, sel: Lang) {
    let rect = centered(area, 44, 10);
    f.render_widget(Clear, rect);
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(3, 3, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let opt = |l: Lang, label: &'static str| {
        if l == sel {
            Line::from(vec![
                Span::styled(" › ", st(TEXT)),
                Span::styled(label, st(TEXT).add_modifier(Modifier::BOLD)),
            ])
            .style(Style::new().bg(SEL))
        } else {
            Line::from(vec![Span::raw("   "), Span::styled(label, st(MUTED))])
        }
    };
    // Shown before a language is chosen, so the hint is bilingual.
    let lines = vec![
        Line::styled("Language · Dil", st(TEXT).add_modifier(Modifier::BOLD)),
        Line::default(),
        opt(Lang::En, "English"),
        opt(Lang::Tr, "Türkçe"),
        Line::default(),
        hints(&[("↑↓", "select / seç"), ("⏎", "OK / tamam")]),
        Line::styled("l: change later / sonra değiştir", st(DIM)),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let t = s();
    let mut lines = vec![Line::styled(t.help_title, st(TEXT).add_modifier(Modifier::BOLD)), Line::default()];
    for (k, d) in t.help {
        lines.push(Line::from(vec![
            Span::styled(format!("{k:<11}"), st(TEXT)),
            Span::styled(d.to_string(), st(MUTED)),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::styled(t.help_cli, st(MUTED)));
    lines.push(Line::styled(tf!(t.help_store, short_path(&app.path.display().to_string())), st(DIM)));
    let rect = centered(area, 78, lines.len() as u16 + 2);
    f.render_widget(Clear, rect);
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
