use crate::i18n::{self, s};
use crate::import;
use crate::tf;
use crate::model::{Host, Store};
use crate::probe::{self, Status};
use crate::ssh;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

pub struct Message {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

pub enum Pending {
    Connect(usize),
    Sftp(usize),
}

pub enum Mode {
    Normal,
    Filter,
    Form(Form),
    Confirm(usize),
    Help,
}

pub struct App {
    pub store: Store,
    pub path: PathBuf,
    pub filter: String,
    pub selected: usize,
    pub mode: Mode,
    pub status: HashMap<String, Status>,
    tx: Sender<(String, Status)>,
    rx: Receiver<(String, Status)>,
    pub message: Option<Message>,
    pub quit: bool,
    pub pending: Option<Pending>,
}

impl App {
    pub fn new(store: Store, path: PathBuf) -> App {
        let (tx, rx) = mpsc::channel();
        App {
            store,
            path,
            filter: String::new(),
            selected: 0,
            mode: Mode::Normal,
            status: HashMap::new(),
            tx,
            rx,
            message: None,
            quit: false,
            pending: None,
        }
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message { text: text.into(), error: false, at: Instant::now() });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message { text: text.into(), error: true, at: Instant::now() });
    }

    pub fn tick(&mut self) {
        while let Ok((k, s)) = self.rx.try_recv() {
            self.status.insert(k, s);
        }
        if let Some(m) = &self.message {
            if m.at.elapsed() > Duration::from_secs(6) {
                self.message = None;
            }
        }
    }

    pub fn save(&mut self) {
        if let Err(e) = self.store.save(&self.path) {
            self.error(tf!(s().save_failed, format!("{e:#}")));
        }
    }

    fn probe_host(&mut self, i: usize) {
        let h = &self.store.hosts[i];
        let key = h.probe_key();
        if !h.jump.is_empty() {
            self.status.insert(key, Status::Skipped);
            return;
        }
        if matches!(self.status.get(&key), Some(Status::Checking)) {
            return;
        }
        self.status.insert(key.clone(), Status::Checking);
        probe::spawn(key, h.host.clone(), h.port, self.tx.clone());
    }

    pub fn probe_all(&mut self) {
        for i in 0..self.store.hosts.len() {
            self.probe_host(i);
        }
    }

    pub fn status_of(&self, h: &Host) -> Option<&Status> {
        self.status.get(&h.probe_key())
    }

    /// Indices of filtered entries, sorted by group then name.
    pub fn visible(&self) -> Vec<usize> {
        let terms: Vec<String> = self.filter.to_lowercase().split_whitespace().map(String::from).collect();
        let hosts = &self.store.hosts;
        let mut v: Vec<usize> = (0..hosts.len())
            .filter(|&i| {
                let h = &hosts[i];
                let hay = format!("{} {} {} {} {}", h.name, h.target(), h.group, h.notes, h.jump).to_lowercase();
                terms.iter().all(|t| hay.contains(t))
            })
            .collect();
        v.sort_by(|&a, &b| {
            let (ha, hb) = (&hosts[a], &hosts[b]);
            (ha.group.is_empty(), ha.group.to_lowercase(), ha.name.to_lowercase())
                .cmp(&(hb.group.is_empty(), hb.group.to_lowercase(), hb.name.to_lowercase()))
        });
        v
    }

    pub fn current(&self) -> Option<usize> {
        self.visible().get(self.selected).copied()
    }

    fn clamp(&mut self) {
        let n = self.visible().len();
        if self.selected >= n {
            self.selected = n.saturating_sub(1);
        }
    }

    fn select_index(&mut self, store_idx: usize) {
        if let Some(pos) = self.visible().iter().position(|&i| i == store_idx) {
            self.selected = pos;
        }
    }

    pub fn on_paste(&mut self, text: &str) {
        let clean: String = text.chars().filter(|c| !c.is_control()).collect();
        match &mut self.mode {
            Mode::Form(f) => f.insert_str(&clean),
            Mode::Filter => {
                self.filter.push_str(&clean);
                self.selected = 0;
            }
            _ => {}
        }
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            match self.mode {
                Mode::Normal => self.quit = true,
                _ => self.mode = Mode::Normal,
            }
            return;
        }
        let mode = std::mem::replace(&mut self.mode, Mode::Normal);
        self.mode = match mode {
            Mode::Normal => {
                self.key_normal(k);
                // key_normal may have changed the mode
                std::mem::replace(&mut self.mode, Mode::Normal)
            }
            Mode::Filter => self.key_filter(k),
            Mode::Form(f) => self.key_form(f, k),
            Mode::Confirm(i) => self.key_confirm(i, k),
            Mode::Help => Mode::Normal,
        };
    }

    fn key_normal(&mut self, k: KeyEvent) {
        let n = self.visible().len();
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.clamp();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if n > 0 {
                    self.selected = (self.selected + 1) % n;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if n > 0 {
                    self.selected = (self.selected + n - 1) % n;
                }
            }
            KeyCode::PageDown => self.selected = (self.selected + 10).min(n.saturating_sub(1)),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = n.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(i) = self.current() {
                    self.pending = Some(Pending::Connect(i));
                }
            }
            KeyCode::Char('f') => {
                if let Some(i) = self.current() {
                    self.pending = Some(Pending::Sftp(i));
                }
            }
            KeyCode::Char('t') => {
                if let Some(i) = self.current() {
                    let h = self.store.hosts[i].clone();
                    match ssh::open_tab(&h) {
                        Ok(()) => {
                            self.info(tf!(s().opened_tab, h.name));
                        }
                        Err(e) => self.error(format!("{e:#}")),
                    }
                }
            }
            KeyCode::Char('c') => {
                if let Some(i) = self.current() {
                    let cmd = ssh::command_line(&self.store.hosts[i], true);
                    match ssh::copy_to_clipboard(&cmd) {
                        Ok(()) => self.info(s().copied),
                        Err(e) => self.error(format!("{e:#}")),
                    }
                }
            }
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Char('a') => {
                self.mode = Mode::Form(Form::new(s().form_new, None, &Host::default()));
            }
            KeyCode::Char('e') => {
                if let Some(i) = self.current() {
                    let h = self.store.hosts[i].clone();
                    self.mode = Mode::Form(Form::new(&tf!(s().form_edit, h.name), Some(i), &h));
                }
            }
            KeyCode::Char('y') => {
                if let Some(i) = self.current() {
                    let mut h = self.store.hosts[i].clone();
                    h.name = self.store.unique_name(&format!("{}-{}", h.name, s().copy_suffix));
                    h.last_used = None;
                    h.uses = 0;
                    self.mode = Mode::Form(Form::new(s().form_copy, None, &h));
                }
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if let Some(i) = self.current() {
                    self.mode = Mode::Confirm(i);
                }
            }
            KeyCode::Char('r') => {
                self.status.clear();
                self.probe_all();
                self.info(s().reprobing);
            }
            KeyCode::Char('i') => self.import(),
            KeyCode::Char('l') => {
                let l = i18n::lang().toggle();
                i18n::set(l);
                self.store.lang = Some(l.code().to_string());
                self.save();
                self.info(s().lang_switched);
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    pub fn import(&mut self) {
        let found = import::discover();
        let before = self.store.hosts.len();
        let added = import::merge(&mut self.store, found);
        if added.is_empty() {
            self.info(s().import_none);
            return;
        }
        self.save();
        for i in before..self.store.hosts.len() {
            self.probe_host(i);
        }
        self.info(tf!(s().imported_n, added.len(), added.join(", ")));
    }

    fn key_filter(&mut self, k: KeyEvent) -> Mode {
        match k.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.clamp();
                Mode::Normal
            }
            KeyCode::Enter => {
                if self.visible().len() == 1 {
                    self.selected = 0;
                    self.pending = self.current().map(Pending::Connect);
                }
                Mode::Normal
            }
            KeyCode::Down => {
                let n = self.visible().len();
                if n > 0 {
                    self.selected = (self.selected + 1) % n;
                }
                Mode::Filter
            }
            KeyCode::Up => {
                let n = self.visible().len();
                if n > 0 {
                    self.selected = (self.selected + n - 1) % n;
                }
                Mode::Filter
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
                Mode::Filter
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.selected = 0;
                Mode::Filter
            }
            _ => Mode::Filter,
        }
    }

    fn key_confirm(&mut self, i: usize, k: KeyEvent) -> Mode {
        if matches!(k.code, KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter) {
            let h = self.store.hosts.remove(i);
            self.save();
            self.clamp();
            self.info(tf!(s().deleted, h.name));
        }
        Mode::Normal
    }

    fn key_form(&mut self, mut f: Form, k: KeyEvent) -> Mode {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return Mode::Normal,
            KeyCode::Char('s') if ctrl => return self.submit(f),
            KeyCode::Enter if f.focus + 1 == f.fields.len() => return self.submit(f),
            KeyCode::Enter | KeyCode::Tab | KeyCode::Down => f.focus = (f.focus + 1) % f.fields.len(),
            KeyCode::BackTab | KeyCode::Up => f.focus = (f.focus + f.fields.len() - 1) % f.fields.len(),
            KeyCode::Left => f.field().left(),
            KeyCode::Right => f.field().right(),
            KeyCode::Home => f.field().cursor = 0,
            KeyCode::End => f.field().end(),
            KeyCode::Backspace => f.field().backspace(),
            KeyCode::Delete => f.field().delete(),
            KeyCode::Char('u') if ctrl => {
                let fl = f.field();
                fl.value.clear();
                fl.cursor = 0;
            }
            KeyCode::Char(c) if !ctrl => f.insert_str(&c.to_string()),
            _ => {}
        }
        Mode::Form(f)
    }

    fn submit(&mut self, mut f: Form) -> Mode {
        let base = match f.editing {
            Some(i) => self.store.hosts[i].clone(),
            None => Host::default(),
        };
        let h = match f.to_host(&base, &self.store) {
            Ok(h) => h,
            Err(e) => {
                f.error = Some(e);
                return Mode::Form(f);
            }
        };
        let name = h.name.clone();
        let idx = match f.editing {
            Some(i) => {
                self.store.hosts[i] = h;
                i
            }
            None => {
                self.store.hosts.push(h);
                self.store.hosts.len() - 1
            }
        };
        self.save();
        self.status.remove(&self.store.hosts[idx].probe_key());
        self.probe_host(idx);
        self.select_index(idx);
        self.info(tf!(s().saved, name));
        Mode::Normal
    }
}

pub struct Field {
    pub label: &'static str,
    pub hint: &'static str,
    pub value: String,
    /// in chars, not bytes
    pub cursor: usize,
}

impl Field {
    fn new((label, hint): (&'static str, &'static str), value: String) -> Field {
        let cursor = value.chars().count();
        Field { label, hint, value, cursor }
    }

    fn byte_at(&self, ci: usize) -> usize {
        self.value.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(self.value.len())
    }

    fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.chars().count());
    }

    fn end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            let b = self.byte_at(self.cursor - 1);
            self.value.remove(b);
            self.cursor -= 1;
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.value.chars().count() {
            let b = self.byte_at(self.cursor);
            self.value.remove(b);
        }
    }
}

pub const F_NAME: usize = 0;
const F_HOST: usize = 1;
const F_USER: usize = 2;
const F_PORT: usize = 3;
const F_KEY: usize = 4;
const F_GROUP: usize = 5;
const F_JUMP: usize = 6;
const F_TMUX: usize = 7;
const F_FWD: usize = 8;
const F_EXTRA: usize = 9;
const F_NOTES: usize = 10;

pub struct Form {
    pub title: String,
    pub editing: Option<usize>,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
}

impl Form {
    pub fn new(title: &str, editing: Option<usize>, h: &Host) -> Form {
        let t = s();
        let fields = vec![
            Field::new(t.f_name, h.name.clone()),
            Field::new(t.f_host, h.host.clone()),
            Field::new(t.f_user, h.user.clone()),
            Field::new(t.f_port, if h.port == 22 { String::new() } else { h.port.to_string() }),
            Field::new(t.f_key, h.key.clone()),
            Field::new(t.f_group, h.group.clone()),
            Field::new(t.f_jump, h.jump.clone()),
            Field::new(t.f_tmux, h.tmux.clone()),
            Field::new(t.f_fwd, h.forwards.join(", ")),
            Field::new(t.f_extra, h.extra.clone()),
            Field::new(t.f_notes, h.notes.clone()),
        ];
        Form { title: title.to_string(), editing, fields, focus: 0, error: None }
    }

    pub fn field(&mut self) -> &mut Field {
        &mut self.fields[self.focus]
    }

    pub fn insert_str(&mut self, s: &str) {
        let fl = self.field();
        let b = fl.byte_at(fl.cursor);
        fl.value.insert_str(b, s);
        fl.cursor += s.chars().count();
        self.error = None;
    }

    fn v(&self, i: usize) -> String {
        self.fields[i].value.trim().to_string()
    }

    fn to_host(&self, base: &Host, store: &Store) -> Result<Host, String> {
        let t = s();
        let name = self.v(F_NAME);
        if name.is_empty() {
            return Err(t.e_name_empty.into());
        }
        if name.starts_with('-') || name.contains([';', '"', '\'']) {
            return Err(t.e_name_chars.into());
        }
        if store.name_taken(&name, self.editing) {
            return Err(tf!(t.e_name_taken, name));
        }
        let host = self.v(F_HOST);
        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err(t.e_host.into());
        }
        let user = self.v(F_USER);
        if user.contains(char::is_whitespace) {
            return Err(t.e_user.into());
        }
        let port_s = self.v(F_PORT);
        let port: u16 = if port_s.is_empty() {
            22
        } else {
            match port_s.parse::<u16>() {
                Ok(p) if p > 0 => p,
                _ => return Err(t.e_port.into()),
            }
        };
        let key = self.v(F_KEY).trim_matches('"').to_string();
        if !key.is_empty() && !Path::new(&ssh::expand_tilde(&key)).exists() {
            return Err(tf!(t.e_key, key));
        }
        let tmux = self.v(F_TMUX);
        if !tmux.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
            return Err(t.e_tmux.into());
        }
        let forwards: Vec<String> = self
            .v(F_FWD)
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Host {
            name,
            host,
            user,
            port,
            key,
            group: self.v(F_GROUP),
            jump: self.v(F_JUMP),
            tmux,
            forwards,
            extra: self.v(F_EXTRA),
            notes: self.v(F_NOTES),
            last_used: base.last_used,
            uses: base.uses,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn typ(app: &mut App, s: &str) {
        for c in s.chars() {
            key(app, KeyCode::Char(c));
        }
    }

    fn app() -> App {
        let dir = std::env::temp_dir().join(format!("sshm-test-{}", std::process::id()));
        App::new(Store::default(), dir.join("hosts.toml"))
    }

    #[test]
    fn add_filter_edit_delete() {
        let mut a = app();
        key(&mut a, KeyCode::Char('a'));
        key(&mut a, KeyCode::Enter); // empty name
        // Enter moves to the next field; Ctrl+S with an empty name → error
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        match &a.mode {
            Mode::Form(f) => assert!(f.error.is_some()),
            _ => panic!("form kapanmamalıydı"),
        }
        key(&mut a, KeyCode::BackTab);
        typ(&mut a, "web");
        key(&mut a, KeyCode::Tab);
        typ(&mut a, "10.0.0.5");
        key(&mut a, KeyCode::Tab);
        typ(&mut a, "root");
        key(&mut a, KeyCode::Tab);
        typ(&mut a, "2222");
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(a.mode, Mode::Normal));
        assert_eq!(a.store.hosts[0].target(), "root@10.0.0.5");
        assert_eq!(a.store.hosts[0].port, 2222);

        // second entry + filter
        key(&mut a, KeyCode::Char('y'));
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(a.store.hosts[1].name, format!("web-{}", s().copy_suffix));
        key(&mut a, KeyCode::Char('/'));
        typ(&mut a, s().copy_suffix);
        assert_eq!(a.visible().len(), 1);
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.visible().len(), 2);

        // edit: renaming to an existing name is rejected
        a.selected = a.visible().iter().position(|&i| a.store.hosts[i].name != "web").unwrap();
        key(&mut a, KeyCode::Char('e'));
        a.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        typ(&mut a, "WEB");
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(&a.mode, Mode::Form(f) if f.error.is_some()));
        key(&mut a, KeyCode::Esc);

        // delete
        key(&mut a, KeyCode::Char('d'));
        key(&mut a, KeyCode::Char('n'));
        assert_eq!(a.store.hosts.len(), 2);
        key(&mut a, KeyCode::Char('d'));
        key(&mut a, KeyCode::Char('y'));
        assert_eq!(a.store.hosts.len(), 1);
        let _ = std::fs::remove_dir_all(a.path.parent().unwrap());
    }

    #[test]
    fn utf8_editing() {
        let mut a = app();
        key(&mut a, KeyCode::Char('a'));
        typ(&mut a, "çğü");
        key(&mut a, KeyCode::Left);
        key(&mut a, KeyCode::Backspace);
        match &a.mode {
            Mode::Form(f) => assert_eq!(f.fields[F_NAME].value, "çü"),
            _ => panic!(),
        }
    }
}
