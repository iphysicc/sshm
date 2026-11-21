use crate::import;
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
            self.error(format!("Kaydedilemedi: {e:#}"));
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

    /// Filtrelenmiş ve gruplara göre sıralanmış kayıt indeksleri.
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
                // key_normal modu değiştirmiş olabilir
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
                            self.info(format!("{} yeni sekmede açıldı", h.name));
                        }
                        Err(e) => self.error(format!("{e:#}")),
                    }
                }
            }
            KeyCode::Char('c') => {
                if let Some(i) = self.current() {
                    let cmd = ssh::command_line(&self.store.hosts[i], true);
                    match ssh::copy_to_clipboard(&cmd) {
                        Ok(()) => self.info("Komut panoya kopyalandı"),
                        Err(e) => self.error(format!("{e:#}")),
                    }
                }
            }
            KeyCode::Char('/') => self.mode = Mode::Filter,
            KeyCode::Char('a') => {
                self.mode = Mode::Form(Form::new("Yeni bağlantı", None, &Host::default()));
            }
            KeyCode::Char('e') => {
                if let Some(i) = self.current() {
                    let h = self.store.hosts[i].clone();
                    self.mode = Mode::Form(Form::new(&format!("{} düzenleniyor", h.name), Some(i), &h));
                }
            }
            KeyCode::Char('y') => {
                if let Some(i) = self.current() {
                    let mut h = self.store.hosts[i].clone();
                    h.name = self.store.unique_name(&format!("{}-kopya", h.name));
                    h.last_used = None;
                    h.uses = 0;
                    self.mode = Mode::Form(Form::new("Kopyadan yeni bağlantı", None, &h));
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
                self.info("Sunucular yeniden denetleniyor");
            }
            KeyCode::Char('i') => self.import(),
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
    }

    pub fn import(&mut self) {
        let found = import::discover();
        let before = self.store.hosts.len();
        let added = import::merge(&mut self.store, found);
        if added.is_empty() {
            self.info("Yeni bağlantı bulunamadı (Belgeler, Masaüstü, ~/.ssh/config)");
            return;
        }
        self.save();
        for i in before..self.store.hosts.len() {
            self.probe_host(i);
        }
        self.info(format!("{} bağlantı içe aktarıldı: {}", added.len(), added.join(", ")));
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
            self.info(format!("{} silindi", h.name));
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
        self.info(format!("{name} kaydedildi"));
        Mode::Normal
    }
}

pub struct Field {
    pub label: &'static str,
    pub hint: &'static str,
    pub value: String,
    /// karakter cinsinden
    pub cursor: usize,
}

impl Field {
    fn new(label: &'static str, hint: &'static str, value: String) -> Field {
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
        let fields = vec![
            Field::new("Ad", "listede görünen kısa ad", h.name.clone()),
            Field::new("Sunucu", "IP ya da alan adı", h.host.clone()),
            Field::new("Kullanıcı", "ubuntu, root…", h.user.clone()),
            Field::new("Port", "22", if h.port == 22 { String::new() } else { h.port.to_string() }),
            Field::new("Anahtar", r"C:\yol\anahtar.pem (boşsa ssh varsayılanı)", h.key.clone()),
            Field::new("Grup", "Oracle, AWS, Ev…", h.group.clone()),
            Field::new("Atlama", "kullanıcı@bastion (ProxyJump)", h.jump.clone()),
            Field::new("tmux", "oturum adı, ör. main (sunucuda tmux kurulu olmalı)", h.tmux.clone()),
            Field::new("Yönlendirme", "L 8080:localhost:80, D 1080", h.forwards.join(", ")),
            Field::new("Ek argüman", "-A -o Compression=yes", h.extra.clone()),
            Field::new("Not", "", h.notes.clone()),
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
        let name = self.v(F_NAME);
        if name.is_empty() {
            return Err("Ad boş olamaz".into());
        }
        if name.starts_with('-') || name.contains([';', '"', '\'']) {
            return Err("Ad '-' ile başlayamaz, ; \" ' içeremez".into());
        }
        if store.name_taken(&name, self.editing) {
            return Err(format!("'{name}' adı zaten var"));
        }
        let host = self.v(F_HOST);
        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err("Sunucu boş olamaz, boşluk içeremez".into());
        }
        let user = self.v(F_USER);
        if user.contains(char::is_whitespace) {
            return Err("Kullanıcı adı boşluk içeremez".into());
        }
        let port_s = self.v(F_PORT);
        let port: u16 = if port_s.is_empty() {
            22
        } else {
            match port_s.parse::<u16>() {
                Ok(p) if p > 0 => p,
                _ => return Err("Port 1-65535 arası bir sayı olmalı".into()),
            }
        };
        let key = self.v(F_KEY).trim_matches('"').to_string();
        if !key.is_empty() && !Path::new(&ssh::expand_tilde(&key)).exists() {
            return Err(format!("Anahtar dosyası bulunamadı: {key}"));
        }
        let tmux = self.v(F_TMUX);
        if !tmux.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
            return Err("tmux oturum adı yalnız harf, rakam, - _ . içerebilir".into());
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
        key(&mut a, KeyCode::Enter); // boş ad
        // Enter ilk alanda sonraki alana geçer; Ctrl+S ile kaydet → hata
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

        // ikinci kayıt + filtre
        key(&mut a, KeyCode::Char('y'));
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(a.store.hosts[1].name, "web-kopya");
        key(&mut a, KeyCode::Char('/'));
        typ(&mut a, "kopya");
        assert_eq!(a.visible().len(), 1);
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.visible().len(), 2);

        // düzenle: aynı ada çevirmek reddedilir
        a.selected = a.visible().iter().position(|&i| a.store.hosts[i].name == "web-kopya").unwrap();
        key(&mut a, KeyCode::Char('e'));
        a.on_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        typ(&mut a, "WEB");
        a.on_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(matches!(&a.mode, Mode::Form(f) if f.error.is_some()));
        key(&mut a, KeyCode::Esc);

        // sil
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
