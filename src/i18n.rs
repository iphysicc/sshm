//! English / Turkish UI strings. English is the default.

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Tr,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        match s.trim().to_lowercase().as_str() {
            "en" | "eng" | "english" => Some(Lang::En),
            "tr" | "tur" | "turkish" | "türkçe" | "turkce" => Some(Lang::Tr),
            _ => None,
        }
    }

    pub fn toggle(self) -> Lang {
        match self {
            Lang::En => Lang::Tr,
            Lang::Tr => Lang::En,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set(l: Lang) {
    LANG.store(if l == Lang::Tr { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 { Lang::Tr } else { Lang::En }
}

pub fn s() -> &'static Strings {
    match lang() {
        Lang::En => &EN,
        Lang::Tr => &TR,
    }
}

/// Replaces each `{}` in `t` with the next argument.
pub fn fill(t: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(t.len() + 16);
    let mut it = args.iter();
    let mut rest = t;
    while let Some(p) = rest.find("{}") {
        out.push_str(&rest[..p]);
        if let Some(a) = it.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[p + 2..];
    }
    out.push_str(rest);
    out
}

/// `tf!(s().saved, name)` → filled string.
#[macro_export]
macro_rules! tf {
    ($t:expr $(, $a:expr)* $(,)?) => {
        $crate::i18n::fill($t, &[$(&$a as &dyn std::fmt::Display),*])
    };
}

pub fn ago(secs: u64) -> String {
    let (n, unit) = match secs {
        0..=59 => return if lang() == Lang::Tr { "az önce".into() } else { "just now".into() },
        60..=3599 => (secs / 60, 0),
        3600..=86399 => (secs / 3600, 1),
        _ => (secs / 86400, 2),
    };
    match lang() {
        Lang::Tr => format!("{n} {} önce", ["dk", "saat", "gün"][unit]),
        Lang::En => {
            let u = ["min", "hour", "day"][unit];
            let plural = if n == 1 || unit == 0 { "" } else { "s" };
            format!("{n} {u}{plural} ago")
        }
    }
}

pub fn duration(d: Duration) -> String {
    let s = d.as_secs();
    let (sec, min, hr) = if lang() == Lang::Tr { ("sn", "dk", "sa") } else { ("s", "min", "h") };
    match s {
        0..=59 => format!("{s} {sec}"),
        60..=3599 => format!("{} {min}", s / 60),
        _ => format!("{} {hr} {} {min}", s / 3600, (s % 3600) / 60),
    }
}

pub fn times(n: u32) -> String {
    match lang() {
        Lang::Tr => format!("{n} kez"),
        Lang::En if n == 1 => "1 time".into(),
        Lang::En => format!("{n} times"),
    }
}

pub struct Strings {
    // storage
    pub read_failed: &'static str,
    pub corrupt: &'static str,
    pub write_failed: &'static str,
    pub file_header: &'static str,
    // import
    pub imported_from: &'static str,
    // ssh
    pub exe_not_found: &'static str,
    pub window_failed: &'static str,
    #[cfg_attr(windows, allow(dead_code))]
    pub tab_unsupported: &'static str,
    pub clipboard_open: &'static str,
    pub clipboard_write: &'static str,
    // probe
    pub dns_failed: &'static str,
    pub timed_out: &'static str,
    pub refused: &'static str,
    pub unreachable: &'static str,
    // app messages
    pub save_failed: &'static str,
    pub opened_tab: &'static str,
    pub copied: &'static str,
    pub form_new: &'static str,
    pub form_edit: &'static str,
    pub form_copy: &'static str,
    pub copy_suffix: &'static str,
    pub reprobing: &'static str,
    pub import_none: &'static str,
    pub imported_n: &'static str,
    pub deleted: &'static str,
    pub saved: &'static str,
    pub lang_switched: &'static str,
    // form
    pub f_name: (&'static str, &'static str),
    pub f_host: (&'static str, &'static str),
    pub f_user: (&'static str, &'static str),
    pub f_port: (&'static str, &'static str),
    pub f_key: (&'static str, &'static str),
    pub f_group: (&'static str, &'static str),
    pub f_jump: (&'static str, &'static str),
    pub f_tmux: (&'static str, &'static str),
    pub f_fwd: (&'static str, &'static str),
    pub f_extra: (&'static str, &'static str),
    pub f_notes: (&'static str, &'static str),
    // validation
    pub e_name_empty: &'static str,
    pub e_name_chars: &'static str,
    pub e_name_taken: &'static str,
    pub e_host: &'static str,
    pub e_user: &'static str,
    pub e_port: &'static str,
    pub e_key: &'static str,
    pub e_tmux: &'static str,
    // ui
    pub subtitle: &'static str,
    pub matches: &'static str,
    pub servers: &'static str,
    pub up_n: &'static str,
    pub empty_title: &'static str,
    pub empty_add: &'static str,
    pub empty_import: &'static str,
    pub no_match: &'static str,
    pub no_match_hint: &'static str,
    pub other_group: &'static str,
    pub cell_down: &'static str,
    pub cell_jump: &'static str,
    pub st_unchecked: &'static str,
    pub st_checking: &'static str,
    pub st_up: &'static str,
    pub st_down: &'static str,
    pub st_skipped: &'static str,
    pub r_status: &'static str,
    pub r_version: &'static str,
    pub r_port: &'static str,
    pub r_key: &'static str,
    pub r_jump: &'static str,
    pub r_tmux: &'static str,
    pub r_fwd: &'static str,
    pub r_extra: &'static str,
    pub r_last: &'static str,
    pub persistent: &'static str,
    pub never_used: &'static str,
    pub k_connect: &'static str,
    pub k_tab: &'static str,
    pub k_sftp: &'static str,
    pub k_search: &'static str,
    pub k_add: &'static str,
    pub k_edit: &'static str,
    pub k_copy: &'static str,
    pub k_help: &'static str,
    pub k_quit: &'static str,
    pub k_ok: &'static str,
    pub k_clear: &'static str,
    pub k_next_field: &'static str,
    pub k_save: &'static str,
    pub k_cancel: &'static str,
    pub k_delete: &'static str,
    pub k_any: &'static str,
    pub confirm_pre: &'static str,
    pub confirm_post: &'static str,
    pub help_title: &'static str,
    pub help: &'static [(&'static str, &'static str)],
    pub help_cli: &'static str,
    pub help_store: &'static str,
    // cli / main
    pub cli_help: &'static str,
    pub not_found: &'static str,
    pub cli_import_none: &'static str,
    pub cli_imported: &'static str,
    pub welcome: &'static str,
    pub spawn_failed: &'static str,
    pub press_enter: &'static str,
    pub ssh_missing: &'static str,
    pub session_closed: &'static str,
    pub session_closed_code: &'static str,
    pub connect_failed: &'static str,
    pub ssh_exit: &'static str,
    pub ssh_killed: &'static str,
    pub lang_set: &'static str,
    pub lang_invalid: &'static str,
}

pub static EN: Strings = Strings {
    read_failed: "could not read {}",
    corrupt: "{} is not valid",
    write_failed: "could not write {}",
    file_header: "# sshm connections. Safe to edit by hand while sshm is not running.",
    imported_from: "Imported from {}",
    exe_not_found: "could not locate the sshm executable",
    window_failed: "could not open a new window",
    tab_unsupported: "new tabs are only supported in Windows Terminal",
    clipboard_open: "could not open the clipboard",
    clipboard_write: "could not write to the clipboard",
    dns_failed: "name not resolved",
    timed_out: "timed out",
    refused: "connection refused",
    unreachable: "unreachable",
    save_failed: "Could not save: {}",
    opened_tab: "{} opened in a new tab",
    copied: "Command copied to clipboard",
    form_new: "New connection",
    form_edit: "Editing {}",
    form_copy: "New connection from copy",
    copy_suffix: "copy",
    reprobing: "Checking servers again",
    import_none: "No new connections found (Documents, Desktop, ~/.ssh/config)",
    imported_n: "Imported {} connections: {}",
    deleted: "{} deleted",
    saved: "{} saved",
    lang_switched: "Language: English",
    f_name: ("Name", "short name shown in the list"),
    f_host: ("Host", "IP address or domain"),
    f_user: ("User", "ubuntu, root…"),
    f_port: ("Port", "22"),
    f_key: ("Key", r"C:\path\key.pem (empty = ssh default)"),
    f_group: ("Group", "Oracle, AWS, Home…"),
    f_jump: ("Jump", "user@bastion (ProxyJump)"),
    f_tmux: ("tmux", "session name, e.g. main (tmux must be installed on the server)"),
    f_fwd: ("Forwards", "L 8080:localhost:80, D 1080"),
    f_extra: ("Extra args", "-A -o Compression=yes"),
    f_notes: ("Notes", ""),
    e_name_empty: "Name can't be empty",
    e_name_chars: "Name can't start with '-' or contain ; \" '",
    e_name_taken: "'{}' already exists",
    e_host: "Host can't be empty or contain spaces",
    e_user: "User can't contain spaces",
    e_port: "Port must be a number between 1 and 65535",
    e_key: "Key file not found: {}",
    e_tmux: "tmux session name may only contain letters, digits, - _ .",
    subtitle: "connections",
    matches: "{} matches",
    servers: "{} servers",
    up_n: "{} up",
    empty_title: "No connections yet.",
    empty_add: "add a connection",
    empty_import: "import .bat files from Documents / Desktop",
    no_match: "No matching connections.",
    no_match_hint: "Esc clears the filter",
    other_group: "Other",
    cell_down: "down",
    cell_jump: "jump",
    st_unchecked: "not checked",
    st_checking: "checking…",
    st_up: "up",
    st_down: "down",
    st_skipped: "via jump host, not checked directly",
    r_status: "Status",
    r_version: "Version",
    r_port: "Port",
    r_key: "Key",
    r_jump: "Jump",
    r_tmux: "tmux",
    r_fwd: "Forwards",
    r_extra: "Extra args",
    r_last: "Last used",
    persistent: "{} (persistent session)",
    never_used: "never connected",
    k_connect: "connect",
    k_tab: "tab",
    k_sftp: "sftp",
    k_search: "search",
    k_add: "add",
    k_edit: "edit",
    k_copy: "copy",
    k_help: "help",
    k_quit: "quit",
    k_ok: "done",
    k_clear: "clear",
    k_next_field: "next field",
    k_save: "save",
    k_cancel: "cancel",
    k_delete: "delete",
    k_any: "any other key",
    confirm_pre: "Delete ",
    confirm_post: "?",
    help_title: "Shortcuts",
    help: &[
        ("⏎", "connect here; you return to the list when the session ends"),
        ("t", "connect in a new Windows Terminal tab"),
        ("f", "open an sftp session"),
        ("c", "copy the ssh command to the clipboard"),
        ("/", "search (name, address, group, notes); ⏎ connects on a single match"),
        ("a  e  y", "add · edit · duplicate"),
        ("d", "delete"),
        ("r", "check servers again"),
        ("i", "import from .bat files and ~/.ssh/config"),
        ("l", "English / Türkçe"),
        ("j k  ↑ ↓", "move   g G  first / last"),
        ("q", "quit"),
    ],
    help_cli: "CLI: sshm <name> connects directly, sshm --list lists connections",
    help_store: "Saved in: {}",
    cli_help: "sshm — SSH connection manager

Usage:
  sshm               open the interface
  sshm <name>        connect directly (exact name, prefix or unique substring)
  sshm --list        list connections
  sshm --import      import from .bat files and ~/.ssh/config
  sshm --lang en|tr  set the interface language
  sshm --config      print the path of the connections file
",
    not_found: "'{}' not found. Connections:",
    cli_import_none: "No new connections found.",
    cli_imported: "Added {} connections: {}",
    welcome: "Welcome. Imported {} connections: {}",
    spawn_failed: "could not start ssh: {}",
    press_enter: "Press Enter to return to the list…",
    ssh_missing: "ssh not found. Is the OpenSSH client installed?",
    session_closed: "{} session closed · {}",
    session_closed_code: "{} session closed · {} · exit code {}",
    connect_failed: "Could not connect to {} (255)",
    ssh_exit: "ssh exited with code {}",
    ssh_killed: "ssh ended unexpectedly",
    lang_set: "Language set to English",
    lang_invalid: "unknown language '{}', use en or tr",
};

pub static TR: Strings = Strings {
    read_failed: "{} okunamadı",
    corrupt: "{} bozuk",
    write_failed: "{} yazılamadı",
    file_header: "# sshm bağlantıları. Elle de düzenlenebilir; sshm açıkken değil.",
    imported_from: "{} dosyasından alındı",
    exe_not_found: "sshm yolu bulunamadı",
    window_failed: "yeni pencere açılamadı",
    tab_unsupported: "yeni sekme yalnız Windows Terminal'de destekleniyor",
    clipboard_open: "pano açılamadı",
    clipboard_write: "panoya yazılamadı",
    dns_failed: "ad çözümlenemedi",
    timed_out: "zaman aşımı",
    refused: "bağlantı reddedildi",
    unreachable: "ulaşılamıyor",
    save_failed: "Kaydedilemedi: {}",
    opened_tab: "{} yeni sekmede açıldı",
    copied: "Komut panoya kopyalandı",
    form_new: "Yeni bağlantı",
    form_edit: "{} düzenleniyor",
    form_copy: "Kopyadan yeni bağlantı",
    copy_suffix: "kopya",
    reprobing: "Sunucular yeniden denetleniyor",
    import_none: "Yeni bağlantı bulunamadı (Belgeler, Masaüstü, ~/.ssh/config)",
    imported_n: "{} bağlantı içe aktarıldı: {}",
    deleted: "{} silindi",
    saved: "{} kaydedildi",
    lang_switched: "Dil: Türkçe",
    f_name: ("Ad", "listede görünen kısa ad"),
    f_host: ("Sunucu", "IP ya da alan adı"),
    f_user: ("Kullanıcı", "ubuntu, root…"),
    f_port: ("Port", "22"),
    f_key: ("Anahtar", r"C:\yol\anahtar.pem (boşsa ssh varsayılanı)"),
    f_group: ("Grup", "Oracle, AWS, Ev…"),
    f_jump: ("Atlama", "kullanıcı@bastion (ProxyJump)"),
    f_tmux: ("tmux", "oturum adı, ör. main (sunucuda tmux kurulu olmalı)"),
    f_fwd: ("Yönlendirme", "L 8080:localhost:80, D 1080"),
    f_extra: ("Ek argüman", "-A -o Compression=yes"),
    f_notes: ("Not", ""),
    e_name_empty: "Ad boş olamaz",
    e_name_chars: "Ad '-' ile başlayamaz, ; \" ' içeremez",
    e_name_taken: "'{}' adı zaten var",
    e_host: "Sunucu boş olamaz, boşluk içeremez",
    e_user: "Kullanıcı adı boşluk içeremez",
    e_port: "Port 1-65535 arası bir sayı olmalı",
    e_key: "Anahtar dosyası bulunamadı: {}",
    e_tmux: "tmux oturum adı yalnız harf, rakam, - _ . içerebilir",
    subtitle: "bağlantılar",
    matches: "{} eşleşme",
    servers: "{} sunucu",
    up_n: "{} açık",
    empty_title: "Henüz bağlantı yok.",
    empty_add: "yeni bağlantı ekle",
    empty_import: "Belgeler / Masaüstü'ndeki .bat dosyalarından al",
    no_match: "Eşleşen bağlantı yok.",
    no_match_hint: "Esc ile filtreyi temizle",
    other_group: "Diğer",
    cell_down: "kapalı",
    cell_jump: "atlama",
    st_unchecked: "denetlenmedi",
    st_checking: "denetleniyor…",
    st_up: "açık",
    st_down: "kapalı",
    st_skipped: "atlama üzerinden, doğrudan denetlenmez",
    r_status: "Durum",
    r_version: "Sürüm",
    r_port: "Port",
    r_key: "Anahtar",
    r_jump: "Atlama",
    r_tmux: "tmux",
    r_fwd: "Yönlendirme",
    r_extra: "Ek argüman",
    r_last: "Son",
    persistent: "{} (kalıcı oturum)",
    never_used: "hiç bağlanılmadı",
    k_connect: "bağlan",
    k_tab: "sekme",
    k_sftp: "sftp",
    k_search: "ara",
    k_add: "ekle",
    k_edit: "düzenle",
    k_copy: "kopyala",
    k_help: "yardım",
    k_quit: "çık",
    k_ok: "tamam",
    k_clear: "temizle",
    k_next_field: "sonraki alan",
    k_save: "kaydet",
    k_cancel: "vazgeç",
    k_delete: "sil",
    k_any: "başka tuş",
    confirm_pre: "",
    confirm_post: " silinsin mi?",
    help_title: "Kısayollar",
    help: &[
        ("⏎", "bu terminalde bağlan; oturum bitince listeye dönülür"),
        ("t", "Windows Terminal'de yeni sekmede bağlan"),
        ("f", "sftp oturumu aç"),
        ("c", "ssh komutunu panoya kopyala"),
        ("/", "ara (ad, adres, grup, not); tek sonuçta ⏎ bağlanır"),
        ("a  e  y", "ekle · düzenle · kopyasını oluştur"),
        ("d", "sil"),
        ("r", "sunucuları yeniden denetle"),
        ("i", ".bat dosyalarından ve ~/.ssh/config'ten içe aktar"),
        ("l", "Türkçe / English"),
        ("j k  ↑ ↓", "gezin   g G  baş / son"),
        ("q", "çık"),
    ],
    help_cli: "Komut satırı: sshm <ad> doğrudan bağlanır, sshm --list listeler",
    help_store: "Kayıtlar: {}",
    cli_help: "sshm — SSH bağlantı yöneticisi

Kullanım:
  sshm               arayüzü aç
  sshm <ad>          doğrudan bağlan (tam ad, önek ya da tek eşleşen parça)
  sshm --list        bağlantıları listele
  sshm --import      .bat dosyalarından ve ~/.ssh/config'ten içe aktar
  sshm --lang en|tr  arayüz dilini ayarla
  sshm --config      kayıt dosyasının yolunu yaz
",
    not_found: "'{}' bulunamadı. Bağlantılar:",
    cli_import_none: "Yeni bağlantı bulunamadı.",
    cli_imported: "{} bağlantı eklendi: {}",
    welcome: "Hoş geldin. {} bağlantı içe aktarıldı: {}",
    spawn_failed: "ssh başlatılamadı: {}",
    press_enter: "Listeye dönmek için Enter'a bas…",
    ssh_missing: "ssh bulunamadı. OpenSSH istemcisi kurulu mu?",
    session_closed: "{} oturumu kapandı · {}",
    session_closed_code: "{} oturumu kapandı · {} · çıkış kodu {}",
    connect_failed: "{} sunucusuna bağlanılamadı (255)",
    ssh_exit: "ssh {} koduyla çıktı",
    ssh_killed: "ssh beklenmedik şekilde sonlandı",
    lang_set: "Dil Türkçe olarak ayarlandı",
    lang_invalid: "bilinmeyen dil '{}', en ya da tr kullan",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill("{} of {}", &[&1, &"x"]), "1 of x");
        assert_eq!(fill("no args", &[]), "no args");
        assert_eq!(fill("{} {}", &[&"a"]), "a ");
    }
}
