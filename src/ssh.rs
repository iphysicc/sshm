//! ssh/sftp komutlarını kurar ve dış süreçleri başlatır.

use crate::import::tokenize;
use crate::model::Host;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Windows'ta sistemin OpenSSH'ı (PATH'te Git'in ssh'ı önde olsa bile).
fn system_bin(name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let p = PathBuf::from(root).join("System32").join("OpenSSH").join(format!("{name}.exe"));
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(name)
}

pub fn ssh_bin() -> PathBuf {
    system_bin("ssh")
}

pub fn sftp_bin() -> PathBuf {
    system_bin("sftp")
}

pub fn expand_tilde(s: &str) -> String {
    let s = s.trim().trim_matches('"');
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest).display().to_string();
        }
    }
    s.to_string()
}

/// "L 8080:localhost:80" / "-L 8080:..." / "8080:localhost:80" → ("-L", "8080:localhost:80")
pub fn forward_arg(f: &str) -> Option<(&'static str, String)> {
    let f = f.trim();
    if f.is_empty() {
        return None;
    }
    for (p, flag) in [("-L", "-L"), ("-R", "-R"), ("-D", "-D")] {
        if let Some(r) = f.strip_prefix(p) {
            return Some((flag, r.trim().to_string()));
        }
    }
    if let Some((k, r)) = f.split_once(char::is_whitespace) {
        let flag = match k.to_ascii_uppercase().as_str() {
            "L" => Some("-L"),
            "R" => Some("-R"),
            "D" => Some("-D"),
            _ => None,
        };
        if let Some(flag) = flag {
            return Some((flag, r.trim().to_string()));
        }
    }
    Some(("-L", f.to_string()))
}

const KEEPALIVE: [&str; 4] = ["-o", "ServerAliveInterval=30", "-o", "ServerAliveCountMax=3"];

fn base_args(h: &Host, keepalive: bool) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    if !h.key.is_empty() {
        a.push("-i".into());
        a.push(expand_tilde(&h.key));
    }
    if h.port != 22 {
        a.push("-p".into());
        a.push(h.port.to_string());
    }
    if !h.jump.is_empty() {
        a.push("-J".into());
        a.push(h.jump.clone());
    }
    for f in &h.forwards {
        if let Some((flag, spec)) = forward_arg(f) {
            a.push(flag.into());
            a.push(spec);
        }
    }
    // Kullanıcının -o ayarları önce gelir; ssh ilk değeri kullanır.
    a.extend(tokenize(&h.extra));
    if keepalive {
        a.extend(KEEPALIVE.iter().map(|s| s.to_string()));
    }
    a
}

pub fn ssh_args(h: &Host) -> Vec<String> {
    let mut a = base_args(h, true);
    if !h.tmux.is_empty() {
        a.push("-t".into());
    }
    a.push(h.target());
    if !h.tmux.is_empty() {
        a.push(format!("tmux new -A -s {}", h.tmux));
    }
    a
}

pub fn sftp_args(h: &Host) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    if !h.key.is_empty() {
        a.push("-i".into());
        a.push(expand_tilde(&h.key));
    }
    if h.port != 22 {
        a.push("-P".into());
        a.push(h.port.to_string());
    }
    if !h.jump.is_empty() {
        a.push("-J".into());
        a.push(h.jump.clone());
    }
    a.extend(KEEPALIVE.iter().map(|s| s.to_string()));
    a.push(h.target());
    a
}

fn quote(s: &str) -> String {
    if s.is_empty() || s.contains(char::is_whitespace) || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// Ekranda gösterilen / kopyalanan komut. `full` keep-alive seçeneklerini de ekler.
pub fn command_line(h: &Host, full: bool) -> String {
    let mut a = base_args(h, full);
    if !h.tmux.is_empty() {
        a.push("-t".into());
    }
    a.push(h.target());
    if !h.tmux.is_empty() {
        a.push(format!("tmux new -A -s {}", h.tmux));
    }
    let mut out = String::from("ssh");
    for arg in a {
        out.push(' ');
        out.push_str(&quote(&arg));
    }
    out
}

/// Windows Terminal'de yeni sekme açar; sekmede `sshm <ad>` çalışır.
pub fn open_tab(h: &Host) -> Result<()> {
    let exe = std::env::current_exe().context("sshm yolu bulunamadı")?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let wt = Command::new("wt.exe")
            .args(["-w", "0", "new-tab", "--title", &h.name, "--"])
            .arg(&exe)
            .arg(&h.name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if wt.is_ok() {
            return Ok(());
        }
        // Windows Terminal yoksa yeni konsol penceresi.
        Command::new("cmd")
            .args(["/c", "start"])
            .raw_arg(format!("\"{}\"", h.name))
            .arg(&exe)
            .arg(&h.name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("yeni pencere açılamadı")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (exe, Stdio::null());
        anyhow::bail!("yeni sekme yalnız Windows Terminal'de destekleniyor")
    }
}

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut cb = arboard::Clipboard::new().context("pano açılamadı")?;
    cb.set_text(text.to_string()).context("panoya yazılamadı")?;
    Ok(())
}

pub fn ssh_command(h: &Host) -> Command {
    let mut c = Command::new(ssh_bin());
    c.args(ssh_args(h));
    c
}

pub fn sftp_command(h: &Host) -> Command {
    let mut c = Command::new(sftp_bin());
    c.args(sftp_args(h));
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_args() {
        let h = Host {
            name: "x".into(),
            host: "1.2.3.4".into(),
            user: "ubuntu".into(),
            port: 2222,
            key: r"C:\k\a b.pem".into(),
            tmux: "main".into(),
            forwards: vec!["D 1080".into(), "8080:localhost:80".into()],
            ..Host::default()
        };
        let a = ssh_args(&h);
        assert_eq!(&a[..4], &["-i", r"C:\k\a b.pem", "-p", "2222"]);
        assert!(a.windows(2).any(|w| w == ["-D", "1080"]));
        assert!(a.windows(2).any(|w| w == ["-L", "8080:localhost:80"]));
        assert_eq!(a[a.len() - 2], "ubuntu@1.2.3.4");
        assert_eq!(a[a.len() - 1], "tmux new -A -s main");
        assert_eq!(
            command_line(&h, false),
            r#"ssh -i "C:\k\a b.pem" -p 2222 -D 1080 -L 8080:localhost:80 -t ubuntu@1.2.3.4 "tmux new -A -s main""#
        );
    }
}
