use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Tek bir SSH bağlantısı. Boş string = ayar yok.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Host {
    pub name: String,
    pub host: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user: String,
    pub port: u16,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// ProxyJump (-J)
    #[serde(skip_serializing_if = "String::is_empty")]
    pub jump: String,
    /// Doluysa bağlanınca `tmux new -A -s <ad>` ile kalıcı oturuma girilir.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tmux: String,
    /// "L 8080:localhost:80", "R 9000:localhost:9000", "D 1080"
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forwards: Vec<String>,
    /// Ek ssh argümanları, olduğu gibi eklenir.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub extra: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<u64>,
    #[serde(skip_serializing_if = "is_zero")]
    pub uses: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Default for Host {
    fn default() -> Self {
        Host {
            name: String::new(),
            host: String::new(),
            user: String::new(),
            port: 22,
            key: String::new(),
            group: String::new(),
            jump: String::new(),
            tmux: String::new(),
            forwards: Vec::new(),
            extra: String::new(),
            notes: String::new(),
            last_used: None,
            uses: 0,
        }
    }
}

impl Host {
    pub fn target(&self) -> String {
        if self.user.is_empty() {
            self.host.clone()
        } else {
            format!("{}@{}", self.user, self.host)
        }
    }

    /// Durum tablosunun anahtarı; aynı uç noktayı paylaşan kayıtlar tek kez denetlenir.
    pub fn probe_key(&self) -> String {
        format!("{}:{}", self.host.to_lowercase(), self.port)
    }

    pub fn same_endpoint(&self, other: &Host) -> bool {
        self.host.eq_ignore_ascii_case(&other.host)
            && self.user == other.user
            && self.port == other.port
    }

    pub fn touch(&mut self) {
        self.last_used = Some(now());
        self.uses += 1;
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default, rename = "host")]
    pub hosts: Vec<Host>,
}

pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("SSHM_CONFIG") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sshm")
        .join("hosts.toml")
}

const HEADER: &str = "# sshm bağlantıları. Elle de düzenlenebilir; sshm açıkken değil.\n\n";

impl Store {
    /// Dosya yoksa `None`.
    pub fn load(path: &Path) -> Result<Option<Store>> {
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("{} okunamadı", path.display()))?;
        let store: Store = toml::from_str(&text)
            .with_context(|| format!("{} bozuk", path.display()))?;
        Ok(Some(store))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = toml::to_string_pretty(self)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, format!("{HEADER}{body}"))?;
        fs::rename(&tmp, path).with_context(|| format!("{} yazılamadı", path.display()))?;
        Ok(())
    }

    /// Tam ad, sonra tek eşleşen önek, sonra tek eşleşen parça.
    pub fn find(&self, query: &str) -> Option<usize> {
        let q = query.to_lowercase();
        if let Some(i) = self.hosts.iter().position(|h| h.name.to_lowercase() == q) {
            return Some(i);
        }
        let unique = |pred: &dyn Fn(&Host) -> bool| {
            let m: Vec<usize> = (0..self.hosts.len()).filter(|&i| pred(&self.hosts[i])).collect();
            (m.len() == 1).then(|| m[0])
        };
        unique(&|h: &Host| h.name.to_lowercase().starts_with(&q))
            .or_else(|| unique(&|h: &Host| h.name.to_lowercase().contains(&q)))
    }

    pub fn name_taken(&self, name: &str, except: Option<usize>) -> bool {
        self.hosts
            .iter()
            .enumerate()
            .any(|(i, h)| Some(i) != except && h.name.eq_ignore_ascii_case(name))
    }

    pub fn unique_name(&self, base: &str) -> String {
        if !self.name_taken(base, None) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|n| !self.name_taken(n, None))
            .unwrap()
    }
}
