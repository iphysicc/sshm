<p align="right"><a href="README.md">English</a> · <b>Türkçe</b></p>

# sshm

Terminalde çalışan SSH bağlantı yöneticisi. Sistemdeki OpenSSH'ı çağırır; anahtarlar,
`known_hosts`, 2FA ve `~/.ssh/config` olduğu gibi çalışır.

## Kurulum

Rust ve OpenSSH istemcisi gerekir (Windows 10/11'de OpenSSH hazır gelir).

```powershell
cargo install --git https://github.com/iphysicc/sshm
```

Ya da repoyu klonlayıp içinde `cargo install --path .` çalıştır.

`sshm` artık her terminalde çalışır. İlk açılışta Belgeler ve Masaüstü'ndeki `ssh` komutu
içeren `.bat`/`.cmd` dosyaları ile `~/.ssh/config` otomatik içe aktarılır.

## Kullanım

```
sshm               arayüzü aç
sshm web           doğrudan bağlan (önek de olur: sshm we)
sshm --list        bağlantıları listele
sshm --import      yeniden içe aktar (aynı uç noktalar atlanır)
sshm --lang en|tr  arayüz dilini ayarla
sshm --config      kayıt dosyasının yolunu yaz
```

| Tuş | İş |
| --- | --- |
| ⏎ | Bu terminalde bağlan; oturum bitince listeye dönülür |
| t | Windows Terminal'de yeni sekmede bağlan |
| f | sftp |
| c | ssh komutunu panoya kopyala |
| / | Ara; tek sonuç kalınca ⏎ bağlanır |
| a / e / y / d | Ekle / düzenle / kopyala / sil |
| r | Sunucuları yeniden denetle |
| i | İçe aktar |
| l | Dil değiştir (English / Türkçe) |
| ? | Tüm kısayollar |

## Dil

Arayüz İngilizce ve Türkçe kullanılabilir, varsayılan İngilizce. Arayüzde `l`'ye bas ya da
`sshm --lang tr` çalıştır; seçim kaydedilir. `SSHM_LANG=tr` yalnız o çalıştırma için geçerlidir.

## Sunucuda kalıcı oturum

Bir bağlantıda **tmux** alanına bir ad yazarsan (ör. `main`) bağlanınca
`tmux new -A -s main` çalışır: bağlantı koparsa ya da kapatırsan oturum sunucuda yaşamaya
devam eder, tekrar bağlanınca kaldığın yere dönersin. Sunucuda `tmux` kurulu olmalı
(`sudo apt install tmux`).

Bağlantılara varsayılan olarak `ServerAliveInterval=30` eklenir; boşta kalan oturumlar
düşmez.

## Durum denetimi

Açılışta ve `r` ile her sunucuya bir TCP bağlantısı açılır, SSH karşılama satırı okunur
(gecikme + sunucu sürümü). Kimlik doğrulama yapılmaz. Bilerek periyodik değildir: sık
tekrarlanan doğrulamasız bağlantılar fail2ban ya da OpenSSH'ın PerSourcePenalties
ayarına takılabilir. Atlama sunucusu (ProxyJump) kullanan kayıtlar denetlenmez.

## Kayıtlar

Windows'ta `%APPDATA%\sshm\hosts.toml` (TOML, sshm kapalıyken elle düzenlenebilir).
`SSHM_CONFIG` ortam değişkeni ile başka bir dosya gösterilebilir.

## Lisans

MIT
