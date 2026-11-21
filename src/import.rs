//! .bat dosyalarından ve ~/.ssh/config'ten bağlantı içe aktarma.

use crate::model::{Host, Store};
use std::fs;
use std::path::{Path, PathBuf};

/// Kabuk benzeri bölme: tırnak içindeki boşluklar korunur.
pub fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_tok = false;
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    in_tok = true;
                }
                c if c.is_whitespace() => {
                    if in_tok {
                        out.push(std::mem::take(&mut cur));
                        in_tok = false;
                    }
                }
                _ => {
                    cur.push(c);
                    in_tok = true;
                }
            },
        }
    }
    if in_tok {
        out.push(cur);
    }
    out
}

/// ssh'ın değer alan seçenekleri.
const VALUE_FLAGS: &str = "BbcDEeFIiJLlmOoPpQRSWw";

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// .bat içindeki göreli yolu, %~dp0 ve %USERPROFILE% dahil, mutlak yola çevirir.
fn resolve_path(val: &str, base: &Path) -> String {
    let mut v = val.to_string();
    let base_s = base.display().to_string();
    v = v.replace("%~dp0", &format!("{base_s}\\"));
    let home_s = home().display().to_string();
    for var in ["%USERPROFILE%", "%userprofile%", "%HOME%"] {
        v = v.replace(var, &home_s);
    }
    if let Some(rest) = v.strip_prefix("~/").or_else(|| v.strip_prefix("~\\")) {
        return home().join(rest).display().to_string();
    }
    let p = Path::new(&v);
    if p.is_absolute() {
        v
    } else {
        base.join(p).display().to_string()
    }
}

fn apply_option(h: &mut Host, val: &str, base: &Path, extra: &mut Vec<String>) {
    let (k, v) = val
        .split_once('=')
        .or_else(|| val.split_once(' '))
        .unwrap_or((val, ""));
    let v = v.trim();
    match k.trim().to_lowercase().as_str() {
        "user" => h.user = v.to_string(),
        "port" => h.port = v.parse().unwrap_or(22),
        "identityfile" => h.key = resolve_path(v, base),
        "proxyjump" => h.jump = v.to_string(),
        _ => {
            extra.push("-o".into());
            extra.push(val.to_string());
        }
    }
}

fn quote_arg(s: &str) -> String {
    if s.is_empty() || s.contains(char::is_whitespace) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

/// `ssh ...` komutunu çözümler. İlk kelime ssh değilse `None`.
pub fn parse_ssh_command(tokens: &[String], base: &Path) -> Option<Host> {
    let prog = Path::new(tokens.first()?)
        .file_stem()?
        .to_string_lossy()
        .to_lowercase();
    if prog != "ssh" {
        return None;
    }
    let mut h = Host::default();
    let mut dest: Option<String> = None;
    let mut extra: Vec<String> = Vec::new();
    let mut i = 1;
    while i < tokens.len() && dest.is_none() {
        let t = &tokens[i];
        if t.len() > 1 && t.starts_with('-') {
            let flag = t.chars().nth(1).unwrap();
            if VALUE_FLAGS.contains(flag) {
                let val = if t.len() > 2 {
                    t[2..].to_string()
                } else {
                    i += 1;
                    match tokens.get(i) {
                        Some(v) => v.clone(),
                        None => break,
                    }
                };
                match flag {
                    'i' => h.key = resolve_path(&val, base),
                    'p' => h.port = val.parse().unwrap_or(22),
                    'l' => h.user = val,
                    'J' => h.jump = val,
                    'L' | 'R' | 'D' => h.forwards.push(format!("{flag} {val}")),
                    'o' => apply_option(&mut h, &val, base, &mut extra),
                    _ => {
                        extra.push(format!("-{flag}"));
                        extra.push(val);
                    }
                }
            } else if t != "-t" && t != "-tt" {
                extra.push(t.clone());
            }
        } else {
            dest = Some(t.clone());
        }
        i += 1;
    }
    let dest = dest?;
    let dest = dest.strip_prefix("ssh://").unwrap_or(&dest);
    let rest = match dest.rsplit_once('@') {
        Some((u, r)) => {
            h.user = u.to_string();
            r
        }
        None => dest,
    };
    // ssh://host:port biçimi
    match rest.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && port.parse::<u16>().is_ok() => {
            h.host = host.to_string();
            h.port = port.parse().unwrap();
        }
        _ => h.host = rest.to_string(),
    }
    if h.host.is_empty() {
        return None;
    }
    h.extra = extra.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" ");
    Some(h)
}

fn name_from_stem(stem: &str) -> String {
    let lower = stem.to_lowercase();
    for prefix in ["bağlan", "baglan", "connect", "ssh"] {
        if let Some(r) = lower.strip_prefix(prefix) {
            let r = r.trim_matches(|c: char| c == '_' || c == '-' || c == ' ' || c == '.');
            if !r.is_empty() {
                return r.to_string();
            }
        }
    }
    lower
}

pub fn guess_group(h: &Host) -> String {
    let n = h.name.to_lowercase();
    let host = h.host.to_lowercase();
    let g = if host.contains("amazonaws.com") || n.starts_with("aws") || n.starts_with("ec2") {
        "AWS"
    } else if host.contains("oraclecloud") || n.starts_with("oracle") || n.starts_with("oci") {
        "Oracle"
    } else if host.contains("hetzner") || n.starts_with("hetzner") {
        "Hetzner"
    } else if host.contains("azure") || n.starts_with("azure") {
        "Azure"
    } else if host.contains("googleusercontent") || n.starts_with("gcp") {
        "GCP"
    } else {
        ""
    };
    g.to_string()
}

pub fn parse_bat(path: &Path) -> Option<Host> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let base = path.parent()?;
    let stem = path.file_stem()?.to_string_lossy().to_string();
    for line in text.lines() {
        let l = line.trim().trim_start_matches('@').trim();
        let low = l.to_lowercase();
        if l.is_empty() || low.starts_with("rem ") || low.starts_with("::") {
            continue;
        }
        if let Some(mut h) = parse_ssh_command(&tokenize(l), base) {
            h.name = name_from_stem(&stem);
            h.group = guess_group(&h);
            h.notes = format!("{} dosyasından alındı", path.file_name()?.to_string_lossy());
            return Some(h);
        }
    }
    None
}

pub fn parse_ssh_config(text: &str) -> Vec<Host> {
    let mut out = Vec::new();
    let mut cur: Vec<Host> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let (k, v) = match l.find(|c: char| c.is_whitespace() || c == '=') {
            Some(i) => (&l[..i], l[i + 1..].trim_start_matches(['=', ' ', '\t']).trim()),
            None => continue,
        };
        let v = v.trim_matches('"');
        match k.to_lowercase().as_str() {
            "host" => {
                out.append(&mut cur);
                for alias in v.split_whitespace() {
                    if alias.contains(['*', '?', '!']) {
                        continue;
                    }
                    cur.push(Host {
                        name: alias.to_string(),
                        host: alias.to_string(),
                        notes: "~/.ssh/config dosyasından alındı".into(),
                        ..Host::default()
                    });
                }
            }
            "match" => out.append(&mut cur),
            "hostname" => cur.iter_mut().for_each(|h| h.host = v.to_string()),
            "user" => cur.iter_mut().for_each(|h| h.user = v.to_string()),
            "port" => cur.iter_mut().for_each(|h| h.port = v.parse().unwrap_or(22)),
            "identityfile" => cur.iter_mut().filter(|h| h.key.is_empty()).for_each(|h| {
                h.key = resolve_path(v, &home().join(".ssh"));
            }),
            "proxyjump" => cur.iter_mut().for_each(|h| h.jump = v.to_string()),
            "localforward" | "remoteforward" | "dynamicforward" => {
                let flag = &k.to_lowercase()[..1].to_uppercase();
                let spec = v.split_whitespace().collect::<Vec<_>>().join(":");
                cur.iter_mut().for_each(|h| h.forwards.push(format!("{flag} {spec}")));
            }
            _ => {}
        }
    }
    out.append(&mut cur);
    for h in &mut out {
        h.group = guess_group(h);
    }
    out
}

/// Belgeler ve Masaüstü'ndeki .bat/.cmd dosyalarıyla ~/.ssh/config'i tarar.
pub fn discover() -> Vec<Host> {
    let mut found = Vec::new();
    let dirs_to_scan: Vec<PathBuf> = [dirs::document_dir(), dirs::desktop_dir()]
        .into_iter()
        .flatten()
        .collect();
    for dir in dirs_to_scan {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
            if matches!(ext.as_deref(), Some("bat") | Some("cmd")) {
                if let Some(h) = parse_bat(&p) {
                    found.push(h);
                }
            }
        }
    }
    if let Ok(text) = fs::read_to_string(home().join(".ssh").join("config")) {
        found.extend(parse_ssh_config(&text));
    }
    found
}

/// Yeni uç noktaları ekler, eklenenlerin adlarını döner.
pub fn merge(store: &mut Store, found: Vec<Host>) -> Vec<String> {
    let mut added = Vec::new();
    for mut h in found {
        if store.hosts.iter().any(|e| e.same_endpoint(&h)) {
            continue;
        }
        h.name = store.unique_name(&h.name);
        added.push(h.name.clone());
        store.hosts.push(h);
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bat_line() {
        let base = Path::new(r"C:\Users\x\Documents");
        let h = parse_ssh_command(&tokenize(r#"ssh -i "keyim.pem" ubuntu@1.2.3.4"#), base).unwrap();
        assert_eq!(h.user, "ubuntu");
        assert_eq!(h.host, "1.2.3.4");
        assert_eq!(h.port, 22);
        assert_eq!(h.key, base.join("keyim.pem").display().to_string());
        assert!(h.extra.is_empty());
    }

    #[test]
    fn parses_flags() {
        let base = Path::new("/tmp");
        let h = parse_ssh_command(
            &tokenize("ssh.exe -p 2222 -J me@bastion -L 8080:localhost:80 -o User=root -A srv uptime"),
            base,
        )
        .unwrap();
        assert_eq!((h.port, h.jump.as_str(), h.user.as_str()), (2222, "me@bastion", "root"));
        assert_eq!(h.host, "srv");
        assert_eq!(h.forwards, vec!["L 8080:localhost:80"]);
        assert_eq!(h.extra, "-A");
    }

    #[test]
    fn rejects_non_ssh() {
        assert!(parse_ssh_command(&tokenize("echo hi"), Path::new(".")).is_none());
    }

    #[test]
    fn names() {
        assert_eq!(name_from_stem("baglanoracle2no2fa"), "oracle2no2fa");
        assert_eq!(name_from_stem("ssh-web"), "web");
        assert_eq!(name_from_stem("sunucu"), "sunucu");
    }

    #[test]
    fn ssh_config() {
        let hs = parse_ssh_config(
            "Host web web2\n  HostName 10.0.0.5\n  User deploy\n  Port 2200\nHost *\n  ServerAliveInterval 30\n",
        );
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[1].name, "web2");
        assert_eq!((hs[0].host.as_str(), hs[0].port), ("10.0.0.5", 2200));
    }
}
