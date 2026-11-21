# sshm

Terminalde çalışan SSH bağlantı yöneticisi. Sistemdeki OpenSSH'ı çağırır; anahtarlar,
`known_hosts`, 2FA ve `~/.ssh/config` olduğu gibi çalışır.

## Kurulum

Rust ve OpenSSH istemcisi gerekir (Windows 10/11'de OpenSSH hazır gelir).

```powershell
cargo install --git https://github.com/iphysicc/sshm
```

ya da repoyu klonlayıp içinde `cargo install --path .`

`sshm` artık her terminalde çalışır. İlk açılışta Belgeler ve Masaüstü'ndeki ssh içeren
`.bat`/`.cmd` dosyaları ile `~/.ssh/config` otomatik içe aktarılır.

## Kullanım

```
sshm              arayüz
sshm web          doğrudan bağlan (önek de olur: sshm we)
sshm --list       liste
sshm --import     yeniden içe aktar (aynı uç noktalar atlanır)
sshm --config     kayıt dosyasının yolu
```

| Tuş | İş |
| --- | --- |
| ⏎ | Bu terminalde bağlan; oturum bitince listeye dönülür |
| t | Windows Terminal'de yeni sekmede bağlan |
| f | sftp |
| c | ssh komutunu panoya kopyala |
| / | ara; tek sonuç kalınca ⏎ bağlanır |
| a / e / y / d | ekle / düzenle / kopyala / sil |
| r | sunucuları yeniden denetle |
| i | içe aktar |
| ? | tüm kısayollar |

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

`%APPDATA%\sshm\hosts.toml` (TOML, elle düzenlenebilir). `SSHM_CONFIG` ortam değişkeni
ile başka bir dosya gösterilebilir.

## Lisans

MIT
