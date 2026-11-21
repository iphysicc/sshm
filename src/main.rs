mod app;
mod i18n;
mod import;
mod model;
mod probe;
mod ssh;
mod ui;

use anyhow::Result;
use app::{App, Pending};
use crossterm::event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen, SetTitle};
use i18n::{s, Lang};
use model::{config_path, Store};
use ratatui::DefaultTerminal;
use std::io::{stdout, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// SSHM_LANG (for this run) > saved setting > English.
fn apply_lang(store: Option<&Store>) {
    let env = std::env::var("SSHM_LANG").ok().and_then(|v| Lang::parse(&v));
    let saved = store.and_then(|st| st.lang.as_deref()).and_then(Lang::parse);
    i18n::set(env.or(saved).unwrap_or(Lang::En));
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = config_path();
    apply_lang(None);
    let mut first_run_names = None;
    let mut store = match Store::load(&path)? {
        Some(st) => {
            apply_lang(Some(&st));
            st
        }
        None => {
            // First run: pick up existing .bat files and ~/.ssh/config automatically.
            let mut st = Store::default();
            let added = import::merge(&mut st, import::discover());
            st.save(&path)?;
            first_run_names = Some(added);
            st
        }
    };

    match args.first().map(String::as_str) {
        None => run_tui(store, path, first_run_names),
        Some("-h" | "--help" | "help") => {
            print!("{}", s().cli_help);
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
                println!("{}", s().cli_import_none);
            } else {
                println!("{}", tf!(s().cli_imported, added.len(), added.join(", ")));
            }
            Ok(())
        }
        Some("--lang" | "lang") => {
            let arg = args.get(1).map(String::as_str).unwrap_or("");
            let Some(l) = Lang::parse(arg) else {
                eprintln!("{}", tf!(s().lang_invalid, arg));
                std::process::exit(2);
            };
            i18n::set(l);
            store.lang = Some(l.code().to_string());
            store.save(&path)?;
            println!("{}", s().lang_set);
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
        eprintln!("{}", tf!(s().not_found, q));
        list(&store);
        std::process::exit(1);
    };
    let h = store.hosts[i].clone();
    let _ = execute!(stdout(), SetTitle(format!("{} · sshm", h.name)));
    println!("→ {} ({})", h.name, h.target());
    let t0 = Instant::now();
    let status = ssh::ssh_command(&h).status()?;
    if status.success() || t0.elapsed() > Duration::from_secs(5) {
        store.hosts[i].touch();
        store.save(path)?;
    }
    std::process::exit(status.code().unwrap_or(1));
}

fn run_tui(store: Store, path: PathBuf, first_run: Option<Vec<String>>) -> Result<()> {
    let mut terminal = ratatui::init();
    let _ = execute!(stdout(), EnableBracketedPaste, SetTitle("sshm"));
    // Ask for a language until one is saved, unless SSHM_LANG decides it.
    let ask_lang = store.lang.is_none() && std::env::var_os("SSHM_LANG").is_none();
    let mut app = App::new(store, path);
    app.welcome = first_run;
    if ask_lang {
        app.mode = app::Mode::Lang(i18n::lang());
    } else {
        app.show_welcome();
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
                // Windows also reports key releases.
                Event::Key(k) if k.kind != KeyEventKind::Release => app.on_key(k),
                Event::Paste(text) => app.on_paste(&text),
                _ => {}
            }
        }
    }
}

/// Suspends the UI, runs ssh/sftp in this terminal, then comes back.
fn run_external(terminal: &mut DefaultTerminal, app: &mut App, p: Pending) -> Result<()> {
    let t = s();
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
        Ok(status) => status.code(),
        Err(e) => {
            println!("{}", tf!(t.spawn_failed, e));
            None
        }
    };
    let quick_fail = code != Some(0) && took < Duration::from_secs(5);
    if quick_fail {
        // Keep the error output readable before switching back to the alternate screen.
        print!("\n{}", t.press_enter);
        let _ = stdout().flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    } else {
        app.store.hosts[i].touch();
        app.save();
    }

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableBracketedPaste, SetTitle("sshm"))?;
    terminal.clear()?;

    let dur = i18n::duration(took);
    match code {
        _ if res.is_err() => app.error(t.ssh_missing),
        Some(0) => app.info(tf!(t.session_closed, h.name, dur)),
        Some(255) if quick_fail => app.error(tf!(t.connect_failed, h.name)),
        Some(c) if quick_fail => app.error(tf!(t.ssh_exit, c)),
        Some(c) => app.info(tf!(t.session_closed_code, h.name, dur, c)),
        None => app.error(t.ssh_killed),
    }
    Ok(())
}
