mod app;
mod import;
mod model;
mod probe;
mod ssh;
mod ui;

use anyhow::Result;
use app::{App, Pending};
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind};
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen, SetTitle};
use crossterm::execute;
use model::{config_path, Store};
use ratatui::DefaultTerminal;
use std::io::{stdout, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const HELP: &str = "sshm — SSH bağlantı yöneticisi

Kullanım:
  sshm              arayüzü aç
  sshm <ad>         doğrudan bağlan (tam ad, önek ya da tek eşleşen parça)
  sshm --list       bağlantıları listele
  sshm --import     .bat dosyalarından ve ~/.ssh/config'ten içe aktar
  sshm --config     kayıt dosyasının yolunu yaz
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = config_path();
    let mut first_run_msg = None;
    let mut store = match Store::load(&path)? {
        Some(s) => s,
        None => {
            // İlk açılış: mevcut .bat dosyalarını ve ssh config'i otomatik al.
            let mut s = Store::default();
            let added = import::merge(&mut s, import::discover());
            s.save(&path)?;
            if !added.is_empty() {
                first_run_msg = Some(format!(
                    "Hoş geldin. {} bağlantı içe aktarıldı: {}",
                    added.len(),
                    added.join(", ")
                ));
            }
            s
        }
    };

    match args.first().map(String::as_str) {
        None => run_tui(store, path, first_run_msg),
        Some("-h" | "--help" | "help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("-l" | "--list" | "list" | "ls") => {
            list(&store);
            Ok(())
        }
        Some("--import" | "import") => {
            let added = import::merge(&mut store, import::discover());
            store.save(&path)?;
            if added.is_empty() {
                println!("Yeni bağlantı bulunamadı.");
            } else {
                println!("{} bağlantı eklendi: {}", added.len(), added.join(", "));
            }
            Ok(())
        }
        Some("--config") => {
            println!("{}", path.display());
            Ok(())
        }
        Some(q) => connect_cli(store, &path, q),
    }
}

fn list(store: &Store) {
    let mut hosts: Vec<_> = store.hosts.iter().collect();
    hosts.sort_by_key(|h| (h.group.is_empty(), h.group.to_lowercase(), h.name.to_lowercase()));
    let w = hosts.iter().map(|h| h.name.chars().count()).max().unwrap_or(0);
    for h in hosts {
        let g = if h.group.is_empty() { String::new() } else { format!("  [{}]", h.group) };
        println!("{:<w$}  {}{}", h.name, h.target(), g);
    }
}

fn connect_cli(mut store: Store, path: &Path, q: &str) -> Result<()> {
    let Some(i) = store.find(q) else {
        eprintln!("'{q}' bulunamadı. Bağlantılar:");
        list(&store);
        std::process::exit(1);
    };
    let h = store.hosts[i].clone();
    let _ = execute!(stdout(), SetTitle(format!("{} · sshm", h.name)));
    println!("→ {} ({})", h.name, h.target());
    let t0 = Instant::now();
    let st = ssh::ssh_command(&h).status()?;
    if st.success() || t0.elapsed() > Duration::from_secs(5) {
        store.hosts[i].touch();
        store.save(path)?;
    }
    std::process::exit(st.code().unwrap_or(1));
}

fn run_tui(store: Store, path: PathBuf, first_msg: Option<String>) -> Result<()> {
    let mut terminal = ratatui::init();
    let _ = execute!(stdout(), EnableBracketedPaste, SetTitle("sshm"));
    let mut app = App::new(store, path);
    if let Some(m) = first_msg {
        app.info(m);
    }
    app.probe_all();

    let res = event_loop(&mut terminal, &mut app);

    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    res
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        app.tick();
        terminal.draw(|f| ui::draw(f, app))?;
        if app.quit {
            return Ok(());
        }
        if let Some(p) = app.pending.take() {
            run_external(terminal, app, p)?;
            continue;
        }
        if event::poll(Duration::from_millis(150))? {
            match event::read()? {
                // Windows bırakma olaylarını da gönderir.
                Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                Event::Paste(text) => app.on_paste(&text),
                _ => {}
            }
        }
    }
}

fn fmt_duration(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s} sn"),
        60..=3599 => format!("{} dk", s / 60),
        _ => format!("{} sa {} dk", s / 3600, (s % 3600) / 60),
    }
}

/// Arayüzü askıya alır, ssh/sftp'yi bu terminalde çalıştırır, sonra geri döner.
fn run_external(terminal: &mut DefaultTerminal, app: &mut App, p: Pending) -> Result<()> {
    let (i, sftp) = match p {
        Pending::Connect(i) => (i, false),
        Pending::Sftp(i) => (i, true),
    };
    let h = app.store.hosts[i].clone();
    let mut cmd = if sftp { ssh::sftp_command(&h) } else { ssh::ssh_command(&h) };

    let _ = execute!(stdout(), DisableBracketedPaste);
    ratatui::restore();
    let _ = execute!(stdout(), SetTitle(format!("{} · sshm", h.name)));
    println!("\n→ {} {} ({})\n", if sftp { "sftp" } else { "ssh" }, h.name, h.target());

    let t0 = Instant::now();
    let res = cmd.status();
    let took = t0.elapsed();

    let code = match &res {
        Ok(st) => st.code(),
        Err(e) => {
            println!("ssh başlatılamadı: {e}");
            None
        }
    };
    let quick_fail = code != Some(0) && took < Duration::from_secs(5);
    if quick_fail {
        // Hata çıktısı alternatif ekrana geçince kaybolmasın.
        print!("\nListeye dönmek için Enter'a bas…");
        let _ = stdout().flush();
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
    if !quick_fail {
        app.store.hosts[i].touch();
        app.save();
    }

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste, SetTitle("sshm"))?;
    terminal.clear()?;

    match code {
        _ if res.is_err() => app.error("ssh bulunamadı; Windows'ta OpenSSH istemcisi kurulu mu?"),
        Some(0) => app.info(format!("{} oturumu kapandı · {}", h.name, fmt_duration(took))),
        Some(255) if quick_fail => app.error(format!("{} sunucusuna bağlanılamadı (255)", h.name)),
        Some(c) if quick_fail => app.error(format!("ssh {c} koduyla çıktı")),
        Some(c) => app.info(format!("{} oturumu kapandı · {} · çıkış kodu {c}", h.name, fmt_duration(took))),
        None => app.error("ssh beklenmedik şekilde sonlandı"),
    }
    Ok(())
}
