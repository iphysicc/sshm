use crate::app::{App, Form, Mode};
use crate::model::{now, Host};
use crate::probe::Status;
use crate::ssh;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap};
use ratatui::Frame;

// Kromasız zemin; renk yalnız durum için.
const BG: Color = Color::Rgb(17, 17, 17);
const PANEL: Color = Color::Rgb(27, 27, 28);
const RAISED: Color = Color::Rgb(38, 38, 40);
const SEL: Color = Color::Rgb(44, 44, 47);
const TEXT: Color = Color::Rgb(236, 236, 236);
const MUTED: Color = Color::Rgb(150, 150, 150);
const DIM: Color = Color::Rgb(96, 96, 98);
const UP: Color = Color::Rgb(94, 214, 140);
const DOWN: Color = Color::Rgb(240, 113, 113);

fn s(fg: Color) -> Style {
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
        _ => {}
    }
}

fn draw_top(f: &mut Frame, area: Rect, app: &App) {
    let total = app.store.hosts.len();
    let up = app
        .store
        .hosts
        .iter()
        .filter(|h| matches!(app.status_of(h), Some(Status::Up { .. })))
        .count();
    let left = Line::from(vec![
        Span::styled("sshm", s(TEXT).add_modifier(Modifier::BOLD)),
        Span::styled("  bağlantılar", s(DIM)),
    ]);
    let mut right = vec![];
    if !app.filter.is_empty() {
        right.push(Span::styled(format!("/{}  ", app.filter), s(MUTED)));
        right.push(Span::styled(format!("{} eşleşme · ", app.visible().len()), s(DIM)));
    }
    right.push(Span::styled(format!("{total} sunucu"), s(MUTED)));
    if total > 0 {
        right.push(Span::styled(" · ", s(DIM)));
        right.push(Span::styled(format!("{up} açık"), s(if up > 0 { UP } else { MUTED })));
    }
    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(Line::from(right)).right_aligned(), area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &App) {
    let wide = area.width >= 78;
    if wide {
        let list_w = (area.width * 2 / 5).clamp(30, 46);
        let [list, details] =
            Layout::horizontal([Constraint::Length(list_w), Constraint::Fill(1)]).spacing(1).areas(area);
        draw_list(f, list, app);
        draw_details(f, details.inner(Margin::new(1, 0)), app);
    } else {
        draw_list(f, area, app);
    }
}

fn status_cell(st: Option<&Status>) -> (&'static str, Color, String) {
    match st {
        None => ("○", DIM, String::new()),
        Some(Status::Checking) => ("○", DIM, "…".into()),
        Some(Status::Up { ms, .. }) => ("●", UP, format!("{ms} ms")),
        Some(Status::Down(_)) => ("●", DOWN, "kapalı".into()),
        Some(Status::Skipped) => ("◌", MUTED, "atlama".into()),
    }
}

fn draw_list(f: &mut Frame, area: Rect, app: &App) {
    let vis = app.visible();
    let inner = area.inner(Margin::new(1, 0));
    if vis.is_empty() {
        let lines = if app.store.hosts.is_empty() {
            vec![
                Line::styled("Henüz bağlantı yok.", s(TEXT)),
                Line::default(),
                Line::from(vec![Span::styled("a", s(TEXT)), Span::styled("  yeni bağlantı ekle", s(MUTED))]),
                Line::from(vec![
                    Span::styled("i", s(TEXT)),
                    Span::styled("  Belgeler/Masaüstü'ndeki .bat dosyalarından al", s(MUTED)),
                ]),
            ]
        } else {
            vec![
                Line::styled("Eşleşen bağlantı yok.", s(TEXT)),
                Line::styled("Esc ile filtreyi temizle", s(MUTED)),
            ]
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
        let g = if h.group.is_empty() { "Diğer".to_string() } else { h.group.clone() };
        if last_group.as_deref() != Some(g.as_str()) {
            if last_group.is_some() {
                items.push(ListItem::new(Line::default()));
            }
            let count = vis.iter().filter(|&&j| {
                let hj = &app.store.hosts[j];
                (if hj.group.is_empty() { "Diğer" } else { hj.group.as_str() }) == g
            }).count();
            items.push(ListItem::new(Line::from(vec![
                Span::styled(g.clone(), s(MUTED).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {count}"), s(DIM)),
            ])));
            last_group = Some(g);
        }
        let (dot, dot_c, right) = status_cell(app.status_of(h));
        let left_spans = vec![
            Span::raw(" "),
            Span::styled(dot, s(dot_c)),
            Span::raw("  "),
            Span::styled(h.name.clone(), s(TEXT)),
        ];
        let used: usize = left_spans.iter().map(|sp| sp.width()).sum();
        let right_w = right.chars().count();
        let pad = width.saturating_sub(used + right_w + 1);
        let mut spans = left_spans;
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(right, s(DIM)));
        if pos == app.selected {
            sel_item = items.len();
        }
        items.push(ListItem::new(Line::from(spans)));
    }

    let mut state = ListState::default().with_selected(Some(sel_item));
    let list = List::new(items).highlight_style(Style::new().bg(SEL).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, inner, &mut state);
}

fn ago(ts: u64) -> String {
    let d = now().saturating_sub(ts);
    match d {
        0..=59 => "az önce".into(),
        60..=3599 => format!("{} dk önce", d / 60),
        3600..=86399 => format!("{} saat önce", d / 3600),
        _ => format!("{} gün önce", d / 86400),
    }
}

fn short_path(p: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let hs = home.display().to_string();
        if let Some(rest) = p.strip_prefix(&hs) {
            return format!("~{rest}");
        }
    }
    p.to_string()
}

fn draw_details(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let Some(i) = app.current() else { return };
    let h = &app.store.hosts[i];

    let row = |label: &str, value: Vec<Span<'static>>| -> Line<'static> {
        let mut spans = vec![Span::styled(format!("{label:<12}"), s(DIM))];
        spans.extend(value);
        Line::from(spans)
    };
    let txt = |v: String| vec![Span::styled(v, s(TEXT))];

    let mut lines = vec![
        Line::styled(h.name.clone(), s(TEXT).add_modifier(Modifier::BOLD)),
        Line::styled(h.target(), s(MUTED)),
        Line::default(),
    ];

    let st = app.status_of(h);
    let state = match st {
        None => vec![Span::styled("denetlenmedi", s(MUTED))],
        Some(Status::Checking) => vec![Span::styled("denetleniyor…", s(MUTED))],
        Some(Status::Up { ms, .. }) => vec![
            Span::styled("açık", s(UP)),
            Span::styled(format!(" · {ms} ms"), s(MUTED)),
        ],
        Some(Status::Down(why)) => vec![
            Span::styled("kapalı", s(DOWN)),
            Span::styled(format!(" · {why}"), s(MUTED)),
        ],
        Some(Status::Skipped) => vec![Span::styled("atlama üzerinden, doğrudan denetlenmez", s(MUTED))],
    };
    lines.push(row("Durum", state));
    if let Some(Status::Up { banner: Some(b), .. }) = st {
        lines.push(row("Sürüm", vec![Span::styled(b.clone(), s(MUTED))]));
    }
    lines.push(row("Port", txt(h.port.to_string())));
    if !h.key.is_empty() {
        lines.push(row("Anahtar", txt(short_path(&h.key))));
    }
    if !h.jump.is_empty() {
        lines.push(row("Atlama", txt(h.jump.clone())));
    }
    if !h.tmux.is_empty() {
        lines.push(row("tmux", txt(format!("{} (kalıcı oturum)", h.tmux))));
    }
    if !h.forwards.is_empty() {
        lines.push(row("Yönlendirme", txt(h.forwards.join(", "))));
    }
    if !h.extra.is_empty() {
        lines.push(row("Ek argüman", txt(h.extra.clone())));
    }
    let last = match h.last_used {
        Some(t) => format!("{} · {} kez", ago(t), h.uses),
        None => "hiç bağlanılmadı".into(),
    };
    lines.push(row("Son", vec![Span::styled(last, s(MUTED))]));
    if !h.notes.is_empty() {
        lines.push(Line::default());
        lines.push(Line::styled(h.notes.clone(), s(MUTED)));
    }

    let cmd = ssh::command_line(h, false);
    let cmd_h = ((cmd.chars().count() as u16) / inner.width.max(1) + 1).min(4);
    let [top, cmd_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(cmd_h)]).spacing(1).areas(inner);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), top);
    f.render_widget(
        Paragraph::new(Line::styled(cmd, s(DIM))).wrap(Wrap { trim: false }),
        cmd_area,
    );
}

fn hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled(k.to_string(), s(TEXT)));
        spans.push(Span::styled(format!(" {label}"), s(DIM)));
    }
    Line::from(spans)
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    match &app.mode {
        Mode::Filter => {
            let line = Line::from(vec![
                Span::styled("/", s(MUTED)),
                Span::styled(app.filter.clone(), s(TEXT)),
            ]);
            f.render_widget(Paragraph::new(line), area);
            let x = area.x + 1 + Span::raw(app.filter.clone()).width() as u16;
            f.set_cursor_position(Position::new(x.min(area.right().saturating_sub(1)), area.y));
            f.render_widget(
                Paragraph::new(hints(&[("⏎", "tamam"), ("Esc", "temizle")])).right_aligned(),
                area,
            );
        }
        _ => {
            if let Some(m) = &app.message {
                f.render_widget(
                    Paragraph::new(Line::styled(m.text.clone(), s(if m.error { DOWN } else { TEXT }))),
                    area,
                );
                return;
            }
            let all = [
                ("⏎", "bağlan"),
                ("t", "sekme"),
                ("f", "sftp"),
                ("/", "ara"),
                ("a", "ekle"),
                ("e", "düzenle"),
                ("c", "kopyala"),
                ("?", "tümü"),
                ("q", "çık"),
            ];
            // Sığmayanları baştan değil ortadan at: ? ve q hep kalsın.
            let mut shown: Vec<(&str, &str)> = all.to_vec();
            while hints(&shown).width() > area.width as usize && shown.len() > 3 {
                shown.remove(shown.len() - 3);
            }
            f.render_widget(Paragraph::new(hints(&shown)), area);
        }
    }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn draw_form(f: &mut Frame, area: Rect, form: &Form) {
    let n = form.fields.len() as u16;
    // başlık + boşluk + alanlar + boşluk + ipucu + hata + alt satır
    let want_h = n + 8;
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

    f.render_widget(Paragraph::new(Line::styled(form.title.clone(), s(TEXT).add_modifier(Modifier::BOLD))), title_a);

    let label_w = 13u16;
    let rows = fields_a.height as usize;
    let start = if form.focus >= rows { form.focus + 1 - rows } else { 0 };
    for (row, (idx, field)) in form.fields.iter().enumerate().skip(start).take(rows).enumerate() {
        let y = fields_a.y + row as u16;
        let focused = idx == form.focus;
        let label_rect = Rect::new(fields_a.x, y, label_w, 1);
        let val_rect = Rect::new(fields_a.x + label_w, y, fields_a.width.saturating_sub(label_w), 1);
        f.render_widget(
            Paragraph::new(Span::styled(field.label, s(if focused { TEXT } else { MUTED }))),
            label_rect,
        );
        let vw = val_rect.width.saturating_sub(2) as usize;
        let chars: Vec<char> = field.value.chars().collect();
        let scroll = if field.cursor > vw { field.cursor - vw } else { 0 };
        let visible: String = chars.iter().skip(scroll).take(vw).collect();
        let style = if focused { Style::new().bg(RAISED).fg(TEXT) } else { Style::new().fg(TEXT) };
        let content = if field.value.is_empty() && !focused {
            Span::styled(" —", s(DIM))
        } else {
            Span::raw(format!(" {visible}"))
        };
        f.render_widget(Paragraph::new(Line::from(content)).style(style), val_rect);
        if focused {
            let cx = val_rect.x + 1 + Span::raw(chars[scroll..field.cursor].iter().collect::<String>()).width() as u16;
            f.set_cursor_position(Position::new(cx.min(val_rect.right().saturating_sub(1)), y));
        }
    }

    let hint = form.fields[form.focus].hint;
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" ".repeat(label_w as usize + 1)),
            Span::styled(hint, s(DIM)),
        ])),
        hint_a,
    );
    if let Some(e) = &form.error {
        f.render_widget(Paragraph::new(Line::styled(e.clone(), s(DOWN))), err_a);
    }
    f.render_widget(
        Paragraph::new(hints(&[("Tab", "sonraki alan"), ("Ctrl+S", "kaydet"), ("Esc", "vazgeç")])),
        keys_a,
    );
}

fn draw_confirm(f: &mut Frame, area: Rect, h: &Host) {
    let rect = centered(area, 52, 6);
    f.render_widget(Clear, rect);
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines = vec![
        Line::from(vec![
            Span::styled(h.name.clone(), s(TEXT).add_modifier(Modifier::BOLD)),
            Span::styled(" silinsin mi?", s(TEXT)),
        ]),
        Line::default(),
        hints(&[("y", "sil"), ("herhangi bir tuş", "vazgeç")]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let keys: &[(&str, &str)] = &[
        ("⏎", "bu terminalde bağlan; çıkınca listeye dönülür"),
        ("t", "Windows Terminal'de yeni sekmede bağlan"),
        ("f", "sftp oturumu aç"),
        ("c", "ssh komutunu panoya kopyala"),
        ("/", "ara (ad, adres, grup, not); tek sonuçta ⏎ bağlanır"),
        ("a  e  y", "ekle · düzenle · kopyasını oluştur"),
        ("d", "sil"),
        ("r", "sunucuları yeniden denetle"),
        ("i", ".bat dosyalarından ve ~/.ssh/config'ten içe aktar"),
        ("j k  ↑ ↓", "gezin   g G  baş / son"),
        ("q", "çık"),
    ];
    let mut lines = vec![
        Line::styled("Kısayollar", s(TEXT).add_modifier(Modifier::BOLD)),
        Line::default(),
    ];
    for (k, d) in keys {
        lines.push(Line::from(vec![
            Span::styled(format!("{k:<11}"), s(TEXT)),
            Span::styled(d.to_string(), s(MUTED)),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::styled("Komut satırı: sshm <ad> doğrudan bağlanır, sshm --list listeler", s(MUTED)));
    lines.push(Line::styled(format!("Kayıtlar: {}", short_path(&app.path.display().to_string())), s(DIM)));
    let h = lines.len() as u16 + 2;
    let rect = centered(area, 74, h);
    f.render_widget(Clear, rect);
    let block = Block::new().style(Style::new().bg(PANEL)).padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
